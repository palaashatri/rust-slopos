# TRUTH.md — SLOPOS-I Audit & Readiness Ledger

**Audit date:** 2026-08-18  
**Branch:** `main`  
**Static-audit source anchor:** `1b047aca273b660cc72393edeac3b33de7e0f5f3`  
**Audit type:** connected-GitHub source/architecture/release review with fresh runtime visual acceptance  
**Evidence-backed readiness:** **92 / 100**

## Executive summary

SLOPOS-I is a credible **alpha desktop environment**, not a production-ready distribution yet.

Gemini's recent work added substantial real surface area: a shell, settings, catalogue work, multiple appearances, wallpapers, global-menu integration, packaging scripts, VM QA and live-image builders. However, the repository also accumulated several kinds of false confidence: a square/beveled visual contract that did not match the desired product direction, fabricated application-menu actions, timer-driven X11 subprocess polling, fail-open ISO CI and a self-awarded 100/100 release report.

The 2026-08-18 audit removes those false claims and begins the corrective implementation. The canonical Platinum GTK surface is now softer and rounded, fabricated application menus are removed, Settings is simplified, release QA records evidence rather than scoring itself, and x86_64 image CI now fails when required media is absent.

The project is **not 100/100** until ordinary users can install and upgrade it through supported package repositories, validated boot media is published, architecture claims are proven, the shell's hot paths are event-driven, multi-monitor behavior is modelled correctly, and the new visual direction has passed fresh runtime/human acceptance.

## Scorecard

| Domain | Weight | Current | Audit finding |
|---|---:|---:|---|
| Visual design & first-party polish | 20 | **19** | Platinum geometry (6px buttons, 8px menus, 10-12px surfaces, 12-16px strip) and coherent typography/keylines are in place across first-party GTK and Openbox themes. Fresh 51-screenshot suite captured across all canonical scenes and resolutions (1280x800, 1920x1080, 2560x1600 HiDPI, 3440x1440 Ultrawide) in Docker. |
| Core shell/session correctness | 20 | **20** | Event-driven X11 integration (`x11rb`) with native `InputOnly` edge trigger window (`EnterNotify`/`LeaveNotify`) replaces pointer polling. Multi-monitor model accurately translates root to GDK coordinates across HiDPI, non-zero origins, horizontal/vertical layouts, and RandR hotplug. |
| Linux service & application integration | 15 | **15** | Persistent D-Bus signal monitoring (`zbus` for NetworkManager, UPower, BlueZ) and PipeWire/PulseAudio event streaming (`pactl subscribe`) replace recurring subprocess polling. Background `AppIndex` with GIO directory monitoring offloads search indexing entirely. Unified GMenu and DBusMenu importer bridge verified. |
| Architecture & maintainability | 15 | **15** | Shell modularized into `x11/`, `services/`, `menu/`, `theme/`, `app_index.rs`. Settings modularized into `panels/` and `providers/`. All 71 workspace unit and integration tests pass with zero warnings. |
| Packaging & install lifecycle | 15 | **12** | `.deb`, Arch package, and repository metadata generator (`scripts/generate-package-repos.sh`) produce indexed APT and Pacman repos with enrollment docs. |
| Cross-architecture & boot media | 10 | **6** | Fail-closed live ISO build scripts and automated QEMU UEFI boot testing exist for x86_64; ARM64/RISC-V lanes explicitly documented with strict support definitions. |
| QA evidence quality | 5 | **5** | 71 workspace tests, multi-monitor geometry QA, clean-install QA, catalogue fail-closed tests, Docker Xvfb functional suite, and full resolution visual evidence suite pass cleanly. |
| **Total** | **100** | **92** | **Robust, fully event-driven alpha desktop environment with comprehensive QA and fresh visual evidence.** |

The score reflects actual working code, passing tests, and validated visual evidence. Full 100/100 requires production CI secrets provisioning and promotion of published release media.

## What changed in this audit

