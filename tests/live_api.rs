//! Live round trips against a real endpoint, one set per modelled model.
//!
//! Gated on a key file: `GEMINI_API_KEY_FILE` names it, or `.key` in the crate
//! root (git-ignored). With neither, each test prints `[skip]` and passes, so
//! `cargo test` stays offline. They cost quota — the free tier allows 20
//! requests a day per model — so run one model at a time, by its module name:
//!
//! ```text
//! GEMINI_API_KEY_FILE=~/path/to/key cargo test --test live_api gemini_3_8_flash -- --nocapture
//! ```
//!
//! The endpoint is Google's unless two more variables say a gateway that
//! speaks the same API: `GEMINI_API_BASE` is its origin, and
//! `GEMINI_MODEL_PREFIX` is put before the model id in the body, as in
//! `gateway/<model>`. The key travels in the header Google names either way.

use std::io::{BufRead, BufReader};

use google::content::FunctionOutput;
use google::conversation::Conversation;
use google::error::ApiError;
use google::model::{Gemini3_8Flash, Gemini3_8FlashThinking, Gemini4Argon, Gemini4ArgonThinking, Model};
use google::request::Request;
use google::response::decode_interaction;
use google::settle::Settling;
use google::step::StepView;
use google::tools::{Function, Tool};
use google::turn::Turn;
use google::{API_BASE, HEADER_API_KEY, INTERACTIONS_PATH, KnownStatus, ThinkingSummaries};
use serde_json::{Value, json};

/// Why a call did not become a turn: the HTTP status, or 200 for a failure
/// the stream reports itself, and what the API said.
#[derive(Debug)]
struct Failure {
    status: u16,
    message: String,
}

fn key() -> Option<String> {
    let path = std::env::var("GEMINI_API_KEY_FILE").unwrap_or_else(|_| format!("{}/.key", env!("CARGO_MANIFEST_DIR")));
    let path = path.strip_prefix("~/").map(|rest| format!("{}/{rest}", std::env::var("HOME").unwrap())).unwrap_or(path);
    std::fs::read_to_string(path).ok().map(|k| k.trim().to_owned()).filter(|k| !k.is_empty())
}

/// The body of `request`, with the model id the endpoint under test knows.
fn body_of(request: &Request) -> Value {
    let mut body = serde_json::to_value(request).unwrap();
    let prefix = std::env::var("GEMINI_MODEL_PREFIX").unwrap_or_default();
    body["model"] = json!(format!("{prefix}{}", request.model.id()));
    body
}

fn send_body(key: &str, body: &Value) -> Result<Turn, Failure> {
    let origin = std::env::var("GEMINI_API_BASE").unwrap_or_else(|_| API_BASE.to_owned());
    let failed = |status, message: String| Failure { status, message };
    let response = ureq::post(&format!("{origin}{INTERACTIONS_PATH}"))
        .set(HEADER_API_KEY, key)
        .set("content-type", "application/json")
        .send_string(&body.to_string());
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            return Err(failed(code, ApiError::from_body(&body).map(|e| e.to_string()).unwrap_or(body)));
        }
        Err(e) => return Err(failed(0, e.to_string())),
    };
    if body["stream"] == true {
        let mut settling = Settling::new();
        for line in BufReader::new(response.into_reader()).lines() {
            settling
                .consume_line(&line.map_err(|e| failed(0, e.to_string()))?)
                .map_err(|e| failed(200, e.to_string()))?;
        }
        settling.settle().map_err(|e| failed(200, e.to_string()))
    } else {
        let text = response.into_string().map_err(|e| failed(0, e.to_string()))?;
        decode_interaction(&text).map_err(|e| failed(200, e.to_string()))
    }
}

fn send(key: &str, request: &Request) -> Result<Turn, Failure> {
    send_body(key, &body_of(request))
}

