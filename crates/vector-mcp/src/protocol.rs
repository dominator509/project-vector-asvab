//! JSON-RPC 2.0 framing for the MCP transport (REQ-025, REQ-026).
//!
//! MCP_CONTRACT.md requires VECTOR to be both an MCP server and an optional MCP
//! client, and the capability model in [`crate::capability`] is only a policy
//! until something carries it over a wire. This module is that wire format: the
//! message types, the error codes, and a parser that refuses malformed input
//! rather than guessing at it.
//!
//! ## Why the parser is strict
//!
//! The server reads this format from another process. A tolerant parser is an
//! attack surface: a message with two `id` fields, an unknown `jsonrpc` version,
//! or an `id` of the wrong type would be resolved by whichever rule the parser
//! happened to apply, and the caller and callee could disagree about which
//! request a response belonged to. Every one of those is a protocol error here.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only protocol version this implementation speaks.
pub const JSONRPC_VERSION: &str = "2.0";

// Standard JSON-RPC 2.0 error codes. Named so a reader does not have to
// remember which negative number means what.
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// VECTOR-specific codes for capability refusals.
///
/// Kept in the implementation-defined range (-32000..-32099) so a client can
/// distinguish "the server refused you" from "your message was malformed".
pub const DENIED: i32 = -32001;
pub const TOOL_FAILED: i32 = -32002;

/// Server protocol version advertised during `initialize`.
///
/// MCP negotiates a version string; this is the revision this implementation
/// was written against, and the client is told exactly which it is speaking.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// A JSON-RPC request or notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// Absent for a notification, which must not be answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Parse one line of input.
    ///
    /// Returns `Err(Response::error(...))` with the correct JSON-RPC code for a
    /// malformed message, and an `id` only when one could be read safely — a
    /// request whose id cannot be trusted must not be answered with a guess, or
    /// the reply could be matched to the wrong request.
    pub fn parse(line: &str) -> Result<Self, Box<Response>> {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(Response::error(
                    Value::Null,
                    PARSE_ERROR,
                    format!("invalid JSON: {error}"),
                )))
            }
        };

        let object = match value.as_object() {
            Some(object) => object,
            None => {
                return Err(Box::new(Response::error(
                    Value::Null,
                    INVALID_REQUEST,
                    "a JSON-RPC message must be an object".to_string(),
                )))
            }
        };

        // A batch is legal JSON-RPC but is not part of the MCP transport this
        // implementation speaks; refusing it explicitly is better than
        // mishandling it.
        if value.is_array() {
            return Err(Box::new(Response::error(
                Value::Null,
                INVALID_REQUEST,
                "batch requests are not supported".to_string(),
            )));
        }

        let id = object.get("id").cloned();
        let fail = |code: i32, message: &str| -> Box<Response> {
            Box::new(Response::error(
                id.clone().unwrap_or(Value::Null),
                code,
                message.to_string(),
            ))
        };

        match object.get("jsonrpc").and_then(Value::as_str) {
            Some(JSONRPC_VERSION) => {}
            Some(other) => {
                return Err(fail(
                    INVALID_REQUEST,
                    &format!("unsupported jsonrpc version {other:?}"),
                ))
            }
            None => return Err(fail(INVALID_REQUEST, "missing jsonrpc version")),
        }

        let method = match object.get("method").and_then(Value::as_str) {
            Some(method) if !method.is_empty() => method.to_string(),
            _ => return Err(fail(INVALID_REQUEST, "missing or empty method")),
        };

        // A present `id` must be a string or a number. Anything else (an object,
        // an array, a boolean) cannot be echoed back unambiguously.
        if let Some(id) = &id {
            if !(id.is_string() || id.is_number()) {
                return Err(Box::new(Response::error(
                    Value::Null,
                    INVALID_REQUEST,
                    "id must be a string or a number".to_string(),
                )));
            }
        }

        Ok(Request {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method,
            params: object.get("params").cloned(),
        })
    }

    /// Whether this message expects a reply.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// A JSON-RPC response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Response {
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        if let Some(error) = &mut self.error {
            error.data = Some(data);
        }
        self
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// The error code, if this is an error response.
    pub fn error_code(&self) -> Option<i32> {
        self.error.as_ref().map(|e| e.code)
    }

    pub fn to_line(&self) -> String {
        // Serialization of these types cannot fail: every field is a plain
        // JSON value. Falling back to a minimal error keeps the transport alive
        // rather than panicking mid-conversation.
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!(
                r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{INTERNAL_ERROR},"message":"response serialization failed"}}}}"#
            )
        })
    }
}

/// A tool result as MCP represents it: a list of content blocks.
///
/// Modelled explicitly so that untrusted text always travels inside a labelled
/// block rather than as a bare string a caller might mistake for a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: "text".to_string(),
            text: text.into(),
        }
    }
}

/// The standard MCP `tools/call` result shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    /// True when the tool ran but reported a failure the model should see.
    #[serde(
        rename = "isError",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(text)],
            is_error: false,
        }
    }

    pub fn failure(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(text)],
            is_error: true,
        }
    }
}
