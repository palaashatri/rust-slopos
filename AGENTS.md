# AGENTS.md — SLOPOS-I Development Source of Truth

**Authority:** This is the sole normative development document for SLOPOS-I.
Every implementation agent, reviewer, maintainer and automation must follow it.

The repository may contain only three Markdown files:

- `README.md` — public introduction and quick start;
- `AGENTS.md` — product requirements, architecture, execution plan and acceptance gates;
- `TRUTH.md` — factual audit, evidence ledger, scores, known defects and current next gate.

Do not add competing roadmaps, plans, audit reports, hand-off notes or session
summaries as Markdown. Raw QA evidence belongs under `artifacts/qa/` in JSON,
logs, screenshots, traces, recordings or other machine-readable formats.

---

## 1. Mission

SLOPOS-I is being built as a **100/100 production Linux desktop environment**
that can genuinely compete with KDE Plasma and GNOME as a daily-driver desktop.
This is not a themed shell, a mock desktop, a collection of demos or a wrapper
around another compositor.

The final product must be credible for ordinary users, developers, creators,
accessibility users, gamers, workstation users and organisations that expect a
complete modern desktop environment.

SLOPOS-I combines:

- the clarity, directness and compact visual discipline of classic Macintosh
  System 7 and Platinum;
- a sovereign Rust compositor, shell, toolkit, SDK and first-party application
  stack;
- modern Wayland, multi-monitor, scaling, security, accessibility, packaging,
  recovery and application-compatibility expectations;
- local-first services, including SLOPOS Vision;
- user-controlled behaviour rather than hidden product-manager decisions.

The public ambition is explicit:

> **SLOPOS-I will become a production-grade Linux desktop environment whose
> reliability, compatibility, accessibility, polish and daily-driver breadth
> make it a credible alternative to KDE Plasma and GNOME.**

That statement is a product target, not a current-state claim. `TRUTH.md` must
always state the current evidence-backed maturity without promotional inflation.

---

## 2. Definition of 100/100

A component reaches 100/100 only when all of the following are true:

1. the production implementation exists;
2. automated tests cover normal and failure paths;
3. runtime evidence demonstrates the actual behaviour;
4. applicable physical hardware has been tested;
5. compatibility has been tested against representative third-party software;
6. accessibility and keyboard-only operation are complete;
7. performance and resource use meet the frozen budget;
8. installation, upgrade, recovery and uninstall paths are proven;
9. known defects do not contradict the completion claim;
10. evidence is tied to an exact commit SHA and reproducible command.

Source presence, state structures, unit tests, generated screenshots, mocked
clients or a successful build are never enough by themselves.

### 2.1 Product score interpretation

| Score | Meaning |
|---:|---|
| 0–19 | Requirement, experiment or placeholder |
| 20–39 | Early implementation with major disconnected paths |
| 40–59 | Credible prototype |
| 60–74 | Functional alpha |
| 75–84 | Strong beta |
| 85–91 | Release candidate |
| 92–99 | Production-ready with bounded known gaps |
| 100 | Frozen acceptance contract completely satisfied |

The overall score is not a percentage of lines written. It is a product-maturity
judgment under the evidence rules above.

### 2.2 Competitive standard

Competing with KDE and GNOME does not mean copying every design choice or
matching their project age. It means SLOPOS-I must independently provide a
complete and dependable answer for the same daily-driver responsibilities:

- session startup, login, lock, logout, suspend, restart and shutdown;
- stable compositing and window management;
- first-class Wayland and practical X11 compatibility;
- multiple monitors, scaling, rotation, refresh rates and hotplug;
- keyboard, pointer, touchpad, touch, tablet and accessibility input;
- clipboard, drag-and-drop, text input and IME;
- application launching, file associations, portals and notifications;
- network, audio, Bluetooth, power, display, input and account settings;
- file management, terminal, text editing, image/document preview and software
  management;
- accessibility, localisation, theming and font management;
- crash resilience, updates, rollback, diagnostics and recovery;
- performance suitable for laptops, desktops, workstations and virtual machines.

SLOPOS-I may differentiate through its classic visual lineage, compactness,
local-first design, SLOPOS Spaces, SLOPOS Share and SLOPOS Vision. Differentiation
never excuses missing baseline desktop functionality.

---

## 3. Product identity and naming

