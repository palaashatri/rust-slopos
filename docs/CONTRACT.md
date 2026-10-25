# SLOPOS-I Engineering Contract

> **Moved from `AGENTS.md` on 2026-10-20.** This is the long-form requirements, architecture and evidence reference. Day-to-day agent instructions (commands, rules, workflow) now live in the root [`AGENTS.md`](../AGENTS.md), which takes precedence where the two differ. Section numbers (§) below are unchanged so existing references still resolve to this file. Older data files under `qa/` that cite `AGENTS.md §N` mean a section of this contract.

**Status:** normative contract plus separately labelled evidence  
**Product:** SLOPOS-I / Platinum Classic  
**Primary branch:** `main`; one monorepo  
**Execution target:** Linux/X11; Openbox; primarily Rust  
**Visual authority:** Classic Macintosh UI Kit, Figma `LGMlwNCoVdakZxDBvPKg1W`, root `0:1`  
**Contract audit:** 2026-10-02 IST (2026-10-01 UTC), source `7639ccd0b89955fc2657a3d67a489822f70c0837`  
**Implementation readiness:** NOT COMPLETE; scoped Linux VM execution evidence is recorded in Part II.K, not a production-readiness claim.

This is the single project-wide source for requirements, architecture, work sequencing, audit findings and evidence. Root/subproject `README.md` files provide scoped orientation only. Do not create competing plans, `TRUTH.md`, design documents or per-agent Markdown ledgers. Machine-readable specifications, tests and raw execution artifacts are permitted; see §34.

Current explicit user instructions take precedence over this file, subject to the execution environment's higher-priority rules. Old chats, branches, comments and screenshots do not override the latest adopted contract. Requirements describe the target; only evidence establishes implementation truth.

# 0. Agent execution contract — read first

This file is deliberately comprehensive. A harness may supply only its beginning. Every coordinator reads the whole file; every worker explicitly reads this section, §§20–21, §§28–30, its assigned component sections and Part II before changing code. Do not assume an automatically injected excerpt contains the entire contract.

## 0.1 Decisions to preserve

| Decision | Current instruction |
|---|---|
| Product generation | SLOPOS-I is an X11 desktop environment. Keep Openbox as the current WM; no Wayland/Smithay revival or custom display server in this tranche. |
| Language and appearance | Primarily Rust; Platinum Classic geometry and interaction. Earlier Cheetah/Aqua, Cutefish, tucch and C/C++ explorations do not authorize a rename or rewrite of this tree. |
| UI first | Build and verify `slopos-ui`, then `slopos-appkit`; native applications and shell surfaces consume them. GTK3/GDK/Pango/GIO may remain hidden infrastructure. |
| Required shell | Dock, global menu, status applets, launcher, desktop, notifications, workspaces and volume/brightness OSDs. The later Dock requirement supersedes the former “no Dock” rule. |
| Applications | Preserve the full inventory in §§16–18. Start with the component gallery and small proof apps, then core daily-use apps; do not start every app at once. |
| Repository | One monorepo, agreed 2026-09-29. Use temporary task branches/worktrees, never permanent compositor/app branches that each contain a different product. |
| Execution | All local project compilation, tests, formatting, lint, runtime, packaging and visual QA run inside a Linux VM. A host container is not a substitute. |
| Documentation | This file is the central contract and evidence ledger; Git history archives superseded prose. |
| Later capabilities | Keep HDR/VRR/color-management and live-wallpaper ambitions visible, but require supported-backend/hardware evidence before exposure. They do not authorize Wayland work now or fake controls in an X11 VM. Kindle/AI/vision and office-suite expansion do not displace the core desktop. |

The September 2026 X11/UI-library decisions and the merged Dock amendment in PR #14 are the operative baseline. Do not revive an older architecture because it sounds more ambitious.

## 0.2 Start and resume procedure

1. Read `git status --short`, `git diff --stat`, `git log -5 --oneline`, `git rev-parse HEAD` and `git worktree list`. Preserve unrelated changes. Record the exact base SHA, branch and dirty state.
2. Read Part II and inspect the assigned source paths. Distinguish **existing**, **to create**, **historical** and **unverified**. No `slopos-ui`, `slopos-appkit`, gallery or `xtask` exists at the audited baseline.
3. The coordinator establishes the VM and resource record (§§28–29), selects a dependency-ready task from §21 and records ownership in Part II.M. No worker self-assigns overlapping paths.
4. Agree the narrow acceptance criteria, interface shape and test command before implementation. Missing UI atoms become UI tasks, not app-local workarounds.
5. On resume, recheck the coordinator's integration SHA, task ownership and upstream interface changes. Stop only the affected task if its dependency changed; continue independent authorized work.
6. A documentation-only audit may read/edit source and perform text consistency checks without a VM. It must not run repository code or claim project test/visual passes.

## 0.3 Coordinator and worker ownership

Use one coordinator/integrator, a small number of workers and an independent review pass. Start with two or three workers; increase only after measuring RAM, disk and build contention. Agent count is not permission to launch that many Cargo builds or graphical sessions.

The coordinator alone owns:

- this contract (`docs/CONTRACT.md`), the root `AGENTS.md` instruction sheet, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and the integration branch;
- shared public interfaces, crate/module registration (`lib.rs`, `mod.rs`), gallery registries and specification manifests;
- CI/workspace-wide policy, shared theme/token changes and packaging entry points;
- task assignment, resource reservations, integration order and final evidence reconciliation.

An owner may explicitly delegate one of these paths in a task packet; record the handoff and suspend other edits to it. Leaf-module workers send proposed registration/dependency changes to that owner. Only one owner changes a file at a time, even if worktrees would let Git merge it.

Workers own only the exact listed paths and tests. They do not rewrite shared APIs, reformat the workspace, update lockfiles, weaken goldens, edit another worker's files, change the product contract or push to `main`. A blocker report includes the smallest needed upstream change. The coordinator either schedules it or reassigns a non-conflicting task. Workers do not spawn additional implementers without coordinator allocation.

The coordinator updates Part II for the integrated change set. Workers return evidence and proposed ledger text instead of independently editing this file.

## 0.4 Task packet and handoff

Every assignment includes these fields; a broad role such as “finish Settings” is not a task.

| Field | Required content |
|---|---|
| Identity | Stable task ID from §21, named owner, branch/worktree and base SHA |
| Outcome | One reviewable user behavior or infrastructure contract; explicit exclusions |
| Dependencies | Integrated prerequisite SHAs, accepted spec/state IDs and public API signatures |
| Ownership | Exact writable files/directories, read-only references and coordinator-owned registration paths |
| Acceptance | Positive behavior, failure/cancellation behavior, accessibility where relevant and observable result |
| Verification | Existing exact commands; planned tests clearly marked as to be implemented; required VM/visual/hardware evidence |
| Resources | Build permit, maximum Cargo jobs, target/artifact paths and any exclusive runtime/system-service lease |
| Handoff | Commit SHA(s), changed paths, commands/exit codes, artifact paths/hashes, limitations and next dependency |

Task states are `PLANNED → READY → CLAIMED → IMPLEMENTED → VERIFIED → INTEGRATED`; `BLOCKED` records a precise missing dependency and unblock condition. `IMPLEMENTED` is not `VERIFIED`. `INTEGRATED` also requires review and the relevant integration gates. A planning row does not establish that a component exists.

A task is small enough when it can be reviewed, reverted and tested independently. Split provider state discovery from mutations, control rendering from interaction, and application commands from their views when that produces clear ownership. Do not split into empty crates or stub callbacks merely to increase the completed-task count.

## 0.5 Branches and worktrees

Keep all clones, worktrees, targets and test data on guest-local storage. Start each task from the coordinator's recorded integration commit, not an arbitrary latest remote branch.

Example **inside the established guest** (replace paths and SHA before use):

```bash
git -C /work/rust-slopos worktree add -b agent/ui-02-buttons /work/slopos-worktrees/ui-02-buttons <integration-sha>
```

Use `agent/<task-id>-<description>` and a temporary `integration/<tranche>` branch. The coordinator commits workspace/interface bootstrap first, then publishes that SHA to workers. Dependent tasks start only after their prerequisites integrate; do not independently invent the same API.

Each worker uses its own target directory if the disk reservation allows it. Existing QA scripts often hard-code `target/debug` or `target/release`; they must use the expected worktree-local target until QA-01 makes path handling explicit. Do not set a shared `CARGO_TARGET_DIR` and assume legacy launchers honor it. Sharing Cargo's download cache is acceptable; sharing mutable build outputs across simultaneous workers is not.

Workers deliver small commits. The coordinator reviews diffs, integrates in dependency order, resolves shared-file conflicts and reruns affected integration gates at the integrated SHA. No automatic force-push, broad reset, branch deletion or cleanup of unmerged work. Remove a worktree only after its changes and evidence are preserved. Keep branch/PR work reviewable; merge upstream only when authorized.

## 0.6 VM concurrency and runtime leases

Separate **editing concurrency**, **build concurrency** and **runtime-test concurrency**.

- Start with one build/test permit; cap `CARGO_BUILD_JOBS` explicitly for the guest. Raise parallel build permits only after measuring available memory and aggregate target growth (§29).
- One coordinator-owned runtime lease covers all graphical/Xvfb sessions, global hotkeys, session D-Bus ownership, audio/network/display mutations, package installs and session restart/lock tests in a shared guest.
- At the audited baseline, run existing QA suites **serially in a dedicated test account/session**. Fixed displays, shared output paths and name-based `pkill` make separate worktrees alone insufficient isolation.
- New harnesses must use unique display allocation, per-run XDG/runtime/config/data directories, isolated session buses, explicit child PID/process-group cleanup and unique evidence directories. A private session bus does not isolate the system bus or physical devices.
- Do not run crash/restart QA against a guest desktop someone is using. Retain a recovery console and coordinate any network/display/locker tests that may disconnect the agent.
- Never “clean up” with global `pkill`, `killall`, `cargo clean`, removal of shared caches or another agent's targets. Before using a legacy script with those behaviors, QA-01 must isolate or repair it; until then use only a disposable, exclusive test session.
- The lease records owner, command, guest/session and start time outside versioned product files. Release it after owned children exit; reclaim an abandoned lease only after checking those processes. Parallel runtime testing is allowed only after isolation is demonstrated.

Locks and task messages may be kept in a coordinator's scratch directory outside the checkout. They are transient orchestration state, not another project roadmap.

## 0.7 Evidence and completion

For each gate retain: task ID, source SHA, clean/dirty state, command, exit code, UTC timestamp, guest/hypervisor/OS, toolchain, relevant dependency versions, artifact location/checksum and explicit result. Visual evidence additionally records resolution, scale, fonts, backend and spec/golden revision. Dirty-tree experiments are provisional until rerun from the recorded commit.

Use `PASS`, `FAIL`, `NOT_RUN`, `BLOCKED` and `NOT_APPLICABLE` precisely. A skip needs a reason and cannot become PASS. Fixture-backed protocol tests, real guest-service tests, graphical VM tests and physical-device tests are separate evidence categories. A VM without Wi-Fi, Bluetooth or a backlight can verify unavailability and test fixtures; it cannot prove real-device support.

Avoid the self-referencing-commit trap: test code commit A, then add a documentation-only evidence commit B that explicitly cites A. The coordinator may retain A's evidence if a diff proves B changes only documentation. Never relabel it as a run of B; runtime/build/spec/golden changes require the affected gates again. Release candidates require evidence for the actual candidate code.

Scoped tasks may finish while the overall product remains incomplete. Report partial work as `IMPLEMENTED / NOT VERIFIED`, and external dependencies as `BLOCKED`; do not run indefinitely or invent evidence to satisfy a demand for “100%.” Completion of this contract edit is not completion of SLOPOS.

## 0.8 Reading map

| Work | Read before editing |
|---|---|
| Every task | §0, §§20–21, §§28–30, §33–34 and Part II |
| UI/spec/gallery | §§2, 4–5, 7–11, 25, 27, 35 |
| Appkit and native apps | §§6, 12, 15–19, 26, 31, 36 |
| Services, X11 and shell | §§6, 13–14, 18, 22–26, 31 |
| Integration/release | §§28–32, 37 and the complete ledger |

## 0.9 Coordinator launch instruction

Use this as the initial instruction to an agent running with access to the intended VM:

> Read the full contract (`docs/CONTRACT.md`) and the root `AGENTS.md` in this checkout, and audit the current SHA before acting. Coordinate a bounded implementation tranche using §21, starting with VM-00, SPEC-01 and QA-00 where access permits. Use one coordinator and up to three workers with non-overlapping task packets/worktrees. Integrate the bootstrap interfaces before dispatching dependent code. Preserve Rust/X11/Openbox, Platinum Classic, the UI-library-first architecture and the full required inventory. Serialize builds/runtime tests until resource and isolation evidence allows more concurrency. Keep implementing ready tasks, report exact blockers for unavailable dependencies, and update Part II through the coordinator with verified commits and artifacts. Do not claim desktop completion from a task pass.

If a subagent facility is unavailable, perform the same dependency-ready tasks sequentially. The plan does not require a particular model, agent vendor or VM automation tool.

---

# Part I — Product and engineering requirements

# 1. Product mission

SLOPOS-I is a first-party Linux desktop environment designed to be viable for normal daily use and engineered to the quality level expected of mature environments such as GNOME and KDE while retaining its own deliberately compact Classic-Macintosh-inspired interaction and visual language.

Build one coherent desktop platform: shared design specification and UI components, reusable application services, real platform adapters, native applications and an integrated shell/session. The dependency graph in §6 is authoritative; providers and UI foundations can be developed independently.

The system must feel intentional from login to shutdown:

- session startup and recovery;
- desktop composition;
- windows, focus and workspaces;
- a first-party Dock;
- a global application menu bar;
- menu-bar system applets;
- non-focus-stealing on-screen displays for volume, brightness and related hardware state;
- application launching and search;
- file browsing and removable media;
- network, Wi-Fi, VPN, Bluetooth, audio, brightness, displays and power;
- settings and defaults;
- clipboard and drag-and-drop;
- notifications;
- screenshots and screen recording;
- file dialogs and application choosers;
- authentication and session dialogs;
- accessibility;
- first-party utilities;
- normal third-party X11 application compatibility;
- packaging, installation, upgrade, removal and recovery.

SLOPOS is a desktop environment, not merely a window manager plus themed applications. The Dock, menu bar, applets, OSD layer, notification service, global shortcuts, system providers and native daily-use applications are first-class platform components.

The product must never contain an enabled control that merely looks functional. Preserve the existing MIT license, COPYRIGHT attribution and third-party notices.

---

# 2. Non-negotiable architectural principle

## 2.1 The UI library comes first

The first architectural dependency of every first-party SLOPOS surface is the SLOPOS UI component library. Backend and QA work may proceed independently under §21; application UI may consume only the verified atoms it needs, and missing atoms must be completed in the library.

The project must create:

```text
crates/slopos-ui
```

before attempting to finish or visually polish the native application suite.

Every visible first-party component is designed as a SLOPOS component.

First-party applications must not independently invent:

- buttons;
- checkboxes;
- radio controls;
- text fields;
- menus;
- list rows;
- scrollbars;
- dialogs;
- alerts;
- toolbars;
- status bars;
- icon grids;
- file items;
- spacing;
- typography;
- focus treatment;
- selection treatment;
- window-content chrome.

Applications consume the component system; they do not define it.

## 2.2 No raw-GTK first-party UI architecture

GTK3/GDK/Pango/GIO may remain mature infrastructure beneath SLOPOS.

They are not the first-party design system.

First-party apps depend on `slopos-appkit`/`slopos-ui`; those SLOPOS abstractions own presentation over GTK3/GDK/Pango/GIO/AT-SPI infrastructure. Follow the explicit “depends on” graph in §6.3.

A first-party application crate must not directly build its user interface from arbitrary `gtk::Button`, `gtk::Entry`, `gtk::Dialog`, `gtk::ListBox`, CSS fragments, or ad-hoc layout constants.

Direct GTK/GDK/Pango UI dependencies are permitted only in explicitly designated boundary crates such as:

- `slopos-ui`;
- low-level engine adapters where required;
- compatibility adapters whose API is hidden behind SLOPOS-owned abstractions.

CI must enforce this architectural boundary.

When a native app needs a missing control, stop the dependent UI slice and request a UI task from the coordinator. The assigned UI owner adds it to `slopos-ui` and verifies conformance before app integration resumes. Continue independent app model/test work within the existing packet; do not edit another owner's component or bypass it.

## 2.3 "Designed from scratch" definition

"Designed from scratch" means SLOPOS owns:

- geometry;
- measurement;
- padding;
- visual hierarchy;
- colors;
- state appearance;
- interaction state machines;
- focus behavior;
- accessible semantics;
- keyboard behavior;
- hit testing;
- drawing composition;
- public widget API;
- conformance tests.

It does **not** require reimplementing invisible mature infrastructure such as:

- Unicode shaping;
- text rasterization;
- input-method protocols;
- AT-SPI transport;
- terminal escape parsing;
- PDF rendering engines;
- video codecs;
- TLS;
- filesystem syscalls.

SLOPOS owns the user-facing component. Mature engines may remain underneath it.

---

# 3. Scope and non-goals

## 3.1 In scope

SLOPOS-I is:

- Linux-only;
- X11-only for this generation;
- Openbox-based unless an evidence-backed limitation requires another solution;
- implemented primarily in Rust;
- built around the first-party `slopos-ui` component library and `slopos-appkit`;
- allowed to use GTK3/GDK/Pango/GIO underneath the SLOPOS component system;
- allowed to use mature engines such as VTE, WebKitGTK, Poppler, libarchive and libmpv where appropriate;
- allowed to delegate system ownership to NetworkManager, PipeWire/WirePlumber or PulseAudio-compatible APIs, BlueZ, UPower, systemd/logind, udev, CUPS/IPP, UDisks2/GIO and XRandR;
- required to provide a coherent first-party shell including Dock, global menu bar, status applets, OSDs, launcher, notifications, desktop surface and session UI.

## 3.2 Out of scope

Do not introduce:

- a Wayland session or fallback in SLOPOS-I;
- a custom display server merely for ownership;
- a custom kernel;
- a speculative future SLOPOS generation;
- a general-purpose GUI toolkit unrelated to SLOPOS;
- the retired legacy Application Strip;
- a Dock copied from modern macOS or any other vendor;
- Aqua / modern macOS visual language;
- GNOME/libadwaita card-heavy design;
- KDE Breeze design;
- Windows Fluent design;
- touch-first spacing as the default desktop density;
- fake compatibility;
- fake state;
- fake application actions.

A proper first-party **SLOPOS Dock is required**. The former "no dock" rule applied to the retired ad-hoc Application Strip and is superseded by this contract.

Wayland work remains paused until the X11 product is mature.

---

# 4. Source-of-truth precedence

Normative precedence is: current explicit user direction (subject to higher-priority environment rules), this contract, the approved machine-readable design specification, approved clean-room assets and component contracts, then implementation conventions. Resolve a genuine product conflict with the user; the coordinator resolves ordinary implementation choices.

Evidence is not beneath prose in this hierarchy. A requirement cannot overrule a failing test, missing component or measured runtime behavior. Part II describes facts separately from Part I's target. Static presence is not runtime verification.

Existing `qa/reference/` files predate the atomic reset and are historical composition references. They cannot override extracted Figma geometry or serve as current conformance goldens. Example measurements in §5 are not a complete or freshly re-extracted specification.

No old numeric readiness score is authoritative. Scoped task verification and product readiness are different claims.

---

# 5. Canonical visual reference

The canonical visual/component reference is:

- **Classic Macintosh UI Kit (Community)**
- Figma file key: `LGMlwNCoVdakZxDBvPKg1W`
- root node: `0:1`

Treat the Figma file as a component specification, not vague inspiration.

Observed examples from the 2026-09-24 audit include:

- menu bar example: 19 px high;
- menu row example: 16 px high;
- large Finder item example: 71×44 px;
- large Finder icon example: 32×32 px;
- large Finder label region example: 71×12 px;
- regular button example: 80×20 px;
- default button example: 88×28 px;
- secondary button example: 80×16 px;
- explicit Rest / Pressed / Disabled button states;
- Action / Hierarchical menu row variants;
- Active / Hover / Disabled menu states;
- Active / Inactive title-bar variants;
- 1-bit and 8-bit reference variants.

These measurements are examples only. The implementation must extract all relevant component data into the machine-readable specification before claiming exact conformance.

## 5.1 Clean-room rule

SLOPOS may reproduce:

- geometry;
- hierarchy;
- spacing;
- density;
- interaction affordances;
- state treatment;
- visual grammar.

Do not ship:

- Apple logos;
- proprietary Macintosh artwork;
- copied proprietary icons;
- proprietary Apple fonts;
- proprietary sounds;
- copied wallpapers;
- copied documentation text.

Third-party assets or fonts require verified redistribution licenses.

If a reference font cannot legally be redistributed, choose or create a redistributable metrically appropriate replacement and document the decision.

---

# 6. High-level architecture

SLOPOS has design/UI, application, platform-service, shell and session/runtime layers. Preserve these boundaries while migrating the existing four crates.

## 6.1 Design and implementation authority

The Figma reference and approved SLOPOS extension specs feed `qa/spec/classic/`. Those definitions drive `slopos-ui` implementations and independent conformance assertions. `slopos-appkit`, shell surfaces and apps compose verified controls. Spec-derived assertions must not simply call the same implementation formula they are supposed to check.

## 6.2 Shell-to-provider rule

A domain has one authoritative backend implementation and shared typed contracts, consumed by any number of views. For example, Volume applet, OSD, Sound panel and media keys use the same AudioProvider API. They may not implement independent `pactl`, `amixer` or PipeWire behavior.

“One provider” means consistent ownership and API, not necessarily one new daemon for the whole desktop. Within a process share its provider instance. Across processes use the same adapter implementation and subscribe/read back from the real OS service; a dedicated SLOPOS daemon is justified only when a recorded requirement needs it. Shell restarts must resubscribe instead of inventing state.

Minimum provider contract:

| Concern | Required behavior |
|---|---|
| Availability | Distinguish loading, available, unsupported hardware, missing service, permission denied, disconnected and failed. Never substitute fixture values. |
| Snapshot/events | Typed state with stable device IDs and an update revision; subscribe without leaking callbacks on reconnect. Handle hotplug and service-owner changes. |
| Command | Typed request, capability/authorization check, bounded completion, error propagation and cancellation where meaningful. |
| Mutation result | Show pending state; confirm success by authoritative reply/read-back or event. Roll back optimistic UI if the action fails. |
| Concurrency | Serialize/conflict-resolve writes to the same device; discard stale completions after device removal or a newer request. |
| Test seam | Injectable transport/clock/filesystem where needed. Fixture data is only available to tests/gallery, never a production fallback. |
| Credentials | Secret-agent integration; no credentials in logs, screenshots or command lines. |

System I/O and blocking D-Bus work do not run on the UI thread. GTK/GDK/Pango objects remain on their required main thread; worker results cross through typed messages. Bound queues and define shutdown/cancellation ownership before adding background loops.

## 6.3 Dependency direction

**Every arrow below means “depends on”; runtime ownership is shown separately in §6.4.**

```mermaid
flowchart TD
    APPS["Native applications"] --> APPKIT["slopos-appkit"]
    SHELL["slopos-shell"] --> APPLETS["slopos-applets"]
    SHELL --> UI["slopos-ui"]
    SHELL --> SERVICES["slopos-services"]
    SHELL --> X11["slopos-x11"]
    APPLETS --> UI
    APPLETS --> SERVICES
    APPKIT --> UI
    APPKIT --> SERVICES
    APPKIT --> X11
    SERVICES --> X11
    UI --> CORE["slopos-core"]
    X11 --> CORE
    SERVICES --> CORE
```

Apps may also consume typed services directly for domain-specific operations. No lower layer imports apps, shell or applets. `slopos-core` has no GUI/system-service dependencies. `slopos-x11` is UI-free; `slopos-services` is UI-free and may use it for display state. Engine adapters expose SLOPOS-owned APIs and may not leak raw GTK widgets into app code.

`slopos-session` supervises processes; it must not depend on UI libraries to keep a broken UI from preventing recovery. The graph describes allowed dependencies, not an instruction to add every dependency to every crate.

## 6.4 Runtime process topology

```mermaid
flowchart TD
    LOGIN["Display-manager X11 session"] --> SESSION["slopos-session"]
    SESSION -->|"supervises"| WM["Openbox"]
    SESSION -->|"supervises"| SHELL["slopos-shell"]
    SESSION -->|"supervises"| AUTH["Authentication and locker integration"]
    SHELL -->|"launches or activates"| APPS["Native and third-party applications"]
    SHELL -->|"reads and requests"| OS["OS services and X11"]
    APPS -->|"reads and requests"| OS
    WM -->|"manages windows"| OS
```

The display manager/Xorg own the X server lifecycle; Openbox is a window manager, not the X server. Shell modules initially share one process with explicit state/lifecycle boundaries. Their UI, model and provider contracts must allow later process isolation without a desktop rewrite.

Session startup establishes the session bus, launches supervised components, detects crash loops with bounded backoff, and offers a recoverable failure path. Restarting the shell/WM must not terminate unrelated applications, duplicate global grabs or lose their window/menu state. Test clean logout, bounded shutdown and recovery separately.

## 6.5 UI construction pipeline

```mermaid
flowchart TD
    SPEC["Approved component spec"] --> IMPLEMENT["SLOPOS component"]
    SPEC --> ASSERT["Independent expected states"]
    IMPLEMENT --> GALLERY["Gallery capture"]
    ASSERT --> COMPARE["Conformance comparison"]
    GALLERY --> COMPARE
    COMPARE -->|"accepted"| COMPOSE["App or shell integration"]
    COMPOSE --> JOURNEY["Graphical VM journey"]
```

There is no application-specific styling step. For Dock, OSD or other absent reference compositions, write a SLOPOS extension spec built from canonical atoms before production UI implementation. Missing Figma access blocks exact extraction/conformance for affected atoms; it does not block unrelated provider, VM or QA work.

## 6.6 Extensibility contracts

Use `SystemProvider`, `SystemApplet`, `ShellSurface`, `ControlPanel`, `Application` and shared `Action` concepts. Add later capabilities through those interfaces rather than miscellaneous shell conditionals. HDR/VRR/color management require a separately validated backend/capability plan; no generic XRandR toggle or VM fixture proves them. Live wallpaper requires lifecycle, resource-budget and fullscreen behavior evidence before exposure.

---

# 7. Target repository structure

These are **target paths**, not a claim they already exist. Part II lists the current four-crate workspace. Every Rust crate has `Cargo.toml` and `src/lib.rs` and/or `src/main.rs`; the module directories below live beneath `src/`, not next to the manifest.

| Path | Owned modules/responsibilities |
|---|---|
| `crates/slopos-core/` | config, errors, IDs, XDG paths, build metadata, capability/IPC identifiers |
| `crates/slopos-ui/` | foundation, render, input, layout, primitives, controls, containers, menus, dialogs, views, accessibility, theme, testing |
| `crates/slopos-appkit/` | lifecycle, actions, menus, documents, undo, clipboard, drag/drop, file dialogs, recent items, MIME, restore, jobs, errors |
| `crates/slopos-x11/` | connection, EWMH, windows, monitors/RandR, workspaces, selections and input |
| `crates/slopos-services/` | network, audio, brightness, Bluetooth, power, media, session, timedate, displays, printers, storage/removable media, applications, input; additional Control Panel providers as required |
| `crates/slopos-applets/` | framework and per-domain models/presenters: Wi-Fi/VPN, Bluetooth, volume, brightness, battery, displays, keyboard, media, removable media, notifications, clock |
| `crates/slopos-session/` | process supervision and recovery |
| `crates/slopos-shell/` | desktop, global menu, Dock, launcher, notifications, OSD, status area, workspace/session UI and shell IPC |
| `apps/` | files, control-panels, terminal, notes, calculator, software, system-monitor, screenshot, system-information, image-viewer, archive-utility, disks, fonts, help, media, documents |
| `utilities/` | auth/locker adapters, shortcuts/media keys, clipboard, removable media, wallpaper, chooser/default-apps, display confirmation, secrets/pairing, URI/desktop-entry launch, session dialogs |
| `tools/` | `slopos-ui-gallery`, `slopos-conformance`, `slopos-qa-driver`, `figma-spec-import` as needed |
| `qa/` | `spec/classic/`, goldens, interaction tests, journeys, fixtures; raw run output follows §30 |
| `assets/`, `themes/` | reviewed clean-room assets and transitional GTK/Openbox theme integration |
| `packaging/`, `scripts/`, `.github/workflows/` | install/release integration and gate runners |

The utility inventory describes responsibilities, not a requirement for a separate process/crate per row. Keep shortcuts and media-key registration under one runtime owner. Keep file dialogs in appkit rather than duplicating them in a Files-only implementation.

New app Cargo package names should consistently use `slopos-<app-name>`; directory and executable compatibility changes are coordinated with packaging. Do not rename the four legacy crates before their replacements preserve session/install behavior. Central manifest/module registration belongs to the coordinator. Prefer focused modules with private implementation details over large shared files.

---

# 8. slopos-core

`slopos-core` contains shared non-UI, non-platform primitives.

It may define:

- application IDs;
- stable IPC identifiers;
- common result/error types;
- XDG path handling;
- configuration serialization;
- version/build metadata;
- resource lookup;
- logging conventions;
- capability descriptors.

It must not depend on GTK.

It must not become a dumping ground for unrelated helpers.

---

# 9. slopos-ui — first-party component library

`slopos-ui` is the foundation of every native SLOPOS interface.

Its public API must make the correct visual language easier than bypassing it.

## 9.1 UI library responsibilities

`slopos-ui` owns:

- all first-party visual tokens;
- all first-party component geometry;
- state rendering;
- input state machines;
- focus behavior;
- accessibility semantics;
- layout primitives;
- icon rendering;
- menu presentation;
- dialog presentation;
- list/table/icon views;
- common content chrome;
- deterministic test rendering.

Applications supply data, actions and domain logic.

Applications do not draw their own design language.

## 9.2 Foundation modules

Required foundation concepts:

```text
foundation/
├── geometry.rs
├── scale.rs
├── metrics.rs
├── insets.rs
├── color.rs
├── typography.rs
├── font_metrics.rs
├── patterns.rs
├── bevel.rs
├── border.rs
├── focus.rs
├── selection.rs
├── icon_metrics.rs
├── animation.rs
└── theme.rs
```

Canonical 1× geometry is the primary coordinate system.

Supported integer HiDPI scaling must preserve deterministic geometry.

Fractional scaling is not required until explicitly added to this contract.

## 9.3 Rendering

Use mature rendering/text infrastructure underneath the SLOPOS API.

Acceptable low-level facilities include:

- Cairo;
- GDK;
- Pango;
- GdkPixbuf;
- GTK widget subclassing/composition where appropriate.

SLOPOS-owned components must not rely on arbitrary distro GTK theme defaults for their canonical appearance.

Pixel-oriented artwork must use pixel-preserving scaling.

Do not use bilinear filtering for reference pixel art where it produces blur.

## 9.4 Required primitive components

At minimum:

- `Surface`
- `Text`
- `Icon`
- `Image`
- `Separator`
- `Rule`
- `PatternFill`
- `Bevel`
- `FocusRing`
- `SelectionHighlight`
- `Spacer`

## 9.5 Required layout components

At minimum:

- `Row`
- `Column`
- `Grid`
- `Overlay`
- `Stack`
- `SplitPane`
- `ScrollView`
- `GroupBox`
- `Inset`
- `Align`
- `FixedCanonicalLayout` for exact reference compositions where appropriate.

Do not use arbitrary magic padding in application crates.

## 9.6 Required input/control components

At minimum:

### Buttons

- Regular Button
- Default Button
- Secondary Button
- Icon Button
- Repeat/Arrow Button where referenced

Required states:

- rest;
- hover if the platform uses hover feedback;
- pressed;
- focused;
- disabled;
- default;
- destructive only if explicitly defined by the SLOPOS language.

### Selection controls

- Checkbox
- Radio Button
- Toggle where a real use case exists
- Disclosure Triangle

Required states include:

- unchecked/unselected;
- checked/selected;
- pressed;
- focused;
- disabled;
- mixed/indeterminate where semantically required.

### Text entry

- Text Field
- Search Field
- Password Field
- Multi-line Text Area
- Numeric Field
- Editable Label where required for file rename

Text-editing implementation may reuse mature GTK text infrastructure internally, but visible geometry, focus, selection, disabled state and surrounding chrome are SLOPOS-owned.

### Choice controls

- Pop-up Button
- Combo/Choice control
- Stepper
- Slider
- segmented control only if the canonical specification explicitly requires one.

### Progress/state controls

- determinate progress;
- indeterminate progress;
- busy indicator if required;
- meter where required.

## 9.7 Required collection/view components

At minimum:

- List View;
- List Row;
- Table View;
- Table Header;
- Tree/Outline View;
- Icon View;
- File Item;
- Desktop Item;
- Empty State;
- Status Row.

Collection components must support keyboard navigation and accessible selection semantics.

## 9.8 Required scrolling components

At minimum:

- Vertical Scrollbar;
- Horizontal Scrollbar;
- Scroll Thumb;
- Scroll Arrow;
- Track;
- Corner/resize affordance where applicable.

Scrolling behavior must be consistent across all native apps.

## 9.9 Required menu components

At minimum:

- Menu Bar;
- Menu;
- Menu Item;
- Hierarchical/Submenu Item;
- Checked Menu Item;
- Radio Menu Item where needed;
- Disabled Menu Item;
- Separator;
- Shortcut/Command label;
- Application menu host;
- Context menu.

Menu state and geometry must derive from the Figma specification.

## 9.10 Required dialog/window-content components

Window-manager decorations are owned by the WM/theme integration, but native window content uses SLOPOS components.

Required:

- Alert;
- Confirmation Dialog;
- Error Dialog;
- Information Dialog;
- Progress Dialog;
- File Open Dialog;
- File Save Dialog;
- Open With Dialog;
- Properties Dialog;
- Preferences/Control Panel content;
- About/System Information content;
- utility-window layout;
- modal/transient host behavior.

## 9.11 Required navigation/content components

Where needed:

- Toolbar;
- Status Bar;
- Path/Location Bar;
- Breadcrumb/Path control if adopted by Files;
- Sidebar only where an explicit application workflow requires it and where it can be reconciled with the canonical visual language;
- Tab strip only where product requirements justify tabs;
- Inspector/Info panel;
- split-view divider.

Do not default to modern sidebars or cards merely because GTK makes them convenient.

## 9.12 Icon system

SLOPOS must define:

- canonical icon sizes;
- device icons;
- folder icons;
- file-type icons;
- action icons;
- status icons;
- application icons;
- high-DPI variants.

Assets must be original or license-compatible.

The UI library controls icon alignment and scaling.

## 9.13 Typography

The library must define shared roles:

- global menu text;
- window/title text;
- body text;
- secondary text;
- list text;
- icon-label text;
- section title;
- utility display text;
- monospace text.

Each role defines:

- family;
- size;
- weight;
- baseline;
- line height;
- scaling rule.

First-party applications may not invent unrelated font sizes.

## 9.14 Accessibility

Every public interactive component must expose:

- accessible role;
- accessible name;
- state;
- selection where applicable;
- keyboard reachability;
- logical focus order;
- disabled semantics.

Accessibility is part of component completion, not a later application patch.

## 9.15 UI Gallery

Create:

```text
tools/slopos-ui-gallery
```

The gallery must display every component and every required state without needing to launch a production application.

It is the canonical component-development environment.

The gallery must support:

- deterministic window size;
- deterministic component fixture data;
- 1× capture;
- integer HiDPI capture;
- keyboard navigation;
- pointer-state tests;
- accessibility inspection;
- stable screenshot names.

No component is complete until it appears in the gallery and its state matrix passes.

---

# 10. Figma-derived machine-readable design specification

Create:

```text
qa/spec/classic/
```

The exact format may be JSON, TOML or another reviewable text format.

It must include:

```text
qa/spec/classic/
├── manifest.*
├── colors.*
├── typography.*
├── metrics.*
├── patterns.*
├── icons.*
├── buttons.*
├── selection-controls.*
├── text-fields.*
├── menus.*
├── scrollbars.*
├── lists.*
├── windows.*
├── file-items.*
└── dialogs.*
```

For each Figma component classify it:

- `REQUIRED`
- `OPTIONAL_VARIANT`
- `NOT_USED`

`NOT_USED` requires a reason.

Each required component/state records as appropriate:

- Figma node/source;
- width;
- height;
- padding;
- margins;
- text baseline;
- icon box;
- border;
- bevel;
- pattern;
- colors;
- state transitions;
- hit target;
- keyboard behavior;
- accessibility semantics.

A completed release may not have required `UNKNOWN` specification entries.

---

# 11. slopos-ui conformance testing

For every required atom/state produce:

- specification fixture;
- rendered fixture;
- geometry assertion;
- state assertion;
- accessibility assertion;
- visual comparison where useful.

Geometry has zero tolerance unless a documented platform-specific exception exists.

Anti-aliased text may use a fixed justified raster threshold, but geometry remains exact.

Do not increase tolerance merely to pass a regression.

