# Developing SLOPOS-I

## Prerequisites

Ubuntu or Debian:

```bash
sudo apt-get install -y build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev \
  libssl-dev libdbus-1-dev
# For the session smoke test and the X11 QA scripts:
sudo apt-get install -y xvfb openbox xdotool imagemagick dbus-x11 pcmanfm
```

The Rust version is pinned in `rust-toolchain.toml` (currently 1.99.0); `rustup` installs it on first use.

## Everyday checks

These are the commands CI runs (`.github/workflows/ci.yml`):

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

cargo build --workspace --locked
bash scripts/smoke-session.sh                  # real session under Xvfb, ~30 s
```

`smoke-session.sh` uses display `:97` (override with `SMOKE_DISPLAY`) and a throwaway `HOME`. It checks:

- the menu bar and the Dock appear;
- a menu opens;
- Calculator opens, the Dock adds a tile for it, and destroying its window ends the process;
- logging out ends every session process.

It writes screenshots and logs to `artifacts/smoke/` and is safe to run on a desktop machine.

## Running things from the source tree

| What | Command |
|---|---|
| Calculator | `cargo run -p slopos-calculator` |
| Calculator, headless PNG | `cargo run -p slopos-calculator -- --snapshot calc.png` |
| Component gallery | `cargo run -p slopos-ui-gallery -- --component buttons --scale 2` |
| Dock on the current display | `cargo run -p slopos-dock` |
| Whole session in a nested X server | `Xephyr :5 -screen 1280x800 & DISPLAY=:5 SLOPOS_SESSION_BIN=$PWD/target/debug/slopos-session dbus-run-session -- scripts/start-slopos-i` |

These environment variables change a session's behaviour:

| Variable | Effect |
|---|---|
| `SLOPOS_SCALE` / `GDK_SCALE` | Integer UI scale, 1 to 4. |
| `SLOPOS_ICON_THEME` | Icon theme for toolkit apps. The default is `SLOPOS-Platinum`. |
| `SLOPOS_NO_COMPOSITOR` | Do not start picom/xcompmgr. |
| `SLOPOS_NO_DOCK` | Start the session without the Dock. |
| `SLOPOS_OPENBOX_CONFIG` | Use this Openbox rc file. |
| `RUST_LOG=debug` | Verbose logs. |

## Writing a toolkit app

1. Create `apps/<name>` and add it to the workspace `members`.
2. Depend on `slopos-ui` (and `slopos-core` if needed). Do not depend on GTK; the architecture check fails if you do.
3. Implement `slopos_ui::platform::App`:
   - `event` handles input and uses `Control` to request a redraw, start timers or quit;
   - `paint` draws the whole window into the `RasterSurface`;
   - `accessibility` returns an AccessKit tree, so screen readers can use the app.
4. Keep app logic in a plain module with unit tests (see `apps/calculator/src/engine.rs`). Test the UI by sending events and checking pixels (see `apps/calculator/src/main.rs`).
5. Add a `packaging/<app>.desktop` entry, install it in `packaging/debian/rules`, `packaging/arch/PKGBUILD` and `install.sh`, and add the binary to the payload lists in the package workflows and QA scripts.

A missing control belongs in `crates/slopos-ui/src/controls`, with its own state machine and tests, not inside one app.

## Longer QA

The scripts in `scripts/run-*-qa.sh` drive heavier X11, AT-SPI, multi-monitor, resolution, packaging and recovery checks. They run nightly in `.github/workflows/qa.yml`, and you can start them by hand with *Run workflow*. Read a script before running it locally: several use fixed displays (`:90`, `:95`) and name-based `pkill`, so run them one at a time on a machine where that is safe.

The ISO, installed-VM, recovery and hardware-contract workflows run weekly and on demand.

## Packages and releases

- `nightly.yml` builds packages on pushes to `main`, its daily schedule, and manual dispatch. Runs whose ref is `main` publish them, with SHA256SUMS, to the rolling [`nightly`](https://github.com/palaashatri/rust-slopos/releases/tag/nightly) pre-release.
- Local Debian build: `cp -a packaging/debian debian && dpkg-buildpackage --build=binary --no-sign`.
- Local Arch build: `cd packaging/arch && makepkg`.

## Where to look

| Task | Start here |
|---|---|
| How the pieces fit | [architecture.md](architecture.md) |
| Target behaviour of a component or app | [CONTRACT.md](CONTRACT.md) → the area file |
| Design rules and review checklist | [contract §5.2](contract/02-design.md) |
| What was verified, when | [ledger](contract/ledger.md) |
