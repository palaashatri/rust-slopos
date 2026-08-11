# AGENTS.md — SLOPOS-I System 7 Platinum Normative Contract

**Authority:** This is the sole normative development document for SLOPOS-I. Every implementation agent, reviewer, maintainer, and automation must follow it.

The repository contains three Markdown files:
- `README.md` — public introduction, architecture, quick start, build instructions;
- `AGENTS.md` — product requirements, System 7 Platinum visual specifications, architecture, and acceptance criteria;
- `TRUTH.md` — factual audit, evidence ledger, visual scores, known defects, and current score.

---

## 1. Mission & Product Definition

SLOPOS-I is a lightweight, highly polished, Macintosh System 7 / Platinum-inspired Linux desktop operating environment built exclusively on mature X11 infrastructure (Openbox WM, GTK3 theme system, X.Org, and curated AppImages).

> **"Own the experience. Do not unnecessarily own the infrastructure."**

SLOPOS-I delivers the visual discipline and desktop atmosphere of Classic Macintosh System 7 rebuilt into a usable modern Linux desktop OS.

---

## 2. Explicit Non-Goals & Architecture Constraints

SLOPOS-I is strictly **X11-only**. There must be:
- NO Wayland session, compositor, wlroots, or Smithay integration;
- NO custom window manager project (e.g., `slopos-x11-wm`);
- NO custom rendering engine, GUI toolkit, or application SDK;
- NO first-party replacements for standard desktop applications (file manager, text editor, terminal, media player);
- NO local AI/Vision platform or daemon (`slopos-visiond`);
- NO FreeBSD platform abstraction layer or multi-OS portability layer.

---

## 3. Product Architecture & System 7 Platinum Design System

```text
Linux Kernel / Drivers / systemd / logind / udev
 ├── NetworkManager (networking & Wi-Fi)
 ├── PipeWire / WirePlumber (audio)
 ├── BlueZ (Bluetooth)
 ├── UPower (power management)
 └── Distro package manager (base system maintenance)
      │
      ▼
X.Org-compatible X11 Server + Openbox Window Manager (System 7 Theme)
      │
      ▼
SLOPOS Desktop & Session Layer
 ├── slopos-session (X11 session supervisor & watchdog)
 ├── slopos-shell (System 7 top menu bar, Spotlight launcher, Application Strip, notifications)
 ├── slopos-catalogue (curated AppImage manager)
 ├── slopos-settings (unified display, audio, network, appearance utility)
 ├── GTK3 Platinum Theme (`assets/config/gtk-3.0/gtk.css`)
 └── Desktop Atmosphere (Macintosh cool-gray `#758090` background)
      │
      ▼
Upstream Linux Applications & AppImages
 ├── PCManFM (file manager)
 ├── Xfce4-Terminal (terminal emulator)
 ├── Mousepad (text editor)
 ├── Viewnior / Ristretto (image viewer)
 ├── Zathura (document / PDF viewer)
 ├── MPV (media playback)
 ├── Firefox (web browser)
 ├── Galculator (calculator)
 └── Downloaded AppImage applications
```

---

## 4. System 7 Platinum Design Specifications

1. **Top Menu Bar**: 24px full-width bar (`#DDDDDD`) with SLOPOS system logo menu (``), active application name in bold, global menu structure (`File`, `Edit`, `View`, `Window`, `Help`), and right-aligned compact status icons (Search, Volume, Network, Battery, Clock).
2. **Window Decoration ("slopos-openbox")**: Square corners, crisp 1px black outline, active titlebar with classic horizontal pinstripes (`#E6E6E6` to `#CDCDCD`), Close box (top-left) and Zoom box (top-right).
3. **Application Strip**: Beveled 3D Platinum box container at bottom center with raised 3D icon buttons and active running indicators.
4. **Form Controls**: 3D raised bevel buttons with light top/left edges and dark bottom/right edges, double-ring default buttons, and sunken 3D text entry boxes.
5. **Desktop Atmosphere**: Classic Macintosh cool-gray background (`#758090`).

---

## 5. Upstream Application Matrix

| Category | Primary Choice | Desktop Association |
|---|---|---|
| File Manager | **PCManFM** | Default folder / file manager |
| Terminal | **Xfce4-Terminal** | Default shell emulator |
| Text Editor | **Mousepad** | Default text / markdown editor |
| Image Viewer | **Viewnior** | Default image viewer |
| PDF / Document | **Zathura** | Default PDF viewer |
| Media Player | **MPV** | Default audio/video player |
| Web Browser | **Firefox** | Default web browser |
| Calculator | **Galculator** | Default calculator |

---

## 6. AppImage Catalogue Model

Graphical application installation is strictly handled via **AppImages**:
- `slopos-catalogue` queries structured metadata feeds (JSON format).
- Downloads AppImages over HTTPS to `~/.local/share/slopos-i/applications/`.
- Verifies SHA-256 integrity prior to execution.
- Generates `.desktop` launcher shortcuts in `~/.local/share/applications/` and icon assets.
- Provides atomic updates and clean uninstallation.

---

## 7. Scorecard Rubric (100/100 Total)

| Domain | Weight |
|---|---:|
| System 7 Platinum Visual identity / polish | 20 |
| Desktop shell / interaction | 15 |
| X11 window management integration | 10 |
| Upstream app integration | 10 |
| Software Catalogue / AppImage | 8 |
| System services integration | 8 |
| Installer / session supervision | 8 |
| Functional QA | 7 |
| Visual regression QA | 5 |
| Performance | 3 |
| Accessibility / localization | 3 |
| Recovery / resilience | 3 |
| **Total** | **100** |

---

## 8. Definition of Done (100/100 Completion Gate)

SLOPOS-I reaches 100/100 only when all of the following are satisfied:
1. Production implementation of supervisor (`slopos-session`), shell (`slopos-shell`), AppImage catalogue (`slopos-catalogue`), and settings (`slopos-settings`);
2. Clean X11 session launch with Openbox System 7 theme, top menu bar, Application Strip, and desktop background pattern;
3. Verified AppImage download, checksum validation, desktop entry integration, and uninstall flow;
4. Upstream application matrix installed, configured, and bound via MIME defaults;
5. System settings GTK interface controlling displays, audio, networking, Bluetooth, and power;
6. Bootable live ISO installer (`packaging/iso/build-iso.sh`);
7. Reset/recovery script (`scripts/slopos-recovery.sh`);
8. Automated Docker/Xvfb integration test suite (`scripts/run-docker-qa.sh`) passing cleanly;
9. Visual QA screenshots verifying zero layout clipping, pinstripe window titlebars, raised 3D controls, and desktop atmosphere;
10. `TRUTH.md` accurately reflecting 100/100 score supported by commit SHA and test logs.
