//! Decoding and replaying bodies captured from the live endpoint.
//!
//! Every file in `tests/captured/` is a verbatim body from
//! `POST https://generativelanguage.googleapis.com/v1beta/interactions`,
//! captured 2026-09-24 with `store: false`, next to the request that produced
//! it (`*.request.json`). Model and prompt are in each request:
//!
//! * `text-stream` — Gemini 3.8 Flash, low thinking, one arithmetic question.
//! * `fc-turn1-stream` — Gemini 3.8 Flash, high thinking, two tools; the model
//!   calls `read_file` twice in parallel.
//! * `fc-turn2-stream` — the stateless continuation of that turn, with the
//!   first turn's steps replayed as the crate rebuilds them from the stream.
//!   The API accepted it with a 200 and the model called `word_count`.
//! * `buffered` — Gemini 3.8 Flash, `stream: false`.
//! * `cache-read-stream` — Gemini 3.7 Flash, the second of two identical
//!   requests with a long system instruction.
//! * `error-*` — failures: a thinking level the model refuses, a free-tier rate
//!   limit, an overloaded model and an exhausted quota, the last two framed as
//!   SSE events under a JSON content type.
//! * `probe-no-thought` — the continuation above with the thought step
//!   dropped, answered with a 400: model steps must be replayed whole.

use google::content::{FunctionOutput, ResultBlock};
use google::conversation::Conversation;
use google::error::ApiError;
use google::model::{Gemini3_8Flash, Gemini3_8FlashThinking};
use google::request::Request;
use google::response::decode_interaction;
use google::settle::{SettleError, Settling};
use google::step::StepView;
use google::tools::{Function, Tool};
use google::turn::Turn;
use google::{KnownErrorCode, KnownModality, KnownStatus, Modality, ThinkingSummaries};
use serde_json::Value;

fn captured(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/captured/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn settle(name: &str) -> Turn {
    let mut settling = Settling::new();
    for line in captured(name).lines() {
        settling.consume_line(line).unwrap();
    }
    settling.settle().unwrap()
}

#[test]
fn a_text_stream_settles_into_a_thought_and_an_answer() {
    let turn = settle("text-stream.sse");
    assert_eq!(turn.status, KnownStatus::Completed);
    assert_eq!(turn.text(), "17 multiplied by 23 is 391.");
    assert_eq!(turn.steps.len(), 2);
    assert!(
        matches!(turn.steps[0].view(), StepView::Thought { signature: Some(s), summary } if s.len() > 100 && summary.is_empty())
    );
    assert_eq!(turn.usage.total_input_tokens, 22);
    assert_eq!(turn.usage.total_thought_tokens, 139);
    assert_eq!(turn.usage.total_output_tokens, 13);
    assert_eq!(turn.usage.input_tokens_by_modality[&Modality::Known(KnownModality::Text)], 22);
    assert_eq!(turn.model.as_deref(), Some("gemini-3.8-flash"));
}

#[test]
fn a_truncated_stream_does_not_settle() {
    let body = captured("text-stream.sse");
    let cut = body.find("event: interaction.completed").unwrap();
    let mut settling = Settling::new();
    for line in body[..cut].lines() {
        settling.consume_line(line).unwrap();
    }
    assert_eq!(settling.settle().unwrap_err(), SettleError::Truncated);
}

fn tools_of(request: &Value) -> Vec<Tool> {
    request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            Tool::Function(Function::new(
                t["name"].as_str().unwrap(),
                t["description"].as_str().unwrap(),
                t["parameters"].as_object().unwrap().clone(),
            ))
        })
        .collect()
}

