use crate::{client::SignalRClient, protocol::{invoke::Invocation, negotiate::MessageType}, InvocationContext};
use crate::protocol::messages::MessageParser;
use super::actions::UpdatableAction;

pub(crate) struct CallbackAction {
    #[allow(dead_code)]
    target: String,
    callback: Box<dyn Fn(InvocationContext) + 'static>,
    client: SignalRClient,
}

impl CallbackAction {
    pub(crate) fn create(target: String, callback: impl Fn(InvocationContext) + 'static, client: SignalRClient) -> CallbackAction {
        CallbackAction {
            target: target,
            callback: Box::new(callback),
            client: client
        }
    }
}

impl UpdatableAction for CallbackAction {
    fn update_with(&mut self, message: &str, message_type: MessageType) {
        match message_type {
            MessageType::Invocation => {
                let invocation: Invocation = MessageParser::parse_message(message).unwrap();
                let context = InvocationContext::create(self.client.clone(), invocation);

                (self.callback)(context);
            },
            _ => panic!("Callbacks accept only invocation data"),
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
