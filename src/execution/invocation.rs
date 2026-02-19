use crate::{completer::{ManualFuture, ManualFutureCompleter}, protocol::{hub_protocol::MessagePayload, invoke::Completion, negotiate::MessageType}};
use log::{error, info};
use serde::de::DeserializeOwned;

use crate::protocol::messages::MessageParser;

use super::actions::UpdatableAction;

pub(crate) struct InvocationAction<R: DeserializeOwned + Unpin> {
    invocation_id: String,
    completer: Option<ManualFutureCompleter<R>>
}

impl<R: DeserializeOwned + Unpin> InvocationAction<R> {
    pub fn new(invocation_id: String) -> (Self, ManualFuture<R>) {
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

    pub fn complete(&mut self, result: R) {
        info!("Trying to get future completer form Invocation Action");
        let completer = self.completer.take().unwrap();
        info!("Future completer is taken");
        completer.complete(result);
        info!("Future completer is completed");
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

impl<R: DeserializeOwned + Unpin + Send> UpdatableAction for InvocationAction<R> {
    fn update_with(&mut self, message: &MessagePayload, message_type: MessageType) {
        match message_type {
            MessageType::Completion => {
                match message {
                    MessagePayload::Text(s) => {
                        if let Ok(completition) = MessageParser::parse_message::<Completion<R>>(s) {
                            if completition.is_result() {
                                info!("Completition is parsed");
                                self.complete(completition.unwrap_result());
                            } else {
                                error!("Cannot complete invocation {}, error: {}", self.invocation_id, completition.unwrap_error());
                            }
                        } else {
                            error!("Cannot parse completition: {}", s);
                        }
                    },
                    #[cfg(feature = "messagepack")]
                    MessagePayload::Binary(data) => {
                        match crate::protocol::msgpack::parse_msgpack_message(data) {
                            Ok(items) => {
                                log::debug!("MsgPack completion for {}: raw items={:?}", self.invocation_id, items);
                                match crate::protocol::msgpack::parse_completion(&items) {
                                    Ok(comp) => {
                                        log::debug!("MsgPack completion result_kind={}, payload={:?}", comp.result_kind, comp.payload);
                                        match comp.result_kind {
                                            1 => {
                                                let err = comp.payload
                                                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                                                    .unwrap_or_else(|| "Unknown error".to_string());
                                                error!("Cannot complete invocation {}, error: {}", self.invocation_id, err);
                                            },
                                            2 => {
                                                // Void completion - no result to deliver
                                            },
                                            3 => {
                                                if let Some(val) = comp.payload {
                                                    match crate::protocol::msgpack::value_to_type::<R>(&val) {
                                                        Ok(result) => {
                                                            info!("Completition is parsed");
                                                            self.complete(result);
                                                        },
                                                        Err(e) => error!("Cannot deserialize completion result: {}", e),
                                                    }
                                                }
                                            },
                                            _ => error!("Unknown ResultKind: {}", comp.result_kind),
                                        }
                                    },
                                    Err(e) => error!("Cannot parse msgpack completion: {}", e),
                                }
                            },
                            Err(e) => error!("Cannot parse msgpack message: {}", e),
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