| Kind | Canonical form |
|---|---|
| Product | **SLOPOS-I** |
| Historical name | RetroShell, historical references only |
| Crates and binaries | `slopos-*` |
| Environment prefix | `SLOPOS_*` |
| User config | `$XDG_CONFIG_HOME/slopos-i` |
| User data | `$XDG_DATA_HOME/slopos-i` |
| User cache | `$XDG_CACHE_HOME/slopos-i` |
| Session entry | `slopos-i.desktop` |
| System menu | **SLOPOS** |
| Nearby transfer feature | **SLOPOS Share** |
| Virtual-desktop product | **SLOPOS Spaces** |
| Local AI/vision platform | **SLOPOS Vision** |

Do not use Apple product or service names as SLOPOS feature names. In particular,
never label SLOPOS Share as “AirDrop.” Existing visible references must be
removed. Product names such as Finder and App Store also require a deliberate
public naming and trademark review before release.

---

## 4. Documentation and truth discipline

`AGENTS.md` describes the required final product and how to build it.
`TRUTH.md` describes what is actually true now.

Every meaningful implementation wave must update `TRUTH.md` in the same branch.
Do not merely append optimistic progress. Replace stale statements, update the
exact audited commit, identify evidence level and retain unresolved defects.

Allowed evidence labels:

- **PLANNED**
- **SOURCE PRESENT**
- **BUILD VERIFIED**
- **TEST VERIFIED**
- **RUNTIME OBSERVED**
- **HARDWARE VERIFIED**

Never use “complete,” “production,” “supported,” “verified,” “working” or
“100/100” without evidence that satisfies the frozen acceptance contract.

---

## 5. Non-negotiable architecture

### 5.1 Production topology

```text
Display manager / TTY session
└── slopos-session
    ├── slopos-compositor
    │   ├── DRM/KMS output backend
    │   ├── renderer and presentation scheduler
    │   ├── input seats and cursor manager
    │   ├── window manager and SLOPOS Spaces
    │   ├── layer-shell policy and work areas
    │   ├── XWayland bridge
    │   └── private Wayland display socket
    ├── slopos-shell
    ├── first-party SLOPOS applications
    ├── third-party Wayland and XWayland applications
    └── session-scoped services such as slopos-visiond
```

### 5.2 Nested development topology

```text
Host desktop compositor
└── one slopos-compositor nested output window
    ├── slopos-shell
    ├── Finder
    ├── Settings
    ├── TextEdit
    ├── Terminal
    ├── Preview
    ├── software manager
    └── test clients
```

Only `slopos-compositor` may connect to the host display in nested mode. Every
inner shell and application surface must connect to the compositor-owned private
socket. The host compositor must see one SLOPOS output window, not each inner
application.

### 5.3 Sovereignty rules

- No production fallback to labwc, Sway, KWin, Mutter or another compositor.
- Host compositors are allowed only as nested development backends.
- `drm`, `nested` and `headless` modes must be explicit and fail clearly.
- Never scan or delete arbitrary `wayland-*` sockets.
- Use a unique per-session runtime directory, readiness token and exact socket.
- `slopos-session` is the sole session-process supervisor.
- The compositor is the sole authority for mapped-window geometry, focus,
  stacking, output, Space membership, minimize, Zoom, Fill, tiling, fullscreen
  and restore state.
- The shell paints desktop chrome and shell-only overlays. It must not maintain
  a second fake model of ordinary application windows.
- Applications request semantic operations through typed APIs. They do not
  mutate compositor geometry or host windows directly.
- No fake application windows or static screenshots may stand in for real
  clients in production.

### 5.4 Rust and licensing

- First-party implementation is Rust, with assembly only where justified by
  platform, boot, context-switch or performance requirements.
- First-party source and original assets are MIT-licensed.
- Third-party dependencies, fonts, model weights, codecs and system components
  retain their licences and notices.
- Keep `Cargo.lock` committed.
- Add `license = "MIT"` consistently to first-party Cargo packages.
- Do not copy incompatible code into MIT-licensed components.

---

## 6. POSIX and operating-system portability

SLOPOS-I is Linux-first. The first production milestone is the Linux desktop.
The shared userland and policy layers must nevertheless avoid accidental Linux
lock-in so the same desktop can later run on FreeBSD without a fork.

POSIX does not define Wayland, DRM/KMS, window management or desktop UX. The
meaningful goal is a POSIX-portable shared userland with explicit
operating-system backends.

