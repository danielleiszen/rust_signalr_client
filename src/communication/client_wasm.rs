use std::{cell::RefCell, rc::Rc};

use log::{error, info, warn};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_sockets::{ConnectionStatus, PollingClient};

use crate::{completer::CompletedFuture,
    execution::
        {ManualFutureState, Storage, UpdatableActionStorage}, protocol::{hub_protocol::{HubProtocolKind, MessagePayload}, messages::{MessageParser, RECORD_SEPARATOR}, negotiate::{HandshakeRequest, HandshakeResponse, Ping}}};

use super::common::Communication;

#[wasm_bindgen]
extern "C" {
    fn setInterval(closure: &wasm_bindgen::prelude::Closure<dyn Fn()>, time: u32) -> f64;
    fn clearInterval(token: f64);
}

#[derive(Clone)]
pub enum ConnectionState {
    Connect(ManualFutureState),
    Handshake(ManualFutureState),
    Process(UpdatableActionStorage),
}

pub struct CommunicationClient {
    _client: Option<Rc<RefCell<PollingClient>>>,
    _state: Rc<RefCell<ConnectionState>>,
    _token: Option<f64>,
    _protocol_kind: HubProtocolKind,
}

impl Clone for CommunicationClient {
    fn clone(&self) -> Self {
        if self._client.is_some() {
            let count = Rc::strong_count(self._client.as_ref().unwrap());

            info!("Cloning communication client {} times", count + 1);
        } else {
            info!("Cloning empty communication client");
        }
        Self { _client: self._client.clone(), _state: self._state.clone(), _token: self._token.clone(), _protocol_kind: self._protocol_kind }
    }
}

impl Drop for CommunicationClient {
    fn drop(&mut self) {
        self.disconnect_internal();
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

    async fn send<T: serde::Serialize>(&mut self, data: T) -> Result<(), String> {
        let res = self.send_internal(data);

        CompletedFuture::new(res).await
    }

    async fn send_binary(&mut self, data: Vec<u8>) -> Result<(), String> {
        let res = self.send_binary_internal(data);

        CompletedFuture::new(res).await
    }

    fn get_protocol_kind(&self) -> HubProtocolKind {
        self._protocol_kind
    }

    fn get_storage(&self) -> Result<UpdatableActionStorage, String> {
        let procstate: ConnectionState;

        {
            let st = self._state.borrow();
            procstate = st.clone();
            drop(st);
        }

        if let ConnectionState::Process(storage) = procstate {
            Ok(storage)
        } else {
            Err(format!("The connection is in a bad state"))
        }
    }

    async fn disconnect(&mut self) {
        let task = CompletedFuture::new(());
        task.await;

        self.disconnect_internal();
    }    
}

impl CommunicationClient {
    fn create(configuration: &super::ConnectionData) -> Self {
        info!("Creating communication client to {}", &configuration.get_endpoint());
        let res = PollingClient::new(&configuration.get_endpoint());
        let protocol_kind = configuration.get_protocol_kind();

        if res.is_ok() {
            CommunicationClient {
                _state: Rc::new(RefCell::new(ConnectionState::Connect(ManualFutureState::new()))),
                _client: Some(Rc::new(RefCell::new(res.unwrap()))),
                _token: None,
                _protocol_kind: protocol_kind,
            }
        } else {
            CommunicationClient {
                _state: Rc::new(RefCell::new(ConnectionState::Connect(ManualFutureState::new()))),
                _client: None,
                _token: None,
                _protocol_kind: protocol_kind,
            }
        }
    }

    async fn connect_internal(&mut self) -> Result<(), String> {
        let connstate: ConnectionState;
        {
            let st = self._state.borrow_mut();
            connstate = st.clone();
            drop(st);
        }

        if let ConnectionState::Connect(mut connected) = connstate {
            if self._client.is_some() {
                let refclient = self._client.as_ref().unwrap().clone();
                let refstate = self._state.clone();
                let protocol_kind = self._protocol_kind;

                let closure = wasm_bindgen::prelude::Closure::wrap(Box::new(move || {
                    CommunicationClient::polling_loop(&refclient, &refstate, protocol_kind);
                }) as Box<dyn Fn()>);
        
                info!("Starting poll loop");
                let token = setInterval(&closure, 100);
                closure.forget();
        
                info!("Waiting for uplink...");
                connected.awaiter().await;
                self._token = Some(token);
    
                info!("Initiating handshake...");
                let r = self.send(HandshakeRequest::new(self._protocol_kind.protocol_name().to_string())).await;
    
                if r.is_err() {
                    return Err(format!("Handshake cannot be sent. {}", r.unwrap_err()));
                }
    
                let mut state = self._state.borrow_mut(); 
                *state = ConnectionState::Handshake(ManualFutureState::new());    
            } else {
                return Err(format!("Connection client is not created properly. Connection has failed"));
            }
        }

        let handstate: ConnectionState;

        {
            let st = self._state.borrow_mut();
            handstate = st.clone();
            drop(st);
        }
        
        if let ConnectionState::Handshake(mut handshake) = handstate {
            let shook = handshake.awaiter().await;

            if shook {
                let mut state = self._state.borrow_mut(); 
                *state = ConnectionState::Process(UpdatableActionStorage::new());
            } else {
                return Err("Unsuccessfull handshake".to_string());
            }
        }

        Ok(())
    }