fn weather() -> Vec<Tool> {
    let schema = json!({"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]});
    vec![Tool::Function(Function::new("weather", "Current weather in a city.", schema.as_object().unwrap().clone()))]
}

/// A function call answered and the answer read: the second request replays
/// the first turn's steps, thought signature included, which the API refuses
/// if they are not exactly as received.
fn streamed_function_round_trip(key: &str, model: impl Into<Model> + Clone) {
    let mut conversation =
        Conversation::new(Some("Use the tool, then answer in one sentence.".into()), weather()).unwrap();
    conversation.push_user_text("What is the weather in Lausanne?").unwrap();

    let turn = send(key, &Request::new(&conversation, model.clone()).unwrap().streaming()).unwrap();
    assert_eq!(turn.status, KnownStatus::RequiresAction);
    let ids: Vec<String> = turn.function_calls().map(|c| c.id.clone()).collect();
    assert!(!ids.is_empty());
    conversation.push_turn(turn).unwrap();
    for id in ids {
        conversation.push_function_result(&id, FunctionOutput::Text("Sunny, 21 °C.".into()), false).unwrap();
    }

    let answer = send(key, &Request::new(&conversation, model).unwrap().streaming()).unwrap();
    assert_eq!(answer.status, KnownStatus::Completed);
    assert!(answer.text().contains("21"), "{}", answer.text());
    eprintln!("[ok] {}", answer.text());
}

fn buffered_text(key: &str, model: impl Into<Model>) {
    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation.push_user_text("Reply with the single word: pong").unwrap();
    let turn = send(key, &Request::new(&conversation, model).unwrap()).unwrap();
    assert_eq!(turn.status, KnownStatus::Completed);
    assert!(turn.text().to_lowercase().contains("pong"), "{}", turn.text());
    eprintln!("[ok] {} ({} input tokens)", turn.text(), turn.usage.total_input_tokens);
}

/// Thought summaries stream in, and settle into the thought they belong to.
/// A model summarizes only reasoning that is long enough, so the question is
/// one that takes some.
fn streamed_thought_summary(key: &str, model: impl Into<Model>) {
    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation
        .push_user_text(
            "How many positive integers below 1000 are divisible by exactly one of 3 and 5? \
             Think it through, then answer with the number only.",
        )
        .unwrap();
    let turn = send(key, &Request::new(&conversation, model).unwrap().streaming()).unwrap();
    assert_eq!(turn.status, KnownStatus::Completed);
    let summaries: Vec<_> = turn
        .steps
        .iter()
        .filter_map(|step| match step.view() {
            StepView::Thought { summary, .. } => Some(summary.len()),
            _ => None,
        })
        .collect();
    assert!(summaries.iter().any(|&blocks| blocks > 0), "no thought carried a summary: {summaries:?}");
    eprintln!("[ok] {} (summary blocks per thought: {summaries:?})", turn.text());
}

/// What the API refuses is what the model's type has no value for: the body is
/// written from the type, then edited into what the type cannot say.
fn refuses_thinking_level(key: &str, model: impl Into<Model>, level: &str) {
    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation.push_user_text("Reply with the single word: pong").unwrap();
    let mut body = body_of(&Request::new(&conversation, model).unwrap());
    body["generation_config"]["thinking_level"] = json!(level);
    let failure = send_body(key, &body).unwrap_err();
    assert_eq!(failure.status, 400, "{level}: {}", failure.message);
    eprintln!("[ok] refused `{level}`: {}", failure.message);
}

/// The tests of one model type: low thinking where the question is the
/// plumbing, high where it is the reasoning a summary is made of.
macro_rules! live_model {
    ($name:ident, $model:ident, $thinking:ident) => {
        mod $name {
            use super::*;

            fn low() -> $model {
                $model::new().with_thinking($thinking::Low)
            }

            #[test]
            fn streamed_function_round_trip() {
                let Some(key) = key() else { return eprintln!("[skip] no key") };
                super::streamed_function_round_trip(&key, low().with_summaries(ThinkingSummaries::Auto));
            }

            #[test]
            fn buffered_text() {
                let Some(key) = key() else { return eprintln!("[skip] no key") };
                super::buffered_text(&key, low());
            }

            #[test]
            fn streamed_thought_summary() {
                let Some(key) = key() else { return eprintln!("[skip] no key") };
                let model = $model::new().with_thinking($thinking::High).with_summaries(ThinkingSummaries::Auto);
                super::streamed_thought_summary(&key, model);
            }

            #[test]
            fn refuses_the_thinking_levels_its_type_lacks() {
                let Some(key) = key() else { return eprintln!("[skip] no key") };
                for level in ["minimal", "xhigh"] {
                    super::refuses_thinking_level(&key, low(), level);
                }
            }
        }
    };
}

live_model!(gemini_3_8_flash, Gemini3_8Flash, Gemini3_8FlashThinking);
live_model!(gemini_4_argon, Gemini4Argon, Gemini4ArgonThinking);

#[test]
fn gemini_3_8_flash_refuses_a_cap_above_its_limit() {
    let Some(key) = key() else { return eprintln!("[skip] no key") };
    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation.push_user_text("Reply with the single word: pong").unwrap();
    for stream in [false, true] {
        let mut request = Request::new(&conversation, Gemini3_8Flash::new()).unwrap();
        request.stream = stream;
        let mut body = body_of(&request);
        body["generation_config"]["max_output_tokens"] = json!(Gemini3_8Flash::OUTPUT_TOKEN_LIMIT + 1);
        let failure = send_body(&key, &body).unwrap_err();
        eprintln!("[ok] stream={stream} refused: {}", failure.message);
    }
}
