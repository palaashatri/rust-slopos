# SLOPOS-I

SLOPOS-I is a lightweight, polished, Macintosh-inspired Linux desktop operating environment built entirely around mature X11 infrastructure, existing high-quality Linux applications, careful system integration, strong defaults, and a curated AppImage application catalogue.

The objective is explicit:

> **Ship a coherent, attractive, reliable Linux desktop OS that an ordinary user can install and actually use daily.**

---

## Key Features

- **X11 Desktop Architecture:** Powered by X.Org server and Openbox stacking window manager with full EWMH/ICCCM compliance.
- **Macintosh-Inspired Shell:** Top system/menu bar, active window title, system status indicators (clock, volume, network, Bluetooth, power), Spotlight-style application launcher (`Cmd/Super+Space`), bottom application Dock, and desktop notification server.
- **Curated AppImage Catalogue:** Lightweight software catalogue for browsing, installing, updating, and removing AppImage applications with automated SHA-256 integrity checks and launcher entry integration.
- **Upstream Application Integration:** Preconfigured with PCManFM (files), Mousepad (editor), Xfce4-Terminal (terminal), Viewnior (image viewer), Zathura (document viewer), MPV (media player), Firefox (browser), and Galculator (calculator).
- **System Settings:** Unified GTK settings app managing displays (XRandR), audio (PipeWire/WirePlumber), network (NetworkManager), Bluetooth (BlueZ), and power (UPower).
- **Coherent Theming:** Custom GTK theme, matching Openbox window frame styling, icon theme, cursor pointer, and desktop wallpapers.
- **Automated QA Suite:** Continuous integration inside Docker containers using Xvfb for automated X11 desktop testing and visual QA screenshot validation.

---

## System Requirements

- **Processor:** x86-64 CPU (2 cores recommended)
- **RAM:** 2 GB minimum (4 GB recommended)
- **Graphics:** Any OpenGL-capable X11 graphics hardware (Intel, AMD, NVIDIA)
- **Storage:** 10 GB available disk space

---

## Architecture Overview

```text
Linux Kernel / systemd / NetworkManager / PipeWire / BlueZ / UPower
                       │
                       ▼
            X.Org-compatible X11 Server
                       │
                       ▼
        Openbox Stacking Window Manager
                       │
                       ▼
         SLOPOS Desktop & Session Layer
  ┌────────────────────┬────────────────────┐
  │   slopos-session   │    slopos-shell    │
  ├────────────────────┼────────────────────┤
  │  slopos-catalogue  │  slopos-settings   │
  └────────────────────┴────────────────────┘
                       │
                       ▼
 Integrated Upstream Applications & AppImages
```

---

## Build & Test Instructions

### Workspace Requirements
- Rust toolchain (stable)
- GTK3 development headers (`libgtk-3-dev`)
- X11 development headers (`libx11-dev`, `libxrandr-dev`)
- PKG-Config (`pkg-config`)

### Local Build
```bash
cargo build --workspace --release
```

### Run Desktop Session (Nested / Virtual X Server)
```bash
# Launch inside Xephyr or Xvfb for testing
Xephyr -ac -screen 1280x800 :1 &
DISPLAY=:1 ./target/release/slopos-session
```

### Docker Automated QA
```bash
./scripts/run-docker-qa.sh
```

---

## Documentation

- [`AGENTS.md`](AGENTS.md) — Production contract, architecture, non-goals, and completion criteria.
- [`TRUTH.md`](TRUTH.md) — Live readiness score, category scores, and evidence log.

---

## Licensing

SLOPOS-I first-party source code and original assets are MIT-licensed. Third-party components retain their respective upstream open-source licenses.
