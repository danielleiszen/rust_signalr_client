use std::{future::Future, pin::Pin, sync::{Arc, Mutex}, task::{Context, Poll, Waker}};
use std::marker::Unpin;

use log::{error, warn};

enum State<T> {
    Incomplete,
    Waiting(Waker),
    Complete(Option<T>),
}

impl<T> State<T> {
    fn new(value: Option<T>) -> Self {
        match value {
            None => Self::Incomplete,
            v @ Some(_) => Self::Complete(v),
        }
    }
}

/// A future that is manually completed.
///
/// `ManualFuture` is useful in sync-to-async boundaries where a future should be returned,
/// but the completion should be triggered by an async call. Its creation returns the future
/// and a completer that can be used to trigger completion.
///
/// # Examples
///
/// ```
/// #[tokio::main]
/// async fn main() {
///     let (future, completer) = ManualFuture::new();
///
///     tokio::spawn(async move {
///         // Simulate some async work
///         tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
///         completer.complete(42);
///     });
///
///     let result = future.await;
///     println!("Result: {}", result);
/// }
/// ```
pub struct ManualFuture<T: Unpin> {
    state: Arc<Mutex<State<T>>>,
}

impl<T: Unpin> ManualFuture<T> {
    /// Creates a new `ManualFuture` and its completer.
    ///
    /// # Returns
    ///
    /// A tuple containing the `ManualFuture` and its `ManualFutureCompleter`.
    ///
    /// # Examples
    ///
    /// ```
    /// let (future, completer) = ManualFuture::new();
    /// ```
    pub fn new() -> (Self, ManualFutureCompleter<T>) {
        let state: State<T> = State::new(None);
        let a = Arc::new(Mutex::new(state));
        (Self { state: a.clone() }, ManualFutureCompleter { state: a })
    }

    /// Returns if the `ManualFuture` is completed.
    ///
    /// # Examples
    ///
    /// ```
    /// let (future, completer) = ManualFuture::new();
    /// let mustbefalse = future.is_completed();
    /// completer.complete(42);
    /// let mustbetrue = future.is_completed();
    /// ```    
    #[allow(dead_code)]
    pub fn is_completed(&self) -> bool {
        let state = self.state.lock().unwrap();

        match *state {
            State::Incomplete => false,
            State::Waiting(_) => false,
            State::Complete(_) => true,
        }
    }
}

impl<T: Unpin> Clone for ManualFuture<T> {
    fn clone(&self) -> Self {
        Self { state: self.state.clone() }
    }
}

/// The completer utility for `ManualFuture`.
///
/// # Examples
///
/// ```
/// let (future, completer) = ManualFuture::new();
/// completer.complete(42);
/// ```
pub struct ManualFutureCompleter<T: Unpin> {
    state: Arc<Mutex<State<T>>>,
}

impl<T: Unpin> ManualFutureCompleter<T> {             
    /// Completes the future with the given value.
    ///
    /// This method triggers the completion of the associated `ManualFuture`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to complete the future with.
    ///
    /// # Examples
    ///
    /// ```
    /// let (future, completer) = ManualFuture::new();
    /// completer.complete(42);
    /// ```
    pub fn complete(self, value: T) {
        let mut state = self.state.lock().unwrap();

        match std::mem::replace(&mut *state, State::Complete(Some(value))) {
            State::Incomplete => {}
            State::Waiting(w) => w.wake(),
            _ => panic!("Future is completed or cancelled already. This happened because complete method is called more than once or after cancel is called. If not sure, study is_completed before calling complete."),
        }
    }

    /// Cancels the future, dropping any pending value.
    ///
    /// This method cancels the associated `ManualFuture`, ensuring it will not complete.
    ///
    /// # Examples
    ///
    /// ```
    /// let (future, completer) = ManualFuture::new();
    /// completer.cancel();
    /// ```
    pub fn cancel(self) {
        warn!("Cancelling future...");
        let mut state = self.state.lock().unwrap();

        match std::mem::replace(&mut *state, State::Complete(None)) {
            _ => {},
        }
    }

    /// Returns if the `ManualFuture` is completed.
    ///
    /// # Examples
    ///
    /// ```
    /// let (future, completer) = ManualFuture::new();
    /// let mustbefalse = completer.is_completed();
    /// completer.complete(42);
    /// let mustbetrue = completer.is_completed();
    /// ```
    #[allow(dead_code)]
    pub fn is_completed(&self) -> bool {
        let state = self.state.lock().unwrap();

        match *state {
            State::Incomplete => false,
            State::Waiting(_) => false,
            State::Complete(_) => true,
        }
    }
}

impl<T: Unpin> Clone for ManualFutureCompleter<T> {
    fn clone(&self) -> Self {
        Self { state: self.state.clone() }
    }
}

impl<T: Unpin> Future for ManualFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        match &mut *state {
            s @ State::Incomplete => *s = State::Waiting(cx.waker().clone()),
            State::Waiting(w) if w.will_wake(cx.waker()) => {}
            s @ State::Waiting(_) => *s = State::Waiting(cx.waker().clone()),
            State::Complete(v) => match v.take() {
                Some(v) => {
                    return Poll::Ready(v)
                },
                None => {
                    error!("Future is cancelled or double polled...");
                },
            },
        }

        Poll::Pending
    }
}

