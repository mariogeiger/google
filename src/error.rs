//! An error the API reports, in a response body or in a stream.

use serde::Deserialize;

use crate::frame::data_payload;
use crate::values::ErrorCode;

/// What the API said went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiError {
    /// The kind of failure.
    pub code: ErrorCode,
    /// A human-readable explanation, verbatim.
    #[serde(default)]
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ApiError {}

/// A non-success body that does not hold an error object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAnErrorBody(pub String);

impl std::fmt::Display for NotAnErrorBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "body holds no error object: {}", self.0)
    }
}

impl std::error::Error for NotAnErrorBody {}

#[derive(Deserialize)]
struct Envelope {
    error: ApiError,
}

impl ApiError {
    /// Decode the body of a non-success response.
    ///
    /// Two shapes arrive. A buffered request gets `{"error": {…}}`. A streamed
    /// request that fails before its first event gets the same object framed as
    /// one SSE `error` event — under a `Content-Type: application/json` header
    /// (measured 2026-09-24 on 429 and 503). Both decode here.
    pub fn from_body(body: &str) -> Result<Self, NotAnErrorBody> {
        let json = body.lines().find_map(data_payload).unwrap_or(body);
        serde_json::from_str::<Envelope>(json).map(|e| e.error).map_err(|e| NotAnErrorBody(e.to_string()))
    }
}
