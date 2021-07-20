use std::collections::HashMap;

use eyre::{eyre, Result, WrapErr};
use rmp_serde::Serializer;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zmq::{Context, Socket, SocketType};

/// An RPCQ client
///
/// # Examples
///
/// ## Implementing the `get_version` RPC call to `quilc`
/// ```rust
/// use std::collections::HashMap;
///
/// use serde::Deserialize;
///
/// use rpcq::{Client, RPCRequest};
/// use qcs_util::Configuration;
///
/// #[derive(Deserialize, Debug)]
///  struct VersionResult {
///     quilc: String,
///     githash: String,
///  }
///
/// let config = qcs_util::Configuration::default();
///  // This endpoint wants an empty object as params, not null.
///  let params: HashMap<String, String> = HashMap::new();
///  let request = RPCRequest::new("get_version_info", params);
///  let client = Client::new(&config.quilc_url).expect("Could not connect to endpoint");
///  let resp: VersionResult = client.run_request(&request).expect("Failed to talk to quilc");
///  let version_parts: Vec<&str> = resp.quilc.split(".").collect();
///  // We can't guarantee the quilc version, but this has only been tested with major version 1
///  // so we'll just check for that.
///  assert_eq!(version_parts[0], "1");
/// ```
pub struct Client {
    socket: Socket,
}

impl Client {
    /// Construct a new [`Client`] with no authentication configured.
    pub fn new(endpoint: &str) -> Result<Self> {
        let socket = Context::new()
            .socket(SocketType::DEALER)
            .wrap_err("Could not create a socket")?;
        socket
            .connect(endpoint)
            .wrap_err("Could not connect to ZMQ endpoint")?;
        Ok(Self { socket })
    }

    /// Construct a new [`Client`] with authentication.
    pub fn new_with_credentials(endpoint: &str, credentials: &Credentials) -> Result<Self> {
        let socket = Context::new()
            .socket(SocketType::DEALER)
            .wrap_err("Could not create a socket")?;
        socket
            .set_curve_publickey(credentials.client_public_key.as_bytes())
            .wrap_err("Could not set public key")?;
        socket
            .set_curve_secretkey(credentials.client_secret_key.as_bytes())
            .wrap_err("Could not set private key")?;
        socket
            .set_curve_serverkey(credentials.server_public_key.as_bytes())
            .wrap_err("Could not set server public key")?;
        socket
            .connect(endpoint)
            .wrap_err("Could not connect to ZMQ endpoint")?;
        Ok(Self { socket })
    }

    /// Send an RPC request and immediately retrieve and decode the results.
    ///
    /// # Arguments
    ///
    /// * `request`: An [`RPCRequest`] containing some params.
    pub fn run_request<Request: Serialize, Response: DeserializeOwned>(
        &self,
        request: &RPCRequest<Request>,
    ) -> Result<Response> {
        self.send(request).wrap_err("Could not send request")?;
        self.receive::<Response>(&request.id)
    }

    /// Send an RPC request.
    ///
    /// # Arguments
    ///
    /// * `request`: An [`RPCRequest`] containing some params.
    pub fn send<Request: Serialize>(&self, request: &RPCRequest<Request>) -> Result<()> {
        let mut data = vec![];
        request
            .serialize(&mut Serializer::new(&mut data).with_struct_map())
            .wrap_err("Could not serialize request as MessagePack")?;

        self.socket
            .send(data, 0)
            .wrap_err("Could not send request to ZMQ server")
    }

    /// Retrieve and decode a response
    ///
    /// returns: Result<Response, Error> where Response is a generic type that implements
    /// [`DeserializeOwned`] (meaning [`Deserialize`] with no lifetimes).
    fn receive<Response: DeserializeOwned>(&self, request_id: &str) -> Result<Response> {
        let data = self.receive_raw()?;

        let reply: RPCResponse<Response> = rmp_serde::from_read(data.as_slice())
            .wrap_err("Could not decode ZMQ server's response")?;
        match reply {
            RPCResponse::RPCReply { id, result } => {
                if id == request_id {
                    Ok(result)
                } else {
                    Err(eyre!("Response ID did not match request ID"))
                }
            }
            RPCResponse::RPCError { error, .. } => {
                Err(eyre!("Received error message from server: {}", error))
            }
        }
    }

    /// Retrieve the raw bytes of a response
    pub fn receive_raw(&self) -> Result<Vec<u8>> {
        self.socket
            .recv_bytes(0)
            .wrap_err("Could not receive data from ZMQ server")
    }
}

/// A single request object according to the JSONRPC standard.
///
/// Construct this using [`RPCRequest::new`]
#[derive(Serialize)]
#[serde(tag = "_type")]
pub struct RPCRequest<T = HashMap<String, String>> {
    method: &'static str,
    params: T,
    id: String,
    jsonrpc: &'static str,
    client_timeout: u8,
    client_key: Option<String>,
}

impl<T> RPCRequest<T> {
    /// Construct a new [`RPCRequest`] to send via [`send_request`]
    ///
    /// # Arguments
    ///
    /// * `method`: The name of the RPC method to call on the server.
    /// * `params`: The parameters to send. This must implement [`serde::Serialize`].
    ///
    /// returns: RPCRequest<T> where T is the type you passed in as `params`.
    ///
    /// # Examples
    ///
    /// See [`send_request`].
    pub fn new(method: &'static str, params: T) -> Self {
        Self {
            method,
            params,
            id: Uuid::new_v4().to_string(),
            jsonrpc: "2.0",
            client_timeout: 10,
            client_key: None,
        }
    }
}

/// Credentials for connecting to RPCQ Server
pub struct Credentials {
    pub client_secret_key: String,
    pub client_public_key: String,
    pub server_public_key: String,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "_type")]
pub enum RPCResponse<T> {
    RPCReply { id: String, result: T },
    RPCError { id: String, error: String },
}
