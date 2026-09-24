# Changelog

## 0.2.0 — 2026-09-24

A consumer that stores conversations can now replay them.

- `ModelStep::from_wire` is public, and `ModelStep` implements `Deserialize`:
  a step stored as the JSON it arrived as decodes back to the same step. A
  `user_input` or `function_result` object is refused as a model step.
- `Conversation::push_model_steps` appends decoded steps under the same rule
  as `push_turn`.
- `StreamEvent::from_value` decodes a frame an SSE parser already read as JSON.

### Breaking: `ConversationError::NotAwaitingModel` is gone

A model turn may now follow a model turn; only an unanswered function call
blocks one. The API documents no rule against it, and `Request::new` still
refuses to ask the model when the history ends with its own step.

**Migration.** Remove any match arm for `ConversationError::NotAwaitingModel`.

## 0.1.0 — 2026-09-24

First release: typed, stateless bindings for `POST /v1beta/interactions`.

- A `Conversation` holding the system instruction and tools fixed, and the
  history append-only. Model steps replay verbatim; function results take
  their `call_id` and `name` from the call they answer; a user message or a
  new model turn is refused while a call is unanswered.
- `Gemini3_8Flash`, with its three thinking levels, thought summaries, an
  output cap checked against its 65,536-token limit, and a text response
  format.
- All eight tool declarations, and tool choice by mode or by named tools.
- User input in text, image, audio, PDF/CSV and video; function results as a
  string, a JSON object or text and image blocks.
- A stream decoder for every documented event and delta kind, a settler that
  refuses a truncated stream, a buffered-body decoder, full usage with
  pointwise joins, and error bodies in both their JSON and SSE shapes.
- Tests against bodies captured live on 2026-09-24. The streamed and buffered
  live tests passed end to end the same day against Gemini 3.6 Flash, with
  only the model id substituted, after the free tier's daily limit for Gemini
  3.8 Flash was spent on the captures.
