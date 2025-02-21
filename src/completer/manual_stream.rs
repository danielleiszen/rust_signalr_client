use std::{collections::VecDeque, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll, Waker}};

use futures::Stream;

struct ManualStreamState<T> {
    queue: Arc<Mutex<VecDeque<Option<T>>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> Clone for ManualStreamState<T> {
    fn clone(&self) -> Self {
        Self { queue: self.queue.clone(), waker: self.waker.clone() }
    }
}

impl<T> ManualStreamState<T> {
    pub fn new() -> Self {
        ManualStreamState {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            waker: Arc::new(Mutex::new(None)),
        }
    }

    fn push(&self, item: T) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(Some(item));
        if let Some(waker) = self.waker.lock().unwrap().take() {
            // debug!("Waking stream...");
            waker.wake();
        }
    }

    fn close(&self) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(None);
        if let Some(waker) = self.waker.lock().unwrap().take() {
            waker.wake();
        }
    }
}


pub struct ManualStream<T> {
    state: ManualStreamState<T>,
}

impl<T> ManualStream<T> {
    pub fn create() -> (Self, ManualStreamCompleter<T>) {
        let state = ManualStreamState::new();

        (ManualStream {
            state: state.clone()
        }, ManualStreamCompleter {
            state: state
        })
    }
}

pub struct ManualStreamCompleter<T> {
    state: ManualStreamState<T>,
}

impl<T> ManualStreamCompleter<T> {
    pub fn push(&self, item: T) {
        self.state.push(item);
    }

    pub fn close(&self) {
        self.state.close();
    }
}

impl<T> Stream for ManualStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // debug!("Polling stream...");
        let mut queue = self.state.queue.lock().unwrap();
        if let Some(item) = queue.pop_front() {
            // debug!("Item popped...");
            match item {
                Some(value) => { 
                    // debug!("Poll Ready with value");
                    Poll::Ready(Some(value))
                },
                None => {
                    // debug!("Poll Ready without value");
                    Poll::Ready(None)
                },
            }
        } else {
            // debug!("Waker is peding..");
            let mut waker = self.state.waker.lock().unwrap();
            *waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
