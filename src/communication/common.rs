use crate::client::{Authentication, ConnectionConfiguration};
use crate::execution::UpdatableActionStorage;
use crate::protocol::hub_protocol::HubProtocolKind;
use crate::protocol::negotiate::NegotiateResponseV0;
use base64::{engine::general_purpose, Engine};
use serde::{de::DeserializeOwned, Serialize};

const WEB_SOCKET_TRANSPORT: &str = "WebSockets";

#[derive(Clone, Debug)]
pub struct ConnectionData {
    endpoint: String,
    connection_id: String,
    protocol_kind: HubProtocolKind,
}

impl ConnectionData {
    pub fn get_endpoint(&self) -> String {
        self.endpoint.clone()
    }

    #[allow(dead_code)]
    pub fn get_connection_id(&self) -> String {
        self.connection_id.clone()
    }

    pub fn get_protocol_kind(&self) -> HubProtocolKind {
        self.protocol_kind
    }
}

pub trait Communication : Clone {
    async fn connect(configuration: &ConnectionData) -> Result<Self, String>;
    async fn send<T: Serialize>(&mut self, data: T) -> Result<(), String>;
    async fn send_binary(&mut self, data: Vec<u8>) -> Result<(), String>;
    fn get_storage(&self) -> Result<UpdatableActionStorage, String>;
    fn get_protocol_kind(&self) -> HubProtocolKind;
    async fn disconnect(&mut self);
}

pub struct HttpClient {
    
}

impl HttpClient {
    pub(crate) async fn negotiate(options: ConnectionConfiguration) -> Result<ConnectionData, String> {
        let negotiate_endpoint = format!("{}/negotiate?negotiateVersion=1", options.get_web_url());
        let protocol_kind = options.get_protocol_kind();
        let negotiation = HttpClient::post::<NegotiateResponseV0>(negotiate_endpoint.clone(), options.get_authentication()).await;

        if negotiation.is_ok() {
            let configuration = HttpClient::create_configuration(options.get_socket_url(), negotiation.unwrap(), protocol_kind);

            if configuration.is_some() {
                Ok(configuration.unwrap())
            } else {
                Err(format!("The negotiation concluded no matching communication protocols for {:?} transfer format", protocol_kind.transfer_format()))
            }
        } else {
            Err(format!("HTTP negotiation with endpoint {} failed {}", negotiate_endpoint, negotiation.unwrap_err().to_string()))
        }
    }

    fn create_configuration(endpoint: String, negotiate: NegotiateResponseV0, protocol_kind: HubProtocolKind) -> Option<ConnectionData> {
        let required_format = protocol_kind.transfer_format();
        let fit = negotiate
            .available_transports
            .iter()
            .find(|i| i.transport == WEB_SOCKET_TRANSPORT)
            .and_then(|i| {
                i.transfer_formats
                    .iter()
                    .find(|j| j.as_str() == required_format)
            })
            .is_some();

        if fit {
            Some(ConnectionData {
                endpoint,
                connection_id: negotiate.connection_id,
                protocol_kind,
            })
        } else {
            None
        }
    }

    fn basic_auth(username: String, password: Option<String>) -> String        
    {
        let mut ret = String::new();

        if password.is_some() {
            general_purpose::STANDARD.encode_string(format!("{}:{}", username, password.unwrap()), &mut ret);
        } else {
            general_purpose::STANDARD.encode_string(format!("{}:", username), &mut ret);
        }
        
        format!("Basic {}", &ret)
    }

    pub async fn post<T: 'static + DeserializeOwned + Send>(endpoint: String, authentication: Authentication) -> Result<T, String> {
        let (s, r) = futures::channel::oneshot::channel::<Result<T, String>>();

        let mut request = ehttp::Request::post(endpoint, vec![]);

        match authentication {
            Authentication::None => {},
            Authentication::Basic { user, password } => {
                request.headers.insert("Authorization", HttpClient::basic_auth(user, password));
            },
            Authentication::Bearer { token } => {
                request.headers.insert("Authorization", format!("Bearer {}", token));
            },
        }

        ehttp::fetch(request, move |result| {
            if result.is_ok() {
                let response = result.ok();

                if response.is_some() {
                    let json = response.unwrap();

                    if let Some(text) = json.text() {
                        let nr = serde_json::from_str::<T>(text);

                        if nr.is_ok() {
                            _ = s.send(Result::Ok(nr.unwrap()));
                        } else {
                            _ = s.send(Result::<T, String>::Err(format!("The HTTP response is failed to deserialize: {:?}, {}", nr.err(), text)));
                        }
                    } else {
                        _ = s.send(Result::<T, String>::Err("The returned json is empty".to_string()));
                    }
                } else {
                    _ = s.send(Result::<T, String>::Err("The response is empty.".to_string()));
                }
            } else {
                _ = s.send(Result::<T, String>::Err(format!("The call failed {:?}", result.err())));
            }
        });

        let result = r.await;

        if result.is_ok() {
            result.unwrap()
        } else {
            Result::<T, String>::Err("The request is cancelled.".to_string())
        }
    }
}