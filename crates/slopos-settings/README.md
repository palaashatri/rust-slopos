# SLOPOS Settings, GTK 3 (`slopos-settings`), legacy

The first unified settings window, built with GTK 3. It is **superseded** by
[`apps/control-panels`](../../apps/control-panels/README.md), which uses the
SLOPOS toolkit and covers the same panels.

## Status

- **Not launched.** Nothing in the session starts this program, and the top bar's
  System menu opens Control Panels.
- **Not installed.** `install.sh` and the live ISO do not install it.
- **Still built** in the workspace, so its tests and helpers keep running until
  they are moved or removed.
- **No new work here** (AGENTS.md rule 2). New settings go into Control Panels.

## What is in the crate

- `src/panels/`: the GTK 3 panels.
- `src/providers/`: the service adapters the panels used.
- `src/theme.rs`: the GTK theme and appearance choices.
- `src/main.rs`: the window.

No screenshots are provided, because the program is not shipped. See the
Control Panels README for the current screenshots of the same panels.

## Tests

```sh
cargo test -p slopos-settings
```