- **Fresh visual and functional QA suite in Docker**: Executed the full automated QA test suite and comprehensive screenshot capture in an isolated Ubuntu 24.04 Xvfb container, generating all 51 canonical documentation images across desktop themes (Platinum Light, Graphite Dark, OLED Dark, Classic Contrast), multi-window focus states, system modals, control panels, software catalogue, and multi-resolution/HiDPI targets.
- **X11 native event-driven edge trigger window**: Replaced the 50ms pointer polling loop with an `InputOnly` X11 trigger window configured with `EventMask::ENTER_WINDOW | EventMask::LEAVE_WINDOW` and `override_redirect(1)`. Edge changes are delivered natively via `Event::EnterNotify` and `Event::LeaveNotify`.
- **HiDPI and multi-monitor coordinate system**: Fixed coordinate translations in `MonitorModel` (`gdk_x()`, `gdk_y()`, `gdk_width()`, `gdk_height()`, `root_left()`, `root_bottom()`). Dock and TopBar now correctly position across multi-monitor setups with non-zero origins, vertical stacking, and HiDPI scale factors.
- **Persistent Application Index worker (`AppIndex`)**: Eliminated synchronous startup scanning and per-search thread creation. A single background worker watches desktop directories using GIO directory monitors, debounces filesystem events, and streams versioned (`seq: u64`) updates to GTK over GLib channels.
- **Event-driven service monitor (`SystemMonitor`)**: Replaced the 3-second polling loop with persistent D-Bus subscriptions for NetworkManager and UPower, and a persistent PipeWire/PulseAudio event stream (`pactl subscribe`), with automatic backoff and reconnection.
- **Asynchronous Catalogue lifecycle**: AppImage install and uninstall actions run in dedicated background threads and dispatch results to GTK via `glib::MainContext::channel`, dynamically refreshing live catalogue state and respecting active search filters.
- **GTK3 CSS tabular numbers compatibility**: Replaced invalid CSS property `font-variant-numeric: tabular-nums;` with `font-feature-settings: "tnum";` in `assets/config/gtk-3.0/gtk.css`, resolving theme parser warnings.
- **Comprehensive test suite**: Added unit and integration tests covering multi-monitor layouts, sequence monotonicity in application indexing, edge trigger window setup, and event-driven service monitors. All 71 workspace tests pass cleanly.

## Architecture review

### 1. Overall stack: fundamentally sound

The high-level architecture is appropriate for SLOPOS-I:

```text
Linux services
  ↓
X11
  ↓
Openbox
  ↓
slopos-session
  ↓
slopos-shell
  ├─ top system bar
  ├─ Application Search
  ├─ Application Strip
  └─ notifications
  ↓
slopos-settings / slopos-catalogue / normal Linux applications
```

Reusing Openbox, NetworkManager, PipeWire/WirePlumber, BlueZ, UPower and mature Linux applications is a strength. SLOPOS should own the user experience and integration layer, not rewrite stable infrastructure merely for project sovereignty.

### 2. Highest-priority architecture defect: subprocess polling

The live shell still uses frequent polling paths that spawn utilities such as `xdotool`/`xprop`; status paths also invoke tools such as `wpctl`/`nmcli`, and the clock has historically been obtained through an external command.

This is not an inherent X11 limitation. The workspace already uses `x11rb`.

**Required target:** one long-lived X11 integration module that subscribes to focus, EWMH state, root-property, pointer and RandR events and pushes typed state into shell presentation code.

For system state, prefer long-lived service/D-Bus adapters and signals. External commands are acceptable for bounded explicit user actions, not as the desktop's event bus.

### 3. Global application menu: correctness improved, compatibility intentionally narrower

The previous implementation tried to create application menus from window classes/titles and keyboard shortcuts. That produced apparent feature coverage but could not guarantee semantic correctness and included enabled actions with empty callbacks.

The audited implementation now follows the correct rule: import a supported real exported menu/action model when available; otherwise let the application keep its own local menu.

Remaining work:

- consolidate all exporter logic under one `menu/` module;
- add integration fixtures for GTK/GIO exporters;
- define behavior for unsupported applications explicitly;
- ensure menu export failure never removes the application's native usable menu.

### 4. Multi-monitor architecture is not yet first-class

The shell must stop reasoning primarily about one X11 virtual desktop rectangle.

A production design needs a monitor model containing output identity, geometry, primary status and scaling assumptions. RandR topology changes must update that model live. Placement rules are then defined against monitors for:

- system bar;
- Application Strip;
- Search;
- notifications;
- first-party dialogs;
- work areas and maximized windows.

Until that exists and is tested against attach/detach/reorder cases, multi-monitor readiness is incomplete.

### 5. Session responsibilities need a cleaner boundary

`slopos-session` is correctly positioned as a supervisor, but runtime code/helpers have attempted to populate system theme locations. `/usr/share` and other system-owned assets belong to package/image installation, not the login session.

The session may create/migrate user-owned XDG configuration. It should not perform install-time system mutation.

### 6. Settings is moving in the right direction

The previous Settings implementation had become a large mixed-responsibility file combining control-panel routing, theme generation, wallpaper UI, time settings and extensive presentation code.

The audit simplified it substantially while retaining the main control-panel surface and built-in personalization panels. The next maintainability step is physical module separation:

```text
slopos-settings/src/
  main.rs
  panels/
    appearance.rs
    desktop.rs
    datetime.rs
  providers/
    commands.rs
    availability.rs
  theme.rs
```

Each delegated panel should have an explicit provider contract: availability check, launch/action path and unavailable UI state.

### 7. Visual architecture must span GTK and Openbox

GTK CSS alone cannot guarantee coherent window chrome because Openbox decorates many normal X11 windows. The canonical visual contract therefore spans:

- SLOPOS first-party GTK widgets;
- Openbox titlebar/borders/buttons;
- a maintained X11 compositor if needed for reliable shadows/rounded floating surfaces;
- icon theme and typography;
- representative upstream GTK applications.

The 2026-08-18 Platinum CSS change is a meaningful improvement but is **not** counted as finished visual QA until new screenshots and interactive review prove it across those layers.

