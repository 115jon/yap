//! The `{"op": <int>, "d": <payload>}` envelope both gateways speak.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::opcode::GatewayOp;

/// A decoded text frame. `op` stays a plain number so a frame with an opcode
/// this build does not know still decodes (clients ignore such frames); use
/// [`Frame::gateway_op`] to classify it. `d` is `null` when absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub op: i64,
    #[serde(default)]
    pub d: Value,
}

impl Frame {
    /// A frame for `op` carrying `d`.
    pub fn new(op: GatewayOp, d: Value) -> Self {
        Self {
            op: i64::from(op.code()),
            d,
        }
    }

    /// A frame for `op` carrying a typed payload.
    pub fn with_payload<T: Serialize>(
        op: GatewayOp,
        payload: &T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self::new(op, serde_json::to_value(payload)?))
    }

    /// Decodes one text frame. Fails when it is not JSON or has no integer `op`.
    pub fn decode(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// The opcode, when this build knows it.
    pub fn gateway_op(&self) -> Option<GatewayOp> {
        GatewayOp::try_from(self.op).ok()
    }

    /// Decodes `d` into a payload type. Unknown fields are ignored.
    pub fn payload<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(&self.d)
    }

    /// The frame as the JSON text to send (`op` first, then `d`).
    pub fn encode(&self) -> String {
        // A `Value` payload and an integer cannot fail to serialise.
        serde_json::to_string(self).unwrap_or_default()
    }
}
