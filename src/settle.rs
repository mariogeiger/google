//! A stream becomes a finished turn, or does not.
//!
//! [`Settling`] accumulates events and cannot yield a turn; [`Turn`] is
//! finished and cannot take more events; [`Settling::settle`] is the only
//! bridge, and it fails on a stream that never reached a final status. So a
//! stream cut off halfway is not readable as an answer.
//!
//! Each step is rebuilt into the same JSON object the buffered endpoint
//! returns for it, which is what replay sends back: a thought gets its
//! `signature` and `summary`, a function call its `arguments` object parsed
//! from the concatenated argument text, model output its `content` blocks, and
//! a server-side tool step the fields of its payload delta.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::ApiError;
use crate::frame::{FrameError, data_payload};
use crate::step::{ModelStep, StepError};
use crate::stream::{Delta, Snapshot, StreamEvent};
use crate::turn::Turn;
use crate::usage::Usage;
use crate::values::KnownStatus;

/// Why a stream did not become a turn.
#[derive(Debug, Clone, PartialEq)]
pub enum SettleError {
    /// A frame contradicts the schema.
    Frame(FrameError),
    /// The API failed the interaction.
    Api(ApiError),
    /// An event names a step that has not started, or already stopped.
    UnknownStep(u32),
    /// A step started twice.
    DuplicateStep(u32),
    /// A function call's concatenated arguments are not a JSON object.
    Arguments {
        /// Which step.
        index: u32,
        /// What the parser said.
        detail: String,
    },
    /// A finished step contradicts the schema.
    Step(StepError),
    /// `interaction.completed` reported a status that is not final.
    NotFinal(String),
    /// The stream ended before `interaction.completed`.
    Truncated,
    /// Events arrived after the interaction completed.
    AfterCompletion,
}

impl std::fmt::Display for SettleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettleError::Frame(e) => e.fmt(f),
            SettleError::Api(e) => write!(f, "interaction failed: {e}"),
            SettleError::UnknownStep(i) => write!(f, "event for step {i}, which is not open"),
            SettleError::DuplicateStep(i) => write!(f, "step {i} started twice"),
            SettleError::Arguments { index, detail } => write!(f, "step {index} arguments: {detail}"),
            SettleError::Step(e) => e.fmt(f),
            SettleError::NotFinal(s) => write!(f, "interaction completed with non-final status `{s}`"),
            SettleError::Truncated => f.write_str("stream ended before interaction.completed"),
            SettleError::AfterCompletion => f.write_str("event after interaction.completed"),
        }
    }
}

impl std::error::Error for SettleError {}

impl From<FrameError> for SettleError {
    fn from(e: FrameError) -> Self {
        SettleError::Frame(e)
    }
}

#[derive(Debug, Default)]
struct Open {
    wire: Map<String, Value>,
    arguments: String,
}

impl Open {
    fn blocks(&mut self, key: &str) -> &mut Vec<Value> {
        let slot = self.wire.entry(key.to_owned()).or_insert_with(|| Value::Array(Vec::new()));
        if !slot.is_array() {
            *slot = Value::Array(Vec::new());
        }
        slot.as_array_mut().expect("just made an array")
    }

    fn last_text(&mut self) -> Option<&mut Map<String, Value>> {
        match self.blocks("content").last_mut() {
            Some(Value::Object(block)) if block.get("type").and_then(Value::as_str) == Some("text") => Some(block),
            _ => None,
        }
    }

    fn apply(&mut self, delta: Delta) {
        match delta {
            Delta::Text(text) => match self.last_text() {
                Some(block) => {
                    let joined = format!("{}{text}", block.get("text").and_then(Value::as_str).unwrap_or(""));
                    block.insert("text".into(), Value::String(joined));
                }
                None => {
                    let mut block = Map::new();
                    block.insert("type".into(), "text".into());
                    block.insert("text".into(), Value::String(text));
                    self.blocks("content").push(Value::Object(block));
                }
            },
            Delta::TextAnnotations(items) => {
                if let Some(block) = self.last_text() {
                    let slot = block.entry("annotations").or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(list) = slot {
                        list.extend(items);
                    }
                }
            }
            Delta::ThoughtSignature(signature) => {
                self.wire.insert("signature".into(), Value::String(signature));
            }
            Delta::ThoughtSummary(block) => self.blocks("summary").push(block),
            Delta::Arguments(text) => self.arguments.push_str(&text),
            Delta::Media(block) => self.blocks("content").push(block),
            Delta::Payload(fields) => self.wire.extend(fields),
            Delta::Unrecognized(_) => {}
        }
    }