### 6.1 Required boundary

Create or evolve interfaces equivalent to:

```text
crates/slopos-platform
crates/slopos-platform-linux
crates/slopos-platform-freebsd
```

Shared crates depend on interfaces, not Linux or FreeBSD implementations.
Platform responsibilities include:

- session and seat acquisition;
- graphics and input device discovery;
- display and colour control;
- audio;
- networking and Bluetooth;
- power and battery state;
- authentication and account integration;
- notifications and system status;
- filesystem and removable-media integration.

Portable crates must not directly depend on `/proc`, `/sys`, udev, systemd,
logind, epoll, inotify, signalfd, memfd-specific behaviour, Linux credential
structures, Linux DRM ioctls, NetworkManager, PipeWire or Linux-only command
output. These belong inside the Linux backend.

General Unix functionality may use `std::os::unix` and reviewed libc APIs.
Linux-only code must be isolated under Linux-owned modules and
`cfg(target_os = "linux")`. A broad `cfg(unix)` is not proof of portability.

### 6.2 Portable scripts

Every script required to build, install, start, stop, recover, upgrade, package
or test a supported release must use POSIX shell unless explicitly platform
owned:

```sh
#!/bin/sh
set -eu
```

Portable release paths must not require Bash arrays, `[[ ... ]]`,
`${BASH_SOURCE[0]}`, process substitution, `pipefail`, GNU-only `stat`, GNU-only
`sed`, `grep -P`, `readlink -f`, `timeout` or `seq`.

Linux-only QA scripts may use Bash when labelled clearly and when an equivalent
portable or FreeBSD release path exists.

### 6.3 Portability gates

- Linux glibc workspace build and test;
- Linux musl portability build where dependencies permit;
- native FreeBSD workspace build and test;
- portable scripts under `dash`, BusyBox `ash` and FreeBSD `/bin/sh`;
- automated dependency-boundary checks;
- shared behavioural tests for filesystem, process, IPC, settings and session
  abstractions;
- identical first-party application tests across supported systems.

Do not claim FreeBSD support from `cargo check` alone. Full support requires a
native compositor/session, input, graphics, audio, power, network, packaging and
application runtime evidence.

---

## 7. Visual and interaction design

### 7.1 Design intent

SLOPOS-I should feel recognisably descended from classic Macintosh without
becoming a pixel-for-pixel museum recreation.

Required qualities:

- compact, legible and information-dense;
- clear hierarchy and visible affordances;
- direct manipulation;
- restrained animation;
- high contrast without visual noise;
- consistent control metrics;
- keyboard discoverability;
- user-selectable classic and modern typography;
- modern accessibility and scaling;
- no visual drift into generic GNOME, elementary OS or modern macOS imitation.

### 7.2 Window controls

The user controls the semantic action of the zoom/green control and title-bar
double click.

Supported actions:

- Smart Zoom;
- Fill usable work area;
- fullscreen;
- layout menu;
- minimize;
- no action.

The compositor owns these states:

```rust
pub enum WindowPresentationState {
    Normal,
    Minimized,
    SmartZoomed,
    Filled,
    Fullscreen,
    Tiled(TilePlacement),
}
```

Every transition preserves normal geometry, output, Space and stacking intent.
Fullscreen is distinct from Fill. Minimize must not destroy restore geometry.
Output removal must safely clamp or migrate restored windows.

### 7.3 SLOPOS Spaces

SLOPOS Spaces is a user-controlled desktop organisation product, not merely a
fixed workspace counter.

Required:

- dynamic creation and removal;
- stable IDs;
- naming and reordering;
- persistent active Space and metadata;
- per-Space wallpaper and appearance metadata;
- move windows between Spaces and outputs;
- application assignment to one Space, all Spaces or policy-selected Spaces;
- optional dedicated fullscreen Spaces;
- unified-across-displays and independent-per-display policies;
- real overview thumbnails;
- drag windows between Spaces and displays;
- keyboard, touchpad and accessibility operation;
- reduced-motion mode;
- robust recovery after output changes or invalid persisted state.

The compositor is the sole Space authority. The shell is a controller and view,
not an independent fixed workspace model.

### 7.4 Fonts and text profiles

Users may choose bundled permissively licensed fonts and install their own
TTF, OTF, TTC and variable fonts.

Required profiles:

- Classic;
- Modern;
- Accessible;
- Custom.

Required roles:

- system UI;
- menu;
- window title;
- body;
- small text;
- monospace;
- document default.

Never ship Apple’s San Francisco fonts without permission. User-installed fonts
may be discovered and selected when legally present.

---

## 8. 100/100 production scorecard

SLOPOS-I is complete only when every domain below reaches its acceptance gate.
The weighting is used for the public maturity score.

| Domain | Weight |
|---|---:|
| Compositor, input and display stack | 18 |
| Renderer, text, images and fonts | 12 |
| Shell, window management and Spaces | 10 |
| System services and Settings | 9 |
| First-party applications | 10 |
| Third-party application compatibility | 10 |
| Accessibility and localisation | 8 |
| Security, permissions and trust | 7 |
| Performance, reliability and recovery | 8 |
| Packaging, updates and release engineering | 5 |
| POSIX portability and FreeBSD readiness | 3 |
| **Total** | **100** |

A weighted overall 100 is prohibited while any release-blocking item remains
unverified.

---

## 9. Milestone programme

### Milestone 0 — Truth, reproducibility and build health

Required:

- clean checkout builds with a documented stable Rust toolchain;
- `cargo fmt --check` passes;
- `cargo check --workspace --all-targets --locked` passes;
- `cargo test --workspace --locked` passes;
- Clippy runs with an explicit warning policy;
- release workspace build passes;
- dependency licence and vulnerability policy is enforced;
- every QA artefact records exact commit, environment and command;
- no generated evidence reports untested fields as passed;
- `TRUTH.md` matches the exact branch head.

Exit gate: another machine can reproduce the build and identify precisely which
runtime and hardware claims are proved.

### Milestone 1 — Linux compositor 100/100

This is the first subsystem required to reach a genuine 100/100.

#### Session and lifecycle

- private runtime directory and socket;
- compositor starts before clients;
- verified readiness token and PID;
- display-manager and TTY launch;
- SIGTERM, SIGINT, SIGHUP and abnormal-exit cleanup;
- no orphaned clients or stale sockets;
- suspend/resume and lid-close recovery;
- compositor crash diagnostics and safe session teardown.

#### Wayland lifecycle

- XDG toplevels, popups and positioners;
- subsurfaces, synchronized and desynchronized commits;
- initial-configure correctness;
- min/max constraints;
- parent/transient/modal relationships;
- activation and stacking;
- client move/resize with serial validation;
- minimize, restore, Smart Zoom, Fill, tiling and fullscreen;
- popup grabs, dismissal and reposition;
- safe destruction and disconnect during every operation.

#### Input

- keyboards and keymap changes;
- pointer, high-resolution wheel and cursor shape;
- touchpads and gestures;
- touch;
- tablets where supported;
- relative pointer and pointer constraints;
- multiple devices and hotplug;
- focus safety during destruction and Space/output changes;
- no input to hidden, minimized or inactive surfaces.

#### Data transfer and text input

- clipboard and primary selection;
- MIME negotiation and cancellation;
- cross-client drag-and-drop and drag icons;
- file URI and text transfer;
- large asynchronous transfers without blocking the compositor;
- XWayland bridging;
- Wayland text-input and input-method protocols;
- IME operation in first- and third-party applications.

#### Rendering and scheduling

- damage-driven redraw;
- correct frame callbacks and presentation feedback;
- occlusion and minimized-window throttling;
- cursor and subsurface damage;
- alpha composition and transforms;
- direct scanout where safe;
- fixed and variable refresh pacing;
- GPU surface loss and reset recovery;
- no tearing in normal desktop mode;
- stable 60, 120 and high-refresh operation.

#### Displays

- discovery and hotplug;
- unplug with windows present;
- arrangement, primary output, mirror, rotation and reflection;
- mixed resolutions, refresh rates and scales;
- integer and fractional scaling;
- popup and cursor scaling;
- per-output work areas;
- window migration after topology change;
- laptop panel close/open;
- safe fallback when an output disappears.

#### Compatibility

The release matrix must include representative:

- GTK 3 and GTK 4;
- Qt 5 and Qt 6;
- SDL, GLFW and Winit;
- Electron;
- Firefox and Chromium;
- MPV;
- LibreOffice;
- Steam and fullscreen games;
- file choosers, popup-heavy apps and multi-window apps.

Each must launch, render real content, receive input, move/resize, open menus,
copy/paste, drag/drop where applicable, fullscreen, survive Space/output changes
and exit cleanly.

