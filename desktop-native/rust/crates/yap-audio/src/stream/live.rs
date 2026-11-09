//! A live, endless HTTP stream (a radio station's `streamUrl`) as a
//! non-seekable media source.
//!
//! The web plays a radio entry with a plain audio element on the station's
//! own URL: no Range, no session, no seeking. Here one GET is read chunk by
//! chunk on the engine's network runtime and handed to the decode thread
//! through a small bounded channel, so a paused mixer (a full queue) holds
//! the connection back instead of buffering without end.

use std::io::{self, Read, Seek, SeekFrom};
use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use symphonia::core::io::MediaSource;
use tokio::sync::mpsc;

use super::recovery::parse_retry_after;
use super::source::{Cancel, FetchError, Net};

/// Chunks in flight between the socket and the decoder.
const CHANNEL_CHUNKS: usize = 16;
/// The response must start within this long.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// A station that sends nothing for this long counts as a network failure.
const STALL_TIMEOUT: Duration = Duration::from_secs(20);

type Chunk = Result<Vec<u8>, FetchError>;

pub(crate) struct LiveSource {
    chunks: mpsc::Receiver<Chunk>,
    buffer: Vec<u8>,
    at: usize,
    /// The reader task; aborted when the source drops.
    task: Option<tokio::task::JoinHandle<()>>,
    /// The response's `Content-Type`, a decoder hint.
    pub content_type: Option<String>,
}

impl LiveSource {
    /// Opens `url` (no credentials, no Range) and starts reading it.
    pub(crate) fn open(net: &Net, url: &str, cancel: Cancel) -> Result<Self, FetchError> {
        let url = reqwest::Url::parse(url).map_err(|e| FetchError::Network(e.to_string()))?;
        // Its own client: a station is a stranger's host, so a redirect
        // must stay on public https and a name must resolve to a public
        // address (the media relay listens on 127.0.0.1).
        let client = {
            let _entered = net.handle.enter();
            crate::guard::public_client_builder()
                .build()
                .map_err(|e| FetchError::Network(e.without_url().to_string()))?
        };
        let request = async move {
            let network =
                |error: reqwest::Error| FetchError::Network(error.without_url().to_string());
            let response = tokio::time::timeout(CONNECT_TIMEOUT, client.get(url).send())
                .await
                .map_err(|_| FetchError::Network("timed out".into()))?
                .map_err(network)?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(FetchError::Status {
                    status,
                    retry_after_secs: response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(parse_retry_after),
                });
            }
            Ok(response)
        };
        let opened = {
            let cancel = cancel.clone();
            net.handle.block_on(async move {
                tokio::select! {
                    result = request => result,
                    () = cancel.cancelled() => Err(FetchError::Cancelled),
                }
            })
        };
        let mut response = opened?;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let (tx, rx) = mpsc::channel::<Chunk>(CHANNEL_CHUNKS);
        let task = net.handle.spawn(async move {
            loop {
                let next = tokio::select! {
                    next = tokio::time::timeout(STALL_TIMEOUT, response.chunk()) => next,
                    () = cancel.cancelled() => return,
                };
                let item = match next {
                    Err(_) => Err(FetchError::Network("the station stopped sending".into())),
                    Ok(Err(error)) => Err(FetchError::Network(error.without_url().to_string())),
                    Ok(Ok(None)) => return,
                    Ok(Ok(Some(bytes))) => Ok(bytes.to_vec()),
                };
                let failed = item.is_err();
                if tx.send(item).await.is_err() || failed {
                    return;
                }
            }
        });
        let mut source = Self::from_channel(rx, content_type);
        source.task = Some(task);
        Ok(source)
    }

    /// A source over chunks from elsewhere (tests).
    pub(crate) fn from_channel(
        chunks: mpsc::Receiver<Chunk>,
        content_type: Option<String>,
    ) -> Self {
        Self {
            chunks,
            buffer: Vec::new(),
            at: 0,
            task: None,
            content_type,
        }
    }

    /// The decoder hint for the response: `audio/aac` here is ADTS (a raw
    /// AAC stream), not an mp4 file.
    pub(crate) fn hint(&self) -> Option<String> {
        let mime = self
            .content_type
            .as_deref()?
            .split(';')
            .next()?
            .trim()
            .to_ascii_lowercase();
        Some(match mime.as_str() {
            "audio/aac" | "audio/aacp" | "audio/x-aac" => "aac".into(),
            _ => mime,
        })
    }
}

impl Drop for LiveSource {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Read for LiveSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.at >= self.buffer.len() {
            match self.chunks.blocking_recv() {
                Some(Ok(chunk)) => {
                    self.buffer = chunk;
                    self.at = 0;
                }
                Some(Err(error)) => return Err(io::Error::other(error)),
                // The station closed the stream (or it was stopped).
                None => return Ok(0),
            }
        }
        let take = (self.buffer.len() - self.at).min(buf.len());
        buf[..take].copy_from_slice(&self.buffer[self.at..self.at + take]);
        self.at += take;
        Ok(take)
    }
}

impl Seek for LiveSource {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a live stream cannot seek",
        ))
    }
}

impl MediaSource for LiveSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_should_read_in_order_then_end() {
        let (tx, rx) = mpsc::channel(4);
        tx.try_send(Ok(vec![1, 2, 3])).expect("send");
        tx.try_send(Ok(vec![4])).expect("send");
        drop(tx);
        let mut source = LiveSource::from_channel(rx, None);
        let mut out = Vec::new();
        source.read_to_end(&mut out).expect("read");
        assert_eq!(out, vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_failed_chunk_should_surface_as_a_fetch_error() {
        let (tx, rx) = mpsc::channel(4);
        tx.try_send(Ok(vec![9])).expect("send");
        tx.try_send(Err(FetchError::Network("gone".into())))
            .expect("send");
        let mut source = LiveSource::from_channel(rx, None);
        let mut buf = [0u8; 8];
        assert_eq!(source.read(&mut buf).expect("first"), 1);
        let error = source.read(&mut buf).expect_err("second");
        assert!(super::super::source::fetch_error(&error).is_some());
    }

    #[test]
    fn it_should_never_seek() {
        let (_tx, rx) = mpsc::channel(1);
        let mut source = LiveSource::from_channel(rx, None);
        assert!(!source.is_seekable() && source.byte_len().is_none());
        assert!(source.seek(SeekFrom::Start(0)).is_err());
    }

    #[test]
    fn aac_should_hint_adts() {
        let (_tx, rx) = mpsc::channel(1);
        let source = LiveSource::from_channel(rx, Some("audio/aacp; charset=x".into()));
        assert_eq!(source.hint().as_deref(), Some("aac"));
        let (_tx, rx) = mpsc::channel(1);
        let source = LiveSource::from_channel(rx, Some("audio/mpeg".into()));
        assert_eq!(source.hint().as_deref(), Some("audio/mpeg"));
    }
}
