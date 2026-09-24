//! Steps: the history an interaction is made of.
//!
//! A stateless conversation is its steps, sent back in full every turn. Two
//! kinds of step go into it and they obey different rules:
//!
//! * A step the **caller** writes — [`UserInput`] or a [`FunctionResult`] — is
//!   a value the crate serializes from typed fields.
//! * A step the **model** wrote — thoughts, function calls, output, server-side
//!   tool calls and their results — must be sent back exactly as received.
//!   Dropping a thought step is answered with a 400 (measured 2026-09-24), and
//!   its signature is opaque, so the crate cannot rebuild one. A [`ModelStep`]
//!   therefore keeps the JSON object it arrived as and serializes *that*; its
//!   typed [`StepView`] is for reading only. A model step is only ever
//!   *decoded* — by [`crate::settle`], by [`crate::response`], or by
//!   [`ModelStep::from_wire`] from a copy the caller stored — and it has no
//!   setter, so what was received is what is sent.
//!
//! ```compile_fail
//! // Its fields are private: a decoded step cannot be edited in place.
//! let _ = google::step::ModelStep { wire: Default::default(), view: todo!() };
//! ```

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::content::{FunctionOutput, InputContent, OutputContent};
use crate::values::api_enum;

// ── Caller-authored steps ────────────────────────────────────────────────────

/// What the user says: one `user_input` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInput {
    /// The blocks, in order.
    pub content: Vec<InputContent>,
}

impl Serialize for UserInput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(2))?;
        map.serialize_entry("type", "user_input")?;
        map.serialize_entry("content", &self.content)?;
        map.end()
    }
}

/// The result of running one function the model called.
///
/// Built by [`crate::conversation::Conversation::push_function_result`], which
/// takes `call_id` and `name` from the call it answers, so the two cannot
/// disagree with the call.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionResult {
    call_id: String,
    name: String,
    /// What the function returned.
    pub output: FunctionOutput,
    /// Whether the function failed; the model reads `output` as an error message.
    pub is_error: bool,
}

impl FunctionResult {
    pub(crate) fn new(call: &FunctionCall, output: FunctionOutput, is_error: bool) -> Self {
        Self { call_id: call.id.clone(), name: call.name.clone(), output, is_error }
    }

    /// The id of the call this answers.
    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    /// The name of the function that ran.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Serialize for FunctionResult {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(5))?;
        map.serialize_entry("type", "function_result")?;
        map.serialize_entry("call_id", &self.call_id)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("result", &self.output)?;
        map.serialize_entry("is_error", &self.is_error)?;
        map.end()
    }
}

// ── Model-authored steps ─────────────────────────────────────────────────────

/// A function call the model made and the caller must answer.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    /// The id a [`FunctionResult`] answers with.
    pub id: String,
    /// Which declared function.
    pub name: String,
    /// The arguments, as a JSON object.
    pub arguments: Map<String, Value>,
}

api_enum! { open
    /// Which server-side tool step a [`StepView::Server`] is.
    ServerStepType / KnownServerStepType {
        /// Code the model runs with the code-execution tool.
        CodeExecutionCall => "code_execution_call",
        /// What that code printed.
        CodeExecutionResult => "code_execution_result",
        /// A file-search query.
        FileSearchCall => "file_search_call",
        /// What file search found.
        FileSearchResult => "file_search_result",
        /// A Google Maps query.
        GoogleMapsCall => "google_maps_call",
        /// What Google Maps found.
        GoogleMapsResult => "google_maps_result",
        /// A Google Search query.
        GoogleSearchCall => "google_search_call",
        /// What Google Search found.
        GoogleSearchResult => "google_search_result",
        /// A call to a tool of a remote MCP server.
        McpServerToolCall => "mcp_server_tool_call",
        /// What the MCP server returned.
        McpServerToolResult => "mcp_server_tool_result",
        /// Server-initiated media analysis.
        ProcessingCall => "processing_call",
        /// Its result.
        ProcessingResult => "processing_result",
        /// A retrieval query.
        RetrievalCall => "retrieval_call",
        /// Its result.
        RetrievalResult => "retrieval_result",
        /// A URL fetch.
        UrlContextCall => "url_context_call",
        /// What the fetch returned.
        UrlContextResult => "url_context_result",
    }
}

