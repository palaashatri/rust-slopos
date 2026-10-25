# AGENTS.md — SLOPOS-I

SLOPOS-I is an experimental X11 desktop environment for Linux in a Classic Mac OS ("Platinum Classic") style: a Rust session manager, shell (global menu bar, launcher, notifications), Settings and a software catalogue, running under Openbox. It is **alpha** and not production-ready.

This file is the short, always-loaded instruction sheet for coding agents. The full requirements, architecture, application inventory, acceptance gates and evidence ledger are in [`docs/CONTRACT.md`](docs/CONTRACT.md) (§ numbers below refer to it). Read the contract sections relevant to your task before changing code; you do not need to read all 3,500 lines for a small fix.

## Current state (2026-10-20, after the all-branches integration)

- Eight workspace packages: the four legacy GTK3 "donor" crates (`slopos-session`, `slopos-shell`, `slopos-settings`, `slopos-catalogue`) plus the partial bootstrap `slopos-core`, `slopos-services` (NetworkManager, audio, brightness, power, timedate, Bluetooth, MPRIS, UDisks providers), `slopos-ui` (button plate/button, layout, accessibility base) and `tools/slopos-ui-gallery`. There is still no `slopos-appkit`, Dock, applet framework, OSD or native app.
- Verified at `b332657` in a container (Rust 1.97, because the pinned 1.99.0 couldn't be downloaded there): build, **205 tests passed / 11 ignored**, clippy `-D warnings` and fmt all pass. Contract Part II.L, *2026-10-20 post-integration container check*, has the details.
- **CI on `main` is red in 4 jobs**, each with a known cause:
  1. *lockfile consistency* — two pytest cases broke in the spec-01 merge (F-QA-08). Fix the tests, not the checker.
  2. and 3. *.deb* and *pkg.tar.zst payload contract* — the session descriptor runs `slopos-session` instead of `start-slopos-i` (F-PKG-01). This is a real bug: display-manager logins get no global-menu export and no desktop icons. The Arch job also lacks `bsdtar` on the new Ubuntu 26 runner (F-CI-01).
  4. *Debian Live ISO* — failing since before the merge; cause unknown.
- Fabricated network/sound state and the `xdotool` menu fallbacks are gone. Still open: the About dialog in `topbar.rs` calls the desktop "consumer-ready" (F-CLAIM-01), and Settings shows no live network state even though `slopos-services::network` can provide it.
- All agent branches are merged; they are kept only for history. Older `qa/` data files that cite `AGENTS.md §N` mean `docs/CONTRACT.md` §N.

## Commands

Build dependencies (Ubuntu/Debian):

```bash
sudo apt-get install -y build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev libssl-dev libdbus-1-dev
```

Everyday checks — the same ones CI's *build + test + lint* job runs:

```bash
cargo build --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Focused checks for one crate: `cargo test -p slopos-shell --locked`, `cargo clippy -p slopos-shell --all-targets --locked -- -D warnings`.

Architecture/QA checkers (Python, see `qa/requirements-architecture.txt`):

```bash
python3 scripts/check-architecture.py
bash -n scripts/<changed-script>.sh      # syntax-check any shell script you touch
```

Graphical and integration QA lives in `scripts/run-*-qa.sh` (Xvfb/Openbox, AT-SPI, resolution, multi-monitor, packaging). Read a script before running it: several use fixed displays (`:90`, `:95`), hard-coded `target/release` paths and name-based `pkill`, so run them **one at a time** in a disposable session (contract §0.6, §30.2).

## Where to run things

- On the owner's machines, build, test, lint, package and run SLOPOS **inside a Linux VM** (UTM/QEMU/VirtualBox/VMware; Ubuntu LTS preferred), with the clone on guest storage, never on the macOS host (§28). Installing the session (`sudo ./install.sh --distro ubuntu|arch`) modifies the system — VM only.
- Disposable cloud/CI containers are fine for `cargo` builds and unit tests. They are not evidence of graphical, hardware or install behaviour.
- Keep an eye on disk: Rust targets and VM images grow fast (§29). Never `cargo clean` or delete shared caches, other worktrees or other agents' targets.

## Repository layout

| Path | What it is |
|---|---|
| `crates/slopos-session` | X11 session supervisor (restart/backoff) |
| `crates/slopos-shell` | Top bar + global menu, launcher, app finder, notifications, X11/EWMH helpers (`src/x11/`), services (`src/services/`) |
| `crates/slopos-settings` | Legacy GTK Settings panels (`src/panels/`) and provider stubs (`src/providers/`) |
| `crates/slopos-catalogue` | AppImage catalogue/installer |
| `crates/slopos-core`, `crates/slopos-services`, `crates/slopos-ui` | Partial bootstrap of the target architecture (shared primitives, D-Bus providers, component library) — see contract Part II for what is and is not verified |
| `tools/slopos-ui-gallery` | Component gallery (geometry QA fixture, not a conformance result) |
| `qa/spec/classic/` | Classic component spec dataset + source-mapping checker; `qa/spec/classic-guest-extraction/` is the separately relocated guest-extracted dataset; `qa/reference/` is historical and non-normative |
| `assets/`, `themes/` | GTK/Openbox themes, icons, sounds, wallpapers |
| `scripts/` | QA runners, architecture checkers and their pytest suites |
| `qa/` | QA inventories and release-runner inventory |
| `packaging/` | Debian, Arch, ISO, VM and package-repo tooling |
| `docs/CONTRACT.md` | Full contract and evidence ledger |

## Decisions that are settled — do not reopen

- X11 + Openbox. No Wayland/Smithay, no custom display server in this generation.
- Rust, Platinum Classic geometry and behaviour. The Classic Macintosh UI Kit (Figma `LGMlwNCoVdakZxDBvPKg1W`) is the visual authority (§5); clean-room, no copied Apple assets.
- **UI library first**: build and verify `slopos-ui`, then `slopos-appkit`; apps and shell surfaces consume them. Do not add new first-party UI written directly in raw GTK (§2.2). The existing GTK crates may only get bug fixes, truthful-state fixes and incremental extraction (§N, transitional exceptions).
- One monorepo. The shell needs a Dock, global menu, status applets, launcher, desktop, notifications, workspaces and volume/brightness OSDs.

## Future feature goal: SLOPOS Intelligence (§21.6)

- Plan an **opt-in, local-first desktop operator** that visibly opens/navigates real apps, clicks/types, captures screens and verifies multistep tasks, with text first and voice later.
- Prioritize typed `slopos-services`/`slopos-appkit` actions, then AT-SPI2 accessibility, then bounded screenshot/vision-assisted X11 input as a fallback. Keep the planner backend-agnostic; X11/Openbox remains the only permitted SLOPOS-I session.
- Require per-task authorization, clear progress, immediate stop, explicit confirmation of sensitive/destructive/external actions and truthful verification; never silently export screen contents or grant unrestricted execution.
- **PLANNED / NOT IMPLEMENTED and not release-blocking.** Do not divert the current UI/appkit, native app, shell and CI milestones. The authoritative target, task IDs AI-01–AI-05 and acceptance criteria are in `docs/CONTRACT.md` §21.6.

## Cross-generation feature goals (§21.7)

The full SLOPOS platform vision is captured in \`docs/CONTRACT.md\` §21.7 and **does not change the SLOPOS-I release gates**. Future, not-yet-implemented tracks: a first-party accessible UI toolkit with consistent **System 7/Platinum Classic** and optional **Loom Design Bible**-inspired whole-desktop themes; Loom Writer/Sheets/Present integration while preserving Loom's own Rust/Slint and cross-platform status; local-first Vision/Live Text, Copy Subject, Visual Look Up, dictation/writing assistance; permissioned Bixby-style desktop automation (§21.6); an independently proven SLOPOS-owned compositor with HDR/VRR, Spaces and live wallpapers; and **optional PARAMA hosting only once its real kernel/user-space/security/driver/graphics gates exist**.

Keep Loom, PARAMA and SLOPOS separate. Do not implement future work on the current SLOPOS-I branch without an owner-approved scoped task, bypass \`slopos-ui\`/appkit requirements, falsely mark third-party compatibility, or silently turn planned goals into product readiness claims. Future task identifiers: FUT-UI, FUT-LOOM, FUT-VISION, FUT-COMP, FUT-PARAMA.

## Rules

1. **Never display invented system state.** No hard-coded interfaces, SSIDs, IPs, devices or levels. If a provider is missing, show "unavailable". Read state through `slopos-services` providers, never by parsing ad-hoc command output. `crates/slopos-shell/tests/shell_tests.rs` asserts the old fake strings stay gone; keep those tests.
2. A missing UI control becomes a `slopos-ui` task, not an app-local workaround.
3. Don't weaken, skip or delete tests or CI jobs to get green. A skipped gate is reported as NOT_RUN, never PASS.
4. Don't claim a feature works because it compiles or a screenshot was captured. Screenshots in `docs/screenshots/` need a note saying what they do and don't show.
5. `Cargo.lock`, `Cargo.toml`, `rust-toolchain.toml` and CI changes must be deliberate and explained in the PR.
6. Don't add new planning/status Markdown files (`TRUTH.md`, per-agent ledgers). Update `docs/CONTRACT.md` Part II instead, in the same PR as the code.

## Git workflow

- **Never commit or push to `main` without an explicit instruction from the repo owner.** Work on `agent/<task-id>-<short-desc>` (or `claude/...`) branches and open a PR to `main`.
- Small, reviewable commits. Re-run the four cargo checks before pushing.
- In the PR description, list the commands you ran and their results, what you could not run (e.g. graphical QA) and why.
- If several agents work in parallel, give each its own worktree and target directory, and serialise anything that needs a display, D-Bus name, audio/network change or package install (§0.3–0.6).

## Definition of done for a change

- The four cargo checks pass locally; changed shell scripts pass `bash -n`.
- New behaviour has a test, or the PR explains why it can't (e.g. needs real hardware).
- No new fake state, no new raw-GTK first-party UI.
- If the change closes or adds a known defect, `docs/CONTRACT.md` Part II is updated.

## Further reading in `docs/CONTRACT.md`

| Task | Sections |
|---|---|
| UI library, spec, gallery | §2, §5, §9–11, §25, §27 |
| Appkit and native apps | §12, §15–19 |
| Shell, providers, X11 | §13–14, §18, §22–24 |
| QA, release, evidence | §28–33, Part II |
| Production-readiness definition | §35–37 |
| Future SLOPOS Intelligence feature goal | §21.6 (AI-01–AI-05) |
| Broader SLOPOS platform vision, Loom/Vision/compositor/PARAMA | §21.7 (FUT-*) |
