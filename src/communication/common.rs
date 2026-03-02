use crate::client::{Authentication, ConnectionConfiguration};
use crate::execution::UpdatableActionStorage;
use crate::protocol::hub_protocol::HubProtocolKind;
use crate::protocol::negotiate::NegotiateResponse;
use base64::{engine::general_purpose, Engine};
use serde::Serialize;

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
        let authentication = options.get_authentication();
        let json_text = HttpClient::post_text(negotiate_endpoint.clone(), authentication.clone()).await;

        match json_text {
            Ok(text) => {
                let negotiate = NegotiateResponse::from_json(&text)
                    .map_err(|e| format!("Failed to parse negotiate response: {e}"))?;

                HttpClient::create_configuration(options.get_socket_url(), negotiate, protocol_kind, &authentication)
                    .ok_or_else(|| format!(
                        "The negotiation concluded no matching communication protocols for {:?} transfer format",
                        protocol_kind.transfer_format()
                    ))
            }
            Err(e) => Err(format!("HTTP negotiation with endpoint {} failed {}", negotiate_endpoint, e)),
        }
    }

    fn create_configuration(endpoint: String, negotiate: NegotiateResponse, protocol_kind: HubProtocolKind, authentication: &Authentication) -> Option<ConnectionData> {
        let required_format = protocol_kind.transfer_format();
        let fit = negotiate
            .available_transports()
            .iter()
            .find(|i| i.transport == WEB_SOCKET_TRANSPORT)
            .and_then(|i| {
                i.transfer_formats
                    .iter()
                    .find(|j| j.as_str() == required_format)
            })
            .is_some();

        if fit {
            let mut full_endpoint = format!("{}{}", endpoint, negotiate.endpoint_query());

            // Browsers cannot set custom headers on WebSocket connections.
            // Append the bearer token as a query parameter per the SignalR convention.
            if let Authentication::Bearer { ref token } = authentication {
                let separator = if full_endpoint.contains('?') { "&" } else { "?" };
                full_endpoint = format!("{}{}access_token={}", full_endpoint, separator, token);
            }

            Some(ConnectionData {
                endpoint: full_endpoint,
                connection_id: negotiate.connection_id().to_string(),
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

    pub async fn post_text(endpoint: String, authentication: Authentication) -> Result<String, String> {
        let (s, r) = futures::channel::oneshot::channel::<Result<String, String>>();

        let mut request = ehttp::Request::post(endpoint, vec![]);

        match authentication {
            Authentication::None => {},
            Authentication::Basic { ref user, ref password } => {
                request.headers.insert("Authorization", HttpClient::basic_auth(user.clone(), password.clone()));
            },
            Authentication::Bearer { ref token } => {
                request.headers.insert("Authorization", format!("Bearer {}", token));
            },
        }

        ehttp::fetch(request, move |result| {
            match result {
                Ok(response) => {
                    if let Some(text) = response.text() {
                        _ = s.send(Ok(text.to_string()));
                    } else {
                        _ = s.send(Err("The returned response has no text body".to_string()));
                    }
                }
                Err(e) => {
                    _ = s.send(Err(format!("The call failed: {e}")));
                }
            }
        });

        r.await.unwrap_or(Err("The request is cancelled.".to_string()))
    }

}