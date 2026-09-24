//! The request body the crate writes, and the requests it refuses to write.

use google::content::{Base64, FunctionOutput, Image, InputContent, ResultBlock, Source};
use google::conversation::{Conversation, ConversationError};
use google::model::{Gemini3_8Flash, OutputLimitExceeded, TextFormat};
use google::request::{Request, RequestError};
use google::settle::Settling;
use google::stream::{Delta, StreamEvent};
use google::tools::{Function, LatLng, Tool, ToolChoice};
use google::turn::Turn;
use google::usage::Usage;
use google::{ImageMime, MediaResolution, SearchType, ServiceTier, ToolChoiceMode};
use serde_json::json;

fn function(name: &str) -> Tool {
    Tool::Function(Function::new(name, "Does a thing.", json!({"type": "object"}).as_object().unwrap().clone()))
}

fn settled(lines: &[&str]) -> Turn {
    let mut settling = Settling::new();
    for line in lines {
        settling.consume_payload(line).unwrap();
    }
    settling.settle().unwrap()
}

fn one_call(id: &str) -> Turn {
    settled(&[
        &format!(
            r#"{{"index":0,"step":{{"id":"{id}","type":"function_call","name":"f","arguments":{{}}}},"event_type":"step.start"}}"#
        ),
        r#"{"index":0,"delta":{"arguments":"{\"x\":1}","type":"arguments_delta"},"event_type":"step.delta"}"#,
        r#"{"index":0,"event_type":"step.stop"}"#,
        r#"{"interaction":{"id":"","status":"requires_action"},"event_type":"interaction.completed"}"#,
    ])
}

#[test]
fn the_body_is_complete_and_in_prefix_order() {
    let mut conversation = Conversation::new(Some("Be brief.".into()), vec![function("f")]).unwrap();
    conversation.push_user_text("hi").unwrap();
    let body = serde_json::to_string(&Request::new(&conversation, Gemini3_8Flash::new()).unwrap()).unwrap();
    assert_eq!(
        body,
        concat!(
            r#"{"model":"gemini-3.8-flash","system_instruction":"Be brief.","#,
            r#""tools":[{"type":"function","name":"f","description":"Does a thing.","parameters":{"type":"object"}}],"#,
            r#""input":[{"type":"user_input","content":[{"type":"text","text":"hi"}]}],"#,
            r#""generation_config":{"thinking_level":"medium","thinking_summaries":"none"},"#,
            r#""store":false,"stream":false}"#
        )
    );
}

#[test]
fn per_call_parameters_land_where_the_api_reads_them() {
    let mut conversation = Conversation::new(None, vec![function("f"), function("g")]).unwrap();
    conversation.push_user_text("hi").unwrap();
    let model = Gemini3_8Flash::new()
        .with_max_output_tokens(1_000)
        .unwrap()
        .with_response_format(TextFormat::Json(json!({"type": "object"}).as_object().unwrap().clone()));
    let allowed = conversation.allow_tools(ToolChoiceMode::Any, &["g"]).unwrap();
    let mut request = Request::new(&conversation, model).unwrap().with_tool_choice(ToolChoice::Allowed(allowed));
    request.seed = Some(7);
    request.stop_sequences = vec!["END".into()];
    request.service_tier = Some(ServiceTier::Flex);
    let body = serde_json::to_value(&request).unwrap();
    assert_eq!(
        body["generation_config"],
        json!({
            "thinking_level": "medium",
            "thinking_summaries": "none",
            "max_output_tokens": 1000,
            "seed": 7,
            "stop_sequences": ["END"],
            "tool_choice": {"allowed_tools": {"mode": "any", "tools": ["g"]}},
        })
    );
    assert_eq!(
        body["response_format"],
        json!({"type": "text", "mime_type": "application/json", "schema": {"type": "object"}})
    );
    assert_eq!(body["service_tier"], "flex");
    assert!(body.get("system_instruction").is_none());
}

