use std::{str::FromStr, sync::{Arc, Weak}};

use crate::{execution::{Storage, UpdatableActionStorage}, protocol::{messages::{MessageParser, RECORD_SEPARATOR}, negotiate::{HandshakeRequest, Ping}}};

use super::{Communication, reconnection::ReconnectionConfig};
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use http::Uri;
use log::{error, info};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_native_tls::native_tls::TlsConnector;
use tokio_websockets::{ClientBuilder, MaybeTlsStream, Message, WebSocketStream};

trait CommunicationDisconnectionHandler: Send {
    fn on_connection_dropped(&self);
}

/// Context for manual reconnection, passed to user's disconnection handler.
/// Allows the user to manually trigger reconnection attempts.
#[derive(Clone)]
pub struct ReconnectionContext {
    state: Weak<Mutex<ConnectionState>>,
    endpoint: Uri,
    actions: UpdatableActionStorage,
    reconnection_config: ReconnectionConfig,
}

impl ReconnectionContext {
    /// Attempt to reconnect once. Returns Ok(()) on success.
    pub async fn reconnect(&self) -> Result<(), String> {
        let state = self.state.upgrade().ok_or("Client has been dropped")?;

        // Check if already connected
        {
            let guard = state.lock().await;
            if let ConnectionState::Connected(_) = &*guard {
                return Ok(());
            }
        }

        // Set state to reconnecting
        {
            let mut guard = state.lock().await;
            if let ConnectionState::NotConnected(DisconnectionReason::LocalClosed) = *guard {
                return Err("Client was locally disconnected".to_string());
            }
            *guard = ConnectionState::NotConnected(DisconnectionReason::Reconnecting);
        }

        info!("Manual reconnection attempt to {}...", self.endpoint);

        match CommunicationClient::connect_to_server(self.endpoint.clone()).await {
            Ok((write, read)) => {
                let mut guard = state.lock().await;

                // Check again if locally closed during reconnection
                if let ConnectionState::NotConnected(DisconnectionReason::LocalClosed) = *guard {
                    return Err("Client was locally disconnected".to_string());
                }

                let new_handler = ClientDisconnectionHandler {
                    state: self.state.clone(),
                    endpoint: self.endpoint.clone(),
                    actions: self.actions.clone(),
                    reconnection_config: self.reconnection_config.clone(),
                    user_handler: None, // Manual mode doesn't re-trigger automatic reconnection
                };

                let mut conn_struct = CommunicationConnection {
                    _receiver: None,
                    _sink: write,
                };
                conn_struct.start_receiving(read, self.actions.clone(), new_handler);
                *guard = ConnectionState::Connected(Arc::new(Mutex::new(conn_struct)));
                info!("Manual reconnection successful");
                Ok(())
            },
            Err(e) => {
                error!("Manual reconnection failed: {}", e);
                let mut guard = state.lock().await;
                *guard = ConnectionState::NotConnected(DisconnectionReason::RemoteClosed);
                Err(e)
            }
        }
    }

    /// Attempt reconnection with automatic retries using the configured policy.
    /// Returns Ok(()) on success, Err if all attempts are exhausted.
    pub async fn reconnect_with_policy(&self) -> Result<(), String> {
        let mut retry_count = 0u32;
        let start_time = std::time::Instant::now();

        loop {
            let delay = self.reconnection_config.policy.next_retry_delay(
                retry_count,
                start_time.elapsed().as_millis() as u64
            );

            if let Some(d) = delay {
                if retry_count > 0 {
                    tokio::time::sleep(d).await;
                }

                match self.reconnect().await {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        if e.contains("locally disconnected") {
                            return Err(e);
                        }
                        retry_count += 1;
                    }
                }
            } else {
                return Err("Reconnection attempts exhausted".to_string());
            }
        }
    }

    /// Check if currently connected
    pub async fn is_connected(&self) -> bool {
        if let Some(state) = self.state.upgrade() {
            let guard = state.lock().await;
            matches!(&*guard, ConnectionState::Connected(_))
        } else {
            false
        }
    }

    /// Get the endpoint URI
    pub fn endpoint(&self) -> &Uri {
        &self.endpoint
    }
}

