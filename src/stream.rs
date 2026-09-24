//! One streamed frame becomes one typed event.
//!
//! A stream is a timeline of steps: each step opens with `step.start`, grows
//! by `step.delta`, and closes with `step.stop`, and the interaction as a whole
//! is bracketed by `interaction.created` and `interaction.completed`. The
//! twenty-odd documented delta kinds are fewer shapes than names — most
//! server-tool deltas carry the finished payload of their step — so the
//! variants of [`Delta`] are the shapes.
//!
//! Google may add events and delta kinds; an unknown one decodes as
//! `Unrecognized` rather than failing the frame.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::error::ApiError;
use crate::frame::{DONE, FrameError};
use crate::usage::Usage;
use crate::values::Status;

/// The interaction as a lifecycle event reports it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Snapshot {
    /// Where it stands.
    pub status: Status,
    /// Usage so far, on `interaction.completed`.
    #[serde(default)]
    pub usage: Option<Usage>,
    /// The model id, verbatim.
    #[serde(default)]
    pub model: Option<String>,
    /// The tier that served it, verbatim.
    #[serde(default)]
    pub service_tier: Option<String>,
    /// Its id; empty for unstored interactions.
    #[serde(default)]
    pub id: Option<String>,
}

/// How one step grows.
#[derive(Debug, Clone, PartialEq)]
pub enum Delta {
    /// More model text, appended to the step's last text block.
    Text(String),
    /// Citations for the step's last text block.
    TextAnnotations(Vec<Value>),
    /// A thought's signature.
    ThoughtSignature(String),
    /// A new block of a thought's summary.
    ThoughtSummary(Value),
    /// More of a function call's arguments, as JSON text.
    Arguments(String),
    /// A whole media block (image, audio, document or video) of model output.
    Media(Value),
    /// The payload of a server-side tool step, merged into it field by field.
    Payload(Map<String, Value>),
    /// A delta kind newer than this crate, verbatim.
    Unrecognized(Value),
}

/// One frame of a stream.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// The interaction exists.
    Created(Snapshot),
    /// The interaction's status changed.
    StatusUpdate(Status),
    /// A step begins; the object is its opening form.
    StepStart {
        /// Its position in the turn.
        index: u32,
        /// Its opening fields, at least `type`.
        step: Map<String, Value>,
    },
    /// A step grows.
    StepDelta {
        /// Which step.
        index: u32,
        /// How.
        delta: Delta,
        /// Cumulative usage, when the frame carries it.
        usage: Option<Usage>,
    },
    /// A step is complete.
    StepStop {
        /// Which step.
        index: u32,
        /// Cumulative usage, when the frame carries it.
        usage: Option<Usage>,
    },
    /// The interaction reached a final status. Not the end of the bytes: the
    /// [`Done`](StreamEvent::Done) sentinel may follow.
    Completed(Snapshot),
    /// The API failed the interaction mid-stream.
    Error(ApiError),
    /// The closing `[DONE]` sentinel.
    Done,
    /// An event newer than this crate.
    Unrecognized {
        /// Its `event_type`.
        event_type: String,
    },
}

fn malformed(event_type: &str, e: impl std::fmt::Display) -> FrameError {
    FrameError::Malformed { event_type: event_type.to_owned(), detail: e.to_string() }
}

