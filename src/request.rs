//! Per-call parameters and the `POST /v1beta/interactions` body.
//!
//! A [`Request`] borrows a [`Conversation`] and adds what may change from one
//! call to the next without touching the cached prefix: the model and its
//! parameters, the tool choice, sampling controls, the service tier, and
//! whether to stream.
//!
//! `store` is always `false`. This crate is stateless by mission: a stored
//! interaction continued with `previous_interaction_id` hands the prefix to
//! the server, where no type can guard it, so neither field exists here.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::conversation::Conversation;
use crate::model::Model;
use crate::tools::ToolChoice;
use crate::values::ServiceTier;

/// A request refused before it is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    /// The history does not end with the caller's step, or a function call is
    /// unanswered, so there is nothing for the model to answer.
    NotAwaitingModel,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::NotAwaitingModel => {
                f.write_str("the conversation does not end with a user input or a complete set of function results")
            }
        }
    }
}

impl std::error::Error for RequestError {}

/// One call's body.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    conversation: &'a Conversation,
    /// The model and its parameters.
    pub model: Model,
    /// Whether the response is Server-Sent Events.
    pub stream: bool,
    /// Which tools the model may call, when restricted. No documented default,
    /// so absent means the caller said nothing.
    pub tool_choice: Option<ToolChoice>,
    /// A decoding seed, for reproducibility.
    pub seed: Option<i32>,
    /// Strings that end the answer when generated.
    pub stop_sequences: Vec<String>,
    /// Which capacity pool serves the call, when the caller says.
    pub service_tier: Option<ServiceTier>,
}

impl<'a> Request<'a> {
    /// A request for the model's next turn, refused unless the conversation is
    /// waiting for one.
    pub fn new(conversation: &'a Conversation, model: impl Into<Model>) -> Result<Self, RequestError> {
        if !conversation.awaits_model() {
            return Err(RequestError::NotAwaitingModel);
        }
        Ok(Self {
            conversation,
            model: model.into(),
            stream: false,
            tool_choice: None,
            seed: None,
            stop_sequences: Vec::new(),
            service_tier: None,
        })
    }

    /// The same request, streamed.
    pub fn streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    /// The same request with a tool choice.
    pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }

    /// The conversation it sends.
    pub fn conversation(&self) -> &Conversation {
        self.conversation
    }
}

struct GenerationConfig<'r, 'a>(&'r Request<'a>);

impl Serialize for GenerationConfig<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let request = self.0;
        let mut map = s.serialize_map(None)?;
        request.model.write_generation_config(&mut map)?;
        if let Some(seed) = request.seed {
            map.serialize_entry("seed", &seed)?;
        }
        if !request.stop_sequences.is_empty() {
            map.serialize_entry("stop_sequences", &request.stop_sequences)?;
        }
        if let Some(choice) = &request.tool_choice {
            map.serialize_entry("tool_choice", choice)?;
        }
        map.end()
    }
}

impl Serialize for Request<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let conversation = self.conversation;
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("model", self.model.id())?;
        if let Some(instruction) = conversation.system_instruction() {
            map.serialize_entry("system_instruction", instruction)?;
        }
        if !conversation.tools().is_empty() {
            map.serialize_entry("tools", conversation.tools())?;
        }
        map.serialize_entry("input", conversation.steps())?;
        map.serialize_entry("generation_config", &GenerationConfig(self))?;
        if let Some(format) = self.model.response_format() {
            map.serialize_entry("response_format", format)?;
        }
        if let Some(tier) = self.service_tier {
            map.serialize_entry("service_tier", &tier)?;
        }
        map.serialize_entry("store", &false)?;
        map.serialize_entry("stream", &self.stream)?;
        map.end()
    }
}
