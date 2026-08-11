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

## 12. Docker/Xvfb development QA

Docker Desktop/WSL2 on Windows is a first-class development environment.

The automated X11 QA flow should run:

```text
Linux container
  → Xvfb
  → slopos-session
  → Openbox
  → slopos-shell
  → real upstream applications
  → functional assertions
  → screenshots
```

The QA script must fail when required processes/windows/screenshots are absent. Screenshot commands may not be hidden behind `|| true`.

Docker is appropriate for build, X11 interaction and visual evidence. It does **not** prove real GPU, monitor hotplug, suspend/resume, Bluetooth/Wi-Fi hardware or installation/EFI behavior.

## 13. Visual acceptance

Canonical scenes:

1. clean desktop;
2. menu open;
3. Search open;
4. notification;
5. active application window;
6. active/inactive multi-window scene;
7. file manager;
8. terminal;
9. Software Catalogue;
10. Settings hub;
11. real modal/dialog state.

Review at 1280×800 plus representative 1366×768, 1920×1080 and HiDPI evidence when available.

A vision/human critic scores every canonical scene:

| Criterion | Points |
|---|---:|
| Distinct SLOPOS identity | 10 |
| System 7/Platinum alignment | 10 |
| Composition | 10 |
| Typography | 10 |
| Spacing/alignment | 10 |
| Icon consistency | 10 |
| Control consistency | 10 |
| Window chrome | 10 |
| Readability/usability | 10 |
| Overall product polish | 10 |

Release visual gate: **≥95/100 per canonical scene and ≥97/100 average**, with no obvious placeholder glyphs, clipping, unstyled core controls or theme leakage.

Automated screenshot generation alone never awards those points.

## 14. CI and tests

CI must match the X11 product and must not install/verify the removed compositor stack.

Required continuous gates:

- locked Cargo metadata;
- Linux build;
- unit/integration tests;
- rustfmt;
- clippy;
- Xvfb/Openbox smoke session;
- source-contract checks preventing accidental reintroduction of Wayland/compositor workspace members.

Stale tests referencing deleted APIs must be removed or rewritten immediately.

## 15. Packaging/install

The installer must install only the current four SLOPOS binaries:

- `slopos-session`;
- `slopos-shell`;
- `slopos-catalogue`;
- `slopos-settings`.

It must install the X11 session descriptor, Openbox config/theme and SLOPOS GTK/assets. It must not mention or require removed compositor/Wayland binaries or first-party apps.

## 16. Scorecard (100 points)

| Domain | Weight |
|---|---:|
| System 7 / Platinum visual identity & polish | 20 |
| Desktop shell / interaction | 15 |
| X11 window management integration | 10 |
| Upstream application integration | 10 |
| Software Catalogue / AppImage | 8 |
| System services integration | 8 |
| Installer / boot / session | 8 |
| Functional QA | 7 |
| Visual regression / review QA | 5 |
| Performance | 3 |
| Accessibility / localization | 3 |
| Recovery / resilience | 3 |
| **Total** | **100** |

`TRUTH.md` must deduct points for unverified or knowingly incomplete behavior.

## 17. Definition of done

SLOPOS-I is 100/100 only when all weighted requirements have objective evidence and:

- X11 session boots reliably;
- real applications behave correctly;
- shell geometry works at required resolutions;
- every visible core control is functional or honestly disabled;
- AppImage installation is fail-closed and integrity-verified;
- installer contains no obsolete architecture;
- Docker/Xvfb functional QA passes without ignored failures;
- bootable/installable VM release evidence exists;
- canonical screenshots independently pass the visual gate;
- accessibility/localization/recovery claims have evidence;
- `README.md`, `AGENTS.md` and `TRUTH.md` describe the same shipping product;
- there are zero known release-blocking defects.

Never declare 100/100 because the project compiles, a Docker script prints PASS, or screenshots were merely generated.