    fn get_text_messages(message: wasm_sockets::Message) -> Vec<String> {
        match message {
            wasm_sockets::Message::Text(txt) => {
                txt.split(RECORD_SEPARATOR).map(|s| MessageParser::strip_record_separator(s).to_string()).collect()
            },
            wasm_sockets::Message::Binary(_) => {
                warn!("Received binary message in text mode, ignoring");
                Vec::new()
            },
        }
    }

    #[cfg(feature = "messagepack")]
    fn get_binary_messages(message: wasm_sockets::Message) -> Vec<Vec<u8>> {
        match message {
            wasm_sockets::Message::Binary(data) => {
                match crate::protocol::msgpack::split_framed_messages(&data) {
                    Ok(frames) => frames,
                    Err(e) => {
                        error!("Failed to split binary frames: {}", e);
                        Vec::new()
                    }
                }
            },
            wasm_sockets::Message::Text(_) => {
                warn!("Received text message in binary mode, ignoring");
                Vec::new()
            },
        }
    }

    fn send_internal<T: serde::Serialize>(&self, data: T) -> Result<(), String> {
        let json = MessageParser::to_json(&data).unwrap();

        if self._client.is_some() {
            let bclient = self._client.as_ref().unwrap().borrow();
            return bclient.send_string(&json).map_err(|e| e.as_string().unwrap());
        } else {
            return Err(format!("The client is not connected. Cannot send data"));
        }
    }

    fn send_binary_internal(&self, data: Vec<u8>) -> Result<(), String> {
        if self._client.is_some() {
            let bclient = self._client.as_ref().unwrap().borrow();
            return bclient.send_binary(&data).map_err(|e| e.as_string().unwrap());
        } else {
            return Err(format!("The client is not connected. Cannot send data"));
        }
    }

    fn polling_loop(client: &Rc<RefCell<wasm_sockets::PollingClient>>, state: &Rc<RefCell<ConnectionState>>, protocol_kind: HubProtocolKind) {
        let status = client.borrow().status();

        if status == ConnectionStatus::Connected {
            let mstate = &mut *state.borrow_mut();

            match mstate {
                ConnectionState::Connect(connected) => {
                    connected.complete(true);
                },
                ConnectionState::Handshake(handshake) => {
                    // Handshake is always JSON text, even for MessagePack
                    let messages = CommunicationClient::receive_text_messages(client);

                    if messages.len() == 1 {
                        let hs = MessageParser::parse_message::<HandshakeResponse>(messages.first().unwrap());

                        if hs.is_ok() {
                            handshake.complete(true);
                        } else {
                            handshake.complete(false);
                        }
                    }
                },
                ConnectionState::Process(storage) => {
                    match protocol_kind {
                        HubProtocolKind::Json => {
                            let messages = CommunicationClient::receive_text_messages(client);

                            for message in messages {
                                let ping = MessageParser::parse_message::<Ping>(&message);

                                if ping.is_ok() {
                                    let r = storage.process_message(MessagePayload::Text(message), ping.unwrap().message_type());

                                    if r.is_err() {
                                        error!("Message could not be processed: {}", r.unwrap_err());
                                    }
                                } else {
                                    error!("Message could not be parsed: {:?}", message);
                                }
                            }
                        },
                        #[cfg(feature = "messagepack")]
                        HubProtocolKind::MessagePack => {
                            let payloads = CommunicationClient::receive_binary_messages(client);

                            for payload in payloads {
                                match crate::protocol::msgpack::read_message_type(&payload) {
                                    Ok(msg_type) => {
                                        let r = storage.process_message(MessagePayload::Binary(payload), msg_type);
                                        if r.is_err() {
                                            error!("Error processing msgpack message: {}", r.unwrap_err());
                                        }
                                    },
                                    Err(e) => error!("Cannot read msgpack message type: {}", e),
                                }
                            }
                        },
                    }
                },
            }
        } else if status == ConnectionStatus::Connecting {
            info!("Hub is connecting");
        } else if status == ConnectionStatus::Disconnected {
            warn!("Hub is NOT connected at endpoint {}", client.borrow().url);
        } else if status == ConnectionStatus::Error {
            error!("Hub error at endpoint {}", client.borrow().url);
        }
    }

    fn receive_text_messages(client: &Rc<RefCell<wasm_sockets::PollingClient>>) -> Vec<String> {
        let response = client.borrow_mut().receive();
        let mut ret = Vec::new();

        for msg in response {
            for message in CommunicationClient::get_text_messages(msg).into_iter() {
                if message.len() > 0 {
                    ret.push(message);
                }
            }
        }

        ret
    }

    #[cfg(feature = "messagepack")]
    fn receive_binary_messages(client: &Rc<RefCell<wasm_sockets::PollingClient>>) -> Vec<Vec<u8>> {
        let response = client.borrow_mut().receive();
        let mut ret = Vec::new();

        for msg in response {
            for payload in CommunicationClient::get_binary_messages(msg).into_iter() {
                if !payload.is_empty() {
                    ret.push(payload);
                }
            }
        }

        ret
    }

    fn disconnect_internal(&mut self) {
        if self._token.is_some() {
            if self._client.is_some() {
                let refc = self._client.as_ref().unwrap();
                let count = Rc::strong_count(refc);

                if count == 2 {
                    info!("Breaking message loop, destroying clients...");
                    let token = self._token.take().unwrap();
    
                    clearInterval(token);
                } else {
                    info!("Connection cannot be destroyed, has still {} references", count);
                }
            } else {
                info!("Connection is already disconnected");
            }
        } else {
            info!("Message loop is presumably stopped already");
        }
    }
}