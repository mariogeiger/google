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
- The free tier allows 20 requests a day per model and 5 a minute. Capture
  once, save the body under `tests/captured/` next to the request that made
  it, and test against the saved bytes. Never edit a captured file.
- State provenance in the test that reads it: model, prompt, date.
- `503 service_unavailable` is common under load; it is not a verdict on the
  request.

## The remaining gap, in priority order

1. **More models.** Only Gemini 3.8 Flash has a type. Gemini 3.1 Pro Preview
   (`low`, `medium`, `high`; default `high`) and the 3.5–3.7 Flash models
   (which accept `minimal`) are next, each its own type with its own limits.
2. **Thought summaries, captured.** Every request with `thinking_summaries:
   auto` and high thinking got a 503 on 2026-09-24. The settle rule — each
   `thought_summary` delta appends one block — follows the reference and is
   not yet confirmed against a capture.
3. **Server-side tool steps, typed.** They decode and replay verbatim, but
   their views are only a step type. Model the call arguments and results of
   Google Search, URL context, code execution, Maps, file search and MCP, and
   capture each (Google Search returned `quota_exceeded` on the free tier).
4. **`safety_settings`.** The OpenAPI document references a `SafetySetting`
   schema it does not define. Model it once the reference does.
5. **`labels`**, with the Cloud label rules on keys and values.
6. **Output modalities other than text**: image, audio, video and speech
   configuration and response formats, on the models that produce them.
7. **Video processing segments**: the object form of `processing` with start
   and end offsets.
8. **Agents** (`agent`, `agent_config`, `environment`) and `webhook_config`.
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
