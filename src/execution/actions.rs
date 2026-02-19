use crate::protocol::hub_protocol::MessagePayload;
use crate::protocol::negotiate::MessageType;

pub(crate) trait UpdatableAction: Send {
    fn update_with(&mut self, message: &MessagePayload, message_type: MessageType);
    #[allow(dead_code)]
    fn is_completed(&self) -> bool;
    #[allow(dead_code)]
    fn dispose(self);
}