#### XWayland

- rootless startup;
- scene integration;
- geometry, focus and stacking;
- override-redirect menus;
- clipboard and DnD;
- fullscreen and multiple monitors;
- DPI and scaling policy;
- clean XWayland restart;
- representative application matrix.

#### Hardware display features

- safe SDR default;
- HDR detection and verified mode switch;
- metadata and colour-space programming;
- SDR-on-HDR mapping;
- VRR capability and fullscreen policy;
- return to fixed refresh for desktop UI;
- physical Intel, AMD and NVIDIA evidence where available;
- unsupported combinations reported honestly.

#### Stability

- malformed clients cannot crash the compositor;
- bounded protocol allocations;
- 24-hour idle soak;
- 24-hour mixed-application soak;
- repeated create/destroy and popup cycles;
- output-topology stress;
- stable memory and file descriptor plateau;
- protocol and control-message fuzzing;
- clean shutdown after every test.

Exit gate: `TRUTH.md` may state `SLOPOS compositor: 100/100` only after all
items above have exact evidence.

### Milestone 2 — Production renderer, text, images and fonts

Replace prototype rendering with a retained, batched GPU architecture.

Required:

- glyph atlas with subpixel/greyscale policy;
- shaped runs using `cosmic-text` or an equivalent authoritative engine;
- bidi, script fallback, line breaking and cluster mapping;
- grapheme-aware caret and selection geometry;
- IME preedit and candidate positioning;
- image textures, colour-correct scaling and large-image tiling;
- retained textures, buffers and pipelines;
- clip stacks, rounded masks and shadows where required;
- cache lifetime and GPU-loss recovery;
- scale-aware invalidation;
- measured draw-call, upload, memory and frame-time budgets;
- no panel-per-pixel or rectangle-per-glyph production path.

Font platform:

- recursive discovery and metadata database;
- family/style/face matching;
- TTC face enumeration;
- variable axes;
- validation and duplicate handling;
- install, activate, deactivate and remove;
- script coverage and fallback chains;
- guaranteed embedded recovery font;
- live role/profile updates;
- Font Manager in Settings.

Exit gate: every first-party surface uses the authoritative text/image/font path
and meets visual/performance budgets at 1.0, 1.25, 1.5, 2.0 and mixed scales.

### Milestone 3 — Shell and SLOPOS Spaces

Required shell products:

- desktop background and icons;
- global menu with application ownership;
- Dock with launch/running/minimized indicators;
- application launcher and search;
- notifications with actions, grouping and history;
- lock screen;
- force quit and session controls;
- overview and SLOPOS Spaces;
- keyboard navigation and accessibility;
- multi-monitor policy;
- restrained animations and reduced-motion mode.

No shell control may be a dead label. Every visible command must either work or
be hidden until it does.

Exit gate: users can complete a full session using pointer, keyboard-only and
assistive-technology paths without encountering fake or disconnected controls.

### Milestone 4 — System services and Settings

Settings must control authoritative typed services, not merely write preferences
or shell out without feedback.

Required service domains:

- displays, scaling, HDR, VRR and colour;
- keyboard, pointer, touchpad, touch and shortcuts;
- audio input/output;
- networking and VPN integration;
- Bluetooth;
- power, battery and performance modes;
- accounts and authentication;
- date, time, locale and language;
- accessibility;
- appearance, themes and fonts;
- SLOPOS Spaces;
- zoom/title-bar behaviour;
- notifications and focus modes;
- default apps and file associations;
- permissions, portals and privacy;
- software sources and updates.

Every control must read current state, apply changes, report failure, preserve
unknown configuration and roll back safely when necessary.

### Milestone 5 — First-party applications

#### File manager

- icon, list, column and gallery views;
- thumbnails and metadata;
- search and indexing;
- mounts, removable media and network locations;
- copy/move/trash/restore with progress, pause, conflict handling and undo;
- drag-and-drop within and across applications;
- file associations and Open With;
- tags, favourites and recents where included;
- SLOPOS Share integration;
- SLOPOS Vision context actions;
- keyboard and accessibility completeness.

#### Text editor

- shaped multiline editing;
- grapheme, bidi and IME correctness;
- scalable undo transactions;
- find/replace;
- encoding and line-ending handling;
- autosave, recovery and safe-write semantics;
- plain text and explicitly supported document formats;
- printing/export only when genuinely implemented.

