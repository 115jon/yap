//! Cloudflare Realtime SFU through Yap's PartyTracks proxy
//! (`/api/channels/<roomSlug>/partytracks/...`). See VOICE_PROTOCOL.md §3.
//!
//! The proxy swaps raw SFU session ids for signed capabilities: always send
//! back exactly the `sessionId` values it returned.

use std::future::Future;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, bail, Result};
use reqwest::{Client, Method};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct Sfu {
    http: Client,
    prefix: String,
    participant_id: String,
    /// Shared by clones, so a refresh reaches every holder.
    voice_token: Arc<RwLock<String>>,
}

/// The SFU's answer to a push.
pub struct Pushed {
    pub answer_sdp: String,
    pub track_name: String,
}

/// A simulcast layer switch for one pulled camera (`tracks/update`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerUpdate {
    pub mid: String,
    pub sfu_track_name: String,
    /// The publisher's session capability.
    pub session_id: String,
    pub rid: String,
    /// The first update of a switch; the settle update 2 s later is not.
    pub switch_now: bool,
}

/// The SFU's offer for a pull (the client answers through [`Sfu::renegotiate`]).
pub struct Pulled {
    pub mid: String,
    pub offer_sdp: Option<String>,
}

impl Sfu {
    pub fn new(
        http: Client,
        api_base: &str,
        room_slug: &str,
        participant_id: &str,
        voice_token: &str,
    ) -> Self {
        Self {
            http,
            prefix: format!("{api_base}/api/channels/{room_slug}/partytracks"),
            participant_id: participant_id.to_owned(),
            voice_token: Arc::new(RwLock::new(voice_token.to_owned())),
        }
    }

    pub fn set_voice_token(&self, token: &str) {
        if let Ok(mut current) = self.voice_token.write() {
            *current = token.to_owned();
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.prefix))
            .bearer_auth(
                self.voice_token
                    .read()
                    .map(|t| t.clone())
                    .unwrap_or_default(),
            )
            .header("X-Voice-Session-Id", &self.participant_id);
        if let Some(body) = body {
            request = request.json(&body);
        }
        // Without the URL: it holds our signed session capability.
        let response = request.send().await.map_err(reqwest::Error::without_url)?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        // Name the operation, not the path: the path holds our signed
        // session capability, which does not belong in the UI or logs.
        let op = path.rsplit('/').next().unwrap_or("call");
        if !status.is_success() {
            let error = body.get("error").and_then(Value::as_str).unwrap_or("");
            bail!("SFU {op}: HTTP {} {error}", status.as_u16());
        }
        if let Some(code) = body.get("errorCode").and_then(Value::as_str) {
            let detail = body
                .get("errorDescription")
                .and_then(Value::as_str)
                .unwrap_or("");
            bail!("SFU {op}: {code} {detail}");
        }
        Ok(body)
    }

    /// `POST sessions/new` → this participant's session capability.
    pub async fn new_session(&self) -> Result<String> {
        let body = self.call(Method::POST, "/sessions/new", None).await?;
        body.get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("sessions/new without sessionId"))
    }

    /// Pushes one local track: our offer in, the SFU's answer out.
    pub async fn push(
        &self,
        session: &str,
        offer_sdp: &str,
        mid: &str,
        track_name: &str,
    ) -> Result<Pushed> {
        let body = self
            .call(
                Method::POST,
                &format!("/sessions/{session}/tracks/new"),
                Some(json!({
                    "sessionDescription": { "sdp": offer_sdp, "type": "offer" },
                    "tracks": [{ "trackName": track_name, "mid": mid, "location": "local" }],
                })),
            )
            .await?;
        let answer_sdp = body
            .pointer("/sessionDescription/sdp")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("push without an answer"))?
            .to_owned();
        Ok(Pushed {
            answer_sdp,
            track_name: track_name.to_owned(),
        })
    }

    /// Asks to pull another participant's track into our session; a
    /// simulcast camera at `preferred_rid` (the proxy adds the downward
    /// fallback, `realtime/partytracks-proxy.ts`).
    pub async fn pull(
        &self,
        session: &str,
        track_name: &str,
        remote_session: &str,
        preferred_rid: Option<&str>,
    ) -> Result<Pulled> {
        let mut track =
            json!({ "location": "remote", "trackName": track_name, "sessionId": remote_session });
        if let Some(rid) = preferred_rid {
            track["simulcast"] = json!({ "preferredRid": rid });
        }
        let body = self
            .call(
                Method::POST,
                &format!("/sessions/{session}/tracks/new"),
                Some(json!({ "tracks": [track] })),
            )
            .await?;
        let track = body
            .pointer("/tracks/0")
            .cloned()
            .ok_or_else(|| anyhow!("pull without tracks"))?;
        if let Some(code) = track.get("errorCode").and_then(Value::as_str) {
            bail!("pull {track_name}: {code}");
        }
        let mid = track
            .get("mid")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("pull without mid"))?
            .to_owned();
        let offer_sdp = if body
            .get("requiresImmediateRenegotiation")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            body.pointer("/sessionDescription/sdp")
                .and_then(Value::as_str)
                .map(str::to_owned)
        } else {
            None
        };
        Ok(Pulled { mid, offer_sdp })
    }

    /// Closes receive tracks by mid: our offer in, the SFU's answer out.
    pub async fn close(&self, session: &str, mids: &[String], offer_sdp: &str) -> Result<String> {
        let tracks: Vec<Value> = mids.iter().map(|mid| json!({ "mid": mid })).collect();
        let body = self
            .call(
                Method::PUT,
                &format!("/sessions/{session}/tracks/close"),
                Some(json!({
                    "tracks": tracks,
                    "sessionDescription": { "sdp": offer_sdp, "type": "offer" },
                    "force": false,
                })),
            )
            .await?;
        body.pointer("/sessionDescription/sdp")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("close without an answer"))
    }

    /// Moves a simulcast pull to another layer in place (no renegotiation).
    /// `switch_now` asks the SFU to switch at once (`priorityOrdering:
    /// "none"`); the follow-up without it restores the proxy's
    /// bandwidth-aware policy, as the patched PartyTracks does.
    pub async fn update_layer(&self, session: &str, layer: &LayerUpdate) -> Result<()> {
        let mut simulcast = json!({ "preferredRid": layer.rid });
        if layer.switch_now {
            simulcast["priorityOrdering"] = json!("none");
        }
        self.call(
            Method::PUT,
            &format!("/sessions/{session}/tracks/update"),
            Some(json!({ "tracks": [{
                "location": "remote", "trackName": layer.sfu_track_name,
                "sessionId": layer.session_id, "mid": layer.mid, "simulcast": simulcast,
            }]})),
        )
        .await
        .map(|_| ())
    }

    /// Sends our answer to an SFU offer.
    pub async fn renegotiate(&self, session: &str, answer_sdp: &str) -> Result<()> {
        self.call(
            Method::PUT,
            &format!("/sessions/{session}/renegotiate"),
            Some(json!({ "sessionDescription": { "sdp": answer_sdp, "type": "answer" } })),
        )
        .await
        .map(|_| ())
    }
}

