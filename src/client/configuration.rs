use crate::client::client::DisconnectionHandler;
use crate::communication::reconnection::ReconnectionConfig;

#[derive(Clone)]
pub(crate) enum Authentication {
    None,
    Basic {
        user: String,
        password: Option<String>,
    },
    Bearer {
        token: String,
    },
} 

pub struct ConnectionConfiguration {
    _secure: bool,
    _domain: String,
    _hub: String,
    _port: Option<i32>,
    _authentication: Authentication,
    _disconnection: Option<Box<dyn DisconnectionHandler + Send + Sync>>,
    _reconnection: ReconnectionConfig,
}

impl ConnectionConfiguration {
    pub(crate) fn new(domain: String, hub: String) -> Self {
        ConnectionConfiguration {
            _authentication: Authentication::None,
            _domain: domain,
            _secure: true,
            _hub: hub,
            _port: None,
            _disconnection: None,
            _reconnection: ReconnectionConfig::default(),
        }
    }

    /// Sets the port for the connection.
    ///
    /// # Arguments
    ///
    /// * `port` - An integer specifying the port number to use for the connection.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.with_port(5220);
    /// }).await.unwrap();
    /// ```
    pub fn with_port(&mut self, port: i32) -> &ConnectionConfiguration {
        self._port = Some(port);

        self
    }

    /// Sets the hub name for the connection.
    ///
    /// # Arguments
    ///
    /// * `hub` - A `String` specifying the name of the hub to connect to.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.with_hub("myHub".to_string());
    /// }).await.unwrap();
    /// ```
    pub fn with_hub(&mut self, hub: String) -> &ConnectionConfiguration {
        self._hub = hub;

        self
    }

    /// Configures the connection to use a secure (HTTPS) protocol.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.secure();
    /// }).await.unwrap();
    /// ```    
    pub fn secure(&mut self) -> &ConnectionConfiguration {
        self._secure = true;

        self
    }

    /// Configures the connection to use an unsecure (HTTP) protocol.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.unsecure();
    /// }).await.unwrap();
    /// ```    
    pub fn unsecure(&mut self) -> &ConnectionConfiguration {
        self._secure = false;

        self
    }

    /// Configures the connection to use basic authentication.
    ///
    /// # Arguments
    ///
    /// * `user` - A `String` specifying the username for authentication.
    /// * `password` - An `Option<String>` specifying the password for authentication. If `None`, no password is used.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.authenticate_basic("username".to_string(), Some("password".to_string()));
    /// }).await.unwrap();
    /// ```    
    pub fn authenticate_basic(&mut self, user: String, password: Option<String>) -> &ConnectionConfiguration {
        self._authentication = Authentication::Basic { user: user, password: password };

        self
    }

    /// Configures the connection to use bearer token authentication.
    ///
    /// # Arguments
    ///
    /// * `token` - A `String` specifying the bearer token for authentication.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.authenticate_bearer("your_bearer_token".to_string());
    /// }).await.unwrap();
    /// ```    
    pub fn authenticate_bearer(&mut self, token: String) -> &ConnectionConfiguration {
        self._authentication = Authentication::Bearer { token: token };

        self
    }

    /// Sets a disconnection handler for the connection.
    ///
    /// # Arguments
    ///
    /// * `handler` - An implementation of the `DisconnectionHandler` trait to handle disconnection events.
    ///
    /// # Returns
    ///
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    /// # Examples
    /// ```
    /// let client = SignalRClient::connect_with("localhost", "test", |c| {
    ///     c.with_disconnection_handler(MyDisconnectionHandler{});
    /// }).await.unwrap();
    /// ```
    pub fn with_disconnection_handler<Handler: DisconnectionHandler + Send + Sync + 'static>(&mut self, handler: Handler) -> &ConnectionConfiguration {
        self._disconnection = Some(Box::new(handler));

        self
    }

    pub(crate) fn get_web_url(&self) -> String {
        format!("{}://{}/{}", self.get_http_schema(), self.get_domain(), self._hub)
    }

    pub(crate) fn get_socket_url(&self) -> String {
        format!("{}://{}/{}", self.get_socket_schema(), self.get_domain(), self._hub)
    }

    pub(crate) fn get_authentication(&self) -> Authentication {
        self._authentication.clone()
    }

    fn get_http_schema(&self) -> String {
        if self._secure {
            "https".to_string()
        } else {
            "http".to_string()
        }
    }

    fn get_socket_schema(&self) -> String {
        if self._secure {
            "wss".to_string()
        } else {
            "ws".to_string()
        }
    }

    fn get_domain(&self) -> String {
        match self._port {
            Some(port) => format!("{}:{}", self._domain, port),
            None => self._domain.clone()
        }
    }

    pub(crate)fn get_disconnection_handler(&mut self) -> Option<Box<dyn DisconnectionHandler + Send + Sync>> {
        let handler =  self._disconnection.take();
        handler
    }

    pub(crate) fn get_reconnection_config(&self) -> ReconnectionConfig {
        self._reconnection.clone()
    }

    /// Sets the reconnection policy for the connection.
    /// 
    /// # Arguments
    /// 
    /// * `config` - A `ReconnectionConfig` specifying the policy.
    /// 
    /// # Returns
    /// 
    /// * `&ConnectionConfiguration` - Returns a reference to the updated connection configuration.
    pub fn with_reconnection_policy(&mut self, config: ReconnectionConfig) -> &ConnectionConfiguration {
        self._reconnection = config;
        self
    }
}