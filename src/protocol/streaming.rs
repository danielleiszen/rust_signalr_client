use std::{collections::HashMap, fmt::Debug};
use serde::{Deserialize, Serialize};
use super::negotiate::MessageType;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// Indicates individual items of streamed response data from a previous `StreamInvocation` message.
pub struct StreamItem<I> {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    pub(crate) invocation_id: String,
    pub(crate) item: I,
}

// WILL BE USED WHEN STREAM IS UPLOADING
// NOT SUPPORTED YET
// impl<I> StreamItem<I> {
//     pub fn new(invocation_id: impl Into<String>, item: I) -> Self {
//         StreamItem {
//             r#type: MessageType::StreamItem,
//             headers: None,
//             invocation_id: invocation_id.into(),
//             item,
//         }
//     }
// }