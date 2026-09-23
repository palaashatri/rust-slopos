# AGENTS.md — SLOPOS-I Engineering Contract

**Status:** normative  
**Product generation:** SLOPOS-I  
**Primary branch:** `main`  
**Execution target:** Linux/X11 only  
**Canonical visual reference:** Classic Macintosh UI Kit (Community), Figma file `LGMlwNCoVdakZxDBvPKg1W`, root node `0:1`  
**Readiness ledger:** `TRUTH.md`

This file is the authoritative engineering contract for SLOPOS-I. `TRUTH.md` records what is actually proven on the current tree. `README.md` is user-facing documentation. When old screenshots, old QA reports, comments, branches, agent prose, generated assets, or historical claims disagree with this file, this file wins.

SLOPOS-I is not considered complete because an agent says it is complete. Completion is established only by current evidence tied to the exact source revision.

---

## 1. Product mission

SLOPOS-I is a coherent, mature, consumer-usable Linux desktop environment with a compact late-1990s Macintosh-inspired visual and interaction language implemented clean-room on modern Linux/X11 infrastructure.

The project owns the desktop experience while reusing mature system infrastructure where that improves reliability.

The product must feel like one system from login to shutdown:

- session startup;
- window management;
- global menu bar;
- desktop objects;
- application launching;
- file browsing;
- Settings / Control Panels;
- notifications;
- network, sound, display, Bluetooth, power, keyboard and pointer integration;
- screenshots and session actions;
- normal Linux application compatibility;
- packaging, installation, update and recovery.

The product must not contain enabled controls that merely look functional.

---

## 2. Scope and non-goals

### 2.1 In scope

SLOPOS-I is:

- Linux-only;
- X11-only for this generation;
- Openbox-based unless a documented technical blocker proves a replacement is necessary;
- implemented primarily in Rust;
- permitted to use GTK3/GDK/Pango/GIO and other mature Linux/X11 libraries;
- permitted to delegate low-level service ownership to NetworkManager, PipeWire/WirePlumber or PulseAudio-compatible APIs, BlueZ, UPower, systemd/logind, udev, XRandR and related established infrastructure.

### 2.2 Out of scope

Do not introduce:

- a Wayland session;
- Wayland fallback logic;
- a custom display server merely for ownership;
- a custom general-purpose GUI toolkit merely for ownership;
- a custom kernel;
- a speculative future SLOPOS generation;
- a persistent bottom dock or retired Application Strip;
- Aqua, modern macOS, GNOME/libadwaita, KDE Breeze, Windows Fluent or other unrelated visual languages;
- fake compatibility or fake system state.

Wayland work is explicitly paused until the X11 product is mature.

---

## 3. Source-of-truth precedence

When requirements disagree, use this order:

1. this product contract;
2. the Figma-derived SLOPOS component specification created from the canonical Figma file;
3. approved clean-room SLOPOS assets and interaction specifications;
4. implementation;
5. current VM-generated screenshots and test evidence;
6. historical screenshots, historical QA, old branches and old prose.

The existing files under `qa/reference/` were created before the current atomic-conformance reset. They may be used as historical context only. They are not allowed to override Figma-derived geometry, component states or the contract in this file.

No previous numeric score is authoritative.

---

## 4. Canonical visual design

The canonical appearance is **SLOPOS Platinum Classic**.

The canonical Figma reference is:

- URL: `https://www.figma.com/design/LGMlwNCoVdakZxDBvPKg1W/Classic-Macintosh-UI-Kit--Community-?node-id=0-1&p=f`
- file key: `LGMlwNCoVdakZxDBvPKg1W`
- root node: `0:1`

Treat the Figma file as a component and geometry specification, not vague inspiration.

The implementation must extract and record exact relevant component metadata before claiming conformance. Examples already observed during the 2026-09-24 audit include:

- menu bar example height: 19 px;
- menu item example height: 16 px;
- large Finder item example: 71×44 px;
- large Finder icon example: 32×32 px;
- large Finder label region example: 71×12 px;
- regular button example: 80×20 px;
- default button example: 88×28 px;
- secondary button example: 80×16 px;
- button states include Rest, Pressed and Disabled;
- menu items distinguish Action and Hierarchical variants with Active, Hover and Disabled states;
- window-title-bar examples distinguish Active/Inactive and 8-bit/1-bit variants.

These examples are audit observations, not a substitute for full extraction.

### 4.1 Clean-room rule

The visual language may reproduce geometry, hierarchy, density, state treatment and interaction affordances.

Do not ship proprietary Apple material, including:

