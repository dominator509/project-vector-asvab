//! MCP client transport over a child process's stdio (REQ-026).
//!
//! `REQ-026` requires allowlisted peers with consent and audit; the capability
//! model already decides *whether* a peer may call a tool. This module is the
//! other half: an actual connection to an external MCP server, over the stdio
//! transport MCP defines, so "VECTOR is an optional MCP client" is a fact about
//! the program rather than a claim in a document.
//!
//! ## Containment
//!
//! Three things are enforced here rather than trusted to the peer:
//!
//! * **The allowlist is the spawn list.** A server can only be started from an
//!   [`AllowedServer`] entry, so an arbitrary path cannot be executed by
//!   constructing a request. There is no API that takes a bare command.
//! * **Sessions are bounded.** Each request carries a timeout, and a peer that
//!   never answers is terminated rather than left running.
//! * **Responses are validated.** A reply whose `id` does not match, or that
//!   carries neither `result` nor `error`, is a protocol violation and fails the
//!   call instead of being interpreted.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::protocol::{Request, Response, INVALID_REQUEST, JSONRPC_VERSION};

/// A server the user has allowed VECTOR to connect to (REQ-026).
#[derive(Debug, Clone, PartialEq)]
pub struct AllowedServer {
    /// Stable identity used for capability scoping.
    pub peer_id: String,
    /// The exact program to run. There is no shell involved, so no argument in
    /// this struct can be interpreted as a command.
    pub program: String,
    pub args: Vec<String>,
    /// Whether the human consented to this peer being connected.
    pub user_consented: bool,
    /// Environment variables removed before the child is started, so a peer is
    /// not handed credentials it was never granted.
    pub scrub_env: Vec<String>,
}

impl AllowedServer {
    pub fn new(peer_id: &str, program: &str, args: &[&str]) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            user_consented: false,
            scrub_env: Vec::new(),
        }
    }

    pub fn with_consent(mut self, consented: bool) -> Self {
        self.user_consented = consented;
        self
    }

    pub fn scrubbing(mut self, names: &[&str]) -> Self {
        self.scrub_env = names.iter().map(|n| (*n).to_string()).collect();
        self
    }
}

/// Why a connection could not be established or used.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientError {
    /// The peer is not consented, so it was never started.
    NotConsented(String),
    /// The child process could not be spawned.
    SpawnFailed(String),
    /// The peer did not answer within the request's deadline.
    Timeout { method: String, millis: u64 },
    /// The peer closed the connection.
    Disconnected,
    /// The peer sent something that is not a valid JSON-RPC response, or an
    /// answer to a different request.
    ProtocolViolation(String),
    /// The peer returned an error response. Carries the code and message.
    Remote { code: i32, message: String },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotConsented(peer) => {
                write!(f, "peer {peer} has not been consented to")
            }
            ClientError::SpawnFailed(message) => write!(f, "cannot start the peer: {message}"),
            ClientError::Timeout { method, millis } => {
                write!(f, "{method} did not answer within {millis} ms")
            }
            ClientError::Disconnected => write!(f, "the peer closed the connection"),
            ClientError::ProtocolViolation(message) => write!(f, "protocol violation: {message}"),
            ClientError::Remote { code, message } => {
                write!(f, "the peer returned error {code}: {message}")
            }
        }
    }
}

impl std::error::Error for ClientError {}

/// A live MCP session with a child process.
pub struct McpClient {
    server: AllowedServer,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    initialized: bool,
}