Goldens must never auto-update during normal CI.

Changing a golden requires an explicit reviewed specification change.

---

# 12. slopos-appkit — native application framework

`slopos-appkit` is the common application layer above `slopos-ui`.

It prevents every native application from separately reinventing normal desktop behavior.

## 12.1 Application lifecycle

Provide:

- stable application identity;
- single-instance policy where appropriate;
- open-file/open-URI dispatch;
- new-window behavior;
- quit lifecycle;
- session shutdown handling;
- state restoration hooks;
- clean error propagation.

## 12.2 Action/command model

All native apps use a real action model.

Actions define:

- ID;
- label;
- enabled state;
- checked/stateful value where needed;
- shortcut;
- callback;
- menu visibility.

The same action must back:

- menu entries;
- toolbar entries;
- keyboard shortcuts;
- contextual commands.

Do not duplicate action logic across UI surfaces.

## 12.3 Global menu export

Native SLOPOS apps must export their menu/action model through the shared appkit transport so `slopos-shell` displays the same real actions in the global menu bar. Follow the protocol contract in §21.4; keep GTK/GIO transport mechanics behind SLOPOS-owned APIs. Verify enabled/checked state, window-specific actions, exporter disappearance and focus switching with a real producer/consumer test.

No guessed keyboard injection is allowed.

## 12.4 Standard application services

Provide shared APIs for:

- clipboard;
- drag-and-drop;
- undo/redo;
- recent files;
- MIME/default apps;
- file open/save dialogs;
- open-with dialogs;
- error presentation;
- background jobs;
- progress/cancellation;
- notifications;
- preferences;
- persistent window geometry;
- restoration of documents/windows;
- application help;
- About integration.

## 12.5 Document model

For document-based apps define a reusable document lifecycle:

```text
new
→ edited
→ save/save-as
→ clean
→ external modification handling
→ close
```

Support:

- unsaved-change confirmation;
- autosave/recovery where appropriate;
- atomic writes;
- recent documents;
- reopen;
- failure recovery.

## 12.6 Asynchronous work

Long-running work must not block the UI event loop.

Use cancellable jobs for:

- filesystem operations;
- network operations;
- installation;
- archive operations;
- thumbnail generation;
- media metadata;
- document rendering.

Standardize progress/error/cancellation presentation.

---

# 13. Platform integration libraries

## 13.1 slopos-x11

Maintain one authoritative X11 layer using `x11rb` and stable X11 mechanisms.

It owns:

- active window;
- EWMH state;
- root properties;
- window metadata;
- workspaces;
- monitor/RandR model;
- work area;
- placement;
- input integration where appropriate;
- clipboard/selection integration where appropriate.

Do not use `xdotool`, `xprop`, `wmctrl` or `xrandr` as high-frequency polling infrastructure.

Tests may use those tools to drive an isolated VM session.

## 13.2 slopos-services

System providers must be typed and testable.

Required provider modules:

- NetworkManager;
- audio;
- BlueZ;
- UPower/logind;
- timedate;
- display/RandR;
- CUPS/IPP printers;
- removable media/udisks/GIO;
- storage;
- session;
- default applications;
- MIME database;
- desktop entry/application index.

Providers expose real state and real actions.

No provider may return sample state in production.

Fixtures are allowed only behind test-only code paths.

---

# 14. SLOPOS shell

`slopos-shell` is a first-party consumer of the same design system, not a separate visual universe.

It must consume `slopos-ui` and typed provider/applet APIs.

The shell owns:

- global top menu bar;
- application-menu host;
- menu-bar status area;
- first-party Dock;
- desktop surface integration;
- desktop objects;
- launcher/search;
- notification service/presentation;
- OSD manager;
- workspace UI;
- session actions;
- global keyboard shortcuts;
- shell-level media-key routing.

The shell must not create duplicate versions of controls already implemented in `slopos-ui`.

## 14.1 Global menu bar

The menu bar has two regions:

| Region | Content/authority |
|---|---|
| Application region | Focused application's real App/File/Edit/View/Window/Help actions, where exported |
| System/status region | Provider-backed Wi-Fi, Bluetooth, volume, brightness, battery and clock applets |

The left region is backed by the focused application's real action/menu model.

There must be exactly one application-menu bridge.

Native SLOPOS applications export their real `slopos-appkit` action model.

Third-party applications may be integrated only when they expose a supported real menu/action protocol.

If a third-party app does not export one:

- leave its local menu intact; or
- expose only guaranteed shell/window actions.

Do not fabricate Cut/Copy/Paste/Select All by blindly injecting shortcuts.

The right region hosts system applets backed by shared typed providers.

## 14.2 First-party Dock

The Dock is a required shell component and replaces the retired Application Strip.

Required behavior:

- pinned applications;
- running applications;
- active/focused application state;
- multiple-window indication;
- launch;
- raise/focus existing window;
- minimize where the interaction design specifies;
- application/window context menus;
- drag-to-reorder;
- pin/unpin;
- attention state;
- badges/progress only where a real source exists;
- workspace awareness;
- multi-monitor policy;
- configurable auto-hide, dodge and always-visible behavior;
- keyboard accessibility;
- drag/drop where meaningful;
- correct work-area reservation only when visible policy requires it.

The Dock must use `slopos-ui` and a dedicated SLOPOS Dock specification. It must not copy modern macOS Dock appearance or behavior blindly.

Control Panels must expose a Dock panel for at least:

- visibility mode;
- position if multiple positions are supported;
- size/density where supported;
- animation/reveal timing if configurable;
- running-app indicators;
- pinned application management;
- monitor policy where supported.

## 14.3 System applet framework

System applets are reusable projections of `slopos-services` state into the menu bar.

Required initial applets:

- Wi-Fi / Network;
- VPN where NetworkManager exposes it;
- Bluetooth;
- Volume / microphone state;
- Brightness;
- Battery / Power;
- Displays;
- Keyboard layout/input state where applicable;
- Media playback;
- Removable media;
- Notifications / Do Not Disturb where implemented;
- Clock / Date.

Every applet must support a consistent contract equivalent to:

~~~text
identity
current icon/state
accessible label/tooltip
summary state
popup/menu model
real actions
deep link to the relevant Control Panel
availability/unavailable state
~~~

No applet may shell out independently to implement state already owned by a provider.

## 14.4 On-screen display manager

SLOPOS requires a shell-level OSD manager.

Initial OSD events include:

- volume up/down;
- mute/unmute;
- microphone mute;
- display brightness;
- keyboard backlight where available;
- Caps Lock / Num Lock where useful;
- touchpad enable/disable where supported;
- airplane/network mode where supported;
- display-switching feedback where supported.

OSDs must:

- never steal keyboard focus from the active application;
- never appear in Alt+Tab;
- never appear in normal task lists;
- preserve the active X11 window/focus;
- work over fullscreen applications where the X11/compositor stack permits;
- coalesce repeated key presses into one updating overlay;
- dismiss automatically;
- be monitor-aware;
- be accessible without behaving like a normal focus-taking window;
- use actual provider state after the requested change;
- be click-through unless an OSD is intentionally interactive.

Implementation must explicitly test `_NET_ACTIVE_WINDOW`/focus preservation around OSD mapping. Do not accept an OSD implementation that causes a fullscreen game/video/application to lose focus.

## 14.5 Desktop

The desktop supports:

- desktop objects;
- selection;
- keyboard navigation;
- drag/drop;
- Trash;
- mounted volumes where exposed;
- context menus;
- wallpaper;
- multi-monitor placement.

PCManFM may remain temporarily during migration but is not the final source of first-party desktop UI if it prevents conformance.

## 14.6 Launcher/search

The launcher must support:

- application discovery from desktop entries;
- keyboard-first invocation;
- incremental search;
- launch;
- focus/raise existing applications where policy calls for it;
- recent or suggested items only when backed by real data;
- accessibility;
- no fake search categories.

Search providers may expand later, but application launch must remain fast and deterministic.

## 14.7 Notifications

Implement the freedesktop notification protocol truthfully.

Support:

- stable notification IDs;
- replacement/update;
- expiry;
- close;
- application attribution;
- actions only where correctly supported;
- monitor-aware placement;
- Do Not Disturb when implemented;
- notification history only if persistence and privacy behavior are explicitly designed.

## 14.8 Global shortcuts and media keys

One shell-level shortcut service owns global hotkeys.

It routes:

- launcher;
- screenshots;
- volume;
- brightness;
- media playback;
- workspace switching;
- window switching;
- lock/session shortcuts;
- user-configurable shortcuts.

Do not scatter global shortcut registration across unrelated applications.

## 14.9 Shell isolation and restartability

Dock, applets, OSD, notifications and launcher may initially share one executable, but must have clear module/state boundaries.

A failure in one shell surface must not corrupt provider state.

Long-term process separation is permitted when it materially improves reliability.

---

# 15. First-party application suite

A mature SLOPOS desktop needs a coherent set of native applications. The applications below are the canonical first-party product inventory.

Applications are divided into release-critical core applications and extended daily-use applications.

Every first-party application must use `slopos-ui` through `slopos-appkit` unless explicitly documented as a lower-level system utility.

---

# 16. Release-critical core applications

These applications are required for a first-class SLOPOS desktop release.

## 16.1 SLOPOS Files

Target path:

```text
apps/files
```

Purpose: first-party file management and desktop filesystem interaction.

Required functionality:

- directory navigation;
- back/forward/up;
- location/path navigation;
- icon view;
- list view if included;
- keyboard navigation;
- single/multiple selection;
- open;
- default-application/MIME launch;
- rename;
- new folder;
- create supported basic files where exposed;
- copy;
- cut/move;
- paste;
- duplicate if exposed;
- Trash;
- restore from Trash if exposed;
- empty Trash;
- permanent-delete confirmation;
- file/folder properties;
- context menus;
- drag-and-drop;
- hidden files;
- filesystem change watching;
- error handling;
- removable volumes;
- mount/unmount/eject where supported;
- thumbnailing where appropriate;
- network mounts if exposed.

Suggested mature backends:

- GIO/GVfs;
- freedesktop MIME database;
- udisks/GIO volume APIs.

PCManFM is a temporary migration dependency only if it cannot meet the final visual/interaction contract.

## 16.2 SLOPOS Control Panels

Target path:

```text
apps/control-panels
```

This replaces the current first-party Settings UI architecture.

Required panels for production daily use:

### Desktop and appearance
- Appearance
- Desktop / Wallpaper
- Dock
- Menu Bar / Status Items
- Fonts
- Notifications
- Default Applications

### Hardware
- Displays
- Brightness where hardware supports it
- Sound
- Keyboard
- Mouse
- Touchpad where present
- Bluetooth
- Printers

### Connectivity
- Network
- Wi-Fi
- Wired
- VPN integration through NetworkManager where available
- Proxy if supported by platform policy

### System
- Power
- Date & Time
- Region & Language
- Users
- Storage
- Removable Media
- Autostart / Session
- Keyboard Shortcuts
- Accessibility
- About / System Information entry point

Every panel reads real provider state.

No sample SSIDs, IPs, device names, volume values or hardware state may appear in production.

## 16.3 SLOPOS Terminal

Target path:

```text
apps/terminal
```

Use a mature terminal engine such as VTE.

SLOPOS owns:

- window;
- menus;
- tabs if supported;
- preferences;
- keyboard shortcut model;
- appearance;
- dialogs;
- search UI;
- profile UI.

Required:

- shell launch;
- terminal resize;
- scrollback;
- copy/paste;
- selection;
- search;
- Unicode;
- IME compatibility;
- links;
- working-directory launch;
- command exit behavior;
- accessibility;
- tabs only if completed coherently.

Do not write a terminal emulator parser from scratch.

## 16.4 SLOPOS Notes

Target path:

```text
apps/notes
```

Purpose: lightweight first-party writing and text-note application.

Required:

- new/open;
- save/save-as;
- plain text;
- Unicode;
- undo/redo;
- cut/copy/paste;
- find;
- basic formatting only if explicitly supported;
- modified-document warning;
- autosave/recovery if adopted;
- recent documents;
- drag/drop file opening;
- print only when genuinely implemented.

It may later support Markdown, but unsupported formatting controls must not be exposed.

## 16.5 SLOPOS Calculator

Target path:

```text
apps/calculator
```

Calculator is both a real utility and an early conformance proving ground.

Required:

- keyboard and pointer input;
- basic arithmetic;
- decimal;
- sign;
- clear;
- backspace;
- operator precedence policy documented/tested;
- divide-by-zero/error state;
- copy result;
- predictable focus.

Its UI must be composed entirely from `slopos-ui`.

## 16.6 SLOPOS Software

Target path:

```text
apps/software
```

This evolves from `slopos-catalogue`.

It manages SLOPOS-curated application delivery according to the release policy.

Required:

- catalogue search;
- application detail;
- install;
- integrity verification;
- launch;
- uninstall;
- version display;
- error reporting;
- background/cancellable downloads;
- progress;
- architecture validation.

Initial delivery policy: evolve the existing curated AppImage catalogue. Installed user applications and desktop entries live in user-owned XDG locations; downloads require an explicitly trusted source, integrity verification, architecture checks, atomic installation, cancellation cleanup and uninstall that preserves unrelated user data. A checksum alone does not establish publisher trust. Do not invent a public SLOPOS package repository.

Distribution package management remains owned by the base distribution unless the product contract explicitly expands scope. Native distro-package management is not silently part of the AppImage store task.

## 16.7 SLOPOS System Monitor

Target path:

~~~text
apps/system-monitor
~~~

This is SLOPOS's full task-manager/system-observation application.

Required views:

- Processes;
- CPU;
- Memory;
- Storage activity where practical;
- Network activity where practical;
- GPU activity where a stable provider exists.

Process view must expose at least:

- name;
- PID;
- CPU usage;
- memory usage;
- state;
- user;
- command/details on inspection.

Required actions:

- search/filter;
- inspect process;
- end/terminate process;
- force kill with explicit confirmation;
- process priority adjustment only when correctly authorized;
- refresh without UI stalls.

No process action may silently claim success. Read back process state.

Use procfs/sysfs and stable system APIs.



## 16.8 SLOPOS Screenshot & Recorder

Target path:

~~~text
apps/screenshot
~~~

Screenshot is a first-party shell-integrated capture utility.

Required screenshot modes:

- entire screen;
- selected monitor where useful;
- selected window;
- selected region.

Required output paths:

- save;
- clipboard;
- open captured image in SLOPOS Image Viewer.

Required behavior:

- configurable delay;
- multi-monitor awareness;
- predictable filenames;
- visible success/error feedback;
- global keyboard shortcuts owned by the shortcut service;
- non-destructive cancellation.

Recording is enabled only when functional.

Target recording modes:

- entire screen;
- selected monitor;
- selected window where technically reliable;
- selected region;
- optional microphone;
- optional system audio.

Use a mature capture/encoding stack such as PipeWire, FFmpeg or GStreamer as appropriate for X11. Do not implement codecs.

Do not ship a Record control until the selected recording path passes real VM tests. The required “open captured image in SLOPOS Image Viewer” path also makes APP-08's minimum viewer slice a capture-release dependency; an unimplemented handoff is not a completed action.



## 16.9 SLOPOS System Information / About

Target path:

```text
apps/system-information
```

Required:

- SLOPOS version;
- source/build revision where appropriate;
- OS;
- kernel;
- architecture;
- CPU;
- memory;
- storage summary;
- graphics/display information where available;
- X11/session information;
- license/credits.

Every displayed value must be real.

This app also provides the canonical compact About composition required for visual QA.

---

# 17. Extended first-party daily-use applications

These are part of the target SLOPOS application suite. They may be implemented after the release-critical core is stable, but they must follow the same architecture.

## 17.1 SLOPOS Image Viewer

Target:

```text
apps/image-viewer
```

Required:

- common image formats through mature loaders;
- next/previous;
- zoom;
- fit;
- actual pixels;
- rotate where supported;
- fullscreen;
- file metadata;
- copy;
- set as wallpaper integration where appropriate.

## 17.2 SLOPOS Archive Utility

Target:

```text
apps/archive-utility
```

Use mature engines such as `libarchive`.

Required:

- inspect common archive formats;
- extract;
- create supported archive formats;
- drag/drop integration;
- overwrite/conflict handling;
- path traversal protection;
- progress/cancel;
- error reporting.

Files should invoke it transparently for archive operations.

## 17.3 SLOPOS Disks

Target:

```text
apps/disks
```

Purpose: storage inspection and safe user-level disk operations.

Use mature storage APIs such as UDisks2.

Required scope may include:

- device/partition information;
- mount/unmount;
- eject;
- SMART information where safely available;
- filesystem usage;
- formatting only when properly authorized and guarded by explicit destructive confirmations.

Destructive functionality must not be enabled until thoroughly tested.

## 17.4 SLOPOS Fonts

Target:

```text
apps/fonts
```

Required:

- browse installed fonts;
- preview;
- metadata;
- install user font;
- remove user-installed font;
- refresh font cache;
- clear distinction between system-owned and user-owned fonts.

## 17.5 SLOPOS Help

Target:

```text
apps/help
```

Required:

- offline-first SLOPOS help;
- searchable topics;
- keyboard navigation;
- deep links from applications/settings;
- release-appropriate troubleshooting;
- no stale generated feature claims.

## 17.6 SLOPOS Media

Target:

```text
apps/media
```

Use a mature media engine such as libmpv or GStreamer.

SLOPOS owns:

- menus;
- transport controls;
- playlist;
- open dialogs;
- subtitles/audio track UI;
- fullscreen UI;
- preferences.

Do not implement codecs from scratch.

## 17.7 SLOPOS Documents

Target:

```text
apps/documents
```

Purpose: PDF/document viewing.

Use a mature rendering engine such as Poppler for PDF.

Required:

- open;
- page navigation;
- zoom;
- search;
- thumbnails if implemented;
- print only if fully integrated;
- metadata;
- keyboard navigation.

## 17.8 SLOPOS Browser — optional ecosystem application

A first-party browser shell may be built only around a maintained secure engine such as WebKitGTK.

Do not build a browser engine.

Until this application exists and meets security/update expectations, SLOPOS must integrate established browsers such as Firefox/Chromium cleanly.

A browser is not allowed to delay core desktop correctness.

---

# 18. System utilities and background infrastructure

A production desktop depends on many components that users do not launch as ordinary applications. These are first-class requirements, not afterthoughts.

## 18.1 Authentication agent

Target:

~~~text
utilities/polkit-agent
~~~

Provide a SLOPOS-native Polkit authentication UI backed by the real Polkit agent API.

Required:

- requesting application/action identity;
- password prompt;
- cancellation;
- error state;
- correct secure entry;
- no credential logging.

## 18.2 Lock screen

Target:

~~~text
utilities/lock-screen
~~~

SLOPOS may initially use a mature external locker if necessary.

A first-party lock screen becomes acceptable only when:

- PAM/authentication path is correct;
- secure keyboard handling is reviewed;
- failure cannot expose the session;
- multi-monitor behavior is correct;
- suspend/resume behavior is correct.

Security takes priority over ownership. A working, tested locker integration is mandatory for daily-use release readiness even when a mature external locker supplies it; an unavailable Lock item is an honest alpha limitation, not a production pass.

## 18.3 File chooser

Native SLOPOS apps use a SLOPOS file chooser implemented through `slopos-appkit` and `slopos-ui`.

It must support:

- open;
- save;
- folder selection;
- filters;
- recent locations;
- filename validation;
- overwrite confirmation;
- keyboard operation.

Third-party GTK apps may continue using GTK dialogs unless a safe toolkit integration mechanism exists.

## 18.4 Open With / Default Apps

Provide:

- application chooser;
- default association selection;
- MIME-aware candidate list;
- persistent default update;
- one-time open without changing default.

## 18.5 Global shortcut service

One service owns shell-level shortcuts and media keys.

It must provide conflict detection, configured bindings, provider dispatch and real feedback.

## 18.6 Clipboard manager

Provide base X11 clipboard/selection correctness first.

Persistent clipboard history is optional until explicitly designed with:

- privacy policy;
- exclusion rules;
- secret/sensitive content behavior;
- storage lifecycle.

## 18.7 Removable-media handler

Respond to real media events and integrate with:

- Files;
- notifications;
- menu-bar applet;
- safe unmount/eject;
- configured autorun/open behavior.

## 18.8 Wallpaper manager

Own:

- configured wallpaper;
- per-monitor policy where supported;
- static wallpaper application;
- live wallpaper only when explicitly implemented and performance-tested.

## 18.9 Display-change confirmation

Risky display changes must offer an automatic rollback countdown so an unusable mode does not strand the user.

## 18.10 Network-secret UI

Network authentication/secrets must use NetworkManager's supported secret-agent mechanism or another appropriate stable API.

Do not log secrets.

## 18.11 Bluetooth pairing UI

Pairing requests must present:

- device identity;
- PIN/passkey/confirmation state;
- cancel/failure;
- trusted/connected result.

## 18.12 Printer authentication UI

Where CUPS/IPP requires credentials or authorization, present a truthful native prompt with no credential logging.

## 18.13 Low-battery and hardware-state notifications

Power/provider events may trigger:

- low battery;
- critical battery;
- device removal;
- network loss;
- other actionable hardware state.

Rate-limit repetitive notifications.

## 18.14 URI opener and desktop-entry launcher

Provide one safe launch path for:

- URIs;
- MIME/default applications;
- desktop entries;
- files.

Avoid every application inventing process-spawn rules.

## 18.15 Session dialogs

Provide SLOPOS-native:

- Log Out;
- Restart;
- Shut Down;
- Sleep confirmation where appropriate.

Actions must use real logind/session provider capability checks.

## 18.16 Crash/error presentation

Unexpected first-party application failures should produce useful logs and recoverable user-facing errors where practical.

Do not silently restart forever or hide repeated failure.

## 18.17 Infrastructure inventory rule

When a later requirement is remembered, first classify it as one of:

- provider;
- applet;
- shell surface;
- Control Panel;
- native application;
- background utility.

Add it through the appropriate stable interface instead of putting miscellaneous code into `slopos-shell`.

---

# 19. Native application anatomy

Every native application should follow a common internal structure:

```text
apps/<name>/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── actions.rs
│   ├── menu.rs
│   ├── model/
│   ├── services/
│   ├── views/
│   └── tests/
└── resources/
```

Recommended layering:

```mermaid
flowchart TB
    VIEW["Views using slopos-ui"]
    ACTIONS[actions/menu commands]
    MODEL[domain model/state]
    SERVICES[app-specific adapters]
    APPKIT[slopos-appkit]
    PLATFORM[slopos-services / slopos-x11]

    VIEW --> ACTIONS
    VIEW --> MODEL
    ACTIONS --> MODEL
    ACTIONS --> SERVICES
    SERVICES --> APPKIT
    SERVICES --> PLATFORM
```

Views must not directly shell out to command-line tools.

Views must not contain system-provider logic.

---

# 20. Architectural enforcement

Add automated architecture tests.

At minimum enforce:

- first-party app crates do not directly depend on `gtk`, `gdk`, `pango` or arbitrary CSS files;
- first-party app UI goes through `slopos-ui`/`slopos-appkit`;
- duplicated UI constants are rejected where practical;
- production source does not contain known fixture/sample SSIDs, IP addresses or hardware labels;
- there is one global-menu implementation;
- no new `xdotool` runtime application-action injection;
- no enabled empty callbacks;
- no TODO placeholder controls in release-visible code.

Record each temporary exception in Part II.N with exact path, reason, owner, test and removal task. Do not create a competing architecture document or allow whole new app directories to bypass the boundary.

Inspect Cargo metadata by resolved package identity (including dependency aliases) and combine it with source/behavior checks; a grep for `gtk::Button` alone is not enforcement. Test-only fixture strings are allowed in test fixtures; production sample values are not. Keep domain logic testable without initializing GTK.

Legacy crates initially need a bounded migration exception. Freeze the exception list at the audited baseline; no new raw-GTK surface is allowed under it. Narrow/delete entries as migration lands. An exception is acknowledged debt, never a conformance pass.

QA-01 must also check task path ownership, specification completeness, Markdown policy, duplicate menu ownership, and unregistered required gates. Until that checker exists, the coordinator reviews these constraints manually and records the limitation.

---

# 21. Dependency-driven implementation plan

The current tree is a migration donor. Preserve boot/session behavior and introduce one tested slice at a time. “UI first” forbids app-local controls and premature application completion; it does not prevent independent VM, backend, protocol or QA work while UI atoms are built.

The following task IDs are a starting backlog. At the audited baseline **none is assigned or verified**. The coordinator records actual status in Part II.M and expands any multi-step row into leaf packets before dispatch. A path prefix is a boundary, not permission to edit every file underneath it.

## 21.1 Bootstrap and first dispatch

1. Coordinator: establish VM-00, record the baseline and reserve resource/runtime ownership.
2. Parallel read/preparation: SPEC-01 extracts the authoritative component inventory; QA-00 inspects and measures the existing baseline. Neither invents current passes.
3. Coordinator: integrate BOOT-01's minimal shared interfaces/manifests and publish an immutable starting SHA. New crates must contain a useful implemented invariant/test, not empty “completed” scaffolds.
4. Dispatch UI-01, QA-01 and PLAT-01 or one SVC task against that SHA if resources allow. They can edit in parallel; build/runtime permits remain separately scheduled.
5. Unlock controls, appkit, apps and shell only when their actual dependencies integrate. A worker waiting on UI can take a provider/model/test task with distinct ownership.

Do not launch “an agent for every app” at the start. Do not make each agent update this contract or reinvent tokens, menus, providers, file choosers and command dispatch.

## 21.2 Foundation tasks

| ID | Depends on | Writable area to allocate | Concrete acceptance |
|---|---|---|---|
| VM-00 | none | Guest setup/evidence; coordinator | Confirm hypervisor/guest, guest-local checkout, disk/RAM budget, X11 test account, recovery access and execution policy. |
| QA-00 | VM-00 | Baseline evidence only | Record existing build/test results or exact failures at baseline SHA; inventory script side effects, dependencies and inherited defects. No “all green” inference. |
| SPEC-01 | source access | `qa/spec/classic/`, licensed asset provenance | Machine-readable schema, complete required-component inventory, source node IDs, revision/scale/font/license metadata and first foundation/button/menu/text specs. Unknowns explicit. |
| BOOT-01 | QA-00 recorded | Coordinator: workspace, core, initial UI/gallery manifests and module roots | Freeze IDs/errors/capabilities and crate APIs; compile first meaningful invariant tests; keep existing session/package names working. Record exact Rust version; choose and test a toolchain pin before reproducibility claims. |
| QA-01 | BOOT-01 | Assigned QA runners/tests and CI changes | Isolate displays, buses, child cleanup and output paths; enforce architecture/migration exceptions; validate required-gate inventory, failure exit codes and fixture separation. |
| UI-01 | BOOT-01, SPEC-01 | `crates/slopos-ui/src/foundation,render,layout,primitives,accessibility`; assigned gallery fixtures | Real surface/text/icon/bevel/focus/layout at 1×/2×; deterministic measurements and independent geometry assertions; AT-SPI base. |
| UI-02 | UI-01 | `crates/slopos-ui/src/controls/` allocated button/selection modules | Button, checkbox, radio and disclosure states; pointer press/release/cancel, focus and keyboard activation; disabled controls cannot fire actions. |
| UI-03 | UI-01 | Allocated text-entry modules | Text/search/password/numeric/multiline entry as required; selection, IME, clipboard, Unicode, validation and password privacy. |
| UI-04 | UI-01 | `crates/slopos-ui/src/menus/` | Menu bar/items/submenus/check states; keyboard traversal, dismissal, enabled actions, exact geometry and focus restoration. |
| UI-05 | UI-01, UI-02 | Allocated scrolling, list/table/tree/icon-view modules | Scroll, selection, keyboard navigation, long labels/large collections, accessible selection and pixel-preserving icon scale. |
| UI-06 | UI-03, UI-04, UI-05 | Allocated dialogs/window-content/navigation modules | Dialog/alert/toolbar/status/path compositions, modal focus/escape/default behavior, overflow handling and accessibility. |
| SPEC-02 | SPEC-01 | Allocated extension specs, no app styling | Approved Dock, OSD, status-area and modern system-dialog compositions built from Classic atoms; independent acceptance fixtures. |
| PLAT-01 | BOOT-01 | `crates/slopos-x11/` plus explicitly assigned donor extraction | One X11 connection/event/monitor/window authority; preserve active window, workspace and work-area state across hotplug and WM restart. |
| KIT-01 | UI-02, UI-03, UI-04, PLAT-01 | `crates/slopos-appkit/src/application,actions,menus,jobs,errors` | Lifecycle plus real action/menu export; one command invoked from keyboard/local/global menu; state changes and cancellation cross process boundaries correctly. |
| KIT-02 | KIT-01, UI-05, UI-06, SVC-07 | Appkit documents/undo/clipboard/drag-drop/dialogs/MIME/restore | Atomic save and recovery, unsaved-close prompt, chooser overwrite handling, clipboard/drag-drop and safe MIME/URI dispatch. |
| PROOF-01 | KIT-01, required verified atoms | `apps/calculator/` | Arithmetic/error/keyboard/copy journeys; no direct GTK UI. |
| PROOF-02 | KIT-01, required verified atoms | `apps/system-information/` | Real system/build values, truthful unavailable fields, compact About scene and accessibility. |

Gallery entry files and tests belong to their component task; shared gallery registration belongs to the coordinator. Unextracted required variants remain SPEC-01/UI work and must be closed before §35 component completion.

## 21.3 Providers and shared infrastructure

Provider work may overlap UI development after BOOT-01. Register modules centrally; workers own distinct domain files. Split a row by discovery, mutations and reconnect tests where necessary.

| ID | Depends on | Area | Acceptance |
|---|---|---|---|
| SVC-01 | BOOT-01 | services/network state/transport | Real NetworkManager adapters, APs, connection/IP state; missing daemon, no adapter, disconnect and hotplug tests. |
| DONOR-TRUTH-01 | None; legacy-shell maintenance exception | Coordinator: `crates/slopos-shell/src/topbar.rs` | Remove fabricated Ethernet/SSID rows from the current donor menu and state explicitly that connection detail is unavailable there; preserve the separately monitored summary and preferences action. This is a scoped truthfulness repair, not SVC-01 or target-app completion. |
| DONOR-TRUTH-04 | None; legacy Settings maintenance exception | Coordinator: `crates/slopos-settings/src/panels/network.rs`, `crates/slopos-shell/tests/shell_tests.rs` | Remove invented Ethernet/Wi-Fi/IP state and inert switches/connect buttons; truthfully state this panel has no live network details, preserve only a real external editor handoff when installed and show spawn failures. This repair does not complete Settings Network or SVC-01/SVC-02 and does not authorize new raw-GTK UI. |
| DONOR-MENU-TRUTH-02 | None; legacy-shell maintenance exception | Coordinator: `crates/slopos-shell/src/topbar.rs`, `crates/slopos-shell/tests/shell_tests.rs` | Remove fallback menu items that synthesize application/desktop hotkeys rather than invoking exported actions or guaranteed window operations; keep authoritative app-menu import and real window-management actions. |
| DONOR-MENU-BRIDGE-03 | None; legacy-shell maintenance exception | Coordinator: `crates/slopos-shell/src/main.rs`, `crates/slopos-shell/src/gmenu.rs`, `crates/slopos-shell/src/menu/gmenu.rs`, `crates/slopos-shell/tests/shell_tests.rs` | Keep one registered X11 global-menu bridge and its GTK/KDE property handling; remove the unused duplicate implementation and point tests at the runtime bridge. |
| SVC-02 | SVC-01 | services/network actions/secrets model | Connect/disconnect/toggle/VPN capability; authorization/cancel/timeouts; authoritative read-back; no sample SSIDs or logged secrets. |
| SVC-03 | BOOT-01 | services/audio | Real outputs/inputs/defaults/volume/mute and events; denied/failed writes and hotplug; one API for Sound, applet and OSD. |
| SVC-04 | BOOT-01 | services/brightness and power, separately allocated | Real capability/state/change paths, bounds/read-back, no-backlight/no-battery behavior, low-battery and suspend capability. |
| SVC-04-SUSPEND | SVC-04 power provider foundation | `crates/slopos-services/src/power.rs`, `crates/slopos-shell/src/services/power.rs`, shell Sleep menu | Bounded logind suspend request; query CanSuspend, allow yes/challenge only, request interactive authorization, surface denial/failure, preserve suspend state without UPower, and drive menu enablement from provider state. Run away from GTK thread; physical suspend is a separate VM/hardware gate. |
| SVC-05 | PLAT-01 | services/displays | Real modes/layout, hotplug, timed apply/confirm/rollback; recover from loss of the controlling output. |
| SVC-06 | BOOT-01 | services/Bluetooth, media, timedate, input; one domain per packet | Real API plus unavailable/denied/reconnect fixtures per domain; same state consumed by panel and applet. |
| SVC-07 | BOOT-01 | services/storage, removable-media, applications | Safe media events/mounts, desktop-entry/MIME discovery and launch; command argument handling and untrusted filename tests. |
| SVC-08 | BOOT-01 | Additional Control Panel providers, one per packet | Printers, users, region/language, autostart, accessibility/preferences and other §16.2 panels get explicit authority/capability/read-back contracts; none is silently forgotten. |
| SYS-01 | KIT-01, UI-06 | Auth/locker/session integration, exact paths allocated | Native Polkit flow; mature locker integration first; cancellation, logout, shell/WM crash recovery, lock/suspend/resume and all-monitor behavior. |
| SYS-02 | KIT-02, relevant SVC tasks | Remaining §18 utilities, one per packet | Chooser/defaults, secret/pairing prompts, wallpaper, media events and session dialogs integrated without duplicate providers or shortcut owners. |

System-service tests use isolated fixtures or an exclusive disposable guest session. Fixture success must not be relabelled real hardware success.

## 21.4 Applications and shell

The small proof applications validate the common platform before larger native UI migrations. Reuse already verified atoms; schedule any missing atom back into the UI queue.

| ID | Depends on | Area | First acceptance slice, then remaining section contract |
|---|---|---|---|
| APP-01 | KIT-02, UI-05, SVC-07, proof apps | `apps/files/` | Navigation/watch/open/selection, then independently reviewed copy/move/conflict/cancel/Trash/removable-media tasks (§16.1). |
| APP-02 | KIT-02, relevant SVC tasks, proof apps | `apps/control-panels/` | Real provider-backed panel host; one panel per task; applet deep links and persistence (§16.2). |
| APP-03 | KIT-02, proof apps | `apps/terminal/` and named engine adapter | PTY/VTE launch/resize/exit; later search, Unicode/IME, clipboard and accessible chrome (§16.3). |
| APP-04 | KIT-02, proof apps | `apps/notes/` | New/edit/save/reopen/error/unsaved-close journey and undo/find (§16.4). |
| APP-05 | KIT-02, SVC-07, proof apps | `apps/software/` and assigned catalogue donor paths | Trusted fixture search/install/integrity/launch/uninstall; cancel/failed download and architecture rejection (§16.6). |
| APP-06 | KIT-01, UI-05, proof apps | `apps/system-monitor/` | Real process/resource snapshots; PID-reuse-safe terminate confirmation/read-back; permission failures (§16.7). |
| APP-07 | KIT-02, PLAT-01, SHELL-04, APP-08 | `apps/screenshot/` | Screen/window/region capture, clipboard/save/open-in-viewer; cancellation and multiple monitors (§16.8). Recording is a separate task. |
| APP-08 | KIT-02, proof apps | `apps/image-viewer/` | Open/fit/zoom/actual pixels; this minimum is required by Screenshot's advertised viewer handoff. Finish remaining §17.1 functions subsequently. |
| SHELL-01 | KIT-01, UI-04, PLAT-01, proof apps | Shell global-menu module and named donor paths | One protocol-aware bridge; correct app/window focus tracking and action state; preserve local menus for unsupported apps. |
| SHELL-02 | SPEC-02, UI-05, PLAT-01, SVC-07, proof apps | Shell Dock module | Pin/launch/activate/reorder/persist; multiple windows, hide/dodge/work-area and focus behavior (§14.2). |
| SHELL-03 | KIT-01, SPEC-02, relevant SVC tasks, proof apps | `slopos-applets/` and shell status host | One applet per provider: real state/action/read-back, unavailable states and Control Panel deep links. |
| SHELL-04 | SPEC-02, PLAT-01, SVC-03/04/06, proof apps | Assigned OSD and global-shortcut modules | One key owner; coalesced real-state OSD; active X11 window and input focus unchanged through fullscreen use. |
| SHELL-05 | KIT-02, PLAT-01, SVC-07, APP-01 | Desktop, launcher, notifications, workspace/session UI; separate packets | Native surface migration with real launch/notification/desktop actions and restart restoration; retain working migration path until replacement passes. |
| EXT-01 | Core desktop stable, required appkit/providers | Remaining §17 apps; one app/behavior per packet | Archive Utility, Disks, Fonts, Help, Media, Documents and remaining Image Viewer requirements; browser remains optional. |
| INT-01 | Required core tasks integrated, QA-01 | Integration worktree, journeys, packaging and ledger | Exact-SHA workspace gates; graphical VM daily-use journeys; install/upgrade/remove/recovery; no hidden failing required gate. |

