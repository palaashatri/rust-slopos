# AGENTS.md — SLOPOS-I

SLOPOS-I is an experimental X11 desktop environment for Linux in a Classic Mac OS ("Platinum Classic") style: a Rust session manager, shell (global menu bar, launcher, notifications), Settings and a software catalogue, running under Openbox. It is **alpha** and not production-ready.

This file is the short, always-loaded instruction sheet for coding agents. The full requirements, architecture, application inventory, acceptance gates and evidence ledger are in [`docs/CONTRACT.md`](docs/CONTRACT.md) (§ numbers below refer to it). Read the contract sections relevant to your task before changing code; you do not need to read all 3,000 lines for a small fix.

## Current state (2026-10-20)

- The workspace has four crates, all legacy GTK3 "donor" code: `slopos-session`, `slopos-shell`, `slopos-settings`, `slopos-catalogue` (~9.8k lines of Rust).
- The target architecture (`slopos-ui` component library → `slopos-appkit` → native apps, Dock, applets, OSD, typed system providers) **does not exist yet** on `main`. See contract Part II for the subsystem table.
- `cargo build`, `cargo test` (76 tests), `cargo clippy -D warnings` and `cargo fmt --check` pass on `main`.
- `main` CI is red on one job: *AT-SPI translated-locale matrix (fr_FR.UTF-8)*.
- Unmerged work: `agent/spec-01-schema-inventory` (one commit adding the Classic spec JSON under `qa/spec/classic/`, no PR yet). `agent/qa-01-release-runner-inventory` is already in `main`, although contract Part II still lists it as unmerged. The UI gallery shown in the README screenshots was never committed — if you have that worktree, push it to a branch before doing anything else.

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
| `assets/`, `themes/` | GTK/Openbox themes, icons, sounds, wallpapers |
| `scripts/` | QA runners, architecture checkers and their pytest suites |
| `qa/` | QA inventories; `qa/reference/` is historical and non-normative |
| `packaging/` | Debian, Arch, ISO, VM and package-repo tooling |
| `docs/CONTRACT.md` | Full contract and evidence ledger |

## Decisions that are settled — do not reopen

- X11 + Openbox. No Wayland/Smithay, no custom display server in this generation.
- Rust, Platinum Classic geometry and behaviour. The Classic Macintosh UI Kit (Figma `LGMlwNCoVdakZxDBvPKg1W`) is the visual authority (§5); clean-room, no copied Apple assets.
- **UI library first**: build and verify `slopos-ui`, then `slopos-appkit`; apps and shell surfaces consume them. Do not add new first-party UI written directly in raw GTK (§2.2). The existing GTK crates may only get bug fixes, truthful-state fixes and incremental extraction (§N, transitional exceptions).
- One monorepo. The shell needs a Dock, global menu, status applets, launcher, desktop, notifications, workspaces and volume/brightness OSDs.

## Rules

1. **Never display invented system state.** No hard-coded interfaces, SSIDs, IPs, devices or levels. If a provider is missing, show "unavailable". Known violations to fix, not copy: `slopos-settings/src/panels/network.rs`, `panels/sound.rs`, and the static network labels and `xdotool` Cut/Copy/Paste fallbacks in `slopos-shell/src/topbar.rs` (contract Part II §D).
2. A missing UI control becomes a `slopos-ui` task, not an app-local workaround.
3. Don't weaken, skip or delete tests or CI jobs to get green. A skipped gate is reported as NOT_RUN, never PASS.
4. Don't claim a feature works because it compiles or a screenshot was captured. Screenshots in `docs/screenshots/` need a note saying what they do and don't show.
5. `Cargo.lock`, `Cargo.toml`, `rust-toolchain.toml` and CI changes must be deliberate and explained in the PR.
6. Don't add new planning/status Markdown files (`TRUTH.md`, per-agent ledgers). Update `docs/CONTRACT.md` Part II instead, in the same PR as the code.

## Git workflow

- **Never commit or push to `main`.** Work on `agent/<task-id>-<short-desc>` (or `claude/...`) branches and open a PR to `main`.
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
