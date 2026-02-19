use crate::{client::SignalRClient, protocol::{hub_protocol::MessagePayload, invoke::Invocation, negotiate::MessageType}, InvocationContext};
use crate::protocol::messages::MessageParser;
use super::actions::UpdatableAction;

pub(crate) struct CallbackAction {
    #[allow(dead_code)]
    target: String,
    callback: Box<dyn Fn(InvocationContext) + Send + 'static>,
    client: SignalRClient,
}

impl CallbackAction {
    pub(crate) fn create(target: String, callback: impl Fn(InvocationContext) + Send + 'static, client: SignalRClient) -> CallbackAction {
        CallbackAction {
            target: target,
            callback: Box::new(callback),
            client: client
        }
    }
}

impl UpdatableAction for CallbackAction {
    fn update_with(&mut self, message: &MessagePayload, message_type: MessageType) {
        match message_type {
            MessageType::Invocation => {
                match message {
                    MessagePayload::Text(s) => {
                        let invocation: Invocation = MessageParser::parse_message(s).unwrap();
                        let context = InvocationContext::create(self.client.clone(), invocation);
                        (self.callback)(context);
                    },
                    #[cfg(feature = "messagepack")]
                    MessagePayload::Binary(data) => {
                        match crate::protocol::msgpack::parse_msgpack_message(data) {
                            Ok(items) => {
                                match crate::protocol::msgpack::parse_invocation(&items) {
                                    Ok(parsed) => {
                                        // Convert rmpv arguments to serde_json::Value for InvocationContext compatibility
                                        let json_args: Vec<serde_json::Value> = parsed.arguments.iter()
                                            .map(crate::protocol::msgpack::msgpack_value_to_json)
                                            .collect();

                                        let mut invocation = Invocation::create_single(parsed.target);
                                        invocation.arguments = Some(json_args);
                                        if let Some(id) = parsed.invocation_id {
                                            invocation.with_invocation_id(id);
                                        }

                                        let context = InvocationContext::create(self.client.clone(), invocation);
                                        (self.callback)(context);
                                    },
                                    Err(e) => log::error!("Cannot parse msgpack invocation: {}", e),
                                }
                            },
                            Err(e) => log::error!("Cannot parse msgpack message: {}", e),
                        }
                    },
                }
            },
            _ => log::error!("Callbacks accept only invocation data, got {:?}", message_type),
        }
    }

    fn is_completed(&self) -> bool {
        false
    }

    fn dispose(self) {
        drop(self.callback);
        drop(self.client);
        drop(self.target);
    }
}