/// The SFU calls the pull worker makes; [`Sfu`] in production.
pub(crate) trait Signaling: Clone + Send + Sync + 'static {
    fn pull(
        &self,
        session: &str,
        track_name: &str,
        remote_session: &str,
        preferred_rid: Option<&str>,
    ) -> impl Future<Output = Result<Pulled>> + Send;

    fn update_layer(
        &self,
        session: &str,
        layer: &LayerUpdate,
    ) -> impl Future<Output = Result<()>> + Send;

    fn renegotiate(
        &self,
        session: &str,
        answer_sdp: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn close(
        &self,
        session: &str,
        mids: &[String],
        offer_sdp: &str,
    ) -> impl Future<Output = Result<String>> + Send;
}

impl Signaling for Sfu {
    fn pull(
        &self,
        session: &str,
        track_name: &str,
        remote_session: &str,
        preferred_rid: Option<&str>,
    ) -> impl Future<Output = Result<Pulled>> + Send {
        Sfu::pull(self, session, track_name, remote_session, preferred_rid)
    }

    fn update_layer(
        &self,
        session: &str,
        layer: &LayerUpdate,
    ) -> impl Future<Output = Result<()>> + Send {
        Sfu::update_layer(self, session, layer)
    }

    fn renegotiate(
        &self,
        session: &str,
        answer_sdp: &str,
    ) -> impl Future<Output = Result<()>> + Send {
        Sfu::renegotiate(self, session, answer_sdp)
    }

    fn close(
        &self,
        session: &str,
        mids: &[String],
        offer_sdp: &str,
    ) -> impl Future<Output = Result<String>> + Send {
        Sfu::close(self, session, mids, offer_sdp)
    }
}
