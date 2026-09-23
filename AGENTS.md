# AGENTS.md — SLOPOS-I Product Contract

**Authority:** This file is the normative development contract for SLOPOS-I. `README.md` is end-user documentation. `TRUTH.md` is the evidence-backed readiness ledger. When implementation, comments, screenshots, old planning material, model assumptions, or previous audit prose disagree with this file, this file wins.

> **Anti-hallucination rule:** If a visual or product goal is not explicitly written in `AGENTS.md` or directly observable in the canonical SLOPOS reference, the reviewer must mark it **UNKNOWN** rather than inventing a requirement.

## 1. Mission

SLOPOS-I is an original, polished, consumer-oriented Linux desktop environment built on mature X11 infrastructure.

> **Own the experience. Do not unnecessarily own the infrastructure.**

The product goal is not merely to demonstrate a shell. It is to provide a desktop that an ordinary user can install, understand, and use every day, with coherent visual design, predictable window behavior, real settings, normal Linux application compatibility, and a reliable release/install path.

SLOPOS-I must have its own identity. Product copy, code comments, asset names, and UI labels must not depend on another operating-system vendor's product names, trademarks, logos, fonts, or proprietary visual assets.

A mandatory red release gate automatically prevents SLOPOS-I from being considered release-ready.

## 2. Product architecture

SLOPOS-I is **X11-only** for this product generation.

The supported conceptual stack is:

```text
Linux + systemd/logind/udev
  ├─ NetworkManager
  ├─ PipeWire/WirePlumber
  ├─ BlueZ
  ├─ UPower
  └─ distribution package manager
        ↓
X.Org-compatible X11 server
        ↓
Openbox window manager
        ↓
slopos-session
        ↓
slopos-shell
  ├─ top global menu/system bar
  ├─ application search
  ├─ notifications
  └─ desktop/session integration
        ↓
slopos-settings + slopos-catalogue
        ↓
Normal Linux applications and explicitly scoped SLOPOS utilities
```

### Display manager neutrality and screen-locker policy

- The standalone `slopos-i` package must not mandate one display manager. It must install a standards-compliant `/usr/share/xsessions/slopos-i.desktop` session entry compatible with modern X11 display managers.
- Reference live images may choose a default greeter for convenience, but that greeter is not an architectural requirement.
- Screen locking must use a real available mechanism. If no functional locker exists, disable the action and explain why instead of faking success.

### Architectural non-goals

SLOPOS-I must not introduce:

- a Wayland session or fallback in this product generation;
- a custom display server merely for project ownership;
- a custom window manager merely for project ownership;
- a custom general-purpose GUI toolkit merely for project ownership;
- speculative kernel or future-OS architecture in this product contract;
- fake controls or fake compatibility claims;
- a bottom dock, Application Strip, or any reserved bottom-dock work area.

Do not rewrite mature applications merely for sovereignty. A first-party utility or replacement surface is allowed when it is explicitly scoped and materially required for visual or functional cohesion, and its maintenance cost is accepted in this contract or a later explicit amendment.

Openbox is infrastructure. Replacing it is permitted only after an evidence-backed architecture review demonstrates a material reliability, maintenance, or UX advantage while preserving ordinary X11 application behavior.

## 3. Required internal architecture

The shell must evolve away from timer-driven subprocess polling.

### 3.1 X11 integration

Maintain one long-lived X11 integration layer, backed by `x11rb`, that owns:

- active-window/focus tracking;
- EWMH state changes such as fullscreen and maximization;
- root-window property changes;
- monitor/RandR topology changes;
- active-window metadata and class lookup;
- work-area and placement state required by the top bar and desktop.

Subscribe to X11 events and update GTK state from those events. Do **not** spawn `xdotool`, `xprop`, `xrandr`, or `wmctrl` on high-frequency timers.

Command-line utilities may remain bounded fallbacks for explicit user actions when no stable API is practical. They are not an event bus.

### 3.2 System-service integration

Prefer stable APIs and D-Bus signals for changing system state:

- NetworkManager for connectivity;
- UPower for battery/power state;
- systemd/logind for session and power actions;
- the installed audio stack for volume/mute state;
- BlueZ for Bluetooth availability where SLOPOS displays live state.