impl Delta {
    fn from_value(value: Value) -> Result<Self, String> {
        let Value::Object(mut map) = value else { return Err("delta is not an object".into()) };
        let kind = map.get("type").and_then(Value::as_str).ok_or("delta has no `type`")?.to_owned();
        let mut string = |key: &str| match map.remove(key) {
            Some(Value::String(s)) => Ok(s),
            _ => Err(format!("`{kind}` delta has no string `{key}`")),
        };
        Ok(match kind.as_str() {
            "text" => Delta::Text(string("text")?),
            "thought_signature" => Delta::ThoughtSignature(string("signature")?),
            "arguments_delta" => Delta::Arguments(string("arguments")?),
            "text_annotation_delta" => match map.remove("annotations") {
                Some(Value::Array(items)) => Delta::TextAnnotations(items),
                None | Some(Value::Null) => Delta::TextAnnotations(Vec::new()),
                Some(_) => return Err("`annotations` is not an array".into()),
            },
            "thought_summary" => Delta::ThoughtSummary(map.remove("content").ok_or("no `content`")?),
            "image" | "audio" | "document" | "video" => Delta::Media(Value::Object(map)),
            "code_execution_call"
            | "code_execution_result"
            | "file_search_call"
            | "file_search_result"
            | "function_result"
            | "google_maps_call"
            | "google_maps_result"
            | "google_search_call"
            | "google_search_result"
            | "mcp_server_tool_call"
            | "mcp_server_tool_result"
            | "processing_call"
            | "processing_result"
            | "url_context_call"
            | "url_context_result" => {
                map.remove("type");
                Delta::Payload(map)
            }
            _ => Delta::Unrecognized(Value::Object(map)),
        })
    }
}

impl StreamEvent {
    /// Decode the payload of one `data:` line.
    pub fn decode(payload: &str) -> Result<Self, FrameError> {
        if payload.trim() == DONE {
            return Ok(StreamEvent::Done);
        }
        Self::from_value(serde_json::from_str(payload).map_err(|e| FrameError::NotJson(e.to_string()))?)
    }

    /// Decode one frame an SSE parser has already read as JSON.
    pub fn from_value(value: Value) -> Result<Self, FrameError> {
        let Value::Object(mut map) = value else { return Err(FrameError::NotObject) };
        let event_type = map.get("event_type").and_then(Value::as_str).ok_or(FrameError::NoEventType)?.to_owned();
        let et = event_type.as_str();
        let index = |map: &Map<String, Value>| {
            map.get("index")
                .and_then(Value::as_u64)
                .and_then(|i| u32::try_from(i).ok())
                .ok_or_else(|| malformed(et, "no integer `index`"))
        };
        let usage = |map: &mut Map<String, Value>, key: &str| -> Result<Option<Usage>, FrameError> {
            match map.remove(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => serde_json::from_value(v).map(Some).map_err(|e| malformed(et, e)),
            }
        };
        let snapshot = |map: &mut Map<String, Value>| -> Result<Snapshot, FrameError> {
            let v = map.remove("interaction").ok_or_else(|| malformed(et, "no `interaction`"))?;
            serde_json::from_value(v).map_err(|e| malformed(et, e))
        };
        Ok(match et {
            "interaction.created" => StreamEvent::Created(snapshot(&mut map)?),
            "interaction.completed" => StreamEvent::Completed(snapshot(&mut map)?),
            "interaction.status_update" => {
                let v = map.remove("status").ok_or_else(|| malformed(et, "no `status`"))?;
                StreamEvent::StatusUpdate(serde_json::from_value(v).map_err(|e| malformed(et, e))?)
            }
            "step.start" => {
                let index = index(&map)?;
                match map.remove("step") {
                    Some(Value::Object(step)) if step.get("type").is_some_and(Value::is_string) => {
                        StreamEvent::StepStart { index, step }
                    }
                    _ => return Err(malformed(et, "no `step` object with a `type`")),
                }
            }
            "step.delta" => {
                let index = index(&map)?;
                let delta = map.remove("delta").ok_or_else(|| malformed(et, "no `delta`"))?;
                let delta = Delta::from_value(delta).map_err(|e| malformed(et, e))?;
                let usage = match map.remove("metadata") {
                    Some(Value::Object(mut metadata)) => usage(&mut metadata, "total_usage")?,
                    _ => None,
                };
                StreamEvent::StepDelta { index, delta, usage }
            }
            "step.stop" => {
                let index = index(&map)?;
                StreamEvent::StepStop { index, usage: usage(&mut map, "usage")? }
            }
            "error" => {
                let v = map.remove("error").ok_or_else(|| malformed(et, "no `error`"))?;
                StreamEvent::Error(serde_json::from_value(v).map_err(|e| malformed(et, e))?)
            }
            _ => StreamEvent::Unrecognized { event_type },
        })
    }
}
