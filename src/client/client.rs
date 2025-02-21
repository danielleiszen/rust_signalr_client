use futures::Stream;
use log::info;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::communication::{Communication, CommunicationClient, HttpClient};
use crate::protocol::invoke::Invocation;
use crate::execution::{ArgumentConfiguration, CallbackHandler, Storage, StorageUnregistrationHandler, UpdatableActionStorage};

use super::{ConnectionConfiguration, InvocationContext};

/// A client for connecting to and interacting with a SignalR hub.
///
/// The `SignalRClient` can be used to invoke methods on the hub, send messages, and register callbacks.
/// The client can be cloned and used freely across different parts of your application.
///
/// # Examples
///
/// ```
/// // Connect to the SignalR server with custom configuration
/// let mut client = SignalRClient::connect_with("localhost", "test", |c| {
///     c.with_port(5220); // Set the port to 5220
///     c.unsecure(); // Use an unsecure (HTTP) connection
/// }).await.unwrap();
///
/// // Invoke the "SingleEntity" method and assert the result
/// let re = client.invoke::<TestEntity>("SingleEntity".to_string()).await;
/// assert!(re.is_ok());
///
/// // Unwrap the result and assert the entity's text
/// let entity = re.unwrap();
/// assert_eq!(entity.text, "test".to_string());
///
/// // Log the entity's details
/// info!("Entity {}, {}", entity.text, entity.number);
///
/// // Enumerate "HundredEntities" and log each entity
/// let mut he = client.enumerate::<TestEntity>("HundredEntities".to_string()).await;
/// while let Some(item) = he.next().await {
///     info!("Entity {}, {}", item.text, item.number);
/// }
///
/// info!("Finished fetching entities, calling pushes");
///
/// // Invoke the "PushEntity" method with arguments and assert the result
/// let push1 = client.invoke_with_args::<bool, _>("PushEntity".to_string(), |c| {
///     c.argument(TestEntity {
///         text: "push1".to_string(),
///         number: 100,
///     });
/// }).await;
/// assert!(push1.unwrap());
///
/// // Clone the client and invoke the "PushTwoEntities" method with arguments
/// let mut secondclient = client.clone();
/// let push2 = secondclient.invoke_with_args::<TestEntity, _>("PushTwoEntities".to_string(), |c| {
///     c.argument(TestEntity {
///         text: "entity1".to_string(),
///         number: 200,
///     }).argument(TestEntity {
///         text: "entity2".to_string(),
///         number: 300,
///     });
/// }).await;
/// assert!(push2.is_ok());
///
/// // Unwrap the result and assert the merged entity's number
/// let entity = push2.unwrap();
/// assert_eq!(entity.number, 500);
/// info!("Merged Entity {}, {}", entity.text, entity.number);
///
/// // Drop the second client
/// drop(secondclient);
///
/// // Register callbacks for "callback1" and "callback2"
/// let c1 = client.register("callback1".to_string(), |ctx| {
///     let result = ctx.argument::<TestEntity>(0);
///     if result.is_ok() {
///         let entity = result.unwrap();
///         info!("Callback results entity: {}, {}", entity.text, entity.number);
///     }
/// });
///
/// let c2 = client.register("callback2".to_string(), |mut ctx| {
///     let result = ctx.argument::<TestEntity>(0);
///     if result.is_ok() {
///         let entity = result.unwrap();
///         info!("Callback2 results entity: {}, {}", entity.text, entity.number);
///         let e2 = entity.clone();
///         spawn(async move {
///             info!("Completing callback2");
///             let _ = ctx.complete(e2).await;
///         });
///     }
/// });
///
/// // Trigger the callbacks
/// info!("Calling callback1");
/// _ = client.send_with_args("TriggerEntityCallback".to_string(), |c| {
///     c.argument("callback1".to_string());
/// }).await;
///
/// info!("Calling callback2");
/// let succ = client.invoke_with_args::<bool, _>("TriggerEntityResponse".to_string(), |c| {
///     c.argument("callback2".to_string());
/// }).await;
/// assert!(succ.unwrap());
///
/// // Measure the time taken to fetch a million entities
/// let now = Instant::now();
/// {
///     let mut me = client.enumerate::<TestEntity>("MillionEntities".to_string()).await;
///     while let Some(_) = me.next().await {}
/// }
/// let elapsed = now.elapsed();
/// info!("1 million entities fetched in: {:.2?}", elapsed);
///
/// // Unregister the callbacks and disconnect the client
/// c1.unregister();
/// c2.unregister();
/// client.disconnect();
/// ```
pub struct SignalRClient {
    _actions: UpdatableActionStorage,
    _connection: CommunicationClient,
}

