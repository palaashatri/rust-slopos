# AGENTS.md — SLOPOS-I Development Source of Truth & Normative Contract

**Authority:** This is the sole normative development document for SLOPOS-I. Every implementation agent, reviewer, maintainer, and automation must follow it.

The repository may contain only three Markdown files:
- `README.md` — public introduction, quick start, build instructions;
- `AGENTS.md` — product requirements, architecture, execution plan, and acceptance criteria;
- `TRUTH.md` — factual audit, evidence ledger, scores, known defects, and current next gate.

Do not add competing roadmaps, plans, hand-off notes, or session summaries as Markdown files. Raw QA evidence belongs under `artifacts/qa/` in machine-readable formats.

---

## 1. Mission & Product Definition

SLOPOS-I is a lightweight, polished, Macintosh-inspired Linux desktop operating environment built entirely around mature X11 infrastructure, existing high-quality Linux applications, careful system integration, strong defaults, and a curated AppImage application catalogue.

The conceptual inspiration is closer to **helloSystem** than KDE, GNOME, or a from-scratch desktop stack. The objective is not technological sovereignty, but to:

> **Ship a coherent, attractive, reliable Linux desktop OS that an ordinary user can install and actually use daily.**

---

## 2. Explicit Non-Goals & Architecture Constraints

SLOPOS-I is strictly **X11-only**. There must be:
- NO Wayland session, compositor, wlroots, or Smithay integration;
- NO custom window manager project (e.g., `slopos-x11-wm`);
- NO custom rendering engine, GUI toolkit, or application SDK;
- NO first-party replacements for standard desktop applications (file manager, text editor, terminal, media player);
- NO local AI/Vision platform or daemon (`slopos-visiond`);
- NO FreeBSD platform abstraction layer or multi-OS portability layer;
- NO speculative future architecture or "SLOPOS-II" scope.

---

## 3. Product Architecture

```text
Linux Kernel / Drivers / systemd / logind / udev
 ├── NetworkManager (networking & Wi-Fi)
 ├── PipeWire / WirePlumber (audio)
 ├── BlueZ (Bluetooth)
 ├── UPower (power management)
 └── Distro package manager (base system maintenance)
      │
      ▼
X.Org-compatible X11 Server
      │
      ▼
Openbox Stacking Window Manager (ICCCM / EWMH compliance)
      │
      ▼
SLOPOS Desktop & Session Layer
 ├── slopos-session (X11 session supervisor & recovery)
 ├── slopos-shell (Macintosh top bar, launcher, Dock, notifications, tray)
 ├── slopos-catalogue (curated AppImage manager)
 ├── slopos-settings (unified display, audio, network, appearance utility)
 ├── Themes & Icons (GTK theme, Openbox window border theme, wallpapers)
 └── MIME & Desktop Defaults (file associations, browser, terminal)
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

## 4. Window Management Model

SLOPOS-I uses **Openbox** as its primary floating/stacking window manager.

Core interaction principles:
- Overlapping windows with standard Macintosh-inspired titlebar controls (close, minimize, zoom/maximize on top-left);
- EWMH `_NET_CLIENT_LIST` and `_NET_WM_STATE` compliance for task tracking;
- Multi-desktop workspace switching;
- Sensible initial window placement and window rules;
- Multi-monitor support via XRandR;
- Snapping and keyboard-driven window tiling conveniences.

---

## 5. Upstream Application Matrix

SLOPOS-I integrates high-quality, lightweight, mature upstream Linux applications instead of reimplementing basic tools:

| Category | Primary Choice | Fallback / Alternative |
|---|---|---|
| File Manager | **PCManFM** | Thunar |
| Terminal | **Xfce4-Terminal** | Alacritty / Kitty |
| Text Editor | **Mousepad** | FeatherPad |
| Image Viewer | **Viewnior** | Ristretto |
| PDF / Document | **Zathura** | Evince |
| Media Player | **MPV** | VLC |
| Web Browser | **Firefox** | Chromium |
| Calculator | **Galculator** | KCalc |

---

## 6. AppImage Catalogue Model

User-facing graphical application installation is strictly handled via **AppImages**:
- `slopos-catalogue` queries structured metadata feeds (JSON format with name, version, description, categories, icon URL, download URL, SHA-256 checksum).
- Downloads AppImages over HTTPS to managed directory `~/.local/share/slopos-i/applications/`.
- Verifies SHA-256 integrity prior to execution.
- Generates `.desktop` launcher shortcuts in `~/.local/share/applications/` and icon assets in `~/.local/share/icons/`.
- Registers MIME file associations.
- Provides clean one-click uninstall and safe atomic updates.

---

## 7. 100/100 Production Rubric

| Domain | Weight |
|---|---:|
| Desktop UX & visual polish | 15 |
| Core desktop behavior | 15 |
| Application compatibility | 12 |
| Hardware/display/input integration | 10 |
| System services integration | 10 |
| AppImage Catalogue | 8 |
| Installer & first boot | 8 |
| Updates & recovery | 7 |
| Performance | 5 |
| Accessibility / localization | 4 |
| Security | 3 |
| QA / release engineering | 3 |
| **Total** | **100** |

---

## 8. Definition of Done (100/100 Completion Gate)

SLOPOS-I reaches 100/100 only when all of the following are satisfied:
1. Production implementation of session supervisor (`slopos-session`), shell (`slopos-shell`), AppImage catalogue (`slopos-catalogue`), and settings app (`slopos-settings`);
2. Clean X11 session launch with Openbox, top bar, dock, launcher, and system status indicators;
3. Verified AppImage download, checksum validation, desktop entry integration, and uninstall flow;
4. Upstream application matrix installed, configured, and bound via MIME defaults;
5. System settings GTK interface controlling displays (XRandR), audio (PipeWire), networking (NetworkManager), Bluetooth (BlueZ), and power (UPower);
6. Bootable live ISO installer (Debian/Ubuntu base with Calamares or automated installer) resulting in a working installed desktop;
7. Reset/recovery script (`slopos-recovery`) to restore malformed session configurations;
8. Automated Docker/Xvfb integration test suite passing cleanly;
9. Visual QA screenshots verifying zero layout clipping or theme mismatch;
10. `TRUTH.md` accurately reflecting 100/100 score supported by commit SHA and test logs.
