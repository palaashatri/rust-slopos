# AGENTS.md — SLOPOS-I Product Contract

**Authority:** This file is the normative development contract for SLOPOS-I. `README.md` is end-user documentation. `TRUTH.md` is the evidence-backed readiness ledger. When implementation, comments, screenshots or old planning material disagree with this file, this file wins.

## 1. Mission

SLOPOS-I is an original, polished, consumer-oriented Linux desktop environment built on mature X11 infrastructure.

> **Own the experience. Do not unnecessarily own the infrastructure.**

> **SLOPOS-I is opinionated about the desktop environment, not the surrounding Linux system. The standalone desktop must coexist with the user's chosen distribution, display manager, authentication stack, screen locker, bootloader and other mature system infrastructure. Reference images may choose defaults only to provide a complete demonstrable system.**

> **A mandatory red release pipeline lane automatically prevents SLOPOS-I from being considered release-ready.**

The product goal is not merely to demonstrate a shell. It is to provide a desktop that an ordinary user can install, understand and use every day, with coherent visual design, predictable window behavior, real settings, normal Linux application compatibility and a reliable release/install path.

SLOPOS-I must have its own identity. Product copy, code comments, asset names and UI labels must not depend on another operating-system vendor's product names, trademarks or proprietary visual assets.

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
  + maintained X11 composition layer when needed for shadows/corners/transitions
        ↓
slopos-session
        ↓
slopos-shell
  ├─ top system bar
  ├─ application search
  ├─ Application Strip
  ├─ notifications
  └─ desktop/session integration
        ↓
slopos-settings + slopos-catalogue
        ↓
Normal Linux applications
```

### Display Manager Neutrality & Screen Locker Policy

- **Display Manager Neutrality:** The standalone `slopos-i` package must not mandate or depend upon LightDM. It must install a standards-compliant `/usr/share/xsessions/slopos-i.desktop` session entry compatible with any modern display manager (GDM, SDDM, LightDM, LXDM, XDM). Reference live ISOs may choose LightDM as a default convenience greeter, but LightDM is never an architectural requirement.
- **Screen Locking Abstraction:** SLOPOS-I uses a multi-tiered session-lock abstraction:
  1. D-Bus freedesktop screensaver interface (`org.freedesktop.ScreenSaver`);
  2. D-Bus GNOME screensaver interface (`org.gnome.ScreenSaver`);
  3. Concrete installed locker executables (`light-locker-command`, `i3lock`, `slock`, `xscreensaver`, `xflock4`).
  If no functional screen locker is installed, the desktop disables the lock-screen action with a clear explanation instead of faking success via `loginctl lock-session`.

### Non-goals

SLOPOS-I must not introduce:

- a Wayland session or fallback;
- a custom display server;
- a custom window manager merely for project ownership;
- a custom general-purpose GUI toolkit;
- first-party replacements for mature file managers, terminals, browsers, media players or document viewers;
- speculative kernel or future-OS architecture in the SLOPOS-I product contract;
- fake controls or fake compatibility claims.

Openbox is infrastructure. It may be paired with a maintained X11 compositing manager if required to achieve the SLOPOS visual contract. Replacing Openbox is permitted only after an evidence-backed architecture review demonstrates a material reliability, maintenance or UX advantage and preserves ordinary X11 application behavior.

## 3. Required internal architecture

The current shell must evolve away from timer-driven subprocess polling.

### 3.1 X11 integration

Create one long-lived X11 integration layer, backed by `x11rb`, that owns:

- active-window/focus tracking;
- EWMH state changes such as fullscreen and maximization;
- root-window property changes;
- monitor/RandR topology changes;
- pointer-edge events required by Application Strip reveal behavior;
- active-window metadata and class lookup.

Subscribe to X11 events and update GTK state from those events. Do **not** spawn `xdotool`, `xprop`, `xrandr` or `wmctrl` on 100–500 ms timers.

Command-line utilities may remain bounded fallbacks for explicit user actions when no stable API is practical. They are not an event bus.

### 3.2 System service integration

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

The duplicate `appmenu.rs` / `gmenu.rs` implementations must be consolidated. Dead protocol implementations must be removed after the selected implementation has equivalent tests.

### 3.4 Shell module boundaries

`slopos-shell` is allowed to be one process, but it must not be one architectural blob.

Target boundaries:

```text
shell/
  x11/            event source, EWMH, monitor model
  services/       audio/network/power/session adapters
  menu/           exported application-menu bridge
  topbar/         top-bar widgets and presentation
  dock/           Application Strip presentation/state
  launcher/       desktop-entry index + search UI
  notifications/  notification service + toast UI
  theme/          appearance tokens and runtime selection
