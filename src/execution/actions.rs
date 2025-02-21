use crate::protocol::negotiate::MessageType;

pub(crate) trait UpdatableAction {
    fn update_with(&mut self, message: &str, message_type: MessageType);
    #[allow(dead_code)]
    fn is_completed(&self) -> bool;
    #[allow(dead_code)]
    fn dispose(self);
}