#### Terminal

- correct grapheme and cell-width model;
- combining marks, CJK and emoji;
- broad escape-sequence compatibility;
- tabs, profiles, search, selection and clipboard;
- bracketed paste and mouse modes;
- child lifecycle, resize and crash recovery;
- performance under large output.

#### Preview

- real GPU image and document display;
- large-image tiling and colour management;
- zoom, pan, rotate and metadata;
- supported document/PDF viewing where implemented;
- annotations only when fully persisted;
- SLOPOS Vision OCR and subject extraction;
- safe save/export and clipboard workflows.

#### Software manager

- signed catalogues and package metadata;
- publisher identity and trust;
- install, update, remove and rollback;
- transaction confirmation and progress;
- offline/retry behaviour;
- safe bundle extraction;
- dependency and permission disclosure;
- no misleading “store” branding without a real distribution service.

Exit gate: each application is dependable for its advertised purpose and has no
prominent non-functional commands.

### Milestone 6 — Third-party ecosystem

Required:

- xdg-desktop-portal implementation and compatibility;
- file chooser, open URI, notifications, screencast, screenshot and settings
  portals;
- PipeWire integration where appropriate;
- desktop entries, MIME database and launch services;
- Flatpak and common distribution packaging compatibility;
- browser, office, media, communication, development and gaming application
  matrix;
- robust XWayland fallback for applications that require it;
- crash isolation and diagnostics.

Exit gate: a normal Linux user can install and use a representative application
set without modifying SLOPOS source or launching a second compositor.

### Milestone 7 — Accessibility and localisation

Required:

- live AT-SPI tree bound to real widgets;
- complete roles, states, relations, actions and bounds;
- live text, caret, selection and value interfaces;
- Orca-driven workflows;
- keyboard-only operation everywhere;
- high contrast, large text and reduced motion;
- screen magnification hooks where applicable;
- focus visibility and logical traversal;
- localisation framework and extraction;
- bidirectional UI layout where required;
- locale-aware dates, numbers, sorting and collation;
- translation QA and fallback.

Accessibility is release-blocking, not optional polish.

### Milestone 8 — Security and trust

Required:

- private session IPC and strict runtime permissions;
- authenticated control protocols;
- application permission model;
- portal-enforced sensitive operations;
- sandbox strategy for untrusted applications and plugins;
- signed application bundles and trust storage;
- safe handling of received files;
- executable quarantine or explicit trust approval;
- bounded parsers and archive extraction;
- threat model and security regression suite;
- no secrets inherited by unrelated child processes;
- vulnerability response and update process.

SLOPOS Share must use authenticated encrypted transfer, explicit acceptance,
integrity verification, partial-file handling and safe destination commits.

### Milestone 9 — Performance, reliability and recovery

Budgets must be measured and frozen for:

- idle CPU and GPU use;
- compositor frame time;
- input latency;
- memory at login and under representative workloads;
- launch latency;
- text and image rendering;
- file operations;
- battery impact;
- suspend/resume;
- crash recovery.

Required:

- no continuous redraw while idle;
- bounded caches;
- leak detection;
- stress and soak tests;
- corrupted-config recovery;
- atomic user-data writes;
- safe-mode or recovery session;
- crash reports that preserve privacy;
- service restart without session destruction where safe.

### Milestone 10 — Packaging and production release

Required Linux release paths:

- supported distribution packages;
- display-manager session entry;
- clean install;
- upgrade from previous release;
- interrupted-upgrade recovery;
- rollback where supported;
- uninstall without deleting user data unexpectedly;
- configuration migration;
- reproducible release builds where practical;
- signed release artefacts;
- release notes generated from evidence, not aspiration;
- hardware and application support matrix;
- known-issue disclosure;
- end-user diagnostics.

Exit gate: a non-developer can install, update, use, recover and remove SLOPOS-I
without repository knowledge.

### Milestone 11 — FreeBSD portability

Begin only after the Linux desktop architecture is stable enough that platform
interfaces can be implemented without duplicating product logic.

Required:

- native session and seat backend;
- graphics and input device integration;
- audio, power, networking and Bluetooth adapters;
- native packaging and login session;
- same shell, applications, toolkit and settings semantics;
- shared non-regression suite;
- documented platform differences only where unavoidable.

Linux remains fully supported. FreeBSD support must not weaken the Linux product.