“Proof apps” means PROOF-01 and PROOF-02 verified/integrated. Runtime wiring files (`main.rs`, startup scripts, menu registries, desktop entries and package manifests) are integrated by their single designated owner. Independent workers must not each migrate `topbar.rs`.

Native menu transport initially follows the existing GTK/GIO GMenu/action-group path, hidden behind appkit and a SLOPOS menu model. GTK GMenu and DBusMenu are distinct adapters; discovering a service/object path is not protocol validation. Consolidate to one bridge owner with explicitly supported adapters, not one decoder pretending every exporter uses the same protocol. Add producer/consumer integration tests before claiming an adapter supported.

## 21.5 Milestone gates and migration cutover

| Milestone | Entry/exit condition |
|---|---|
| Foundation | VM baseline recorded; spec inventory and all atoms used by the proof apps verified; shared APIs, gallery and boundary checks integrated. No desktop-readiness claim. |
| Platform proof | Both proof apps complete; appkit menus/actions, async/error lifecycle and necessary providers verified. |
| Desktop alpha | Native core apps/shell integrated; enabled actions truthful; core journeys usable; unresolved device/release gates enumerated. Alpha does not mean production-ready. |
| Daily-use release candidate | All §37 requirements for the declared scope, mandatory app dependencies, graphical VM evidence and install/upgrade/remove/recovery gates pass on the candidate. |
| Production | §37 and declared release matrix satisfied with current evidence; no blocking defect or unverified required claim. Extended inventory remains tracked, not silently deleted. |

For each replacement, first record the legacy entry point, desktop ID/config migration, behavior to preserve and rollback path. Integrate the new implementation behind an explicit development selection if needed. After it passes, switch launcher/session/package wiring atomically and remove the obsolete path/exception. Do not delete PCManFM or other working fallback before the native replacement covers its required journeys; do not keep fallback use hidden in a “native complete” claim.

Keep fixes for fabricated production state and misleading readiness text in the migration queue. A truthful unavailable state is a valid temporary repair, not completion of a required provider. A blocked specification or device task does not block unrelated ready work.

---

## 21.6 Future feature goal — SLOPOS Intelligence

**Status: PLANNED / NOT IMPLEMENTED. Priority: post-core-desktop; not a SLOPOS-I alpha, daily-use release candidate or production gate under §21.5/§37.** This is an architectural and user-experience goal, not permission to build new shell UI before `slopos-ui` and `slopos-appkit` are ready. Prototype its backend independently in the mandated disposable Linux VM after the needed platform interfaces are stable.

### User-facing outcome

Provide an opt-in, local-first AI desktop operator invoked through text, a global shortcut and eventually voice. It understands a natural-language objective, decomposes it into observable steps, **visibly operates the actual SLOPOS desktop** (opens/focuses windows, moves the cursor, clicks, types, navigates menus, captures screenshots) and verifies each result before proceeding. It must be pausable, interruptible and explicitly disclose what it is doing.

Representative journeys, not present capabilities:

1. "Open Files, select the three newest screenshots, archive them and place the ZIP on the desktop."
2. "Open Control Panels, change a supported preference and show the authoritative applied state."
3. "Capture the active window, open the resulting image in the viewer, and save it to the requested location."
4. "Navigate a supported third-party application through its accessible controls; if semantics are absent, use an approved visual-control fallback." Never promise universal arbitrary-app support.

### Proposed architecture (target paths, not existing crates)

- `crates/slopos-automation/`: window enumeration/focus, screenshot capture, X11/XTest input injection and AT-SPI2 accessibility observation/actions, behind typed backend interfaces. Restrict operations to the active, authorized user session.
- `crates/slopos-intelligence/`: opt-in model/provider interface, task planner, bounded action executor, observation/verification loop, recovery, permission/policy engine and sanitized local task history. Local model support is the default goal; optional remote inference requires separate informed opt-in and a clear account of transmitted content.
- `crates/slopos-assistant/` or an allocated `slopos-shell` surface: non-focus-stealing task HUD, keyboard/text command input, action preview, confirmations, pause/stop, progress and errors. Voice recognition/TTS are later optional adapters, not a dependency for the first slice.
- First-party apps expose stable typed `slopos-appkit` actions and useful accessibility labels/states; prefer shared `slopos-services` commands/read-back for reliable system mutations. The operator does **not** implement duplicate NetworkManager/audio/display provider logic or bypass normal authorization.
- The intelligence/planning layer must not depend on X11-specific types. X11/Openbox is the only backend for SLOPOS-I; any future Wayland backend is separately scoped and is not authorized by this goal.

**Execution precedence:** authorized typed system/app action with authoritative read-back → AT-SPI2 named role/action/state traversal → bounded screenshot/vision-assisted coordinate actions as fallback. A system action may reveal its outcome in an actual app window; it must not be represented as visible clicking if no visible clicking occurred. The executor re-observes after each meaningful step rather than running a blind fixed click script. Do not give an LLM unconstrained shell execution, unrestricted file writes or unrestricted synthetic input.

### Privacy, security and reliability requirements

- Off by default; clear per-task permission and an always-visible operating indicator. Provide immediate stop shortcut, user takeover, timeout/step/action budgets and cancellation without leaving synthetic keys/buttons held.
- Separate read-only actions, ordinary reversible writes, and sensitive/destructive/external actions (deletion, payments, messaging, credential entry, permission or security changes); the latter require explicit context-specific user confirmation. No silent approval chaining.
- Mask or exclude secrets/secure text, private window contents and unrelated applications wherever possible; never intentionally send screenshots, accessibility trees or credentials to remote services without explicit opt-in. Support per-app and per-action deny rules; redact logs and clear task data.
- Record that X11 cannot enforce strong per-app input/screen isolation against other local X clients: consent, masking and policy are best-effort protections, **not** an OS-level sandbox or confidentiality guarantee.
- Fail closed when the UI target cannot be identified confidently, state is stale, input focus differs, the screen is locked, a permission is denied or a verification step fails. Do not fabricate completion, app state or results.

### Future milestone packets and testable acceptance

| ID | Dependencies | First independently verifiable acceptance |
|---|---|---|
| AI-01 Observe/act foundation | PLAT-01, QA-01 | In a disposable graphical X11 VM, enumerate/focus a test window, inspect AT-SPI2 tree, inject bounded click/type, capture the actual display and restore input state. Demonstrate permission denial, stop and teardown. |
| AI-02 Typed planner/executor | AI-01, applicable KIT/SVC interfaces | Using deterministic stub planner and mocked providers, execute/verify a multi-step scenario, recover from wrong window/absent control, cancel safely, and reject unapproved system mutations. No model-quality claim from stub tests. |
| AI-03 First-party visible journey | AI-02, native Files/Control Panels/Screenshot functions | Execute at least one user-observable cross-app journey on real X11/Openbox with true read-back, on-screen progress, task stop and explicit confirmation where required; collect ordered action/observation evidence in a VM. |
| AI-04 Third-party/vision fallback | AI-03 | Demonstrate accessible third-party navigation, one deliberately inaccessible fixture requiring screenshot/vision fallback, misidentification rejection and safe failure. Report app coverage; no universal-compatibility claims. |
| AI-05 Speech and model choices | AI-03 | Opt-in local text/model baseline; separately verify offline operation, voice activation/transcription if implemented, resource limits, data handling and remote-model consent if offered. |

Nothing in this section may be marked COMPLETE without implementation and task-specific evidence. These milestones are an **optional later roadmap**, not new blocking gates for existing SLOPOS-I release scope.

---

## 21.7 Cross-generation vision: SLOPOS as a coherent desktop platform

**Status: FUTURE FEATURE GOALS / NOT IMPLEMENTED**, except for individually audited foundations elsewhere in this contract. This section captures long-term aspirations, not new deliverables or hidden release gates for SLOPOS-I. Keep §3's SLOPOS-I scope and §21.5's milestones authoritative: **Rust + X11/Openbox + Platinum Classic** remain settled for the present generation. Do not start a Wayland compositor, PARAMA port, Loom port or model feature merely because this roadmap exists. Evaluate ambitious work as independently scoped, explicitly approved follow-on milestones, without derailing the required UI-first migration.

