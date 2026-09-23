# TRUTH.md — SLOPOS-I Current Audit and Readiness Ledger

**Status:** evidence ledger, not a roadmap  
**Audit date:** 2026-09-24  
**Code baseline audited:** `main@a39dc523526dde0d02736ac29134c6af2cd63d3b`  
**Audit type:** static source audit + canonical Figma metadata inspection  
**Executed build/runtime evidence for this audit:** none  
**Release state:** NOT RELEASE READY  
**Completion state:** NOT COMPLETE

This file records what is currently known about SLOPOS-I. It does not preserve old scores and it does not infer success from file existence.

The engineering contract is `AGENTS.md`. If this ledger and the contract disagree, the contract wins and this file must be corrected.

---

## 1. Audit boundary

The 2026-09-24 refresh audited the current `main` source at:

`a39dc523526dde0d02736ac29134c6af2cd63d3b`

That commit is the merge of PR #10, "Integrate Classic SLOPOS-I UI direction into main".

The audit inspected:

- root workspace metadata;
- `slopos-session`;
- `slopos-shell`;
- `slopos-settings`;
- `slopos-catalogue`;
- X11/Openbox configuration;
- GTK styling;
- global-menu code;
- system-service UI;
- canonical QA scripts;
- the existing reference SVG/JSON;
- current README/AGENTS/TRUTH material;
- package-repository documentation;
- stale historical QA documentation;
- the canonical Figma UI kit metadata.

No SLOPOS code was compiled or executed for this audit.

That is deliberate. The new contract requires all future compilation, execution and visual QA to run inside a Linux VM under UTM, QEMU, VirtualBox or VMware. The current audit environment was not established as that controlled guest.

Therefore all runtime/build assertions remain UNKNOWN until re-tested in a compliant VM.

---

## 2. Current repository shape

The root Cargo workspace currently contains four first-party crates:

- `crates/slopos-session`
- `crates/slopos-shell`
- `crates/slopos-catalogue`
- `crates/slopos-settings`

The workspace currently uses GTK3-era Rust bindings:

- `gtk = 0.18`
- `gdk = 0.18`
- `glib = 0.18`
- `gio = 0.18`
- `gdk-pixbuf = 0.18`
- `pango = 0.18`

X11 integration is based on `x11rb`.

The session architecture remains:

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
  ├─ global menu/system bar
  ├─ launcher/search
  ├─ notifications
  └─ desktop integration
  ↓
