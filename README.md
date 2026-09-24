# SLOPOS-I

SLOPOS-I is an experimental Linux desktop environment focused on a coherent, compact Classic-Macintosh-inspired desktop experience on modern **X11** infrastructure.

The project is currently undergoing an **atomic UI and functionality reset**. The goal is not to approximate a retro look with a GTK theme; it is to define each primitive, control, interaction and workflow precisely, make it correct, then compose those verified pieces into the desktop.

## Current status

**Not release ready. Not production complete.**

The current tree contains a working architectural base and substantial QA/release infrastructure, but the 2026-09-24 audit found visual and functional gaps that must be corrected before the desktop can be considered mature.

The current implementation includes:

- an X11 session supervisor;
- Openbox window management;
- a SLOPOS shell and global top bar;
- application search/launcher;
- notifications;
- a Control Panels-style Settings application;
- an AppImage Software Catalogue;
- PCManFM-based desktop/file integration;
- X11 window/monitor integration;
- packaging and QA scripts.

Known current gaps include:

- the UI has not yet been rebuilt from the new Figma-derived atomic specification;
- several surfaces still contain modern rounded/shadow/card styling;
- Network Settings currently contains sample/fabricated state and must be replaced with real NetworkManager state;
- Sound Settings is only partially connected to real audio state;
- generic fallback application Edit actions are not a truthful global-menu implementation;
- file-browser visual conformance is not proven;
- canonical About and accessory utility surfaces are still missing;
- current build/runtime/visual evidence must be regenerated in a compliant Linux VM.

See [AGENTS.md](AGENTS.md#part-ii--current-audit-and-evidence-ledger) for the current evidence-backed audit and blocker ledger.

## Canonical design direction

The visual/component reference is the community **Classic Macintosh UI Kit** in Figma:

`https://www.figma.com/design/LGMlwNCoVdakZxDBvPKg1W/Classic-Macintosh-UI-Kit--Community-?node-id=0-1&p=f`

SLOPOS uses that reference as a clean-room geometry, density, hierarchy and component-state target.

SLOPOS does **not** ship proprietary Apple artwork, logos, fonts, sounds or copied assets.

The canonical SLOPOS appearance is called **Platinum Classic**.

## Product architecture

```text
Linux services
  ├─ NetworkManager
  ├─ PipeWire / WirePlumber or compatible audio stack
  ├─ BlueZ
  ├─ UPower
  └─ systemd / logind
        ↓
X11
        ↓
Openbox
        ↓
slopos-session
        ↓
slopos-shell
  ├─ global menu/system bar
  ├─ launcher/search
  ├─ notifications
  └─ desktop integration
        ↓
slopos-settings
slopos-catalogue
normal X11 applications
```

SLOPOS-I is intentionally **X11-only** for this product generation. Wayland work is paused while the X11 desktop is brought to maturity.

## Development method

The project now develops bottom-up:

```text
foundation
→ primitive
→ control
→ container
→ window component
→ menu component
→ shell component
→ system provider
→ workflow
→ application
→ complete desktop
```

A higher-level surface is not considered finished while a lower-level component it depends on is still approximate.

The full engineering contract lives in [AGENTS.md](AGENTS.md).

## Development environment

SLOPOS project code must **not** be compiled or executed on the native host during local development.

All local:

- compilation;
- tests;
- linting/format validation;
- package builds;
- runtime testing;
- X11 testing;
- visual QA

must run inside a Linux VM hosted by **UTM, QEMU, VirtualBox or VMware**.

Ubuntu LTS is the preferred general development guest. Distribution-specific guests may be used for Debian/Ubuntu or Arch acceptance work.

The build clone and build output should live on guest storage, not a host shared folder.

Primary visual QA must be performed in a real graphical X11 session inside the VM. Xvfb/container testing may be used as a secondary deterministic layer inside the VM.

The project also enforces disk-space budgets so build caches, Rust targets, VM snapshots, package staging and media artifacts do not exhaust the host or guest. See [AGENTS.md](AGENTS.md).

## Build from source

Source installation exists for development/testing. It is not yet the preferred consumer installation path.

Ubuntu/Debian-family development guest:

```bash
git clone https://github.com/palaashatri/rust-slopos.git
cd rust-slopos
sudo ./install.sh --distro ubuntu
```

Arch development guest:

```bash
git clone https://github.com/palaashatri/rust-slopos.git
cd rust-slopos
sudo ./install.sh --distro arch
```

After installation, select **SLOPOS-I** from the display manager's X11 session chooser.

Do this only inside a disposable or dedicated Linux VM while the project remains in active alpha development.

## Package repositories

There is currently **no public official SLOPOS-I APT or Pacman repository**.

Do not use documentation or commands that claim `repo.slopos.org` is a live package source.

Repository-generation infrastructure may exist in source, but public package-manager installation must not be advertised until signed repositories are actually published and verified.

## Useful project documents

- [AGENTS.md](AGENTS.md) — single project-wide source of truth: engineering contract, architecture, plans, audit evidence and blockers.
- [packaging/browser/README.md](packaging/browser/README.md) — scoped upstream browser integration.
- [packaging/repo/README.md](packaging/repo/README.md) — current package-repository status and future publication contract.

Historical screenshots and old QA material are not authoritative acceptance evidence for the current atomic/Figma contract.

## License

SLOPOS-I is licensed under the MIT License. See [LICENSE](LICENSE).

Third-party software and assets retain their own licenses. See [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt).
