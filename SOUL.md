# Soul

Represent the entire Gemini Interactions API faithfully in types, statelessly.

That is why this crate exists. Not a convenience wrapper, not the subset one
consumer happens to need: the whole wire of `POST /v1beta/interactions`, in
both directions, as types that admit exactly what the API admits. A request
the API would answer with a 400 should not compile, and a response the API can
send should decode.

## The mission, stated precisely

*Faithfully* cuts three ways.

**Complete.** Every field, step, delta and enumerated value the documented API
carries has a place here. A gap is a defect, not a scope decision, and the gaps
are listed in `AGENTS.md` rather than rediscovered.

**Exact.** A closed set of wire strings is an enum, a range-checked number is
checked where it enters, and a combination the API refuses is a combination
that cannot be written.

**Honest.** The crate reports the API, not a probe's opinion of it. Google's
reference — the OpenAPI document at
`https://ai.google.dev/static/api/interactions.openapi.json` and the guides —
is the specification. A 200 is not evidence of legality: the endpoint accepted
a function result with an unknown `call_id`, and a `temperature` the reference
does not have. A 400 *is* evidence, so live probes are for measuring what the
API rejects.

## Stateless, by decision

The Interactions API stores interactions by default and continues them with
`previous_interaction_id`. That hands the conversation, and with it the cached
prefix, to the server, where no type can guard it. So every request here sends
the whole history with `store: false`, and neither `store: true` nor
`previous_interaction_id` nor `background` (which needs storage) exists.

Stateless replay has one hard rule, and the crate is built around it: the
steps the model produced are sent back exactly as received. A thought step
carries an opaque signature of the model's reasoning; dropping it is a 400.
So a model step keeps the JSON object it arrived as and serializes that; its
typed view is for reading only, and there is no public constructor. A model
step comes from decoding or not at all.

## Both directions, no transport

The crate produces a serializable request body and decodes what comes back. No
HTTP client, no retry logic, no reconnection. Callers bring their own stack.

## Design principles

### A type per model, so a refused parameter cannot be written

Models accept different parameters: Gemini 3.8 Flash refuses the `minimal`
thinking level that Gemini 3.6 Flash accepts, and a text-only model has no
speech or image configuration. So each model is its own type, carrying only
what it accepts, and adding a model means a new type, never widening one.

### No invented defaults, no normalization

A parameter with a documented default is a plain field whose `Default` is that
value, and it is always emitted: the body is then a complete record of what the
model sees, and stays one when Google changes a default. A parameter with no
documented default is an `Option`, and absence means the caller said nothing.

### An invariant is held by the type, or it is not held

A closed vocabulary is an enum. A cross-field or cross-step invariant means
private fields and a checking constructor: a function result takes its
`call_id` and `name` from the call it answers, and a conversation refuses a
user message while a call is unanswered. Each impossibility claim is proved by
a `compile_fail` doctest that fails for the reason it states.

### The prefix is append-only

Caching is implicit and prefix-based: the system instruction, then the tools,
then the steps. A conversation fixes the first two at construction and only
appends to the third. There is no API for rewriting history. Narrowing which
tools are callable is a per-call tool choice, which leaves the tool array — and
the cache — untouched.

### Unknown is not broken; incomplete is a different type from complete

Google may add events, delta kinds, step types and enum values. An unknown one
decodes as `Unrecognized`, and an unknown step type still replays verbatim. What
is an error is a frame that contradicts the schema.

A stream cut off early must not read as an answer. The accumulator cannot
yield a turn, a turn cannot take more events, and the one bridge fails unless
`interaction.completed` arrived with a final status.

### Usage is evidence

The only sign the implicit cache worked is `total_cached_tokens`. Usage
therefore decodes fully, absent counters read as zero, and the several usage
objects a stream sends merge by pointwise maximum.
