//! Gemini 4 Argon parameters and its documented limits.

use serde::ser::SerializeMap;

use super::{OutputLimitExceeded, TextFormat};
use crate::values::{ThinkingLevel, ThinkingSummaries, api_enum};

api_enum! { closed
    /// The thinking levels Gemini 4 Argon accepts.
    ///
    /// `minimal` and `xhigh` are not among them: the API answers each with
    /// "Thinking level is unsupported: THINKING_LEVEL_MINIMAL" (measured
    /// 2026-10-15 and again 2026-11-18) and "…THINKING_LEVEL_XHIGH" (measured
    /// 2026-11-18), so neither is a value this type has.
    ///
    /// ```compile_fail
    /// let _ = google::model::Gemini4ArgonThinking::Minimal;
    /// ```
    ///
    /// ```compile_fail
    /// let _ = google::model::Gemini4ArgonThinking::Xhigh;
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
/// Its long decode, past what one request returns, goes on in further
/// requests that resume from a `continuation_token`, or in a background
/// interaction; this crate models neither yet, so its output cap stops at
/// what one streamed request returns. A request that is not streamed and sets
/// no cap may still reach the lower limit of a request that is not streamed;
/// the API then ends it with `requires_continuation`, which decoding refuses as
/// not final rather than reading half an answer as a whole one.
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
    /// The most tokens one streamed request can output, thinking included, and
    /// the output limit Google documents for the model. On 2026-10-15 the API
    /// answered a larger cap with "Long decodes are not supported using the
    /// Interactions API"; on 2026-11-18 it accepts one up to 1,048,576 (the
    /// window), and Google's long-decode guide has a request that reaches this
    /// limit with budget left end in `requires_continuation`.
    pub const OUTPUT_TOKEN_LIMIT: u32 = 65_536;
    /// The most tokens one request that is not streamed can output, thinking
    /// included: Google's long-decode guide documents it as the limit of a
    /// request that is not streamed on this model, and the API answered a cap
    /// of 32,769 with the same refusal on 2026-10-15 and with a 200 on
    /// 2026-11-18.
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
