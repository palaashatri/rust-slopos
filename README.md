# SLOPOS-I — X11 Platinum Desktop

SLOPOS-I is a lightweight Linux desktop environment inspired by the clarity and compactness of classic Macintosh System 7 / Platinum interfaces. It deliberately builds on mature X11, Openbox and upstream Linux applications rather than maintaining a custom display stack or first-party application suite.

> **Own the experience. Do not unnecessarily own the infrastructure.**

## Current status

The `pivot` branch is an active product reboot. The 2026-08-13 remediation audit records **78/100** in [`TRUTH.md`](TRUTH.md): the X11 packaging, catalogue validation, shell contracts, Docker/Xvfb evidence, upstream application matrix, installed-disk VirtualBox session and QEMU-booted LightDM session are materially stronger, but the project is **not production-ready**. Independent visual acceptance, hardware-service, accessibility/localization and long-run performance evidence remain open.

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

## Workspace

- `crates/slopos-session` — supervises Openbox and the SLOPOS shell.
- `crates/slopos-shell` — top bar, Search, Application Strip and SLOPOS notifications.
- `crates/slopos-catalogue` — curated AppImage catalogue/installer. Installation is accepted only when trusted integrity metadata is available.
- `crates/slopos-settings` — small SLOPOS-styled hub that delegates to mature system utilities rather than duplicating their engines.

The optional browser integration keeps Firefox/Chromium/Chrome upstream. `scripts/start-slopos-browser` clears Wayland inheritance, forces the X11/GTK identity, and loads the optional Chromium-family theme when available; `scripts/install-browser-theme.sh` can add a backed-up Firefox profile stylesheet or an explicit Chromium/Chrome profile theme. Browser content and engine UI remain upstream-owned, so this is a best-effort Platinum frame/GTK integration rather than a browser fork. The Figma reference used for the visual audit is the supplied [Classic Macintosh UI Kit](https://www.figma.com/design/LGMlwNCoVdakZxDBvPKg1W/Classic-Macintosh-UI-Kit--Community-?node-id=0-1&p=f).

## SLOPOS Platinum

The canonical appearance is an original System-7/Platinum-inspired light theme with compact controls, crisp borders, restrained 3D bevels, classic blue selection, a cool slate desktop and a distinctive SLOPOS identity.

Reference projects and design kits are used for visual study only. SLOPOS does not ship Apple logos, proprietary Apple fonts or copied proprietary assets.

## Docker Desktop / Xvfb development QA

Docker Desktop on Windows is supported as the primary build and X11 integration-test environment.

From PowerShell/CMD at the repository root:

```bash
docker run --rm -v "%CD%:/workspace" -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh
```

The QA script builds the workspace, starts a D-Bus-backed Xvfb/Openbox X11 session, asserts required processes and fresh visible windows, exercises representative upstream apps, checks launched PIDs, verifies deliberate window move/resize plus Alt+Tab focus switching, and records eleven non-empty 1280x800 canonical screenshots covering the desktop, menu, Search, notification, modal, application, overlap, file manager, terminal, Catalogue and Settings states. The separate Arch application gate runs PCManFM, Xfce Terminal, Mousepad, Ristretto, Chromium through the SLOPOS wrapper against a deterministic local browser fixture, and a packaged SuperTux world1 level with movement/jump input plus a PulseAudio sink assertion; its browser/game and null-sink results remain bounded container evidence, not proof of physical speaker or GPU behavior.

The dedicated `scripts/run-atspi-qa.sh` acceptance starts the same X11 session with the AT-SPI bridge and verifies six named SLOPOS surfaces plus focused Search through the maintained AT-SPI 2 API. It is also a separate CI job; focus order, text scaling, localization and hardware accessibility remain additional release gates.

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

## Project truth

- `AGENTS.md` — normative product/engineering contract.
- `TRUTH.md` — live evidence-backed readiness ledger.
- `README.md` — public overview.

Do not infer release readiness from screenshots or build success alone; use `TRUTH.md`.
