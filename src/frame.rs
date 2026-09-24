//! The Server-Sent Events envelope, and what a broken frame is.
//!
//! Each event is an `event:` line naming it and a `data:` line carrying a JSON
//! object whose `event_type` repeats the name, so the `data:` payload alone is
//! enough to decode. The stream ends with `event: done` / `data: [DONE]`, a
//! sentinel the reference does not list (measured 2026-09-24).

/// The payload of a `data:` line, or `None` for any other line.
///
/// Whatever the HTTP client yields, feed it here line by line.
pub fn data_payload(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("data:")?;
    Some(rest.strip_prefix(' ').unwrap_or(rest).trim_end_matches(['\r', '\n']))
}

/// The payload of the stream's closing sentinel.
pub const DONE: &str = "[DONE]";

/// A frame that contradicts the schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The payload is not JSON.
    NotJson(String),
    /// The payload is JSON but not an object.
    NotObject,
    /// The object has no string `event_type`.
    NoEventType,
    /// A known event whose fields have the wrong shape.
    Malformed {
        /// Which event.
        event_type: String,
        /// What was wrong.
        detail: String,
    },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::NotJson(e) => write!(f, "frame is not JSON: {e}"),
            FrameError::NotObject => f.write_str("frame is not a JSON object"),
            FrameError::NoEventType => f.write_str("frame has no `event_type`"),
            FrameError::Malformed { event_type, detail } => write!(f, "malformed `{event_type}` frame: {detail}"),
        }
    }
}

impl std::error::Error for FrameError {}