struct CommunicationConnection {
    _sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    _receiver: Option<JoinHandle<()>>,
}

impl CommunicationConnection {
    fn start_receiving(&mut self, mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>, mut storage: impl Storage + Send + 'static, disconnection_handler: impl CommunicationDisconnectionHandler + 'static) {
        let handle = tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                if item.is_ok() {
                    for message in CommunicationClient::get_messages(item.unwrap()) {
                        let ping = MessageParser::parse_message::<Ping>(&message);

                        if ping.is_ok() {
                            let res = storage.process_message(message, ping.unwrap().message_type());

                            if res.is_err() {
                                error!("Error occured parsing message {}", res.unwrap_err());
                            }
                        } else {
                            error!("Message could not be parsed: {:?}", message);
                        }
                    }
                }
            }

            disconnection_handler.on_connection_dropped();
        });

        self._receiver = Some(handle);
    }

    async fn send<T: serde::Serialize>(&mut self, data: T) -> Result<(), String> {
        let json = MessageParser::to_json(&data).unwrap();
        
        self._sink.send(Message::text(json)).await.map_err(|e| e.to_string())
    }

    fn stop_receiving(&mut self) {
        if self._receiver.is_some() {
            info!("Stopping receiver...");
            let receiver = self._receiver.take().unwrap();

            receiver.abort();
            info!("Receiver thread aborted");
        }
    }
}

impl Drop for CommunicationConnection {
    fn drop(&mut self) {
        info!("Dropping connection...");

        self.stop_receiving();
    }
}

#[derive(Clone, Debug)]
enum DisconnectionReason {
    RemoteClosed,
    LocalClosed,
    NeverOpened,
    Reconnecting,
}

enum ConnectionState {
    NotConnected(DisconnectionReason),
    Connected(Arc<Mutex<CommunicationConnection>>)
}

impl Clone for ConnectionState{
    fn clone(&self) -> Self {
        match self {
            Self::NotConnected(reason) => Self::NotConnected(reason.clone()),
            Self::Connected(arg0) => Self::Connected(arg0.clone()),
        }
    }
}

pub struct CommunicationClient {
    _endpoint: Uri,
    _state : Arc<Mutex<ConnectionState>>,
    _actions: UpdatableActionStorage,
    _reconnection_config: ReconnectionConfig,
    _disconnection_handler: Option<Arc<Box<dyn Fn(ReconnectionContext) + Send + Sync>>>,
}

impl Clone for CommunicationClient {
    fn clone(&self) -> Self {
        Self { 
            _endpoint: self._endpoint.clone(), 
            _state: self._state.clone(),
            _actions: self._actions.clone(),
            _reconnection_config: self._reconnection_config.clone(),
            _disconnection_handler: self._disconnection_handler.clone(),
        }
    }
}

impl Communication for CommunicationClient {
    async fn connect(configuration: &super::ConnectionData) -> Result<Self, String> {
        let mut ret = CommunicationClient::create(configuration);

        let res = ret.connect_internal().await;

        if res.is_ok() {
            return Ok(ret);
        } else {
            return Err(res.err().unwrap());
        }
    }

    fn get_storage(&self) -> Result<crate::execution::UpdatableActionStorage, String> {
        Ok(self._actions.clone())
    }
    
    async fn send<T: serde::Serialize>(&mut self, data: T) -> Result<(), String> {
        let state = self._state.lock().await;
        match &*state {
            ConnectionState::NotConnected(reason) => Err(format!("Client is not connected, cannot send: {:?}", reason)),
            ConnectionState::Connected(mutex) => {
                let mut connection = mutex.lock().await;

                connection.send(data).await
            },
        }
    }

