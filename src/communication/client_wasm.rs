use std::{cell::RefCell, rc::Rc};

use log::{error, info, warn};
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_sockets::{ConnectionStatus, PollingClient};

use crate::{completer::CompletedFuture, 
    execution::
        {ManualFutureState, Storage, UpdatableActionStorage}, protocol::{messages::{MessageParser, RECORD_SEPARATOR}, negotiate::{HandshakeRequest, HandshakeResponse, Ping}}};

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
}

impl Clone for CommunicationClient {
    fn clone(&self) -> Self {
        if self._client.is_some() {
            let count = Rc::strong_count(self._client.as_ref().unwrap());

            info!("Cloning communication client {} times", count + 1);
        } else {
            info!("Cloning empty communication client");
        }
        Self { _client: self._client.clone(), _state: self._state.clone(), _token: self._token.clone() }
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

    fn disconnect(&mut self) {
        self.disconnect_internal();
    }    
}

impl CommunicationClient {
    fn create(configuration: &super::ConnectionData) -> Self {
        info!("Creating communication client to {}", &configuration.get_endpoint());
        let res = PollingClient::new(&configuration.get_endpoint());

        if res.is_ok() {
            CommunicationClient {
                _state: Rc::new(RefCell::new(ConnectionState::Connect(ManualFutureState::new()))),
                _client: Some(Rc::new(RefCell::new(res.unwrap()))),
                _token: None,
            }    
        } else {
            CommunicationClient {
                _state: Rc::new(RefCell::new(ConnectionState::Connect(ManualFutureState::new()))),
                _client: None,
                _token: None,
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
        
                let closure = wasm_bindgen::prelude::Closure::wrap(Box::new(move || {
                    CommunicationClient::polling_loop(&refclient, &refstate);
                }) as Box<dyn Fn()>);
        
                info!("Starting poll loop");
                let token = setInterval(&closure, 100);
                closure.forget();
        
                info!("Waiting for uplink...");
                connected.awaiter().await;
                self._token = Some(token);
    
                info!("Initiating handshake...");
                let r = self.send(HandshakeRequest::new("json".to_string())).await;
    
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

    fn get_messages(message: wasm_sockets::Message) -> Vec<String> {
        match message {
            wasm_sockets::Message::Text(txt) => {
                txt.split(RECORD_SEPARATOR).map(|s| MessageParser::strip_record_separator(s).to_string()).collect()
            },
            wasm_sockets::Message::Binary(_) => {
                panic!("Binary message is not supported");
            },
        }
    }

    fn send_internal<T: serde::Serialize>(&self, data: T) -> Result<(), String> {
        let json = MessageParser::to_json(&data).unwrap();
        // debug!("CLIENT invocation json: {}", json);

        // debug!("CLIENT is borrowing polling wasm client");

        if self._client.is_some() {
            let bclient = self._client.as_ref().unwrap().borrow();
            return bclient.send_string(&json).map_err(|e| e.as_string().unwrap());    
        } else {
            return Err(format!("The client is not connected. Cannot send data"));
        }
    }

    fn polling_loop(client: &Rc<RefCell<wasm_sockets::PollingClient>>, state: &Rc<RefCell<ConnectionState>>) {
        let status = client.borrow().status();
        
        if status == ConnectionStatus::Connected {
            let mstate = &mut *state.borrow_mut();

            match mstate {
                ConnectionState::Connect(connected) => {
                    connected.complete(true);
                },
                ConnectionState::Handshake(handshake) => {
                    let messages = CommunicationClient::receive_messages(client);

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
                    let messages = CommunicationClient::receive_messages(client);

                    for message in messages {
                        let ping = MessageParser::parse_message::<Ping>(&message);

                        if ping.is_ok() {
                            let r = storage.process_message(message, ping.unwrap().message_type());

                            if r.is_err() {
                                error!("Message could not be processed: {}", r.unwrap_err());
                            }
                        } else {
                            error!("Message could not be parsed: {:?}", message);
                        }
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

    fn receive_messages(client: &Rc<RefCell<wasm_sockets::PollingClient>>) -> Vec<String> {
        let response = client.borrow_mut().receive();
        let mut ret = Vec::new();

        for msg in response {
            for message in CommunicationClient::get_messages(msg).into_iter() {
                if message.len() > 0 {
                    ret.push(message);
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