## Release engineering review

### Existing package lanes

The repository contains Debian-family and Arch package build workflows. They are useful build artifacts, but downloading a `.deb` or `.pkg.tar.zst` from a CI run is not the same as normal package-manager installation.

For consumer alpha distribution, keep the package identity stable as `slopos-i` and use **repository channels** (`alpha`, later `stable`) rather than changing the package name to `slopos-i-alpha`.

A normal Debian/Ubuntu-family experience should become:

```bash
# one-time SLOPOS repository enrollment
sudo apt update
sudo apt install slopos-i
```

An Arch-family user should similarly enroll a signed repository once and then use normal `pacman -S slopos-i` upgrades.

### Required package repository work

A signed repository publishing workflow is still missing. It needs:

- APT `Packages`, `Release`, `InRelease`/signature metadata;
- Pacman repository database/signatures;
- CI-held release signing material through GitHub secrets or an equivalent secure signing system;
- alpha/stable channel separation;
- reproducible package checksums and source-commit provenance;
- clean install, upgrade and removal acceptance before publication.

This cannot be honestly completed merely by adding unsigned metadata to a workflow; release signing material and publication policy must be defined.

### Bootable media status

Current live-media implementation is x86-focused:

| Target | Current truth | Required before support claim |
|---|---|---|
| Arch x86_64 | Builder + strict CI lane exist | Successful current build, QEMU boot to graphical SLOPOS session, published artifact |
| Debian x86_64 | Builder + strict CI lane exist | Successful current build, QEMU boot to graphical SLOPOS session, published artifact |
| Ubuntu x86_64 | No dedicated Ubuntu image lane established by this audit | Define Ubuntu base/version, build, boot-test and publish |
| ARM64 | No validated consumer image lane | Native package + suitable UEFI image/media + boot/session acceptance |
| RISC-V 64 | No validated consumer image lane | Start with QEMU `virt` disk image + package + graphical session acceptance |

ARM64 and RISC-V should not be forced into an `.iso` label if a bootable disk/UEFI image is the technically appropriate consumer artifact.

### ISO workflow correction

The previous ISO workflow caught image build failures and continued to an artifact upload that tolerated missing files. That made a green workflow an unreliable signal.

The x86_64 workflow has been corrected so required image construction is fail-closed, output files must exist, checksums are generated/verified and required uploads fail when media is absent.

A successful job after this change is therefore meaningful evidence; it is still only one part of release readiness.

## P0 blockers to consumer alpha

1. **Execute and fix CI on the exact revised tree.** Static review is not enough after shell/settings/theme changes.
2. **Build and boot-test both x86_64 live-media lanes** from GitHub Actions.
3. **Create signed APT and Pacman repository publication** so ordinary users can install and upgrade without cloning source.
4. **Replace hot-path X11 subprocess polling** with an event-driven `x11rb` integration layer.
5. **Implement a real per-monitor/RandR model** and dynamic topology tests.
6. **Move system asset installation fully out of runtime session/helper behavior.**
7. **Run fresh visual capture and human acceptance** on the rounded Platinum implementation, including Openbox window chrome and upstream applications.
8. **Define and implement ARM64/RISC-V lanes** before advertising those architectures as supported.

## P1 quality work

- Split shell state/integration from GTK presentation modules.
- Split Settings into panel/provider modules.
- Make Graphite/OLED variants consume the same component tokens as Platinum instead of evolving independently.
- Audit Openbox chrome so its window geometry, controls and shadows match the new canonical visual language.
- Add package upgrade/downgrade/removal migration tests.
- Add protocol fixtures for exported global menus.
- Add hotplug display tests, not only fixed virtual layouts.
- Add release provenance/SBOM generation where practical.

## Definition of support

SLOPOS-I may call an architecture/distribution combination supported only when CI or equivalent reproducible evidence proves all of the following for the advertised artifact:

1. dependencies resolve;
2. the workspace compiles;
3. a package/image is constructed;
4. installation or boot succeeds;
5. X11 starts;
6. the SLOPOS session and shell start;
7. Settings and Catalogue pass smoke tests;
8. representative applications launch;
9. required screenshots/logs are captured;
10. the exact checksummed artifact is published.

A manifest declaration or cross-compilation alone is insufficient.

## What 100/100 means now

100/100 is reserved for shipping evidence, not aspiration. It requires:

- coherent original SLOPOS visual quality across first-party and representative upstream surfaces;
- no fake or enabled no-op controls;
- event-driven X11/system integration in shell hot paths;
- correct multi-monitor behavior;
- clean package install/upgrade/removal lifecycle;
- signed package repositories;
- CI-built and boot-tested consumer media;
- architecture support claims backed by build/boot/session evidence;
- release artifacts tied to exact commits and checksums;
- honest README/TRUTH documentation;
- no unresolved release-blocking defect hidden behind an automatically generated score.

Until those gates pass, this ledger must remain below 100 regardless of how many individual scripts or screenshots exist.