- Apple logos;
- copied Macintosh icons or artwork;
- proprietary Apple fonts;
- proprietary sounds;
- copied wallpapers;
- copied documentation text.

Any font or third-party asset must have a verified redistribution license before it is bundled. If licensing is unclear, use or create a redistributable replacement and document the choice.

### 4.2 Canonical characteristics

At 1× scale the desktop should use:

- the exact Figma-derived global-menu geometry;
- compact hard-edged platinum window surfaces;
- thin dark keylines;
- restrained raised/sunken control depth;
- compact title bars;
- dense desktop typography;
- narrow visible scrollbars;
- dense icon-oriented file browsing;
- a dockless desktop;
- right-aligned desktop objects where the canonical composition requires them;
- a blue desktop field;
- compact utility-window proportions;
- strong active/inactive distinction.

Avoid:

- pervasive rounded corners;
- large soft shadows as the main depth cue;
- floating modern cards;
- translucent glass;
- giant touch-first spacing;
- pill-heavy controls;
- contemporary traffic-light controls;
- generic toolkit defaults leaking into first-party UI.

---

## 5. Atomic implementation method

SLOPOS development proceeds strictly bottom-up:

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

A higher layer must not compensate for a known broken lower layer.

Every required atom needs:

- canonical dimensions;
- visual states;
- behavior states;
- accessibility semantics;
- test coverage;
- conformance evidence.

A component is not complete merely because its default screenshot looks correct.

### 5.1 Foundation

The shared design layer must centralize:

- geometry and scaling;
- pixel snapping;
- typography tokens and baselines;
- colors;
- borders and bevels;
- patterns;
- icon sizing and loading;
- focus treatment;
- selection treatment;
- state transitions;
- deterministic rendering hooks;
- common accessibility helpers.

A reusable SLOPOS component crate such as `slopos-ui` is acceptable and encouraged.

This is a SLOPOS design/component layer on top of GTK3/GDK/Pango/GIO, not a replacement for those toolkits.

### 5.2 Required control families

Where used by SLOPOS, the conformance inventory must include all relevant states for:

- labels;
- separators;
- icons/images;
- buttons;
- default buttons;
- secondary buttons;
- checkboxes;
- radio buttons;
- text fields;
- text inputs;
- popup selectors;
- arrow controls;
- progress indicators;
- disclosure controls;
- list rows;
- selected rows;
- scrollbars, arrows and thumbs;
- menu items;
- hierarchical menu items;
- menu separators;
- keyboard-command indicators;
- Finder/file items;
- dialogs and alerts.

Each Figma component must be classified as `REQUIRED`, `OPTIONAL VARIANT` or `NOT USED`. A `NOT USED` classification requires a concrete reason.

No required component may remain `UNKNOWN` in a completed build.

---

## 6. Interaction contract

Visual fidelity and functional fidelity are one acceptance gate.

Every interactive atom must have an explicit state machine.

At minimum test:

### Buttons

- pointer down inside;
- pointer leaves while held;
- pointer re-enters;
- release inside;
- release outside;
- keyboard activation;
- focus;
- disabled behavior.

### Menus

- pointer opening;
- keyboard opening;
- traversal;
- submenu ownership;
- disabled items;
- accelerators;
- Escape;
- click outside;
- active-window changes while open.

### Windows

- activate/deactivate;
- move;
- resize;
- minimize;
- maximize;
- restore;
- fullscreen;
- modal/transient ownership;
- Alt+Tab;
- workspace movement and switching.

### File items

- click;
- double-click;
- keyboard navigation;
- single and multiple selection;
- rename;
- drag/drop;
- context menu;
- focus persistence.

Where the Figma reference does not define behavior, use documented classic desktop behavior when practical, extended deliberately for modern Linux requirements and accessibility.

---

## 7. X11 and window-management architecture

Openbox provides the base ICCCM/EWMH window-manager infrastructure.

Maintain one long-lived X11 integration layer using `x11rb` for:

- active-window tracking;
- root-window property changes;
- fullscreen/maximize state;
- window metadata;
- monitor/RandR topology;
- work-area state;
- shell placement state.

Do not use `xdotool`, `xprop`, `xrandr` or `wmctrl` as a high-frequency event bus.

Command-line tools may be bounded fallbacks for explicit user actions when a stable API is not practical.

Window behavior must cover:

- predictable focus;
- overlapping windows;
- drag/resize;
- minimize/maximize/restore;
- fullscreen;
- transient/modal relationships;
- Alt+Tab;
- multiple workspaces;
- correct top-bar work area;
- dynamic monitor changes;
- integer HiDPI.

