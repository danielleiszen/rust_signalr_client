use std::fmt::Debug;
use serde::{Deserialize, Serialize};
use super::negotiate::MessageType;


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

// NEVER SENT

// impl Close {
//     pub fn new() -> Self {
//         Close {
//             r#type: MessageType::Close,
//             allow_reconnect: Some(false),
//             error: None
//         }
//     }
// }