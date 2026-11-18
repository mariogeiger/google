//! An interaction that stops short of its answer is not a turn.
//!
//! The reference lists `requires_continuation` among the statuses of an
//! interaction: one request reached the output cap its model allows a single
//! request, and the rest of the answer comes from a further request carrying
//! the `continuation_token` of this one. The bodies below are written from
//! that reference (`Interaction.status`, `Interaction.continuation_token`) and
//! Google's long-decode guide, not captured: reaching the cap takes tens of
//! thousands of generated tokens.

use google::response::{ResponseError, decode_interaction};
use google::settle::{SettleError, Settling};
use google::{KnownStatus, Status};

#[test]
fn only_a_hop_that_is_still_to_come_or_running_is_not_final() {
    for known in KnownStatus::ALL {
        let expected =
            !matches!(known, KnownStatus::InProgress | KnownStatus::Queued | KnownStatus::RequiresContinuation);
        assert_eq!(Status::Known(*known).is_final(), expected, "{}", known.as_str());
    }
    assert!(Status::Unrecognized("a_status_newer_than_the_crate".into()).is_final());
}

#[test]
fn a_streamed_hop_that_requires_continuation_does_not_settle() {
    let frames = [
        r#"{"event_type":"interaction.created","interaction":{"id":"","status":"in_progress"}}"#,
        r#"{"event_type":"step.start","index":0,"step":{"type":"model_output"}}"#,
        r#"{"event_type":"step.delta","index":0,"delta":{"type":"text","text":"1 2 3"}}"#,
        r#"{"event_type":"step.stop","index":0}"#,
        r#"{"event_type":"interaction.completed","interaction":{"id":"","status":"requires_continuation","continuation_token":"AAAA"}}"#,
    ];
    let mut settling = Settling::new();
    let mut failure = None;
    for frame in frames {
        if let Err(error) = settling.consume_payload(frame) {
            failure = Some(error);
        }
    }
    assert_eq!(failure, Some(SettleError::NotFinal("requires_continuation".into())));
    assert_eq!(settling.settle().unwrap_err(), SettleError::Truncated);
}

#[test]
fn a_buffered_hop_that_requires_continuation_does_not_decode_into_a_turn() {
    let body = r#"{"id":"","status":"requires_continuation","continuation_token":"AAAA","steps":[
        {"type":"model_output","content":[{"type":"text","text":"1 2 3"}]}]}"#;
    assert_eq!(decode_interaction(body).unwrap_err(), ResponseError::NotFinal("requires_continuation".into()));
}
