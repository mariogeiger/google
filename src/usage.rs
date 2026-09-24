//! What a request cost, and what the implicit cache did.
//!
//! Implicit caching is silent: the only evidence a prefix was reused is
//! `total_cached_tokens`. So usage decodes rather than being skipped, absent
//! counters read as zero, and merging the several usage objects a stream sends
//! is a pointwise maximum — the join of a product lattice of counters — so a
//! later frame that omits a field cannot erase an earlier one.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::values::{GroundingTool, Modality};

/// Tokens broken down by modality.
pub type ByModality = BTreeMap<Modality, u64>;

/// Token counts for one interaction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    /// Prompt tokens, cached ones included.
    pub total_input_tokens: u64,
    /// Prompt tokens read from the implicit cache.
    pub total_cached_tokens: u64,
    /// Answer tokens, thinking excluded.
    pub total_output_tokens: u64,
    /// Thinking tokens, billed as output.
    pub total_thought_tokens: u64,
    /// Tokens of server-side tool prompts.
    pub total_tool_use_tokens: u64,
    /// Everything, as the API totals it.
    pub total_tokens: u64,
    /// Prompt tokens per modality.
    pub input_tokens_by_modality: ByModality,
    /// Cached tokens per modality.
    pub cached_tokens_by_modality: ByModality,
    /// Answer tokens per modality.
    pub output_tokens_by_modality: ByModality,
    /// Tool-use tokens per modality.
    pub tool_use_tokens_by_modality: ByModality,
    /// How many grounding calls each grounding tool made.
    pub grounding_tool_count: BTreeMap<GroundingTool, u64>,
}

#[derive(Deserialize)]
struct ModalityWire {
    modality: Modality,
    #[serde(default)]
    tokens: u64,
}

#[derive(Deserialize)]
struct GroundingWire {
    #[serde(rename = "type")]
    tool: GroundingTool,
    #[serde(default)]
    count: u64,
}

fn by_modality<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ByModality, D::Error> {
    let items = Option::<Vec<ModalityWire>>::deserialize(d)?.unwrap_or_default();
    let mut out = ByModality::new();
    for item in items {
        *out.entry(item.modality).or_default() += item.tokens;
    }
    Ok(out)
}

fn grounding<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<GroundingTool, u64>, D::Error> {
    let items = Option::<Vec<GroundingWire>>::deserialize(d)?.unwrap_or_default();
    let mut out = BTreeMap::new();
    for item in items {
        *out.entry(item.tool).or_default() += item.count;
    }
    Ok(out)
}

impl<'de> Deserialize<'de> for Usage {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            total_input_tokens: Option<u64>,
            #[serde(default)]
            total_cached_tokens: Option<u64>,
            #[serde(default)]
            total_output_tokens: Option<u64>,
            #[serde(default)]
            total_thought_tokens: Option<u64>,
            #[serde(default)]
            total_tool_use_tokens: Option<u64>,
            #[serde(default)]
            total_tokens: Option<u64>,
            #[serde(default, deserialize_with = "by_modality")]
            input_tokens_by_modality: ByModality,
            #[serde(default, deserialize_with = "by_modality")]
            cached_tokens_by_modality: ByModality,
            #[serde(default, deserialize_with = "by_modality")]
            output_tokens_by_modality: ByModality,
            #[serde(default, deserialize_with = "by_modality")]
            tool_use_tokens_by_modality: ByModality,
            #[serde(default, deserialize_with = "grounding")]
            grounding_tool_count: BTreeMap<GroundingTool, u64>,
        }
        let w = Wire::deserialize(d)?;
        Ok(Usage {
            total_input_tokens: w.total_input_tokens.unwrap_or(0),
            total_cached_tokens: w.total_cached_tokens.unwrap_or(0),
            total_output_tokens: w.total_output_tokens.unwrap_or(0),
            total_thought_tokens: w.total_thought_tokens.unwrap_or(0),
            total_tool_use_tokens: w.total_tool_use_tokens.unwrap_or(0),
            total_tokens: w.total_tokens.unwrap_or(0),
            input_tokens_by_modality: w.input_tokens_by_modality,
            cached_tokens_by_modality: w.cached_tokens_by_modality,
            output_tokens_by_modality: w.output_tokens_by_modality,
            tool_use_tokens_by_modality: w.tool_use_tokens_by_modality,
            grounding_tool_count: w.grounding_tool_count,
        })
    }
}

fn join<K: Ord + Clone>(into: &mut BTreeMap<K, u64>, from: &BTreeMap<K, u64>) {
    for (k, v) in from {
        let slot = into.entry(k.clone()).or_default();
        *slot = (*slot).max(*v);
    }
}

impl Usage {
    /// The pointwise maximum of two reports of the same interaction.
    pub fn join(&mut self, other: &Usage) {
        self.total_input_tokens = self.total_input_tokens.max(other.total_input_tokens);
        self.total_cached_tokens = self.total_cached_tokens.max(other.total_cached_tokens);
        self.total_output_tokens = self.total_output_tokens.max(other.total_output_tokens);
        self.total_thought_tokens = self.total_thought_tokens.max(other.total_thought_tokens);
        self.total_tool_use_tokens = self.total_tool_use_tokens.max(other.total_tool_use_tokens);
        self.total_tokens = self.total_tokens.max(other.total_tokens);
        join(&mut self.input_tokens_by_modality, &other.input_tokens_by_modality);
        join(&mut self.cached_tokens_by_modality, &other.cached_tokens_by_modality);
        join(&mut self.output_tokens_by_modality, &other.output_tokens_by_modality);
        join(&mut self.tool_use_tokens_by_modality, &other.tool_use_tokens_by_modality);
        join(&mut self.grounding_tool_count, &other.grounding_tool_count);
    }
}