impl Drop for SignalRClient {
    fn drop(&mut self) {
        self._connection.disconnect();
    }
}

impl SignalRClient {
    /// Connects to a SignalR hub using the default connection configuration.
    ///
    /// # Arguments
    ///
    /// * `domain` - A string slice that holds the domain of the SignalR server.
    /// * `hub` - A string slice that holds the name of the hub to connect to.
    ///
    /// # Returns
    ///
    /// * `Result<Self, String>` - On success, returns an instance of `Self`. On failure, returns an error message as a `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// ```
    pub async fn connect(domain: &str, hub: &str) -> Result<Self, String> {
        SignalRClient::connect_internal(domain, hub, None::<fn(&mut ConnectionConfiguration)>).await
    }
    
    /// Connects to a SignalR hub with custom connection properties.
    ///
    /// # Arguments
    ///
    /// * `domain` - A string slice that holds the domain of the SignalR server.
    /// * `hub` - A string slice that holds the name of the hub to connect to.
    /// * `options` - A closure that allows the user to configure the connection properties.
    ///
    /// # Returns
    ///
    /// * `Result<Self, String>` - On success, returns an instance of `Self`. On failure, returns an error message as a `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.with_port(5220);
    ///     c.unsecure();
    /// }).await.unwrap();
    /// ```
    pub async fn connect_with<F>(domain: &str, hub: &str, options: F) -> Result<Self, String>
        where F: FnMut(&mut ConnectionConfiguration) 
    {
        SignalRClient::connect_internal(domain, hub, Some(options)).await
    }

    async fn connect_internal<F>(domain: &str, hub: &str, options: Option<F>) -> Result<Self, String>
        where F: FnMut(&mut ConnectionConfiguration)
    {
        let mut config = ConnectionConfiguration::new(domain.to_string(), hub.to_string());

        if options.is_some() {
            let mut ops = options.unwrap();
            (ops)(&mut config);
        }

        let result = HttpClient::negotiate(config).await;

        if result.is_ok() {
            // debug!("Negotiate response returned {:?}", result);
            let configuration = result.unwrap();
            info!("Negotiation successfull: {:?}", configuration);
            let res = CommunicationClient::connect(&configuration).await;

            if res.is_ok() {
                let client  = res.unwrap();
                let storage = client.get_storage();

                if storage.is_ok() {
                    let ret = SignalRClient {
                        _actions: storage.unwrap(),
                        _connection: client
                    };    
    
                    Ok(ret)    
                } else {
                    Err(storage.err().unwrap())
                }
            } else {
                return Err(res.err().unwrap());
            }
        } else {
            Err(result.err().unwrap())
        }
    }