slopos-settings / slopos-catalogue / normal X11 applications
```

This architecture remains acceptable for the X11-first SLOPOS-I objective.

Wayland is not part of the current product generation.

---

## 3. Canonical visual reference changed

The previous repository contract treated:

- `qa/reference/slopos-classic-reference.svg`
- `qa/reference/slopos-classic-reference.json`

as the canonical visual target.

That is no longer sufficient.

The canonical reference is now the Classic Macintosh UI Kit (Community) Figma document:

- file key: `LGMlwNCoVdakZxDBvPKg1W`
- root node: `0:1`

The old SVG/JSON may remain temporarily as historical context, but it is not authoritative for exact component geometry or state behavior.

Observed Figma metadata from the current audit includes examples such as:

| Reference item | Observed example |
|---|---:|
| menu bar | 19 px high |
| menu item row | 16 px high |
| Finder large item | 71×44 px |
| Finder large icon | 32×32 px |
| Finder large label region | 71×12 px |
| Regular button | 80×20 px |
| Default button | 88×28 px |
| Secondary button | 80×16 px |

The Figma kit also explicitly enumerates control states and variants, including button Rest/Pressed/Disabled, Action/Hierarchical menu rows, Active/Hover/Disabled menu states and Active/Inactive window-title variants.

A complete machine-readable extraction has not yet been created.

**Current Figma-derived specification status: MISSING**

This is the first implementation gate of the reset.

---

## 4. Visual audit findings

### 4.1 Current visual implementation is not canonically conformant

The current GTK CSS still contains substantial modern styling that conflicts with both the new Figma authority and parts of the existing classic contract.

Observed examples include:

- `.slopos-topbar` minimum height of 26 px rather than the observed 19 px Figma menu-bar example;
- generic buttons with 6 px corner radius;
- menus/popovers with 10 px corner radius and large soft shadows;
- launcher/search surfaces with 12 px radius and large floating shadows;
- notification/alert surfaces with 10 px radius;
- tooltip rounding;
- prior card-like styling still present in the stylesheet;
- soft gradient/shadow treatment inconsistent with a strict atomic classic component system.

Some later rules moved toward hard-edged controls, but the stylesheet is still internally mixed rather than generated from one canonical token/component system.

**Canonical visual conformance: FAIL / redesign required**

### 4.2 Window chrome

Openbox currently uses compact classic-oriented theme rules with:

- 1 px outer borders;
- striped/interlaced active title treatment;
- active/inactive distinction;
- square bordered title buttons.

This is directionally aligned but has not been measured against the Figma window-title components.

**Window chrome exact conformance: UNKNOWN**

### 4.3 Global top bar

The shell has a full-width top bar and no current Application Strip requirement.

However:

- its geometry is not derived from the Figma spec;
- it combines presentation, system actions, menus, dialogs and fallback behavior in a very large module;
- it contains fake system-state text described below.

**Top-bar presence: statically present**  
**Top-bar canonical conformance: FAIL/UNKNOWN pending rebuild and VM evidence**

### 4.4 File browsing

The current product relies on PCManFM with a SLOPOS-specific profile.

The previous ledger already identified generic PCManFM visual leakage. The current audit found no new evidence proving that PCManFM now satisfies the canonical Finder-like density/chrome requirement.

**File-browser functional baseline: delegated to PCManFM**  
**File-browser canonical conformance: NOT PROVEN**

### 4.5 Settings

Settings has moved toward a Control Panels icon-grid presentation, which matches the intended direction better than the earlier modern dashboard.

However:

- several control panels are delegated to external utilities;
- native panel detail views still use generic GTK composition;
- real system state is not consistently wired;
- spacing/control geometry is not Figma-derived.

**Settings concept: directionally correct**  
**Settings canonical conformance: NOT PROVEN**  
**Settings functional truth: contains release-blocking fake state**

### 4.6 Missing required canonical surfaces

The current acceptance reference requires dedicated evidence for compact first-party utility windows.

Current gaps:

- canonical About/System Information surface is not implemented as a proper first-party utility;
- no canonical first-party accessory such as Calculator is currently implemented for acceptance.

A simple message dialog from the Help menu does not satisfy the intended About/System Information surface.

**About surface: MISSING**  
**Accessory surface: MISSING**

---

## 5. Functional audit findings

These are release-blocking because the product contract forbids enabled controls that do not reflect or change real state.

### 5.1 Network panel contains fabricated production state

`crates/slopos-settings/src/panels/network.rs` currently hard-codes example runtime information, including values such as:

- `eth0`;
- `1000 Mbps Full Duplex`;
- `192.168.1.100`;
- `192.168.1.1`;
- `SLOPOS-Fast-5G`;
- `Home-Network-Guest`;
- `CoffeeShop_Free_WiFi`.

The panel also constructs a Wi-Fi switch and Connect buttons without complete real NetworkManager-backed behavior.

This is not acceptable as production UI.

**Network provider: FAIL**

Required remediation: replace sample state with real NetworkManager D-Bus/API state and real connect/disconnect/toggle behavior, or expose an unavailable state.

### 5.2 Top-bar network menu also fabricates state

`crates/slopos-shell/src/topbar.rs` currently contains static menu text equivalent to:

- Ethernet connected on `eth0`;
- Wi-Fi active on `SLOPOS-Fast-5G`.

This is fake state.

**Top-bar network status: FAIL**

### 5.3 Sound panel is only partially functional

`crates/slopos-settings/src/panels/sound.rs` currently creates a synthetic device list such as:

- Default Audio Output;
- Built-in Analog Stereo Speakers;
- Headphones / Line Out;
- HDMI / DisplayPort.

It initializes sample volume/microphone levels rather than reading the actual provider.

On Apply, output volume and mute may be changed through `pactl` or `amixer`, but:

- selected output device is not applied;
- microphone level is not applied;
- microphone mute is not applied;
- displayed initial state is not read from the real audio stack.

**Sound provider: FAIL**

### 5.4 Date & Time needs real state/read-back audit

The panel invokes `timedatectl` for writes, but its UI defaults do not establish that current timezone/NTP/date/time state is correctly read and represented.

The new contract requires real read state and read-back verification.

**Date & Time provider: UNKNOWN / incomplete**

### 5.5 Generic fallback application menus violate the intended menu contract

The top bar has a real GTK/GIO exported-menu bridge.

However, its fallback menus also expose generic Edit commands implemented by injecting:

- `Ctrl+X`;
- `Ctrl+C`;
- `Ctrl+V`;
- `Ctrl+A`

through `xdotool`.

This is not a real application action model and can misrepresent application capabilities.

**Fallback app-menu semantics: FAIL**

Required remediation: only expose protocol-backed application actions or guaranteed shell-owned window actions.

### 5.6 Global-menu implementation is duplicated

The tree contains both:

- `crates/slopos-shell/src/gmenu.rs`
- `crates/slopos-shell/src/menu/gmenu.rs`

with substantially overlapping GTK remote-menu bridge responsibilities.

The contract requires one authoritative implementation.

**Global-menu module ownership: FAIL / duplicate**

### 5.7 Top-bar module is oversized and mixed-responsibility

The audited `crates/slopos-shell/src/topbar.rs` is approximately 1,110 lines and mixes:

- widget construction;
- global-menu state;
- desktop menus;
- system menu;
- audio menu;
- network menu;
- dialogs;
- process spawning;
- session actions;
- fallback app actions.

This structure makes atomic testing and provider/presentation separation unnecessarily difficult.

**Top-bar modularity: NEEDS REFACTOR**

---

## 6. Session and X11 audit

`slopos-session` statically contains bounded restart/backoff behavior for critical children and supervises Openbox, the shell and an optional X11 compositor.

It establishes the X11 desktop environment and restores session components after failure.

The shell contains:

- an `x11rb` event layer;
- EWMH window operations;
- monitor/RandR modeling;
- window state helpers.

The architecture is directionally aligned with the X11-first contract.

However no compliant-VM execution was performed in this audit.

**Static architecture: ACCEPTABLE DIRECTION**  
**Runtime reliability: UNKNOWN**

---

## 7. QA audit

The repository already contains substantial QA infrastructure, including:

- workspace tests;
- Xvfb/Openbox smoke;
- AT-SPI acceptance;
- resolution QA;
- packaging QA;
- installed-VM/media scripts;
- screenshot capture;
- canonical visual capture.

This is useful infrastructure, but it does not yet satisfy the new atomic-conformance model.

### 7.1 Existing visual QA limitation

`scripts/run-canonical-visual-qa.sh` currently:

- builds the workspace;
- starts an Xvfb session;
- launches SLOPOS;
- captures whole-screen screenshots.

It does not by itself prove:

- exact atom geometry;
- every component state;
- pointer state machines;
- text baselines;
- hit rectangles;
- focus behavior;
- system-effect truth;
- Figma-derived per-component conformance.

It also uses Xvfb rather than a real graphical VM display.

Under the new policy it may remain as a secondary deterministic test **inside the Linux VM**, but it does not replace real VM visual acceptance.

### 7.2 Current-commit CI evidence

The GitHub connector returned no combined commit statuses and no workflow runs associated with `a39dc523526dde0d02736ac29134c6af2cd63d3b` during this audit.

That is not proof that CI never ran; it means this audit has no current connector-visible CI evidence for the merge commit.

**Current audited commit CI: UNKNOWN**

Do not reuse CI claims from older commits.

---

## 8. Documentation/noise audit

### 8.1 Historical QA document

`docs/QA_EVIDENCE_2026-08-13.md` refers to obsolete commits, a former `pivot` branch, old hosted runs and an old `78/100` readiness statement.

It conflicts with the current evidence model.

**Action: delete it.**

Historical Git history remains available if archaeology is needed.

### 8.2 Package-repository README

`packaging/repo/README.md` currently claims public repositories exist at `repo.slopos.org` and contains a placeholder-looking signing key ID.

The root README simultaneously says public package repositories do not yet exist.

This is contradictory and unsafe documentation.

**Action: rewrite it to describe repository-generation intent only and explicitly state that no public SLOPOS package repository is currently available.**

### 8.3 Root README

The current README contains a very large gallery of older screenshots and multiple appearance variants.

Those captures predate the strict Figma-atomic contract and can bias future agents toward preserving the wrong styling.

**Action: replace the gallery-heavy README with a concise current-state document.**

Old screenshot files may remain for archaeology but are non-authoritative unless regenerated for the current commit/spec.

### 8.4 Existing reference SVG/JSON

The old hand-built classic reference is now superseded as an exact visual authority.

It should eventually be removed or explicitly migrated into a `legacy` location once the Figma-derived machine-readable specification exists.

Deleting it before a replacement exists would unnecessarily break current QA scripts, so it remains temporarily.

**Status: LEGACY / NON-NORMATIVE**

---

## 9. Current truth by subsystem

| Area | Static state | Current evidence status |
|---|---|---|
| X11-only product direction | explicit | PASS |
| Openbox base WM | present | PASS statically |
| Session supervisor/backoff | present | PASS statically |
| Bottom Application Strip | retired by current contract | PASS statically |
| Figma-derived component spec | absent | FAIL |
| Atomic shared design layer | absent/incomplete | FAIL |
| Canonical top-bar geometry | not Figma-derived | FAIL/UNKNOWN |
| Window chrome exact parity | directionally classic | UNKNOWN |
| Global menu exporter bridge | present | PARTIAL |
| Duplicate GMenu modules | present | FAIL |
| Fake generic Edit fallback | present | FAIL |
| Launcher | present | runtime UNKNOWN |
| Notifications | present | runtime UNKNOWN |
| Settings shell | present | PARTIAL |
| Network Settings | fake state | FAIL |
| Sound Settings | partial/fake state | FAIL |
| Date & Time | write path present | UNKNOWN/PARTIAL |
| Bluetooth | delegated | UNKNOWN |
| Power | delegated | UNKNOWN |
| Displays | delegated | UNKNOWN |
| File browsing | PCManFM-based | visual NOT PROVEN |
| About utility | missing | FAIL |
| Accessory utility | missing | FAIL |
| Catalogue | present | runtime/security revalidation required |
| Accessibility infrastructure | present | current-commit runtime UNKNOWN |
| Multi-monitor model | source present | current-commit runtime UNKNOWN |
| Current canonical VM visual QA | not performed | FAIL/UNKNOWN |
| Public APT/Pacman repository | not established | NOT AVAILABLE |
| Release-ready package/media evidence | not current | UNKNOWN |

---

## 10. Required reset plan

This section records the accepted work sequence. Detailed normative requirements remain in `AGENTS.md`.

### Phase 0 — contract and evidence reset

- make Figma the geometry/component authority;
- remove stale readiness scores;
- delete obsolete QA prose;
- remove misleading package-repository claims;
- enforce VM-only execution;
- establish disk-space safety rules.

**Status: IN PROGRESS on documentation branch**

### Phase 1 — extract the design system

Create a machine-readable Figma-derived specification covering:

- typography;
- patterns;
- colors;
- geometry;
- buttons;
- fields;
- selection controls;
- lists;
- scrollbars;
- menus;
- window chrome;
- file items;
- dialogs;
- utility-window compositions.

Every component is classified REQUIRED / OPTIONAL VARIANT / NOT USED.

**Status: NOT STARTED**

### Phase 2 — atomic UI foundation

Create a shared SLOPOS component/design layer and deterministic atom test harness.

Migrate typography, borders, patterns, icons, focus and state treatment first.

**Status: NOT STARTED**

### Phase 3 — real providers

Replace fake/sample state with tested providers for:

- NetworkManager;
- audio stack;
- timedate;
- BlueZ;
- UPower/logind;
- XRandR;
- keyboard/pointer state.

**Status: NOT STARTED**

### Phase 4 — shell reconstruction

Rebuild:

- top menu bar;
- system menus;
- global app-menu integration;
- launcher;
- notifications;
- dialogs;
- window chrome integration

from validated atoms.

Remove duplicate menu implementation and fake xdotool app actions.

**Status: NOT STARTED**

### Phase 5 — desktop applications/workflows

Bring to conformance and functional completeness:

- file browsing;
- Settings;
- About;
- Calculator or other canonical accessory;
- Software Catalogue.

**Status: NOT STARTED**

### Phase 6 — conformance and VM QA

Inside a Linux VM:

- fmt;
- clippy;
- workspace tests;
- atom/state conformance;
- accessibility;
- X11 integration;
- real graphical-session screenshots;
- 800×600;
- 1280×800;
- 1920×1080;
- 3440×1440;
- integer HiDPI;
- full user journeys.

**Status: NOT STARTED**

### Phase 7 — packaging and release validation

Inside appropriate Linux VM guests and hosted release CI:

- clean package builds;
- install/upgrade/remove;
- live-media build;
- boot;
- session startup;
- checksums/provenance;
- publication only when real infrastructure exists.

**Status: NOT STARTED**

---

## 11. Mandatory VM execution environment

All future code execution must follow `AGENTS.md`.

Summary:

- native host: edit/read/Git/hypervisor management only;
- all SLOPOS code execution: Linux VM only;
- allowed hypervisors: UTM, QEMU, VirtualBox, VMware;
- Ubuntu LTS preferred for general development;
- distribution-specific guests used as needed;
- guest-local clone for builds;
- no `target/` or build caches on host shared folders;
- real graphical X11 VM session required for primary visual QA;
- Xvfb/container QA is secondary and runs inside the VM only.

No current compliant VM run is recorded in this ledger yet.

---

## 12. Disk-space evidence policy

No heavy build should start until free space is checked.

Routine development target:

- host free space >= 20 GiB;
- guest free space >= 12 GiB.

Release/media target:

- host free space >= 30 GiB;
- guest free space >= 25 GiB.

Future executed audit entries should record:

- hypervisor;
- guest distribution/version;
- virtual-disk size;
- host free space before run;
- guest free space before run;
- relevant `target/` and artifact sizes;
- cleanup performed.

The project must not solve disk pressure by deleting unrelated host data.

---

## 13. Readiness scoring policy

There is currently **no readiness score**.

A numeric score can obscure critical failures by averaging them with unrelated successes.

The reset therefore uses hard gates:

- REQUIRED atom coverage;
- exact geometry/state conformance;
- real functional behavior;
- real provider state;
- VM runtime evidence;
- accessibility;
- packaging/release evidence where applicable.

A future supplementary visual score may be recorded for human comparison, but it cannot override a failed required atom or functional gate.

---

## 14. Current blockers

The current blocking set is:

1. Figma-derived machine-readable design specification does not exist.
2. Current UI is not atomically derived from the Figma reference.
3. Current styling contains modern rounded/shadow/card drift.
4. Network Settings contains fabricated production state.
5. Top-bar network status contains fabricated production state.
6. Sound Settings contains fabricated/partially disconnected state.
7. Date/time real-state/read-back behavior is not proven.
8. Generic xdotool Edit fallback misrepresents application capabilities.
9. Global-menu bridge code is duplicated.
10. File-browser visual conformance is not proven.
11. Canonical About utility is missing.
12. Canonical accessory utility is missing.
13. Current-commit build/test evidence has not been produced in a compliant Linux VM.
14. Current-commit graphical visual QA has not been produced in a compliant Linux VM.
15. Public signed SLOPOS package repositories are not established.
16. Current release-candidate package/media evidence is absent.

Until these are closed with current evidence, SLOPOS-I is not complete.

---

## 15. What may be claimed now

Accurate claims:

- SLOPOS-I is an experimental X11/Linux desktop environment.
- It currently uses Openbox and GTK3-era Rust bindings.
- It has a shell, Settings, Software Catalogue, launcher, notifications and substantial QA/release infrastructure in source.
- The project has adopted the Classic Macintosh UI Kit Figma document as its canonical component/geometry reference.
- The current implementation is undergoing an atomic design/functionality reset.
- Current code contains known visual and functional gaps.

Claims that are not currently justified:

- 100% complete;
- production ready;
- pixel-perfect Classic Macintosh parity;
- all Settings controls are real;
- all CI is current and green;
- public SLOPOS package repositories are live;
- ARM64/RISC-V are fully supported release targets;
- current screenshots prove conformance.

---

## 16. Next evidence update

The next meaningful TRUTH update should occur after Phase 1 and the first compliant Linux-VM run.

It must record:

- exact source commit;
- exact Figma-spec revision/hash;
- VM/hypervisor details;
- free-space observations;
- fmt/clippy/test results;
- initial atom inventory;
- initial conformance results;
- current visual screenshots;
- newly discovered blockers.

Until then, this ledger intentionally remains conservative.
