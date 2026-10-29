# SLOPOS Intelligence — text-first preview

**Status (2026-10-28):** working-code *preview* for Linux SLOPOS-I, not a
system-integrated Siri replacement. A separate opt-in `slopos-intelligence`
binary uses an **Ollama instance bound to loopback** and the
`lfm3:1.7b` model to propose one *typed* desktop action. No model or speech
engine is bundled. No process starts automatically.

## Try it (Linux, session or terminal)

Install Ollama separately, then fetch the model:

```sh
ollama pull lfm3:1.7b
cargo run --locked -p slopos-intelligence -- --list
cargo run --locked -p slopos-intelligence -- "Open the calculator"
```

The model must be running on `127.0.0.1:11434`; the client is hardwired to
loopback with HTTP redirects and proxy use disabled, a 30-second timeout,
and a strict 1 MiB maximum response body. A proposed launch requires typing
`yes` on an interactive terminal (both stdin and stdout must be TTYs); piping
`yes` to the command does not authorize execution. There is no autonomous
shell execution.

## Implemented threat boundary

1. Application metadata comes from SLOPOS's existing freedesktop application
   scanner, truncated to the first 80 scanned entries/8 KiB of labels; entries
   without an executable or with empty/overlong identifiers are excluded from
   the model's app list and the authorization set. The request is
   bounded to 2 KiB. No emails, contacts, browser history, documents, or
   screenshots are read or sent to inference.
2. The model returns strict JSON: `list_apps`, `open_app {app_id}`, or
   `help`. Unknown actions/fields/identifiers are rejected. Prompts and
   application labels are **untrusted**, not tool authorization.
3. Only an exact desktop-entry identifier from the disclosed list resolves.
   SLOPOS does not accept model-produced argv, paths, URLs, or shell scripts.
   Each launch requires explicit interactive confirmation; a denied or
   invalid action has no side effects.
4. The launcher invokes an installed desktop entry without a shell, using
   the existing SLOPOS process helper. Entries declaring `Terminal=true` are
   launched via `xfce4-terminal -e` or `xterm -e` (whichever is installed),
   and rejected if neither terminal is available. App IDs and names are
   escaped before printing to prevent terminal escape-sequence injection.
   A malicious installed desktop entry is still a local system trust issue;
   this is not a sandbox.
5. Stdio is the only interface for this preview. There is no background
   microphone, conversation memory, telemetry, or remote service.

## Next integration gates

- Versioned app-intent schemas (typed parameters/results) and a central
  permission store; confirm privileged/state-changing actions *at execution*.
- Desktop UI using `slopos-ui`, accessible transcript, local context search
  with explicit per-app opt-in and a per-session stop/cancel mechanism.
- Backend trait for local vision/audio and optional cloud inference; an
  on-device speech model is not an automatic requirement for low-memory
  devices.
- Authenticated IPC and app identities; no world-writable unauthenticated
  agent socket. Tests must cover prompt injection, forged app identities,
  revoked permissions, cancellation and result verification.
- PARAMA: shared versioned protocol and service contract only, after native
  Ring 3/IPC/capabilities; never place model inference into BRAHMA.

**Non-goals:** claiming this preview hears, sees, manages files, remembers
conversations, clicks GUI controls, or runs natively on PARAMA. All of those
need separate code and evidence.