    /// Registers a callback that can be called by the SignalR hub.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to register the callback for.
    /// * `callback` - A closure that takes an `InvocationContext` as an argument and defines the callback logic.
    ///
    /// # Returns
    ///
    /// * `impl CallbackHandler` - Returns an implementation of `CallbackHandler` that can be used to manage the callback. The `CallbackHandler` can be used to unregister the callback using its `unregister` method.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let handler = client.register("callback1".to_string(), |ctx| {
    ///     let result = ctx.argument::<TestEntity>(0);
    ///     if result.is_ok() {
    ///         let entity = result.unwrap();
    ///         info!("Callback results entity: {}, {}", entity.text, entity.number);
    ///     }
    /// });
    ///
    /// // Unregister the callback when it's no longer needed
    /// handler.unregister();
    /// ```   
    pub fn register(&mut self, target: String, callback: impl Fn(InvocationContext) + 'static) -> impl CallbackHandler
    {
        // debug!("CLIENT registering invocation callback to {}", &target);
        self._actions.add_callback(target.clone(), callback, self.clone());

        StorageUnregistrationHandler::new(self._actions.clone(), target.clone())
    }

    /// Invokes a specific target method on the SignalR hub and waits for the response.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to invoke on the hub.
    ///
    /// # Returns
    ///
    /// * `Result<T, String>` - On success, returns the response of type `T`. On failure, returns an error message as a `String`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the response, which must implement `DeserializeOwned` and `Unpin`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let response: Result<TestEntity, String> = client.invoke("SingleEntity".to_string()).await;
    /// match response {
    ///     Ok(entity) => {
    ///         info!("Received entity: {}, {}", entity.text, entity.number);
    ///     }
    ///     Err(e) => {
    ///         error!("Failed to invoke method: {}", e);
    ///     }
    /// }
    /// ```    
    pub async fn invoke<T: 'static + DeserializeOwned + Unpin>(&mut self, target: String) -> Result<T, String> {
        return self.invoke_internal(target, None::<fn(&mut ArgumentConfiguration)>).await;
    }