#[test]
fn the_output_cap_is_checked_against_the_model() {
    assert_eq!(
        Gemini3_8Flash::new().with_max_output_tokens(65_537).unwrap_err(),
        OutputLimitExceeded { requested: 65_537, limit: 65_536 }
    );
    assert!(Gemini3_8Flash::new().with_max_output_tokens(0).is_err());
    assert_eq!(Gemini3_8Flash::new().with_max_output_tokens(65_536).unwrap().max_output_tokens(), Some(65_536));
}

#[test]
fn tool_names_are_unique_and_choices_name_declared_tools() {
    assert_eq!(
        Conversation::new(None, vec![function("f"), function("f")]).unwrap_err(),
        ConversationError::DuplicateToolName("f".into())
    );
    let conversation = Conversation::new(None, vec![function("f"), Tool::CodeExecution]).unwrap();
    assert_eq!(
        conversation.allow_tools(ToolChoiceMode::Auto, &["nope"]).unwrap_err(),
        ConversationError::UnknownTool("nope".into())
    );
    assert_eq!(serde_json::to_value(ToolChoice::Mode(ToolChoiceMode::Validated)).unwrap(), json!("validated"));
}

#[test]
fn a_model_turn_needs_a_caller_step_before_it() {
    let empty = Conversation::new(None, vec![]).unwrap();
    assert_eq!(Request::new(&empty, Gemini3_8Flash::new()).unwrap_err(), RequestError::NotAwaitingModel);

    let mut conversation = Conversation::new(None, vec![]).unwrap();
    conversation.push_user_text("hi").unwrap();
    let answer = settled(&[
        r#"{"index":0,"step":{"type":"model_output"},"event_type":"step.start"}"#,
        r#"{"index":0,"delta":{"text":"hello","type":"text"},"event_type":"step.delta"}"#,
        r#"{"index":0,"event_type":"step.stop"}"#,
        r#"{"interaction":{"id":"","status":"completed"},"event_type":"interaction.completed"}"#,
    ]);
    conversation.push_turn(answer.clone()).unwrap();
    assert_eq!(conversation.push_turn(answer).unwrap_err(), ConversationError::NotAwaitingModel);
    assert!(Request::new(&conversation, Gemini3_8Flash::new()).is_err());
    conversation.push_user_text("again").unwrap();
    assert!(Request::new(&conversation, Gemini3_8Flash::new()).is_ok());
}

#[test]
fn every_call_is_answered_once_before_anything_else() {
    let mut conversation = Conversation::new(None, vec![function("f")]).unwrap();
    conversation.push_user_text("hi").unwrap();
    conversation.push_turn(one_call("call_1")).unwrap();
    assert_eq!(
        conversation.push_user_text("more").unwrap_err(),
        ConversationError::UnansweredCalls(vec!["call_1".into()])
    );
    assert_eq!(
        conversation.push_function_result("call_2", FunctionOutput::Text("x".into()), false).unwrap_err(),
        ConversationError::UnknownCall("call_2".into())
    );
    conversation.push_function_result("call_1", FunctionOutput::Json(Default::default()), true).unwrap();
    assert!(conversation.push_function_result("call_1", FunctionOutput::Text("x".into()), false).is_err());
    let body = serde_json::to_value(Request::new(&conversation, Gemini3_8Flash::new()).unwrap()).unwrap();
    assert_eq!(body["input"][1], json!({"type": "function_call", "id": "call_1", "name": "f", "arguments": {"x": 1}}));
    assert_eq!(
        body["input"][2],
        json!({"type": "function_result", "call_id": "call_1", "name": "f", "result": {}, "is_error": true})
    );
}