#[test]
fn a_streamed_function_call_turn_replays_as_the_accepted_continuation() {
    let turn1: Value = serde_json::from_str(&captured("fc-turn1-stream.request.json")).unwrap();
    let turn2: Value = serde_json::from_str(&captured("fc-turn2-stream.request.json")).unwrap();

    let mut conversation =
        Conversation::new(turn1["system_instruction"].as_str().map(Into::into), tools_of(&turn1)).unwrap();
    conversation.push_user_text(turn1["input"][0]["content"][0]["text"].as_str().unwrap()).unwrap();

    let turn = settle("fc-turn1-stream.sse");
    assert_eq!(turn.status, KnownStatus::RequiresAction);
    let calls: Vec<_> = turn.function_calls().map(|c| (c.id.clone(), c.arguments["path"].clone())).collect();
    assert_eq!(calls.len(), 2);
    conversation.push_turn(turn).unwrap();

    let files = [("notes.txt", "buy milk and eggs"), ("todo.txt", "fix the roof, call mom, file taxes, walk the dog")];
    for (id, path) in &calls {
        let text = files.iter().find(|(p, _)| path == p).unwrap().1;
        let output = FunctionOutput::Blocks(vec![ResultBlock::Text(text.into())]);
        conversation.push_function_result(id, output, false).unwrap();
    }

    let model =
        Gemini3_8Flash::new().with_thinking(Gemini3_8FlashThinking::High).with_summaries(ThinkingSummaries::Auto);
    let body = serde_json::to_value(Request::new(&conversation, model).unwrap().streaming()).unwrap();

    for key in ["model", "system_instruction", "tools", "generation_config", "store", "stream"] {
        assert_eq!(body[key], turn2[key], "{key}");
    }
    let (ours, theirs) = (body["input"].as_array().unwrap(), turn2["input"].as_array().unwrap());
    assert_eq!(ours.len(), theirs.len());
    for (ours, theirs) in ours.iter().zip(theirs) {
        let mut ours = ours.clone();
        if ours["type"] == "function_result" {
            assert_eq!(ours.as_object_mut().unwrap().remove("is_error"), Some(Value::Bool(false)));
        }
        assert_eq!(&ours, theirs);
    }

    let next = settle("fc-turn2-stream.sse");
    assert_eq!(next.status, KnownStatus::RequiresAction);
    assert!(next.function_calls().all(|c| c.name == "word_count"));
}

#[test]
fn a_buffered_body_decodes_into_the_same_turn_shape() {
    let turn = decode_interaction(&captured("buffered.response.json")).unwrap();
    assert_eq!(turn.status, KnownStatus::Completed);
    assert_eq!(turn.text(), "101");
    assert!(matches!(turn.steps[0].view(), StepView::Thought { signature: Some(_), .. }));
    assert_eq!(turn.service_tier.as_deref(), Some("standard"));
}

#[test]
fn the_implicit_cache_reports_through_usage() {
    let turn = settle("cache-read-stream.sse");
    assert_eq!(turn.usage.total_cached_tokens, 4_081);
    assert_eq!(turn.usage.total_input_tokens, 10_185);
}

#[test]
fn every_captured_error_body_decodes() {
    let cases = [
        ("error-minimal-thinking.response.json", KnownErrorCode::InvalidRequest),
        ("probe-no-thought.response.json", KnownErrorCode::InvalidRequest),
        ("error-rate-limit.response.json", KnownErrorCode::TooManyRequests),
        ("error-unavailable-stream.body", KnownErrorCode::ServiceUnavailable),
        ("error-quota-stream.body", KnownErrorCode::QuotaExceeded),
    ];
    for (name, code) in cases {
        let error = ApiError::from_body(&captured(name)).unwrap();
        assert_eq!(error.code, google::ErrorCode::Known(code), "{name}");
        assert!(!error.message.is_empty());
    }
}

#[test]
fn an_error_event_fails_the_stream() {
    let mut settling = Settling::new();
    let mut failure = None;
    for line in captured("error-unavailable-stream.body").lines() {
        if let Err(e) = settling.consume_line(line) {
            failure = Some(e);
        }
    }
    assert!(matches!(failure, Some(SettleError::Api(e)) if e.code == KnownErrorCode::ServiceUnavailable));
}