    /// Invokes a specific target method on the SignalR hub with custom arguments and waits for the response.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to invoke on the hub.
    /// * `configuration` - A mutable closure that allows the user to configure the arguments for the method call.
    ///
    /// # Returns
    ///
    /// * `Result<T, String>` - On success, returns the response of type `T`. On failure, returns an error message as a `String`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the response, which must implement `DeserializeOwned` and `Unpin`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let response: Result<TestEntity, String> = client.invoke_with_args("PushTwoEntities".to_string(), |c| {
    ///     c.argument(TestEntity {
    ///         text: "entity1".to_string(),
    ///         number: 200,
    ///     }).argument(TestEntity {
    ///         text: "entity2".to_string(),
    ///         number: 300,
    ///     });
    /// }).await;
    /// match response {
    ///     Ok(entity) => {
    ///         info!("Merged Entity {}, {}", entity.text, entity.number);
    ///     }
    ///     Err(e) => {
    ///         error!("Failed to invoke method: {}", e);
    ///     }
    /// }
    /// ```    
    pub async fn invoke_with_args<T: 'static + DeserializeOwned + Unpin, F>(&mut self, target: String, configuration: F) -> Result<T, String>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        return self.invoke_internal(target, Some(configuration)).await;
    }

    async fn invoke_internal<T: 'static + DeserializeOwned + Unpin, F>(&mut self, target: String, configuration: Option<F>) -> Result<T, String>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        let invocation_id = self._actions.create_key(target.clone());
        let ret = self._actions.add_invocation::<T>(invocation_id.clone());

        let mut invocation = Invocation::create_single(target.clone());
        invocation.with_invocation_id(invocation_id);

        if configuration.is_some() {
            let mut args = ArgumentConfiguration::new(invocation);
            configuration.unwrap()(&mut args);

            invocation = args.build_invocation();
        }

        let res = self._connection.send(&invocation).await;

        if res.is_ok() {
            Ok(ret.await)
        } else {
            Err(res.err().unwrap())
        }
    }

    /// Calls a specific target method on the SignalR hub without waiting for the response.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to call on the hub.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let result = client.send("TriggerEntityCallback".to_string()).await;
    /// match result {
    ///     Ok(_) => {
    ///         info!("Method called successfully");
    ///     }
    ///     Err(e) => {
    ///         error!("Failed to call method: {}", e);
    ///     }
    /// }
    /// ```
    pub async fn send(&mut self, target: String) -> Result<(), String>
    {
        return self.send_internal(target, None::<fn(&mut ArgumentConfiguration)>).await;
    }

    /// Calls a specific target method on the SignalR hub with custom arguments without waiting for the response.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to call on the hub.
    /// * `configuration` - A closure that allows the user to configure the arguments for the method call.
    ///
    /// # Returns
    ///
    /// * `Result<(), String>` - On success, returns `Ok(())`. On failure, returns an error message as a `String`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let result = client.send_with_args("TriggerEntityCallback".to_string(), |c| {
    ///     c.argument("callback1".to_string());
    /// }).await;
    /// match result {
    ///     Ok(_) => {
    ///         info!("Method called successfully");
    ///     }
    ///     Err(e) => {
    ///         error!("Failed to call method: {}", e);
    ///     }
    /// }
    /// ```    
    pub async fn send_with_args<F>(&mut self, target: String, configuration: F) -> Result<(), String>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        return self.send_internal(target, Some(configuration)).await;
    }

    async fn send_internal<F>(&mut self, target: String, configuration: Option<F>) -> Result<(), String>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        // debug!("CLIENT creating actual invocation data");
        let mut invocation = Invocation::create_single(target.clone());

        if configuration.is_some() {
            let mut args = ArgumentConfiguration::new(invocation);
            configuration.unwrap()(&mut args);

            invocation = args.build_invocation();
        }

        let ret = self._connection.send(&invocation).await;
        ret
    }

    pub(crate) async fn send_direct<T: Serialize>(&mut self, data: T) -> Result<(), String>
    {
        let ret = self._connection.send(&data).await;
        
        ret
    }

    /// Calls a specific target method on the SignalR hub and returns a stream for receiving data asynchronously.
    ///
    /// The target method on the hub should return an `IAsyncEnumerable` to send back data asynchronously.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to call on the hub.
    ///
    /// # Returns
    ///
    /// * `impl Stream<Item = T>` - Returns a stream of items of type `T`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the items in the stream, which must implement `DeserializeOwned` and `Unpin`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let mut stream = client.enumerate::<TestEntity>("HundredEntities".to_string()).await;
    /// while let Some(entity) = stream.next().await {
    ///     info!("Received entity: {}, {}", entity.text, entity.number);
    /// }
    /// ```
    pub async fn enumerate<T: 'static + DeserializeOwned + Unpin>(&mut self, target: String) -> impl Stream<Item = T> {
        return self.enumerate_internal(target, None::<fn(&mut ArgumentConfiguration)>).await;
    }

    /// Calls a specific target method on the SignalR hub with custom arguments and returns a stream for receiving data asynchronously.
    ///
    /// The target method on the hub should return an `IAsyncEnumerable` to send back data asynchronously.
    ///
    /// # Arguments
    ///
    /// * `target` - A `String` specifying the name of the target method to call on the hub.
    /// * `configuration` - A mutable closure that allows the user to configure the arguments for the method call.
    ///
    /// # Returns
    ///
    /// * `impl Stream<Item = T>` - Returns a stream of items of type `T`.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The type of the items in the stream, which must implement `DeserializeOwned` and `Unpin`.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect("localhost", "test").await.unwrap();
    /// let mut stream = client.enumerate_with_args::<TestEntity, _>("HundredEntities".to_string(), |c| {
    ///     c.argument("some_argument".to_string());
    /// }).await;
    /// while let Some(entity) = stream.next().await {
    ///     info!("Received entity: {}, {}", entity.text, entity.number);
    /// }
    /// ```    
    pub async fn enumerate_with_args<T: 'static + DeserializeOwned + Unpin, F>(&mut self, target: String, configuration: F) -> impl Stream<Item = T>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        return self.enumerate_internal(target, Some(configuration)).await;
    }

    async fn enumerate_internal<T: 'static + DeserializeOwned + Unpin, F>(&mut self, target: String, configuration: Option<F>) -> impl Stream<Item = T>
        where F : FnMut(&mut ArgumentConfiguration)
    {
        let invocation_id = self._actions.create_key(target.clone());
        let res = self._actions.add_stream::<T>(invocation_id.clone());        
        let mut invocation = Invocation::create_multiple(target.clone());
        invocation.with_invocation_id(invocation_id);

        if configuration.is_some() {
            let mut args = ArgumentConfiguration::new(invocation);
            configuration.unwrap()(&mut args);

            invocation = args.build_invocation();
        }

        let _ = self._connection.send(&invocation).await;

        res
    }

    pub fn disconnect(mut self) {
        self._connection.disconnect();
    }
}

impl Clone for SignalRClient {
    fn clone(&self) -> Self {
        Self { _actions: self._actions.clone(), _connection: self._connection.clone() }
    }
}