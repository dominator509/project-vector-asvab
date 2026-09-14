//! The MCP server: capability-checked tools and resources over JSON-RPC.
//!
//! The capability model in [`crate::capability`] decides what a peer may do; this
//! module is where that decision is actually enforced, because a policy that is
//! never consulted on the request path is not a control.
//!
//! ## Design
//!
//! [`McpServer::handle`] is a pure function of the registry, the data source and
//! one input line, so every rule below is testable without spawning a process.
//! The stdio loop that drives it lives in the tool that hosts it.
//!
//! ## What is enforced per call
//!
//! 1. The peer must be allowlisted and consented, and must hold the capability
//!    the tool declares ([`McpRegistry::authorize`]).
//! 2. Arguments must be within the tool's byte bound.
//! 3. A path argument must fall inside the peer's scoped roots
//!    ([`McpRegistry::check_path`]).
//! 4. The arguments are treated as *untrusted*: a tool result is returned inside
//!    a labelled data block, and any delimiter markers in it are neutralized, so
//!    a peer cannot use a tool result to forge instructions
//!    ([`crate::capability::isolate_context`]).
//! 5. Every attempt, allowed or denied, is audited.

use serde_json::{json, Value};

use crate::capability::{
    isolate_context, render_isolated, AuditOutcome, Capability, ContentTrust, ContextItem,
    McpRegistry, PeerGrant,
};
use crate::protocol::{
    CallToolResult, Request, Response, DENIED, INTERNAL_ERROR, INVALID_PARAMS,
    MCP_PROTOCOL_VERSION, METHOD_NOT_FOUND, TOOL_FAILED,
};

/// Read-only study data the server exposes to authorized peers.
///
/// A trait rather than a direct dependency on the storage crate: the MCP layer
/// must not be able to reach anything it has not been handed, and the host
/// decides what that is. Every method returns already-serialized text, so this
/// layer cannot accidentally leak a richer object than intended.
pub trait StudyData {
    fn study_plan(&self, learner_id: &str) -> Result<String, String>;
    fn evidence(&self) -> Result<String, String>;
    fn content_metadata(&self) -> Result<String, String>;
    fn diagnostics(&self) -> Result<String, String>;
}

/// A data source that has nothing. Used when the host has no database, so a
/// probe cannot silently succeed against fabricated content.
pub struct NoStudyData;

impl StudyData for NoStudyData {
    fn study_plan(&self, _learner_id: &str) -> Result<String, String> {
        Err("no study database is attached to this server".to_string())
    }
    fn evidence(&self) -> Result<String, String> {
        Err("no study database is attached to this server".to_string())
    }
    fn content_metadata(&self) -> Result<String, String> {
        Err("no study database is attached to this server".to_string())
    }
    fn diagnostics(&self) -> Result<String, String> {
        Err("no study database is attached to this server".to_string())
    }
}

/// The MCP server bound to a registry and a data source.
pub struct McpServer<'a> {
    registry: McpRegistry,
    data: &'a dyn StudyData,
    /// Identity of the peer currently connected.
    ///
    /// The stdio transport is one peer per process, so the identity is fixed at
    /// construction: a `tools/call` cannot claim to be someone else, because
    /// there is no field in the request that could carry an identity.
    peer_id: String,
    initialized: bool,
    /// Projected root the server advertises as writable, if any.
    writable_root: Option<String>,
}

impl<'a> McpServer<'a> {
    pub fn new(peers: Vec<PeerGrant>, peer_id: &str, data: &'a dyn StudyData) -> Self {
        Self {
            registry: McpRegistry::new(peers),
            data,
            peer_id: peer_id.to_string(),
            initialized: false,
            writable_root: None,
        }
    }

    /// Advertise a writable root for the `create_draft_artifact` tool.
    pub fn with_writable_root(mut self, root: &str) -> Self {
        self.writable_root = Some(root.to_string());
        self
    }

    pub fn audit_log(&self) -> &[crate::capability::AuditRecord] {
        self.registry.audit_log()
    }

