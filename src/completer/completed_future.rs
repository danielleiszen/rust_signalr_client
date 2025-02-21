use std::{cell::RefCell, future::Future, pin::Pin, task::{Context, Poll}};
use std::marker::Unpin;

pub struct CompletedFuture<T: Unpin> {
    data: RefCell<Option<T>>,
}

impl<T: Unpin> CompletedFuture<T> {
    #[allow(dead_code)]
    pub fn new(data: T) -> Self {
        Self { data: RefCell::new(Some(data)) }
    }

    #[allow(dead_code)]
    pub fn is_completed(&self) -> bool {
        true
    }
}

impl<T: Unpin> Future for CompletedFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        let data = self.data.borrow_mut().take().unwrap(); 
        
        return Poll::Ready(data);
    }
}

