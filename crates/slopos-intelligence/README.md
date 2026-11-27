# slopos-intelligence

An opt-in, text-first preview of a desktop action planner. You type a request,
and a local language model proposes **one** typed action, such as opening an
installed application. The program asks you to confirm before it does anything.

It is a preview, not a system assistant. No speech, no screen reading, no
background process, and nothing starts at session start. The full trust boundary
and roadmap are in [docs/intelligence.md](../../docs/intelligence.md).

## How it works

- **Model:** uses an [Ollama](https://ollama.com) instance that is bound to
  loopback (`127.0.0.1:11434`), with the `lfm3:1.7b` model. No model is bundled.
- **Input:** the list of installed applications, from SLOPOS's own desktop-entry
  scanner, truncated to the first 80 entries and 8 KiB of labels.
- **Output:** strict JSON that must match one of the allowed action types. Any
  other output is rejected.
- **Confirmation:** a proposed launch runs only after you type `yes` on an
  interactive terminal (stdin and stdout must both be TTYs). Piping `yes` in does
  not authorise it.
- **Network limits:** the client talks only to loopback, follows no redirects,
  ignores proxy settings, has a 30-second timeout and caps responses at 1 MiB.

## Running it

```sh
ollama pull lfm3:1.7b
cargo run --locked -p slopos-intelligence -- --list
cargo run --locked -p slopos-intelligence -- "Open the calculator"
```

## Source files

- `src/lib.rs`: the action types, the JSON check and the application list.
- `src/main.rs`: the command line, the model client and the confirmation prompt.

## Tests

```sh
cargo test -p slopos-intelligence
```

The tests cover that only known action types and fields are accepted, that only
disclosed applications can be proposed, that the context stays bounded and does
not scan private user data, that the authorised application set matches the one
shown to the model, and that nothing runs without confirmation.

## Status and limits

- Not built: a window, voice, personal search, control of the UI through
  accessibility, and any execution beyond launching a listed application.
- It is not part of the session, and no other SLOPOS program depends on it.