    async fn disconnect(&mut self) {        
        let mut state = self._state.lock().await;
        let mut dropped = false;
        
        match &*state {
            ConnectionState::NotConnected(reason) => {
                info!("The client is not connected: {:?}, cannot disconnect", reason);
            },
            ConnectionState::Connected(mutex) => {
                let count = Arc::strong_count(mutex) - 1;

                if count == 0 {
                    info!("The underlying connection is going to be disposed.");                 
                    dropped = true;
                } else {
                    info!("The underlying connection has {} more references, not disconnecting.", count);
                }
            },
        }

        if dropped {
            *state = ConnectionState::NotConnected(DisconnectionReason::LocalClosed);
        }
    }    
}

impl CommunicationClient {
    fn create(configuration: &super::ConnectionData) -> Self {
        info!("Creating communication client to {}", &configuration.get_endpoint());
        let endpoint = Uri::from_str(&configuration.get_endpoint()).expect(&format!("The endpoint Uri {:?} is invalid", configuration.get_endpoint().as_str()));

        CommunicationClient {
            _endpoint: endpoint,           
            _state: Arc::new(Mutex::new(ConnectionState::NotConnected(DisconnectionReason::NeverOpened))),
            _actions: UpdatableActionStorage::new(),
            _reconnection_config: ReconnectionConfig::default(),
            _disconnection_handler: None,
        }
    }

    pub fn set_reconnection_config(&mut self, config: ReconnectionConfig) {
        self._reconnection_config = config;
    }

    pub fn set_disconnection_handler(&mut self, handler: impl Fn(ReconnectionContext) + Send + Sync + 'static) {
        self._disconnection_handler = Some(Arc::new(Box::new(handler)));
    }

    async fn connect_to_server(endpoint: Uri) -> Result<(SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>, SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>), String> {
        let stream: Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, http::Response<()>), tokio_websockets::Error>;
        info!("Connecting to endpoint {}", endpoint);
         
        if Some("wss") == endpoint.scheme_str() {
            info!("Connection to secure endpoint...");
            let Ok(connector) = TlsConnector::new() else { return Err("Cannot create default TLS connector".to_string()); };
        
            let connector = tokio_websockets::Connector::NativeTls(connector.into());
            stream = ClientBuilder::from_uri(endpoint.clone()).connector(&connector).connect().await;             
        } else {
            info!("Connection to plain endpoint...");
            stream = ClientBuilder::from_uri(endpoint.clone()).connect().await;
        }        

        match stream {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
        
                info!("Initiating handshake...");
                let handshake = HandshakeRequest::new("json".to_string());
                let message = MessageParser::to_json(&handshake).unwrap();
                let hsres = write.send(Message::text(message)).await;
        
                if hsres.is_ok() {            
                    if let Some(hand) = read.next().await {
                        if hand.is_ok() {                            
                            Ok((write, read))
                        } else {
                            Err(hand.err().unwrap().to_string())
                        }
                    } else {
                        Err("Handshake error".to_string())
                    }
                } else {
                    Err(hsres.err().unwrap().to_string())
                }    
            },
            Err(error) => {
                Err(error.to_string())
            },
        }
    }

    async fn connect_internal(&mut self) -> Result<(), String> {
        let res = CommunicationClient::connect_to_server(self._endpoint.clone()).await;

        match res {
            Ok((write, read)) => {
                let mut connection = CommunicationConnection {
                    _receiver: None,
                    _sink: write,
                };

                let handler = ClientDisconnectionHandler {
                    state: Arc::downgrade(&self._state),
                    endpoint: self._endpoint.clone(),
                    actions: self._actions.clone(),
                    reconnection_config: self._reconnection_config.clone(),
                    user_handler: self._disconnection_handler.clone(),
                };
        
                connection.start_receiving(read, self._actions.clone(), handler);                
                
                let mut state = self._state.lock().await;
                *state = ConnectionState::Connected(Arc::new(Mutex::new(connection)));

                Ok(())
            },
            Err(e) => Err(e),
        }
    }
    
    fn get_messages(message: Message) -> Vec<String> {
        if message.is_text() {
            if let Some(txt) = message.as_text() {
                return txt.split(RECORD_SEPARATOR)
                   .map(|s| MessageParser::strip_record_separator(s).to_string())
                   .filter(|s| s.len() > 0)
                   .collect();
            }
        }

        Vec::new()
    }
}

