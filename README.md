# SLOPOS-I — X11 Platinum Desktop

SLOPOS-I is a lightweight Linux desktop environment inspired by the clarity and compactness of classic Macintosh System 7 / Platinum interfaces. It deliberately builds on mature X11, Openbox and upstream Linux applications rather than maintaining a custom display stack or first-party application suite.

> **Own the experience. Do not unnecessarily own the infrastructure.**

## Current status

The `pivot` branch is an active product reboot. The 2026-08-12 remediation audit records **64/100** in [`TRUTH.md`](TRUTH.md): the X11 packaging, catalogue validation, shell contracts and Docker/Xvfb evidence are materially stronger, but the project is **not production-ready**. VM/boot, trusted metadata for three catalogue entries, independent visual acceptance, hardware-service, accessibility/localization, performance and full recovery evidence remain open.

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

## SLOPOS Platinum

The canonical appearance is an original System-7/Platinum-inspired light theme with compact controls, crisp borders, restrained 3D bevels, classic blue selection, a cool slate desktop and a distinctive SLOPOS identity.

Reference projects and design kits are used for visual study only. SLOPOS does not ship Apple logos, proprietary Apple fonts or copied proprietary assets.

## Docker Desktop / Xvfb development QA

Docker Desktop on Windows is supported as the primary build and X11 integration-test environment.

From PowerShell/CMD at the repository root:

```bash
docker run --rm -v "%CD%:/workspace" -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh
```

The QA script builds the workspace, starts a D-Bus-backed Xvfb/Openbox X11 session, asserts required processes and fresh visible windows, exercises representative upstream apps, checks launched PIDs and records five non-empty 1280x800 canonical screenshots.

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

## Recovery

```bash
bash scripts/slopos-recovery.sh
```

## Project truth

- `AGENTS.md` — normative product/engineering contract.
- `TRUTH.md` — live evidence-backed readiness ledger.
- `README.md` — public overview.

Do not infer release readiness from screenshots or build success alone; use `TRUTH.md`.
