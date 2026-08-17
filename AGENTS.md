# AGENTS.md — SLOPOS-I X11 Platinum Product Contract

**Authority:** This is the sole normative development contract for SLOPOS-I. `README.md` describes the public product and `TRUTH.md` records evidence-backed readiness. If implementation, comments, tests, or old artifacts conflict with this file, this file wins.

## 1. Mission

SLOPOS-I is a lightweight, highly polished, classic-Macintosh/System-7/Platinum-inspired Linux desktop environment built on mature X11 infrastructure.

> **Own the experience. Do not unnecessarily own the infrastructure.**

The product goal is a coherent desktop that ordinary users can install and use, not technological sovereignty. It should feel deliberately designed from login through daily application use while relying on proven Linux components for window management, networking, audio, Bluetooth, power and ordinary applications.

## 2. Non-negotiable architecture

SLOPOS-I is **X11-only**.

There must be:

- no Wayland session or Wayland fallback;
- no Smithay or wlroots compositor;
- no custom display server;
- no custom window manager;
- no XWayland requirement;
- no custom renderer, general GUI toolkit or application SDK;
- no first-party replacements for file manager, terminal, text editor, browser, media player or document viewer;
- no Vision/AI daemon or local-AI platform requirement;
- no FreeBSD or hypothetical SLOPOS-II portability architecture.

The supported stack is conceptually:

```text
Linux + systemd/logind/udev
  ├─ NetworkManager
  ├─ PipeWire/WirePlumber
  ├─ BlueZ
  ├─ UPower
  └─ distro package manager
        ↓
X.Org-compatible X11 server
        ↓
Openbox stacking/floating window manager
        ↓
SLOPOS session + shell
  ├─ top menu/system bar
  ├─ application search
  ├─ compact Application Strip
  ├─ notifications
  ├─ Software Catalogue
  └─ small Settings hub
        ↓
Mature upstream applications + verified AppImages
```

Openbox is infrastructure. Do not replace it with `slopos-x11-wm` or another in-house WM.

## 3. Product identity

The canonical appearance is **SLOPOS Platinum**: an original design strongly informed by System 7/Platinum control grammar, with modern Linux functionality and legally redistributable assets.

Reference material may include:

- `Calculable/System7Components` for control grammar and states;
- the supplied System-7-like Figma community UI kits for composition/proportions;
- historical Macintosh/System 7 screenshots for interaction understanding.

References are for visual/behavioral study. Do not copy proprietary Apple assets or redistribute reference-project assets unless their licenses are explicitly compatible.

### Legal/identity constraints

Do not ship:

- Apple logos or rainbow Apple marks;
- proprietary Apple fonts (including SF, Chicago, Geneva, Charcoal) unless explicit redistribution rights are documented;
- copied Apple icons, wallpapers, sounds or other proprietary resources;
- Apple product names as SLOPOS feature branding when avoidable.

Use an original SLOPOS mark and original/open-licensed visual assets.

## 4. Visual language

SLOPOS Platinum must look like one system, not a collection of default GTK applications.

Required traits:

- compact full-width menu/system bar;
- floating/stacking windows with crisp classic decoration;
- square or nearly square geometry;
- high-contrast 1 px outlines;
- restrained raised/sunken bevels;
- classic blue selection (`#000080`) with white text where appropriate;
- neutral Platinum grays;
- compact controls and menus;
- visible active/inactive-window distinction;
- consistent custom/fallback icon sizing;
- one canonical light appearance before additional themes;
- no glassmorphism, giant rounded cards, Material/SaaS pill controls or arbitrary border radii.

The default desktop background is an original muted SLOPOS pattern/color, currently based around cool slate `#758090`. Solid black is forbidden as the shipping default.

### Canonical palette

Theme files and CSS must remain aligned around a single palette. At minimum:

- desktop: `#758090`;
- panel/window face: `#D9D9D9` to `#DDDDDD`;
- light bevel: `#FFFFFF`;
- mid bevel: `#B0B0B0`;
- dark bevel: `#606060`;
- text/borders: `#000000`;
- inactive text: approximately `#707070`;
- selection: `#000080`;
- selection text: `#FFFFFF`.

