use log::error;
use serde::Serialize;

use crate::protocol::invoke::Invocation;

/// Lets the arguments to be configured for a method on the Hub
pub struct ArgumentConfiguration {
    invocation: Option<Invocation>,
}

impl ArgumentConfiguration {
    pub(crate) fn new(invocation: Invocation) -> Self {
        Self {  
            invocation: Some(invocation)
        }
    }

    /// Adds an argument to the method call configuration.
    ///
    /// The arguments do not have names; the order of the arguments must match the order expected by the hub method.
    ///
    /// # Arguments
    ///
    /// * `value` - The value of the argument to add, which must implement `Serialize`.
    ///
    /// # Returns
    ///
    /// * `&mut ArgumentConfiguration` - Returns a mutable reference to the updated argument configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let response: Result<bool, String> = client.invoke_with_args("PushEntity".to_string(), |c| {
    ///     c.argument(TestEntity {
    ///         text: "push1".to_string(),
    ///         number: 100,
    ///     });
    /// }).await;
    /// match response {
    ///     Ok(success) => {
    ///         if success {
    ///             info!("Entity pushed successfully");
    ///         } else {
    ///             error!("Failed to push entity");
    ///         }
    ///     }
    ///     Err(e) => {
    ///         error!("Failed to invoke method: {}", e);
    ///     }
    /// }
    /// ```    
    pub fn argument<T: Serialize>(&mut self, value: T) -> &mut ArgumentConfiguration {
        if self.invocation.is_some() {
            let succ = self.invocation.as_mut().unwrap().with_argument(value);

            if succ.is_err() {
                error!("Argument could not be put into invocation data.");
            }
        }

        self
    }

    pub(crate) fn build_invocation(mut self) -> Invocation {
        if self.invocation.is_some() {
            return self.invocation.take().unwrap();
        } else {
            panic!("Invocation cannot be built before it is provided");
        }     
    } 
}