The clock must use an in-process local-time API. Do not spawn `date` every second.

### 3.3 Global application menu

There must be exactly one global-menu architecture.

The shell may display an application's menu only when the application exports a supported real menu/action model. GTK/GIO menu/action exports and compatible D-Bus menu exporters are valid integration paths.

If an application does not export a usable menu:

- keep its own local menu visible; or
- show only shell-owned window actions whose semantics are guaranteed.

Do **not** invent application menus from window titles/classes and guessed keyboard shortcuts. Do not expose enabled menu items backed by empty callbacks.

### 3.4 Shell module boundaries

`slopos-shell` may be one process, but it must not be one architectural blob.

Target boundaries:

```text
shell/
  x11/            event source, EWMH, monitor model
  services/       audio/network/power/session adapters
  menu/           exported application-menu bridge
  topbar/         global menu/system-bar presentation
  desktop/        wallpaper, desktop-object integration
  launcher/       desktop-entry index + search UI
  notifications/  notification service + toast UI
  theme/          appearance tokens and runtime selection
```

Business/state logic must be testable without constructing a full GTK window wherever practical.

### 3.5 Session ownership

`slopos-session` supervises session processes and establishes environment state. It must not behave like an installer.

At runtime it may create or migrate **user-owned** files under XDG user directories. It must not attempt to populate `/usr/share`, `/usr/local/share`, or other system directories. System assets belong to packages/images and are installed before login.

The supervisor must use bounded restart/backoff behavior for critical components and terminate children cleanly when the session ends.

### 3.6 Settings architecture

`slopos-settings` is a coherent Control Panels-style shell, not a reimplementation of every Linux service.

Each panel must declare:

- whether it is built in or delegated;
- how availability is detected;
- the real action/provider it invokes;
- a testable unavailable state.

SLOPOS-owned preferences may be native. Hardware/system services may delegate to mature utilities when that is more reliable, but delegated windows still need to be evaluated for visual coherence in release screenshots.

## 4. Canonical SLOPOS classic visual language

The canonical appearance is **SLOPOS Platinum Classic**.

The visual target is not a vague "retro" theme and not a generic GTK skin. It is a clean-room recreation of the **late-1990s compact platinum desktop feel** represented by the repository's canonical reference asset:

- `qa/reference/slopos-classic-reference.svg`
- machine-readable review contract: `qa/reference/slopos-classic-reference.json`

The reference is derived from the user-provided late-1990s desktop screenshot. Third-party branding, product names, logos, proprietary artwork, and non-goal elements were removed or replaced. It is a **composition, geometry, density, and interaction-style reference**, not permission to copy proprietary assets.

### 4.1 Required visual characteristics

At 1× scale, the canonical desktop must trend toward:

- a **full-width top global menu bar** approximately 20–24 px tall;
- a **dockless desktop** with no persistent bottom launcher surface;
- square or nearly square, compact window geometry;
- thin dark keylines around windows and important controls;
- compact title bars with restrained striped/linear texture where useful;
- clearly raised/sunken classic control states rather than modern flat cards;
- small, dense menu and body typography;
- compact button, field, scrollbar, and list spacing;
- dense icon-oriented file/system browsing;
- small utility-window proportions for Calculator, About, Notes, and similar tools;
- a blue desktop field with high contrast against platinum windows;
- right-aligned desktop objects, with Trash near the lower-right in the canonical composition;
- overlapping windows and a visually busy but readable desktop composition.

The target is **desktop-dense**, not touch-first.

### 4.2 Window chrome

Use:

- hard-edged platinum-gray frame surfaces;
- compact title bars;
- thin dark outer borders;
- visible active/inactive distinction;
- small, legible window controls;
- classic beveled or recessed states where the reference shows depth;
- mechanical-looking scrollbars and resize affordances where supported.

Avoid:

- pervasive rounded corners;
- large compositor shadows as the primary depth cue;
- glass/translucency as a primary visual language;
- oversized client-side header bars;
- modern card layouts;
- contemporary traffic-light-style window controls;
- generic toolkit defaults leaking into first-party surfaces.

### 4.3 Widgets and controls

Controls should read as compact desktop controls, not mobile/touch controls.

Canonical characteristics:

- little to no corner radius by default;
- visible 1 px light/dark bevel hierarchy;
- compact push buttons;
- recessed text fields and list wells;
- compact checkboxes/radio controls;
- visible, narrow scrollbars;
- navy/dark-blue selection highlight with high-contrast text where appropriate;
- dense icon labels with minimal unnecessary whitespace.

Do not apply modern pill styling unless an explicitly documented feature requires it.

### 4.4 Typography

Use redistributable fonts only. Text must remain crisp at 1× and HiDPI.

The visual system must establish shared typography tokens for:

- global menu/system text;
- window/title text;
- body text;
- secondary labels;
- section titles;
- monospace text.

Typography should remain compact and visually close to the density of the reference without copying proprietary fonts.

### 4.5 Color

Keep one canonical light palette before multiplying themes.

Platinum Classic should use:

- light neutral gray window surfaces;
- white/recessed content wells;
- dark keylines;
- a restrained navy/dark-blue selection/accent;
- the canonical blue desktop background.

Alternative dark/OLED themes are secondary and must not influence the canonical classic acceptance score.

### 4.6 Icons and assets

Ship original or license-compatible assets only. Do not copy proprietary logos, fonts, icons, wallpapers, sounds, or other resources.

The reference's folder/device icons are deliberately generic clean-room approximations. Production assets may improve them while preserving scale, density, and composition.

### 4.7 Explicit visual non-goals

Models and contributors must **not** infer any of the following as desired modernization:

- no bottom dock or Application Strip;
- no Aqua/glass visual language;
- no modern rounded desktop UI;
- no GNOME/libadwaita card-heavy settings design;
- no KDE Breeze visual language;
- no Windows 10/11 Fluent visual language;
- no giant touch-friendly spacing;
- no oversized rounded buttons or pill-heavy controls;
- no sidebar-heavy Settings redesign unless explicitly added to this file;
- no third-party branding recreation;
- no invented "modernization" goals simply because a contributor thinks they look better.

If an implementation decision is not specified here or directly observable in the canonical reference, mark it **UNKNOWN** and preserve the existing behavior until an explicit decision is made.

## 5. Window behavior

Openbox provides ICCCM/EWMH window management. SLOPOS integration must support:

- overlapping windows;
- predictable focus;
- drag and resize;
- minimize/maximize/restore/fullscreen;
- transient/modal relationships;
- Alt+Tab;
- multiple workspaces;
- top-bar reserved-area behavior;
- dynamic monitor changes;
- correct behavior on more than one monitor;
- supported integer HiDPI scaling.

Fullscreen and maximized state must be driven from X11/EWMH state changes rather than high-frequency subprocess polling.

## 6. Multi-monitor model

Do not treat the X11 virtual desktop rectangle as one monitor.

Maintain an explicit monitor model containing geometry, primary/output identity, and scale assumptions. Define which monitor owns:

- the primary global menu/system bar;
- search palette placement;
- notifications;
- desktop objects;
- newly opened first-party dialogs.

The shell must respond to RandR topology changes without requiring restart.

## 7. Upstream application policy

Prefer mature, maintained Linux applications for ordinary tasks unless an explicit SLOPOS utility is justified by the product contract.

SLOPOS may theme or integrate with upstream applications through documented, redistributable mechanisms. A private profile/configuration is acceptable when it improves visual cohesion without silently changing the user's unrelated desktop configuration.

An application's own controls and menu must continue working even when optional SLOPOS integration is unavailable.

Visual QA must include representative upstream applications because a shell that looks coherent only in first-party windows is not sufficient.

## 8. No fake functionality

This is a release-blocking rule:

> **An enabled control must execute the behavior it advertises.**

Forbidden examples include:

- empty callbacks;
- guessed application-menu shortcuts presented as guaranteed commands;
- settings that only update a label without changing underlying state;
- an installer workflow that catches a required build failure and still reports success;
- an artifact upload step that silently accepts a missing required artifact;
- a claimed architecture that has never been built and boot-tested;
- a visual score generated without current screenshots and an explicit review method.

When functionality cannot be implemented reliably, disable or omit the control and document the limitation in `TRUTH.md`.

## 9. Software Catalogue

The SLOPOS Software Catalogue handles curated AppImages only. Distribution packages remain the base distribution's responsibility.