#[test]
fn media_and_results_take_the_documented_shapes() {
    let image = Image {
        source: Source::Inline(Base64::encode(b"png")),
        mime: ImageMime::Png,
        resolution: Some(MediaResolution::High),
    };
    assert_eq!(
        serde_json::to_value(InputContent::Image(image.clone())).unwrap(),
        json!({"type": "image", "data": "cG5n", "mime_type": "image/png", "resolution": "high"})
    );
    let by_uri = Image::new(Source::Uri("https://example.com/a.jpg".into()), ImageMime::Jpeg);
    assert_eq!(
        serde_json::to_value(InputContent::Image(by_uri)).unwrap(),
        json!({"type": "image", "uri": "https://example.com/a.jpg", "mime_type": "image/jpeg"})
    );
    assert!(Base64::from_encoded("not base64!").is_err());
    assert_eq!(Base64::from_encoded("cG5n").unwrap().decode(), b"png");
    let blocks = FunctionOutput::Blocks(vec![ResultBlock::Text("t".into()), ResultBlock::Image(image)]);
    assert_eq!(serde_json::to_value(blocks).unwrap()[1]["type"], "image");
    assert_eq!(serde_json::to_value(FunctionOutput::Text("t".into())).unwrap(), json!("t"));
}

#[test]
fn hosted_tools_serialize_only_what_the_caller_set() {
    let tools = vec![
        Tool::GoogleSearch { search_types: vec![SearchType::WebSearch] },
        Tool::GoogleMaps { location: Some(LatLng { latitude: 1.5, longitude: 2.0 }), enable_widget: None },
        Tool::UrlContext,
        Tool::CodeExecution,
    ];
    assert_eq!(
        serde_json::to_value(tools).unwrap(),
        json!([
            {"type": "google_search", "search_types": ["web_search"]},
            {"type": "google_maps", "latitude": 1.5, "longitude": 2.0},
            {"type": "url_context"},
            {"type": "code_execution"},
        ])
    );
}

#[test]
fn unknown_events_and_deltas_are_not_errors() {
    assert_eq!(
        StreamEvent::decode(r#"{"event_type":"interaction.teleported"}"#).unwrap(),
        StreamEvent::Unrecognized { event_type: "interaction.teleported".into() }
    );
    let event =
        StreamEvent::decode(r#"{"index":0,"delta":{"type":"hologram","x":1},"event_type":"step.delta"}"#).unwrap();
    assert!(matches!(event, StreamEvent::StepDelta { delta: Delta::Unrecognized(_), .. }));
    assert!(StreamEvent::decode("{").is_err());
    assert!(StreamEvent::decode(r#"{"index":0}"#).is_err());
}

#[test]
fn server_tool_steps_replay_their_payload_verbatim() {
    let turn = settled(&[
        r#"{"index":0,"step":{"id":"s1","type":"google_search_call","arguments":{"queries":[]}},"event_type":"step.start"}"#,
        r#"{"index":0,"delta":{"type":"google_search_call","arguments":{"queries":["tour de france"]},"signature":"c2ln"},"event_type":"step.delta"}"#,
        r#"{"index":0,"event_type":"step.stop"}"#,
        r#"{"interaction":{"id":"","status":"completed"},"event_type":"interaction.completed"}"#,
    ]);
    let wire = serde_json::to_value(&turn.steps[0]).unwrap();
    assert_eq!(
        wire,
        json!({"id": "s1", "type": "google_search_call", "arguments": {"queries": ["tour de france"]}, "signature": "c2ln"})
    );
}

#[test]
fn usage_joins_pointwise() {
    let a: Usage = serde_json::from_value(json!({"total_input_tokens": 10, "total_output_tokens": 1})).unwrap();
    let b: Usage = serde_json::from_value(json!({"total_output_tokens": 5, "total_cached_tokens": 3})).unwrap();
    let mut joined = a.clone();
    joined.join(&b);
    assert_eq!((joined.total_input_tokens, joined.total_output_tokens, joined.total_cached_tokens), (10, 5, 3));
    assert_eq!(joined.total_tokens, 0);
}