### 7.1 Window chrome

Use Openbox themes/assets to reproduce the Figma-derived chrome as precisely as Openbox permits.

If Openbox blocks a required behavior or geometry:

1. create a minimized reproducible case;
2. document the exact limitation;
3. evaluate the least invasive solution;
4. change architecture only after evidence.

Never weaken the specification merely to fit Openbox.

---

## 8. Global menu architecture

There must be one authoritative application-menu bridge.

The current duplicate `gmenu` implementations must be consolidated during the reset.

Application menus may be displayed globally only when backed by a real supported menu/action model such as GTK/GIO exports or a compatible D-Bus menu protocol.

If an application does not export an actionable menu:

- retain its local menu; or
- expose only truthful shell-owned window actions.

Forbidden:

- guessed app commands;
- fabricated app menu structures;
- enabled empty callbacks;
- using `xdotool key ctrl+x`, `ctrl+c`, `ctrl+v`, `ctrl+a` as a generic application-menu implementation.

`xdotool` may be used by QA to drive a test session.

---

## 9. System services and truthful state

An enabled control must perform the behavior it advertises.

A stateful control must display real state, not a sample value.

### 9.1 Network

Use NetworkManager through a stable API/D-Bus integration for:

- adapters;
- connection state;
- Wi-Fi enabled state;
- access points;
- signal strength;
- active connection;
- connect/disconnect actions;
- failures.

No production UI may contain synthetic SSIDs, sample IP addresses or assumed interface names.

### 9.2 Sound

Use the active PipeWire/WirePlumber/PulseAudio-compatible stack to read and set:

- output devices;
- default output;
- output volume;
- output mute;
- input devices;
- input volume;
- input mute.

Read back changes.

### 9.3 Date and time

Prefer `org.freedesktop.timedate1`/D-Bus where practical.

Display real:

- timezone;
- NTP state;
- current date/time.

Handle authorization failure explicitly.

### 9.4 Bluetooth

Use BlueZ D-Bus for first-party live Bluetooth state and actions.

### 9.5 Power/session

Use UPower and systemd/logind as appropriate.

### 9.6 Displays

Use XRandR state for:

- connected outputs;
- current modes;
- available modes;
- geometry;
- primary display;
- applied configuration.

### 9.7 Keyboard and pointer

Use real X11/XKB/XInput-compatible mechanisms.

When a provider is absent, expose a disabled/unavailable state with a truthful reason.

---

## 10. File browsing

A release-visible file browser must be both visually coherent and functionally real.

PCManFM may remain only if a private SLOPOS profile can satisfy the required visual and interaction contract without fragile hacks.

If it cannot, document the blocker and implement a scoped first-party browser using mature filesystem/GIO facilities.

Required core workflows include:

- directory navigation;
- open;
- MIME/default-app launch;
- keyboard navigation;
- selection and multi-selection;
- rename;
- new folder;
- copy/move/paste;
- Trash;
- context menus;
- drag/drop;
- filesystem change handling;
- error handling;
- removable volumes if exposed.

Do not ship a screenshot-only Finder imitation.

---

## 11. Settings / Control Panels

Settings is a compact Control Panels-style shell.

It is not a modern card dashboard and not a generic launcher for unrelated tools.

Every panel must declare:

- built-in or delegated;
- provider;
- availability detection;
- real action path;
- unavailable state;
- tests.

Core categories should include, where supported:

- Displays;
- Sound;
- Network/Wi-Fi;
- Bluetooth;
- Power;
- Appearance;
- Desktop;
- Keyboard;
- Mouse/Pointer;
- Date & Time.

Delegation is acceptable only when the resulting user experience remains coherent enough for the release contract.

---

## 12. Required first-party surfaces

The canonical acceptance set requires functional first-party surfaces for:

- About SLOPOS-I / system information;
- at least one compact accessory utility, preferably Calculator;
- Settings / Control Panels;
- launcher;
- notifications;
- Software Catalogue.

About must display real runtime/build information for any value it exposes.

If Calculator is used, arithmetic and keyboard behavior must be tested. It must not exist only to satisfy a screenshot.

---

## 13. Software Catalogue

The catalogue handles curated AppImages only unless the product contract is explicitly changed.

An installable entry requires trusted metadata, including:

- HTTPS source;
- version;
- architecture;
- digest or verified signature metadata;
- safe desktop-integration metadata.

Install must:

- fail closed;
- stage to a temporary location;
- verify integrity before final placement;
- report failures visibly;
- support launch and uninstall correctly.

