//! Cache-safe, append-only conversation state.
//!
//! The Interactions API caches implicitly: a request whose rendered prefix
//! matches an earlier one reads it from cache (measured 2026-09-24: 4,081 of
//! 10,185 input tokens cached on an identical repeat). The prefix is the
//! system instruction, then the tools, then the steps, so a [`Conversation`]
//! fixes the first two at construction and only ever appends to the third.
//! There is no method that edits, removes or reorders history.
//!
//! It also holds the one ordering rule the API states for stateless use: every
//! function call the model made is answered before the model is asked again.

use std::collections::BTreeSet;

use crate::content::{FunctionOutput, InputContent};
use crate::step::{FunctionCall, FunctionResult, Step, UserInput};
use crate::tools::{AllowedTools, Tool};
use crate::turn::Turn;
use crate::values::ToolChoiceMode;

/// A refused change to a conversation, returned before anything is appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationError {
    /// Two tools declare the same name.
    DuplicateToolName(String),
    /// The model called functions that have no result yet; their ids.
    UnansweredCalls(Vec<String>),
    /// No pending call has this id.
    UnknownCall(String),
    /// The history already ends with the model's turn, so another model turn
    /// cannot follow it.
    NotAwaitingModel,
    /// No declared tool has this name.
    UnknownTool(String),
}

impl std::fmt::Display for ConversationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversationError::DuplicateToolName(name) => write!(f, "tool `{name}` is declared twice"),
            ConversationError::UnansweredCalls(ids) => write!(f, "function calls without a result: {}", ids.join(", ")),
            ConversationError::UnknownCall(id) => write!(f, "no pending function call has id `{id}`"),
            ConversationError::NotAwaitingModel => f.write_str("the history already ends with a model turn"),
            ConversationError::UnknownTool(name) => write!(f, "no declared tool is named `{name}`"),
        }
    }
}

impl std::error::Error for ConversationError {}

/// System instruction, tools and history: the part of a request that stays put.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    system_instruction: Option<String>,
    tools: Vec<Tool>,
    steps: Vec<Step>,
    pending: Vec<FunctionCall>,
}

impl Conversation {
    /// A conversation with its system instruction, if any, and its tools, both
    /// fixed for its lifetime.
    pub fn new(system_instruction: Option<String>, tools: Vec<Tool>) -> Result<Self, ConversationError> {
        let mut names = BTreeSet::new();
        for name in tools.iter().filter_map(Tool::name) {
            if !names.insert(name) {
                return Err(ConversationError::DuplicateToolName(name.to_owned()));
            }
        }
        Ok(Self { system_instruction, tools, steps: Vec::new(), pending: Vec::new() })
    }

    /// The system instruction.
    pub fn system_instruction(&self) -> Option<&str> {
        self.system_instruction.as_deref()
    }

    /// The declared tools, in declaration order.
    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// The history, oldest first.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The model's function calls that still need a result, in call order.
    pub fn pending_calls(&self) -> &[FunctionCall] {
        &self.pending
    }

    /// Whether the next step belongs to the model: the history ends with the
    /// caller's step and no call is waiting.
    pub fn awaits_model(&self) -> bool {
        self.pending.is_empty() && matches!(self.steps.last(), Some(Step::UserInput(_) | Step::FunctionResult(_)))
    }

    fn refuse_pending(&self) -> Result<(), ConversationError> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err(ConversationError::UnansweredCalls(self.pending.iter().map(|c| c.id.clone()).collect()))
        }
    }

    /// Append what the user says.
    pub fn push_user(&mut self, content: Vec<InputContent>) -> Result<(), ConversationError> {
        self.refuse_pending()?;
        self.steps.push(Step::UserInput(UserInput { content }));
        Ok(())
    }

    /// Append a user message of plain text.
    pub fn push_user_text(&mut self, text: impl Into<String>) -> Result<(), ConversationError> {
        self.push_user(vec![InputContent::text(text)])
    }

    /// Append the model's turn, every step exactly as received.
    ///
    /// Its function calls become pending until each has a result.
    pub fn push_turn(&mut self, turn: Turn) -> Result<(), ConversationError> {
        if !self.awaits_model() {
            self.refuse_pending()?;
            return Err(ConversationError::NotAwaitingModel);
        }
        for step in turn.steps {
            if let Some(call) = step.as_function_call() {
                self.pending.push(call.clone());
            }
            self.steps.push(Step::Model(step));
        }
        Ok(())
    }

    /// Answer one pending function call.
    pub fn push_function_result(
        &mut self,
        call_id: &str,
        output: FunctionOutput,
        is_error: bool,
    ) -> Result<(), ConversationError> {
        let at = self
            .pending
            .iter()
            .position(|c| c.id == call_id)
            .ok_or_else(|| ConversationError::UnknownCall(call_id.to_owned()))?;
        let call = self.pending.remove(at);
        self.steps.push(Step::FunctionResult(FunctionResult::new(&call, output, is_error)));
        Ok(())
    }

    /// A restriction to named tools of this conversation, checked by name.
    pub fn allow_tools(&self, mode: ToolChoiceMode, names: &[&str]) -> Result<AllowedTools, ConversationError> {
        for name in names {
            if !self.tools.iter().any(|t| t.name() == Some(name)) {
                return Err(ConversationError::UnknownTool((*name).to_owned()));
            }
        }
        Ok(AllowedTools::unchecked(mode, names.iter().map(|n| (*n).to_owned()).collect()))
    }
}
