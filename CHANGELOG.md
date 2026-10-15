# Changelog

## 0.3.0 — 2026-10-15

A restriction to named tools can no longer be written in a mode the API
refuses.

### Breaking: `allowed_tools` takes an `AllowedToolsMode`

The API answers a restriction in `auto` or `none` mode with "allowed_tools
mode must be either ANY or VALIDATED" (measured on Gemini 3.8 Flash and
Gemini 4 Argon). `Conversation::allow_tools`, `AllowedTools::unchecked` and
`AllowedTools::mode` now take or return `AllowedToolsMode`, which has only
`Any` and `Validated`. `ToolChoiceMode` still says the mode of a choice that
names no tools.

**Migration.** Pass `AllowedToolsMode::Any` for `ToolChoiceMode::Any`, and
`AllowedToolsMode::Validated` where `ToolChoiceMode::Auto` meant "the model
decides, among these tools".

## 0.2.1 — 2026-10-15

- `Gemini4Argon`, with its three thinking levels (`low`, `medium` by default,
  `high`; `minimal` is refused by the API and absent from the type), thought
  summaries, a response format, and an output cap of at most 65,536 tokens.
  A request that is not streamed refuses, as its body is written, a cap above
  the 32,768 tokens the API allows one unstreamed response.

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
