use log::error;
use serde::de::DeserializeOwned;

use crate::{completer::{ManualStream, ManualStreamCompleter}, protocol::{messages::MessageParser, invoke::Completion, negotiate::MessageType, streaming::StreamItem}};

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

impl<R: DeserializeOwned + Unpin> UpdatableAction for EnumerableAction<R> {
    fn update_with(&mut self, message: &str, message_type: MessageType) {
        match message_type {
            MessageType::Invocation => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
            MessageType::StreamItem => {
                if let Ok(item) = MessageParser::parse_message::<StreamItem<R>>(message) {
                    self.completer.push(item.item);
                } else {
                    error!("Cannot update stream {} with unparseable item {}", self.invocation_id, message);
                }
            },
            MessageType::Completion => {
                if let Ok(_) = MessageParser::parse_message::<Completion<R>>(message) {
                    self.completer.close();
                } else {
                    error!("Cannot parse completition: {}", message);
                }
            },
            MessageType::StreamInvocation => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
            MessageType::CancelInvocation => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
            MessageType::Ping => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
            MessageType::Close => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
            MessageType::Other => panic!("Cannot update stream {} with message {:?}", self.invocation_id, message),
        }
    }

    fn is_completed(&self) -> bool {
        self.completed
    }

    fn dispose(mut self) {
        self.dispose_internal();
    }
}