struct ClientDisconnectionHandler {
    state: Weak<Mutex<ConnectionState>>,
    endpoint: Uri,
    actions: UpdatableActionStorage,
    reconnection_config: ReconnectionConfig,
    /// If set, user has full control over reconnection (manual mode).
    /// If None, automatic reconnection is used.
    user_handler: Option<Arc<Box<dyn Fn(ReconnectionContext) + Send + Sync>>>,
}

impl CommunicationDisconnectionHandler for ClientDisconnectionHandler {
    fn on_connection_dropped(&self) {
        let state = self.state.clone();
        let endpoint = self.endpoint.clone();
        let actions = self.actions.clone();
        let config = self.reconnection_config.clone();
        let user_handler = self.user_handler.clone();

        tokio::spawn(async move {
            // Check if locally closed - if so, don't do anything
            if let Some(s) = state.upgrade() {
                let guard = s.lock().await;
                if let ConnectionState::NotConnected(DisconnectionReason::LocalClosed) = *guard {
                    return;
                }
            } else {
                return;
            }

            // If user has a handler, give them full control (manual mode)
            if let Some(handler) = user_handler {
                // Set state to RemoteClosed (user will change it if they reconnect)
                if let Some(s) = state.upgrade() {
                    let mut guard = s.lock().await;
                    *guard = ConnectionState::NotConnected(DisconnectionReason::RemoteClosed);
                }

                // Create context for manual reconnection
                let context = ReconnectionContext {
                    state: state.clone(),
                    endpoint: endpoint.clone(),
                    actions: actions.clone(),
                    reconnection_config: config.clone(),
                };

                info!("Connection dropped. Calling user's disconnection handler (manual mode).");
                handler(context);
                return;
            }

            // No user handler - use automatic reconnection
            if let Some(s) = state.upgrade() {
                let mut guard = s.lock().await;
                *guard = ConnectionState::NotConnected(DisconnectionReason::Reconnecting);
            }

            let mut retry_count = 0;
            let start_time = std::time::Instant::now();

            loop {
                let delay = config.policy.next_retry_delay(retry_count, start_time.elapsed().as_millis() as u64);

                if let Some(d) = delay {
                    tokio::time::sleep(d).await;
                    info!("Reconnecting to {} (attempt {})...", endpoint, retry_count + 1);

                    match CommunicationClient::connect_to_server(endpoint.clone()).await {
                        Ok((write, read)) => {
                             if let Some(s) = state.upgrade() {
                                let mut guard = s.lock().await;
                                if let ConnectionState::NotConnected(DisconnectionReason::LocalClosed) = *guard {
                                    return;
                                }

                                let new_handler = ClientDisconnectionHandler {
                                    state: state.clone(),
                                    endpoint: endpoint.clone(),
                                    actions: actions.clone(),
                                    reconnection_config: config.clone(),
                                    user_handler: None, // Automatic mode continues without user handler
                                };

                                let mut conn_struct = CommunicationConnection {
                                    _receiver: None,
                                    _sink: write,
                                };
                                conn_struct.start_receiving(read, actions.clone(), new_handler);
                                *guard = ConnectionState::Connected(Arc::new(Mutex::new(conn_struct)));
                                info!("Reconnected successfully (automatic mode)");
                                return;
                             } else {
                                 return;
                             }
                        },
                        Err(e) => {
                            error!("Reconnection failed: {}", e);
                            retry_count += 1;
                        }
                    }
                } else {
                    info!("Automatic reconnection attempts exhausted.");
                    if let Some(s) = state.upgrade() {
                        let mut guard = s.lock().await;
                        *guard = ConnectionState::NotConnected(DisconnectionReason::RemoteClosed);
                    }
                    return;
                }
            }
        });
    }
}