The long-term product aim is one cohesive SLOPOS experience across session, window management, native applications, creative tools, accessibility, on-device intelligence and eventually an independent operating-system host. Keep the SLOPOS, [Loom](https://github.com/palaashatri/rust-loom) and [PARAMA](https://github.com/palaashatri/parama) repositories separate: each has its own architectural contract, release matrix and evidence ledger. Cross-project integration means versioned interfaces, not an unsupported claim of compatibility.

### 21.7.1 Shared native UI toolkit and consistent, selectable design systems

Continue building the SLOPOS-owned **`slopos-ui` + `slopos-appkit`** foundation into a genuine desktop component and application framework: shared geometry, widgets, typography, icons, controls, accessibility, menus, window chrome, dialogs, document/action lifecycle and consistent input behavior. The toolkit owns first-party app UI; the platform backend owns presentation/windowing; service providers own real OS state. No separate app-local style implementations.

**Two coherent first-party visual modes, one shared interaction/platform API:**

1. **SLOPOS Classic — System 7 / Platinum Classic.** Keep the current SLOPOS-I **Platinum Classic** contract and approved clean-room Classic Macintosh reference authoritative for **this generation**. Retain System 7–Mac OS 9-inspired compact controls, menus, careful geometry, keyboard/focus interaction and traditional window affordances. Never bundle unlicensed Apple assets.
2. **SLOPOS Modern — Sinty/Singularity-inspired design direction.** The primary reference for a future **modern SLOPOS theme** is [Singularity Desktop at sinty.dev](https://sinty.dev/), especially its [desktop tour](https://sinty.dev/docs/desktop-tour/), [appearance](https://sinty.dev/docs/appearance/), [theming](https://sinty.dev/docs/theming/) and [under-the-hood](https://sinty.dev/docs/under-the-hood/) guidance (as examined 2026-10-25). Interpret it into **original SLOPOS visual specifications, tokens, components and assets** rather than lifting CSS, icons, trademarks or window designs without license review. Design direction: calm graphite/light neutral surfaces; purposeful surface layering; compact consistent sidebar/window geometry; subtle separators; legible type; restrained accent colors; dark/light appearance; configurable dock, classic top menu bar **or** combined taskbar layout; keyboard-first unified search; coherent Control Panels, notifications, window controls, launcher and first-party applications. Avoid gratuitous glass, blur, shadows, motion or excessively spacious web-style chrome. Interaction quality and accessibility outrank pixel resemblance.
3. **Loom design-language bridge, not a third conflicting toolkit.** The Loom Design Bible continues to govern Loom itself. On SLOPOS, map its shared semantic tokens and chrome where feasible so Loom Writer/Sheets/Present feel integrated under the selected Classic or Modern appearance. An independently approved Loom-flavored variation is possible later; **Sinty/Singularity supersedes Loom as the primary external inspiration for SLOPOS Modern**, without changing Loom's native Rust + Slint identity.

**Architecture and theme contract:**

- Define versioned semantic **`ThemeTokens`** for surfaces, backgrounds, outlines, elevation, text, typography, density, spacing, radius, icon metrics, focus, selection, validation colors, accent, light/dark/high contrast, reduced motion, motion durations and window chrome. Tokens and shared widgets—not app-local conditionals—control the native UI.
- The shell, Dock, menu bar, global search, notifications, lock/login presentation, Settings, native app windows and dialogs must consume the same selected mode. Test coherent live theme switching across participating native applications without restarting or changing document data, action IDs, shortcuts, accessibility or saved settings.
- Offer user-customizable accents, dark/light, scalable typography, compact spacing, window-control placement, menu/Dock arrangement and optional wallpaper-based accent, with persistent settings and sensible defaults. Keep the visual system modular enough for future platform backends and GPU compositor effects, but provide a non-accelerated/no-blur fallback.
- Third-party GTK/Qt/Slint/Electron/Wine applications can receive native theme/portal adapters where truly supported, **not a guarantee of identical appearance**. No silent recoloring of document artwork. Preserve toolkit independence and permit applications to opt out when semantic consistency or accessibility would be harmed.
- **One UI system across two hosts:** the target theme vocabulary and first-party interaction semantics are shared between Linux SLOPOS and potential future PARAMA-native SLOPOS. Renderer, system services, compositor and security adapters are per-platform. PARAMA cannot be called integrated until it genuinely renders the same component states in protected user space.

**Future acceptance:** approved clean-room style-spec and reusable gallery; representative first-party Files, Calculator, Settings and shell surfaces in both modes at 1×/2× and mixed DPI, normal/focused/disabled/error states, dark/light/contrast and reduced motion; keyboard and accessibility parity; live theme switching without relaunch or lost data; appkit/Loom bridges accurately constrained; measured responsiveness and compositor-independent behavior. Mark `FUT-UI` **PLANNED / NOT VERIFIED** until these gates have actual evidence.

### 21.7.2 Loom creative suite as an integrated application family

Treat Loom as the preferred creative suite accessible from SLOPOS, **not as a fork or compulsory rewrite**. Loom keeps its own Rust + Slint, cross-platform builds, original visual identity and document engines; SLOPOS owns the shell and desktop services.

- Initial integration targets Loom **Writer, Sheets and Present**. Loom **Photo, Motion, Video, Studio and Encode** remain later Loom-scoped goals; never infer they are shipping or accepted.
- Create a versioned SLOPOS ↔ Loom desktop-adapter contract for application launch and file associations, native file open/save and URI handoff, clipboard and drag/drop, global menus/commands, notifications, sharing, accessibility and consistent application lifecycle. Reuse Loom's existing adapters where actual behavior is verified.
- Apply the selected SLOPOS appearance with an explicit **theme-token and chrome adapter** wherever Loom supports it; retain Loom's cross-platform default design on other systems. If future exact visual parity requires a port to SLOPOS UI, approve and prove that migration separately—**do not** undermine Loom's Slint architecture by fiat.
- Expose permissioned document actions for SLOPOS Intelligence (§21.6) and on-device Vision (§21.7.3) instead of unnecessarily screen-scraping or copying sensitive document contents.

**Acceptance direction:** graphical Linux VM demonstration of Writer/Sheets/Present launch, real menu action invocation, clean Save/Reopen, native file picker/clipboard handoff and selection-theme consistency. Record unsupported menu/toolkit scenarios truthfully. A documentation adapter design is not an integrated application.

### 21.7.3 SLOPOS Vision: on-device ML capabilities

Provide local-first, opt-in media and text intelligence inspired by useful **Apple Vision / Live Text / Visual Look Up-style workflows**, using original code and properly licensed model packages. Target reusable services callable by SLOPOS Files, Screenshot & Recorder, Image Viewer, Notes, Loom (through adapters) and the AI desktop operator. Do **not** claim Apple feature/model parity without a tested capability matrix.

- **Live Text:** local OCR and text selection from screenshots, images, scans and supported video frames; copy/search with visible confidence, language and rotation behavior; optional local translation adapters.
- **Copy Subject:** foreground segmentation, user-correctable preview and transparent PNG/clipboard export; retain the original document and report uncertain masks.
- **Visual Look Up:** opt-in local visual categorization/recognition with calibrated uncertainty, provenance and clear offline limitations; any web retrieval is separately consented.
- **Dictation and writing tools:** offline speech recognition, optional TTS, selection-scoped proofreading/rewrite/summarization and accessible review of changes. No silent uploads or destructive text replacement.
- **Inference infrastructure:** versioned model provider ABI, integrity/license/provenance checks, CPU fallback, optional GPU/accelerator inference, bounded resource use, permission-scoped screen/document access, graceful unavailable states and honest performance measurements. Local inference must function without an account or mandatory remote service.
- **Responsibility boundary:** Vision analyzes permitted data; §21.6 SLOPOS Intelligence plans/acts. Neither directly bypasses OS privileges or grabs arbitrary user secrets.

**Acceptance direction:** offline VM image/OCR and subject-cutout fixtures with ground truth and false-positive cases, usable selection/copy/alpha export, model-license audit, privacy/no-network checks, latency/CPU and optional GPU measurements, and real multi-app handoff.

### 21.7.4 Future SLOPOS-owned Wayland compositor and advanced display stack

After the X11/Openbox desktop reaches a defensible quality bar, investigate a **SLOPOS-owned compositor** with display-server/window-manager authority rather than depending indefinitely on Openbox. This is a **new-generation feasibility gate**, not permission to change SLOPOS-I. Evaluate **Rust/Smithay** against an explicitly designed **C/C++** alternative for protocol completeness, isolation, maintenance, resource use and testability before fixing the implementation language.

- Planned protocol/service coverage: Wayland xdg-shell, appropriate layer surfaces, XWayland interoperability, clipboard/drag-drop, portals, screenshot/recording and permissioned input/remote-desktop facilities, focus/modal constraints, global menu/action bridges, workspaces/Spaces, animated transitions, multi-monitor hotplug, varied DPI/fractional scaling and resilience to compositor restart.
- Planned rendering/output goals: HDR and wide color gamut with correct color management/ICC and tone mapping, VRR/adaptive sync, frame pacing, fullscreen/direct scanout when valid, responsive animations, high refresh rates and tested GPU/driver fallbacks. Do not assert HDR/VRR from protocol build or VM capture; require real hardware/driver capability and user-visible validation.
- **Live wallpapers** are a first-class future desktop capability, subject to fullscreen pause, battery/resource throttling, reduced motion, lock-screen handling, multi-output support and performance verification.
- Keep shared SLOPOS UI controls, appkit actions, desktop intelligence and provider APIs window-system agnostic; supply isolated X11 and future Wayland adapters rather than embedding input injection or compositor details into application logic.
- Preserve desired macOS-like *usability* traits through original implementations: global app menu, configurable window-control meanings, accessible Dock with dodge/hide, screenshot/recording, consistent Finder-like/Explorer-like navigation policies and integrated native apps.

**Acceptance direction:** cold boot and restart to a **real** Wayland session, compositing/surface/input/focus tests, XWayland and modal/clipboard coverage, mixed-DPI/display hotplug and real hardware HDR/VRR demonstrations. SLOPOS-I's X11 status is unchanged until independently verified migration gates exist.

### 21.7.5 SLOPOS is the intended desktop environment for PARAMA once PARAMA can host it

The independent [PARAMA](https://github.com/palaashatri/parama) project aims for a capability-based OS with **BRAHMA** kernel, **GARUDA** supervision, user-space drivers and **SURYA** graphical service. **Architecture decision for the future full desktop:** SLOPOS is the intended PARAMA desktop environment, rather than growing a second, unrelated production desktop UI in PARAMA. This is **DESIGNATED FUTURE TARGET / NOT IMPLEMENTED** and does not change PARAMA's near-term independent M7 graphical acceptance.

**The critical distinction:** SURYA owns composition, surfaces and input routing; SLOPOS owns shell, design system, Dock, menu bar, launch/search, Settings, notifications, appkit and desktop interaction. PARAMA M7 may use a minimal throwaway/bootstrap shell to prove two surfaces, focus, input and compositor restart; it is **not** a rival polished desktop or proof that SLOPOS has been ported.

- On **Linux now**, SLOPOS-I remains Rust + X11/Openbox/GTK-backed foundations; its planned future Wayland compositor is an optional Linux generation requiring separate §21.7.4 gates.
- On **PARAMA later**, use a portable `PlatformBackend` for lifecycle/processes, capabilities, IPC, file I/O, windows/surfaces, input, clipboard, fonts/text, storage, network, audio and authorization. No Linux syscall/GTK/Slint/Wine assumptions should leak into the PARAMA ABI; runtime/toolkit portability must be demonstrated explicitly.
- **Exactly one compositor:** default target is SLOPOS shell/toolkit as an ordinary capability-restricted SURYA client; optionally integrate SLOPOS composition technology into SURYA *after a reviewed ownership/ABI migration*. Never run separate independent SLOPOS and SURYA compositors. Lock, capture, AI desktop automation and visual ML need revocable security permissions from PARAMA.
- Both intended SLOPOS modes—**Classic System 7/Platinum Classic** and **Sinty/Singularity-inspired SLOPOS Modern**—must use the same original theme contracts on both host platforms, with explicit backend-specific fallback rules. Loom uses its independent Slint implementation and joins only through verified adapters.
- PARAMA must first prove M3–M7: ring-3 isolation, capability/IPC enforcement, user ELF loading/runtime, graphics/input, GARUDA/SURYA restart and basic app lifecycle. Durable files, relevant drivers and real app/runtime compatibility are additional requirements for daily use. Until then PARAMA offers no SLOPOS integration or ordinary Linux-app support.

**Acceptance:** a separate PARAMA QEMU/VirtualBox image boots an independent ring-3 SLOPOS first-party UI sample, shows two theme modes and handles focus/keyboard/pointer, exercises denied capabilities and service restarts, and proves bounded actual native Files/Settings action only when supporting services exist. Record comparable UI token states and performance on Linux/PARAMA without claiming parity from screenshots alone. Do not make this a SLOPOS-I or PARAMA M0–M10 release blocker.

### 21.7.6 Future platform backlog and sequencing

The above initiatives form a **coordinated long-term product vision**, not a single giant implementation milestone. First make the present desktop truthful and usable, then add cross-app integration and ML, and pursue compositor/PARAMA experiments in independently guarded tracks. Useful additional goals retained from prior product discussions include full native app consistency, modern global search, on-device text/vision flows, live wallpapers, reliable screenshot/recording, customizable window buttons, robust gaming/fullscreen behavior, HDR/VRR, multi-display ergonomics and eventually privacy-conscious device/Android interoperability. Each must receive an explicit scope, dependency list, test fixtures and current evidence before any readiness claim.

| Future ID | Area | Prerequisite and proposed verification |
|---|---|---|
| FUT-UI | Classic + Sinty-inspired Modern design systems | `slopos-ui`, appkit, accessible apps; original token specs, native cross-app live theme switching, high-contrast/keyboard conformance |
| FUT-LOOM | Loom desktop adapters | Existing Loom native Linux builds and SLOPOS services; real Writer/Sheets/Present workflow in a graphical Linux VM |
| FUT-VISION | Offline vision/ML system | Permissioned capture, model provider, image/document handoff; OCR, subject copy and local-inference QA with privacy checks |
| FUT-COMP | Own compositor + graphics | Proven X11 baseline, dedicated compositor design/VM/hardware gate; working Wayland session, XWayland, HDR/VRR on capable displays |
| FUT-PARAMA | SLOPOS as intended PARAMA desktop | Independent PARAMA Ring 3, capabilities, drivers, SURYA and loader milestones; capability-scoped SLOPOS shell/toolkit client on SURYA, with Classic/Modern UI proofs in QEMU/VirtualBox |

Do not promote **FUT-*** into SLOPOS-I's required §37 acceptance or source-code task backlog automatically. Review the scope and platform compatibility with the owner and independently record evidence before scheduling any feature implementation.

---

# 22. System providers and truthful state

An enabled control must execute the behavior it advertises.

A stateful control must display real state.

Providers are the single owners of system state. Menu-bar applets, OSDs and Control Panels observe the same provider rather than implementing separate backends.

## 22.1 Network

Use NetworkManager APIs/D-Bus.

Support real:

- Ethernet adapters;
- Wi-Fi adapters;
- Wi-Fi enabled state;
- AP enumeration;
- SSID;
- signal;
- security;
- active connection;
- IP information;
- connect/disconnect;
- errors;
- VPN entries where supported.

No synthetic production networks.

## 22.2 Audio

Use the active PipeWire/WirePlumber/PulseAudio-compatible APIs.

Support:

- outputs;
- default output;
- volume;
- mute;
- inputs;
- default input;
- microphone level;
- microphone mute;
- device hotplug.

Read back changes.

The Volume applet, volume OSD, Sound Control Panel and media keys all use this provider.

## 22.3 Brightness

Provide one brightness abstraction over the correct platform facility, such as backlight sysfs/logind-compatible mechanisms, with explicit capability detection.

Support:

- current brightness;
- min/max or normalized range;
- increment/decrement;
- direct set where supported;
- read-back;
- unavailable state.

The Brightness applet, brightness OSD, Display/Power settings and brightness keys all use this provider.

Do not fabricate brightness support on displays without a controllable backlight.

## 22.4 Bluetooth

Use BlueZ D-Bus.

Support:

- adapter availability;
- power;
- discovery;
- device list;
- pairing;
- connect/disconnect;
- trusted/paired state where exposed.

The applet and Control Panel share this provider.

## 22.5 Power

Use UPower/logind.

Support:

- battery status;
- charge;
- power source;
- suspend capability;
- lid behavior where supported;
- critical/low-battery events;
- power profiles only when provider support is real.

## 22.6 Media

Provide a shared media-session abstraction over supported mechanisms such as MPRIS.

Expose:

- current player;
- title/artist where available;
- play/pause;
- next/previous;
- playback state.

The menu-bar media applet and media keys use this provider.

## 22.7 Date/time

Prefer `org.freedesktop.timedate1`.

Support:

- current timezone;
- current NTP state;
- real date/time;
- authorized changes;
- permission errors.

The menu-bar clock and Control Panel use shared time/locale state where applicable.

## 22.8 Displays

Use XRandR/X11 state.

Support:

- real outputs;
- modes;
- current mode;
- layout;
- primary display;
- rotation where supported;
- apply;
- automatic rollback/confirmation for risky changes;
- hotplug.

## 22.9 Printers

Use CUPS/IPP.

Support:

- printer discovery;
- queue status;
- default printer;
- jobs;
- add/remove only when correctly authorized.

## 22.10 Removable media

Use GIO/UDisks2 as appropriate.

Support:

- mount;
- unmount;
- eject;
- errors;
- volume labels;
- safe removal.

The Files app, removable-media applet and notifications share this state.

## 22.11 Input and hardware keys

Provide a single capability/routing layer for:

- media keys;
- brightness keys;
- keyboard backlight;
- Caps/Num state where surfaced;
- touchpad toggles where supported;
- layout/input-source state.

Global shortcut handling must not be duplicated across apps.

---

# 23. Window management and X11

Openbox provides ICCCM/EWMH window management.

SLOPOS integration must support:

- overlapping windows;
- predictable focus;
- drag and resize;
- minimize;
- maximize with accessible title-bar controls and correct menu/Dock work area;
- restore to the previous usable geometry;
- fullscreen as a distinct state from maximize;
- transient/modal relationships;
- Alt+Tab;
- workspaces;
- top-bar reserved area;
- multi-monitor placement;
- dynamic monitor changes;
- integer HiDPI.

If Openbox prevents a required behavior:

1. reproduce the limitation;
2. document it;
3. test alternatives;
4. change architecture only with evidence.

Never lower the UI specification merely to match Openbox limitations.

---

# 24. Multi-monitor and scaling

Maintain an explicit monitor model.

Define which monitor owns:

- primary menu bar;
- launcher;
- notifications;
- desktop objects;
- new first-party dialogs.

Respond to RandR topology changes without restarting the session.

Normal panels and the Dock must yield to fullscreen applications according to policy. Transient hardware OSDs may overlay fullscreen only as specified in §14.4, without focus theft.

Canonical geometry is 1×.

Support integer scaling predictably.

---

# 25. Accessibility

Production quality requires accessibility parity as a core feature.

At minimum:

- AT-SPI names/roles/states;
- keyboard-only navigation;
- logical focus order;
- visible focus;
- screen-reader-compatible labels;
- accessible collection selection;
- disabled-state semantics;
- high-contrast mode only if fully specified;
- scalable text policy without layout corruption.

Accessibility must be tested in the UI library and again in application workflows.

---

# 26. Application interoperability

SLOPOS must remain a good Linux citizen.

Support:

- `.desktop` entries;
- XDG MIME applications;
- default applications;
- XDG user directories;
- XDG autostart;
- clipboard;
- drag/drop;
- notifications;
- X11 ICCCM/EWMH;
- standard D-Bus services;
- portals only where useful on X11 and supported.

Third-party apps must remain usable even when they cannot adopt SLOPOS global menus or styling.

---

# 27. Visual acceptance

Required current composed scenes include:

- UI Gallery component sheets;
- empty desktop;
- menu open;
- Files;
- Control Panels;
- Terminal;
- Notes;
- Calculator;
- System Information/About;
- launcher;
- notification;
- modal alert;
- Software;
- System Monitor;
- representative upstream GTK application;
- representative ordinary X11 application.

Required resolutions:

- 800×600 canonical composition;
- 1280×800;
- 1920×1080;
- 3440×1440;
- supported integer HiDPI.

The 800×600 canonical scene must be produced by real components, not a mockup.

---

# 28. Mandatory VM-only execution policy

This applies to every local human and autonomous agent. An agent running in a container/cloud shell must establish that its underlying execution environment satisfies this policy; `uname` reporting Linux is insufficient.

## 28.1 Native host restrictions

The host may read/edit source and documentation, perform Git operations and non-executing text/diff reviews, manage the hypervisor, copy artifacts and check host disk space. It must not compile or execute SLOPOS code, run repository scripts, Cargo formatting/lint/tests, build packages/media, run X11/visual QA, or use native-host Docker/Podman as a VM substitute.

All project validation runs in the guest. A docs-only change may receive static text/link/consistency review on the host; that is not project runtime validation. Do not install system dependencies on the host to get around missing VM access.

## 28.2 Guest preflight

Use UTM, QEMU/KVM (including a verified remote VM), VirtualBox or VMware. Ubuntu LTS is the default general development guest; Debian/Arch guests provide distribution-specific acceptance. Record the exact tested release and architecture instead of “latest Ubuntu.”

Before any project execution the coordinator records:

- hypervisor/VM identity, guest OS/kernel/architecture and how they were established;
- whether the agent is directly in the VM or in a container inside it;
- guest-local repository/worktree/output locations and available CPU/RAM;
- host backing-volume free space and guest free space (§29);
- test account, X11 graphical session/recovery console and scheduled runtime lease.

Inside the guest, OS-only checks such as `systemd-detect-virt --vm`, `cat /etc/os-release`, `uname -m`, `df -h`, `free -h` and `nproc` help document the environment. Detection can be incomplete inside containers; use hypervisor/provisioning evidence, not a fabricated success override.

For remote infrastructure where physical-host storage telemetry is unavailable, record that limitation and the provider's virtual-disk quota/free space plus any administrator-supplied backing-storage assurance. Without adequate storage assurance, heavy build/media work remains blocked; source editing and task preparation can continue. Do not invent a host-space number or block every independent task.

Use guest-local storage for source builds, target directories, package caches and QA scratch. Do not build through a host-shared directory. Set up dependencies once under coordinator ownership, from the current distro manifests and CI prerequisites; do not run `sudo ./install.sh` in every worktree.

## 28.3 Graphical and hardware QA

Primary visual acceptance comes from the real graphical X11 session in the Linux VM. Xvfb and containers inside the VM are secondary deterministic layers. Record them as such.

Every visible UI change needs the affected component states and composed scene, plus real interaction assertions. Screenshot generation alone is not visual approval. A desktop running in Xvfb does not prove display-manager login, graphics performance, lock security or physical-device behavior.

Use virtual devices and isolated service fixtures for reproducible failure tests. Validate real hardware claims with relevant device access/passthrough or a separately authorized hardware matrix; do not substitute “unavailable passed” for a required connect/brightness/suspend test. Keep unsupported capabilities honestly unavailable.

## 28.4 Hosted CI

Hosted CI supplements guest evidence and may build release artifacts. It does not authorize local host execution or replace graphical VM journeys. Record the actual checkout SHA and CI run URL; distinguish a PR head from a tested merge result.

# 29. Disk-space and resource safety

Before heavy work record host backing-volume space, guest filesystem space, repository/worktree sizes, target/cache sizes, QA retention and the planned output reservation.

| Work | Minimum free host space | Minimum free guest space |
|---|---|---|
| Routine build/test | 20 GiB | 12 GiB |
| Package/ISO/image/installed-VM generation | 30 GiB | 25 GiB |

These are **floors**, not per-agent allowances. Schedule simultaneous workers against aggregate target/package/image/snapshot growth plus those safety margins. Measure a representative build before increasing concurrency; do not give every worker the same remaining 12 GiB. Lower `CARGO_BUILD_JOBS` or serialize builds if memory pressure/OOM occurs; do not mask OOM as a product failure or success.

Prefer one maintained development VM, sparse virtual disks, bounded evidence retention and a dedicated guest-local build volume. Snapshots consume host storage and must be included in the budget. Recheck headroom before each heavy phase.

Only the coordinator cleans known obsolete SLOPOS-owned guest targets/artifacts after confirming no active owner or needed evidence. Never delete other projects, unrelated VM images, unmerged worktrees or caches currently in use. If required space is unavailable, stop that heavy action and record a precise blocker.

# 30. Verification commands and evidence

## 30.1 Existing baseline commands

Run these **inside the verified guest**, from the selected worktree after the coordinator has installed dependencies and granted a build permit:

```bash
git status --short
git rev-parse HEAD
rustc -Vv
cargo -V
cargo metadata --locked --format-version 1 --no-deps
cargo fmt --all -- --check
cargo build --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The baseline workspace has `slopos-session`, `slopos-shell`, `slopos-settings` and `slopos-catalogue`. A focused worker check may use, for example:

```bash
cargo test -p slopos-shell --locked
cargo clippy -p slopos-shell --all-targets --locked -- -D warnings
```

Use the new package's actual registered name only after its manifest lands. Scope edits/formatting to owned files; the coordinator runs workspace checks at integration. Pure/unit tests may be parallelized when isolated. Any test requiring X11, D-Bus service ownership or system state must follow the runtime lease.

At the audited revision `rust-toolchain.toml` tracks `stable`, not a fixed version. Record `rustc -Vv`; BOOT-01 must establish a tested pin for reproducible builds. `Cargo.lock` changes require deliberate coordinator review and a fresh locked build.

## 30.2 Existing QA is evidence infrastructure, not the new gate implementation

| Existing entry point | What inspection establishes | Execution constraint/remaining gap |
|---|---|---|
| `scripts/run-release-qa.sh` | Runs Cargo and selected legacy suites | Conditional `-x` checks can omit suites; PASS means selected commands passed. Not the complete §37 gate. |
| `scripts/run-canonical-visual-qa.sh` | Captures legacy desktop/app scenes with Xvfb | Shared output path, fixed default display, name-based signals/cleanup; no atom-state conformance or real graphical-VM pass. |
| `scripts/test-session-gui.sh` | Drives legacy session-menu dialogs | Fixed `:95`, global process-name operations and hard-coded release paths; use only exclusive disposable sessions until repaired. |
| `scripts/run-atspi-qa.sh`, `run-settings-service-qa.sh`, `run-appmenu-qa.sh` | Existing scoped QA entry points | Inspect each script's prerequisites/side effects first; passing legacy expectations does not establish native UI/provider parity. |
| `scripts/run-resolution-qa.sh`, `run-multimonitor-qa.sh` | Existing display-test entry points | Coordinate X server/system-state ownership; extend for new menu/Dock/OSD and required resolutions. |
| `packaging/vm/`, package/installed-VM workflows | Existing release/guest tooling | Budget storage, inspect mutating steps, pin source, and verify actual install/boot/upgrade behavior. |

Do not run all of these automatically for a leaf change. Select tests for the changed behavior and dependencies; full candidate gates belong to INT-01.

At baseline there is no `cargo xtask`, component conformance CLI or VM lease manager. QA-01 must implement the required harness before documenting commands as usable. Keep existing scripts working during migration.

## 30.3 Required acceptance gates

| Gate | Required proof |
|---|---|
| G-BUILD | Formatting, locked workspace build/checks, Clippy, meaningful unit/integration tests and changed-script syntax. |
| G-ARCH | Dependency/UI boundaries, bounded migration exceptions, unique provider/menu/shortcut ownership, no production fixture state. |
| G-ATOM | Approved spec and gallery states; independent geometry/state/hit-testing/accessibility assertions; 1× and supported integer scale. |
| G-PROVIDER | Real API and isolated fixture cases: absent/denied/failure/cancel/reconnect/hotplug; changes read back; UI remains responsive. |
| G-X11 | ICCCM/EWMH/window/work-area/focus/fullscreen/monitor behavior; observe both `_NET_ACTIVE_WINDOW` and input focus for OSD. |
| G-A11Y | AT-SPI roles/names/states, keyboard-only workflows and usable focus on components and apps. |
| G-VISUAL | Reviewed graphical VM scenes at §27 resolutions with spec, font and scale provenance. |
| G-JOURNEY | Applicable §31 workflows including error/recovery paths; output artifacts/system effects verified. |
| G-RELEASE | Candidate source/artifact checksums; clean install, upgrade, remove, boot/session and recovery in the declared distro/architecture matrix. |

A task packet selects applicable gates with reasons. The integration/candidate gate covers every required product gate. Missing harnesses are `NOT_RUN/BLOCKED`, never silently skipped. Required-script absence or a failed assertion must make the gate fail; retries preserve the original failure log. Best-effort cleanup may tolerate an already-exited child, but required assertions must not use `|| true`.

Record baseline defects separately from regressions. A pre-existing failure still blocks the relevant completion claim; it is not permission to weaken tests or expand an unrelated worker's scope.

## 30.4 Artifact and evidence ownership

Use unique guest-local output such as `artifacts/qa/<source-sha>/<task-id>/<run-id>/` with machine-readable metadata, logs, screenshots and results. Retain source/spec/golden/toolchain provenance and hashes. Do not overwrite a prior run's evidence.

Existing scripts that require fixed output locations run serially, then their output is archived under a unique run path before the next run. QA-01 should remove that limitation. Do not copy old screenshot files and change only their manifest SHA.

Large/binary raw evidence may live in retained CI/VM artifact storage with stable links/checksums. Part II remains the concise interpretation and status ledger. Review final integrated changes and repeat only gates affected by integration, shared interfaces or changed requirements.

---

# 31. Required end-to-end daily-use journeys

A production-ready desktop must pass workflows, not just unit tests.

## Session

```text
login/session start
→ desktop
→ shell healthy
→ launch apps
→ logout
→ clean shutdown
```

## Applications

```text
launcher
→ search
→ keyboard selection
→ launch
→ global menu
→ switch
→ minimize
→ maximize
→ restore
→ close
```

## Files

```text
open Files
→ navigate
→ create folder
→ create/open document
→ rename
→ copy
→ move
→ Trash
→ restore where supported
→ empty Trash
→ removable media
```

## Network

```text
Control Panels
→ real adapters
→ Wi-Fi state
→ AP list
→ connect/disconnect fixture or real network
→ read-back
```

## Audio

```text
Control Panels
→ actual output
→ volume
→ mute
→ input
→ mic level
→ read-back
```

## Displays

```text
detect monitor
→ available modes
→ apply supported mode
→ verify
→ recover/rollback
```

## Documents

```text
Notes
→ edit
→ save
→ reopen
→ modify
→ unsaved close prompt
→ recover from error
```

## Terminal

```text
launch
→ shell prompt
→ type command
→ output
→ select/copy/paste
→ search
→ exit
```

## Screenshot

```text
capture region/window/screen
→ preview/save/clipboard
→ verify output
```

## Dock

~~~text
pin application
→ launch from Dock
→ observe running indicator
→ open second window
→ activate correct app/window
→ reorder
→ unpin
→ verify persistence
→ test auto-hide/dodge without stealing focus
~~~

## Menu-bar applets

~~~text
open Wi-Fi/Bluetooth/Volume/Brightness/Battery applet
→ observe real provider state
→ perform supported action
→ verify read-back
→ deep-link to matching Control Panel
~~~

## OSD and fullscreen focus

~~~text
focus fullscreen application
→ press volume/brightness key
→ provider state changes
→ OSD appears and updates
→ fullscreen application remains _NET_ACTIVE_WINDOW
→ OSD dismisses
→ Alt+Tab list remains unchanged
~~~

## Software

~~~text
search
→ install trusted fixture
→ verify integrity
→ launch
→ uninstall
~~~

---

# 32. Release engineering

Source installation is not consumer readiness.

Release engineering must eventually prove:

- Debian/Ubuntu package;
- Arch package;
- clean install;
- upgrade;
- removal;
- X11 session registration;
- exact source provenance;
- checksums;
- bootable x86_64 media;
- boot to usable SLOPOS;
- installation from release artifact;
- post-install app launch;
- rollback/recovery where applicable.

Public package repositories may be advertised only after they actually exist and are signed and tested.

Architecture support is evidence-based.

ARM64 or RISC-V is not "supported" merely because a manifest names it.

---

# 33. No-cheating rules

Do not achieve a passing state by:

- weakening the specification;
- raising diff tolerance to hide regressions;
- deleting required tests;
- hard-coding test results into production;
- replacing a real workflow with a mock;
- screenshotting a mockup;
- using old screenshots as current evidence;
- marking UNKNOWN as PASS;
- silently disabling required behavior;
- changing goldens to match incorrect output;
- claiming an upstream limitation without evidence;
- inventing hardware/system state;
- self-awarding completion;
- adding application-specific CSS to hide missing UI-library work;
- bypassing `slopos-ui` to finish an app faster.

A required feature may be removed only through an explicit product-contract change.

---

# 34. Documentation and single-source truth

This file contains §0's execution rules, Part I's normative requirements and Part II's descriptive audit/evidence/task ledger. Root and scoped `README.md` files provide truthful usage/API orientation and link here for architecture and readiness.

Do not add competing `TRUTH.md`, `ROADMAP.md`, `PLAN.md`, `STATUS.md`, `AUDIT.md`, `DESIGN.md`, `ARCHITECTURE.md`, per-agent Markdown plans or dated hand-authored reports. If a generic agent workflow asks for one, put its plan/decision here through the coordinator. Git history is the archive; do not duplicate old ledgers.

The restriction concerns authored/tracked project documentation. Machine-readable specifications, fixtures, test outputs, PR discussion and temporary orchestration messages are not competing truth sources. Existing QA scripts may emit raw `report.md` files into ignored artifacts; those are execution output, must not become committed alternate ledgers, and cannot redefine readiness. Preserve license notices and upstream attribution even when their format differs from this policy.

Only the coordinator edits this contract during parallel work, unless ownership is explicitly delegated. Each integrated tranche updates Part II in the same PR with source SHA, findings changed, exact command results, VM identity, resource observations, links/hashes for evidence and remaining blockers. Workers supply that information in their handoff.

Keep historical results labelled with their tested revision. Apply §0.7's documentation-only evidence rule instead of demanding an impossible self-referencing commit or relabelling old evidence as fresh. Do not replace failures with scores, silence UNKNOWN fields or copy a PASS to changed code.

---

# 35. Definition of component completion

A `slopos-ui` component is complete only when:

- its spec entry exists;
- geometry is defined;
- states are defined;
- behavior is defined;
- keyboard behavior is defined;
- accessibility is defined;
- implementation exists;
- UI Gallery fixture exists;
- unit/state tests pass;
- visual conformance passes;
- HiDPI behavior passes.

"Looks correct in one screenshot" is not completion.

---

# 36. Definition of application completion

A first-party application is complete only when:

- it uses `slopos-ui`/`slopos-appkit`;
- no ad-hoc raw GTK presentation remains except approved engine adapters;
- every enabled action works;
- data shown is real;
- errors are visible;
- keyboard workflow passes;
- accessibility passes;
- application/global menu actions are real;
- persistence works;
- failure cases are tested;
- visual QA passes;
- end-to-end VM journey passes.

---

# 37. Definition of SLOPOS-I production readiness

SLOPOS-I may be described as production-ready for daily use only when all of the following are simultaneously true.

## UI platform

- Figma-derived design specification complete;
- `slopos-ui` complete for all required components;
- `slopos-ui-gallery` complete;
- atomic conformance green;
- no first-party ad-hoc toolkit UI remains.

## Application platform

- `slopos-appkit` provides common application behavior;
- global actions/menus are real;
- file dialogs, clipboard, drag/drop and MIME handling pass;
- state restoration/error handling pass;
- Polkit authentication, secure locker integration and session crash/recovery paths pass.

## Core applications

Release-critical applications pass:

- Files;
- Control Panels;
- Terminal;
- Notes;
- Calculator;
- Software;
- System Monitor;
- Screenshot/Recorder for exposed functionality;
- System Information/About;
- the minimum Image Viewer functionality required by Screenshot's native viewer handoff.

## Shell

- global menu bar;
- protocol-backed application menus;
- first-party Dock;
- system status applets;
- volume/brightness/hardware OSD;
- launcher;
- desktop;
- notifications;
- global shortcuts/media keys;
- workspaces;
- session actions;
- multi-monitor;
- fullscreen focus preservation

all pass.

## System integration

- Network;
- Audio;
- Displays;
- Bluetooth;
- Power;
- Date/Time;
- Input;
- Printers where shipped;
- Removable Media

all report real state and pass real/fixture actions.

## Quality

- fmt;
- clippy;
- tests;
- UI conformance;
- interaction tests;
- accessibility;
- VM runtime;
- visual QA;
- release-candidate packaging

all pass.

## Release

- consumer artifacts exist;
- installation is verified;
- upgrade/removal is verified;
- published documentation matches reality;
- no release-blocking defect remains.

For a **full product completion** assignment, `COMPLETE` means all required gates have current evidence for the declared distro/architecture/capability matrix. `BLOCKED` means a precise dependency prevents remaining required work, with an owner/unblock condition and independent work accounted for.

For a **bounded implementation task**, use §0.4's task states and report what was verified. A successful task does not require finishing the whole desktop. If execution is unavailable, preserve the implementation and report `IMPLEMENTED / NOT VERIFIED`; do not claim completion or loop indefinitely. A docs-only audit can be complete as documentation while SLOPOS remains incomplete.

Neither truthful unavailability nor a fixture pass proves hardware functionality. Unsupported optional capabilities may be excluded only explicitly; required features and enabled actions cannot be silently removed to make a release pass.


---

# Part II — Current Audit and Evidence Ledger

This part describes observed implementation truth. Normative targets above are not evidence.

## A. Audit identity and limits

| Field | Recorded value |
|---|---|
| Refresh | 2026-10-02 IST / 2026-10-01 UTC |
| Audited default-branch revision | `7639ccd0b89955fc2657a3d67a489822f70c0837` |
| Default-branch context | PR #14 merged: complete shell/Dock/applets/OSD contract. No open PR was returned during this audit. |
| Previous production-source audit | `a39dc523526dde0d02736ac29134c6af2cd63d3b` |
| Inspection performed | Recursive tree, workspace/README/toolchain/CI, release and visual/session QA runners, legacy network/sound panels, shell topbar, menu bridge modules, network/audio service code and source comparison. |
| Comparison scope | Base-to-current comparison contains 23 commits; no changes to Rust crate sources or themes. Documentation/reference files plus packaging/QA ledger references changed. Earlier source/style findings are retained as inherited static findings, not new runtime results. |
| New project execution | NONE. No Cargo, repository QA, package build or graphical test was run for this documentation audit. |
| Figma inspection | No fresh extraction in this audit. §5 measurements are inherited observations; a complete machine specification is still missing. |
| VM/hardware/CI pass | NOT ESTABLISHED by this audit. Existing CI configuration is not evidence that its latest run passed. |
| Product state | NOT COMPLETE; production readiness NOT PROVEN. |

The contract edit repairs planning/execution instructions. It does not implement the backlog or close the source defects below.

## B. Current source layout

At the original contract audit, the workspace contained only:

- `crates/slopos-session`;
- `crates/slopos-shell`;
- `crates/slopos-catalogue`;
- `crates/slopos-settings`.

Since the audit, bootstrap work has added partial `slopos-core`, `slopos-ui`, and `slopos-ui-gallery` packages, plus partial `qa/spec/classic/` inventory/spec files. The working tree still has no appkit/services/X11/applets target crates, native `apps/` suite, or atomic conformance tools. `rust-toolchain.toml` is now pinned to tested Rust 1.99.0 in the dirty bootstrap tree; the original baseline remains `stable`.

The donor runtime is X11/Openbox plus session/shell, legacy Settings/Catalogue and external applications such as PCManFM. It is a migration base, not proof of the target architecture.

## C. Design and visual state

| Area | Evidence/status |
|---|---|
| Canonical authority | Classic Macintosh UI Kit is adopted by the contract; no fresh extraction here. |
| `qa/spec/classic/` | PARTIAL; inventory, schema, nine button geometries, black/white values and a separate gallery QA fixture exist. Canonical source revision, full token/atom extraction, detail, behavior, font/license and conformance remain unresolved. |
| `slopos-ui` and gallery | PARTIAL bootstrap renderer and surface fixture exist; no full gallery, component inventory, conformance, AT-SPI or G-ATOM pass. |
| Legacy `qa/reference/` | Explicitly historical/non-normative; not accepted new goldens. |
| Legacy GTK styling | Inherited static FAIL: mixed rounded/shadow/card styling and approximate bar/control geometry; not driven by canonical tokens. |
| Openbox chrome | Inherited directionally classic theme; exact conformance UNKNOWN. |
| Global bar | Present statically; canonical geometry and new provider architecture NOT PROVEN. |
| PCManFM file/desktop path | Delegated baseline; first-party Files/desktop conformance NOT IMPLEMENTED. |
| Control Panels | GTK migration donor with fake/partial state; final architecture NOT IMPLEMENTED. |
| Calculator/About proof apps | MISSING as target native applications; a legacy message dialog is not the System Information app. |

No contemporary screenshots in this audit establish graphical or pixel conformance.

## D. Reconfirmed source defects

| Finding | Inspected source | Status and required remediation |
|---|---|---|
| F-NET-01: invented Settings state | `crates/slopos-settings/src/panels/network.rs` | CLOSED at `b332657` (`621cab9`): fabricated rows removed; the panel now offers only `nm-connection-editor`. Still open: show live state from `slopos-services::network` (SVC-01/02, APP-02). |
| F-NET-02: invented menu state | `crates/slopos-shell/src/topbar.rs` | ORIGINAL AUDIT FAIL: fixed in a bounded donor patch recorded in K—the fabricated interface/SSID rows are replaced by explicit unavailable detail. The separate topbar summary still uses the legacy adapter and is not provider-completeness evidence. |
| F-AUDIO-01: partial/fake Sound state | `crates/slopos-settings/src/panels/sound.rs` | FAIL: sample device/initial levels, incomplete input/device control/read-back, direct command paths. SVC-03/APP-02. |
| F-MENU-01: guessed app actions | `crates/slopos-shell/src/topbar.rs` | ORIGINAL AUDIT FAIL: `xdotool` Edit, refresh and cleanup fallbacks removed in DONOR-MENU-TRUTH-02. GTK exported application menus and guaranteed X11 window actions remain; no claim that donor fallback menus complete the target global-menu contract. |
| F-MENU-02: duplicate menu code | `crates/slopos-shell/src/gmenu.rs`, `crates/slopos-shell/src/menu/gmenu.rs` | ORIGINAL AUDIT FAIL: unused root `gmenu.rs` and duplicate module registration removed; the runtime bridge remains `menu::gmenu`, preserving GTK and KDE exporter property handling. |
| F-MENU-03: adapter truth | `crates/slopos-shell/src/menu/gmenu.rs` | Static risk: GTK/KDE property fallbacks feed GIO GMenu construction. This is not evidence of DBusMenu support; define/test adapters before advertising compatibility. |
| F-SHELL-01: mixed responsibilities | `crates/slopos-shell/src/topbar.rs` | Migration required: menus, service UI, spawning, session actions and presentation share one large file. Allocate extraction to one owner at a time. |
| F-CLAIM-01: stale production claim | `crates/slopos-shell/src/topbar.rs` About text | FAIL (re-confirmed at `b332657`): the About dialog in `topbar.rs` still says “An original, consumer-ready Linux desktop.” Replace with alpha wording. |
| F-DOCK-01: obsolete implementation assumption | `crates/slopos-shell/src/main.rs` | Contains a dockless comment; required new Dock is MISSING. Contract supersedes the comment; do not revive the old strip. |

Earlier Date/Time initialization/read-back uncertainty remains OPEN; this audit did not freshly inspect or execute that panel.

## E. Runtime and provider truth

Session supervision, bounded restart/backoff and X11/EWMH/monitor helpers are present in the inherited source baseline. Current runtime reliability is UNKNOWN until VM execution. Shared typed production providers as specified in §§6/22 are not implemented as the target architecture; partial command-based donor modules are not their completion.

Network, Sound and fallback-menu findings are release blockers. Brightness, Media/MPRIS, input-routing and applet/OSD work remain missing or unproven. Bluetooth, power, displays and other delegated integrations need fresh real-state/read-back tests.

## F. QA and parallel-execution hazards

| Finding | Static evidence | Required response |
|---|---|---|
| F-QA-01: required runner coverage (historical finding) | The original conditional-invocation defect was addressed by `qa/release-runner-inventory.json` and the fixed-template checker in `scripts/check-release-runner-inventory.py`; commit `daac08e3271d31fb7372f1e13459e391e88460a8` is an ancestor of `main` at `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`. The current runner invokes its listed suites unconditionally. | Keep the inventory, template and workflow synchronized. The full release runner, GitHub workflow and product acceptance remain NOT_RUN; this repair is not a release pass. |
| F-QA-02: capture is not conformance | `scripts/run-canonical-visual-qa.sh` launches Xvfb and captures PNGs | Add independent component/state assertions and real graphical VM review. |
| F-QA-03: cross-worker interference | Visual runner defaults to `:90`, session GUI runner uses `:95`; both use process-name operations and shared paths | Serial exclusive test sessions until QA-01 isolates display/bus/process/artifact ownership. |
| F-QA-04: target-path mismatch | QA scripts launch `./target/release/...` | Custom shared target settings are unsafe until launch paths are made explicit; use worktree-local outputs. |
| F-QA-05: compiler drift | `rust-toolchain.toml` uses `stable` | Record actual compiler; test and pin through BOOT-01. |
| F-QA-06: inherited CI expectations | `.github/workflows/ci.yml` tests current four crates/binary names and legacy UI | Update checks together with each migration cutover; do not delete them to make the new layout pass. |
| F-QA-07: absent new harnesses | Recursive tree lacks target gallery/spec/conformance tools | QA-01/UI tasks implement them; do not advertise `xtask` or unimplemented CLIs. |

The existing repository includes workspace, Xvfb/Openbox, AT-SPI, resolution, package and installed-VM infrastructure. It has not yet demonstrated the new component, provider or concurrent-agent contract.

## G. Subsystem readiness

| Subsystem | Current state |
|---|---|
| X11/Openbox/session | Present statically; runtime revalidation required |
| Legacy Application Strip | Retired; do not restore |
| Dock | MISSING |
| Global menu | Partial GTK/GIO path; duplicate/injection/protocol issues above |
| Applet framework and real system applets | MISSING as final architecture |
| OSD/fullscreen focus guarantee | MISSING |
| Single global-shortcut/media-key integration | NOT PROVEN |
| `slopos-core`/`slopos-ui` | PARTIAL bootstrap in dirty tree; current scoped VM gates pass. Full core/platform API, UI atoms, gallery and G-ATOM remain incomplete. |
| Appkit/X11/services/applets target crates | MISSING |
| Native Files and desktop | MISSING; PCManFM donor |
| Native Control Panels | MISSING; legacy Settings donor |
| Native Terminal, Notes, Calculator, System Information | MISSING |
| Software | Legacy catalogue exists; migration/policy/runtime proof required |
| System Monitor and Screenshot/Recorder | MISSING as target apps |
| Image Viewer, Archive Utility, Disks, Fonts, Help, Media, Documents | MISSING as target apps |
| Native Polkit, secure locker integration, file chooser and other §18 utilities | Missing or unverified against the final contracts; do not equate external command presence with readiness |
| Legacy launcher and notifications | Present statically; native migration/runtime verification required |
| Atomic accessibility/conformance and graphical visual acceptance | MISSING current evidence |
| Public signed APT/Pacman repository | No verified publication evidence in this audit; README says none available |
| Current candidate package/install/upgrade/remove/media evidence | MISSING |
| ARM64/RISC-V/HDR/VRR production claims | NOT ESTABLISHED |

## H. Documentation audit resolution

This amendment:

- restores explicit precedence for current user directions and separates requirements from evidence;
- adds front-loaded VM/subagent rules, worktree ownership, serialized shared-file integration and task handoff contracts;
- replaces the monolithic phase queue with dependency-ready tasks and milestone gates;
- clarifies dependency arrows, Rust source layout, provider/process ownership and native menu transport;
- preserves the full UI/application/shell inventory and records deferred capabilities without changing generation;
- names existing QA commands and hazards separately from planned harnesses;
- resolves the single-Markdown policy versus generated raw reports;
- removes the requirement that every bounded task finish the entire product, and resolves exact-SHA evidence bookkeeping;
- records the Image Viewer dependency, Software delivery scope and mandatory secure locker integration.

These are **contract corrections**, not implementation passes. Root/subproject READMEs remain orientation documents; no separate truth ledger was added.

## I. Blocking set and next actions

The active blockers are the absent spec/UI/appkit platform and conformance harness, absent required apps/shell/providers, source defects D/E, QA hazards F, and missing compliant runtime/graphical/release evidence.

Start VM-00, SPEC-01 and QA-00 as in §21; then bootstrap common interfaces and dispatch independent foundation tasks. Do not begin native application polishing while its controls are missing. Fix misleading/fabricated production state under explicitly assigned migration tasks.

External access blockers (VM/storage, Figma source, physical devices, signing/publication credentials) must name the exact missing access and impacted gate. Continue independent ready work. Do not ask for new product choices already settled in §0.1.

### 2026-10-20 next actions (post-integration audit)

In order: (1) F-PKG-01 session descriptor → `start-slopos-i`, which also clears two package jobs; (2) F-QA-08 test repair and F-CI-01 `bsdtar` install, then diagnose the Debian Live ISO job; (3) F-CLAIM-01 About text; (4) APP-02 network: read-only live state from `slopos-services::network` in the legacy panel as a truthful-state fix, then Wi-Fi join; (5) SHELL Dock on `slopos-ui`; (6) volume/brightness OSD with media keys on the existing providers; (7) first native proof app (Calculator); (8) a CI-built ISO that boots to the session in QEMU. Items 4–8 map to the §21 task IDs; the coordinator records ownership in Part II.M before work starts.

## J. Justified and unjustified claims

Justified: experimental X11 Linux desktop, existing Rust/GTK3/Openbox donor architecture and QA tooling, adopted Classic component direction, required first-party UI/app suite and full shell contract, known gaps.

Not justified: production-ready daily-use desktop, exact Figma conformance, complete native apps/providers, all-real Settings state, complete menu compatibility, all-current CI/VM gates green, live signed package repositories, or supported hardware/architectures without evidence.

## K. Evidence register

| Evidence category | Status at the documentation audit |
|---|---|
| Repository/tree/source inspection | Static audit at the SHA in A; specific findings in C–F |
| Historical source/style audit | Retained with original baseline and unchanged-source comparison; not fresh execution |
| Project compilation/test/lint | NOT_RUN — documentation/source inspection only |
| Graphical/Xvfb/component/a11y QA | NOT_RUN |
| Physical devices/performance | NOT_RUN |
| Package/install/upgrade/release | NOT_RUN |

Actual project results are added here by the integrator as implementation proceeds. Record failures and skipped gates as well as successes.

### 2026-10-02 bootstrap execution

Environment: UTM 4.7.5/QEMU-HVF VM `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, GTK 3.24.41, Pango 1.52.1, GIO 2.80.0. One build permit, `CARGO_BUILD_JOBS=2`. Clean baseline checkout: `/home/slopos/work/rust-slopos`, detached `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. Changed checkout: `/home/slopos/work/rust-slopos-bootstrap`, same base with uncommitted core/UI/manifests/spec changes; dirty-tree evidence is provisional, not an integrated/candidate commit.

| Scope | Commands and observed result | Evidence |
|---|---|---|
| QA-00 clean baseline | §30.1 locked metadata, fmt check, all-target workspace build, Clippy with `-D warnings`, workspace tests: each exits 0, 05:37:49–05:39:25 UTC | `/home/slopos/evidence/20261002-bootstrap/gates.tsv`, SHA-256 `d0064f17d27a44b7eaca2ed3ea5ef49a9372c40a58bda3c9023bda5a2f32eb13`; corresponding command logs alongside it |
| BOOT-01 first code slice | Missing API test compile exits 101 before implementation; then `cargo test -p slopos-core -p slopos-ui --locked` reports 3 core and 3 UI tests passing; focused Clippy exits 0 | `bootstrap-red.log`, `bootstrap-green.log` in the same evidence directory; green log SHA-256 `f5d372476bc552468cb2df55c17f2143aa13dcfb1d672a435edcff5c44c67b49` |
| Changed workspace and fixed toolchain | fmt, locked all-target workspace build, strict Clippy, workspace tests: each exits 0, 05:43:18–05:44:54 UTC, with toolchain pinned to 1.99.0 | `changed/gates.tsv`, SHA-256 `788384c155474a36ca820a8e546da6c11386650488f08de367706aa22172cb7b`; `changed/source-sha256.txt`, SHA-256 `e921cf45127b6718fc6cb63ededd5f0a30736de8d5260fc60141bb4c32361703`; host source/spec checksums match all listed guest files |
| SPEC-01 partial extraction | `python3 -m jsonschema -i qa/spec/classic/inventory.json qa/spec/classic/inventory.schema.json` and JSON parsing of authored specs exit 0; this validates structure, not extraction completeness/conformance | `spec-validation.log`, SHA-256 `b30ff918a3121cff5489d1a41d7247a5ad5350bcce8359a3299bbe1620995604`; required drawing/behavior/font/revision unknowns remain |
| VM-00 graphical setup | Real Xorg `:0`/VT7 at 1280×800 renders Openbox and an xterm under `slopos-qa`; SSH remains usable after stopping graphical units. A 2026-10-03 UTM tty1 probe rendered malformed repeated right-parenthesis characters instead of the requested text; Ctrl+U cleared the unsent line and Ctrl+Alt+F1 released capture. No newline or credentials were sent. `getty@tty1` remained active; keyboard recovery remains NOT_VERIFIED | `x11.txt`, `x11-journal.txt`, `vm-00-openbox.png` in the evidence directory; PNG SHA-256 `483a2f0a769a51ec94ba8621ccaa878518fab4d4693de2c879dd42d7dbb4e5d7`. Probe record `/home/slopos/evidence/20261003-bootstrap/vm00-keyboard-20261003T001628Z/keyboard-result.txt`, SHA-256 `78e4c7a319f4e2dee77ab06e4cfd8084deaa4482d7b2e4a28a277fbd49d5c5d8`. Not a SLOPOS UI/visual/journey pass |

The runtime lease was released at 05:49:15 UTC; all three owned graphical units report `MainPID=0`. xterm exits with status 15 during the explicit stop and systemd records that unit as failed; it is not a successful interactive-input check. Final guest free space is 66 GiB; each of the two separate target directories occupies 2.5 GiB. No app/service/hardware/packaging/release gates were run. No new unsafe block or dependency was added. No commits, branches or pushes were created; the unrelated `.serena/project.yml` edit remains untouched.

### 2026-10-02 surface/gallery execution

Same VM and dirty base as the bootstrap record; earlier source hashes are historical, not evidence for this revision. Coordinator implemented the initial Cairo surface/gallery boundary; Raman added narrowly owned XDG/geometry tests and independently reviewed renderer and teardown changes. No commits or integration SHA exist. The initial gallery is a geometry QA fixture, not a completed component gallery or Classic conformance result.

| Scope | Commands and observed result | Evidence |
|---|---|---|
| BOOT-01 core primitives and current workspace | `cargo test -p slopos-core --locked` exits 0: 12 passed (XDG 4, ID 4, error 3, capability 1). Final guest `cargo fmt --all -- --check`, locked workspace build, strict all-target Clippy, workspace tests and JSON parse/schema command each exit 0 at 07:09:40–07:09:41 UTC. Workspace test log: legacy catalogue 19, core 12, session 2, settings 2, shell 26, shell integration 22, reconciliation 5, UI 11 passed/2 ignored, gallery 2; doc-test suites 0. | `/home/slopos/evidence/20261002-bootstrap/core-primitives.Nzbk4K/` focused run; `/home/slopos/evidence/20261002-bootstrap/verified-gallery.9Li52a/` current full gates; `gates.tsv` SHA-256 `2c3d9029112f1949cc0abeaca149d1d1765b8fee10763c06097faca516210725`, `source.sha256` SHA-256 `1aa117d7b911f4ffcf088f65add6024a26aa0a22cafa0260e656a73eae59c83e`, `test.log` SHA-256 `fa9bf4d300868646714f5645ea923c25f63753338b892651ec90fa77d786c691`; host checks all 25 source entries against guest. |
| SPEC-01 button labels | Figma component set `73:3378`, `get_design_context`, 07:08:58 UTC: all nine style/state label specs recorded, including secondary font/offset and disabled color; current guest parses all six JSON documents; no spec schema exists for this detail file yet | `qa/spec/classic/buttons.json`; guest `/home/slopos/evidence/20261002-bootstrap/verified-gallery.9Li52a/spec.log` |
| UI-01 button-plate geometry sub-slice | Guest `cargo test -p slopos-ui -p slopos-ui-gallery --locked` exits 0 after current plate implementation; UI 12 passed/2 ignored and gallery 4 passed. Real Xorg/Openbox QA-account gallery captures at 1×/2× rendered the three styles by three states; window sizes are recorded, gallery units stopped with `MainPID=0`, Xorg/Openbox units stopped with `MainPID=0`. Screenshots are qualitative implementation captures only, not conformance. Initial runs also recorded failed directory-permission and evidence-redirection attempts; the successful retry used a private QA directory and scoped `sudo` evidence transfer. | `/home/slopos/evidence/20261002-bootstrap/button-plate-captures/`; `button-plate-1x_000.png` SHA-256 `d101bd748cf3836610d4e8249d762814665ec1de7d8f72022a933c5f5a95d82f`, `button-plate-2x.png` SHA-256 `3bf6ac76308bff55842088e54a7eb20abbdabf6e2dd1c20adfa4eef43c28463f`; QA `sha256.txt` preserves original run paths/hashes. Source remains dirty-tree and no tested commit exists. Plate has no text, interaction, focus, action or accessibility semantics; disabled border color and remaining details are provisional. Figma `get_screenshot` and `get_metadata` now both return the Starter-plan MCP call-limit error; `whoami` confirms Full seat, Starter tier, team admin. Public read-only canvas was inspected qualitatively at 800%, but its screenshot was not archived. |
| UI-02 button activation/accessibility sub-slice | Added `ButtonControl` on native `gtk::Button`, reusing the plate renderer and accepting caller-supplied Pango font; accessible PushButton name/description are configured, disabled activation is guarded, and visual state follows GTK sensitivity/active flags. Isolated `dbus-run-session` + unique `xvfb-run` exact test passed (1/1); no physical/desktop X11 UI session was available. Full workspace fmt-check, locked build, strict all-target Clippy and locked tests each exit 0; UI 19 passed/4 ignored, shell 30+28, catalogue 19, core 12, services 34/2 ignored, session 2, settings 2, reconciliation 5, gallery 4. | `/home/slopos/evidence/20261002-bootstrap/button-control-x11.0xc9gM/`; `gates.tsv` SHA-256 `0d9f80ece0677ddc78be6790b3f4ee7dfbac9d15d16e8527fcccaa9b21cea595`, `source.sha256` SHA-256 `f25abe54ef6e940a08eac033d4210eab37cfb043255de7ef86e70c12b68ac77f`, focused Xvfb test log SHA-256 `7e7a526e7300538ada4fd40cebcfc3cf5b5167e457c314dc805c5b95daaba610`. Dirty-tree at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. Focus, pointer press/release/cancel and keyboard activation were not independently asserted; gallery registration, state-specific pixel checks, exact typography and Figma conformance remain open. This is not full UI-02. |
| UI-02 mapped button dimensions | Test-first Xvfb allocation assertion exposed the GTK theme adding 20 px to a requested 80 px button (measured allocation 100 px). A component-scoped GTK style override removes theme padding/border/margin and applies variant dimensions; the mapped-widget test verifies all three outer allocations at 80×20, 88×28 and 80×16, and retains disabled-activation and accessibility assertions. Isolated test passes 1/1; complete VM workspace fmt/build/strict Clippy/tests pass (UI 19/4 ignored, shell 30+28, catalogue 19, core 12, services 34/2 ignored, session 2, settings 2, reconciliation 5, gallery 4). One earlier full-suite attempt hit `ETXTBSY` in the pre-existing Pactl timeout fixture; that test passed in isolation and a full rerun passed. | `/home/slopos/evidence/20261002-bootstrap/button-allocation-verified.GdmxE9/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `24973a4a22b8d83f3060ec8e5351632cb1d36fcd6261daa0d2481a26f9f9304b`, `button-allocation.log` SHA-256 `dea82729f18ddbf22c6c786334e36a315d1906aa18187daf5289fa711063decb`. Dirty-tree at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; private-bus shutdown emits `A connection to the bus can't be made` after success. This verifies logical widget allocation under Xvfb, not screenshot/Figma conformance. A key/focus test was attempted but Xvfb did not grant the window focus; focus, actual pointer/cancel journeys and keyboard activation remain unverified. |
| UI text-rendering foundation sub-slice | Added `RasterSurface::draw_text` using PangoCairo, caller-owned font descriptions and current integer surface scale. New raster test confirms non-white glyph pixels at scales 1 and 2; test font is generic `Sans`, not a bundled asset. `pangocairo`/`pangocairo-sys` 0.18.0 are the two new lockfile packages. Full workspace `fmt --check`, locked build, strict all-target Clippy and locked tests each exit 0 at 08:11:09 UTC; suites: catalogue 19, core 12, session 2, settings 2, shell 25, shell integration 24, reconciliation 5, UI 13 passed/2 ignored, gallery 4; doc tests 0. All 33 source hashes match the host dirty tree. | `/home/slopos/evidence/20261002-bootstrap/ui-foundation-final.GFHFCf/`; `gates.tsv` SHA-256 `2366aba5a6969f93e6e40658eef3874234d463010ac96c1ecb22b31d04dde28b`; `source.sha256` SHA-256 `8859a67cf8db206ac846cd958eba37bd0e75ae5dba6e3fd0296e7a726add827c`; `test.log` SHA-256 `55a2f4af1abbc5305e56e62a77e49604386d7672916b979c12942859c14930a6`; `spec.log` SHA-256 `db6fea42ffd88ea88225aa35435214cba8d0632da076d3ae194308ec58e7e767`. Exact Figma typography/version/license, baseline positioning, pixel goldens, UI integration and appkit text APIs remain open; generic test font is not a product font choice. Dirty-tree evidence only. |
| UI-01 Row/Column layout sub-slice | Added `Size` plus deterministic `row`/`column` placement using caller-supplied gap and checked next-child coordinates; tests independently verify negative origins, dimensions, gaps, empty children and overflow. Full workspace `cargo fmt --all -- --check`, locked build, strict all-target Clippy and locked tests each exit 0 at 08:19:30–08:19:32 UTC; UI 17 passed/2 ignored. Inventory schema parses 8 JSON documents and validates. All 35 source hash entries match host. This does not implement Grid/Overlay/Stack/SplitPane/ScrollView/GroupBox/Inset/Align/FixedCanonicalLayout, does not extract visual spacing tokens and does not complete UI-01/G-ATOM. | `/home/slopos/evidence/20261002-bootstrap/layout-foundation-final.HJHWfS/`; `gates.tsv` SHA-256 `217fd30779777df6c955a8ac5df48a8b683c825f6d56ca79fbaf2a71848b4c0d`; `source.sha256` SHA-256 `819670a8c4870632e919f9e195b9f92176ea3562760e4eaf30fd9562951b1b46`; `test.log` SHA-256 `9e1379dc423d7edd525b45e17d03685f4eccdd0b8fdad435eed61de3700a900c`; `spec.log` SHA-256 `db6fea42ffd88ea88225aa35435214cba8d0632da076d3ae194308ec58e7e767`. Dirty-tree evidence only. |
| UI-01 ATK accessibility base sub-slice | Added a checked shared helper that sets a GTK widget's ATK role/name/description and fails when no accessible object exists; gallery surface uses it. A test-first missing-helper compile failed with `E0425` as expected. Real Xorg `:0`/QA-user/private-session-bus test then reports `1 passed` for PushButton role/name/description. Workspace fmt, locked build, strict Clippy and locked tests all exit 0 at 08:25:40–08:25:42 UTC; UI suite 17 passed/3 ignored. Thirty-six source hashes match host. The runtime log also contains `A connection to the bus can't be made` during session-bus shutdown; test process exits 0. This verifies in-process ATK metadata only, not remote AT-SPI inspection or the accessibility acceptance of any actual product control; G-ATOM remains NOT_RUN. Two earlier runtime attempts are explicitly non-passes: QA could not traverse `/home/slopos` to execute the build artifact; a copied stale test binary matched zero tests. | `/home/slopos/evidence/20261002-bootstrap/accessibility-final.o6Wm3O/`; `gates.tsv` SHA-256 `b74cbc09d9b3778464098f4a336a4f6607b47f7c201e24ac12027310cbb1dde3`; `source.sha256` SHA-256 `805fcd2b62270a91cd4fdd7bc251162fe3196e2de6ed6392a1b2fa0ff723080c`; `test.log` SHA-256 `e956b9d27c62ff90a5cfb54e9a32bac381232cd9f89585be0d1955578ec67a09`; runtime `/home/slopos/evidence/20261002-bootstrap/ui-accessibility/`, test-log SHA-256 `a9da8a667cc50aebd33997094828e291599a8d0ea83715900b635aff23b4c463`, run record SHA-256 `adf2483c6a01c9c8f15670168a68b9bb3a4cfc03d5b61499c6b02b2b9774fadd`. Dirty-tree evidence only. |
| UI image/surface composition sub-slice | Added `RasterSurface::draw_surface` with scale-ratio mapping and nearest-neighbor filtering; the first test run failed to compile because the API did not exist, then the fully qualified test passed at target scales 1×/2×. A prior `--exact` run selected zero tests and is not counted. Full workspace format, locked build, strict Clippy and tests exit 0 at 08:29:27–08:29:29 UTC; UI 18 passed/3 ignored; inventory schema validates 8 documents; all 36 source hashes match host. It can compose raster surfaces but is not a complete external Image/Icon primitive, asset loader, or visual-conformance result. | `/home/slopos/evidence/20261002-bootstrap/image-composition-final.QeLbdk/`; `gates.tsv` SHA-256 `43a864f55d8eb32cc706a3bb438d83e87412ac5b93ba4fcc72e1331d4b1e2b28`; `source.sha256` SHA-256 `769809d642799c6e4a617381a9abdfa4f1eb33cd282d4bcc9b4729f90a375561`; `test.log` SHA-256 `728c932739baf7c71b971f51026c2ccda249f142605464f61b64c46c7e5fa71f`; `spec.log` SHA-256 `db6fea42ffd88ea88225aa35435214cba8d0632da076d3ae194308ec58e7e767`. Dirty-tree evidence only. |
| UI Pixbuf image rendering sub-slice | Added `RasterSurface::draw_pixbuf` over the existing GDK Cairo bridge, using logical origin, surface integer scale and nearest filter. Test first failed to compile because API was absent; exact 1×/2× pixel and transparent-exterior assertions then pass. Full workspace fmt, locked build, strict Clippy and tests exit 0 at 08:32:18–08:32:20 UTC; UI 19 passed/3 ignored; inventory schema validates 8 documents; all 36 source hashes match host. No asset loader, alt-text/widget semantics, icon-specific metrics or conformance. | `/home/slopos/evidence/20261002-bootstrap/pixbuf-image-final.INXtzt/`; `gates.tsv` SHA-256 `0dfe073c4c227c888b2936b121811eb70dc25cc1d8182f3adbb7bd828617f34f`; `source.sha256` SHA-256 `dc48919a7794d24514f2f06d5e5ad0b9ad9988ec3203d91e7420968bf588c425`; `test.log` SHA-256 `3c1a0eaa7b21527d7f46947d534f5bf201075a8cfb28a437f0d7e0db0fa7c86a`; `spec.log` SHA-256 `db6fea42ffd88ea88225aa35435214cba8d0632da076d3ae194308ec58e7e767`. Dirty-tree evidence only. |
| Current workspace | `cargo fmt --all -- --check`, `cargo build --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`: each exits 0, 06:53:50–06:53:52 UTC; new crates report core 4, UI 11, gallery 2 passing ordinary tests; UI has 2 explicitly ignored graphical tests; this predates the plate files and must not be used as verification for them | `/home/slopos/evidence/20261002-bootstrap/verified-gallery.6TOO0a/`; `gates.tsv` SHA-256 `98e096f1cd86d5ebb1d146a7bf1aa5af9902a21e236959d5e13774ed4e720890`; `test.log` SHA-256 `9f3ade21a73561c0a464e45533b7bd9b55764663d5bc87b1e315ef382fb1dc3a` |
| Tested sources | All listed host code/spec/manifest files match guest SHA-256 values after guest formatting | `source.sha256` alongside gates, SHA-256 `580791074d8be4af56d0c45b66d0fcd5777ef9af028c73afeb0a5cfcd6e17b1b`; this documentation update changes no tested source |
| Spec structure | Guest JSON parsing reports 6 documents; Draft 2020-12 inventory schema validation passes. Black/white extraction provenance and a separately labelled SLOPOS surface QA fixture are added; complete tokens/font metrics/state extraction remain unknown | `spec.log` alongside gates; `qa/spec/classic/colors.json`, `surface-gallery.json`, `manifest.json` |
| Real graphical capture | Xorg/Openbox, 1280×800, QA account/private buses, X11 backend only; client 160×90 at 1× and 320×180 at 2×. Explicit ignored pixel-check test passes once per scale against every captured pixel, independent expected rectangle geometry; this fixture contains no rendered text or bundled fonts | `gallery-final-captures/client-1x.png` SHA-256 `ca314da9b86c1aa7ce68836f4da9354812a7d7b3dd9bbf9b159d48b35bff94fb`; `client-2x.png` SHA-256 `692132aa85bbe834644d3bcb9253e4081ec3a188e8fee1653fea8843ce036992`; client/window/scene/pixel logs alongside them |
| Lifecycle | Explicit ignored `gallery_windows_are_released_after_errors_and_main_loop_exit` test passes: scale-mismatch error kind/message, timed main-loop exit, native close and XTest Escape all release owned windows. The test permits only GTK's one hidden `GtkTooltipWindow`, not an owned `GtkWindow` | `gallery-lifecycle-verified.log`, exit 0; independent reviewer found no remaining teardown/assertion defect statically, not an additional runtime pass |
| Negative checks | Wrong captured black region fails pixel check with exit 101. Temporarily removing explicit teardown makes ownership assertion fail with exit 101; mutation restored before final gates/captures. Earlier GDK simulated-key attempt timed out; actual XTest injection passes, but physical UTM keyboard passthrough remains unverified | `gallery-capture-negative.log`, `gallery-lifecycle-mutation.log`; scratch capture orchestrator's first run ends exit 5 because its trap tries stopping an already unloaded unit, despite both pixel checks passing. Trap corrected; `gallery-capture-final-rerun.log` exits 0 |
| SVC-01 NetworkManager discovery/events D-Bus fixtures | At 09:16:30 UTC, guest format check, all-target locked workspace build, strict all-target Clippy, workspace tests and inventory-schema validation each exit 0. Services suite: 8 passed. Private D-Bus fixtures cover empty-device state, device add/remove, network-enabled changes, owner loss/reacquisition, monotonic revisions, device state, Wi-Fi AP add/remove signals, active connections and IPv4/IPv6 property types. Test daemons are reaped and all service source hashes match host. System NetworkManager is inactive, so live hardware/API behavior remains NOT_RUN. | `/home/slopos/evidence/20261002-bootstrap/091630-svc01-event-final/`; `gates.tsv` SHA-256 `88defea3ba24e510f2c22800f2e9f697a20b2bbb1787ed65702a66ad3ae59854`, `source.sha256` `87592d5e8e9a10f123e75bd19271a66dd1e89375d88c2bbdeec7a4e07021df8d`, `test.log` `e739cef0f57a425cb42b006b5d7aacd4cf4a7c89c43f21f35a1759284ba15393`. Dirty-tree evidence only; permission/failure/cancellation fixture matrix, mutations/read-back and physical-device verification remain open. |
| SVC-01 error classification follow-up | At 09:24:38 UTC, guest `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, strict all-target Clippy, `cargo test --workspace --locked`, and inventory-schema validation all exit 0. Services: 9 passed; UI: 19 passed/3 ignored; gallery: 4 passed; shell integration: 24 passed; reconciliation: 5 passed. Added typed AccessDenied/FDO availability mapping and access-denial fixture assertion. Source SHA-256 for `crates/slopos-services/src/network/mod.rs` matches host and guest: `7ac981b29a7c12e289ee37012dfdeb95d4104f5e3be683e520574fe202414c66`. No new raw artifact directory was retained for this rerun; results are the live guest command output in this session. | Dirty-tree evidence only at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. SVC-01 remains open: fixture permission/failure coverage is not a complete mutation/read-back/cancellation matrix, and NetworkManager is inactive with no real adapter, so live behavior is NOT_RUN. |
| SVC-02 network-enable mutation slice | At 09:28:12 UTC, guest format check, all-target locked build, strict all-target Clippy, workspace tests and inventory-schema validation exit 0. `set_networking_enabled` calls NetworkManager `Enable(bool)`, returns only after bounded authoritative `NetworkingEnabled` read-back, and propagates fixture AccessDenied without changing the fixture state. Source SHA-256 matches host/guest: `2fce5164df53eb7cee8623b1c3a74d5f5801f7edce3a9da08b957f244a855928`. | `/home/slopos/evidence/20261002-bootstrap/svc02-network-enable-denial-092808/`; `gates.tsv` SHA-256 `45008dc20c253b6efb03b6fabbab53b60b4ec81249135dfb8dd019fff6b10dd4`, `source.sha256` SHA-256 `e878b236415218b83af87a05d58b8d8013afc0e43084fcae15bbc0ff7ef1d52e`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; no real NM service in guest. SVC-02 remains open for connect/disconnect/VPN, cancellation/timeouts in actions and broader failure/read-back tests. |
| SVC-02 deactivation slice | At 09:30:56 UTC, guest format check, all-target locked build, strict all-target Clippy, workspace tests and inventory-schema validation exit 0. Added `deactivate_connection`, validating the object path, invoking NetworkManager `DeactivateConnection`, bounding the operation and returning only after the active-connection snapshot confirms removal. The fixture covers invalid path, unknown connection failure and successful disconnect/read-back. Source SHA-256 matches host/guest: `e04c5afa2de32a8a392729e4ff581c0bd7e91f3a4450e74efc9d0dc840f2e61c`. | `/home/slopos/evidence/20261002-bootstrap/svc02-disconnect-093050/`; `gates.tsv` SHA-256 `111a4b4d3add0638d45a3cf1cf7f840671123e102b0fcfed47e7dc9d1e740b3b`, `source.sha256` SHA-256 `5f2e74894108bd6678f9b66e3eddf65c12a9ce447ad9cf9501a4f7f1c086e932`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest NetworkManager service/hardware tests NOT_RUN. SVC-02 remains open for activation/VPN, secrets/authorization, cancellation and timeout/failure coverage. |
| SVC-02 saved-profile activation slice | At 09:35:09 UTC, guest format check, all-target locked build, strict all-target Clippy, workspace tests and inventory-schema validation exit 0. Added `activate_connection` for existing profile/device/specific-object paths; the bounded call returns only after the active-connection snapshot contains the returned object. Fixture verifies arguments and activation read-back; existing deactivation fixture also verifies removal. Source SHA-256 matches host/guest: `ae18273734d5094a226279143defc93e8de3b0a7254dd0e3bd34269820dc7c91`. | `/home/slopos/evidence/20261002-bootstrap/svc02-activation-093504/`; `gates.tsv` SHA-256 `c77380220a455a970a3f3ca7929364b5bafc7e8e52b572e4635727ae6d25dcff`, `source.sha256` SHA-256 `8939137c49a7492dcf7a489ea626947c28dcf458c54ea303401eefc12a0c28e7`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; no live NetworkManager/hardware evidence. SVC-02 remains open for VPN profile/base semantics, new-network secret-agent flow, authorization behavior beyond denial, cancellation and timeout fixtures. |
| SVC-03 Pulse-compatible audio provider foundation | At 09:54:11 UTC, VM format check, all-target locked build, strict Clippy, workspace tests, inventory schema and Debian/Ubuntu ISO script syntax all exit 0. Services: 16 passed; shell unit 25, integration 24, reconciliation 5; UI 19 passed/3 ignored; gallery 4. Added typed output/input/default/volume/mute snapshots, bounded Pactl child operations, verified mutations/read-back, event subscriptions with refresh/retry, explicit missing-service/denied/failure states and test-only executable fixtures. Pactl runtime dependencies were added to Debian/Ubuntu packages. SHA-256 for four service/lock files and four packaging files matches host/guest in `source.sha256`. | `/home/slopos/evidence/20261002-bootstrap/svc03-audio-provider-final-095409/`; `gates.tsv` SHA-256 `f88b9af324458982a2c55d0bd8ec236efc9ba2e341aa93532dd30a24c11d0547`, `source.sha256` SHA-256 `14694db0e3e5940ebc79279ed654f38689f225bccd4b33a5688332a038f874da`, `audio-runtime-availability.txt` SHA-256 `6ba3f3933ac38aafdea214c1b4b0bac4d717c9414f0db25d49998b705d22ff67`. VM reports `pactl` missing, PipeWire/Pulse/WirePlumber inactive and no audio sockets: live API/device behavior NOT_RUN. Legacy shell and Settings consumers still call their own `wpctl`/Pactl paths; provider adoption/one-authority integration remains open, as do live hardware, process-timeout and reconnect tests. |
| SVC-03 shell shared-client follow-up | At 10:02 UTC, guest format check, locked all-target workspace build, strict Clippy and workspace tests all exit 0. Shell unit: 27 passed; shell integration: 24 passed; service tests: 16 passed; UI: 19 passed/3 ignored. Shell monitor and topbar audio controls now use the service provider through the shared adapter; direct shell pactl/wpctl calls and the fake initial mute state were removed. Settings remains a direct/fake consumer; live audio runtime behavior is NOT_RUN because no Pactl-compatible service/device is present in the guest. | `/home/slopos/evidence/20261002-bootstrap/svc03-shell-shared-audio-100244/`; per-gate logs and exit codes retained, all 0. `source.sha256` SHA-256 `6e5d4bd0cc706ad471d93da3b56ea5dc38e166ca7c752e0304fd3dd7af5c1b5e`. |
| SVC-03 target-removal read-back fix | At 10:07 UTC, the Pactl fixture removes `sink.two` immediately after accepting a volume command while other sinks remain. `set_output_volume` now returns `DeviceUnavailable("sink.two")` on the first authoritative read-back rather than waiting for the mutation timeout. The focused regression passes; full workspace format check, locked all-target build, strict Clippy, and locked workspace tests all exit 0. Services: 17 passed; shell unit: 27; shell integration: 24; reconciliation: 5; UI: 19 passed/3 ignored; gallery: 4. | `/home/slopos/evidence/20261002-bootstrap/svc03-audio-target-removal-100724/`; `source.sha256` SHA-256 `4a243d2c11a0a4cc5ae99b19316ebe8d4a20f462722f9e15b6a4d6c22eefff27`; `test.log` SHA-256 `c9ebf5be4883aa7c1f9dc675b4123c20d1867d4b491a6e12f6a32b41e50b6ec6`. |
| SVC-03 serialized relative-volume action | Added a provider-owned default-output adjustment that holds the mutation lock across snapshot, delta computation, write and authoritative read-back; shell media/menu volume keys now call it without a separate pre-read. Concurrent +5% fixture actions return 90% then 95%, proving neither increment is lost. Guest format check, locked all-target build, strict Clippy and locked workspace tests exit 0; services 18 passed, shell 27+24, UI 19 passed/3 ignored and gallery 4. | `/home/slopos/evidence/20261002-bootstrap/svc03-audio-serialized-adjustment-101007/`; `source.sha256` SHA-256 `3617efc724abedbc7c30c3eda9a3e17adccd0725a217f00878756613d6f77a0a`; `test.log` SHA-256 `100687c984dd3a3cee5b2e10d4b855359b2fb2d4977560c60ece153421b1fd8a`. |
| SVC-03 serialized mute-toggle action | The provider now holds the mutation lock across snapshot, default-device lookup, inverse mute selection, write and read-back. Two concurrent toggles fixture-test to muted then unmuted; the shell no longer performs the pre-read itself. Guest format check, locked all-target build, strict Clippy and locked workspace tests exit 0; services 19 passed, shell 27+24, reconciliation 5, UI 19 passed/3 ignored and gallery 4. | `/home/slopos/evidence/20261002-bootstrap/svc03-audio-serialized-mute-101229/`; `source.sha256` SHA-256 `3fb3c350ddc2d0cd83415fd47b17ac9371b33db5c8124dff9d7c31fa21d357b3`; `test.log` SHA-256 `b74f92cc4ee7f246d84373c48b0e32f1c3019413133ad94291a634fc5ab5804c`. |
| SVC-03 live Pulse-compatible null-sink check | Installed `pipewire-pulse` 1.0.5-1ubuntu3.3, `wireplumber` 0.4.17-1ubuntu4.1 and `pulseaudio-utils` 16.1 in the Ubuntu VM under an exclusive runtime lease. Created only `slopos_test_null`, confirmed Pulse protocol over PipeWire 1.0.5, then ran the opt-in provider integration test in the same SSH session (the user manager is session-scoped). Snapshot/default discovery, 37% volume read-back, mute and unmute read-back all pass. Removed the null sink, stopped services and sockets, verified all units `inactive` and no processes, then released the lease. This proves software null-sink integration only; no hardware support is claimed. | Runtime artifacts: `/home/slopos/evidence/20261002-bootstrap/audio-null-runtime/`; completed lease retained there. `provider-null-sink-test.log` SHA-256 `f159a341214c1c73f520d0cc28a5784e72bcf1b066b9050d6962b3f8fa05a32c`, `sinks.txt` SHA-256 `ab84c9ca945bfd6ed9f93709fa8b119741abbb45cabd3a5510d8f6f2717ec062`, source manifest SHA-256 `d2f1f1d532bb2831d668e1d5c32b72fc547e370eff97d5cbf176b63143f15c96`. Subsequent full gates: `/home/slopos/evidence/20261002-bootstrap/svc03-audio-null-sink-full-gates-102626/`; format/build/Clippy/tests exit 0; services 19 passed/1 ignored, shell 27+24, UI 19 passed/3 ignored, gallery 4; `test.log` SHA-256 `66c3c92c0e2a9c63d0e4e6123abeb975e9370c308431b1dda907624ad0f36201`. |
| VM audio unit quiescence follow-up | A later SSH user-manager startup auto-started the package-enabled units, so a cleanup lease remasked `pipewire.service`, `pipewire.socket`, `pipewire-pulse.service`, `pipewire-pulse.socket`, `wireplumber.service` and `filter-chain.service` for the headless QA account. A subsequent SSH login confirmed all are `masked`/`inactive` and no user audio processes remain; package files remain installed. Unmask/start only under a new exclusive audio runtime lease. | `/home/slopos/evidence/20261002-bootstrap/audio-null-runtime/quiescence-check.txt`, `quiescence-lease.completed.txt`; no active lease remains. |
| SVC-03 live null input/output round-trip | After syncing and matching the exact `audio.rs` SHA-256, the opt-in live test ran under an exclusive lease with `slopos_test_null` and a `slopos_test_input` remapped from its monitor source. It verified default sink/source discovery, output volume 37%, input level 31%, output mute/unmute and microphone mute/unmute read-back. Both test endpoints are virtual; this is API integration evidence, not physical microphone/speaker support. Services/sockets were stopped, masked for the headless QA account, process absence verified, and lease released. | `/home/slopos/evidence/20261002-bootstrap/audio-null-runtime/`; live test log SHA-256 `f92c07124f30748273a8e8a6291499fbf96b2b37d54db606ee1fb8013be82d89`, source list SHA-256 `b87d6671c4f18e642e64b83fa739ad95d99790efa6040d91b6bfd0f4db17b1de`, tested `audio.rs` SHA-256 `c8de3e4f67f5af3658245745f594e31346f2c6e852a4d54a9343707fe1149f2e`. Subsequent full gates: `/home/slopos/evidence/20261002-bootstrap/svc03-audio-null-input-full-gates-103253/`; format/build/Clippy/tests exit 0; services 19 passed/1 ignored; shell 27+24, reconciliation 5, UI 19 passed/3 ignored, gallery 4; `source.sha256` SHA-256 `56e8401156c9411169552b82b9ac213db6518ff6bf38cfe8d0be260e8a5b5828`, `test.log` SHA-256 `dc1079fb99567d2680b7def2130eb2157070c7969223e561e7b1773ea59c6628`. |
| SVC-02 enable/read-back timeout fixture | Extended the private NetworkManager fixture with an acknowledged-but-unapplied `Enable` response. The provider's bounded authoritative read-back returns `NetworkError::Timeout` and does not claim the requested state; fixture then restores normal behavior. The regression was observed failing before the fixture behavior was added and passes afterward. Full guest format check, locked all-target build, strict Clippy and workspace tests exit 0; services 19 passed (including the five-second timeout), shell 27+24, reconciliation 5, UI 19 passed/3 ignored and gallery 4. | `/home/slopos/evidence/20261002-bootstrap/svc02-enable-readback-timeout-101549/`; `source.sha256` SHA-256 `41c2b5c65e6221f63207b0a227702aa424289fe62f099ca51d1a0bfed8aae7a7`; `test.log` SHA-256 `4ebb0321fb780d3f863d3d979b4a0f0e5c6b413001efea3919d5d4f0f515137a`. |
| SVC-02 caller-cancellation fixture | Private D-Bus `Enable` fixture delays its response; the regression aborts the in-flight Tokio task and confirms the caller's join reports cancellation. The test does not claim rollback/cancellation of an operation already accepted by NetworkManager. Full guest format check, locked all-target build, strict Clippy and workspace tests exit 0; service tests 19 passed, shell 27+24, reconciliation 5, UI 19 passed/3 ignored and gallery 4. | `/home/slopos/evidence/20261002-bootstrap/svc02-enable-cancellation-101939/`; `source.sha256` SHA-256 `790a6dbe094beb519efceb254098f5223ded6cc00210efd398d391a7fa3ea267`; `test.log` SHA-256 `d12ba831cd61324134b5b9893a9711e6f1f6b35e813c38a5af43c4378d8a82f0`. |
| SVC-02 saved VPN profile discovery | Added bounded `saved_vpn_connections()` using NetworkManager Settings `ListConnections` and per-profile `GetSettings`; returns only VPN object path/ID/UUID and never calls `GetSecrets`. Private D-Bus fixture lists a VPN and Wi-Fi profile; the VPN-only result passes. Guest `cargo fmt --all -- --check`, strict all-target Clippy and full workspace tests exit 0 at 10:40 UTC (services 20 passed/1 ignored, shell 27+24, reconciliation 5, UI 19/3 ignored, gallery 4). Source SHA-256 matches host and guest: `7db5fb4d9670580de174e8d6f803b7051d81562b53ad96dafc0696c1f087abdb`. | `/home/slopos/evidence/20261002-bootstrap/svc02-saved-vpn-profiles-104035/`; `gates.log` SHA-256 `d6b9c417db3b35c004f07f7c96bc96bb983813f8d402e13961000d48507f23f1`; `source.sha256` SHA-256 `ba749c1967c14c8bcc279b6fc63584cc0cb6999e6fda379439fedb2b540381aa`. Settings secret-agent flow, VPN activation semantics, authorization beyond denial, remote cancellation and live service remain open. |
| SVC-03 Settings shared-audio consumer | Replaced placeholder device choices/levels and direct Pactl/amixer writes in Settings Sound with `AudioClient` snapshots and typed volume/mute/default actions. A regression test rejects fake devices and direct command bypasses. Provider calls run off the GTK thread; pending/error status stays visible, changes are applied only when edited, and controls/close are disabled while a bounded mutation is in flight to avoid abandoning an unresolved operation. Guest `fmt --check`, locked workspace build, strict locked Clippy and locked workspace tests exit 0 at 10:56 UTC: catalogue 19, core 12, services 20/1 ignored, session 2, settings 2, shell 27, shell integration 25, reconciliation 5, UI 19/3 ignored, gallery 4. Isolated Xvfb/private-D-Bus smoke opened the Sound panel against the absent audio service, captured its disconnected status, and verified focused Escape shutdown; AT-SPI teardown printed a bus warning after exit. No real audio device or service is claimed. Four source hashes match host and guest. | `/home/slopos/evidence/20261002-bootstrap/svc03-settings-shared-audio-reviewed-105659/`; `gates.log` SHA-256 `f256f031c508c45aaa4fe72f6948cb7f7a2de482a60e43f41dbc22e89b4ba542`, `source.sha256` SHA-256 `56490df14ee7af4e0789f331ece1b96eb18726c66b7f8b031fe9ee8891f15109`; runtime `/home/slopos/evidence/20261002-bootstrap/svc03-settings-shared-audio-reviewed-105659/runtime/`, lease SHA-256 `802ebc6d477aec7fabcbb166657da8c308b86d4224499b0f963f2fa795e92181`, empty application log, panel PNG SHA-256 `849c981aa4fb2ac1c1f102433927cbe30391a0108e9b2867ad715956b646ee13`. Settings now consumes the shared provider, but does not subscribe to hotplug/state changes; raw GTK architecture, timeout/reconnect, operation cancellation and physical hardware verification remain open. |
| SVC-03 Pactl child timeout | Added a delayed executable fixture for the shared two-second Pactl command path. It verifies a timeout result and inspects `/proc/<pid>/stat` until the direct child is stopped or reaped. Focused test passed in 2.02 s; three parallel service-suite repeats each passed 21/1 ignored. A first full-gate attempt failed this assertion without exposing the unexpected result; after adding diagnostic output, the failure did not recur in the repeats or the final full gates. The original cause is unknown, so retain this as a non-reproduced transient rather than claiming it is diagnosed. Final guest format, locked workspace build, strict locked Clippy and locked workspace tests exit 0 at 11:02 UTC: services 21/1 ignored, shell integration 25, UI 19/3 ignored, other suites match the counts in the adjacent SVC-03 entry. The child test verifies a direct executable process, not arbitrary descendant process groups. | Failed-run artifact `/home/slopos/evidence/20261002-bootstrap/svc03-audio-timeout-child-110001/`; `gates.log` SHA-256 `e2740d82d949c2f1ec580d80b7f983f4e630b4328226dc91fc52fb1ef0956253`, `source.sha256` SHA-256 `0c652df2b780840a0613330d37d7ced0ac2048723786127ef5c26e75bc2f584a`. Final passing evidence `/home/slopos/evidence/20261002-bootstrap/svc03-audio-timeout-child-final-110240/`; `gates.log` SHA-256 `bd9cd5f3988f46c2ce8b7ed57a6cc5534067d28a7f1364e60a43fe118539bdbf`, `source.sha256` SHA-256 `0e61c1cbe2898fb507001124ee666dbc80955370ad6077acf8a74d0771fe2609`; source `audio.rs` SHA-256 `a1085dcc38f715986219ba919263da535dfd164560d4e7e3c9675628eab98f03`. |
| SVC-03 Pactl watcher reconnect fixture | Updated the Pactl executable fixture to terminate its first `subscribe` process, emit `Disconnected`, then restart and publish updated volume on the next subscription. The watcher test asserts the disconnect event and waits through a stale initial snapshot until the new authoritative state (90%) arrives with a higher revision. Focused fixture test passes in 0.59 s. Final guest format, locked workspace build, strict locked Clippy and locked tests exit 0 at 11:05 UTC; services 21 passed/1 ignored, shell integration 25, UI 19 passed/3 ignored, gallery 4 and other suites match recorded counts. This is fixture-backed retry/recovery only; live Pulse service restart/hotplug remains NOT_RUN. | `/home/slopos/evidence/20261002-bootstrap/svc03-audio-timeout-reconnect-final-110552/`; `gates.log` SHA-256 `f30631137423f746091cd12164aaa0feeb04b33bb57ff8edcb90f719bc5e0da6`, `source.sha256` SHA-256 `958ce8813653ed37a59888a8f1638b5a659c947b3d577cd87130455e74d028d1`; source `audio.rs` SHA-256 `5545a51f90a3b54726b604c1c85781d700fcb0d30e0abe92348f791321234fa1`. |
| SVC-03 Settings live-provider follow-up | Settings Sound now consumes a continuous provider event stream, uses one cloned `AudioClient` instance for watch and mutations, preserves user-dirty values across provider events, refreshes device state, disables controls on unavailable service, and stops the watcher/subscription child on dialog close. Existing fixture tests cover disconnect/reconnect and changed read-back. Full guest format, locked build, strict Clippy and locked tests exit 0 at 11:13 UTC: services 21/1 ignored, shell integration 25, UI 19/3 ignored, gallery 4; other suite counts are recorded in the immediately preceding Settings row. Xvfb/private-bus smoke shows unavailable service state and confirms Escape closes the dialog and leaves no `pactl subscribe` process; AT-SPI teardown emits a bus warning. VM audio services are inactive/masked, so no live Settings mutation/device result is claimed. A prior attempt failed only because a source-string guard expected a single-line method call; the guard now checks method identifiers and the complete rerun passes. | `/home/slopos/evidence/20261002-bootstrap/svc03-settings-live-provider-reviewed-111355/`; `gates.log` SHA-256 `50a0a747b425ee53f2e48573b4b7c446af7c7c4ddd337d6f2d58a56a1bac39f8`, `source.sha256` SHA-256 `d1e87ed1dadfcec710f8889aac8ad15797966233792428a58c207f8dd97d8972`; source `sound.rs` SHA-256 `13bab2b20d2bc24ed295cff5800255f41ac7fb4f19b31755670e8dd66c3d2876`, `shell_tests.rs` SHA-256 `7919fcb42a35f20527cf0483e7a271965f942a8a900a6eb236bcea89acfcbc6a`. Runtime `/home/slopos/evidence/20261002-bootstrap/svc03-settings-live-provider-reviewed-111355/runtime/`; lease SHA-256 `7bb9cdf581b03e90bba509ee871afe32112bfe312a2e887d4bdbd14c1d8d25b2`, empty app log, PNG SHA-256 `849c981aa4fb2ac1c1f102433927cbe30391a0108e9b2867ad715956b646ee13`. |
| SVC-04 brightness provider foundation | Implemented `BrightnessClient` over actual `/sys/class/backlight` nodes with reported min/max/current, normalized percentage, explicit unsupported-hardware state, two-second bounded event stream for state/hotplug, serialized direct/relative writes via the current logind session `SetBrightness`, range validation and read-back. Four fixture tests cover no backlight, bounds/normalization, malformed device values and hotplug events/close. The guest has no backlight or power-supply entries, so only fixture behavior and truthful no-hardware availability are verified; no physical brightness or power behavior is claimed. At 11:23 UTC, guest fmt check, locked workspace build, strict all-target Clippy and locked workspace tests exit 0; services 25 passed/1 ignored, UI 19/3 ignored, gallery 4, shell integration 25, reconciliation 5, remaining suite counts are in `gates.log`. | `/home/slopos/evidence/20261002-bootstrap/svc04-brightness-112342/`; `gates.log` SHA-256 `4b77978e5446ef230684bead6c39ced4eae7bbb34ad72fa1a7a1bf51fa1cb647`, `source.sha256` SHA-256 `f9e730e24fa418ed0e4b528ffeed8e61501d27dd5160fd945804969a6b691a7e`; `brightness.rs` SHA-256 `49b552392bf490b1b58a85e2d445c64502a64c52e4c368a9abc89ac4703a42a4`. This is an implemented service foundation, not SVC-04 completion: logind writes were not runtime exercised, provider is not yet adopted by applet/OSD/settings/keys, and §22.5 Power plus low-battery/suspend remain unimplemented. |
| SVC-04 power provider and shell consumer | Added a typed UPower/logind provider for composite battery presence/percentage/state/warning/energy rate, AC-vs-battery source, lid state and `CanSuspend` capability; bounded event stream emits transition-only low/critical warnings. Shell battery status uses this provider instead of its legacy direct sysfs/UPower query, and warnings use the existing notification presenter. Seven private system-bus tests cover battery/no-battery, missing UPower, source, lid, suspend results, invalid percentage, low-warning transition and critical escalation. Final guest format, locked build, strict Clippy and workspace tests exit 0 at 11:47 UTC: catalogue 19, core 12, services 32 passed/1 ignored, session 2, settings 2, shell 28, shell integration 26, reconciliation 5, UI 19/3 ignored, gallery 4. Live check: UPower has no system-bus owner (`b false`); logind `CanSuspend` returns `challenge`; `/sys/class/backlight` and `/sys/class/power_supply` each contain zero devices. Thus no real battery/backlight or physical suspend/brightness behavior is verified. | `/home/slopos/evidence/20261002-bootstrap/svc04-power-final-120000/`; `gates.log` SHA-256 `81cf18da9a93174020f0202384de6d2f091f29c3d96a9b5fb93eb6b930c2cca1`, `source.sha256` SHA-256 `977f6cc93e1563ecba456646fa94ea9aed155e907926a04a17655d1c0ebf6ed5`. Source hashes verified in guest. Dirty-tree evidence only. |
| DONOR-TRUTH-04 Settings Network placeholder cleanup | Removed invented Ethernet link/IP/gateway values, sample SSIDs/signal/security/connected state, and inert Wi-Fi switch/Connect buttons. Panel now states live network detail is unavailable; when installed, `nm-connection-editor` is launched as a real external action and launch errors are shown in-panel. A shell integration guard rejects the fake values, inert controls, and silently discarded launch errors. Guest fmt, locked workspace build, strict Clippy and workspace tests all exit 0 at 11:55 UTC: services 32/1 ignored, shell 28, shell integration 27, reconciliation 5, UI 19/3 ignored, gallery 4, with catalogue 19/core 12/session 2/settings 2. `nm-connection-editor` is not installed in the guest, so its actual launch and graphical panel behavior are NOT_RUN. This is a truthfulness-only migration repair, not a live provider-backed Network panel. | `/home/slopos/evidence/20261002-bootstrap/settings-network-truth-115600/`; `gates.log` SHA-256 `bad7fce7a045c1a8583e04370aa655ed9b2f756a80fc9648de3ae7e9c396cd9e`, `source.sha256` SHA-256 `6405cf45625d7174b251f3f98ade4b1a5365d6f1ef591dd07590ade6d76dccb4`; source `network.rs` SHA-256 `0a80343689632ef5b06a52ecbf15139b040d64d6f9d2e64ff31d3a9cde733300`, `shell_tests.rs` SHA-256 `eaa1120beeaa25bdf0f22b5aa3b53ddb46171115e5bed2fdef720d78ac8c3c5f`. Dirty-tree evidence only. |
| SVC-03 Pactl reconnect fixture race follow-up | A subsequent full gate run exposed one Pactl fixture failure: the fixture's reconnect script truncated `output_volume` before writing `90`, and the concurrent watcher parsed the empty file. Changed the fixture to write a per-process temporary file and atomically rename it. Three complete `cargo test -p slopos-services --locked` repeats pass (32 passed/1 ignored each), followed by the final full workspace gate pass recorded above. The earlier failed `gates.log` contains an unconditional success marker from the capture command and must not be treated as a pass; `correction.txt` explicitly records that defect. | Failed run `/home/slopos/evidence/20261002-bootstrap/svc04-power-final-114400/`; `correction.txt` SHA-256 `d3117f6c37a5da8ae1ef748c0e7875869b0df9343a3e5980dcd0bda4262c10ae`. Repeat logs `/home/slopos/evidence/20261002-bootstrap/svc04-audio-fixture-race-114500/`; repeat-1 `4281d954edcd8b5d42967d8848482aab5afe3d613f67d5b164136e200533650a`, repeat-2 `9735b218ebe6e603b221b6750be8a8da9fad94d8ad1daeea2c039b5a70e88c53`, repeat-3 `e791acebb619ad7dce5b6b6fb2d4c310e4c52e7e58b4b9626286dd90800aa712`. |

The new `unsafe` block in `crates/slopos-ui/src/testing.rs` calls GTK's unsafe destruction API once on the GTK main thread after the fixture closure finishes. Default delete destruction is suppressed, callbacks hold no widget references, and the destroyed widget is never subsequently queried. This is deliberate teardown, not custom memory manipulation; lifecycle and mutation tests cover the owned-window leak. Raster/vector rendering remains safe Rust over Cairo; text rendering uses the Rust `pangocairo` 0.18.0 binding and its `pangocairo-sys` 0.18.0 dependency, both recorded in `Cargo.lock`.

The core `ApplicationId` validates Desktop Entry application names against the standard well-known D-Bus name syntax (including 255-byte maximum) while treating reverse-DNS spelling as convention; `CoreError` preserves wrapped I/O error sources, and `Availability` distinguishes all seven states required by §6.2. Primary source: [Desktop Entry Specification, application names](https://specifications.freedesktop.org/desktop-entry/latest-single/) and [D-Bus Specification, bus names](https://dbus.freedesktop.org/doc/dbus-specification.html#message-protocol-names). These shared primitives have no production consumers yet; the VM tests validate their contracts, not platform-provider integration.

Independent core review found no definite name-syntax defect. It flagged that generic `CoreError::Unavailable` must not become a reason for future providers to discard missing-service/disconnect/failure causes; domain command errors need to preserve those distinctions. No provider consumer exists yet, so that risk is open for SVC API design.

### 2026-10-02 follow-up verification and donor correction

At 07:27:38 UTC in the same dirty guest checkout, `cargo fmt --all -- --check`, `cargo build --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked` each exited 0. JSON parse/inventory-schema validation and a source-node cross-check for all 32 menu variants also passed; component-set parent frames absent from the symbol inventory are accepted only as referenced parents. All 28 source SHA entries match the host checkout. Evidence: `/home/slopos/evidence/20261002-bootstrap/verified-gallery.T1WxtJ/`; `gates.tsv` SHA-256 `9062bf3015090185106112c60e9eacf4753c1bf0d1abf4b7b7c0ccb970e03e46`, `source.sha256` `057e905de3acbcdebfc5de2b5500bf7e1ec6a3bf3d7e846c6ef4d8496c2c4f23`, `test.log` `e3cf039b23f69b728411638e3b2910421d49d1099a2348c49b03c08dc9b942bd`, `spec.log` `43f78175466ab6a3bf03a6594da2613183328c554b184cad6bad1ee679efdcdb`. This remains dirty-tree evidence, not a candidate SHA.

`DONOR-TRUTH-01`: replaced static invented `Ethernet: Connected (eth0)` and `Wi-Fi: Active (SLOPOS-Fast-5G)` detail rows with disabled `Network details unavailable`; existing Preferences action/handler remains. Locked shell tests passed (unit 26, integration 22, reconciliation 5). Actual donor menu capture under Xorg/Openbox at 1280×800 shows the unavailable row; `/home/slopos/evidence/20261002-bootstrap/donor-truth-network-menu.png` SHA-256 `78d568a391c02d169186a31212b008eb0ecebd93bd8796e43185a613bd189fb0`; before capture `1a5396980d9184a5f4a933133d95f147acb85b269f4ed2b60b1291b191066584`. Initial UI launch failed with systemd CHDIR status 200 because the QA account could not traverse the checkout; the corrected evidence-directory launch succeeded. Logs: successful shell `489d986267a8c3ae9a67631583315d6c81329adb19b971ff8c160872aa10965d`, failed attempt `67d7a1cd8f8d764882aeb5fa148905d0f1e6b6e2e03f05c8cd039928dbc80d91`. The runtime lease was released; no network/provider accuracy or Preferences-click result is claimed. The legacy `LAN` summary remains unverified; SVC-01 is open.

`SPEC-01` leaf extraction: `menus.json` contains 32 variant/source rows with geometry and metadata-backed source IDs; structural checks resolve every symbol to inventory or a referenced component-set frame. `typography.json` records two font labels/specimen measurements. Figma Starter-plan limits blocked fresh design-context/screenshot extraction for these sources; style/behavior/accessibility/source revision and exact font version/redistribution license remain unknown. Creator pages display generic CC Attribution only, which does not establish an exact font version/license grant. No fonts are bundled. No pixel or interaction conformance is claimed.

`DONOR-MENU-TRUTH-02`: removed fallback Edit menu Cut/Copy/Paste/Select All, synthetic `F5` refresh and synthetic `Ctrl+R` cleanup. These sent guessed keystrokes with `xdotool` instead of using exported app actions or guaranteed window operations. Added `fallback_menu_does_not_fake_application_edit_or_desktop_hotkeys`; it fails against baseline source (exit 101, expected `xdotool` assertion) and passes after the patch. Guest `cargo test -p slopos-shell --locked` exits 0: 26 unit, 23 integration and 5 reconciliation tests passed. Evidence `/home/slopos/evidence/20261002-bootstrap/menu-truth-073402/`; `red.log` SHA-256 `ee28239082f82099b9276d3f01cd9e230b5d3224e51016f98abec74fd62435e0`, `green.log` `9af1be5c11a7157018b372159c0df0f91374048b3d852c9b8a4784454c385407`. The target global-menu and app action contract remains incomplete.

`DONOR-MENU-BRIDGE-03`: the unused root `src/gmenu.rs` contained a second GTK-only bridge; topbar uses `src/menu/gmenu.rs`, which also preserves KDE appmenu property discovery. Removed the duplicate module/file and updated the protocol test to read the actual runtime bridge. `shell_registers_only_the_single_global_menu_bridge` fails against baseline source (exit 101) and passes after the patch. Guest `cargo test -p slopos-shell --locked` exits 0: 25 unit, 24 integration and 5 reconciliation tests passed. Evidence `/home/slopos/evidence/20261002-bootstrap/gmenu-single-073804/`; `red.log` SHA-256 `f70cad184843b4a5609030fae1dfd3307efe6cf63af1b605e296c79f1ec015f1`, `green.log` `1bf519384747429a49101e4515971fc31fbaaabc1c7b64958e82de2b736d3521`. This removes implementation duplication; it does not prove global-menu interoperability or action completeness.

Final guest verification after both donor fixes and the regression test: `cargo fmt --all -- --check`, locked workspace build, strict all-target Clippy, and locked workspace tests each exit 0 at 07:34:54–07:35:00 UTC on Rust 1.99.0/Linux ARM64. Test suites report catalogue 19, core 12, session 2, settings 2, shell 26, shell integration 23, reconciliation 5, UI 11 passed/2 ignored, gallery 2; doc tests 0. All 29 `source.sha256` entries match host sources. Guest JSON parse/schema and 32-menu-variant source cross-check pass. Evidence `/home/slopos/evidence/20261002-bootstrap/verified-gallery.xGBW6E/`; `gates.tsv` SHA-256 `41a46921cc2ec110941911cef16f496e5c0f439b0e0826700947eba35c6a114e`, `source.sha256` `5607e1a51e9be68c687e289934ae9188db5e1597c850d1646db9725300358fb3`, `test.log` `886cb9fcc2b0b946c84420cef7c7eb6a68376e23af3410aa9dc74adfceb77e49`, `spec.log` `43f78175466ab6a3bf03a6594da2613183328c554b184cad6bad1ee679efdcdb`. Dirty-tree evidence only; no integration/candidate commit exists.

Latest workspace gate after DONOR-MENU-BRIDGE-03, 07:39:07–07:39:14 UTC: formatting, locked build, strict Clippy and locked workspace tests each exit 0; shell 25, shell integration 24, reconciliation 5 pass (other suite counts unchanged from above). Guest spec checks pass; all 31 source hashes match host. Evidence `/home/slopos/evidence/20261002-bootstrap/verified-gallery.PZju9L/`; `gates.tsv` SHA-256 `a0c118fd4fbedcfb32504db53b74ac9727b67b2793e88387ee8e8054ec0723af`, `source.sha256` `8b4cbea1c57e69b1985345535eae7f83225335e629be6965c4710574e34d0c7b`, `test.log` `51c1d48bbe9d9f1a24b0c75f962d1614cf045e95e809d20962a23064d33472c3`, `spec.log` `43f78175466ab6a3bf03a6594da2613183328c554b184cad6bad1ee679efdcdb`. Dirty-tree verification remains provisional.

An initial VM `cargo fmt --all -- --check` exited 1 and printed diffs for the three worker-owned files; `cargo fmt --all` ran in the guest and the later full gate passed. The initial diff was emitted to the session console and was not separately retained as a raw artifact. The dirty tree remains uncommitted at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; these results do not verify an integrated commit.

Graphical runtime lease: 06:08:05–06:54:49 UTC, coordinator-owned Xorg/Openbox units stopped with `MainPID=0`; capture child units also reported `MainPID=0`. SSH recovery remains available. Guest has 65 GiB free; bootstrap target is 3.4 GiB, baseline target 2.5 GiB. Host copies are under `/Users/palaashatri/Virtual Machines/slopos-bootstrap/guest-evidence/`. AT-SPI registry activation is not accessibility verification. Full G-ATOM, application/provider journeys, physical devices and packaging/release gates remain NOT_RUN. Full BOOT-01 integration, remaining core/appkit contracts and the UI foundation remain incomplete; no desktop-completion claim.

| SVC-04-SUSPEND logind action slice | Added bounded `PowerClient::request_suspend()`: checks `CanSuspend`, accepts only `yes`/`challenge`, invokes `Suspend(true)` for interactive Polkit authorization, and returns typed unavailable/permission/service/timeout errors. Two private-bus tests prove challenge authorization is requested and unavailable suspend is not invoked. Guest workspace fmt/build/strict Clippy/tests all exit 0 at 12:01:55 UTC; services 34 passed/1 ignored. The VM exposes `CanSuspend=challenge`; the fixture action passed, but actual suspend was deliberately NOT_RUN to avoid suspending the active development VM. | `/home/slopos/evidence/20261002-bootstrap/power-suspend-action-1201/`; `gates.log` SHA-256 `730e6b1454823ecd2fccd568e085ebf402eb8761e3de6ac80d99d0c34849b9dd`, `source.sha256` SHA-256 `552a052706be6685ff65e61ff21b94cf48baf6bc2ab66f6e90faec0568d5f4fd`; dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. |
| SVC-04-SUSPEND shell consumer | Both Sleep menu handlers now route through `PowerClient` on a named worker thread; failures return to the GLib main context and display an error instead of silently falling back to `systemctl`/`loginctl`. Regression test first failed against the command-backed path, then passed. Full guest fmt, locked workspace build, strict Clippy and workspace tests exit 0 at 12:10:05 UTC: services 34/1 ignored, catalogue 19, core 12, session 2, settings 2, shell 28, shell integration 28, reconciliation 5, UI 19/3 ignored, gallery 4. Actual logind action and physical resume path remain NOT_RUN. | `/home/slopos/evidence/20261002-bootstrap/power-suspend-shell-1230/`; `gates.log` SHA-256 `35edbfc8be2bbae771361e1dde6ee068e9439a134d2a9264789831b6362cf920`, `source.sha256` SHA-256 `ef5d2ad41769204690dc89f175c61dcdedc3f5e6e714aa21260817e5ec88de73`; source hashes for all five paths are in that file. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. |
| SVC-04-SUSPEND capability propagation | Fixed lazy D-Bus failure handling so missing UPower no longer suppresses independent logind `CanSuspend`. `SystemStatus` carries typed capability to both Sleep items; unavailable/unknown are disabled, available/challenge enabled, and the command-presence probe is removed. Four focused regressions pass, including private-bus no-UPower snapshot, monitor propagation, menu mapping and provider routing. Full guest format/build/strict Clippy/workspace tests exit 0 at 12:21:53 UTC: services 34 passed/1 ignored, shell 30 unit/28 integration, catalogue 19, core 12, session 2, settings 2, reconciliation 5, UI 19/3 ignored, gallery 4. Live guest `NameHasOwner(UPower)=false`; `CanSuspend=challenge`, proving provider/menu state no longer depends on a battery service. Actual suspend remains NOT_RUN. | `/home/slopos/evidence/20261002-bootstrap/power-capability-menu-1217/`; `context.txt` SHA-256 `42b207ea634aabfc3b42c2046294da2b49734f5edc10df29b753af8523be93ae`, `gates.log` SHA-256 `c4b1aac20bfc3fd287fd9bff14eb20a2e38ae4bcdb546d8c6a2338f704a66614`, `source.sha256` SHA-256 `8d692d34160cbc3016bf0437094b17ca402ef6caa03098ba1b50fb460339da10`. Host/guest hashes match; dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. |
| SVC-04-SUSPEND live capability test | Executed the ignored, real-system-bus provider test on the Ubuntu guest: `PowerClient::snapshot()` succeeds with UPower absent, reports no battery/unknown source, and retains the live logind suspend capability. Test passes; `CanSuspend=challenge`. Full guest formatting, workspace build, strict Clippy and tests all exit 0 at 12:24:40 UTC; services 34 passed/2 ignored (the separate audio runtime fixture and this live-state test are ignored by default). The live test was explicitly run with `--ignored`; it reads state only and does not invoke suspend. | `/home/slopos/evidence/20261002-bootstrap/power-capability-menu-live-1225/`; `context.txt` SHA-256 `d7b03bee1cf020b86fa324887d8bf8985017ad5bd94ba3859739f7a571d849a6`, `gates.log` SHA-256 `6253c3278065c1761ab949201732c3425a78cc8e9aea376c03871a627c637d5e`, `source.sha256` SHA-256 `9e67b5f341d7ba3994599d909b7ab52656a376a4813b6b85f6c9ccb01c9488f3`; power source SHA-256 `c5b514e693857111938274ccc78faec81eca69d513ae260899b3287baea36511`. Host/guest source hashes match; actual suspend/resume remains NOT_RUN. |

### SVC-02 saved VPN activation

Added `NetworkManagerClient::activate_vpn_connection`: validates the saved VPN profile, selects a current active non-VPN default route, calls NetworkManager using VPN base-connection semantics, and confirms the read-back is both VPN-typed and matches the selected profile UUID. Private D-Bus fixtures cover invalid/unknown profiles, missing base, selecting a physical uplink despite another active VPN, and successful activation. Latest guest format check, locked workspace build, strict all-target Clippy, and workspace tests exit 0; services 34 passed/2 ignored. Evidence: `/home/slopos/evidence/20261002-bootstrap/svc02-vpn-readback.gSs4y9/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `6bd5ddfac4941baf900a048a1b71ee5119c01a92dfb07f789042f7a3ee308d8b`. Dirty-tree at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; system NetworkManager service/hardware NOT_RUN. Profile lookup uses `GetSettings` only and never requests secrets; new-network secret-agent flow and live authorization remain open.

### SVC-02 wireless radio mutation

Added `NetworkManagerClient::set_wireless_enabled(bool)`: sets NetworkManager's `WirelessEnabled` property, then returns only after bounded snapshot read-back confirms the requested state. The private D-Bus fixture verifies enable/disable events, permission denial, and a successful setter response without state change timing out instead of claiming success. Fixed availability classification for D-Bus property-setter errors wrapped by zbus. Guest `cargo fmt --all -- --check`, locked workspace build, strict all-target Clippy and workspace tests all exit 0; services 34 passed/2 ignored, shell 30+28, reconciliation 5, UI 19/4 ignored and gallery 4. Host and guest source SHA-256: `bb713f72f7e4873282c783a1320474487a79d75aac142840a6476e3085624701`.

Evidence: `/home/slopos/evidence/20261002-bootstrap/svc02-wireless-enabled.kOAfMG/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `6630ef20c80e2b6540d66f88788ffc086edf2e29244cf580cb8b1f98a29b76ef`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; real NetworkManager and radio hardware behavior NOT_RUN.

### SVC-01 Wi-Fi scan request

Added `NetworkManagerClient::request_wifi_scan(device_path)`: validates the object path and that the device is a Wi-Fi adapter, sends NetworkManager `Device.Wireless.RequestScan` with empty options, and bounds the operation. The private-bus fixture verifies the request, rejects invalid/missing/Ethernet devices, and propagates AccessDenied. Per NetworkManager's API, this reports request acceptance only; scan completion is signaled separately by `LastScan`, and resulting AP updates continue through the existing snapshot/watch path. Guest format check, locked all-target workspace build, strict Clippy and workspace tests exit 0; services 34 passed/2 ignored, catalogue 19, core 12, session 2, settings 2, shell 30, shell integration 28, reconciliation 5, UI 19/4 ignored, gallery 4. API reference: https://www.networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html.

Evidence: `/home/slopos/evidence/20261002-bootstrap/svc01-wifi-scan.F6vgcu/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `b3dfce76e8d9331bef3ee45e7d2c44968ae51bc961e23674c8ca8a5448d1396f`; host/guest source hash `e92aa6cb7e86d42e00a30fa4452ec86be0c831a1da823a71a9bddc088bfc9b00`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; live NetworkManager and physical Wi-Fi scan NOT_RUN.

### SVC-02 saved connection discovery

Added bounded `saved_connections()` returning NetworkManager profile object path, display ID, UUID and connection type for all saved profiles; it reads only `GetSettings` and exposes no secret values. `saved_vpn_connections()` now filters this shared discovery result. The private D-Bus fixture verifies VPN and Wi-Fi profile metadata; the test failed before the new API existed and passes afterward. Guest format check, locked all-target workspace build, strict Clippy and workspace tests exit 0; services 34 passed/2 ignored, catalogue 19, core 12, session 2, settings 2, shell 30, shell integration 28, reconciliation 5, UI 19/4 ignored and gallery 4. Host/guest source hash: `45ee77e8f5891a59219696f2610a9dd47dd61898fab37df7917e969de280939d`.

Evidence: `/home/slopos/evidence/20261002-bootstrap/svc02-saved-profiles.IkPjAw/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `91afb5419f08b9ff013439cd377fb2f513fb6582ee9844ea33003acc35267d95`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; saved-profile activation and live service remain separately covered/unverified as recorded above.

### BOOT-01 validated D-Bus object paths

Added `slopos_core::ipc::IpcObjectPath`, a dependency-free validated object-path identifier for IPC boundaries. It implements the D-Bus grammar: root path allowed, absolute ASCII path, nonempty slash-separated elements, and only ASCII alphanumeric/underscore characters; paths are not given an invented 255-byte limit. The test was observed failing to compile before the type existed, then passes for valid/root/long paths and missing slash, empty element, punctuation and non-ASCII rejection. Full guest format check, locked all-target workspace build, strict Clippy and workspace tests exit 0; core 13, services 34/2 ignored, catalogue 19, session 2, settings 2, shell 30+28 integration, reconciliation 5, UI 19/4 ignored, gallery 4. Source hashes match host/guest: `lib.rs` `a13b16068dbbb845fc5cbc6af312de44bb4cb12929a668322e36e8f20a5fdde6`, `ipc.rs` `188dd330d2720631a25de9d0faab6df6fcf96119ddc19ea8f76a80f84c733bf8`; the network provider source hash is `45ee77e8f5891a59219696f2610a9dd47dd61898fab37df7917e969de280939d`. D-Bus grammar reference: https://dbus.freedesktop.org/doc/dbus-specification.html#message-protocol-marshaling-object-path.

Evidence: `/home/slopos/evidence/20261002-bootstrap/core-ipc-object-path.5NZN8M/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `38639d6342f1eb8c692a3a1ecb02edbe5fd3e0f84d6f28857d14e6171f3ba790`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; this invariant is not yet consumed by every IPC caller and BOOT-01 remains incomplete.

### SVC-06 timedate provider slice

Added `TimedateClient` with typed snapshot/error data, timezone listing and validated timezone/NTP/time mutations. Timezone and NTP writes request interactive authorization and require bounded authoritative read-back; setting time uses UTC microseconds and the timedated `SetTime` method. A bounded one-second watcher emits revisioned snapshots because timedated does not emit time-change signals. Private D-Bus fixtures cover snapshots, mutation/read-back, authorization denial, unsupported NTP, invalid timezone/time, event revisions, and service loss/reacquisition. The ignored live-system test reads the guest timedated service without changing state and passes. Initial full-workspace test run encountered a non-reproduced `ETXTBSY` in the existing audio test; its isolated test passed, and the complete serial rerun passed. Rerun guest fmt, locked all-target build, strict Clippy and workspace tests exit 0; services 35 passed/3 ignored, catalogue 19, core 13, session 2, settings 2, shell 30+28 integration, reconciliation 5, UI 19/4 ignored, gallery 4. Live timedate read test: 1 passed. Source hashes: `lib.rs` `a5b4580370118bb9926a71c0b26ea239604ed84a61a778116ccf473625c0b294`, `timedate.rs` `b2ebe3eb99be4d4655d3644f67bc3fab78722cfa45f8d030ba0369a3c295546a`. API reference: https://raw.githubusercontent.com/systemd/systemd/main/man/org.freedesktop.timedate1.xml.

Evidence: `/home/slopos/evidence/20261002-bootstrap/svc06-timedate-rerun.N5oluP/`; `gates.tsv` SHA-256 `3d658b4ee5c3b57b9a30b1b1abba1e3e08beeef31a9dd48bc21bfaf89d16835c`, `source.sha256` SHA-256 `062c05af663fc6e6cd5025e9c6dd2e9b8c53109ff5756ce308f23560631488a3`. Live read-only test evidence: `/home/slopos/evidence/20261002-bootstrap/svc06-timedate-live.MdpkVf/`; `live.log` SHA-256 `170a2924cdd3fe89095462c1c04bd86dcaebc7db23ffced65ad1365bc66d8f6e`, `gates.tsv` SHA-256 `dc9a5921ed98c4c70148ede5588733865e616a0c0312b7c4380935bb4c1b000f`. A legacy Date & Time settings consumer slice is now recorded below; the shared shell Clock applet, native Control Panels migration and remaining SVC-06 domains are still open.

### SVC-06 Bluetooth adapter/device foundation

Added a BlueZ `ObjectManager` provider that returns typed adapter/device snapshots, sorted by stable object path, a bounded change watcher, adapter power/discovery actions with authoritative snapshot read-back, typed missing-service/permission/hardware errors and operation timeouts. The private D-Bus fixture verifies adapter metadata, power/discovery state changes, denied power writes and service-loss classification; orderly service-connection close removes teardown timing variance. The ignored live-state test passes and the system bus has no `org.bluez` owner in this guest; no Bluetooth hardware or real-device pairing/connect flow is verified. The original foundation-run source hashes are `bluetooth.rs` `19c2b13ab4f9b7c167b7fb217dd5089c77bc56a0612cf2dd9979cbbead3db248` and `lib.rs` `2e1c44c5a67a9585e5982d916bd4c9a815921ca484aa491b2d0e761f3fa892ec`. BlueZ API references: https://bluez.readthedocs.io/en/latest/adapter-api/ and https://bluez.readthedocs.io/en/latest/device-api/.

Evidence: `/home/slopos/evidence/20261002-bootstrap/bluetooth-provider/`; `gates.tsv` SHA-256 `77a21963fb8d7b5b912b417e37d54f5bb3a288033f804efeb46c939c3538326f`, `live.log` SHA-256 `fae96e6fd04a9cbe2039d70416d4d343df438d917f1c2e4f4c89560c1f241d59`. The later same-base verification is recorded below. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; pairing-agent UX, BlueZ signal-based hotplug, shell/settings integration and physical-device tests remain open.

### SVC-06 Bluetooth Device1 actions and object-path follow-up

Fixed BlueZ `Device1.Adapter` parsing as an object path and validated caller-supplied adapter/device paths. Added bounded `Pair`, `Connect`, and `Disconnect` calls plus the `Trusted` property setter; actions are idempotent when requested state already holds and return only after snapshot read-back confirms the new state. The private D-Bus fixture verifies pairing/connect/disconnect/trust transitions, denied trust writes, invalid and missing device paths, and unavailable-service classification. The read-only live BlueZ probe passes, but this guest has no `org.bluez` owner or attached Bluetooth hardware. `Pair()` relies on BlueZ's default agent; the required first-party pairing prompts/agent, real-device behavior, signal-based hotplug, and applet/Settings integration are NOT_RUN. At the final same-base workspace run, metadata, format, all-target build, strict Clippy, workspace tests, and `git diff --check` each exit 0; services 36 passed/4 ignored, catalogue 19, core 13, session 2, settings 4, shell 30+28 integration, reconciliation 5, UI 19/4 ignored, gallery 4. `bluetooth.rs` SHA-256 is `bfc44a2fc4e9063130e0fa8fb0a805dffb2f049ffc2de1d5c67cfea399913815`; `lib.rs` SHA-256 remains `2e1c44c5a67a9585e5982d916bd4c9a815921ca484aa491b2d0e761f3fa892ec`. BlueZ Device and Agent APIs: https://bluez.readthedocs.io/en/latest/device-api/ and https://bluez.readthedocs.io/en/latest/agent-api/.

Evidence: `/home/slopos/evidence/20261003-bootstrap/datetime-watch-verified/`; `gates.log` SHA-256 `b4534bf83ee69885c1bc348dca9704d91b5bbe58f0bf9420b803b8a8dd3cc4e6`, `source.sha256` SHA-256 `6eada5e496c1f29edad8d9f249a9af1a90ce535b8cf2b538a61d09d968b2f874`. Focused provider and live-state evidence: `/home/slopos/evidence/20261003-bootstrap/bluetooth-device-actions-final/` and `/home/slopos/evidence/20261003-bootstrap/bluetooth-device-actions-green/`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`.

### SVC-06 Bluetooth pairing cancellation follow-up

Added bounded public pairing cancellation, per-device serialization, startup retry for `CancelPairing`, timeout cleanup, and a drop guard that schedules cleanup when the caller future is dropped. BlueZ cleanup failures are surfaced for timeouts and logged for future-drop cleanup; the registration remains reserved until cleanup is confirmed, avoiding a concurrent new pairing during unresolved cancellation. The private D-Bus fixture verifies normal pairing, timeout cancellation, explicit cancellation racing BlueZ startup, caller-future drop during startup, inactive/already-completed distinctions, failed cleanup and recovery. The independent read-only review found no remaining registry-lifecycle, serialization or timeout-bound issue. Pairing-agent prompts/UI, live BlueZ service behavior, real hardware, applet/Settings integration and signal-based hotplug are NOT_RUN/open.

Final same-base dirty-tree VM gates all exit 0: three consecutive focused fixture runs (1 passed each), locked metadata, `cargo fmt --all -- --check`, all-target workspace build, strict all-target Clippy, workspace tests and `git diff --check`; untracked Bluetooth source whitespace check also passes. Workspace test output reports services 42 passed/4 ignored, catalogue 19, core 13, session 2, settings 6, shell 30 plus 28 integration, reconciliation 5, UI 19/4 ignored and gallery 4. Environment: UTM 4.7.5/QEMU-HVF VM `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, `CARGO_BUILD_JOBS=2`; 56 GiB guest disk and 5.8 GiB RAM available at preflight. The fixture is private D-Bus; no `org.bluez` owner or Bluetooth hardware is claimed.

Evidence: `/home/slopos/evidence/20261003-bootstrap/bluetooth-cancel-final-211214/`; command-bearing `gates.tsv` SHA-256 `a40666e0d133809e0612d1d9f9cffcac830adb0492860a5819c3a9461c5b0360`, `source.sha256` file SHA-256 `7571fb4a619376652d17e0a3e5d3b85376d5862bcf42984e70caa02f65dea8a`, `bluetooth.rs` SHA-256 `d90d0c6dcbd02859d240f24d3b73a1d3691f7ab83f7562b4ccbc37d85dc15c8f`, focused test log SHA-256 `ccf4cd8f7a4377f2b2f2c667ed5a39343103a3d1bee3dd41fd5a6a11051d6ec6`, workspace test log SHA-256 `65edd5512dbcde94ff606105e20ced50c0e831f3e5a4bb4e387e843831bba2a7`, and artifact manifest SHA-256 `fc4c2f0ce8790fafb3ede610305837c83531a3240efc64ed9717e939e672cf44`. Source remains uncommitted at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; this evidence verifies that dirty source snapshot, not an integrated commit.

The gate table records the executed commands, UTC intervals and exit codes. After testing, only the raw evidence manifests and this `AGENTS.md` ledger were amended; executable and specification files remained unchanged. The run's `source-files.sha256` manifest SHA-256 is `28bdb7476a4c8382ec2709d33a289cf4942fb65a669c52e969d489e12d68e089`.

### APP-02 Date & Time legacy consumer slice

Replaced hardcoded NTP/time-zone choices and ignored `timedatectl` subprocess calls in the legacy Settings Date & Time dialog with `TimedateClient`. The dialog loads the provider snapshot/time-zone list on a worker, displays provider time in the active zone, keeps controls disabled until authoritative state arrives, and applies timezone/NTP/manual time via the shared provider off the GTK thread. Failures stay visible; a load-time signal-suppression guard prevents the initial population from creating false dirty state. A focused conversion test covers UTC, Asia/Kolkata offset and invalid calendar dates. Full guest fmt/build/strict Clippy/workspace tests exit 0; a read-only live timedated test passes 1/1. A no-mutation isolated Xvfb/private-session-bus smoke confirmed the dialog maps at 520×494 and exits on Escape. Its XWD capture is retained but is not pixel/visual conformance evidence. The graphical Apply workflow, AT-SPI behavior and actual system-changing action were not exercised; GTK remains donor UI, not target `slopos-ui`/`slopos-appkit` Control Panels.

Evidence: `/home/slopos/evidence/20261003-bootstrap/datetime-panel-recheck/`; `gates.tsv` SHA-256 `4518b0e7b9989ae92a6fd5b21bbb1029570cb4bb5344d6a6de1f1d5aa0856fa0`, `source.sha256` SHA-256 `8e73f5f915bcf53951a6fdad028167b25648374da8041321abc7cd1384af1029`, `datetime.rs` SHA-256 `2a1379d5632a1d4d8e04c09409b001a01c3be08c70c0298dec463b9f79aeef31`, `test.log` SHA-256 `49450ed578af32017faae0bb43783e56b21b1e98f5e5e55e215cc931fa1461c5`, `environment.txt` SHA-256 `1ec335fa841bd6391ff18a89e6faae253a08fd9963d38490d8e7798b4a6aee87`, live read-only test `timedate-live.log` SHA-256 `17a5e715644c087769392476d748b411e7ae0e1f92b34ebbcb74c3f6fd38a67b`. Isolated Xvfb/private-D-Bus run: `dialog.xwd` SHA-256 `6394dc9fe3f66ff3de7fd2abf9fb7997cc23d409dcfea654e222cb6ef5bf39a5`. An earlier full-gate attempt at `/home/slopos/evidence/20261003-bootstrap/datetime-panel-provider/` recorded an unformatted source check and a Bluetooth fixture service-close race; guest formatting and deterministic service close were applied before this passing rerun. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`.

### APP-02 Date & Time live-state and failure-safety follow-up

The panel now subscribes to `TimedateClient::watch()` from its Tokio runtime. Live readouts follow current system time/timezone; provider changes update only non-dirty controls, preventing a stale timezone from being used when manually setting wall time. Applying disables timezone, NTP and manual controls. A failed apply leaves the form read-only with a close/reopen recovery message instead of leaving editable fields behind a disabled Apply button. Added a snapshot-timezone conversion regression test. During graphical verification an initial run exposed watcher construction outside Tokio and unsafe removal of a GLib source that had already closed; both root causes were fixed. Final isolated Xvfb/Openbox/private-session-bus smoke maps the 520×494 dialog and exits 0 on Escape; timedated live probe is read-only. Actual Apply/system-time mutation, AT-SPI, pixel conformance, and migration from donor GTK remain NOT_RUN.

Evidence: `/home/slopos/evidence/20261003-bootstrap/datetime-watch-verified/`; `gates.log` SHA-256 `b4534bf83ee69885c1bc348dca9704d91b5bbe58f0bf9420b803b8a8dd3cc4e6`, `source.sha256` SHA-256 `6eada5e496c1f29edad8d9f249a9af1a90ce535b8cf2b538a61d09d968b2f874`; current `datetime.rs` SHA-256 `1c59758548187c49ee604078355eb2a1d783f4b76d2212442a893b364bbe963d`. Successful UI evidence: `/home/slopos/evidence/20261003-bootstrap/datetime-watch-ui-retry/`; `artifacts.sha256` SHA-256 `b410e754f0183c6a003b97dae1bbbfdb764dabf7c86d396d99b3be4b4a1f1a39`, `dialog.xwd` SHA-256 `d9793c62ae7f67c0a416fb3f8714d08774dc6e175388ad45b928d286bceef977`. The failed diagnostic attempt and corrected cause are retained in `/home/slopos/evidence/20261003-bootstrap/datetime-watch-ui-openbox/ui-app.log` and `/home/slopos/evidence/20261003-bootstrap/datetime-watch-ui/attempt-1-correction.txt`. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`.

### APP-02 Date & Time fresh-state conversion follow-up

Manual wall-time Apply now reads a fresh timedated snapshot inside its worker before converting the entered local time. If the timezone was not edited, conversion uses the provider's current timezone rather than the last UI poll; if NTP became enabled externally, the stale form does not set the clock. Added regression coverage for fresh-vs-edited timezones and worker-thread conversion. The final full guest workspace gates pass, and a fresh no-mutation Xvfb/Openbox/private-session-bus smoke maps the 520×494 dialog and exits 0 on Escape. No actual system time mutation, AT-SPI check, or pixel conformance is claimed.

Evidence: `/home/slopos/evidence/20261003-bootstrap/media-mpris-final-200141/`; `gates.tsv` SHA-256 `6c13b8e2681d76699b0d9f58d8951f1d81f47e562a2c9b436a3468c5e953cffe`, `source.sha256` SHA-256 `d031005aeee8c6d5e5402b2b87ade52cd10d0b8adf278aef435419c5ae46770b`; current `datetime.rs` SHA-256 `2bbe92baf0527e89ca60cc4865ea28cadb0ef5be58335d5a5a1a1c5fa4341ddc`. Xvfb evidence: `/home/slopos/evidence/20261003-bootstrap/datetime-watch-final-195940/`; `result.txt` records `app_exit=0`, and `dialog.xwd` SHA-256 is `16ef04369a327eb0c7bd7a55bb926942175ba2ca16d207cbbeb50667c97b11dd`. A preceding artifact-packaging attempt exited 1 during directory hashing after the dialog itself exited 0; the corrected run above and the earlier attempt are both retained. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`.

### SVC-06 MPRIS media-session provider foundation

Added a session-bus `MediaClient` that discovers MPRIS players and exposes identity, current player, optional title/artist, playback state and capabilities. Bounded concurrent reads isolate per-player failures in the snapshot; capability-checked `PlayPause`, `Next`, and `Previous` calls return fresh state. Player disappearance is classified separately from session-bus loss. The polling event stream emits player/error changes and stops polling when dropped. Six focused tests, including five private-bus fixtures, cover metadata, empty/disconnected state, player failure isolation, state races, denied/unsupported actions, owner loss, watcher drop, error transitions, removal and reappearance. The VM has no external MPRIS player, so live-player behavior and shell applet/media-key integration are NOT_RUN. The provider conforms to the [MPRIS Player interface](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html); no UI integration or full SVC-06 completion is claimed.

The final guest metadata, format, all-target build, strict Clippy, workspace tests and `git diff --check` each exit 0. Workspace tests report catalogue 19, core 13, services 42 passed/4 ignored, session 2, settings 6, shell 30 plus shell integration 28, reconciliation 5, UI 19/4 ignored, and gallery 4; doc-test suites report 0 tests. UTM guest: Ubuntu 24.04.5 LTS ARM64, Rust/Cargo 1.99.0, one build permit, `CARGO_BUILD_JOBS=2`, 5.8 GiB RAM and 77 GiB disk. Source hashes: `media.rs` `033b286f7e9841d32300e97ab7e679b7b18449f2dd283b15406d75cd58516a3f`, `lib.rs` `b016301ad05783c8391d93772dda3b02a9c71fd5bc716f976b4e3b2174113c2e`.

Evidence: `/home/slopos/evidence/20261003-bootstrap/media-mpris-final-200141/`; `gates.tsv` SHA-256 `6c13b8e2681d76699b0d9f58d8951f1d81f47e562a2c9b436a3468c5e953cffe`, `source.sha256` SHA-256 `d031005aeee8c6d5e5402b2b87ade52cd10d0b8adf278aef435419c5ae46770b`; task-local and full-gate logs are retained in that directory. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`.

### UI-02-BTN-INPUT pointer and keyboard journeys

Added isolated XTest journeys to `ButtonControl`: a pointer press shows the pressed state, release inside activates, release outside cancels activation after the target receives the release event, and focused Space press/release activates. A test cleanup guard releases held input and closes the fixture window. The existing disabled-button test verifies activation suppression through GTK's synchronous `clicked()` path; no physical event dispatch to a disabled button is claimed. Independent read-only review found no concrete defects.

Three isolated runs each of the pointer-cancel and focused-Space tests, plus one disabled-activation test, pass. Each GTK test ran in its own process with private D-Bus, Xvfb and XDG directories. `cargo fmt --all -- --check`, `cargo build --package slopos-ui --all-targets --locked`, `cargo clippy --package slopos-ui --all-targets --locked -- -D warnings`, and `cargo test --package slopos-ui --locked` exit 0; the package suite reports 19 passed, 6 ignored. VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, one build permit with `CARGO_BUILD_JOBS=2`, 5.8 GiB RAM and 56 GiB free disk.

Evidence: `/home/slopos/evidence/20261003-bootstrap/ui02-button-input-final-20261002T222857Z/`; source `button.rs` SHA-256 `7f179141246dbe7a0fe649b6d4bbc8a97a5e03acf260423272783f1b3de52ce5`, `gates.tsv` SHA-256 `bfdb5187a359701fe3bba66a06d3420ff42c077861a20fdd56e83f201816e72f`, `source.sha` file SHA-256 `58cb90a7bd36156c45b3359f9ab8952e608557bf36c8dd59ab109842836f3a4f`, and `artifacts.sha256` SHA-256 `f73e6a134a676bb762acea6ce4b422f2736c5de7f850fb585325e3e535a7f389`. Dirty-snapshot evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; not an integrated-commit claim.

### SVC-07-UDISKS-DISCOVERY inventory snapshot

Added the UI-free `StorageClient` read-only UDisks2 snapshot with distinct Block and Drive collections, optional Filesystem interface state, stable object paths, Unix-byte-preserving device/mount paths, deterministic ordering, and typed transport/property errors. Private-bus fixtures cover independent and drive-only objects, absent versus unmounted filesystems, non-UTF-8 paths, the `/` Drive sentinel, malformed Block and Drive properties, empty inventories, missing service, denied access, and disconnect classification. No mount/eject/power-off operation is implemented or invoked.

On the dirty source snapshot, `cargo fmt -p slopos-services -- --check`, `cargo test --locked -p slopos-services -- --test-threads=1` (50 passed, 5 ignored), and `cargo clippy --locked -p slopos-services --all-targets -- -D warnings` each exit 0. A separate ignored live system-bus smoke test exits 0 and reads 13 blocks and 2 drives without mutation. Independent read-only review found no concrete defect. This verifies discovery against this VM's UDisks2 service and fixtures, not removable-media hardware, signal watching, actions, or Files integration.

VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, one build permit (`CARGO_BUILD_JOBS=2`). Evidence: `/home/slopos/evidence/20261003-bootstrap/svc07-udisks-discovery-coordinator-20261002T230759Z/`; final source `storage.rs` SHA-256 `f768cff084173d68549aa425b06774c79bcba7235f8761865d7c49a8f2c793c3`, `lib.rs` SHA-256 `58f011344b252672db2457ecdb3e4dd371239a335fa77cc223d5502f8beff1ee`; `fmt.log` SHA-256 `41b81b383d1e081a127f62b5b27e359aa5cd30f7118c77a2979bb93bf46fc88e`, `tests.log` `eabbd5c8f2a8e222c0d820c777244c05ff1640ac6c22bc8fd932f15c2911c7c2`, `live.log` `d3afdd698e375dee0ee7414a5dc4a6f9318d07075bd4a1149aaf940d9d1faaf2`, `clippy.log` `cb7a0325ea0591f4380aec5c5bad708504877cd81a4f00b49959362b75d66136`, and `final-gates.meta` `8094988d8bba13e18dbabbbef260d7f63e5212c98acaa8ba48e06cb15221d469`. The earlier worker's unsuccessful D-Bus attempts remain in `/home/slopos/evidence/20261003-bootstrap/svc07-udisks-discovery-tdd/`; they are not passes. Dirty-tree evidence at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, not an integrated-commit claim.

### 2026-10-03 current workspace gates

At the same dirty code snapshot, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked -- --test-threads=1`, and `git diff --check -- AGENTS.md` each exit 0. Workspace tests report catalogue 19, core 13, services 50/5 ignored, session 2, Settings 6, shell 30, shell integration 28, reconciliation 5, UI 19/6 ignored, gallery 4; doc-test suites report 0 tests. These are build/test gates only, not visual, release, application-completeness, or hardware passes. VM/toolchain and source hashes match the SVC-07 record above. Evidence: `/home/slopos/evidence/20261003-bootstrap/workspace-current-20261002T232800Z/`; `fmt.log` SHA-256 `41b81b383d1e081a127f62b5b27e359aa5cd30f7118c77a2979bb93bf46fc88e`, `build.log` `f2732fdce7e40fec45007f57ad30d2c8351abf6e136f497cece028e1346a781e`, `clippy.log` `83ab699ef06c118f20104cbe5690afca342e64bfe52f11cee011cd68ab249ee7`, `tests.log` `7920b00fb8318a3691d2908868e2a41a8e6eb4cba3f17c3cbea608ff514eb8c6`, `docs.log` `f5bd0008ed4dc49c504c760b81a66c78f0f87299ef7855a3c8c2a27e217a2f5f`, `meta.txt` `0d7701fc5bf978eed5fa0d7de781ee2c10fd32d503a0bd0218fa241d5ba6f7b2`. The `AGENTS.md` checksum in `meta.txt` predates this evidence-only ledger addition; project source hashes did not change.

### BOOT-01-UI-INSETS checked logical geometry

Added the required UI foundation `Insets` type using unsigned canonical logical-pixel edge distances. Applying insets to a `Rect` returns `None` on dimension underflow or coordinate overflow and permits a collapsed zero-size rectangle. Five focused tests cover asymmetric, zero, collapsed, oversized and coordinate-overflow cases. No design spacing values or visual conformance are claimed. An initial formatter check found one line-wrapping difference; guest `cargo fmt -p slopos-ui` corrected it before the final gate set.

At dirty source base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked -- --test-threads=1`, and `git diff --check -- AGENTS.md` each exit 0. Workspace tests report 181 passed, 0 failed and 11 ignored; doc-test suites report zero tests. Independent read-only review found no definite defect. This verifies only the inset-geometry slice; BOOT-01, UI-01 and product readiness remain incomplete.

VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust `1.99.0 (b940084d7 2026-09-28)`, Cargo `1.99.0`, one build permit at `CARGO_BUILD_JOBS=2`. Evidence: `/home/slopos/evidence/20261003-bootstrap/boot01-insets-final-20261002T235153Z/`; `insets.rs` SHA-256 `bee4de9c3e218baca283681923840869d52afd25a7f11ccc160a4b4f0b6bbbaf`, `foundation/mod.rs` `3fed76f73afe0d562a6c85125d9e248ed300f30408c8bbcd8f592776760b70cf`, `gates.tsv` `574c5289f63629e18ba60077ace4867c342e66d31e015dbb95fc1184202c739e`, and `SHA256SUMS` `ad80c5c5f3c714ff594cb2c665d3d27dc144615b7aaad3201607cf88ab3965d8`. The first-attempt formatter output remains under `/home/slopos/evidence/20261003-bootstrap/boot01-insets-20261002T235020Z/`; the final run above is authoritative. Dirty-snapshot verification is not an integration-commit claim.

### UI-01-GRID-PLACEMENT intrinsic track geometry

Added row-major `grid` placement for supplied child sizes, requiring a nonzero column count and explicit horizontal/vertical gaps. Column widths and row heights use their maximum child extents; child sizes are preserved, short rows emit no phantom cells, empty input stays empty, and position overflow uses the shared layout error. Five focused Grid tests and one shared error-message test pass. An independent read-only review found no definite defect.

At dirty source base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked -- --test-threads=1`, and `git diff --check -- AGENTS.md` each exit 0. Workspace tests report 187 passed, 0 failed and 11 ignored; doc-test suites report zero tests. This is a geometry-only layout primitive, not visual conformance or complete UI-01/BOOT-01 evidence.

VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust `1.99.0 (b940084d7 2026-09-28)`, Cargo `1.99.0`, one build permit at `CARGO_BUILD_JOBS=2`. Evidence: `/home/slopos/evidence/20261003-bootstrap/ui01-grid-placement-final-20261003T000602Z/`; `insets.rs` SHA-256 `bee4de9c3e218baca283681923840869d52afd25a7f11ccc160a4b4f0b6bbbaf`, `grid.rs` `1cc303941972fedf397188eb5d73ceb5588329934b8d3ca8a205d6a02299a6af`, `layout/mod.rs` `1ef7b00a75d62c4ea6a46f2e3095eb6c16ce58493c1bdcedf66e67b64f9110c6`, `linear.rs` `529503527052918006a9c5dce01efdd0d7e4465b16c5559eb74b9b4345323179`, `gates.tsv` `574c5289f63629e18ba60077ace4867c342e66d31e015dbb95fc1184202c739e`, and `SHA256SUMS` `bf2da09c55070580a06db637cb384ed012219abd66460cb3ac966b9be5b7f014`. Dirty-snapshot verification is not an integration-commit claim.

### UI-01-OVERLAY-GEOMETRY shared-origin placement

Added and exported `overlay(origin_x, origin_y, children)`; every child retains its dimensions and order at the supplied origin, with empty input producing no rectangles. Two focused tests cover ordering/dimensions and empty input. Independent read-only review found no definite defect.

At dirty source base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked -- --test-threads=1` each exit 0. Workspace tests report 189 passed, 0 failed and 11 ignored; doc-test suites report zero tests. These are code gates only, not visual/spec conformance; broader UI-01 and BOOT-01 remain incomplete.

VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust `1.99.0 (b940084d7 2026-09-28)`, Cargo `1.99.0`, one build permit at `CARGO_BUILD_JOBS=2`. Evidence: `/home/slopos/evidence/20261003-bootstrap/ui01-overlay-final-20261003T002156Z/`; `overlay.rs` SHA-256 `377eef8089a8a568223cc58467e188c75054b8cb2e9d278ce20f415abaf47d21`, `layout/mod.rs` `9583ace45470b7a8b6b1bb9201cb290d1230171ae19245b42bf31a2a6dcb6bad`; `gates.tsv` SHA-256 `e9c95dbea3417ff7efdce5f8a60e97a70cc2ca80b7d6c34922f1567f19f13b4c`, `build.log` `2eab2daec7a68eb3417eb7b9a723b2bcb0865c023810b2d26a8b1e6b7fad6b6b`, `clippy.log` `d85b140bfa12ccb17c19d376a31075ce8b32a58683f148193dfd9bd629b17e19`, and `tests.log` `901510f4bbe7a452f6dcce7a7a98b9747e3559a0e8f4252226215170dadcfa11`. Dirty-snapshot verification only; no commit/integration claim.

Supplemental checksums for the Overlay run: `fmt.log` SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, `source.sha256` `1ef91cf3af3bb00a45fdc6d9960e2103c724d5c80e7cc0b0ae0fc5d83c847006`, and `meta.txt` `00f18f4cdd9b057a40dcdcd184a0482b7d2467ebf91e55789629c462889c222b`.

### UI-01-ALIGN-PLACEMENT caller-controlled alignment geometry

Added the §9.5 geometry-only `align` primitive with independent horizontal/vertical Start, Center and End choices. It preserves child size, does not clip or resize oversized children, uses signed integer division toward zero for Center, and reports unrepresentable origins. Four focused tests cover start placement, odd center space, oversized end placement and overflow on both axes. Independent review found no definite defect.

At dirty source base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked -- --test-threads=1` each exit 0. Workspace tests report 193 passed, 0 failed and 11 ignored; doc-test suites report zero tests. These are code gates only; no visual metrics or Figma conformance are claimed. Broader UI-01 and BOOT-01 remain incomplete.

VM: UTM 4.7.5/QEMU-HVF `0D76A821-040A-4BA1-9B39-084D963B12ED`, Ubuntu 24.04.5 LTS ARM64, kernel `6.8.0-142-generic`, Rust `1.99.0 (b940084d7 2026-09-28)`, Cargo `1.99.0`, one build permit at `CARGO_BUILD_JOBS=2`. Evidence: `/home/slopos/evidence/20261003-bootstrap/ui01-align-final-20261003T003718Z/`; `align.rs` SHA-256 `11e93597a4d6db1db6ba096b339a02fc7f9ea157ecb54008b9b6fc004f08eca6`, `layout/mod.rs` `0a218ee2420bb7a88a734e48d48d65480ee00fd96cf932b3f25ec465adbde22d`; `gates.tsv` `e9c95dbea3417ff7efdce5f8a60e97a70cc2ca80b7d6c34922f1567f19f13b4c`, `fmt.log` `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`, `build.log` `b5d8617408728d8bffa127f8aa81249a9880434c58a5de8421e7e70baad3a74a`, `clippy.log` `2c7974c65f6ae18746c7f63fd8b932196aa01bed6c333b7b6f9f79b33e64e137`, `tests.log` `d5f151eea3a6be6879982e1209110764b036fed54dab64895c0b503e4c616555`, `source.sha256` `7f71f72bf263736e6097bbd1c2d1c2ced416155f0075bbcc548d55df606312f3`, and `meta.txt` `43e13d11f5a2866af229bb0d256042e2c85ff0f90adc880e862efd82b05a2293`. Dirty-snapshot verification only; no commit/integration claim.

### 2026-10-03 SVC-07-UDISKS-WATCH implementation slice

At dirty guest base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `crates/slopos-services/src/storage.rs` SHA-256 `4c87d99d7bb99167d1a2f2ac1646a5f843bd150d1a36eeabdcc33dc914e6a1ca`. Ubuntu 24.04 ARM64, kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, one build permit (`CARGO_BUILD_JOBS=2`). At `2026-10-03T02:14:52Z`, guest format check, locked all-target workspace build, strict Clippy and workspace tests each exit 0; full logs report 198 passed and 11 ignored, including all five private-bus watcher tests. The timeout-after-recovery regression first failed as expected on the no-retry implementation (exit 101): `/home/slopos/evidence/20261003-bootstrap/udisks-retry-red-fixed-021212/test.log`, SHA-256 `b08194c824d888e7e424db81086a3ca822f98bf7978b9af69facf7e6406eeb94`. A preliminary fixture attempt failed because Tokio's timer was unavailable on the zbus executor; the test harness was corrected before the expected-red run. An earlier full check flagged rustfmt-only differences; scoped formatting preceded the final all-green rerun. Evidence: `/home/slopos/evidence/20261003-bootstrap/svc07-watch-retry-final-021452/`; `test.log` SHA-256 `229b4d38593ab46fb6ad33a83980108aafce5ff8650e53a0482537c7706e576c`, build `49764e7d2eb2b73297d77ba6ae50f6e64de0a64910f2c965f72d51a3b9c722de`, Clippy `eb4262d97d822d6319540439ebbf012892308c58333281dc2d824f2b81f22640`, metadata `58ae72efe98cc60eec171e36d79a4b83bee19f371f5a8a12e6a0a4dd566f9389`. An initial re-review was inconclusive; Averroes' exact-SHA follow-up confirmed the retry fix and found no additional concrete defect in the reviewed watcher lifecycle/backpressure/revision scope. Fixture-only evidence; live UDisks/removable hardware and actions remain open; this is not an integration-commit verification.

### 2026-10-03 SVC-07-UDISKS-ACTIONS implementation slice

At dirty guest base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, `crates/slopos-services/src/storage.rs` SHA-256 `6a591972b08747b9cd07ad04b969e45a17c3cf90462c348b3e6ad881bd4145da`. Provider mount, unmount, eject and power-off actions validate current object/interface/capability and mounted/system/ignored state, serialize mutations across `StorageClient` clones, use bounded calls and verify state after successful replies. At `2026-10-03T03:11:29Z` in the UTM/QEMU Ubuntu 24.04.5 ARM64 guest (kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, `CARGO_BUILD_JOBS=2`), focused `cargo test -p slopos-services --locked storage_ -- --test-threads=1` exits 0 with 12 passed, 0 failed; full guest `cargo fmt --all -- --check`, `cargo build --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked` each exit 0, with 205 passed, 0 failed and 11 ignored. Evidence `/home/slopos/evidence/20261003-bootstrap/svc07-actions-final-031129/`; `gates.tsv` SHA-256 `4e4ef4827098504e7a2a3a166df51dd4e852d23b421e27c50a441d3b3b37928b`, `source.sha256` `b0a9bb6c887c8177df1fd62b0adc04d2eef058e3e6bc6aa5c86dc27668254495`, workspace `tests.log` `539f6061e8a7291e3f51c1988774fa53b9ebaae2f8a2c22f9e1063904e83f60e`; focused log `/home/slopos/evidence/20261003-bootstrap/svc07-actions-review-fix-focused-030954/test.log`, SHA-256 `bc6a566d7a8c646530c044d0ea243e8d57d81e07edf66bdc31e232551d3b71fd`. Aristotle's independent review of the exact source SHA found no definite action/watcher regression and confirmed the timeout-classification and dropped-waiter serialization fixes. Residual limitation, explicitly an inference rather than a reproduced failure: after a dispatched D-Bus method times out, the implementation makes one bounded snapshot attempt and returns `ActionOutcomeUnknown`; that local timeout does not establish that UDisks stopped processing, so a late remote effect could outlive the local mutation guard. The fixture does not rule this out; refresh authoritative state before retrying. This evidence is fixture/build verification only: no live UDisks mutation, removable-media hardware, Files integration or graphical journey was run. No integration commit is claimed.

The K entry is a documentation-only update after the recorded gates; those gates apply to the listed `storage.rs` SHA, not to this revised contract file.

### 2026-10-03 SPEC-01 requirement-catalog gate

Guest-only validation at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, Ubuntu 24.04.5 ARM64: JSON Schema validation and the catalog integrity audit passed at `2026-10-03T01:38:55Z`. The audit checked schema shape, conditional requirements, duplicate keys, unknowns, 43 exact source references and known menu crosswalk completeness. It did not establish complete Figma extraction, current source revision, licensing, implemented state, or visual conformance. Validation log: `/home/slopos/evidence/20261003-bootstrap/spec-catalog-final.G3q6z3/validation.log`, SHA-256 `fe251d3cc8e4652b5cc23057aa1493e68176322bd3ea2c4183bba1c22bf8cc5b`.

### 2026-10-04 QA-01-UI-BOUNDARY Rust source-path verification

At dirty source base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, host `main` and guest `/home/slopos/work/rust-slopos-bootstrap` have matching sorted `git status --short` SHA-256 `15582420896cf26a7cc11984c94aa5064031df957df85f8e8d9c4465bbda2d82`. The checker and test files are untracked at this base; their SHA-256 values are `6ead9cda0ee56d408e6d88bbc35c85a092e1feec23add833429177904c7cf4e1` and `42bff3d24ea88f4892c2f891e6dcf33eef29bbb0202021fbae88f1c6579463e5`, respectively. The scanner now accounts for nested inline-module path bases, normalizes paths before package-root checks (including missing intermediate components and symlinks), and fails closed on conditional `cfg_attr(..., path = ...)` rather than assuming that branch applies. A regression for the conditional escape failed before the fix; the symlink escape and nested inline-path regressions pass. Linnaeus identified the conditional-path bypass; Godel (`01a1059b-5d63-7982-b54a-6d9fb0414141`) independently reviewed the final path-resolution scope and reported no findings.

In UTM 4.7.5/QEMU-HVF Ubuntu 24.04.5 LTS ARM64 (kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, Python 3.12.3, pytest 9.1.1), the required isolated-venv command `python -m pytest -q scripts/test_check_architecture.py` exits 0 with 61 passed; log `/home/slopos/evidence/20261004-qa01-ui-boundary/pytest-required-gate-final.log`, SHA-256 `07fa11aa64a450684b94525f293b30b4469210489cff721d896f46233eaa5318`. Focused path tests report 5 passed. Python bytecode compilation, the live checker, CI workflow YAML parsing (PyYAML 6.0.3), and `git diff --check` each exit 0. A direct trailing-whitespace scan covers the untracked QA source/config files because `git diff --check` does not include untracked files. The live checker explicitly reports zero workspace app packages and that app conformance is not demonstrated. CI itself, the full Rust workspace gates, graphical journeys, and product conformance were not run by this Python-only slice.

Evidence is retained in `/home/slopos/evidence/20261004-qa01-ui-boundary/`; `final-gates-03.tsv` SHA-256 `d162017bc335bbf0697af98c107cfb73a2ca9867a69a70aa7ba4a658b6429d59`, `pytest-path-followups.log` `51b2635d3f9dc0f8ceae657229613a5b1a603c20d99c96b063d611acb26d9dec`, expected-red `pytest-cfg-attr-red.log` `935165b03ccaadfdc7901ba9af48338e3f1e8cd7d55b289b90d846fcb8a84ba7`, `trailing-whitespace-final.log` `b128c169e5a8fb18c48cedd05402c439aa1175e2f8914a782a8eb9d1e530d9a9`, and `final-handoff-04.txt` `5b96368997f179f11e24df7d27f4a6f96b6d6525f6b3c56d68ef2501db978fb1`. The documentation-only ledger sync also passed guest `git diff --check`, recorded in `ledger-diff-check.log`; no code changed after the recorded code gates. The active sparse VM package occupies 4.3 GiB; the host has 464 GiB free, and the 77 GiB guest filesystem has 73 GiB free (4.2 GiB used). The existing Ubuntu disk/seed were reused; no new OS image or Cargo build target was created. This remains dirty-tree evidence, not an integration-commit or overall SLOPOS readiness claim.

### 2026-10-04 QA-01 release inventory and README captures

Commit A `daac08e3271d31fb7372f1e13459e391e88460a8` is pushed on `agent/qa-01-release-runner-inventory`; it does not modify `main`. The final QA run used a clean guest worktree at that exact commit in the reused UTM 4.7.5/QEMU-HVF Ubuntu 24.04.5 LTS ARM64 VM (kernel `6.8.0-142-generic`, Rust/Cargo 1.99.0, Python 3.12.3, pytest 9.1.1, PyYAML 6.0.1) at `2026-10-04T08:31:45Z`. The required pinned-venv command `python -m pytest -q scripts/test_check_architecture.py scripts/test_release_runner_inventory.py` passed with 98 tests. The live architecture checker exited 0 and explicitly reported zero workspace app packages and no app-conformance proof; the release inventory checker passed; `bash -n scripts/run-release-qa.sh`, Python bytecode compilation, workflow YAML parsing, `git show --check`, and a direct trailing-whitespace scan passed. Evidence directory `/home/slopos/evidence/20261004T083145Z-qa01-release-runner-inventory-daac08e/`; `summary.log` SHA-256 `be0f902dd330e73985f79b31d26f90881d17b326613ded9040a1b9485792c9ea`, `source-sha256.txt` `a0be17dec21075f5ee2051d9b2c97ed6578c5c0f90a58ced4a851e54e5ab2826`, and `SHA256SUMS` `ba4fc000be491368b18a17d9d8f9b912f4c07a414a212b917da073e8edc35593`.

The checker now compares `run-release-qa.sh` to a fixed verified template with gate invocations rendered from the inventory; this rejects unapproved shell statements instead of heuristically parsing arbitrary Bash. Independent reviews of earlier checker versions found command-hash poisoning, split/commented function declarations, glob-expanded script paths, dynamically expanded early exits, nested/negated conditionals, and duplicate JSON keys. The final implementation adds regressions for these cases and CI runs `bash -n`. A fresh independent review of commit A was not available after the review agents exhausted their thread capacity, so the task is recorded as IMPLEMENTED, not VERIFIED. The GitHub Actions job and full release runner were not executed. Workspace format/Clippy/tests, package/install gates, graphical journeys, and product readiness remain NOT_RUN by this scoped task.

README captures are byte-identical to the files in `/home/slopos/evidence/20261004-ui-gallery-progress/captures/`: `docs/screenshots/current-button-plate-1x.png` is 320×136, SHA-256 `d101bd748cf3836610d4e8249d762814665ec1de7d8f72022a933c5f5a95d82f`; `docs/screenshots/current-button-plate-2x.png` is 640×272, SHA-256 `3bf6ac76308bff55842088e54a7eb20abbdabf6e2dd1c20adfa4eef43c28463f`. They show nine button-plate fixtures only, have no text (font N/A), were captured from the dirty UI worktree based at `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`, and are pixel-identical to the October 2 fixture captures. Capture backend and a Figma/spec golden comparison were not recorded; the gallery logs note the AT-SPI bus was unavailable. The gallery implementation is not included in commit A, so these are qualitative snapshots, not commit-level UI or accessibility evidence. The 51 older documentation PNGs, 16 older QA PNGs, and their QA manifest were removed from the current tree; Git history still retains their prior objects.

Storage check: before creating the clean worktree the guest root filesystem showed 5.7 GiB used / 71 GiB free. The first `rustc --version` auto-installed the missing `stable-aarch64-unknown-linux-gnu` toolchain in the guest; after checks it showed 7.1 GiB used / 70 GiB free. No Cargo build target, new VM image, package install, or host toolchain was created.

### 2026-10-04 QA-01 release-runner merge and follow-up

Commits A `daac08e3271d31fb7372f1e13459e391e88460a8` and documentation-only B `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7` are now on `main` and `origin/main` by fast-forward from `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`. A's recorded guest test evidence remains evidence for A's code; B changes only `AGENTS.md` and `README.md`, and is not a fresh test run.

An independent static review at B found that the checker validates runner presence/executability and exact runner-template calls, but does not pin the required ID-to-script mapping or script contents. Removing a manifest entry while regenerating its matching runner call, rerouting an ID to another executable, or replacing a runner body with a no-op can therefore evade this inventory check. QA-01 is not complete. Follow-up `QA-01-RUNNER-INTEGRITY` is implemented but unverified on `agent/qa-01-required-runner-map`, based at B. The dirty worktree now pins canonical ID/path/hash tuples in the checker and adds omission, rerouting, stale-digest, and self-updated-manifest regressions. Read-only `shasum -a 256` output matches the 13 expected runner-body digests. Guest pytest, the live checker and `bash -n` remain NOT_RUN: key-only SSH to the existing VM is denied, and no project code was run on the host.

Execution is blocked pending guest access: UTM VM `AFB33C5E-99E0-465B-B695-7C80CB619772` is started at its tty login; `utmctl exec` reports the QEMU guest agent is absent, and forwarded SSH on host port `22264` rejects the available public key. No project tests were run on the host. Host data-volume free space was 465 GiB at this check; the existing UTM VM bundle is 7.3 GiB. No additional VM or Cargo target was created.

### 2026-10-06 foundation-tranche preparation

This is an unintegrated, documentation/specification preparation pass at base SHA `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`. The active host worktree is `agent/spec-01-schema-inventory`; its pre-existing `.serena/project.yml` modification was preserved. No SLOPOS code, repository scripts, Cargo commands, or QA tests were run on the host.

The October 4 note above records that commit A had not modified `main` at that time. At this preparation base, A is a direct parent and is in the `main` ancestry.

VM-00 remains BLOCKED. Host facts observed: macOS Darwin 27.0 ARM64; `df -h /` reported 460 GiB available; UTM 4.7.5 is installed. UTM reports VM `SLOPOS Ubuntu 24.04 ARM64 (reused disk)`, UUID `AFB33C5E-99E0-465B-B695-7C80CB619772`. The VM was started; its console currently shows an Ubuntu 24.04.5 LTS `slopos-dev` tty1 login prompt. `utmctl exec` and `utmctl ip-address` report that the QEMU guest agent is unavailable. Guest architecture/resources/free space, a logged-in X11 session, and a guest-local checkout were not revalidated. No runtime lease or build permit was issued.

SPEC-01 has a draft, not an extracted or verified specification. Eleven JSON files were created under `qa/spec/classic/`; the inventory has 130 contract-derived entries. Figma metadata returned root page `0:1` (`The Interface Elements`) and a few example frame/instance IDs (`85:18397`, `85:18399`, `85:18415`); none was established as the canonical source node for an inventory entry. The initial design-context request returned a no-selection message; later MCP calls hit the connected Starter-plan call limit, confirmed by `whoami`. The anonymous web viewer showed the board sections but did not expose component inspection. All component mappings, source revision, exact metrics, font details, and asset/font licenses remain unknown. Values retained from AGENTS.md §5 are labelled example-only. No Figma screenshot artifact was retained.

QA-00 static preparation completed through a read-only source/script audit; it ran no commands from the project. The current release runner invokes its listed legacy suites unconditionally, but overwrites the fixed `artifacts/qa/release/report.md`. Several nested suites use fixed X displays and `/tmp` paths, perform package installation, or use process-name cleanup. Workspace build/test outcomes remain NOT_RUN; `cargo test --workspace --locked` also needs the runtime lease because one inherited monitor test contacts system D-Bus and reads system state. Use a clean guest-local worktree at the recorded base after guest access is restored.

Recheck at 2026-10-06 11:29 UTC: the live UTM console still displayed Ubuntu 24.04.5 LTS at the `slopos-dev` login prompt. The current host shell does not resolve `utmctl`, so no fresh guest-agent or IP check was possible. A fresh read-only Figma metadata request for root `0:1` returned the same Starter-plan call-limit message and provided no additional source data.

At 2026-10-06 11:33 UTC, the bundled UTM CLI at `/Applications/UTM.app/Contents/MacOS/utmctl` reported the VM as `started`; its `ip-address` and `exec ... uname -a` calls both reported that the QEMU guest agent is not running or installed. The guest remains at the login prompt, with no authorized credentials or SSH path available in the project records.

Recheck at 2026-10-06 14:47 UTC: the local SSH config had no `Include` directives or likely VM aliases (`slopos`, `utm`, `ubuntu`, `vm`, `guest`, `linux`, `devbox`), and `ssh-add -l` showed zero loaded identities. CUA still showed the login prompt with only partial text `slopos-d`; no password was entered, and UTM input capture was released (`Capture Input=0`). The user was asked to clear the partial text, authenticate in UTM, and provide inspectable Figma access or a read-only export. A fresh root metadata request still returned the Starter-plan Figma call-limit message.

At 2026-10-06 14:51 UTC, UTM still reported the VM `started`. The guest console showed Ubuntu 24.04.5 LTS `tty1` at the `slopos-dev login:` prompt. A CUA username submission attempt did not reach a logged-in session; no password was entered, and input capture was released (`Capture Input=0`). VM-00/QA-00 remain blocked pending local guest login; SPEC-01 remains blocked pending inspectable Figma access or a source export.

At 2026-10-06 16:06 UTC, the prior VM UUID `AFB33C5E-99E0-465B-B695-7C80CB619772` and a failed replacement `DB05EAF1-5DA2-48BB-9A2A-C69E9F00A1F9` were deleted. Per the user's instruction, a fresh VM was created as `SLOPOS Ubuntu 24.04 ARM64 seeded`, UUID `946DB123-0FD5-4BA0-9BA2-DA19BF70EDC4`, using the preserved pristine Ubuntu Noble ARM64 cloud image, a sparse 64 GiB VirtIO disk, 6 GiB RAM, the seed ISO, Emulated VLAN, and loopback-only SSH forwarding on `127.0.0.1:22264`. UTM reports the VM `started`; the console reached early systemd boot output, but `utmctl ip-address` reports that QEMU guest agent is not running or installed, and an SSH probe timed out during banner exchange. Guest login, OS/kernel/resources, X11 session, guest-local checkout and recovery access therefore remain unverified. No VM build, test, or runtime permit was issued. Do not repeat VM rebuild attempts without a new access or boot path.

## L. Audit update protocol

### 2026-10-07 user-requested checkpoint

The user requested committing and pushing the completed preparation as-is. This checkpoint includes the release-runner failure-handling repair, cached Classic source mappings, integrity checker/regressions, CI registration and this ledger. These additions are **IMPLEMENTED / NOT VERIFIED**: their guest tests, GitHub CI and visual conformance remain **NOT_RUN**. The VM installation has not established VM-00 or QA-00 acceptance. The unrelated `.serena/project.yml` change is excluded from the checkpoint.

For each integrated tranche record the tested source commit, reviewed paths, task IDs, closed/new findings, commands/exit codes, VM/hypervisor/OS/toolchain, disk/resource observations, artifact paths/hashes and applicable gate results. Add graphical resolution/scale/spec provenance for UI work and real-vs-fixture device classification for provider work.

The coordinator owns this update in the same PR; workers provide the data. Follow §0.7 for a documentation-only ledger commit after a tested code commit. Never infer PASS from file existence, a successful screenshot capture, a previous revision or another agent's summary.

### 2026-10-20 integration merge checkpoint

At the owner's explicit instruction, the coordinator integrated every open branch into `main` on top of base `452e3d0`: `agent/qa-01-required-runner-map` (commits `716637e`, `621cab9`, `a997f9e`, `28f5df0`, `a474fcb`, `3dd0b24`), `agent/spec-01-schema-inventory` (`4d98d31`), and `claude/rewrite-agents-md` (`07ec521`, `a78d7d9`). Integration commits on `main`: `0d4feae` relocated the guest-extracted spec dataset intact to `qa/spec/classic-guest-extraction/` so the spec branch's canonical `qa/spec/classic/` could land without add/add conflicts; `c64d39a` is the spec merge; `40aefd3` adopts the AGENTS.md split, moving this contract to `docs/CONTRACT.md`. The two previously uncommitted `.serena/project.yml` files were committed as `3dd0b24` and `4d98d31`. All three branches are fully contained in `main` and retained for audit history; no PR review occurred — this was a direct owner-instructed integration.

Verification for this checkpoint was **text-only on the macOS host, without executing repository code** (§0.2.6): all 23 JSON files under `qa/spec/` plus `qa/release-runner-inventory.json` parse; a static render of `EXPECTED_RUNNER_TEMPLATE` is byte-identical to `scripts/run-release-qa.sh`; `REQUIRED_RUNNER_SCRIPTS` in the checker equals the 13-entry manifest in the same order; all 13 pinned SHA-256 digests match their script files; `bash -n scripts/run-release-qa.sh` and `ast.parse` of the four changed Python files succeed; no merge-conflict markers remain.

**No cargo build/test/clippy/fmt, no pytest, and no graphical or guest QA ran for this merge — all of it is NOT_RUN at the merged SHA.** Guest VM access was unavailable (VM-00 blocked above) and the host policy forbids executing repository code. The last guest-recorded workspace run (2026-10-03, on the pre-merge working tree) reported 205 passed / 0 failed / 11 ignored and does not cover this SHA. The spec branch's checker/test additions and their CI registration were never executed anywhere before this merge (SPEC-01-CACHED-* guest checks remain NOT_RUN). GitHub CI runs after the push; its result for the merged SHA is authoritative and must be read from Actions, not inferred from this entry.

### 2026-10-20 post-integration container check and CI triage

Run by an external audit agent at merged `main` `b332657` in a disposable Ubuntu 24.04 x86-64 container (not the guest VM; §28 still applies to graphical, install and hardware evidence). Rust 1.97.0 was used because the pinned 1.99.0 toolchain could not be downloaded there.

| Gate | Result |
|---|---|
| `cargo build --workspace --all-targets --locked` | PASS |
| `cargo test --workspace --locked` | PASS — 205 passed, 0 failed, 11 ignored (matches the 2026-10-03 guest count) |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check` | PASS |
| `check-architecture.py`, `check-release-runner-inventory.py`, `check-classic-source-mapping.py` | PASS |
| pytest: architecture, release-runner, classic-mapping suites | **FAIL — 111 passed, 2 failed** |
| `dpkg-buildpackage --build=binary` + the workflow's payload smoke check | **FAIL** at the session-descriptor check (see F-PKG-01) |

GitHub Actions on `b332657`: 18 jobs passed; *lockfile consistency*, *Exact-head .deb build + payload contract*, *Exact-head pkg.tar.zst build + payload contract* and *Debian x86_64 Live ISO* failed. Causes found:

- **F-QA-08 (merge regression).** `test_failed_gate_replaces_stale_pass_and_stops_runner[First gate]` and `test_rejects_synchronized_aggregate_runner_registration` arrived from `agent/spec-01-schema-inventory` and assume the old, unpinned fixture (`run-first-qa.sh`, a single error). `a997f9e` pins the 13 real runner scripts by SHA-256, so the fixture cannot replace them and registering the aggregate runner now also breaks the required set. Each parent passes on its own (42 and 40 tests); the merge `c64d39a` fails 2. Fix: intercept `bash <pinned script>` with a fake `bash` on `PATH` instead of writing new scripts, use `FIRST_GATE_LABEL`, and assert the aggregate error is *among* the errors.
- **F-PKG-01 (product defect).** `packaging/slopos-i.desktop` has `Exec=slopos-session` (unchanged since 2026-08-14); both package workflows require `Exec=/usr/bin/start-slopos-i`. The workflows are right: a display-manager login (GDM, LightDM, SDDM) skips `start-slopos-i`, so it gets no `appmenu-gtk-module`/`UBUNTU_MENUPROXY` global-menu export, no PCManFM desktop supervision and no default Platinum wallpaper; only `tuigreet` logins run the launcher. Fix: descriptor `Exec=`/`TryExec=start-slopos-i`, rewritten to `/usr/bin/start-slopos-i` by `packaging/debian/rules` and `packaging/arch/PKGBUILD`, and to `$PREFIX/bin/start-slopos-i` by `scripts/install-session-files.sh`. Verified locally that a deb built with that change passes the workflow's smoke check.
- **F-CI-01.** The Arch payload step exits 127: `bsdtar` is missing on `ubuntu-latest`, which moved to Ubuntu 26 on 2026-10-19. Install `libarchive-tools` in that step.
- **Debian Live ISO** was already failing at `dd81c67`, before this integration; not diagnosed (job logs were not readable from the audit environment).

Truthful-state status at `b332657`: the hard-coded network interface/SSID text, static top-bar network labels, `xdotool` Cut/Copy/Paste fallbacks are gone (F-NET-01/02, F-MENU-01 closed by `621cab9`, statically confirmed). F-CLAIM-01 remains open: the About dialog still calls the desktop "consumer-ready". The Network panel now only offers `nm-connection-editor`; it does not yet read the existing `slopos-services::network` provider, so no live network state is shown anywhere in Settings.

## M. Active task and ownership register

This register reflects the preparation state as of 2026-10-07 IST, amended by the 2026-10-20 integration checkpoint above (all branches merged into `main`; task statuses unchanged until a guest run at the merged SHA). The status at the original contract audit remains historical.

| Task | State | Owner/worktree/base | Dependencies or next action |
|---|---|---|---|
| VM-00 | CLAIMED | Coordinator; current host worktree `agent/spec-01-schema-inventory`, base `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`; new UTM guest UUID `CE2BA300-6754-4904-AFFF-20751B54C5DD`; no guest-local checkout or build/runtime permit | Installing from user-provided Ubuntu Server 26.04 ARM64 ISO onto a fresh sparse 64 GiB disk. The HVF install hit a kernel Oops in overlayfs while extracting files; serial evidence is retained outside the checkout. The same VM is now running with QEMU TCG, two Cortex-A72 CPUs and 6 GiB RAM. Installation, SSH, guest resources, X11/test account and recovery access are not yet verified. |
| QA-01-RELEASE-FAILURE-HANDLING | IMPLEMENTED | Worker `/root/qa_inventory_review`; current host worktree/base as VM-00; exact writes `scripts/run-release-qa.sh`, `scripts/check-release-runner-inventory.py`, `scripts/test_release_runner_inventory.py`; no guest build/runtime permit yet | Independent static review found missing ERR-trap inheritance inside `run_gate` and acceptance of the aggregate runner as its own manifest leaf. Repair both existing QA contracts with focused regressions; coordinator reviews and runs them in the verified guest before integration. |
| SPEC-01-CACHED-MAPPING | IMPLEMENTED | Worker `/root/recover_source_inventory`; branch/worktree/base as VM-00; exact writes `qa/spec/classic/inventory.json`, `buttons.json`, `menus.json`, `source-nodes.json`, `source-metadata.xml`; coordinator retains schema/manifest ownership; no build/runtime permit | Reconcile the contract-derived inventory with retained Figma metadata from the same canonical file/root. Cached XML hash `9a0ee26ac340cccea6b5ca1ab019573a0ab3f8053e5df86b92eaa872570220d4` matches the archived spec manifest. Preserve unknown revision, style, font and license; historical geometry does not establish current conformance or complete SPEC-01. |
| SPEC-01-CACHED-VALIDATOR | IMPLEMENTED | Worker `/root/recover_source_inventory`; same worktree/base; exact writes `scripts/check-classic-source-mapping.py`, `buttons.json`, `menus.json`; coordinator owns registration; no execution permit | Independently reconcile retained XML, catalog and mapped records using the standard library. Guest positive and corrupted-data checks remain NOT_RUN; this integrity check cannot establish visual conformance. |
| SPEC-01-TEXT-MAPPING | IMPLEMENTED | Worker `/root/qa00_static`; same worktree/base; exact write `qa/spec/classic/text-fields.json`; no execution permit | Reconcile the proven text-field source correspondence with the recovered inventory. Preserve incomplete behavior/style/state information and historical bounds. Guest checks and independent review remain NOT_RUN. |
| SPEC-01-CACHED-REGRESSIONS | IMPLEMENTED | Worker `/root/qa_inventory_review`; same worktree/base; exact write `scripts/test_classic_source_mapping.py`; no execution permit | Positive integrity check and focused corrupted-data regressions for missing variants, unsupported authority claims, mismatched bounds/catalog and XML hash. Guest execution remains NOT_RUN. |
| SPEC-01-CACHED-CI | IMPLEMENTED | Worker `/root/qa_inventory_review` edited delegated `.github/workflows/ci.yml`; same worktree/base; CI ownership returned to coordinator; no build/runtime permit | Added the cached source checker and its regressions to the existing Python QA step without new dependencies or conformance claims. Guest execution and GitHub CI remain NOT_RUN. |
| QA-00 | BLOCKED | Read-only audit by `/root/qa00_static` on the current checkout at base `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`; no write paths, build permit or runtime lease | Script side-effect inventory is complete. Guest build/test baseline waits on VM-00. Run from a clean guest-local worktree; schedule one build permit and an exclusive runtime lease for the full workspace tests. The October 2 baseline commands were VERIFIED on the retired guest checkout and remain recorded in Part II.K; no baseline has been run at this merged base. |
| SPEC-01 | CLAIMED | Coordinator sequential fallback; host base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exclusive writes `qa/spec/classic/`; guest validation serialized with build permit | Feynman's worker terminated on an account usage-limit error before handoff; coordinator reclaims ownership under §0.9. Connected Figma API root metadata yields 430 symbol nodes. Initial inventory/schema and nine button geometries are authored; all nine button label font/size/color/alignment states are extracted. Guest parses all authored JSON and validates inventory schema. Full drawing/state extraction, exact source revision, font version/license metrics, behavior/a11y, and independent component conformance remain unresolved. Current Figma account is Full seat on Starter; metadata-only read of node `73:3720` reports Selected=False/Rest at 12×12, while screenshot/design-context API calls hit the Starter-plan tool-call limit. A read-only canvas selection triggered a sign-up modal; it was dismissed without sign-up, edits or data entry. Public canvas remains insufficient for precise component extraction; no missing visual detail was inferred. |
| BOOT-01 | CLAIMED | Coordinator sequential fallback; exact writes root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/slopos-core/`, `crates/slopos-ui/`, `tools/slopos-ui-gallery/`; baseline SHA above | XDG paths, integer-scale/geometry, validated `ApplicationId`, validated D-Bus object paths, source-preserving `CoreError`, and seven-state `Availability` are implemented; changed workspace format/build/Clippy/tests exit 0 in K. Rust 1.99.0 pin tested. Independent core review found no definite defect; `ApplicationId` is scoped to Desktop Entry/well-known bus names, not interface or object-path identifiers. Remaining full core/appkit contracts and component foundation/gallery are open; this is not full BOOT-01 or component completion. No app migration or arbitrary design tokens. |
| BOOT-01 gallery slice | IMPLEMENTED | Coordinator; same dirty baseline; exact writes `crates/slopos-ui/src/render.rs`, `crates/slopos-ui/src/testing.rs`, `crates/slopos-ui/src/foundation/color.rs`, UI module roots/manifests, `tools/slopos-ui-gallery/`, root workspace/lock and ledger | Surface drawing, scale validation, transparent clearing/composition and initial fixture implemented. Current workspace and explicit real-VM pixel/lifecycle checks pass provisionally as recorded in K; no tested commit exists. Full gallery, component conformance and accessibility remain NOT VERIFIED. Runtime/build permits released. |
| BOOT-01 coverage slice | IMPLEMENTED | Raman `01a0fb2d-8b3b-7682-bf6b-6acb05e27c14`; tests-only writes `crates/slopos-core/src/xdg.rs`, `crates/slopos-ui/src/foundation/geometry.rs`; ownership returned to coordinator after handoff | Mixed XDG/error-kind and isolated overflow tests added; current guest gates pass provisionally. No API changes, dependencies, commits or branches. Independent read-only renderer/teardown review also completed; coordinator fixed identified lifecycle leak and missing alpha coverage, then performed verification in K. |
| BOOT-01 shared core primitives | VERIFIED | Raman `01a0fb2d-8b3b-7682-bf6b-6acb05e27c14`; exact implementation paths `crates/slopos-core/src/identity.rs`, `error.rs`, `capability.rs`; coordinator registered modules in `lib.rs`; guest jobs=2 build permit | 12 core tests and full current workspace format/build/strict-Clippy/tests pass in K; independent read-only review by Planck found no definite syntax-contract defect. Scope covers well-known application names only. Provider consumers and command-level error integration remain open; task verification does not complete BOOT-01 or SLOPOS. |
| SPEC-01 menus | VERIFIED | Kierkegaard `01a0fb74-82a0-7f03-b398-300dac6a764b`; exact write `qa/spec/classic/menus.json`; source Figma page `1:4`; no VM/build permit | 32 variant/source rows cross-checked against inventory and referenced component-set parents; guest JSON validation passes. Figma Starter-plan limits block fresh style/behavior/accessibility capture; no conformance claim. |
| SPEC-01 typography | VERIFIED | Ohm `01a0fb74-7d46-7390-9375-96c5a1883d00`; exact write `qa/spec/classic/typography.json`; source Figma page `73:8994`; no VM/build permit | Source font labels/specimen geometry recorded and guest JSON validation passes. Exact version/redistribution license unknown; no bundled fonts or conformance claim. |
| DONOR-TRUTH-01 | VERIFIED | Coordinator; exact write `crates/slopos-shell/src/topbar.rs`; shell tests and exclusive VM graphical lease | Fabricated network-detail labels removed; shell tests pass; guest Xorg/Openbox screenshot shows unavailable-details row. Existing Preferences handler retained but click untested; legacy summary/provider accuracy remains open; does not close SVC-01. |
| DONOR-TRUTH-04 | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-settings/src/panels/network.rs`, `crates/slopos-shell/tests/shell_tests.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest jobs=2 permit released; no graphical lease | Fake network details and inert controls removed; editor handoff has visible launch failure; static regression and full workspace gates pass in K. External editor is absent on the guest and UI runtime is NOT_RUN. Provider-backed network state/actions and SLOPOS UI-component migration remain open; does not complete SVC-01/02. |
| DONOR-MENU-TRUTH-02 | VERIFIED | Coordinator; exact writes `crates/slopos-shell/src/topbar.rs`, `crates/slopos-shell/tests/shell_tests.rs`; no graphical lease | Synthetic `xdotool` fallback entries removed. Regression assertion fails on baseline source and passes after patch; guest shell unit/integration/reconciliation suites pass. Exported app-action integration remains incomplete. |
| DONOR-MENU-BRIDGE-03 | VERIFIED | Coordinator; exact writes `crates/slopos-shell/src/main.rs`, removed `crates/slopos-shell/src/gmenu.rs`, retained `crates/slopos-shell/src/menu/gmenu.rs`, updated `crates/slopos-shell/tests/shell_tests.rs` | Only the runtime bridge remains registered; bridge tests still target GTK protocol support, and retained runtime implementation still contains KDE appmenu support. Guest shell suites pass. |
| UI-01 button-plate geometry sub-slice | IMPLEMENTED | Coordinator; exact writes `crates/slopos-ui/src/primitives/`, `crates/slopos-ui/src/lib.rs`, `crates/slopos-ui/src/render.rs`, `crates/slopos-ui/src/testing.rs`, `tools/slopos-ui-gallery/src/main.rs`, gallery README, `qa/spec/classic/buttons.json`; shared module registration coordinated here | Three variants and three states render pixel-stepped geometry at 1×/2×, with selected independent raster assertions and real guest captures. This is not a complete button or a verified UI-01; no labels, pointer/keyboard/focus/action/accessibility semantics, complete assertions, or exact source state geometry. Focused guest tests and captures recorded in K; rerun full workspace gates and hash all sources before relying on them. |
| UI-02 ButtonControl action/accessibility/dimensions sub-slice | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-ui/src/controls/button.rs`, `crates/slopos-ui/src/controls/mod.rs`, `crates/slopos-ui/src/lib.rs`, `crates/slopos-ui/src/primitives/button_plate.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2, released; isolated Xvfb/private D-Bus test | Wraps native GTK activation; renders existing plate with injected Pango font; overrides GTK theme spacing inside the component and verifies mapped outer geometry for every button variant; configures ATK semantics and blocks disabled callbacks. Pointer-inside activation, release-outside cancellation, focused Space activation and disabled synchronous activation suppression are verified by UI-02-BTN-INPUT in K. No physical disabled-event dispatch is claimed. Gallery registration, pixel/text alignment/state goldens and Figma conformance remain unverified. Checkbox, radio and disclosure controls remain outstanding; UI-02 remains incomplete. |
| UI-02-BTN-INPUT | VERIFIED | Coordinator sequential fallback; host branch `main`, guest checkout detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact write `crates/slopos-ui/src/controls/button.rs`; one guest build/test permit, `CARGO_BUILD_JOBS=2`; isolated private D-Bus and unique Xvfb/XDG directories; no concurrent graphical session | Real pointer press/release inside, release-outside cancellation, and focused Space press/release are verified by three separate process runs per journey. Disabled activation suppression uses the existing GTK synchronous `clicked()` test; no physical disabled-event dispatch is claimed. Tests use a unique `$run=$(mktemp -d /tmp/slopos-ui02.XXXXXX)` with `runtime,config,data,cache,state`, `chmod 700 $run/runtime`, and `env CARGO_BUILD_JOBS=2 XDG_RUNTIME_DIR="$run/runtime" XDG_CONFIG_HOME="$run/config" XDG_DATA_HOME="$run/data" XDG_CACHE_HOME="$run/cache" XDG_STATE_HOME="$run/state" dbus-run-session -- xvfb-run -a cargo test --locked -p slopos-ui <filter> -- --ignored --nocapture --test-threads=1`; filters: `disabled_button_does_not_activate_and_enabled_button_activates` once, `pointer_release_outside_cancels_button_activation` three times, `focused_button_activates_from_space` three times, each separate sequential Cargo process. Package fmt/build/strict-Clippy/tests all pass as recorded in K. Independent review found no concrete defects. Dirty-snapshot verification only; full UI-01/UI-02, focus-ring specification, Figma conformance, gallery registration, and checkbox/radio/disclosure controls remain open. |
| UI text-rendering foundation sub-slice | IMPLEMENTED | Coordinator; exact writes root `Cargo.toml`, `Cargo.lock`, `crates/slopos-ui/Cargo.toml`, `crates/slopos-ui/src/render.rs`; no overlapping worker | PangoCairo renders supplied text at 1×/2× into the Cairo surface; a raster test checks visible glyph output at both scales. This does not choose/bundle a product font, define canonical text geometry, prove Figma conformance or complete UI-01. Full current workspace gates pass in K. |
| UI-01 Row/Column layout sub-slice | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-ui/src/foundation/geometry.rs`, `crates/slopos-ui/src/layout/`, `crates/slopos-ui/src/lib.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; VM build permit jobs=2, released | Checked logical-pixel row/column placement with explicit gap and 4 geometry tests; no product spacing defaults. Workspace gates and 35 source hashes pass in K. No machine-readable layout source extraction, gallery/conformance fixture or complete layout-family implementation; UI-01 remains open. |
| UI-01 ATK accessibility base sub-slice | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-ui/src/accessibility.rs`, `crates/slopos-ui/src/lib.rs`, `crates/slopos-ui/src/testing.rs`; same dirty base; exclusive Xorg runtime lease released | Shared configuration helper and gallery panel metadata use GTK3 ATK; focused real-X11 test asserts role/name/description. No app widget is verified accessible and no remote AT-SPI conformance is claimed; warning on private bus shutdown recorded in K. UI-01/G-ATOM remain incomplete. |
| UI-01 image/surface composition sub-slice | IMPLEMENTED | Coordinator sequential fallback; exact write `crates/slopos-ui/src/render.rs`; same dirty base; jobs=2 build permit released | Raster-surface composition preserves logical origin across target integer scales and selects nearest-neighbor filtering. Independent pixels pass at target 1×/2×. No file-backed Image/Icon API, accessibility semantics, or gallery fixture; UI-01 remains incomplete. Latest workspace gates and hashes in K. |
| UI-01 Pixbuf image rendering sub-slice | IMPLEMENTED | Coordinator sequential fallback; exact write `crates/slopos-ui/src/render.rs`; same dirty base; jobs=2 build permit released | Existing GDK Pixbuf can draw onto the SLOPOS raster surface at integer scale and logical origin with nearest filtering. Independent pixel test passes at 1×/2×. This does not implement complete Image/Icon semantics or asset policy; UI-01 remains incomplete. Latest workspace gates and hashes in K. |
| SVC-01 NetworkManager discovery, events and Wi-Fi scan slices | IMPLEMENTED | Coordinator sequential fallback; exact writes root `Cargo.toml`, `Cargo.lock`, `crates/slopos-services/`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2; isolated D-Bus runtime fixture | UI-free bounded snapshot of manager enablement, devices, hidden/visible Wi-Fi APs, active connections and IPv4/IPv6 address/gateway/name-server data. Monotonic revisions and bounded stream; watcher tracks manager/device/active-connection properties, AP add/remove signals and service-owner changes; fixture verifies hotplug, state/AP events, disconnect and reconnect. `request_wifi_scan` validates device type and dispatches a bounded NetworkManager scan request; completion remains asynchronous through `LastScan`/AP updates. Live service and radio are absent/unverified. Permission/failure/cancellation matrix, complete adapter integration and physical-device verification remain open; SVC-01 overall remains open. Evidence in K. |
| SVC-02 saved-profile actions slice | IMPLEMENTED | Coordinator; exact write `crates/slopos-services/src/network/mod.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2; isolated D-Bus fixtures | Bounded saved profile discovery now returns type/ID/UUID/object path without secrets; validated/bounded activate/deactivate/global networking+wireless enable and saved VPN activation return after authoritative snapshot read-back. Fixtures cover profile classification, physical-uplink selection, VPN state, disconnect, denied toggles, read-back timeout and caller-future cancellation. Remote mutation rollback on cancellation is not claimed. Full SVC-02 remains open: new-network secret-agent flow, authorization beyond denial, remote cancellation and additional timeout failures remain unfinished; live daemon/hardware behavior is NOT_RUN. Evidence recorded in K. |
| SVC-03 audio adapter foundation | IMPLEMENTED | Coordinator; exact writes `crates/slopos-services/Cargo.toml`, `crates/slopos-services/src/audio.rs`, `crates/slopos-services/src/lib.rs`, `Cargo.lock`, `crates/slopos-settings/Cargo.toml`, `crates/slopos-settings/src/panels/sound.rs`, `crates/slopos-shell/tests/shell_tests.rs`, Ubuntu/Debian package lists and ISO build lists; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2; Pactl fixtures plus isolated live null sink/remapped source; isolated Sound-panel Xvfb smoke | Pactl-backed typed snapshot, output/input volume and mute, defaults, bounded actions/read-back and subscriptions are fixture-tested; target disappearance plus serialized relative-volume/mute actions, direct-child command timeout and fixture watcher retry/recovery are covered. Live PipeWire/Pulse-compatible output and remapped virtual input levels/mutes read back successfully; no physical device is attached. Shell monitor/topbar and Settings Sound share one client; Settings now subscribes to device/state events, retains edited values, disables controls on provider errors and stops its watcher on close. Settings holds its dialog open while bounded mutations finish rather than canceling them. Live-service restart/hotplug, physical-device support, raw-GTK design-system replacement and complete SVC-03 provider adoption remain open; SVC-03 is not VERIFIED and G-PROVIDER remains open. One non-reproduced timeout-test failure is documented in K. |
| SVC-04 brightness provider foundation | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-services/src/brightness.rs`, `crates/slopos-services/src/lib.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2, released; isolated temp-sysfs fixtures | Sysfs discovery, bounds, normalized level, typed unavailability, bounded polling events/hotplug, event stream shutdown, validated serialized logind `SetBrightness` mutation and immediate read-back. Four provider tests pass. The VM reports no backlight; logind brightness mutation is not exercised. Not integrated into brightness applet/OSD/settings/keys and no cross-process event propagation. SVC-04 remains incomplete. |
| SVC-04 power provider and shell consumer | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-services/src/power.rs`, `crates/slopos-services/src/lib.rs`, `crates/slopos-shell/src/services/monitor.rs`, `crates/slopos-shell/src/services/mod.rs`, `crates/slopos-shell/src/main.rs`, deleted legacy `crates/slopos-shell/src/services/power.rs`, `crates/slopos-shell/tests/shell_tests.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest build permit jobs=2, released; private D-Bus fixtures | Typed battery/supply/lid/suspend snapshots, bounded change and warning stream, low/critical notifications, no legacy sysfs fallback. Seven service power tests and shell source-contract test pass; entire workspace gates pass in K. UPower absent and no physical battery/backlight; logind writes/suspend not invoked. Power settings, lid behavior configuration, hardware notifications beyond low/critical, and integration into all required brightness surfaces remain unfinished; SVC-04 remains incomplete. |
| SVC-04-SUSPEND | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-services/src/power.rs`, `crates/slopos-shell/src/services/monitor.rs`, `crates/slopos-shell/src/services/power.rs`, `crates/slopos-shell/src/services/session.rs`, `crates/slopos-shell/src/topbar.rs`, `crates/slopos-shell/tests/shell_tests.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; private D-Bus fixtures; guest build permit jobs=2 released | Bounded `CanSuspend`/interactive `Suspend(true)` operation; both menu actions route off the GTK thread; errors reach a dialog; capability survives missing UPower and drives menu availability without command probes. Tests and workspace gates recorded in K. Actual suspend/resume is NOT_RUN. SVC-04 still needs live resume validation and brightness/power UI integration. |
| SVC-06 timedate provider slice | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-services/src/timedate.rs`, `crates/slopos-services/src/lib.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; private D-Bus fixtures and read-only live timedated test; guest jobs=2 permit released | Typed snapshot, timezone enumeration, timezone/NTP/time actions, read-back and revisioned watch are implemented and verified as recorded in K. Integration into clock/Date & Time UI and remaining SVC-06 Bluetooth/media/input work remains open. |
| SVC-06 Bluetooth adapter/device foundation | IMPLEMENTED | Coordinator sequential fallback; exact writes `crates/slopos-services/src/bluetooth.rs`, `crates/slopos-services/src/lib.rs`; same dirty base; private D-Bus fixture and live-state unavailability check; guest jobs=2 permit released | Typed BlueZ adapter/device snapshots, bounded polling watch, power/discovery actions, read-back, typed errors and fixture denial/service-loss coverage pass as recorded in K. Pair/connect/disconnect/trust fixture actions are recorded in the follow-up row. Pairing-agent UX, signal-based hotplug, applet/panel integration and physical-device tests remain open/NOT_RUN. |
| SVC-06 Bluetooth Device1 actions and object-path follow-up | IMPLEMENTED | Coordinator sequential fallback; exact write `crates/slopos-services/src/bluetooth.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; private D-Bus fixture and read-only live probe; guest jobs=2 permit released | Device path parsing/validation, pair/connect/disconnect/trust actions, denial and authoritative read-back are verified in K. Pairing still uses BlueZ's default agent; the first-party pairing prompt/agent, real hardware, hotplug signals and applet/Settings integration are NOT_RUN. |
| SVC-06-BT-CANCEL pairing cancellation follow-up | VERIFIED | Coordinator sequential fallback on existing `main` worktree; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact write `crates/slopos-services/src/bluetooth.rs`; one guest build permit, jobs=2 released; private D-Bus fixture | Bounded public cancellation; BlueZ cleanup on timeout and caller-future drop; startup retry without treating `DoesNotExist` as successful user cancellation; per-device serialization; cleanup failures surfaced and retry registration retained. Fixture covers normal, timeout, explicit/startup race, drop, failed-cleanup and recovery paths. UI/agent prompts, real hardware and integration remain open/NOT_RUN. Evidence and independent review are in K. |
| SVC-06 Media/MPRIS provider foundation | IMPLEMENTED | Coordinator sequential fallback on existing `main` worktree; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-services/src/media.rs`, `crates/slopos-services/src/lib.rs`; one guest build permit, jobs=2 released; private session-bus fixtures | Typed player discovery/snapshots, capabilities, bounded actions, partial-player failure reporting, owner-loss classification and a lifecycle-safe event stream are verified in K. Live external player, shell applet/media-key consumption and broader SVC-06 integration remain open/NOT_RUN. |
| APP-02 Date & Time legacy consumer slice | IMPLEMENTED | Coordinator sequential fallback; exact write `crates/slopos-settings/src/panels/datetime.rs`; base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; guest jobs=2 permit released; isolated Xvfb and private session bus, no system-time mutation | Legacy panel now loads timedated state/time-zones asynchronously and uses the shared provider for timezone/NTP/manual-time actions; truthful unavailable/error states replace hardcoded defaults/subprocesses. Focused timezone conversion test, Xvfb map/close smoke, full workspace gates and read-only timedated test are recorded in K. Graphical Apply action, AT-SPI and visual conformance are NOT_RUN; raw-GTK donor migration and native Control Panels remain open. |
| APP-02 Date & Time live-state/failure-safety follow-up | IMPLEMENTED | Coordinator sequential fallback; exact write `crates/slopos-settings/src/panels/datetime.rs`; same dirty base; isolated Xvfb/Openbox/private-session-bus smoke; guest jobs=2 permit released | Provider watch refreshes non-dirty fields and live clock; Apply locks edits and leaves failure state read-only; watcher starts inside Tokio and is torn down safely. Regression, read-only live service and full gates pass as recorded in K. Actual Apply/system-time mutation, AT-SPI, visual conformance and donor-UI migration remain NOT_RUN/open. |
| SVC-07-UDISKS-DISCOVERY | VERIFIED | Coordinator sequential fallback after Ptolemy `01a0fec7-f692-78c3-a7e6-5ba8cafadf36` shut down before handoff; guest `/home/slopos/work/rust-slopos-bootstrap`, detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-services/src/storage.rs`, `crates/slopos-services/src/lib.rs`; one guest build/test permit, `CARGO_BUILD_JOBS=2`, released | Verified read-only UDisks2 inventory snapshot, byte-preserving paths, separate Block/Drive models, optional Filesystem state, typed availability/errors, malformed-property cases, and deterministic ordering. Private-bus fixtures plus a live system-bus smoke and package gates are recorded in K; independent review found no concrete defect. Snapshot-only leaf: watch/events, mount/unmount/eject/power-off actions, Files integration, removable-media hardware and overall SVC-07 remain open. |
| SVC-07-UDISKS-WATCH | VERIFIED | Coordinator after Arendt `01a0ff64-2cf2-7590-8c09-d791ec0853cb` handoff; host `main`, guest `/home/slopos/work/rust-slopos-bootstrap` detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; pre-task guest `storage.rs` SHA-256 `f768cff084173d68549aa425b06774c79bcba7235f8761865d7c49a8f2c793c3`; exact write `crates/slopos-services/src/storage.rs`, final SHA-256 `4c87d99d7bb99167d1a2f2ac1646a5f843bd150d1a36eeabdcc33dc914e6a1ca`; no branch/commit authorized; one guest build permit (`CARGO_BUILD_JOBS=2`) released; private-bus tests only, no graphical lease | Watcher API and bounded capped retry are implemented. Five private-bus tests cover initial state/revision, object/property changes, service loss/recovery, timeout recovery, slow-consumer coalescing and cancellation. Full guest format/build/strict-Clippy/workspace tests pass at this source SHA, and independent exact-SHA review found no additional defect in the reviewed scope; evidence in K. Fixture-only; live UDisks/removable hardware, mount/eject actions, Files integration and overall SVC-07 remain open. Not integrated at a commit SHA. |
| SVC-07-UDISKS-ACTIONS | VERIFIED | Coordinator sequential takeover after Lagrange `01a0ff46-12f1-7752-8110-89b744e6df38` partial handoff; independent reviewer Aristotle `01a0ff06-0093-7741-a8f5-f3cfdaa0f38d`; host `main`, guest `/home/slopos/work/rust-slopos-bootstrap` detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; prerequisite `storage.rs` SHA-256 `4c87d99d7bb99167d1a2f2ac1646a5f843bd150d1a36eeabdcc33dc914e6a1ca`, final host/guest SHA-256 `6a591972b08747b9cd07ad04b969e45a17c3cf90462c348b3e6ad881bd4145da`; exact write `crates/slopos-services/src/storage.rs`; no branch/commit authorized; one guest build/test permit (`CARGO_BUILD_JOBS=2`) released; private-D-Bus fixtures only, no graphical lease | Mount/unmount/eject/power-off provider actions are fixture-tested; timeout classification and per-client serialization after caller-drop were corrected. Focused tests: 12 passed, 0 failed. Full guest format/build/strict-Clippy/workspace tests at this SHA each exit 0: 205 passed, 0 failed, 11 ignored. Evidence and exact hashes are in K. Independent exact-SHA review found no definite regression, but records an unresolved inferred late-remote-effect risk after a dispatched method timeout: the bounded snapshot result is ignored and the local guard eventually releases, so callers must re-read before retry. VERIFIED is limited to the fixture/code gates, not live UDisks mutation or hardware. Files integration and overall SVC-07 remain open; no integration commit is claimed. |
| BOOT-01-UI-INSETS | VERIFIED | Gauss `01a0ff03-a12b-7a70-b00c-ccd8c6cdc82b`; coordinator registered `crates/slopos-ui/src/foundation/mod.rs`; host branch `main` and guest checkout detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-ui/src/foundation/insets.rs`, `crates/slopos-ui/src/foundation/mod.rs`; one guest build permit, jobs=2, released; no branch/commit authorized | Checked logical-pixel insets and five edge-case tests pass the final full-workspace format/build/strict-Clippy/tests gates; 181 passed, 11 ignored. Independent read-only review found no definite defect. Dirty-snapshot verification only; no visual metrics/conformance or integration-commit claim. BOOT-01 remains incomplete. |
| UI-01-GRID-PLACEMENT | VERIFIED | Fermat `01a0ff0d-cdcf-7dc0-9a43-8bf8bd79911d`; coordinator registered API and generalized shared error; host `main`/guest detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-ui/src/layout/grid.rs`, `crates/slopos-ui/src/layout/mod.rs`, `crates/slopos-ui/src/layout/linear.rs`; one guest build permit, jobs=2, released; no branch/commit authorized | Row-major max-track geometry, empty/short-row/overflow cases, and full-workspace format/build/strict-Clippy/tests gates pass; 187 passed, 11 ignored. Independent review found no definite defect. Dirty-snapshot verification only; no visual metrics/conformance or integration-commit claim. UI-01 and BOOT-01 remain incomplete. |
| UI-01-OVERLAY-GEOMETRY | VERIFIED | Parfit `01a0ff1b-8d69-74d2-bc44-aa5560f9fd61`; independent reviewer Kuhn `01a0ff21-ff69-78e2-862d-af0249da9892`; shared coordinator working tree at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-ui/src/layout/overlay.rs`, `crates/slopos-ui/src/layout/mod.rs`; no branch/commit authorized; one guest build permit, `CARGO_BUILD_JOBS=2`, released | Overlay geometry preserves supplied origin, child dimensions and order; empty input returns empty. Two focused tests, full workspace format/build/strict-Clippy/tests gates and independent review pass as recorded in K. Dirty snapshot only; no visual metrics, clipping/paint behavior, or conformance claimed. Broader UI-01 and BOOT-01 remain incomplete. |
| UI-01-ALIGN-PLACEMENT | VERIFIED | Bacon `01a0ff2c-8fc2-7de1-91f7-f32f6731808f`; independent reviewer Euclid `01a0ff31-03a9-72c3-af46-1fafa4942fdb`; shared dirty tree at base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; exact writes `crates/slopos-ui/src/layout/align.rs`, `crates/slopos-ui/src/layout/mod.rs`; no branch/commit authorized; one guest build permit, `CARGO_BUILD_JOBS=2`, released | Start/Center/End alignment preserves child dimensions and reports coordinate overflow; four focused tests, full workspace format/build/strict-Clippy/tests gates and independent review pass as recorded in K. Dirty snapshot only; no Figma metrics/conformance or integration-commit claim. UI-01 and BOOT-01 remain incomplete. |
| SPEC-01-REQUIREMENT-CATALOG | VERIFIED | Lagrange `01a0ff46-12f1-7752-8110-89b744e6df38`; host `main` and guest `/home/slopos/work/rust-slopos-bootstrap` detached at dirty base `e2f8f6b902da3a3b9cbe707db0994b5ce87d5fd7`; worker writes `qa/spec/classic/requirements.json`, `qa/spec/classic/requirements.schema.json`; coordinator registered them in `qa/spec/classic/manifest.json`; no branch/commit authorized | Covers §§9.4–9.13 with 93 components, 13 states, 20 behaviors, 8 icon requirements, and 9 typography roles × 6 properties. Independent review found missing menu crosswalk mappings; coordinator corrected them, including all known disabled variants, then guest schema, shape, conditions, duplicate-key, unknown-value, and cross-file/menu-completeness checks passed. Final catalog hashes: `requirements.json` `8a29e6a8abf2086922f26befcf9951096645882523fd61de15238c6e9531e866`, schema `42646296e59c00233925dd8c78c51046963f0ffa938fe39b1652ddfddccf096f`, manifest `2f1887d1f9a1242737d1c94ea9452f6d78b7f471b7cd4fe9a3d4e83d3b00e7c7`. Evidence `/home/slopos/evidence/20261003-bootstrap/spec-catalog-final.G3q6z3/validation.log`, SHA-256 `fe251d3cc8e4652b5cc23057aa1493e68176322bd3ea2c4183bba1c22bf8cc5b`. Figma source revision and unextracted design details remain UNKNOWN; implementation and visual conformance are NOT_ASSESSED. |
| QA-01-UI-BOUNDARY | VERIFIED | Coordinator; final read-only review by Godel `01a1059b-5d63-7982-b54a-6d9fb0414141`; task branch `agent/qa-01-release-runner-inventory`, commit A `daac08e3271d31fb7372f1e13459e391e88460a8`, now in `main` ancestry at `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`; exact write scope `scripts/check-architecture.py`, `scripts/test_check_architecture.py`, plus CI/config integration in `.github/workflows/ci.yml` and `qa/requirements-architecture.txt`; guest test permit released | Enforces first-party app UI dependency by resolved package identity and alias, rejects malformed metadata, symlink/package-root escapes and external Cargo targets, masks Rust comments/strings, and fails closed on conditional path attributes. The exact commit A combined guest suite passed 98 tests; the live checker reports zero workspace app packages and explicitly disclaims app conformance. Prior focused architecture suite passed 61 tests and independent review found no remaining issue in this checker. GitHub CI, full Rust workspace gates, graphical journeys, and app conformance were not run. This verifies only the architecture-boundary slice, not overall QA-01 or SLOPOS completion. |
| QA-01-RELEASE-RUNNER-INVENTORY | IMPLEMENTED | Coordinator; task branch `agent/qa-01-release-runner-inventory`, commit A `daac08e3271d31fb7372f1e13459e391e88460a8`, now in `main` ancestry at `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`; clean guest worktree tested at A; exact writes `qa/release-runner-inventory.json`, `scripts/check-release-runner-inventory.py`, `scripts/test_release_runner_inventory.py`, `scripts/run-release-qa.sh`, `.github/workflows/ci.yml`, QA prerequisites/tests, README screenshots and README section; one guest Python test permit released; final independent review NOT_RUN due agent capacity | Required-runner inventory and static runner template are implemented. On exact commit A, the combined pinned-venv suite passed 98 tests; both live checkers, Bash syntax, bytecode compilation, workflow YAML parsing, commit whitespace and direct trailing-whitespace gates passed. Prior independent reviews found false-pass shell forms in the heuristic scanner; it was replaced by exact-template validation with regressions. No independent review of the final rewrite, GitHub CI run, full release runner, full Cargo workspace gates, visual conformance, or product acceptance is claimed. Evidence and storage/toolchain details are in K; no product gates from §30.3 are inferred to pass. |
| QA-01-RUNNER-INTEGRITY | IMPLEMENTED | Coordinator; branch `agent/qa-01-required-runner-map`, base `452e3d0f3441e1c38a7e53b9a9842e38e75e5fe7`; dirty host checkout only, no guest-local worktree because VM access is unavailable | Exact writable paths: `scripts/check-release-runner-inventory.py`, `scripts/test_release_runner_inventory.py`, `qa/release-runner-inventory.json`. The checker pins canonical ID/path mappings and script SHA-256 values independently of the manifest; regressions reject omission, rerouting, changed content without a digest update, and changed content with a self-updated manifest digest. Verify with `python3 -m pytest -q scripts/test_release_runner_inventory.py`, live checker and `bash -n` inside the Ubuntu guest; one Python test permit, no graphical/system-service lease. The tests and checker changes are committed and pushed with this integration (commit a997f9e on `main`); guest pytest, the live checker and `bash -n` remain NOT_RUN pending guest access. |
| Remaining §21 tasks | PLANNED | Unassigned | Instantiate rows when dependencies and exact path ownership are known |

Ruling: QA-01-UI-BOUNDARY also rejects any `apps/**/Cargo.toml` that is not represented by a workspace package/member and fails closed on missing Cargo resolution data; otherwise an unregistered app manifest or malformed metadata could bypass the architecture gate. Cost if wrong: intentionally experimental manifests under `apps/` must be moved outside the product app tree or registered before the check can pass.

The coordinator additionally owns `qa/requirements-architecture.txt`, which pins `pytest==9.1.1` and its verified transitive versions, and uses a guest/CI virtual environment for `python3 -m pytest`; Python QA does not fall back to `unittest` when pytest is absent.

QA-01-UI-BOUNDARY verification amendment: run `qa/requirements-architecture.txt` in the isolated guest venv using `python -m pytest -q scripts/test_check_architecture.py`, then `python scripts/check-architecture.py` with the guest Cargo environment. The earlier `unittest` command in the M row is superseded and is not the accepted Python test gate.

QA-01-UI-BOUNDARY review rulings: Cargo dependency kind, `optional` and `target` fields are part of the UI requirement; only a resolved normal, non-optional, unconditional app dependency satisfies it. The declared dependency alias (Cargo `rename`, or the default Rust crate alias) must match the resolved node edge alias and first-party package identity; the resolved UI package ID must identify the workspace member at `crates/slopos-ui/Cargo.toml` or `crates/slopos-appkit/Cargo.toml`, not a same-named third-party package. These first-party manifests and their directory path components must not be symlinks. Declared dependency records and resolved dependency-kind records must contain Cargo's explicit fields and valid types; an omitted `kind` or `target` is not equivalent to explicit `null`. Every workspace member must have a resolution node, and node IDs must reference packages. Every app Cargo target `src_path` must exist and resolve inside that app's package root; external target source files are not permitted in this slice. The checker rejects app-root, app-directory and `Cargo.toml` symlinks, any app-tree symlink escaping `apps/`, and broken/unresolvable app-tree symlinks because its package/source walks do not follow directory links. Raw-UI source checks mask nested comments and Rust string/character literals before matching complete Unicode-aware Rust identifier tokens; Python's Unicode identifier table is an acknowledged implementation approximation to the selected Rust toolchain's table. CLI metadata failures/invalid JSON return 2, architecture violations return 1, and a no-app run returns 0 while explicitly disclaiming app conformance.

For active rows include the concrete branch, worktree, base/prerequisite SHAs, exact write paths, build/runtime permit and evidence/commit handoff. READY means prerequisites are integrated and the task packet exists. Do not mark the table assigned merely because a future agent prompt mentions a role.

## N. Transitional architecture exceptions

These acknowledge existing debt only; they do not permit new ad-hoc UI.

| Existing boundary | Permitted migration use | Owner/removal condition |
|---|---|---|
| `crates/slopos-shell/` legacy GTK UI | Maintain existing behavior; critical truthful-state fixes; incremental extraction | Coordinator until assigned; remove exception per migrated shell module, SHELL-01–05 |
| `crates/slopos-settings/` legacy GTK UI | Maintain donor behavior while real providers/native panels replace it | APP-02; no additional raw-GTK panels |
| `crates/slopos-catalogue/` legacy GTK UI | Preserve AppImage lifecycle during Software migration | APP-05; verify/install new entry point before retiring donor |
| `assets/config/gtk-3.0/` and `themes/` | Existing external-app/Openbox compatibility and donor styling | Coordinator; separate legitimate external themes from obsolete first-party styling |
| PCManFM/external utility integration | Explicitly labelled interim/delegated behavior | APP-01/SHELL-05 or matching utility task; final native claims require the replacement |

New engine/compatibility exceptions must name exact crate/path, API boundary, reason, test and owner here. No blanket exemption for all applications, all GTK imports or all QA failures is allowed.