    fn close(mut self, index: u32) -> Result<ModelStep, SettleError> {
        if !self.arguments.is_empty() {
            let arguments: Map<String, Value> = serde_json::from_str(&self.arguments)
                .map_err(|e| SettleError::Arguments { index, detail: e.to_string() })?;
            self.wire.insert("arguments".into(), Value::Object(arguments));
        }
        ModelStep::from_wire(self.wire).map_err(SettleError::Step)
    }
}

/// A stream being accumulated.
#[derive(Debug, Default)]
pub struct Settling {
    open: BTreeMap<u32, Open>,
    closed: BTreeMap<u32, ModelStep>,
    usage: Usage,
    completed: Option<Snapshot>,
}

impl Settling {
    /// An empty accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume one `data:` payload.
    pub fn consume_payload(&mut self, payload: &str) -> Result<(), SettleError> {
        self.consume(StreamEvent::decode(payload)?)
    }

    /// Consume one raw line of the body; lines other than `data:` are ignored.
    pub fn consume_line(&mut self, line: &str) -> Result<(), SettleError> {
        match data_payload(line) {
            Some(payload) => self.consume_payload(payload),
            None => Ok(()),
        }
    }

    /// Consume one decoded event.
    pub fn consume(&mut self, event: StreamEvent) -> Result<(), SettleError> {
        if self.completed.is_some() && !matches!(event, StreamEvent::Done) {
            return Err(SettleError::AfterCompletion);
        }
        match event {
            StreamEvent::Created(_) | StreamEvent::StatusUpdate(_) | StreamEvent::Done => {}
            StreamEvent::Unrecognized { .. } => {}
            StreamEvent::StepStart { index, step } => {
                if self.open.contains_key(&index) || self.closed.contains_key(&index) {
                    return Err(SettleError::DuplicateStep(index));
                }
                self.open.insert(index, Open { wire: step, arguments: String::new() });
            }
            StreamEvent::StepDelta { index, delta, usage } => {
                self.open.get_mut(&index).ok_or(SettleError::UnknownStep(index))?.apply(delta);
                if let Some(usage) = usage {
                    self.usage.join(&usage);
                }
            }
            StreamEvent::StepStop { index, usage } => {
                let open = self.open.remove(&index).ok_or(SettleError::UnknownStep(index))?;
                self.closed.insert(index, open.close(index)?);
                if let Some(usage) = usage {
                    self.usage.join(&usage);
                }
            }
            StreamEvent::Completed(snapshot) => {
                let status = &snapshot.status;
                if *status == KnownStatus::InProgress || *status == KnownStatus::Queued {
                    return Err(SettleError::NotFinal(status.as_str().to_owned()));
                }
                if let Some(usage) = &snapshot.usage {
                    self.usage.join(usage);
                }
                self.completed = Some(snapshot);
            }
            StreamEvent::Error(error) => return Err(SettleError::Api(error)),
        }
        Ok(())
    }

    /// The finished turn, or why there is none.
    pub fn settle(self) -> Result<Turn, SettleError> {
        let snapshot = self.completed.ok_or(SettleError::Truncated)?;
        if let Some(index) = self.open.keys().next() {
            return Err(SettleError::UnknownStep(*index));
        }
        Ok(Turn {
            steps: self.closed.into_values().collect(),
            status: snapshot.status,
            usage: self.usage,
            model: snapshot.model,
            service_tier: snapshot.service_tier,
        })
    }
}
