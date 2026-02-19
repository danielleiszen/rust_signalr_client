use log::error;
use serde::de::DeserializeOwned;

use crate::{completer::{ManualStream, ManualStreamCompleter}, protocol::{hub_protocol::MessagePayload, messages::MessageParser, invoke::Completion, negotiate::MessageType, streaming::StreamItem}};

use super::actions::UpdatableAction;

pub(crate) struct EnumerableAction<R: DeserializeOwned + Unpin> {
    invocation_id: String,
    completer: ManualStreamCompleter<R>,
    completed: bool,
}

impl<R: DeserializeOwned + Unpin> EnumerableAction<R> {
    pub fn new(invocation_id: String) -> (Self, ManualStream<R>) {
        let (s, c) = ManualStream::create();

        (EnumerableAction {
            invocation_id: invocation_id,
            completer: c,
            completed: false
        }, s)
    }

    fn dispose_internal(&mut self) {
        self.completed = true;
        self.completer.close();
    }
}

impl<R: DeserializeOwned + Unpin> Drop for EnumerableAction<R> {
    fn drop(&mut self) {
        self.dispose_internal();
    }
}

impl<R: DeserializeOwned + Unpin + Send> UpdatableAction for EnumerableAction<R> {
    fn update_with(&mut self, message: &MessagePayload, message_type: MessageType) {
        match message_type {
            MessageType::StreamItem => {
                match message {
                    MessagePayload::Text(s) => {
                        if let Ok(item) = MessageParser::parse_message::<StreamItem<R>>(s) {
                            self.completer.push(item.item);
                        } else {
                            error!("Cannot update stream {} with unparseable item {}", self.invocation_id, s);
                        }
                    },
                    #[cfg(feature = "messagepack")]
                    MessagePayload::Binary(data) => {
                        match crate::protocol::msgpack::parse_msgpack_message(data) {
                            Ok(items) => {
                                match crate::protocol::msgpack::parse_stream_item(&items) {
                                    Ok(si) => {
                                        match crate::protocol::msgpack::value_to_type::<R>(&si.item) {
                                            Ok(item) => self.completer.push(item),
                                            Err(e) => error!("Cannot deserialize stream item: {}", e),
                                        }
                                    },
                                    Err(e) => error!("Cannot parse msgpack stream item: {}", e),
                                }
                            },
                            Err(e) => error!("Cannot parse msgpack message: {}", e),
                        }
                    },
                }
            },
            MessageType::Completion => {
                match message {
                    MessagePayload::Text(s) => {
                        if let Ok(_) = MessageParser::parse_message::<Completion<R>>(s) {
                            self.completer.close();
                        } else {
                            error!("Cannot parse completition: {}", s);
                        }
                    },
                    #[cfg(feature = "messagepack")]
                    MessagePayload::Binary(_) => {
                        // MessagePack completion for streams just means "stream ended"
                        self.completer.close();
                    },
                }
            },
            _ => error!("Cannot update stream {} with message type {:?}", self.invocation_id, message_type),
        }
    }

    fn is_completed(&self) -> bool {
        self.completed
    }

    fn dispose(mut self) {
        self.dispose_internal();
    }
}