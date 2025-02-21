use std::{collections::HashMap, fmt::Debug};
use serde::{Deserialize, Serialize};
use super::{messages::MessageParser, negotiate::MessageType};

/// Indicates a request to invoke a particular method (the Target) with provided Arguments on the remote endpoint.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Invocation {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invocation_id: Option<String>,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec::<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_ids: Option<Vec<String>>,
}

impl Invocation {
    pub fn create_single(target: impl Into<String>) -> Self {
        Invocation {
            r#type: MessageType::Invocation,
            headers: None,
            invocation_id: None,
            target: target.into(),
            arguments: Some(Vec::new()),
            stream_ids: None,
        }
    }

    pub fn create_multiple(target: impl Into<String>) -> Self {
        Invocation {
            r#type: MessageType::StreamInvocation,
            headers: None,
            invocation_id: None,
            target: target.into(),
            arguments: Some(Vec::new()),
            stream_ids: None,
        }
    }

    pub fn with_argument<T: Serialize>(&mut self, data: T) -> Result<(), String> {
        let rson = MessageParser::to_json_value(&data);

        if rson.is_ok() {
            let json = rson.unwrap();
            let vec: Vec<serde_json::Value>;
        
            if let Some(ref mut vec) = self.arguments {
                vec.push(json);
            } else {
                vec = vec![json];
                self.arguments = Some(vec);
            }

            Ok(())
        } else {
            Err(format!("Serialization error: {}", rson.unwrap_err().to_string()))
        }
    }

    pub fn with_invocation_id(&mut self, invocation_id: impl ToString) -> &mut Self {
        self.invocation_id = Some(invocation_id.to_string());
        self
    }

    #[allow(dead_code)]
    pub fn with_streams(&mut self, stream_ids: Vec<String>) -> &mut Self {
        if !stream_ids.is_empty() {
            self.stream_ids = Some(stream_ids);
        }
        self
    }

    pub(crate) fn get_invocation_id(&self) -> Option<String> {
        if self.invocation_id.is_some() {
            Some(self.invocation_id.as_ref().unwrap().to_string())
        } else {
            None
        }
    }

    pub(crate) fn get_target(&self) -> String {
        self.target.clone()
    }
}

/// Indicates a previous Invocation or StreamInvocation has completed.
/// Contains an error if the invocation concluded with an error or the result of a non-streaming method invocation.
/// The result will be absent for void methods.
/// In case of streaming invocations no further StreamItem messages will be received.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Completion<R> {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    invocation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<R>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<R> Completion<R> {
    pub fn create_result(invocation_id: String, data: R) -> Self {
        Completion {
            r#type: MessageType::Completion,
            invocation_id: invocation_id,
            result: Some(data),
            error: None,
            headers: None,            
        }
    }

    #[allow(dead_code)]
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn is_result(&self) -> bool {
        self.result.is_some()
    }

    pub fn unwrap_error(self) -> String {
        self.error.unwrap()
    }

    pub fn unwrap_result(self) -> R {
        self.result.unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by the client to cancel a streaming invocation on the server.
pub struct CancelInvocation {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    pub invocation_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Sent by the client to cancel a streaming invocation on the server.
pub struct PossibleInvocation {
    r#type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    pub target: Option<String>,
}