An installable catalogue entry requires trusted metadata including:

- HTTPS source;
- architecture;
- version;
- valid digest or verified signature metadata;
- safe desktop-integration metadata.

Installation must fail closed, verify integrity before final placement, use temporary staging, and report errors visibly.

## 10. Packaging and consumer installation

Source installation is not sufficient for consumer readiness.

SLOPOS-I must produce release artifacts entirely from GitHub Actions or an equivalent reproducible CI service. A developer laptop must not be the canonical release machine.

### 10.1 Package artifacts

Required package lanes:

- Debian/Ubuntu-family `.deb`;
- Arch package artifact;
- checksums;
- source commit provenance;
- SBOM/provenance metadata where practical.

Each release package lane must install into a clean image/VM and start a SLOPOS X11 session before release promotion.

### 10.2 Package repositories

A downloadable package is not the same thing as normal package-manager support.

Alpha release readiness requires signed public package repositories generated by CI:

- APT repository metadata for Debian/Ubuntu-family users;
- Pacman repository metadata for Arch-family users;
- channel separation such as `alpha` and later `stable`;
- documented one-time repository enrollment followed by normal package-manager installation and upgrade.

Package installation, upgrade, and removal must not require a source checkout or Rust toolchain.

### 10.3 Bootable media

Required artifact matrix:

| Target | Required consumer artifact |
|---|---|
| x86_64 Arch-family | bootable live ISO + package |
| x86_64 Ubuntu-family | bootable live ISO + `.deb` |
| ARM64 | bootable UEFI image/media appropriate to the distribution + native package |
| RISC-V 64 | QEMU `virt` bootable disk image first; broader hardware images only when board support is defined |

Every advertised bootable artifact must be boot-tested in QEMU or appropriate hardware/emulation and must reach a usable graphical SLOPOS session.

### 10.4 GitHub Actions release rules

Release workflows must:

- use strict failure behavior;
- never swallow a required build failure and continue;
- fail when required artifacts are missing;
- verify non-zero size and expected file type;
- calculate SHA-256 checksums;
- record the exact source commit;
- boot-test media before promotion;
- publish alpha artifacts from a dedicated release workflow only after gates pass.

Normal development pushes should not trigger every expensive packaging/VM/media lane. Routine CI should remain small and high-signal; release, packaging, full resolution, and installed-VM matrices should run manually, on release candidates, or when their relevant paths change.

## 11. Architecture support claims

A package manifest listing `aarch64` or `riscv64` does not establish support.

An architecture is supported only when reproducible evidence proves:

1. dependency availability;
2. compilation;
3. package/image construction;
4. boot or install in the target environment;
5. X11 session startup;
6. shell/settings/catalogue smoke tests;
7. representative application launch;
8. artifact publication.

Until then, describe it as an intended target.

## 12. QA contract

Testing is evidence, not a scoreboard generator.

Required layers:

- Rust formatting, Clippy, and unit/integration tests;
- shell/script syntax and security tests;
- representative composed-X11 runtime smoke;
- clean package install tests for release candidates;
- installed-VM tests for release candidates;
- bootable-media tests for release candidates;
- multi-monitor and resolution tests;
- accessibility checks;
- failure/recovery tests;
- visual regression capture;
- vision-based visual acceptance;
- human visual acceptance for release candidates.

Container/Xvfb tests are valuable but cannot replace all installed-VM, boot-media, and hardware-facing validation.

Screenshots prove only what is visible in that captured environment. They do not prove package availability, architecture support, hardware support, or functional correctness.

### 12.1 Canonical visual QA evidence

Every UI-affecting pass that claims visual progress must produce a current screenshot set from the actual composed SLOPOS session.

Required scenes:

1. **Empty desktop** — top bar, wallpaper, desktop objects, no dock.
2. **System/file-browser window** — dense icon view and classic chrome.
3. **Settings / Control Panels** — proves first-party controls follow the same language.
4. **About This Computer-style window** — compact first-party information window using SLOPOS branding only.
5. **Accessory window** — Calculator, Notes, or equivalent compact utility.

Required resolutions:

- `1280x800`
- `3440x1440`

Recommended additional evidence:

- `1920x1080`
- `3840x2160`
- supported HiDPI scale

Evidence should be stored or uploaded with stable scene names so a reviewer can compare equivalent scenes between commits.

### 12.2 Vision-review procedure

Any model with vision capability performing visual QA must compare:

- **Image A:** rendered `qa/reference/slopos-classic-reference.svg`
- **Image B:** current candidate screenshot
- **Rules:** `AGENTS.md` and `qa/reference/slopos-classic-reference.json`

The reviewer must not use its own aesthetic preferences as requirements.

Required output:

```text
Verdict: PASS / FAIL
Overall score: N/100

Category scores:
- Menu bar fidelity: N/15
- Desktop composition: N/10
- Window chrome fidelity: N/20
- Widget/control fidelity: N/15
- File-browser/icon-view fidelity: N/15
- Typography/density/spacing: N/10
- Non-goal compliance: N/15

Critical failures:
- ...

Minor deltas:
- ...

Exact observed evidence:
- ...

Unknown / ambiguous:
- ...
```

The reviewer must describe concrete observable differences: geometry, spacing, window chrome, control treatment, typography scale, icon density, desktop-object placement, and composition.

Statements such as "looks nicer", "feels modern", or "should be more contemporary" are not valid QA findings unless tied to an explicit requirement in this file.

### 12.3 Visual scoring and pass bar

| Category | Weight |
|---|---:|
| Menu bar fidelity | 15 |
| Desktop composition | 10 |
| Window chrome fidelity | 20 |
| Widget/control fidelity | 15 |
| File-browser/icon-view fidelity | 15 |
| Typography, density, spacing | 10 |
| Non-goal compliance | 15 |
| **Total** | **100** |

Visual PASS requires:

- overall score **>= 85/100**;
- no critical failure;
- window chrome **>= 15/20**;
- non-goal compliance **>= 13/15**.

Critical failures that force FAIL regardless of numeric score:

- missing full-width global top menu bar;
- visible bottom dock or Application Strip;
- obviously modern rounded/card-heavy design dominating the desktop;
- Settings or file-browser surfaces visibly breaking from the classic shell language;
- direct use of third-party branding or proprietary visual assets;
- claiming visual parity without current screenshot evidence and a vision review.

### 12.4 Missing evidence policy

- Missing evidence is **UNKNOWN**, never PASS.
- Ambiguous evidence is **UNKNOWN**, never silently assumed correct.
- A screenshot from an old commit cannot prove a newer commit.
- If screenshots regress, the score must go down.
- A vision model may not preserve a previous score merely because the previous reviewer wrote one.

## 13. Documentation truth rules

`README.md` must be understandable by a non-developer and describe only user-visible behavior that exists.

`TRUTH.md` must:

- identify the exact audited commit;
- distinguish static code evidence from executed tests;
- list release blockers explicitly;
- include the latest visual QA evidence state and score, or explicitly mark it UNKNOWN;
- never retain a 100/100 score when a required release lane is unbuilt, unbooted, fail-open, or visually unverified;
- be updated whenever architecture, release, or visual evidence materially changes.

Old planning documents must not override the three root truth documents. Stale plans that describe superseded branding, dock behavior, visual language, or architecture should be removed rather than left as competing specifications.

## 14. Definition of 100/100

SLOPOS-I reaches 100/100 only when all of the following are simultaneously true:

- the canonical visual review passes at **>= 85/100** with no critical visual failures;
- the visual system is coherent across first-party and representative upstream surfaces;
- no release-visible fake controls remain;
- X11 integration is event-driven rather than subprocess-polled in hot paths;
- global-menu behavior is protocol-backed and truthful;
- shell/settings code is modular enough for focused testing;
- packages install, upgrade, and remove cleanly;
- signed package repositories exist;
- x86_64 live media is built and boot-tested automatically for release candidates;
- ARM64 and RISC-V artifacts meet their defined acceptance lanes before being called supported;
- CI publishes checksummed artifacts from exact source revisions;
- installed-VM and boot-media acceptance passes for release candidates;
- README and TRUTH describe the same shipping reality;
- no known release-blocking defect is hidden behind a self-awarded score.

Until every required gate passes, `TRUTH.md` must report the actual evidence-backed state and remaining blockers.