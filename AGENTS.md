# AGENTS.md — SLOPOS-I

**ONE-SHOT GOAL: If asked to "deliver SLOPOS X11" or handed an unbounded
coding objective, start at [GOAL.md](GOAL.md), then
[docs/autonomous-execution.md](docs/autonomous-execution.md).**
Run `python3 scripts/p1_goal.py validate`, `status`, `next` before coding
(Windows: `py -3`). Follow the manifest dependency graph, checkpoint
progress, review real GUI screenshots, and enforce storage limits.
Do not claim P1 done from a green Cargo build alone.


SLOPOS-I is an alpha X11 desktop environment in Rust. Openbox manages the windows; SLOPOS supplies the session supervisor, menu-bar shell, Dock, Control Panels, software catalogue, a native Calculator and its own UI toolkit.

Read these first:

- [docs/architecture.md](docs/architecture.md): what exists and how it fits together. Trust it over older docs.
- [docs/development.md](docs/development.md): commands, running from the source tree, writing a toolkit app.
- [docs/p1-x11-release.md](docs/p1-x11-release.md): **P1 priority and release gates**, native Files, Image Viewer, Media, Control Panels, Software, and status/OSD.
- [docs/branch-preservation.md](docs/branch-preservation.md): preserved Wayland and design snapshots; do not delete archive branches.
- [docs/CONTRACT.md](docs/CONTRACT.md): the target requirements, split by area, plus the evidence [ledger](docs/contract/ledger.md). "§N" means a contract section; the index says which file it is in.

## Before you push

Run the same checks CI runs:

```bash
cargo metadata --locked --format-version 1 --no-deps > /dev/null   # Cargo.lock matches Cargo.toml
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

## Shared UI toolkit development (owner-directed)

SLOPOS is the **first host and compatibility reference**, not the only
permitted consumer. The toolkit must ultimately serve external MIT-licensed
Rust applications without requiring a SLOPOS session, X11 or a Linux desktop.

1. Keep scene descriptions, animation sampling, layout state and design tokens
   independent of X11, Openbox, GTK, D-Bus, GPU device handles and global state.
   The existing `slopos-ui::platform` module is a host adapter, never a
   dependency of portable toolkit foundations.
2. Maintain the current `RasterSurface` software path as the pixel-exact
   reference for Classic themes. Introduce GPU/Vello/wgpu backends only behind
   optional features after screenshot, geometry, alpha, text and clipping
   parity tests exist. Do not change the existing Calculator/Dock output
   merely to add an abstraction.
3. Keep renderers event driven. No animation frame timer when nothing is
   moving. Motion must respect reduced-motion preferences; effect complexity,
   caches and draw-list sizes must be bounded for the 512 MB Pi Zero 2 W goal.
4. Cross-platform public APIs must use toolkit-owned types rather than
   platform SDK types. Provide native host adapters and isolate any FFI.
5. Every control needs keyboard navigation, focus behavior, pointer
   interactions, accessibility semantics and corresponding headless tests.
   Every renderer needs reference pixels, malformed geometry and scale tests.
6. First-party toolkit source is MIT. Record actual transitive dependency,
   font, icon, shader and model licenses before reuse; an MIT crate must not
   imply all distributed components are MIT.
7. **Keep the toolkit and native applications in the SLOPOS monorepo** as
   the default long-term home. Preserve a standalone, independently usable
   public Rust crate API: no shell, X11 or SLOPOS session requirements for
   general-purpose apps. Extracting a repository is optional, requires
   explicit owner approval, and is NOT a milestone or automatic action.
   Do not move existing apps or replace the compositor in this foundation PR.
8. Keep `docs/architecture.md` factual; use
   [docs/ui-toolkit-program.md](docs/ui-toolkit-program.md) for rendering
   and toolkit milestones, [docs/application-platform.md](docs/application-platform.md)
   for app/surface ownership and vertical slices, and
   [docs/swarm-execution.md](docs/swarm-execution.md) for agent packets,
   dependencies, write leases and merge gates.

## Parallel SLOPOS native-application development (owner-directed)

The toolkit, desktop shell and applications form **one SLOPOS platform**,
implemented as *small, dependency-ready vertical slices* rather than as
twelve simultaneous full rewrites. Treat contract §§15–18 as the complete
product inventory and the documents below as the dispatch instructions:

- [docs/application-platform.md](docs/application-platform.md): canonical
  paths, shared service/control ownership, CPU/GPU and native-app waves.
- [docs/historical-recovery-audit.md](docs/historical-recovery-audit.md):
  audited branches, historical compositor, old toolkit/app reuse candidates
  and explicit do-not-cherry-pick decisions.
- [docs/swarm-execution.md](docs/swarm-execution.md): task IDs, dependency
  graph, exclusive path ownership, task packet template, coordinator
  handoff, QA gates, and ready-to-use kickoff instructions.
- [docs/contract/00-agent-workflow.md](docs/contract/00-agent-workflow.md):
  authoritative Linux VM execution, resource/runtime leases, build
  concurrency and evidence format.

**Coordinator rule:** one integrator alone edits shared UI public APIs,
workspace manifests, module registries, central design tokens, packages,
CI and policy docs, unless it explicitly grants exclusive ownership.
Assign each worker distinct source/test paths and an immutable base SHA.
Allow model/provider/test work in parallel while required UI controls are
integrated; never let an app invent private versions of missing controls.
The **coordinator** may autonomously delegate at most one independent
leaf task with exclusive paths and an exact base SHA, if the IDE has
subagents; no recursive spawning. One Cargo build and one X11 runtime
lease at a time. No agent pushes directly to main.

**First target slices:** Files navigation; Image Viewer opening/zoom;
Control Panels live Network/Sound; Software trusted catalog/transaction;
Media decoder state/playback; Wi-Fi/Bluetooth/clock popovers and volume/
brightness OSD. The already working Calculator and Dock are protected
regressions. Path names follow contract §16–17 (e.g. `apps/image-viewer`,
`apps/media`, `apps/control-panels`); do not invent duplicate app trees.

**Merger acceptance:** real keyboard/pointer interactions, accessible
focus/semantics, app failure/cancel behavior, truthful provider state,
bounded CPU/memory, license/provenance audit, and VM session/package
tests when the change touches the desktop. Missing real hardware must
be marked NOT_RUN, never PASS. The app suite roadmap is NOT a claim that
all these applications, renderer paths or non-Linux hosts already exist.

## P1-first merge and agent priorities (2026-10 owner instruction)

**Ship SLOPOS X11/Openbox first** with a stable native Rust `slopos-ui`
application set: Calculator, Files, Image Viewer, Media, Control Panels,
Software and basic shell status/OSDs. The exact phase/gate list is in
[docs/p1-x11-release.md](docs/p1-x11-release.md). No Loom, Donkey, PARAMA,
custom compositor, or unapproved GPU backend work may displace these.

The existing SLOPOS Design Bible lives at
[docs/contract/02-design.md](docs/contract/02-design.md) §5.2. Do not
create an inconsistent duplicate. The integrated image viewer is an
initial **PNG/SVG proof app**, not a complete photo browser or package-ready
production release. App workers must extend verified functionality, not
promote proof UI to done status. The current compiler/test gates remain
mandatory and run in the designated VM or CI.

Historical Wayland/renderer/app source and design proposals have redundant
archive branches documented in
[docs/branch-preservation.md](docs/branch-preservation.md).
**Never merge or delete those refs as a shortcut**; selected pure policy,
widget behavior, image/file logic and tests may be adapted only after
security/license/API reviews and current exact-SHA verification. Do not
revive the historical compositor during P1.

## Native Files handoff (PR #35)

`apps/files` (PR #35's model plus the FILES-02 operations, Trash and
menus on the shared `slopos-ui::controls::list::ListView`) is the Files
baseline. **Do not start Files from scratch.** Remaining work extends it:
pagination over 8192 rows, drag and drop, icon view, thumbnails,
properties and removable volumes. Follow
[docs/p1-files-live-qa.md](docs/p1-files-live-qa.md) for required VM
interaction and screenshot evidence; do not call Files shipped until those
and the full P1 file operations pass. Preserve the PCManFM donor until
parity is demonstrated.
