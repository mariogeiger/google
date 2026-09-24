//! Live round trips against the real endpoint.
//!
//! Gated on a key file: `GEMINI_API_KEY_FILE` names it, or `.key` in the crate
//! root (git-ignored). With neither, each test prints `[skip]` and passes, so
//! `cargo test` stays offline. They cost quota — the free tier allows 20
//! requests a day per model — so run them deliberately:
//!
//! ```text
//! GEMINI_API_KEY_FILE=~/path/to/key cargo test --test live_api -- --nocapture
//! ```

use std::io::{BufRead, BufReader};

use google::content::FunctionOutput;
use google::conversation::Conversation;
use google::error::ApiError;
use google::model::{Gemini3_8Flash, Gemini3_8FlashThinking};
use google::request::Request;
use google::response::decode_interaction;
use google::settle::Settling;
use google::tools::{Function, Tool};
use google::turn::Turn;
use google::{API_BASE, HEADER_API_KEY, INTERACTIONS_PATH, KnownStatus, ThinkingSummaries};
use serde_json::json;

fn key() -> Option<String> {
    let path = std::env::var("GEMINI_API_KEY_FILE").unwrap_or_else(|_| format!("{}/.key", env!("CARGO_MANIFEST_DIR")));
    let path = path.strip_prefix("~/").map(|rest| format!("{}/{rest}", std::env::var("HOME").unwrap())).unwrap_or(path);
    std::fs::read_to_string(path).ok().map(|k| k.trim().to_owned()).filter(|k| !k.is_empty())
}

fn send(key: &str, request: &Request) -> Result<Turn, String> {
    let response = ureq::post(&format!("{API_BASE}{INTERACTIONS_PATH}"))
        .set(HEADER_API_KEY, key)
        .set("content-type", "application/json")
        .send_string(&serde_json::to_string(request).unwrap());
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            return Err(format!("{code}: {}", ApiError::from_body(&body).map(|e| e.to_string()).unwrap_or(body)));
        }
        Err(e) => return Err(e.to_string()),
    };
    if request.stream {
        let mut settling = Settling::new();
        for line in BufReader::new(response.into_reader()).lines() {
            settling.consume_line(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        settling.settle().map_err(|e| e.to_string())
    } else {
        decode_interaction(&response.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

#[test]
fn live_ok_streamed_function_round_trip() {
    let Some(key) = key() else { return eprintln!("[skip] no key") };
    let schema = json!({"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]});
    let tools = vec![Tool::Function(Function::new(
        "weather",
        "Current weather in a city.",
        schema.as_object().unwrap().clone(),
    ))];
    let mut conversation = Conversation::new(Some("Use the tool, then answer in one sentence.".into()), tools).unwrap();
    conversation.push_user_text("What is the weather in Lausanne?").unwrap();
    let model =
        Gemini3_8Flash::new().with_thinking(Gemini3_8FlashThinking::Low).with_summaries(ThinkingSummaries::Auto);

    let turn = send(&key, &Request::new(&conversation, model.clone()).unwrap().streaming()).unwrap();
    assert_eq!(turn.status, KnownStatus::RequiresAction);
    let ids: Vec<String> = turn.function_calls().map(|c| c.id.clone()).collect();
    assert!(!ids.is_empty());
    conversation.push_turn(turn).unwrap();
    for id in ids {
        conversation.push_function_result(&id, FunctionOutput::Text("Sunny, 21 °C.".into()), false).unwrap();
    }

    let answer = send(&key, &Request::new(&conversation, model).unwrap().streaming()).unwrap();
    assert_eq!(answer.status, KnownStatus::Completed);
    assert!(answer.text().contains("21"), "{}", answer.text());
    eprintln!("[ok] {}", answer.text());
}

#[test]
fn live_ok_buffered_text() {
    let Some(key) = key() else { return eprintln!("[skip] no key") };
    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation.push_user_text("Reply with the single word: pong").unwrap();
    let model = Gemini3_8Flash::new().with_thinking(Gemini3_8FlashThinking::Low);
    let turn = send(&key, &Request::new(&conversation, model).unwrap()).unwrap();
    assert_eq!(turn.status, KnownStatus::Completed);
    assert!(turn.text().to_lowercase().contains("pong"));
    eprintln!("[ok] {} ({} input tokens)", turn.text(), turn.usage.total_input_tokens);
}
