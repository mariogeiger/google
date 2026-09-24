//! Gemini 3.8 Flash parameters and its documented limits.

use serde::ser::SerializeMap;

use super::{OutputLimitExceeded, TextFormat};
use crate::values::{ThinkingLevel, ThinkingSummaries, api_enum};

api_enum! { closed
    /// The thinking levels Gemini 3.8 Flash accepts.
    ///
    /// `minimal` is not one of them: the API answers it with "'minimal' is not
    /// a supported thinking level for this model" (measured 2026-09-24), so it
    /// is not a value this type has.
    ///
    /// ```compile_fail
    /// let _ = google::model::Gemini3_8FlashThinking::Minimal;
    /// ```
    Gemini3_8FlashThinking {
        /// Low thinking.
        Low => "low",
        /// Medium thinking. The documented default.
        Medium => "medium",
        /// High thinking.
        High => "high",
    }
}

impl From<Gemini3_8FlashThinking> for ThinkingLevel {
    fn from(level: Gemini3_8FlashThinking) -> Self {
        match level {
            Gemini3_8FlashThinking::Low => ThinkingLevel::Low,
            Gemini3_8FlashThinking::Medium => ThinkingLevel::Medium,
            Gemini3_8FlashThinking::High => ThinkingLevel::High,
        }
    }
}

/// Gemini 3.8 Flash's per-call parameters.
///
/// It reads text, images, audio, video and PDF and writes only text, so its
/// response format is a [`TextFormat`] and it has no speech, image or video
/// configuration. Thinking is always on.
#[derive(Debug, Clone, PartialEq)]
pub struct Gemini3_8Flash {
    /// How much it thinks. Always emitted, so the body records it.
    pub thinking: Gemini3_8FlashThinking,
    /// Whether thought steps carry summaries. Always emitted.
    pub summaries: ThinkingSummaries,
    /// The shape of the answer, when constrained.
    pub response_format: Option<TextFormat>,
    max_output_tokens: Option<u32>,
}

impl Default for Gemini3_8Flash {
    fn default() -> Self {
        Self {
            thinking: Gemini3_8FlashThinking::Medium,
            summaries: ThinkingSummaries::None,
            response_format: None,
            max_output_tokens: None,
        }
    }
}

impl Gemini3_8Flash {
    /// The model id.
    pub const ID: &'static str = "gemini-3.8-flash";
    /// The most input tokens it accepts.
    pub const INPUT_TOKEN_LIMIT: u32 = 1_048_576;
    /// The most tokens it can output, thinking included.
    pub const OUTPUT_TOKEN_LIMIT: u32 = 65_536;

    /// The documented defaults: medium thinking, no summaries, no output cap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets how much it thinks.
    pub fn with_thinking(mut self, thinking: Gemini3_8FlashThinking) -> Self {
        self.thinking = thinking;
        self
    }

    /// Sets whether thought summaries are returned.
    pub fn with_summaries(mut self, summaries: ThinkingSummaries) -> Self {
        self.summaries = summaries;
        self
    }

    /// Constrains the answer's shape.
    pub fn with_response_format(mut self, format: TextFormat) -> Self {
        self.response_format = Some(format);
        self
    }

    /// Caps output, thinking included, at `tokens`, which must be within the
    /// model's limit.
    pub fn with_max_output_tokens(mut self, tokens: u32) -> Result<Self, OutputLimitExceeded> {
        if tokens == 0 || tokens > Self::OUTPUT_TOKEN_LIMIT {
            return Err(OutputLimitExceeded { requested: tokens, limit: Self::OUTPUT_TOKEN_LIMIT });
        }
        self.max_output_tokens = Some(tokens);
        Ok(self)
    }

    /// The output cap, if one was set.
    pub fn max_output_tokens(&self) -> Option<u32> {
        self.max_output_tokens
    }

    pub(super) fn write_generation_config<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        map.serialize_entry("thinking_level", &ThinkingLevel::from(self.thinking))?;
        map.serialize_entry("thinking_summaries", &self.summaries)?;
        if let Some(tokens) = self.max_output_tokens {
            map.serialize_entry("max_output_tokens", &tokens)?;
        }
        Ok(())
    }
}
