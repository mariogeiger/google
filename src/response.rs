//! A buffered (`stream: false`) response body.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::step::{ModelStep, StepError};
use crate::turn::Turn;
use crate::usage::Usage;
use crate::values::{KnownStatus, Status};

/// Why a buffered body did not become a turn.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseError {
    /// The body is not the interaction object.
    Body(String),
    /// A step contradicts the schema.
    Step(StepError),
    /// The interaction has not reached a final status.
    NotFinal(String),
}

impl std::fmt::Display for ResponseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResponseError::Body(e) => write!(f, "not an interaction body: {e}"),
            ResponseError::Step(e) => e.fmt(f),
            ResponseError::NotFinal(s) => write!(f, "interaction has non-final status `{s}`"),
        }
    }
}

impl std::error::Error for ResponseError {}

#[derive(Deserialize)]
struct Wire {
    status: Status,
    #[serde(default)]
    steps: Vec<Map<String, Value>>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    service_tier: Option<String>,
}

/// Decode the body of a successful buffered request.
pub fn decode_interaction(body: &str) -> Result<Turn, ResponseError> {
    let wire: Wire = serde_json::from_str(body).map_err(|e| ResponseError::Body(e.to_string()))?;
    if wire.status == KnownStatus::InProgress || wire.status == KnownStatus::Queued {
        return Err(ResponseError::NotFinal(wire.status.as_str().to_owned()));
    }
    let steps =
        wire.steps.into_iter().map(ModelStep::from_wire).collect::<Result<_, _>>().map_err(ResponseError::Step)?;
    Ok(Turn {
        steps,
        status: wire.status,
        usage: wire.usage.unwrap_or_default(),
        model: wire.model,
        service_tier: wire.service_tier,
    })
}
