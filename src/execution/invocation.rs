use crate::{completer::{ManualFuture, ManualFutureCompleter}, protocol::{hub_protocol::MessagePayload, invoke::Completion, negotiate::MessageType}};
use log::{error, info};
use serde::de::DeserializeOwned;

use crate::protocol::messages::MessageParser;

use super::actions::UpdatableAction;

pub(crate) struct InvocationAction<R: DeserializeOwned + Unpin> {
    invocation_id: String,
    completer: Option<ManualFutureCompleter<Result<R, String>>>
}

impl<R: DeserializeOwned + Unpin> InvocationAction<R> {
    pub fn new(invocation_id: String) -> (Self, ManualFuture<Result<R, String>>) {
        let (f, c) = ManualFuture::new();
        let invocation = InvocationAction {
            invocation_id: invocation_id,
            completer: Some(c)
        };

        (invocation, f)
    }

    #[allow(dead_code)]
    pub fn completable(&self) -> bool {
        self.completer.is_some()
    }

    pub fn complete_ok(&mut self, result: R) {
        info!("Completing invocation {} with result", self.invocation_id);
        let completer = self.completer.take().unwrap();
        completer.complete(Ok(result));
    }

    pub fn complete_err(&mut self, error: String) {
        error!("Completing invocation {} with error: {}", self.invocation_id, error);
        let completer = self.completer.take().unwrap();
        completer.complete(Err(error));
    }

    fn dispose_internal(&mut self) {
        let c = self.completer.take();

        if c.is_some() {
            c.unwrap().cancel();
        }
    }
}

impl<R: DeserializeOwned + Unpin> Drop for InvocationAction<R> {
    fn drop(&mut self) {
        self.dispose_internal();
    }
}

impl<R: DeserializeOwned + Unpin + crate::platform::MaybeSend> UpdatableAction for InvocationAction<R> {
    fn update_with(&mut self, message: &MessagePayload, message_type: MessageType) {
        match message_type {
            MessageType::Completion => {
                match message {
                    MessagePayload::Text(s) => {
                        if let Ok(completition) = MessageParser::parse_message::<Completion<R>>(s) {
                            if completition.is_result() {
                                info!("Completition is parsed");
                                self.complete_ok(completition.unwrap_result());
                            } else {
                                self.complete_err(completition.unwrap_error());
                            }
                        } else {
                            self.complete_err(format!("Cannot parse completion: {}", s));
                        }
                    },
                    #[cfg(feature = "messagepack")]
                    MessagePayload::Binary(data) => {
                        match crate::protocol::msgpack::parse_msgpack_message(data) {
                            Ok(items) => {
                                match crate::protocol::msgpack::parse_completion(&items) {
                                    Ok(comp) => {
                                        match comp.result_kind {
                                            1 => {
                                                let err = comp.payload
                                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                                                    .unwrap_or_else(|| "Unknown error".to_string());
                                                self.complete_err(err);
                                            },
                                            2 => {
                                                // Void completion - no result to deliver
                                            },
                                            3 => {
                                                if let Some(val) = comp.payload {
                                                    match crate::protocol::msgpack::value_to_type::<R>(&val) {
                                                        Ok(result) => {
                                                            info!("Completition is parsed");
                                                            self.complete_ok(result);
                                                        },
                                                        Err(e) => self.complete_err(format!("Cannot deserialize completion result: {}", e)),
                                                    }
                                                } else {
                                                    self.complete_err("Completion has no result payload".to_string());
                                                }
                                            },
                                            _ => self.complete_err(format!("Unknown ResultKind: {}", comp.result_kind)),
                                        }
                                    },
                                    Err(e) => self.complete_err(format!("Cannot parse msgpack completion: {}", e)),
                                }
                            },
                            Err(e) => self.complete_err(format!("Cannot parse msgpack message: {}", e)),
                        }
                    },
                }
            },
            _ => error!("Cannot complete invocation {} with message type {:?}", self.invocation_id, message_type),
        }
    }
    
    fn is_completed(&self) -> bool {
        self.completer.is_none()
    }

    fn dispose(mut self) {
        self.dispose_internal();
    }
}