---

## 10. SLOPOS Vision

SLOPOS Vision is local-only by default.

Architecture:

```text
slopos-vision
slopos-vision-protocol
slopos-vision-client
slopos-visiond
Preview and file-manager adapters
```

Required product features:

- Extract Text;
- Lift Subject;
- Image Insights where a redistributable model and measured accuracy exist;
- asynchronous jobs, progress and cancellation;
- bounded memory, dimensions, queues and artefacts;
- lazy model load/unload;
- model manifest, hashes and licences;
- clean model-pack install/update/remove;
- no silent network upload;
- labelled evaluation datasets and accuracy reports;
- CPU and supported acceleration benchmarks;
- safe clipboard/save workflows;
- consent and privacy controls.

Do not claim accuracy, acceleration or redistributability without evidence.
SLOPOS Vision is separate from Loom; repository or application coupling is not
allowed merely because both may use related portable algorithms.

---

## 11. Fleet and agent execution model

Cloud agents may deploy specialist subagents, but all work stays on the current
implementation branch unless the user explicitly changes that rule.

### 11.1 Orchestrator responsibilities

The primary agent must:

- inspect current Git status, branch, recent log and `TRUTH.md` before editing;
- preserve user work;
- create a machine-readable task graph under a non-Markdown coordination path;
- divide work by subsystem boundaries;
- prevent two agents from editing the same central file concurrently;
- integrate in dependency order;
- run checks after every integration wave;
- update `TRUTH.md` with exact evidence;
- never declare completion from subagent summaries alone.

### 11.2 Recommended specialist lanes

- compositor protocols and window state;
- DRM/KMS, outputs, HDR/VRR and colour;
- input, clipboard, DnD and IME;
- XWayland and application compatibility;
- renderer, text, images and fonts;
- shell, Dock, notifications and Spaces;
- Settings and system services;
- applications;
- accessibility and localisation;
- security, packaging and release QA;
- SLOPOS Vision;
- POSIX/FreeBSD platform boundary.

### 11.3 Integration rules

- Prefer small compilable commits.
- Do not create a forest of long-lived branches.
- Do not mass-rewrite unrelated code.
- Do not disable tests to make CI green.
- Do not replace production behaviour with mocks.
- Do not add feature flags that silently remove required functionality.
- Do not merge generated code without review.
- Do not create new Markdown hand-off files.
- Resolve architectural conflicts in favour of this document.

---

## 12. Testing and evidence requirements

Every feature needs the appropriate combination of:

- pure unit tests;
- integration tests;
- protocol clients;
- virtual-output tests;
- nested runtime tests;
- DRM/KMS VM tests;
- physical hardware tests;
- application compatibility tests;
- accessibility tests;
- performance benchmarks;
- failure injection;
- fuzzing;
- long-running soak tests;
- screenshot or video evidence for visual behaviour.

Evidence artefacts must include:

- schema version;
- exact commit SHA;
- branch;
- timestamp;
- operating system and kernel;
- hardware or VM description;
- command;
- expected result;
- actual result;
- status;
- logs and referenced media;
- explicit fields that remain untested.

Never convert “not observed” into “passed.”

---

## 13. Current implementation priority

Until the first complete subsystem reaches 100/100, implementation priority is:

1. Linux compositor correctness and release evidence;
2. compositor input, DnD/IME, outputs, XWayland and third-party compatibility;
3. retained renderer, glyph atlas and real image textures;
4. SLOPOS Spaces and font/zoom Settings integration;
5. system-service authority and portals;
6. first-party application completion;
7. accessibility and localisation;
8. security, packaging, performance and long soaks;
9. FreeBSD platform implementation.

Do not divert core engineering capacity into decorative features while an
earlier release-blocking invariant remains broken.

---

## 14. Final release definition

SLOPOS-I may publicly claim to be a production desktop environment competitive
with KDE Plasma and GNOME only when:

- the weighted scorecard reaches at least 92 with no release-blocking zeros;
- compositor, session, security, accessibility, installation and recovery gates
  are fully satisfied;
- the representative third-party application matrix passes;
- ordinary users can install and operate it without development tools;
- `TRUTH.md` contains current exact evidence and no contradictory defects.

The aspirational end state is 100/100. The repository must always prefer an
honest 63/100 with precise next work over a fictional 100/100 produced by labels,
source scaffolding or generated documentation.