impl McpClient {
    /// Start an allowed server and speak MCP to it over stdio.
    ///
    /// Fails rather than prompting when consent is absent: a connection is an
    /// explicit act, and this API has no console to ask on.
    pub fn connect(server: AllowedServer) -> Result<Self, ClientError> {
        if !server.user_consented {
            return Err(ClientError::NotConsented(server.peer_id));
        }

        let mut command = Command::new(&server.program);
        command
            .args(&server.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The peer's stderr is inherited so a failure is visible to whoever
            // is running the check, rather than being swallowed.
            .stderr(Stdio::inherit());
        for name in &server.scrub_env {
            command.env_remove(name);
        }

        let mut child = command
            .spawn()
            .map_err(|e| ClientError::SpawnFailed(format!("{}: {e}", server.program)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ClientError::SpawnFailed("no stdin pipe".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClientError::SpawnFailed("no stdout pipe".to_string()))?;

        Ok(Self {
            server,
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            initialized: false,
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.server.peer_id
    }

    /// Send a request and read its response, ignoring unrelated traffic.
    ///
    /// Responses for other ids are skipped rather than treated as the answer:
    /// accepting the first line that arrives would let a peer satisfy a read
    /// request with the answer to a different one.
    pub fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, ClientError> {
        let id = Value::from(self.next_id);
        self.next_id += 1;

        let request = Request {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Some(id.clone()),
            method: method.to_string(),
            params,
        };
        let line = serde_json::to_string(&request)
            .map_err(|e| ClientError::ProtocolViolation(e.to_string()))?;

        writeln!(self.stdin, "{line}").map_err(|_| ClientError::Disconnected)?;
        self.stdin.flush().map_err(|_| ClientError::Disconnected)?;

        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                self.terminate();
                return Err(ClientError::Timeout {
                    method: method.to_string(),
                    millis: timeout.as_millis() as u64,
                });
            }

            let mut buffer = String::new();
            let read = self
                .stdout
                .read_line(&mut buffer)
                .map_err(|_| ClientError::Disconnected)?;
            if read == 0 {
                return Err(ClientError::Disconnected);
            }
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                ClientError::ProtocolViolation(format!("response is not JSON: {e}"))
            })?;

            let response: Response = serde_json::from_value(value).map_err(|e| {
                ClientError::ProtocolViolation(format!("response is not a JSON-RPC response: {e}"))
            })?;

            if response.jsonrpc != JSONRPC_VERSION {
                return Err(ClientError::ProtocolViolation(format!(
                    "response declares jsonrpc {:?}",
                    response.jsonrpc
                )));
            }
            // A different request's answer is not this request's answer.
            if response.id != id {
                continue;
            }
            if response.result.is_none() && response.error.is_none() {
                return Err(ClientError::ProtocolViolation(
                    "response carries neither result nor error".to_string(),
                ));
            }

            if let Some(error) = response.error {
                return Err(ClientError::Remote {
                    code: error.code,
                    message: error.message,
                });
            }
            return Ok(response.result.unwrap_or(Value::Null));
        }
    }

    /// Perform the MCP handshake.
    pub fn initialize(&mut self, timeout: Duration) -> Result<Value, ClientError> {
        let result = self.request(
            "initialize",
            Some(json!({
                "protocolVersion": crate::protocol::MCP_PROTOCOL_VERSION,
                "clientInfo": { "name": "vector", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": {},
            })),
            timeout,
        )?;
        // The handshake is only complete once the client says so, which is a
        // notification and therefore has no reply.
        let notification = json!({
            "jsonrpc": JSONRPC_VERSION,
            "method": "notifications/initialized",
        });
        writeln!(self.stdin, "{notification}").map_err(|_| ClientError::Disconnected)?;
        self.stdin.flush().map_err(|_| ClientError::Disconnected)?;
        self.initialized = true;
        Ok(result)
    }

    pub fn list_tools(&mut self, timeout: Duration) -> Result<Value, ClientError> {
        self.request("tools/list", None, timeout)
    }

    pub fn read_resource(&mut self, uri: &str, timeout: Duration) -> Result<Value, ClientError> {
        self.request("resources/read", Some(json!({ "uri": uri })), timeout)
    }

    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, ClientError> {
        self.request(
            "tools/call",
            Some(json!({ "name": name, "arguments": arguments })),
            timeout,
        )
    }

    /// Whether the handshake has completed on this session.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Closing stdin is the protocol's shutdown signal; a peer that does not
        // exit is killed so a failed session cannot leave a stray process.
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A response that is an error with the given code.
pub fn is_error_with_code(error: &ClientError, code: i32) -> bool {
    matches!(error, ClientError::Remote { code: c, .. } if *c == code)
}

/// Whether a line is a valid JSON-RPC *error* response with the given code.
pub fn error_code_of(line: &str) -> Option<i32> {
    let value: Value = serde_json::from_str(line).ok()?;
    let response: Response = serde_json::from_value(value).ok()?;
    if !response.is_error() {
        return None;
    }
    response.error_code()
}

/// Code used by a client when it rejects a peer's message outright.
pub const CLIENT_REJECTED: i32 = INVALID_REQUEST;
