//! A finished model turn: what both the stream and the buffered body become.

use crate::step::{FunctionCall, ModelStep, StepView};
use crate::usage::Usage;
use crate::values::Status;

/// The model's side of one interaction, ready to append to a conversation.
///
/// Only [`crate::settle::Settling::settle`] and
/// [`crate::response::decode_interaction`] make one, and both refuse an
/// interaction that has not reached a final status, so a `Turn` is never half
/// an answer.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Turn {
    /// The steps, in order, each replayable verbatim.
    pub steps: Vec<ModelStep>,
    /// The final status: `completed`, `requires_action`, `incomplete`, …
    pub status: Status,
    /// What it cost.
    pub usage: Usage,
    /// The model id the API reports.
    pub model: Option<String>,
    /// The tier that served it, verbatim.
    pub service_tier: Option<String>,
}

impl Turn {
    /// The text of every model-output block, concatenated in order.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            if let StepView::ModelOutput { content } = step.view() {
                content.iter().filter_map(|c| c.as_text()).for_each(|t| out.push_str(t));
            }
        }
        out
    }

    /// The function calls the caller must answer, in order.
    pub fn function_calls(&self) -> impl Iterator<Item = &FunctionCall> {
        self.steps.iter().filter_map(ModelStep::as_function_call)
    }
}