    /// Handle one input line. `None` means "no reply", which happens for a
    /// notification.
    pub fn handle(&mut self, line: &str) -> Option<Response> {
        let request = match Request::parse(line) {
            Ok(request) => request,
            Err(response) => {
                // A parse failure is answered with a null id, as JSON-RPC
                // requires when the id cannot be recovered.
                return Some(*response);
            }
        };

        if request.is_notification() {
            // `notifications/initialized` is the only notification this server
            // acts on, and acting on it produces no reply.
            if request.method == "notifications/initialized" {
                self.initialized = true;
            }
            return None;
        }

        let id = request.id.clone().unwrap_or(Value::Null);
        Some(self.dispatch(&request.method, request.params.as_ref(), id))
    }

    fn dispatch(&mut self, method: &str, params: Option<&Value>, id: Value) -> Response {
        // Everything except `initialize` requires an initialized session, so a
        // client cannot skip negotiation and call tools straight away.
        if method != "initialize" && method != "ping" && !self.initialized {
            return Response::error(
                id,
                INVALID_PARAMS,
                "initialize must be called before any other method",
            );
        }

        match method {
            "initialize" => Response::success(
                id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "serverInfo": { "name": "vector", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "resources": { "subscribe": false, "listChanged": false },
                    },
                }),
            ),

            "ping" => Response::success(id, json!({})),