Distribution packages remain the base distribution's responsibility.

---

## 14. Accessibility

Every meaningful control must provide:

- accessible name;
- appropriate role/state;
- keyboard reachability;
- logical focus order;
- visible focus indication;
- disabled-state semantics.

AT-SPI acceptance remains mandatory.

Visual fidelity must not be achieved by degrading accessibility.

---

## 15. Multi-monitor, fullscreen and HiDPI

Do not treat the full X11 framebuffer as one monitor.

Maintain an explicit monitor model with geometry, identity and scale assumptions.

Define which monitor owns:

- global menu bar;
- launcher;
- notifications;
- desktop objects;
- newly opened first-party dialogs.

Respond to RandR topology changes without session restart.

Fullscreen applications must remain unobstructed.

Canonical geometry is defined at 1×. Integer scaling must remain predictable and avoid fractional blur.

---

## 16. Mandatory VM-only execution policy

This rule applies to every human, model and automation agent working locally on SLOPOS-I.

### 16.1 Native host restrictions

The native host may be used only for:

- reading/editing source and documentation;
- Git operations;
- launching/managing a supported hypervisor;
- copying final artifacts between guest and host;
- checking host free disk space with host system tools.

Do **not** execute SLOPOS project code on the native host.

Forbidden on the host include:

- `cargo build`;
- `cargo test`;
- `cargo run`;
- `cargo clippy`;
- project binaries;
- project shell/Python/PowerShell QA scripts;
- package builds;
- ISO/image builds;
- visual QA;
- X11/GTK runtime tests;
- Docker/Podman project QA as a substitute for a VM.

Run formatting/lint/test tooling in the Linux guest as well, so all engineering validation occurs in one controlled environment.

### 16.2 Required guest

All compilation, execution and visual QA must occur inside a Linux virtual machine hosted by one of:

- UTM;
- QEMU;
- VirtualBox;
- VMware.

Ubuntu LTS is the preferred general development guest. Debian or Arch may be used where a distribution-specific acceptance lane requires them.

The working clone used for compilation must live on the guest filesystem, not in a host shared folder.

Shared folders may be used for source/artifact transfer, but build output, `target/`, package caches and test scratch data must stay on guest storage.

### 16.3 VM visual QA

Primary visual QA must run in a real graphical X11 session inside the VM.

Xvfb/container tests may run **inside the VM** as secondary deterministic tests, but they do not replace VM graphical acceptance.

Required screenshots must come from the current guest session and exact tested commit.

### 16.4 Remote CI

Hosted CI may supplement the evidence and remains required for release automation where specified.

Hosted CI does not authorize executing project code on the local native host and does not replace the mandatory Linux-VM local acceptance run.

---

## 17. Disk-space safety policy

SLOPOS work must not exhaust the host or guest filesystem.

Before compilation or QA, record:

- host free space;
- guest `df -h`;
- guest repository size;
- guest `target/` size if present;
- guest QA artifact size;
- guest Cargo cache size where material.

### 17.1 Routine development budget

Before a routine build/test run:

- host should have at least 20 GiB free;
- guest should have at least 12 GiB free.

If either threshold is not met, do not start a large build. Clean only SLOPOS-owned guest data or expand/move the VM.

### 17.2 Release/media budget

Before package, ISO, multi-architecture or VM-image generation:

- host should have at least 30 GiB free;
- guest should have at least 25 GiB free.

Use a dedicated larger guest disk when needed rather than filling the main host volume.

### 17.3 Storage discipline

Use:

- sparse virtual disks where supported;
- one maintained development VM rather than creating a new full VM per iteration;
- snapshots sparingly;
- guest-local `CARGO_TARGET_DIR` on an ephemeral or dedicated build volume where practical;
- only current-commit QA evidence as retained working artifacts.

After successful evidence extraction, clean stale guest-only data such as:

- old `target/` trees when no longer useful;
- old package staging directories;
- old ISO/image outputs;
- old QA screenshots from superseded commits;
- guest package-manager caches when necessary;
- Docker/Podman caches only if the VM is dedicated to SLOPOS and the cache is known to be disposable.

Never run broad cleanup commands on the native host.

Never delete unrelated user data or unrelated VM images.

Where supported, trim free guest blocks so sparse VM images do not grow indefinitely.

---

## 18. Conformance harness

Build a machine-readable Figma-derived specification under `qa/spec/` or equivalent.

For every required atom/state, provide:

