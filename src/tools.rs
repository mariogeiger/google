//! Tool declarations, and the choice of which ones the model may call.
//!
//! The tool array is part of every request's cached prefix, so it is fixed when
//! a [`crate::conversation::Conversation`] is built. Narrowing what the model
//! may call is a [`ToolChoice`], which leaves the array untouched.

use std::collections::BTreeMap;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::values::{ComputerEnvironment, ComputerSafetyPolicy, SearchType, ToolChoiceMode};

/// A function the caller runs when the model calls it.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// The name the model calls it by; unique within a conversation.
    pub name: String,
    /// What it does, for the model.
    pub description: Option<String>,
    /// The JSON Schema of its arguments object.
    pub parameters: Option<Map<String, Value>>,
}

impl Function {
    /// A function with a description and a JSON Schema for its arguments.
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Map<String, Value>) -> Self {
        Self { name: name.into(), description: Some(description.into()), parameters: Some(parameters) }
    }
}

/// Where the model's Google Maps searches are centred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatLng {
    /// Degrees north, in [-90, 90].
    pub latitude: f64,
    /// Degrees east, in [-180, 180].
    pub longitude: f64,
}

/// A remote MCP server whose tools the API calls on the model's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    /// The server's name, as it appears in its tool steps.
    pub name: String,
    /// The streamable-HTTP endpoint.
    pub url: String,
    /// Headers sent to the server, such as authentication.
    pub headers: BTreeMap<String, String>,
    /// Which of its tools the model may call, when restricted.
    pub allowed_tools: Vec<AllowedTools>,
}

/// One tool the model may use.
#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    /// A function the caller runs.
    Function(Function),
    /// Python the server runs.
    CodeExecution,
    /// Google Search grounding.
    GoogleSearch {
        /// Which searches, when restricted; the server searches the web otherwise.
        search_types: Vec<SearchType>,
    },
    /// Fetching URLs named in the prompt.
    UrlContext,
    /// Google Maps grounding.
    GoogleMaps {
        /// The user's location, when known.
        location: Option<LatLng>,
        /// Whether to return a widget context token for rendering a map.
        enable_widget: Option<bool>,
    },
    /// Retrieval from file-search stores.
    FileSearch {
        /// `fileSearchStores/…` names to search.
        store_names: Vec<String>,
        /// A metadata filter over documents and chunks.
        metadata_filter: Option<String>,
        /// How many chunks to retrieve.
        top_k: Option<u32>,
    },
    /// A remote MCP server.
    McpServer(McpServer),
    /// Operating a computer through predefined actions.
    ComputerUse {
        /// The environment operated.
        environment: ComputerEnvironment,
        /// Predefined actions to leave out.
        excluded_predefined_functions: Vec<String>,
        /// Confirmation policies switched off.
        disabled_safety_policies: Vec<ComputerSafetyPolicy>,
        /// Whether to check requests for prompt injection.
        enable_prompt_injection_detection: Option<bool>,
    },
}

impl Tool {
    /// The name a [`ToolChoice`] refers to it by, for the tools that have one.
    pub fn name(&self) -> Option<&str> {
        match self {
            Tool::Function(f) => Some(&f.name),
            Tool::McpServer(s) => Some(&s.name),
            _ => None,
        }
    }
}

impl Serialize for Tool {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        match self {
            Tool::Function(f) => {
                map.serialize_entry("type", "function")?;
                map.serialize_entry("name", &f.name)?;
                if let Some(description) = &f.description {
                    map.serialize_entry("description", description)?;
                }
                if let Some(parameters) = &f.parameters {
                    map.serialize_entry("parameters", parameters)?;
                }
            }
            Tool::CodeExecution => map.serialize_entry("type", "code_execution")?,
            Tool::GoogleSearch { search_types } => {
                map.serialize_entry("type", "google_search")?;
                if !search_types.is_empty() {
                    map.serialize_entry("search_types", search_types)?;
                }
            }
            Tool::UrlContext => map.serialize_entry("type", "url_context")?,
            Tool::GoogleMaps { location, enable_widget } => {
                map.serialize_entry("type", "google_maps")?;
                if let Some(at) = location {
                    map.serialize_entry("latitude", &at.latitude)?;
                    map.serialize_entry("longitude", &at.longitude)?;
                }
                if let Some(widget) = enable_widget {
                    map.serialize_entry("enable_widget", widget)?;
                }
            }
            Tool::FileSearch { store_names, metadata_filter, top_k } => {
                map.serialize_entry("type", "file_search")?;
                map.serialize_entry("file_search_store_names", store_names)?;
                if let Some(filter) = metadata_filter {
                    map.serialize_entry("metadata_filter", filter)?;
                }
                if let Some(k) = top_k {
                    map.serialize_entry("top_k", k)?;
                }
            }
            Tool::McpServer(server) => {
                map.serialize_entry("type", "mcp_server")?;
                map.serialize_entry("name", &server.name)?;
                map.serialize_entry("url", &server.url)?;
                if !server.headers.is_empty() {
                    map.serialize_entry("headers", &server.headers)?;
                }
                if !server.allowed_tools.is_empty() {
                    map.serialize_entry("allowed_tools", &server.allowed_tools)?;
                }
            }
            Tool::ComputerUse {
                environment,
                excluded_predefined_functions,
                disabled_safety_policies,
                enable_prompt_injection_detection,
            } => {
                map.serialize_entry("type", "computer_use")?;
                map.serialize_entry("environment", environment)?;
                if !excluded_predefined_functions.is_empty() {
                    map.serialize_entry("excluded_predefined_functions", excluded_predefined_functions)?;
                }
                if !disabled_safety_policies.is_empty() {
                    map.serialize_entry("disabled_safety_policies", disabled_safety_policies)?;
                }
                if let Some(detect) = enable_prompt_injection_detection {
                    map.serialize_entry("enable_prompt_injection_detection", detect)?;
                }
            }
        }
        map.end()
    }
}

/// A mode restricted to named tools.
///
/// For a conversation's own tools it is built by
/// [`crate::conversation::Conversation::allow_tools`], which checks every name
/// is declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedTools {
    mode: ToolChoiceMode,
    tools: Vec<String>,
}

impl AllowedTools {
    /// A restriction whose names the caller vouches for, as for an MCP server's
    /// own tools, which the crate cannot see.
    pub fn unchecked(mode: ToolChoiceMode, tools: Vec<String>) -> Self {
        Self { mode, tools }
    }

    /// The mode inside the restriction.
    pub fn mode(&self) -> ToolChoiceMode {
        self.mode
    }

    /// The names allowed.
    pub fn tools(&self) -> &[String] {
        &self.tools
    }
}

impl Serialize for AllowedTools {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(2))?;
        map.serialize_entry("mode", &self.mode)?;
        map.serialize_entry("tools", &self.tools)?;
        map.end()
    }
}

/// Whether, and which, tools the model may call this turn.
///
/// Per call, not part of the conversation: it changes nothing in the tool
/// array, so it can vary every turn without moving the cached prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolChoice {
    /// One mode over every declared tool.
    Mode(ToolChoiceMode),
    /// One mode over a subset.
    Allowed(AllowedTools),
}

impl Serialize for ToolChoice {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            ToolChoice::Mode(mode) => mode.serialize(s),
            ToolChoice::Allowed(allowed) => {
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry("allowed_tools", allowed)?;
                map.end()
            }
        }
    }
}