Do not introduce a second modern-blue design system alongside this one.

### Typography

Use redistributable fonts. Default UI fallback should prioritize an open font such as Liberation Sans/DejaVu Sans or another explicitly compatible font. Monospace should likewise use a redistributable face such as DejaVu Sans Mono.

Typography must remain crisp and compact at 1× and usable at HiDPI.

## 5. Shell contract

`slopos-shell` owns only the desktop chrome that makes the environment SLOPOS.

### Top bar

Required:

- original SLOPOS mark at far left;
- active application/window label that tracks actual X11 focus;
- functional File/Edit/View/Window/Help commands or honest omission of commands that cannot be supported;
- Search launcher trigger;
- live system status where data is available (audio/network/battery/time);
- correct local system time, not a hard-coded UTC offset;
- screen-width-aware geometry.

**No fake menus.** A visible menu item must perform its stated action or be disabled/omitted.

### Application search

`Super+Space` must toggle the **existing** shell launcher. It must never start a second shell instance. Search must support keyboard navigation, Enter launch and Escape dismiss.

### Application Strip

The bottom launcher is an intentional modern concession, but it must use SLOPOS Platinum framing rather than look like a generic toolbar. It must be screen-relative, keyboard accessible and visually consistent. Do not claim running indicators until they actually track running windows/processes.

### Notifications

Notifications must wrap text, remain on-screen at supported resolutions, use SLOPOS framing, dismiss correctly and not claim freedesktop D-Bus notification-server compliance unless the D-Bus interface is actually implemented and tested.

## 6. Window-management contract

Openbox provides ICCCM/EWMH stacking behavior.

Required behavior includes:

- normal overlapping windows;
- predictable focus;
- drag/resize;
- minimize/maximize/fullscreen;
- transient/modal behavior;
- four or more virtual desktops where configured;
- Alt+Tab switching;
- correct top/bottom shell margins;
- multi-resolution placement without hard-coded 1280×800-only geometry.

The Openbox theme must provide a convincing active/inactive Platinum frame. Claims such as “pinstripes” must match actual rendered evidence.

## 7. Upstream application policy

Prefer mature upstream programs. The reference matrix is:

| Role | Preferred application/class |
|---|---|
| File management | PCManFM or equivalent mature X11-friendly file manager |
| Terminal | Xfce4 Terminal or equivalent |
| Text editing | Mousepad or equivalent |
| Image viewing | Ristretto/Viewnior or equivalent |
| PDF viewing | Zathura or equivalent |
| Media | MPV/VLC |
| Browser | Firefox or another maintained browser |
| Calculator | Galculator or equivalent |

Selection may change for maintainability or distro availability. Do not fork applications merely for visual uniformity.

## 8. Settings policy — hub, not reimplementation

`slopos-settings` is a small SLOPOS-styled **hub** for mature system utilities. It must not pretend to be a full settings engine.

Preferred delegation examples:

- Displays → ARandR/LXRandR;
- Audio → Pavucontrol;
- Network → `nm-connection-editor`;
- Bluetooth → Blueman;
- Power → Xfce power-manager settings or equivalent;
- Appearance → LXAppearance / PCManFM desktop preferences;
- Input → a mature X11 input utility where available.

If a utility is absent, the corresponding row must be disabled or explicitly report that it is unavailable. Decorative sliders, switches or comboboxes that do not change real state are forbidden.

## 9. Software Catalogue / AppImage contract

The graphical SLOPOS Software Catalogue handles **AppImages only**. System packages remain the responsibility of the base distribution.

A catalogue entry may be installable only when trusted metadata contains:

- HTTPS download URL;
- architecture;
- version;
- valid non-placeholder SHA-256 (or stronger verified signature metadata);
- enough metadata to create a safe desktop entry.

Installation must:

1. fail if metadata is incomplete or untrusted;
2. fail if download fails;
3. never synthesize a replacement executable;
4. verify digest/signature before installation;
5. write via a temporary/`.part` path;
6. atomically place the final AppImage where practical;
7. mark executable only after verification;
8. create desktop integration only after success;
9. report errors visibly to the user.

