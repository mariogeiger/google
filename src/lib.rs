//! Gemini Interactions API bindings: the typed wire in both directions.
//!
//! Outbound, a request whose invalid forms do not compile: a type per model
//! carrying only the parameters it accepts, and an append-only conversation
//! whose model steps can only be replayed exactly as received. Inbound, a
//! streaming decoder and a buffered decoder that make a truncated stream
//! unreadable as a finished turn.
//!
//! The crate is stateless by mission: every request sends the whole history
//! with `store: false`. See
//! [`SOUL.md`](https://github.com/mariogeiger/google/blob/main/SOUL.md) for the
//! design rules the whole crate follows.
//!
//! # Outbound
//!
//! * [`content`] — the blocks a caller sends, and a view of the model's.
//! * [`step`] — the history: caller steps, and verbatim model steps.
//! * [`tools`] — tool declarations, and the choice among them.
//! * [`conversation`] — cache-safe, append-only conversation state.
//! * [`model`] — one type per model, carrying only what it accepts.
//! * [`request`] — per-call parameters and the request body.
//!
//! # Inbound
//!
//! * [`frame`] — the Server-Sent Events envelope, and what a broken frame is.
//! * [`stream`] — one streamed frame becomes one typed event.
//! * [`settle`] — a stream becomes a finished turn, or does not.
//! * [`response`] — a buffered response body.
//! * [`turn`] — the finished turn both of those produce.
//! * [`usage`] — what a request cost, and what the cache did.
//! * [`error`] — an error the API reports.
//!
//! # Shared
//!
//! * [`values`] — the enums that mirror API vocabularies, re-exported at the root.
//!
//! # One turn with a function call
//!
//! ```
//! use google::content::FunctionOutput;
//! use google::conversation::Conversation;
//! use google::model::Gemini3_8Flash;
//! use google::request::Request;
//! use google::settle::Settling;
//! use google::tools::{Function, Tool};
//! use serde_json::json;
//!
//! let schema = json!({"type": "object", "properties": {"path": {"type": "string"}}});
//! let tools = vec![Tool::Function(Function::new("read_file", "Read a file.", schema.as_object().unwrap().clone()))];
//! let mut conversation = Conversation::new(Some("Be brief.".into()), tools)?;
//! conversation.push_user_text("What is in notes.txt?")?;
//!
//! let body = serde_json::to_value(Request::new(&conversation, Gemini3_8Flash::new())?.streaming())?;
//! assert_eq!(body["store"], false);
//! assert_eq!(body["generation_config"]["thinking_level"], "medium");
//!
//! // Whatever your HTTP client hands you, line by line.
//! let stream = concat!(
//!     r#"data: {"index":0,"step":{"type":"thought"},"event_type":"step.start"}"#, "\n",
//!     r#"data: {"index":0,"delta":{"signature":"c2ln","type":"thought_signature"},"event_type":"step.delta"}"#, "\n",
//!     r#"data: {"index":0,"event_type":"step.stop"}"#, "\n",
//!     r#"data: {"index":1,"step":{"id":"call_1","type":"function_call","name":"read_file","arguments":{}},"event_type":"step.start"}"#, "\n",
//!     r#"data: {"index":1,"delta":{"arguments":"{\"path\":\"notes.txt\"}","type":"arguments_delta"},"event_type":"step.delta"}"#, "\n",
//!     r#"data: {"index":1,"event_type":"step.stop"}"#, "\n",
//!     r#"data: {"interaction":{"id":"","status":"requires_action"},"event_type":"interaction.completed"}"#, "\n",
//!     "data: [DONE]\n",
//! );
//! let mut settling = Settling::new();
//! for line in stream.lines() {
//!     settling.consume_line(line)?;
//! }
//! // The only way to a finished turn: a stream cut short fails here.
//! let turn = settling.settle()?;
//! let call_id = turn.function_calls().next().unwrap().id.clone();
//!
//! // The thought and the call go back verbatim; the result must follow.
//! conversation.push_turn(turn)?;
//! assert!(Request::new(&conversation, Gemini3_8Flash::new()).is_err());
//! conversation.push_function_result(&call_id, FunctionOutput::Text("buy milk".into()), false)?;
//! let body = serde_json::to_value(Request::new(&conversation, Gemini3_8Flash::new())?)?;
//! assert_eq!(body["input"][1], json!({"type": "thought", "signature": "c2ln"}));
//! assert_eq!(body["input"][3]["call_id"], "call_1");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![deny(missing_docs)]

pub mod content;
pub mod conversation;
pub mod error;
pub mod frame;
pub mod model;
pub mod request;
pub mod response;
pub mod settle;
pub mod step;
pub mod stream;
pub mod tools;
pub mod turn;
pub mod usage;
pub mod values;

pub use values::*;

/// The API's origin.
pub const API_BASE: &str = "https://generativelanguage.googleapis.com";
/// Path of the endpoint that creates an interaction.
pub const INTERACTIONS_PATH: &str = "/v1beta/interactions";
/// Name of the API-key header.
pub const HEADER_API_KEY: &str = "x-goog-api-key";
