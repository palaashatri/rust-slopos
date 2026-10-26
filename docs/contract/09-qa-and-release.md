<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

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

`rust-toolchain.toml` pins Rust 1.99.0, so formatting, Clippy, tests and builds all run with that tested version. Record `rustc -Vv` in release evidence. `Cargo.lock` changes require deliberate coordinator review and a fresh locked build.

## 30.2 Existing QA is evidence infrastructure, not the new gate implementation

| Existing entry point | What inspection establishes | Execution constraint/remaining gap |
|---|---|---|
| `scripts/run-release-qa.sh` | Runs every inventory-listed Cargo and legacy suite unconditionally | Long-running and needs X11 and system state; PASS means every listed command passed. Not the complete §37 gate. |
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

# 34. Documentation and single-source truth

This file contains §0's execution rules, Part I's normative requirements and Part II's descriptive audit/evidence/task ledger. Root and scoped `README.md` files provide truthful usage/API orientation and link here for architecture and readiness.

Do not add competing `TRUTH.md`, `ROADMAP.md`, `PLAN.md`, `STATUS.md`, `AUDIT.md`, `DESIGN.md`, `ARCHITECTURE.md`, per-agent Markdown plans or dated hand-authored reports. The SLOPOS Design Bible is §5.2 of this contract, and its values live in the machine-readable theme datasets (§10). Do not split it into a separate design-bible directory. If a generic agent workflow asks for one, put its plan/decision here through the coordinator. Git history is the archive; do not duplicate old ledgers.

The restriction concerns authored/tracked project documentation. Machine-readable specifications, fixtures, test outputs, PR discussion and temporary orchestration messages are not competing truth sources. Existing QA scripts may emit raw `report.md` files into ignored artifacts; those are execution output, must not become committed alternate ledgers, and cannot redefine readiness. Preserve license notices and upstream attribution even when their format differs from this policy.

Only the coordinator edits this contract during parallel work, unless ownership is explicitly delegated. Each integrated tranche updates Part II in the same PR with source SHA, findings changed, exact command results, VM identity, resource observations, links/hashes for evidence and remaining blockers. Workers supply that information in their handoff.

Keep historical results labelled with their tested revision. Apply §0.7's documentation-only evidence rule instead of demanding an impossible self-referencing commit or relabelling old evidence as fresh. Do not replace failures with scores, silence UNKNOWN fields or copy a PASS to changed code.

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
