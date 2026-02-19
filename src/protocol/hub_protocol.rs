/// Represents the raw bytes of a serialized SignalR message,
/// whether it originated from JSON text or MessagePack binary.
#[derive(Debug, Clone)]
pub enum MessagePayload {
    /// JSON text payload
    Text(String),
    /// MessagePack binary payload
    #[cfg(feature = "messagepack")]
    Binary(Vec<u8>),
}

/// Protocol selection for the SignalR hub connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubProtocolKind {
    Json,
    #[cfg(feature = "messagepack")]
    MessagePack,
}

impl HubProtocolKind {
    /// Returns the protocol name string used in the handshake.
    pub fn protocol_name(&self) -> &'static str {
        match self {
            HubProtocolKind::Json => "json",
            #[cfg(feature = "messagepack")]
            HubProtocolKind::MessagePack => "messagepack",
        }
    }

    /// Returns the transfer format expected in negotiation.
    pub fn transfer_format(&self) -> &'static str {
        match self {
            HubProtocolKind::Json => "Text",
            #[cfg(feature = "messagepack")]
            HubProtocolKind::MessagePack => "Binary",
        }
    }
}

impl Default for HubProtocolKind {
    fn default() -> Self {
        HubProtocolKind::Json
    }
}
