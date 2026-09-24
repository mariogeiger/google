# google

Strictly typed, cache-safe Rust bindings for Google's **Gemini Interactions
API** (`POST /v1beta/interactions`), used statelessly.

The crate builds a `Serialize` request body and decodes what comes back, streamed
or buffered. It has no HTTP client: bring your own.

What the types rule out:

- **Parameters a model refuses.** Each model is its own type. Gemini 3.8 Flash
  has no `minimal` thinking level, because the API answers it with a 400.
- **Broken replay.** In stateless use, the model's steps must go back exactly as
  received, thought signatures included. A model step keeps its received JSON,
  and it has no public constructor.
- **Orphan function calls.** A function result takes its `call_id` and `name`
  from the call it answers. A new message waits until every call has a result.
- **Cache drift.** The system instruction and tools are fixed. History is
  append-only. Tool choice is per call.
- **Half answers.** A stream that ends before `interaction.completed` does not
  settle into a turn.

```rust
use google::conversation::Conversation;
use google::model::Gemini3_8Flash;
use google::request::Request;
use google::settle::Settling;

let mut conversation = Conversation::new(Some("Be brief.".into()), vec![])?;
conversation.push_user_text("What is 17 × 23?")?;
let body = serde_json::to_string(&Request::new(&conversation, Gemini3_8Flash::new())?.streaming())?;

// POST `body` to google::API_BASE + google::INTERACTIONS_PATH with the
// `x-goog-api-key` header, then feed the response line by line:
let mut settling = Settling::new();
for line in response_lines { // what your HTTP client yields
    settling.consume_line(line)?;
}
let turn = settling.settle()?;
println!("{}", turn.text());
conversation.push_turn(turn)?;
```

See [`SOUL.md`](SOUL.md) for the design, and [`AGENTS.md`](AGENTS.md) for how
to work on it and what is not covered yet.

License: MIT.
