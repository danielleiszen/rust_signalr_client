use std::{str::FromStr, sync::Arc};

use crate::{execution::{Storage, UpdatableActionStorage}, protocol::{messages::{MessageParser, RECORD_SEPARATOR}, negotiate::{HandshakeRequest, Ping}}};

use super::Communication;
use futures::{stream::{SplitSink, SplitStream}, SinkExt, StreamExt};
use http::Uri;
use log::{error, info};
use tokio::{net::TcpStream, sync::Mutex, task::JoinHandle};
use tokio_native_tls::native_tls::TlsConnector;
use tokio_websockets::{ClientBuilder, MaybeTlsStream, Message, WebSocketStream};

trait CommunicationDisconnectionHandler  {
    fn on_connection_dropped(&self);
}

struct CommunicationConnection {
    _sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    _receiver: Option<JoinHandle<()>>,
}

impl CommunicationConnection {
    fn start_receiving(&mut self, mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>, mut storage: impl Storage + Send + 'static, disconnection_handler: impl CommunicationDisconnectionHandler + Send + 'static) {
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
    _state : ConnectionState,
    _actions: UpdatableActionStorage,
}

impl Clone for CommunicationClient {
    fn clone(&self) -> Self {
        Self { 
            _endpoint: self._endpoint.clone(), 
            _state: self._state.clone(),
            _actions: self._actions.clone(),
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
        match &self._state {
            ConnectionState::NotConnected(reason) => Err(format!("Client is not connected, cannot send: {:?}", reason)),
            ConnectionState::Connected(mutex) => {
                let mut connection = mutex.lock().await;

                connection.send(data).await
            },
        }
    }

    fn disconnect(&mut self) {
        let mut dropped = false;

        match &self._state {
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
            self._state = ConnectionState::NotConnected(DisconnectionReason::LocalClosed);
        }
    }    
}

impl CommunicationClient {
    fn create(configuration: &super::ConnectionData) -> Self {
        info!("Creating communication client to {}", &configuration.get_endpoint());
        let endpoint = Uri::from_str(&configuration.get_endpoint()).expect(&format!("The endpoint Uri {:?} is invalid", configuration.get_endpoint().as_str()));

        CommunicationClient {
            _endpoint: endpoint,           
            _state: ConnectionState::NotConnected(DisconnectionReason::NeverOpened),
            _actions: UpdatableActionStorage::new(),
        }
    }

    async fn connect_internal(&mut self) -> Result<(), String> {
        let stream: Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, http::Response<()>), tokio_websockets::Error>;
        info!("Connecting to endpoint {}", self._endpoint);
         
        if Some("wss") == self._endpoint.scheme_str() {
            info!("Connection to secure endpoint...");
            let Ok(connector) = TlsConnector::new() else { return Err("Cannot create default TLS connector".to_string()); };
        
            let connector = tokio_websockets::Connector::NativeTls(connector.into());
            stream = ClientBuilder::from_uri(self._endpoint.clone()).connector(&connector).connect().await;             
        } else {
            info!("Connection to plain endpoint...");
            stream = ClientBuilder::from_uri(self._endpoint.clone()).connect().await;
        }        

        match stream {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
        
                info!("Initiating handshake...");
                let handshake = HandshakeRequest::new("json".to_string());
                let message = MessageParser::to_json(&handshake).unwrap();
                let hsres = write.send(Message::text(message)).await;
        
                if hsres.is_ok() {            
                    let mut connection = CommunicationConnection {
                        _receiver: None,
                        _sink: write,
                    };
            
                    if let Some(hand) = read.next().await {
                        if hand.is_ok() {                            
                            connection.start_receiving(read, self._actions.clone());                
                            self._state = ConnectionState::Connected(Arc::new(Mutex::new(connection)));
        
                            Ok(())
                        } else {
                            return Err(hand.err().unwrap().to_string());
                        }
                    } else {
                        return Err("Handshake error".to_string());
                    }
                } else {
                    return Err(hsres.err().unwrap().to_string());
                }    
            },
            Err(error) => {
                return Err(error.to_string());
            },
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