            "tools/list" => {
                let tools: Vec<Value> = self
                    .registry
                    .tools()
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "inputSchema": {
                                "type": "object",
                                // MCP_CONTRACT.md: typed arguments, never generic
                                // filesystem/process/network access.
                                "additionalProperties": false,
                            },
                            "x-vector-required-capability": format!("{:?}", tool.required_capability),
                            "x-vector-max-argument-bytes": tool.max_argument_bytes,
                        })
                    })
                    .collect();
                Response::success(id, json!({ "tools": tools }))
            }

            "resources/list" => Response::success(
                id,
                json!({
                    "resources": [
                        { "uri": "vector://evidence", "name": "Evidence vault", "mimeType": "application/json" },
                        { "uri": "vector://content", "name": "Content pack metadata", "mimeType": "application/json" },
                        { "uri": "vector://diagnostics", "name": "Local diagnostics (redacted)", "mimeType": "application/json" },
                    ]
                }),
            ),

            "resources/read" => {
                let uri = params
                    .and_then(|p| p.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.read_resource(&uri, id)
            }

            "tools/call" => {
                let name = params
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let arguments = params
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                self.call_tool(&name, &arguments, id)
            }

            other => Response::error(id, METHOD_NOT_FOUND, format!("unknown method {other:?}")),
        }
    }

    fn read_resource(&mut self, uri: &str, id: Value) -> Response {
        // Reading a resource is a tool-equivalent action, so it goes through the
        // same authorization path rather than around it. `list_evidence` is used
        // as the gate because every registered resource exposes the same class
        // of data — approved, read-only study material — and all of the read
        // tools declare the identical `ReadStudyData` capability. Naming one of
        // them explicitly means the gate cannot drift away from the tool set.
        const READ_GATE_TOOL: &str = "list_evidence";

        if let Err(reason) =
            self.registry
                .authorize(&self.peer_id, READ_GATE_TOOL, 0, ContentTrust::PeerProvided)
        {
            return Response::error(id, DENIED, reason.to_string());
        }

        let body = match self.data_for(uri) {
            Ok(Some(body)) => body,
            Ok(None) => format!("no resource at {uri}"),
            Err(message) => return Response::error(id, TOOL_FAILED, message),
        };

        // The resource body is isolated like any other untrusted content.
        let isolated = isolate_context(&[ContextItem {
            origin: uri.to_string(),
            trust: ContentTrust::PeerProvided,
            text: body,
        }]);

        Response::success(
            id,
            json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": render_isolated(&isolated),
                }]
            }),
        )
    }

    /// The body for a resource URI, or an error message naming the reason.
    fn data_for(&self, uri: &str) -> Result<Option<String>, String> {
        match uri {
            "vector://evidence" => self.data.evidence().map(Some),
            "vector://content" => self.data.content_metadata().map(Some),
            "vector://diagnostics" => self.data.diagnostics().map(Some),
            _ => Ok(None),
        }
    }

    fn call_tool(&mut self, name: &str, arguments: &Value, id: Value) -> Response {
        let serialized = serde_json::to_string(arguments).unwrap_or_default();
        let trust = ContentTrust::PeerProvided;

        // Authorization first: nothing about the arguments is inspected until
        // the caller has been shown to be allowed to call this tool at all.
        let authorized = match self
            .registry
            .authorize(&self.peer_id, name, serialized.len(), trust)
        {
            Ok(call) => call,
            Err(reason) => {
                return Response::error(id, DENIED, reason.to_string()).with_data(json!({
                    "tool": name,
                    "peer": self.peer_id,
                }))
            }
        };

        // A path argument is checked against the peer's roots before anything
        // touches it.
        if let Some(path) = arguments.get("path").and_then(Value::as_str) {
            if !matches!(
                authorized.tool.required_capability,
                Capability::CreateDraft
                    | Capability::ExportRepairBundle
                    | Capability::InvokeWritingAgent
                    | Capability::GeneratePracticeSet
            ) {
                return Response::error(id, DENIED, "this tool does not accept a path argument");
            }
            if let Err(reason) = self.registry.check_path(&self.peer_id, path) {
                return Response::error(id, DENIED, reason.to_string());
            }
        }

        let outcome = self.run_tool(name, arguments);
        let correlation = authorized.audit.correlation_id;

        match outcome {
            Ok(text) => {
                let value = self.render_result(&text, false, &correlation);
                Response::success(id, value)
            }
            Err(message) => {
                // A failure travels the same route as a success. Its message can
                // embed peer-supplied text — an unknown tool name comes straight
                // from the arguments — so it is isolated and labelled rather
                // than returned as a bare string a caller might read as the
                // server's own instruction.
                let value = self.render_result(&message, true, &correlation);
                Response::success(id, value)
            }
        }
    }

    /// Wrap a tool result as an isolated data block with its audit metadata.
    fn render_result(&self, text: &str, is_error: bool, correlation: &str) -> Value {
        let isolated = isolate_context(&[ContextItem {
            origin: format!("tool-result:{}", self.peer_id),
            trust: ContentTrust::PeerProvided,
            text: text.to_string(),
        }]);
        let rendered = render_isolated(&isolated);
        let result = if is_error {
            CallToolResult::failure(rendered)
        } else {
            CallToolResult::text(rendered)
        };
        let mut value = serde_json::to_value(result).unwrap_or_else(|_| json!({}));
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "_meta".to_string(),
                json!({ "correlationId": correlation, "peer": self.peer_id }),
            );
        }
        value
    }

    fn run_tool(&self, name: &str, arguments: &Value) -> Result<String, String> {
        let learner = arguments
            .get("learner_id")
            .and_then(Value::as_str)
            .unwrap_or("default");

        match name {
            "get_study_plan" => self.data.study_plan(learner),
            "list_evidence" => self.data.evidence(),
            "get_content_metadata" => self.data.content_metadata(),
            "get_diagnostics" => self.data.diagnostics(),

            // Mutating tools: this build has no approved write path, so they
            // report that rather than pretending to have done something. A tool
            // that silently did nothing would be worse than one that refuses.
            "create_draft_artifact" => match &self.writable_root {
                Some(root) => Err(format!(
                    "draft creation is not enabled in this build; \
                     the granted root {root} would be used by the review workflow"
                )),
                None => Err("no writable root is granted in this build".to_string()),
            },
            "generate_practice_set" => {
                Err("practice-set generation requires an approved model transport".to_string())
            }
            "export_repair_bundle" => {
                Err("repair-bundle export requires an approved crash record".to_string())
            }
            "invoke_writing_agent" => Err("no writing agent is granted to this peer".to_string()),

            other => Err(format!("unknown tool {other:?}")),
        }
    }

    /// The audit trail, for a host that wants to persist or display it.
    pub fn audit(&self) -> Vec<Value> {
        self.registry
            .audit_log()
            .iter()
            .map(|record| {
                json!({
                    "correlationId": record.correlation_id,
                    "peer": record.peer_id,
                    "tool": record.tool,
                    "outcome": match record.outcome {
                        AuditOutcome::Allowed => "allowed",
                        AuditOutcome::Denied => "denied",
                    },
                    "argumentTrust": format!("{:?}", record.argument_trust),
                    "detail": record.detail,
                })
            })
            .collect()
    }
}

/// Convenience: is this a code the client should treat as a capability refusal?
pub fn is_denial(response: &Response) -> bool {
    response.error_code() == Some(DENIED)
}

/// Convenience: build an internal-error response for a host-level failure.
pub fn internal_error(message: impl Into<String>) -> Response {
    Response::error(Value::Null, INTERNAL_ERROR, message)
}
