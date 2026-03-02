use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(Debug, Serialize_repr, Deserialize_repr, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Invocation = 1,
    StreamItem = 2,
    Completion = 3,
    StreamInvocation = 4,
    CancelInvocation = 5,
    Ping = 6,
    Close = 7,
    Other = 8,
}

/// Version 0: only `connection_id`, used directly as `?id=` on the transport URL.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiateResponseV0 {
    pub connection_id: String,
    pub negotiate_version: u8,
    pub available_transports: Vec<TransportSpec>,
}

/// Version 1: adds `connection_token` which must be used as `?id=` instead of `connection_id`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiateResponseV1 {
    pub connection_id: String,
    pub connection_token: String,
    pub negotiate_version: u8,
    pub available_transports: Vec<TransportSpec>,
}

/// Parsed negotiate response — version is determined from `negotiateVersion` in the JSON.
#[derive(Debug)]
pub enum NegotiateResponse {
    V0(NegotiateResponseV0),
    V1(NegotiateResponseV1),
}

/// Helper for version detection during deserialization.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NegotiateVersionProbe {
    negotiate_version: u8,
}

impl NegotiateResponse {
    /// Deserialize from JSON, selecting V0 or V1 based on `negotiateVersion`.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let probe: NegotiateVersionProbe = serde_json::from_str(text)?;

        if probe.negotiate_version >= 1 {
            let v1: NegotiateResponseV1 = serde_json::from_str(text)?;
            Ok(NegotiateResponse::V1(v1))
        } else {
            let v0: NegotiateResponseV0 = serde_json::from_str(text)?;
            Ok(NegotiateResponse::V0(v0))
        }
    }

    /// The query string to append to the transport URL (`?id=<token>`).
    /// V0 uses `connection_id`; V1 uses `connection_token`.
    pub fn endpoint_query(&self) -> String {
        match self {
            NegotiateResponse::V0(v) => format!("?id={}", v.connection_id),
            NegotiateResponse::V1(v) => format!("?id={}", v.connection_token),
        }
    }

    pub fn connection_id(&self) -> &str {
        match self {
            NegotiateResponse::V0(v) => &v.connection_id,
            NegotiateResponse::V1(v) => &v.connection_id,
        }
    }

    pub fn available_transports(&self) -> &[TransportSpec] {
        match self {
            NegotiateResponse::V0(v) => &v.available_transports,
            NegotiateResponse::V1(v) => &v.available_transports,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportSpec {
    pub transport: String,
    pub transfer_formats: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by the client to agree on the message format.
pub struct HandshakeRequest {
    protocol: String,
    version: u8,
}

impl HandshakeRequest {
    pub fn new(protocol: impl ToString) -> Self {
        HandshakeRequest {
            protocol: protocol.to_string(),
            version: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by the server as an acknowledgment of the previous `HandshakeRequest` message. Contains an error if the handshake failed.
pub struct HandshakeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by either party to check if the connection is active.
pub struct Ping {
    r#type: MessageType,
}

impl Ping {
    pub fn new() -> Self {
        Ping {
            r#type: MessageType::Ping,
        }
    }

    pub fn message_type(&self) -> MessageType {
        self.r#type
    }
}

impl Default for Ping {
    fn default() -> Self {
        Ping::new()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by the server when a connection is closed. Contains an error if the connection was closed because of an error.
pub struct Close {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow_reconnect: Option<bool>,
}