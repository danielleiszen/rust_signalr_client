use crate::{completer::{ManualFuture, ManualFutureCompleter}, protocol::{invoke::Completion, negotiate::MessageType}};
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

impl<R: DeserializeOwned + Unpin> UpdatableAction for InvocationAction<R> {
    fn update_with(&mut self, message: &str, message_type: MessageType) {
        // debug!("Updating invocation {}", self.invocation_id);

        match message_type {
            MessageType::Invocation => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::StreamItem => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::Completion => {
                if let Ok(completition) = MessageParser::parse_message::<Completion<R>>(message) {
                    if completition.is_result() {
                        info!("Completition is parsed");
                        self.complete(completition.unwrap_result());
                    } else {
                        error!("Cannot complete invocation {}, error: {}", self.invocation_id, completition.unwrap_error());
                    }
                } else {
                    error!("Cannot parse completition: {}", message);
                }
            },
            MessageType::StreamInvocation => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::CancelInvocation => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::Ping => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::Close => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
            MessageType::Other => panic!("Cannot complete invocation {}, with message {:?}", self.invocation_id, message),
        }
        
    }
    
    fn is_completed(&self) -> bool {
        self.completer.is_none()
    }

    fn dispose(mut self) {
        self.dispose_internal();
    }
}