```

Business/state logic must be testable without constructing a full GTK window wherever practical.

### 3.5 Session ownership

`slopos-session` supervises session processes and establishes environment state. It must not behave like an installer.

At runtime it may create or migrate **user-owned** files under XDG user directories. It must not attempt to populate `/usr/share`, `/usr/local/share` or other system directories. System assets belong to packages/images and are installed before login.

The supervisor must use bounded restart/backoff behavior for critical components and terminate children cleanly when the session ends.

### 3.6 Settings architecture

`slopos-settings` is a coherent control-panel shell, not a reimplementation of every Linux service.

Split the current monolithic settings source into panel/provider modules. Each panel must declare:

- whether it is built in or delegated;
- how availability is detected;
- the real action/provider it invokes;
- a testable unavailable state.

Appearance and SLOPOS-owned preferences may be native panels. Hardware/system services may delegate to mature utilities when that is the more reliable solution.

## 4. Original SLOPOS visual language

The canonical appearance is **SLOPOS Platinum**. It is an original visual system with compact desktop ergonomics, soft depth, careful typography and restrained color.

The visual target is polished and intentional rather than retro-for-retro's-sake. The desktop must not look like a default GTK installation with a gray stylesheet applied on top.

### 4.1 Geometry

The previous global square-control rule is removed.

Canonical targets at 1× scale:

- normal push buttons: approximately 6 px corner radius;
- text fields and compact controls: approximately 6 px radius;
- menus/popovers: approximately 8 px radius;
- search/notification/dialog content surfaces: approximately 10–12 px radius;
- Application Strip outer surface: approximately 12–16 px radius;
- client-side window decoration: approximately 8–10 px outer radius when technically supported;
- 1 px keylines for separation instead of heavy black boxes.

Radii must scale coherently and be adjusted where GTK/Openbox limitations require it. A component must not become pill-shaped unless its semantics call for a pill.

Do not apply `border-radius: 0` globally.

### 4.2 Depth and window chrome

Use:

- subtle neutral gradients only where they improve hierarchy;
- soft compositor shadows around floating surfaces;
- crisp but restrained active/inactive window distinction;
- compact title bars;
- consistent window controls with clear hover/pressed states;
- light separators and hairlines;
- depth through luminance and shadow before chunky pseudo-3D borders.

Avoid:

- thick black menu/window outlines as the default language;
- large raised/sunken 1990s-style bevels on every control;
- square tiles everywhere;
- generic toolkit defaults leaking into first-party SLOPOS surfaces;
- giant cards, oversized whitespace or touch-first control sizing on desktop;
- gratuitous glass effects that reduce text clarity.

### 4.3 Typography

Use redistributable fonts only. Text must remain crisp at 1× and HiDPI.

Establish shared typography tokens for:

- system/menu text;
- body text;
- secondary labels;
- section titles;
- monospace text.

Do not hard-code unrelated font sizes independently across shell modules.

### 4.4 Color

Keep one canonical light palette before multiplying themes. Platinum should use neutral layered surfaces and a restrained SLOPOS accent.

Dark and OLED appearances are secondary variants of the same component system, not separate design languages.

### 4.5 Icons and assets

Ship original or license-compatible assets only. Do not copy proprietary logos, fonts, icons, wallpapers, sounds or other resources.

Asset filenames and UI labels must use SLOPOS/generic terminology rather than third-party product names.

## 5. Window behavior

Openbox provides ICCCM/EWMH window management. SLOPOS integration must support:

- overlapping windows;
- predictable focus;
- drag and resize;
- minimize/maximize/restore/fullscreen;
- transient/modal relationships;
- Alt+Tab;
- multiple workspaces;
- top-bar and Application Strip reserved-area behavior;
- dynamic monitor changes;
- correct behavior on more than one monitor;
- supported integer HiDPI scaling.

Fullscreen and maximized state must be driven from X11/EWMH state changes rather than high-frequency subprocess polling.

## 6. Multi-monitor model

Do not treat the X11 virtual desktop rectangle as one monitor.

Maintain an explicit monitor model containing geometry, primary/output identity and scale assumptions. Define which monitor owns:

- the primary system bar;
- Application Strip placement;
- search palette placement;
- notifications;
- newly opened first-party dialogs.

The shell must respond to RandR topology changes without requiring restart.

## 7. Upstream application policy

Prefer mature, maintained Linux applications for ordinary tasks. A selected application may change according to distribution availability and maintenance status.

SLOPOS may theme or integrate with upstream applications through documented, redistributable mechanisms. Do not fork applications merely to make screenshots more uniform.

An application's own controls and menu must continue working even when optional SLOPOS integration is unavailable.

## 8. No fake functionality

This is a release-blocking rule:

> **An enabled control must execute the behavior it advertises.**

Forbidden examples include:

- empty callbacks;
- guessed application-menu shortcuts presented as guaranteed commands;
- settings that only update a label without changing the underlying state;
- an installer workflow that catches an image-build failure and still reports success;
- an artifact upload step that silently accepts a missing required release artifact;
- a claimed architecture that has never been built and boot-tested;
- a visual score generated without a reproducible scoring method and real review.

When functionality cannot be implemented reliably, disable or omit the control and document the limitation in `TRUTH.md`.

## 9. Software Catalogue

The SLOPOS Software Catalogue handles curated AppImages only. Distribution packages remain the base distribution's responsibility.

An installable catalogue entry requires trusted metadata including:

- HTTPS source;
- architecture;
- version;
- valid digest or verified signature metadata;
- safe desktop-integration metadata.

Installation must fail closed, verify integrity before final placement, use temporary staging and report errors visibly.

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

Each package lane must install into a clean image/VM and start a SLOPOS X11 session before release promotion.

### 10.2 Package repositories

A downloadable `.deb` is not the same thing as `apt install` support.

Alpha release readiness requires a signed public package repository generated by CI:

- APT repository metadata for Debian/Ubuntu-family users;
- Pacman repository metadata for Arch-family users;
- channel separation such as `alpha` and later `stable`;
- documented one-time repository enrollment followed by normal package-manager installation and upgrade.

Package installation, upgrade and removal must not require a source checkout or Rust toolchain.

### 10.3 Bootable media

Do not force every CPU architecture into an `.iso` filename when that is not the native/reliable boot format for the platform.

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

- use `set -euo pipefail` or equivalent strict failure behavior;
- never wrap a required build in `|| { echo ...; }` and continue;
- use `if-no-files-found: error` for required artifacts;
- verify non-zero size and expected file type;
- calculate SHA-256 checksums;
- record the exact source commit;
- boot-test media before promotion;
- publish alpha artifacts from a dedicated release workflow only after gates pass.

## 11. Architecture support claims

A package manifest listing `aarch64` or `riscv64` does not establish support.

An architecture is supported only when CI proves:

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

- Rust formatting, Clippy and unit/integration tests;
- shell/script syntax and security tests;
- clean package install tests;
- installed-VM tests;
- bootable-media tests;
- multi-monitor and resolution tests;
- accessibility checks;
- failure/recovery tests;
- visual regression capture;
- human visual acceptance for release candidates.

Container/Xvfb tests are valuable but cannot replace all installed-VM, boot-media and hardware-facing validation.

Screenshots prove only what is visible in that captured environment. They do not prove package availability, architecture support or hardware compatibility.

## 13. Documentation truth rules

`README.md` must be understandable by a non-developer and must describe only user-visible behavior that exists.

`TRUTH.md` must:

- identify the exact audited commit;
- distinguish static code evidence from executed tests;
- list release blockers explicitly;
- never retain a 100/100 score when a required release lane is unbuilt, unbooted or fail-open;
- be updated whenever architecture or release evidence materially changes.

Old planning documents must not override the three root truth documents. Stale plans that describe superseded branding or architecture should be removed rather than left as competing specifications.

## 14. Definition of 100/100

SLOPOS-I reaches 100/100 only when all of the following are simultaneously true:

- the visual system is coherent, polished and original across first-party surfaces;
- no release-visible fake controls remain;
- X11 integration is event-driven rather than subprocess-polled;
- global-menu behavior is protocol-backed and truthful;
- shell/settings code is modular enough for focused testing;
- packages install, upgrade and remove cleanly;
- signed package repositories exist;
- x86_64 live media is built and boot-tested automatically;
- ARM64 and RISC-V release artifacts meet their defined acceptance lanes before being called supported;
- CI publishes checksummed artifacts from exact source revisions;
- installed VM and boot-media acceptance passes;
- README and TRUTH describe the same shipping reality;
- no known release-blocking defect is hidden behind a self-awarded score.

Until every required gate passes, `TRUTH.md` must report the actual evidence-backed score and remaining blockers.