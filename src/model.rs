//! One type per model, carrying only the parameters that model accepts.
//!
//! Models differ in which thinking levels they take — Gemini 3.8 Flash answers
//! `minimal` with a 400 that Gemini 3.6 Flash accepts — and in what they can
//! output. So each model is its own type, and a parameter a model refuses is
//! not a field of it. Adding a model means adding a type, never widening one.

mod gemini_3_8_flash;

pub use gemini_3_8_flash::{Gemini3_8Flash, Gemini3_8FlashThinking};

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

/// A requested `max_output_tokens` above what the model can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLimitExceeded {
    /// What was asked for.
    pub requested: u32,
    /// The model's documented output limit.
    pub limit: u32,
}

impl std::fmt::Display for OutputLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "max_output_tokens {} is outside 1..={}", self.requested, self.limit)
    }
}

impl std::error::Error for OutputLimitExceeded {}

/// The shape a text answer must take.
#[derive(Debug, Clone, PartialEq)]
pub enum TextFormat {
    /// Free text.
    Plain,
    /// A JSON value matching this JSON Schema.
    Json(Map<String, Value>),
}

impl Serialize for TextFormat {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("type", "text")?;
        match self {
            TextFormat::Plain => map.serialize_entry("mime_type", "text/plain")?,
            TextFormat::Json(schema) => {
                map.serialize_entry("mime_type", "application/json")?;
                map.serialize_entry("schema", schema)?;
            }
        }
        map.end()
    }
}

/// The model a request is sent to, with its own parameters.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Model {
    /// Gemini 3.8 Flash.
    Gemini3_8Flash(Gemini3_8Flash),
}

impl Model {
    /// Gemini 3.8 Flash with its documented defaults.
    pub fn gemini_3_8_flash() -> Self {
        Model::Gemini3_8Flash(Gemini3_8Flash::new())
    }

    /// The model id sent as `model`.
    pub fn id(&self) -> &'static str {
        match self {
            Model::Gemini3_8Flash(_) => Gemini3_8Flash::ID,
        }
    }

    pub(crate) fn write_generation_config<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        match self {
            Model::Gemini3_8Flash(m) => m.write_generation_config(map),
        }
    }

    pub(crate) fn response_format(&self) -> Option<&TextFormat> {
        match self {
            Model::Gemini3_8Flash(m) => m.response_format.as_ref(),
        }
    }
}

impl From<Gemini3_8Flash> for Model {
    fn from(m: Gemini3_8Flash) -> Self {
        Model::Gemini3_8Flash(m)
    }
}