Fail-open behavior is a release blocker.

## 10. Theme ownership

`themes/platinum/` and the installed GTK/Openbox theme must describe the same visual system. Do not allow token files, GTK CSS and rendered screenshots to diverge.

The theme should cover at least:

- menus and menu items;
- buttons/default buttons/disabled buttons;
- entries;
- lists and selected rows;
- notebook/tabs where still used;
- checkboxes/radio buttons;
- sliders/progress;
- scrollbars;
- tooltips;
- dialogs/alerts;
- status areas;
- shell launcher/catalogue/settings-specific containers.

## 11. No fake functionality rule

This is a global release rule:

> A control that does not execute its advertised behavior must not appear enabled.

Examples of forbidden behavior:

- a resolution selector whose Apply button always chooses 1920×1080 regardless of selection;
- a volume slider that does not set volume;
- a dark-mode switch that changes nothing;
- a Power timeout control that never changes a timeout;
- menu items with empty callbacks;
- “Install AppImage” when trusted integrity metadata is absent.

## 12. Docker/Xvfb development and release QA

All SLOPOS-I release QA (both Functional QA and Visual QA) is performed inside Docker-based environments.

The automated X11 QA flow executes:

```text
Linux container (Docker)
  → Xvfb / Xephyr virtual X11 server
  → slopos-session supervisor
  → Openbox window manager
  → slopos-shell
  → real upstream applications & AppImages
  → functional assertions (EWMH, AT-SPI, D-Bus, IPC, virtual audio/network)
  → high-resolution canonical screenshot capture
  → automated & independent vision-model critique
```

### Validation Boundary

The SLOPOS-I 1.0 readiness score measures:

> **SLOPOS-I Docker-validated product readiness**

Physical hardware validation (such as physical Wi-Fi/Bluetooth chipsets, physical speaker codecs, physical GPU silicon, monitor hotplug, ACPI suspend/resume hardware states, and external bare-metal/VM hypervisors) is explicitly out of scope for the automated 1.0 readiness score. Hardware services are validated via container-contained virtual fixtures (virtual PipeWire/PulseAudio audio sinks, Linux network namespaces, BlueZ D-Bus integration, XRandR virtual displays, and session recovery).

## 13. Visual acceptance

Canonical scenes:

1. clean desktop (Platinum light theme);
2. system menu open;
3. Search palette open;
4. D-Bus notification;
5. modal dialog (About SLOPOS-I);
6. active application window (Mousepad with real imported GMenu or clean fallback);
7. multi-window / overlapping windows with Alt+Tab focus;
8. file manager (PCManFM with SLOPOS-Platinum icon theme);
9. terminal (Xfce4 Terminal);
10. Software Catalogue;
11. Settings hub (compact Control Panels);
12. Graphite dark appearance;
13. Graphite Settings / Catalogue presentation;
14. Ultrawide layout (3440×1440);
15. HiDPI (2× GTK scale);
16. Multi-window workspace state.

Captured across 1280×800, 1366×768, 1920×1080, 2560×1440, 3440×1440, 3840×2160, and 5120×2880.

A vision critic evaluates every canonical scene against the visual rubric:

| Criterion | Points |
|---|---:|
| Layout consistency | 20 |
| Typography | 15 |
| Spacing & alignment | 15 |
| Widget consistency | 15 |
| Window chrome & decorations | 10 |
| Visual hierarchy | 10 |
| Upstream application integration | 5 |
| Iconography | 5 |
| Resolution / scale robustness | 5 |
| **Total** | **100** |

Release visual gate: **≥90/100 per canonical scene and ≥95/100 average**, with no clipping, truncation, overlapping widgets, missing icons, unstyled core controls or theme leakage.

## 14. CI and tests

CI validates the X11 product exclusively.

Required continuous gates:

- locked Cargo metadata;
- Linux workspace build, clippy (-D warnings), rustfmt (--check), unit & integration tests;
- Debian package build, inspection and clean-root extraction;
- Arch package build, inspection and clean-root extraction;
- clean-root installation and session launch from installed binaries;
- X11 window management, EWMH/ICCCM compliance and supervisor recovery;
- upstream application matrix and AppImage Catalogue lifecycle;
- Settings delegates, virtual audio sink, network transitions, BlueZ mock;
- AT-SPI accessibility tree, Orca screen reader with speech evidence, and UTF-8 locales;
- performance budgets (startup latency, memory RSS, soak stability);
- security audit and failure injection;
- canonical screenshot capture and vision QA.

## 15. Packaging/install

The installer and packages install only the four SLOPOS binaries:

- `slopos-session`;
- `slopos-shell`;
- `slopos-catalogue`;
- `slopos-settings`.

Alongside helper scripts (`start-slopos-i`, `start-slopos-browser`, `slopos-appearance`, `slopos-recovery`), X11 session descriptors, Openbox configs/themes, GTK themes, and SLOPOS icon assets.

## 16. Docker-validated 100-Point Scorecard

| Domain | Weight | Acceptance Criteria |
|---|---:|---|
| A. Desktop UX and visual polish | 15 | Canonical scenes pass vision audit (≥90 scene, ≥95 avg), Platinum/Graphite themes, icon theme, top bar, search, notifications. |
| B. Core desktop/window/session behavior | 15 | Openbox/shell supervision, bounded crash recovery, single-instance lock, EWMH window management, workspaces, Alt+Tab, shortcuts. |
| C. Upstream application compatibility | 12 | PCManFM, Xfce4 Terminal, Mousepad, Ristretto, Zathura, MPV, Galculator, Chromium/Firefox, SuperTux, AppImage execution. |
| D. Virtual display/input/X11 integration | 10 | 1366×768 to 5120×2880 resolutions, 2× GTK HiDPI, virtual multi-monitor XRandR geometry, proper panel bounds. |
| E. System-service integration | 10 | Settings hub delegates, virtual PipeWire/PulseAudio sink PCM capture, network transitions, BlueZ D-Bus integration. |
| F. AppImage Software Catalogue | 8 | Fail-closed HTTPS/SHA-256/ELF validation, local HTTP fixture, install, launch, desktop integration, update, removal. |
| G. Installation and clean first-start | 8 | Clean-root installation, Debian package build/payload, Arch package build/payload, session starts from installed paths. |
| H. Updates and recovery | 7 | `slopos-recovery` under destructive corruption, backup verified, supervisor survives, update simulation from previous layout. |
| I. Performance and resource behavior | 5 | Startup budgets (<2s session, <500ms shell/settings/catalogue, <100ms search), RSS <150MB, idle CPU <2%, soak stability. |
| J. Accessibility and localization | 4 | AT-SPI tree audit, Orca screen reader speech evidence, keyboard navigation, UTF-8 locales (en, fr, de, ar, he). |
| K. Security and failure handling | 3 | Adversarial URL/path traversal/symlink tests, no shell injection, graceful failure under killed D-Bus/X11/services. |
| L. QA and release engineering | 3 | One-command master Docker QA runner (`run-release-qa.sh`), exact source SHA tracking, clean CI, verified TRUTH ledger. |
| **Total** | **100** | **Objective, Docker-reproducible evidence required for all 100 points.** |

## 17. Definition of done

SLOPOS-I is 100/100 only when all weighted requirements have objective Docker-reproducible evidence and:

- X11 session boots reliably from clean-installed root;
- real upstream applications and AppImages launch and operate correctly;
- shell geometry adapts across the complete resolution and scaling matrix;
- every visible core control is functional or honestly disabled;
- AppImage installation is fail-closed and integrity-verified;
- Debian and Arch packages build cleanly and extract complete payloads;
- recovery restores defaults idempotently under configuration corruption;
- virtual audio sink captures valid PCM and volume controls operate;
- AT-SPI and Orca announce desktop surfaces;
- canonical screenshots independently pass the visual gate (≥90 per scene, ≥95 mean);
- `README.md`, `AGENTS.md` and `TRUTH.md` describe the same shipping product;
- there are zero known release-blocking defects.

