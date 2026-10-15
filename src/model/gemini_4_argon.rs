//! Gemini 4 Argon parameters and its documented limits.

use serde::ser::SerializeMap;

use super::{OutputLimitExceeded, TextFormat};
use crate::values::{ThinkingLevel, ThinkingSummaries, api_enum};

api_enum! { closed
    /// The thinking levels Gemini 4 Argon accepts.
    ///
    /// `minimal` is not one of them: the API answers it with "Thinking level
    /// is unsupported: THINKING_LEVEL_MINIMAL" (measured 2026-10-15), so it is
    /// not a value this type has.
    ///
    /// ```compile_fail
    /// let _ = google::model::Gemini4ArgonThinking::Minimal;
    /// ```
    Gemini4ArgonThinking {
        /// Low thinking.
        Low => "low",
        /// Medium thinking. The documented default.
        Medium => "medium",
        /// High thinking.
        High => "high",
    }
}

impl From<Gemini4ArgonThinking> for ThinkingLevel {
    fn from(level: Gemini4ArgonThinking) -> Self {
        match level {
            Gemini4ArgonThinking::Low => ThinkingLevel::Low,
            Gemini4ArgonThinking::Medium => ThinkingLevel::Medium,
            Gemini4ArgonThinking::High => ThinkingLevel::High,
        }
    }
}

/// Gemini 4 Argon's per-call parameters.
///
/// It reads text, images, audio, video and PDF and writes only text, so its
/// response format is a [`TextFormat`] and it has no speech, image or video
/// configuration. Thinking is always on. Input, thinking and output share one
/// window of [`Self::INPUT_TOKEN_LIMIT`] tokens.
///
/// Its long decode, past 64K output tokens, needs background execution, which
/// this stateless crate does not send; so its output cap stops at the 64K one
/// response may stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Gemini4Argon {
    /// How much it thinks. Always emitted, so the body records it.
    pub thinking: Gemini4ArgonThinking,
    /// Whether thought steps carry summaries. Always emitted.
    pub summaries: ThinkingSummaries,
    /// The shape of the answer, when constrained.
    pub response_format: Option<TextFormat>,
    max_output_tokens: Option<u32>,
}

impl Default for Gemini4Argon {
    fn default() -> Self {
        Self {
            thinking: Gemini4ArgonThinking::Medium,
            summaries: ThinkingSummaries::None,
            response_format: None,
            max_output_tokens: None,
        }
    }
}

impl Gemini4Argon {
    /// The model id.
    pub const ID: &'static str = "gemini-4-argon";
    /// The one window input, thinking and output share.
    pub const INPUT_TOKEN_LIMIT: u32 = 1_048_576;
    /// The most tokens one streamed response can output, thinking included:
    /// the API answers more with "Long decodes are not supported using the
    /// Interactions API" (measured 2026-10-15).
    pub const OUTPUT_TOKEN_LIMIT: u32 = 65_536;
    /// The most tokens one response that is not streamed can output, thinking
    /// included: the API answers 32,769 with the same refusal (measured
    /// 2026-10-15).
    pub const UNARY_OUTPUT_TOKEN_LIMIT: u32 = 32_768;

    /// The documented defaults: medium thinking, no summaries, no output cap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets how much it thinks.
    pub fn with_thinking(mut self, thinking: Gemini4ArgonThinking) -> Self {
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
    /// streamed limit. A request that is not streamed refuses a cap above
    /// [`Self::UNARY_OUTPUT_TOKEN_LIMIT`] when it is written.
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
