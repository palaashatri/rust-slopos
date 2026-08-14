# SLOPOS-I — X11 Platinum Desktop

SLOPOS-I is a lightweight Linux desktop environment inspired by the clarity and compactness of classic Macintosh System 7 / Platinum interfaces. It deliberately builds on mature X11, Openbox and upstream Linux applications rather than maintaining a custom display stack or first-party application suite.

> **Own the experience. Do not unnecessarily own the infrastructure.**

## Current status

The `pivot` branch is an active product reboot. The 2026-08-13 remediation audit records **78/100** in [`TRUTH.md`](TRUTH.md): the X11 packaging, catalogue validation, shell contracts, Docker/Xvfb evidence, AT-SPI surface acceptance, complete Settings delegate-boundary checks, upstream application matrix, installed-disk VirtualBox session and QEMU-booted LightDM session are materially stronger, but the project is **not production-ready**. One Installer point and one Recovery point are conservatively withheld because current-tree VM/EFI and hardware-recovery evidence is not available. Independent visual acceptance, real-exporter AppMenu interaction, hardware-service mutation, hardware accessibility, independent localization review and hardware-target performance remain open. Hosted CI run [#605](https://github.com/palaashatri/rust-slopos/actions/runs/31763457747) for commit `5b784f0` confirms the locked build/test/lint, rustfmt, release, Xvfb/Openbox, AT-SPI/Orca, Settings, locale and retained Xvfb resolution gates (including 8K and 2560x1600 HiDPI); the manual real-AppMenu workflow was intentionally skipped. The latest source hardening also preserves browser profile data and makes VM/bootstrap/session checks fail closed; these safeguards do not increase the score by themselves.

## Architecture

```text
Linux + system services
        ↓
X.Org-compatible X11 server
        ↓
Openbox stacking/floating window manager
        ↓
SLOPOS session + shell
  ├─ classic top menu/system bar
  ├─ application Search
  ├─ compact Application Strip
  ├─ notifications
  ├─ Software Catalogue
  └─ Settings hub
        ↓
Upstream applications + verified AppImages
```

SLOPOS-I is X11-only. There is no custom compositor, custom window manager, Wayland session, general SLOPOS GUI toolkit, Vision platform or custom replacement for ordinary desktop applications.

### Global menu policy

The shell owns only SLOPOS commands. It no longer fabricates File/Edit/View/Window/Help commands for whichever X11 window happens to have focus. For an application that does not export the X11 AppMenu properties (`_GTK_UNIQUE_BUS_NAME` plus `_GTK_APP_MENU_OBJECT_PATH` or `_GTK_MENUBAR_OBJECT_PATH`), the top bar shows a disabled App status and the application's upstream local menu remains authoritative. When the properties are present, the App button enables a bounded `com.canonical.dbusmenu` importer: it requests a depth/item-limited layout off the GTK thread and sends only protocol-defined `Event(..., "clicked", ...)` actions. Malformed layouts, missing buses and timeouts fall back to the upstream local menu. `scripts/run-appmenu-qa.sh` remains a property-only fallback smoke; the full Docker harness now contains a disposable QA-only `libdbus-1` exporter fixture that can prove `GetLayout`/`Event` without shipping a service. That real-import leg still needs a fresh provisioned Docker/Xvfb run before it receives readiness credit; Rust parser tests cover imported layouts in the meantime.

## Workspace

- `crates/slopos-session` — supervises Openbox and the SLOPOS shell.
- `crates/slopos-shell` — top bar, Search, Application Strip and SLOPOS notifications.
- `crates/slopos-catalogue` — curated AppImage catalogue/installer. Installation is accepted only when trusted integrity metadata is available.
- `crates/slopos-settings` — small SLOPOS-styled hub that delegates to mature system utilities rather than duplicating their engines.

The optional browser integration keeps Firefox/Chromium/Chrome upstream. `scripts/start-slopos-browser` clears Wayland inheritance, forces the X11/GTK identity, and loads the optional Chromium-family theme when available; `scripts/install-browser-theme.sh` can add a backed-up Firefox profile stylesheet or an explicit Chromium/Chrome profile theme. The installed `slopos-browser.desktop` entry owns the HTML/HTTP(S) defaults and Openbox/Search launch path while preserving the selected upstream engine; Search also routes discovered raw Firefox/Chromium entries through that wrapper when it is available. The cache-only Arch application gate captures Chromium in its provisioned image and exercises the Firefox path when Firefox is installed; Firefox uses a disposable explicit profile in that optional leg. Browser content and engine UI remain upstream-owned, so this is a best-effort Platinum frame/GTK integration rather than a browser fork. The Figma reference used for the visual audit is the supplied [Classic Macintosh UI Kit](https://www.figma.com/design/LGMlwNCoVdakZxDBvPKg1W/Classic-Macintosh-UI-Kit--Community-?node-id=0-1&p=f).

## SLOPOS Platinum

The canonical appearance is an original System-7/Platinum-inspired light theme with compact controls, crisp borders, restrained 3D bevels, classic blue selection, a cool slate desktop and a distinctive SLOPOS identity.

Reference projects and design kits are used for visual study only. SLOPOS does not ship Apple logos, proprietary Apple fonts or copied proprietary assets.

## Docker Desktop / Xvfb development QA

Docker Desktop on Windows is supported as the primary build and X11 integration-test environment.

From PowerShell/CMD at the repository root:

```bash
docker run --rm -v "%CD%:/workspace" -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh
```

The QA script builds the workspace, starts a D-Bus-backed Xvfb/Openbox X11 session, asserts required processes and fresh visible windows, exercises representative upstream apps, checks launched PIDs, verifies deliberate window move/resize plus Alt+Tab focus switching, and records eleven non-empty 1280x800 canonical screenshots covering the desktop, menu, Search, notification, modal, application, overlap, file manager, terminal, Catalogue and Settings states. It also compiles a disposable QA-only DBusMenu exporter when the cached `libdbus-1` headers are present; `SLOPOS_QA_REQUIRE_REAL_APPMENU=1` turns missing headers or a failed `GetLayout`/`Event` click-through into a hard failure. `scripts/run-resolution-qa.sh` separately retains desktop/Search/Settings evidence at 1366x768, 1920x1080, 3440x1440, 3840x2160, 5120x2880 and 7680x4320 at scale 1, plus 2560x1600 with `GDK_SCALE=2`, validating full-width topbar and centered Application Strip geometry. These are Xvfb geometry/render checks; high-refresh physical timing and GPU/VRR behavior remain unverified. `scripts/run-appmenu-qa.sh` is a cache-only capability/fallback smoke for Mousepad and an X11 property-advertised exporter fixture. The separate Arch application gate runs PCManFM, Xfce Terminal, Mousepad, Ristretto and Chromium through the SLOPOS wrapper against a deterministic local browser fixture, optionally adds Firefox when that package is provisioned, and runs a packaged SuperTux world1 level with movement/jump input plus both a PulseAudio sink-input check and non-silent raw PCM monitor capture. When SuperTux is installed, the Application Strip exposes the same upstream game as an honest launcher; when it is absent, that control is disabled. Browser/game, null-sink and monitor results remain bounded container evidence, not proof of physical speaker or GPU behavior.

For reproducible end-to-end AppMenu evidence without creating a local container, trigger this workflow with GitHub Actions `workflow_dispatch`; the manual `x11-appmenu-real` job provisions its own runner, requires `GetLayout`/`Event` success, and uploads the marker/log/screenshot bundle. Its result remains evidence for the exact dispatched commit and does not by itself close the independent visual or hardware gates.

The dedicated `scripts/run-atspi-qa.sh` acceptance starts the same X11 session with the AT-SPI bridge and verifies six named SLOPOS surfaces plus focused Search through the maintained AT-SPI 2 API. Its extended mode also checks UTF-8 Entry input and reversible Tab/Shift+Tab focus at normal and 2x-scale Xvfb sizes, and CI runs generated `fr_FR.UTF-8` and `de_DE.UTF-8` locale legs plus an opt-in Orca speech/debug smoke. `scripts/run-settings-service-qa.sh` separately proves that Settings disables absent utilities and invokes every available upstream delegate, while `scripts/benchmark-x11-session.sh` can hold a clean X11 session for a bounded RSS/liveness run. Hardware accessibility remains an additional release gate.

A passing Docker run is **development evidence**, not proof of hardware/installer/visual production readiness.

## Native build

Install the distro dependencies from `packaging/deps/`, then:

```bash
cargo build --workspace --release --locked
```

For a layered X11 installation on a supported Arch/Ubuntu-family system:

```bash
sudo ./install.sh
```

The installer installs only the current X11 product binaries, session descriptor and theme/configuration assets.

## Bootable image

Where the current packaging environment supports it:

```bash
bash packaging/iso/build-iso.sh
```

Boot/install validation in QEMU or physical hardware remains a separate release gate from container QA.

For a reproducible installed-to-disk VirtualBox run, create a fresh VM with
the ISO, generate the ignored QA key beside the VM scripts, and pass the exact
checkout commit to the installer harness:

```powershell
ssh-keygen -t ed25519 -f packaging/vm/qa_key -N ""
$sha = (git rev-parse HEAD).Trim()
pwsh -File packaging/vm/create-vm.ps1 -IsoPath artifacts/iso/<image>.iso -Recreate
pwsh -File packaging/vm/provision.ps1 -RepoCommit $sha
pwsh -File packaging/vm/qa-installed.ps1 -ExpectedCommit $sha -SshKeyPath packaging/vm/qa_key
```

The final command waits for the post-reboot SSH service, verifies the guest
checkout SHA, runs `qa-vm.sh`, captures a VirtualBox screenshot, and writes
`packaging/vm/installed-vm-evidence/status.json`. This is installation evidence;
Xvfb, live-ISO and QEMU checks do not replace it.

## Recovery

```bash
bash scripts/slopos-recovery.sh
```

The recovery helper preserves the current per-user SLOPOS/Openbox config in a
timestamped backup, stages installed vendor defaults when available, and
restarts the existing session's managed X11 children. It exits non-zero when
the session cannot be restored and prints `SLOPOS_RECOVERY_STATUS_0` only after
fresh Openbox and shell processes are observed; it does not claim hardware or
VM recovery.

## Project truth

- `AGENTS.md` — normative product/engineering contract.
- `TRUTH.md` — live evidence-backed readiness ledger.
- `README.md` — public overview.

Do not infer release readiness from screenshots or build success alone; use `TRUTH.md`.
