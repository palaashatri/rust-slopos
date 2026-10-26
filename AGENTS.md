# AGENTS.md — SLOPOS-I

SLOPOS-I is an alpha X11 desktop environment in Rust. Openbox manages the windows; SLOPOS supplies the session supervisor, menu-bar shell, Dock, Control Panels, software catalogue, a native Calculator and its own UI toolkit.

Read these first:

- [docs/architecture.md](docs/architecture.md): what exists and how it fits together. Trust it over older docs.
- [docs/development.md](docs/development.md): commands, running from the source tree, writing a toolkit app.
- [docs/CONTRACT.md](docs/CONTRACT.md): the target requirements, split by area, plus the evidence [ledger](docs/contract/ledger.md). "§N" means a contract section; the index says which file it is in.

## Before you push

Run the same checks CI runs:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

python3 -m venv .venv && .venv/bin/pip install -r qa/requirements-architecture.txt
.venv/bin/python -m pytest -q scripts/test_check_architecture.py \
  scripts/test_classic_source_mapping.py scripts/test_release_runner_inventory.py
python3 scripts/check-architecture.py          # apps/ must not depend on GTK
python3 scripts/check-classic-source-mapping.py
python3 scripts/check-release-runner-inventory.py

bash -n <every shell script you changed>
cargo build --workspace --locked && bash scripts/smoke-session.sh   # needs Xvfb, openbox, xdotool
```

For a quicker loop on one crate, use `cargo test -p <crate>`.

## Rules

1. **Show real state only.** Status, devices, networks and levels come from `slopos-services` providers. If a provider is missing, say "unavailable". Never hard-code a value or parse ad-hoc command output. The tests in `crates/slopos-shell/tests/shell_tests.rs` guard against the old fake strings; keep them.
2. **New UI uses the SLOPOS toolkit** (`crates/slopos-ui`), not GTK. Nothing under `apps/` may depend on GTK (`check-architecture.py` enforces this). The GTK 3 crates (`slopos-shell`, `slopos-settings`, `slopos-catalogue`) get bug fixes and gradual replacement, not new GTK surfaces.
3. **A control that's missing goes into the toolkit**, with a state machine, tests and an accessibility node. Don't build a one-off inside an app.
4. **No busy loops or polling timers.** Block on events (X11 `wait_for_event`, poll(2), D-Bus signals) and wake threads explicitly. Idle CPU matters, because the low-end target is a Raspberry Pi Zero 2 W.
5. **Launch programs with `slopos_core::process::launch`.** It detaches the program and reaps it. Don't hard-code one terminal or editor; use the candidate lists in `process.rs`.
6. **Every behaviour change gets a test.** Keep logic in plain modules that can be tested without X. Test UI by sending events and checking pixels or accessibility trees. If something can only be checked on a display, add it to `smoke-session.sh`.
7. **Don't weaken tests or CI to get green.** If a check can't run, report it as not run, never as passed.
8. **Make claims only from evidence.** "Compiles" or "a screenshot exists" doesn't mean it works. Update `docs/architecture.md` when what exists changes, and add dated evidence to the ledger.
9. **Clean-room design.** The Classic Macintosh UI Kit is a geometry and behaviour reference. Ship no Apple assets. Follow the Design Bible (§5.2) and its review checklist for visible UI.
10. **Explain dependency and CI changes** in the commit or PR: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and `.github/`.

## Settled decisions

- X11 and Openbox for this generation. No Wayland compositor work without the owner's go-ahead (§21.10 lists when to raise it).
- SLOPOS owns its toolkit: tiny-skia, cosmic-text, resvg and x11rb, with AccessKit for accessibility. Keep X11 types out of public toolkit APIs other than `platform`.
- One monorepo. Loom and PARAMA are separate projects with their own repositories and gates.
- Future goals (SLOPOS Intelligence, Modern style, compositor, PARAMA desktop, edge tiers) are in §21.6–21.10. They are planned, not current work, unless the owner asks for them.

## Git

- Work on a branch and open a PR to `main`. Don't push to `main` unless the owner says so.
- Keep commits small, with messages that say what changed and why.
- In the PR, list what you ran and what you couldn't run.
