# Repository instructions

Rust bindings for the Gemini Interactions API. [`SOUL.md`](SOUL.md) states the
mission and the design principles; obey it, and read it before changing a type.
This file is how to work here.

## Required checks

Before every commit:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo doc --no-deps` with no warnings

`#![deny(missing_docs)]` is on. Document *why* an item exists, not what it is.
Prove every impossibility claim with a `compile_fail` doctest, and check it
fails for the stated reason rather than incidentally.

## Where things live

Outbound: `content`, `step`, `tools`, `conversation`, `model` (one file per
model under `model/`), `request`.

Inbound: `frame`, `stream`, `settle`, `response`, `turn`, `usage`, `error`.

Shared: `values`, the enums mirroring API vocabularies, re-exported at the root.

Give each file one bounded mission and split it before 1,000 lines.

## Verifying against the live API

A captured real body beats an invented fixture. When a shape is in doubt,
capture it.

- Never print, log, or commit a credential. The live tests read the key from
  the file `GEMINI_API_KEY_FILE` names, or a git-ignored `.key`.
- The live tests are one module per modelled model; run one by its name. To
  run them against a gateway that speaks the same API, set `GEMINI_API_BASE` to
  its origin and `GEMINI_MODEL_PREFIX` to what its model ids begin with. Name
  neither in the repository.
- The free tier allows 20 requests a day per model and 5 a minute. Capture
  once, save the body under `tests/captured/` next to the request that made
  it, and test against the saved bytes. Never edit a captured file.
- State provenance in the test that reads it: model, prompt, date.
- `503 service_unavailable` is common under load; it is not a verdict on the
  request.

## The remaining gap, in priority order

1. **More models.** Gemini 3.8 Flash and Gemini 4 Argon have a type. Gemini
   3.1 Pro Preview (`low`, `medium`, `high`; default `high`), the 3.5–3.7
   Flash models (which accept `minimal`), and Gemini 4 Hydrogen and Carbon
   (listed by Google's own API on 2026-11-18, and refused by the gateway
   measured through; Hydrogen answered on the free tier, Carbon and Argon
   not) are next, each its own type with its own measured limits.
2. **Long decode.** Past what one request returns (64K tokens streamed, 32K
   not streamed on Gemini 4 Argon), the API ends a request with status
   `requires_continuation` and a `continuation_token`; the next request
   resends the same conversation with that token, stateless as it is. Decoding
   already refuses such a request as not final. Model the token on both sides
   and how the hops' steps join, and capture a real hop (it takes tens of
   thousands of generated tokens, and a request that is not streamed ran into
   a gateway's five-minute timeout on 2026-11-18). Raise the output caps then,
   not before.
3. **Thought summaries, captured.** Every request with `thinking_summaries:
   auto` and high thinking got a 503 on 2026-09-24, and again twice on
   2026-11-18. The settle rule — each `thought_summary` delta appends one
   block — is confirmed on a live stream of Gemini 3.8 Flash and of Gemini 4
   Argon through a gateway (one block per thought, in `streamed_thought_summary`),
   not yet captured from Google's own endpoint.
4. **Server-side tool steps, typed.** They decode and replay verbatim, but
   their views are only a step type. Model the call arguments and results of
   Google Search, URL context, code execution, Maps, file search and MCP, and
   capture each (Google Search returned `quota_exceeded` on the free tier).
5. **`safety_settings`.** The OpenAPI document references a `SafetySetting`
   schema it does not define. Model it once the reference does.
6. **`labels`**, with the Cloud label rules on keys and values.
7. **Output modalities other than text**: image, audio, video and speech
   configuration and response formats, on the models that produce them.
8. **Video processing segments**: the object form of `processing` with start
   and end offsets.
9. **Agents** (`agent`, `agent_config`, `environment`) and `webhook_config`.
   Agents need `background`, which needs storage, so they wait on a decision
   to change `SOUL.md`.

## Deliberately out of scope

Stateful continuation (`store: true`, `previous_interaction_id`, `background`),
the `generateContent` API, every other endpoint, and any HTTP client, runtime,
retry or reconnection. `SOUL.md` gives the reasons.

## Versioning and the changelog

Semantic versioning, pre-1.0, so a breaking change bumps the minor. Bump
`Cargo.toml`, add the `CHANGELOG.md` section, and keep `README.md` true in the
same commit as the change.

## Repository workflow

- Work on `main`. A multi-file change goes through a worktree on a branch,
  fast-forwarded onto `main` when its checks pass.
- The repository is public. Push only with the owner's authority.
- Name no gateway: not its operator, its host, the platform behind it, or the
  model identifiers it routes. Write "a gateway" and `gateway/<model>`, in code,
  fixtures, documentation, and commit messages alike.