- reference dimensions;
- actual dimensions;
- visual golden/reference;
- rendered output;
- image diff where appropriate;
- behavior assertions;
- accessibility assertions.

Geometry has zero tolerance unless the specification explicitly documents a platform exception.

For anti-aliased text where exact pixels are not portable, define a fixed justified comparison threshold. Never increase tolerance merely to make a regression pass.

Normal CI must never auto-update goldens.

A golden/spec change requires an explicit reviewable source change.

---

## 19. Whole-desktop visual acceptance

Required current composed scenes include:

- empty desktop;
- menu open;
- system/file-browser window;
- Settings / Control Panels;
- About;
- accessory utility;
- launcher;
- notification;
- modal dialog;
- representative upstream GTK application;
- representative ordinary X11 application where useful.

Required validation resolutions:

- 800×600 canonical composition;
- 1280×800;
- 1920×1080;
- 3440×1440;
- supported integer HiDPI configuration.

The 800×600 scene must be produced from real running SLOPOS components, not a mock image.

---

## 20. No-cheating rules

Do not achieve a passing state by:

- weakening the contract;
- raising image-diff tolerance to hide regressions;
- deleting required tests;
- hard-coding test results into production;
- replacing a real workflow with a mock;
- screenshotting a mockup;
- treating old screenshots as current evidence;
- marking UNKNOWN as PASS;
- silently disabling required behavior;
- changing goldens to match incorrect output;
- claiming an upstream limitation without evidence;
- inventing system state;
- self-awarding a visual score.

When something cannot be proven, mark it `UNKNOWN` or `BLOCKED`.

---

## 21. CI and test gates

Routine development gates must include:

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- `cargo test --workspace`;
- script syntax checks;
- atomic conformance tests;
- X11 integration tests;
- accessibility acceptance;
- representative visual regression.

All local invocations of these gates occur inside the Linux VM.

Expensive package/media/installed-VM matrices should run for release candidates, manual dispatch, or relevant path changes rather than every trivial commit.

Mandatory failures must fail the job. Do not hide them with `|| true`.

---

## 22. Packaging and release

Source installation alone is not consumer readiness.

Release engineering must eventually prove:

- Debian/Ubuntu package;
- Arch package;
- install;
- upgrade;
- removal;
- X11 session registration;
- checksums;
- exact source provenance;
- x86_64 bootable media;
- boot to a usable SLOPOS session.

Public package repositories may be advertised only after they actually exist and are signed/published.

Architecture support is evidence-based. ARM64 or RISC-V is not supported merely because a package manifest names it.

Release CI should produce official artifacts from exact source revisions. Local VM runs validate development/release candidates but are not the canonical publication mechanism.

---

## 23. Documentation truth

`README.md` describes only user-visible behavior that exists.

`TRUTH.md` must:

- identify the exact audited commit;
- state whether evidence is static, VM-executed, CI-executed or visual;
- list known fake/placeholder behavior;
- list visual deviations;
- list current blockers;
- record the VM environment used for executed evidence;
- record relevant disk-space observations for heavy QA;
- never preserve a score from another commit.

Historical QA documents that conflict with the current contract should be deleted rather than left as competing truth sources.

Historical screenshots may remain for archaeology, but they are never acceptance evidence unless explicitly tied to the current commit and current specification.

---

## 24. Definition of complete

SLOPOS-I may be called complete only when all required gates are simultaneously true.

### Visual

- every REQUIRED atom is implemented;
- every REQUIRED state has a reference/specification;
- every REQUIRED geometry assertion passes;
- every required composed scene passes;
- there are no critical visual deviations;
- there are no required UNKNOWN visual items.

### Functional

- every enabled control has a real action;
- every stateful control reads real state;
- system changes are read back and verified;
- no production fake data remains;
- core workflows pass;
- failure paths are tested.

### Runtime

- session startup/exit passes;
- window management passes;
- global menu is truthful and protocol-backed;
- file browsing passes;
- Settings providers pass;
- multi-monitor/fullscreen/HiDPI acceptance passes;
- accessibility passes.

### Engineering

- fmt passes;
- clippy passes;
- workspace tests pass;
- conformance tests pass;
- required CI passes;
- current VM evidence exists;
- documentation matches the final commit.

### Release

A release-ready claim additionally requires all release-candidate package/media/publication gates defined for that release.

If any required item cannot be proven, the state is not complete.

The only valid terminal states for a full autonomous completion task are:

- `COMPLETE` — every required gate has current evidence;
- `BLOCKED` — all possible work is complete, but a specific external dependency prevents a remaining required gate.

There is no "close enough" terminal state.
