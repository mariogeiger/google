# Changelog

## 0.4.0 — 2026-11-18

An interaction that stops at one request's output cap is no longer read as a
finished answer.

### Breaking: `requires_continuation` is a status, and not a final one

The reference lists the status `requires_continuation`: a request reached the
output cap its model allows a single request, with more of `max_output_tokens`
still to spend, and the rest of the answer comes from a further request that
carries the `continuation_token` of this one. It decoded as
`Status::Unrecognized`, which counts as final, so half an answer settled into a
turn. `KnownStatus::RequiresContinuation` names it, `Status::is_final` says
which statuses end a turn, and both `Settling::settle` and `decode_interaction`
refuse the others with `NotFinal`. The token itself is not modelled yet; the
long decode it serves is the second gap in `AGENTS.md`.

**Migration.** A `match` on `KnownStatus` needs an arm for
`RequiresContinuation`, and one on `ThinkingLevel` an arm for `Xhigh` (below).
A caller that read the turn of an interaction with that status now gets the
`NotFinal` error instead.

### Also

- `ThinkingLevel::Xhigh`, the fifth value of the reference's vocabulary.
  Neither Gemini 3.8 Flash nor Gemini 4 Argon accepts it, nor `minimal`: the
  API answers each with a 400 (measured 2026-11-18, through a gateway), so
  their types have neither, and `compile_fail` doctests say so.
- Gemini 4 Argon's output limits, re-measured 2026-11-18. The refusal "Long
  decodes are not supported using the Interactions API" is gone: the API now
  accepts a cap up to 1,048,576, streamed or not, which Google's long-decode
  guide serves with continuation requests. The 65,536 streamed and 32,768
  unstreamed limits stay as what one request returns whole, until the
  continuation is modelled. Gemini 3.8 Flash's 65,536 still holds, streamed or
  not (a cap of 65,537 is refused in both).
- The live tests are one module per modelled model, and can run against a
  gateway that speaks the same API (`GEMINI_API_BASE`, `GEMINI_MODEL_PREFIX`).
  Each model's module checks a function round trip that replays its thought, a
  buffered text, a streamed thought summary, and that the API still refuses the
  thinking levels its type lacks. All passed 2026-11-18 on Gemini 3.8 Flash and
  Gemini 4 Argon.
- Thought summaries confirmed on a live stream of both: one `thought_summary`
  delta per block, which settles into the thought that precedes the answer.

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