/// What a model step says, for reading.
#[derive(Debug, Clone, PartialEq)]
pub enum StepView {
    /// Reasoning: an opaque signature and, when requested, a summary.
    Thought {
        /// The encrypted reasoning state; absent only if the API sent none.
        signature: Option<String>,
        /// Readable summary blocks, when `thinking_summaries` asked for them.
        summary: Vec<OutputContent>,
    },
    /// A function the caller must run.
    FunctionCall(FunctionCall),
    /// What the model says.
    ModelOutput {
        /// The blocks, in order.
        content: Vec<OutputContent>,
    },
    /// A server-side tool call or result, which the server has already run.
    Server {
        /// Which one; the payload is in [`ModelStep::wire`].
        step_type: ServerStepType,
    },
}

/// A step the model produced, replayed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelStep {
    wire: Map<String, Value>,
    view: StepView,
}

/// A model step whose JSON contradicts the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepError(pub String);

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed step: {}", self.0)
    }
}

impl std::error::Error for StepError {}

impl ModelStep {
    /// Decode one step object, as the API sent it or as the caller stored it.
    ///
    /// Unknown step types are server steps: the `Unrecognized`
    /// [`ServerStepType`] keeps them replayable. A caller-authored type
    /// (`user_input`, `function_result`) is refused, because replaying it as the
    /// model's would put words in the model's mouth.
    pub fn from_wire(wire: Map<String, Value>) -> Result<Self, StepError> {
        let bad = |e: serde_json::Error| StepError(e.to_string());
        let step_type = wire.get("type").and_then(Value::as_str).ok_or_else(|| StepError("no `type`".into()))?;
        let blocks = |key: &str| -> Result<Vec<OutputContent>, StepError> {
            match wire.get(key) {
                None | Some(Value::Null) => Ok(Vec::new()),
                Some(Value::Array(items)) => items.iter().map(|v| OutputContent::from_value(v).map_err(bad)).collect(),
                Some(_) => Err(StepError(format!("`{key}` is not an array"))),
            }
        };
        let view = match step_type {
            "thought" => {
                let signature = match wire.get("signature") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(_) => return Err(StepError("`signature` is not a string".into())),
                };
                StepView::Thought { signature, summary: blocks("summary")? }
            }
            "function_call" => {
                #[derive(Deserialize)]
                struct Wire {
                    id: String,
                    name: String,
                    #[serde(default)]
                    arguments: Map<String, Value>,
                }
                let w = Wire::deserialize(Value::Object(wire.clone())).map_err(bad)?;
                StepView::FunctionCall(FunctionCall { id: w.id, name: w.name, arguments: w.arguments })
            }
            "model_output" => StepView::ModelOutput { content: blocks("content")? },
            "user_input" | "function_result" => {
                return Err(StepError(format!("`{step_type}` is the caller's step, not the model's")));
            }
            other => StepView::Server {
                step_type: KnownServerStepType::from_wire(other)
                    .map(ServerStepType::Known)
                    .unwrap_or_else(|| ServerStepType::Unrecognized(other.to_owned())),
            },
        };
        Ok(Self { wire, view })
    }

    /// What the step says.
    pub fn view(&self) -> &StepView {
        &self.view
    }

    /// The JSON object exactly as it will be sent back.
    pub fn wire(&self) -> &Map<String, Value> {
        &self.wire
    }

    /// The function call, if this step is one.
    pub fn as_function_call(&self) -> Option<&FunctionCall> {
        match &self.view {
            StepView::FunctionCall(call) => Some(call),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ModelStep {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ModelStep::from_wire(Map::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ModelStep {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(s)
    }
}

/// One entry of a conversation's history.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// The user speaks.
    UserInput(UserInput),
    /// The caller answers a function call.
    FunctionResult(FunctionResult),
    /// The model's own step, verbatim.
    Model(ModelStep),
}

impl Serialize for Step {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Step::UserInput(step) => step.serialize(s),
            Step::FunctionResult(step) => step.serialize(s),
            Step::Model(step) => step.serialize(s),
        }
    }
}
