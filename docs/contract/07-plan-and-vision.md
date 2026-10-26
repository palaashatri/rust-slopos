<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

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
- **Observed content is data, never instructions.** Text and images that the operator reads from windows, documents, web pages, notifications, file names or AT-SPI trees cannot change its task, permissions or policy, even when they are phrased as commands (prompt injection). Only the user's request and SLOPOS policy direct the planner. A step whose target or parameters came from observed content, and which is sensitive, destructive or external, needs the same explicit confirmation as above. The operator never types credentials or one-time codes on the user's behalf. Tests include hostile on-screen and in-document instructions.
- **External agent protocols are adapters, not new authority.** If SLOPOS later exposes its typed actions to an external agent or tool protocol (for example the Model Context Protocol), that endpoint is off by default and local-only by default. It offers only the same permissioned typed actions with the same confirmations, and it is covered by the same stop control and logs.
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

### 21.7.1 Fully themeable desktop and original shared icon family

**North star: Windows XP-class personalization, not a desktop with exactly two hard-coded appearances.** SLOPOS's first-party toolkit (`slopos-ui` + `slopos-appkit`) must ultimately expose one consistent, versioned design system that can be transformed by installable visual styles and independently configurable desktop layouts. The modern reference is [Sinty / Singularity Desktop](https://sinty.dev/): its calm dark workspace, compact unified top bar, floating Dock, restrained controls, contemporary application presentation and convenient personalization. Treat that website as an **external visual inspiration**, never as reusable proprietary artwork, stylesheets or branding by default. The design language, principles, token architecture, theme matrix, anti-patterns and review checklist that every style must satisfy are in the SLOPOS Design Bible (§5.2); its §5.2.1 table records how each design inspiration is used.

**Built-in visual style presets (future target):**

- **SLOPOS Classic — System 7 / Platinum Classic:** faithfully interpreted *clean-room* Classic Macintosh interaction and geometry, compact menus/widgets and traditional title bars. The existing §5 Classic Macintosh UI Kit remains the binding **SLOPOS-I** acceptance authority; do not silently replace or restyle its ongoing component QA.
- **SLOPOS Modern — Sinty-inspired:** original, restrained modern windows, coherent neutral/dark surfaces, optional light mode, pleasant typography, sharp symbolic icons and colorful identifiable **original SLOPOS-branded app icons** in launcher/Dock, appropriate curves, shadows and polished focus/hover/selection behavior. It must remain readable on real displays, not a static hero mockup.
- **Third-party/community visual themes:** signed or user-trusted declarative theme packages with live preview, import/export, theme selection, restore defaults and rollback. A theme may vary control rendering substantially while preserving command semantics and accessible behavior. Optional variants (e.g. Loom-inspired) are not separate toolkits.

**Separate four customizability layers rather than conflating them:**

1. **Visual theme:** versioned semantic tokens and style recipes for typography, surfaces, contrast, geometry, density, focus, window controls, menus, buttons, scrollbars, dialogs, elevation, motion, light/dark, wallpaper accents and supported sound effects. Style recipes may redefine control appearance only within validated layout, hit-target, a11y and performance constraints. The mechanism must not run untrusted code or arbitrary theme-install scripts.
2. **Desktop layout/interaction profile:** independent, persistent configuration of top menu/global menu, classic panel, floating Dock vs combined taskbar, panel alignment/size/visibility, app launcher/search, desktop widgets, workspace behavior, window button position and user-selectable hide/dodge/autohide. A Classic-style window appearance should be usable with a Modern-style Dock layout and vice versa, unless a documented capability constraint forbids a combination.
3. **Icon packs and app identity:** distinct themeable icon roles: (a) *symbolic*, monochrome foreground-adaptive 16/20/24/32 px menu/sidebar/settings/search/status glyphs; and (b) *brand/app icons*, recognizable, mostly colorful standalone 32/48/64/128+ px scalable identity artwork for Dock, launcher, app grid, file associations and Software. Keep stable app IDs and icon roles regardless of packs. Missing/inaccessible icons get consistent semantic fallback. Users may override packs without replacing code. Original default brand icons are a **future design deliverable**, not permission to copy Singularity or commercial app artwork. Verify legal rights for every font, logo, asset and redistributed icon.
4. **Personal preferences and accessibility:** fonts/scale, theme accents, cursor styles, sounds, reduced transparency/motion, high contrast, light/dark scheduling, display-specific scaling and keyboard/focus conventions. Persistent per-user settings and defaults survive version upgrades and shell restarts; visual overrides cannot disable required focus, contrast or security indicators.

**Symbolic icon reference assets added with this proposal:** `assets/icons/symbolic/app-symbols.json` maps the 12 pictured default SLOPOS application list entries—Files (folder), Terminal (terminal), Control Panels (settings), Software (package), System Monitor (activity), Screenshot (camera), Image Viewer (image), Calculator (calculator), Notes (notebook-pen), Downloads (download), Dictation (mic), Intelligence (brain-circuit)—to real pinned Lucide SVG glyphs under `assets/icons/symbolic/lucide/`. These are **licensed third-party symbolic prototypes**, not custom-created SLOPOS artwork, not the colorful Sinty Dock icon artwork, and **not yet wired into the running desktop**. Retain the included ISC/MIT upstream license and `NOTICE.txt`. The canonical colorful brand-icon set must be individually designed/reviewed later, with original, coordinated silhouettes, palette, optical sizes, accessibility and visual QA. This reference subset does not license copying Sinty's icons.

**Toolkit and cross-platform contract:** only shared theme/style and appkit interfaces control first-party app surfaces; no app-specific restyling, hardcoded bitmaps, or duplicate shell theme logic. Per-platform display/input/compositor backends remain isolated. Third-party GTK/Qt/Slint/Electron/Wine apps receive best-effort portal/style/icon bridges where implemented, **not** false claims of exact cross-toolkit theme parity. Loom stays Rust + Slint; expose an explicit token/chrome adapter instead of forcing a rewrite. On future PARAMA, the same SLOPOS themes, layout profiles, app IDs and icon semantics are hosted under SURYA through a native adapter, without a second PARAMA theme engine.

**Future quality gates (FUT-UI, not current SLOPOS-I release gates):** a machine-readable schema for safe declarative packages and theme/layout/icon separation; schema validator and malicious-theme rejection; component gallery and pixel/geometry comparisons at 1×/2×/mixed DPI; both built-in themes and at least one third-party theme; switching theme and layout independently, including combinations, with no restart or data loss; same symbolic/brand icons at multiple sizes and high-contrast fallback; minimum three native apps + Dock/menu bar/Settings; keyboard/AT-SPI focus invariants, contrast/reduced motion, login/restart persistence, bounded frame latency, revert-on-failure and platform-specific visual evidence. All unimplemented requirements remain clearly PLANNED.

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

### 21.7.5 SLOPOS is PARAMA's intended eventual desktop environment

**Architecture goal: SLOPOS is the intended complete PARAMA desktop, while SURYA remains PARAMA's graphics/compositor authority.** This replaces the idea of a second, separate production desktop UI in PARAMA, **not** the minimal demonstration shell required to validate PARAMA's own M7 GPU/input/supervision gate. It is a future portability target, not an assertion that PARAMA can run Linux, SLOPOS or Loom today.

- **SURYA** owns surface composition, input dispatch, focus, window registration and capability-mediated screenshot/remote-control services. **SLOPOS** owns the shared UI/theme framework, shell, Dock, desktop profiles, menus, launcher, Settings, notifications, appkit and first-party desktop interaction. One compositor only: default proposal is a capability-restricted SLOPOS shell as a SURYA client; integrating SLOPOS compositor technology into SURYA would require an explicitly reviewed alternate ownership/ABI decision.
- On **Linux now**, SLOPOS-I is Rust + X11/Openbox; a later Wayland backend/compositor has a separate gate (§21.7.4). PARAMA-specific services, processes, storage, graphics, security, fonts, input and clipboard require an independently proven, versioned `PlatformBackend`; GTK/Linux kernel/Wayland/Wine interfaces cannot be presumed portable.
- SLOPOS's **Classic**, **Sinty-inspired Modern**, future community themes and independent desktop-layout profiles should use the *same semantic theme and icon manifests* across Linux and PARAMA. The native renderers and capability adapters differ. PARAMA must not maintain a competing full theme engine or second native app UI library.
- PARAMA must verify ring-3 apps, isolation, capability IPC, native executable loading, user-mode graphics/input and stable SURYA/GARUDA restart (M3–M7). Storage, networking, drivers and robust update/recovery are additional prerequisites for general-purpose use. Third-party application and Loom compatibility require separate tested runtimes, not branding or screenshots.
- SLOPOS Vision and Intelligence run only under revocable, scoped user privileges; user-installed themes and icon packs are data, not executable system authority.

**Acceptance:** boot a real PARAMA VM, launch a SLOPOS component sample as an independently isolated user process rendered through SURYA, exercise keyboard/pointer/focus/denied capability and restart behavior, demonstrate Classic and Modern token states plus theme rollback; compare to the Linux reference only on implemented features. Still not a gate for SLOPOS-I or PARAMA M0–M10.

### 21.7.6 Future platform backlog and sequencing

The above initiatives form a **coordinated long-term product vision**, not a single giant implementation milestone. First make the present desktop truthful and usable, then add cross-app integration and ML, and pursue compositor/PARAMA experiments in independently guarded tracks. Useful additional goals retained from prior product discussions include full native app consistency, modern global search, on-device text/vision flows, live wallpapers, reliable screenshot/recording, customizable window buttons, robust gaming/fullscreen behavior, HDR/VRR, multi-display ergonomics and eventually privacy-conscious device/Android interoperability. Each must receive an explicit scope, dependency list, test fixtures and current evidence before any readiness claim.

| Future ID | Area | Prerequisite and proposed verification |
|---|---|---|
| FUT-UI | Extensible XP-class theming and native toolkit | Shared tokens, installable safe visual themes, independent shell layouts, brand/symbolic icon packs, UI/AT-SPI QA and native app consistency |
| FUT-LOOM | Loom desktop adapters | Existing Loom native Linux builds and SLOPOS services; real Writer/Sheets/Present workflow in a graphical Linux VM |
| FUT-VISION | Offline vision/ML system | Permissioned capture, model provider, image/document handoff; OCR, subject copy and local-inference QA with privacy checks |
| FUT-COMP | Own compositor + graphics | Proven X11 baseline, dedicated compositor design/VM/hardware gate; working Wayland session, XWayland, HDR/VRR on capable displays |
| FUT-PARAMA | SLOPOS as intended PARAMA desktop | Independent PARAMA Ring 3, capabilities, drivers, SURYA and loader milestones; single compositor owner, sandboxed SLOPOS native sample with shared themes |

Do not promote **FUT-*** into SLOPOS-I's required §37 acceptance or source-code task backlog automatically. Review the scope and platform compatibility with the owner and independently record evidence before scheduling any feature implementation.

**Counterpart gates in the sibling repositories.** These IDs only point across projects. Each project owns and evidences its own gate, and passing one never passes another.

| SLOPOS ID | Loom counterpart (`rust-loom/AGENTS.md`) | PARAMA counterpart (`parama/AGENTS.md`) |
|---|---|---|
| FUT-UI | LOOM-SLOPOS-03: Slint semantic-token and icon-role mapping | §5.1 shared-presentation rule; future additional design gate |
| FUT-LOOM | §15 LOOM-SLOPOS-01, -02 and -04 | §5.1 PRM-SLOPOS-04: Loom portability is separate work |
| FUT-PARAMA | §15 last bullet: no PARAMA Loom target is assumed | §5.1 PRM-SLOPOS-01 to -04 |
| EDGE-03 | §17 LOOM-EDGE-01 to -03 | — |
| EDGE-04 | §17 LOOM-EDGE-04 | §5.3 ARM-04 |

---

## 21.8 Platform compatibility, reuse and evidence discipline

**Applies immediately to newly scoped work; not a claim that the targets below are implemented.** This discipline takes lessons from experimental Windows compatibility runtimes such as [Raven](https://useraven.org/news/introducing-raven/), without making Raven a required dependency or replacing the release gates in §21.5 or §37. Preserve SLOPOS-I's Linux, X11/Openbox, Rust, UI-first and VM-only constraints (§3, §28). The future SLOPOS compositor and PARAMA compatibility remain separate, unapproved implementation tracks.

### Own differentiated experience; reuse proven infrastructure

1. Before writing a substantial new engine, protocol, driver or platform subsystem, document the exact missing capability, evaluated upstream alternatives, the technical reason each is insufficient, ABI/support burden, license and update strategy, and the **smallest replaceable adapter** needed. Prefer tested Linux services, DRM/Mesa, text/graphics/IME/accessibility infrastructure, portals and established application runtimes where their contracts suffice. No automatic "rewrite everything" goal.
2. Keep SLOPOS-owned typed interfaces for launch, windows/surfaces, file/URI associations, notifications, menus/actions, accessibility, capture and input permission, theme tokens and provider-backed state. Adapt upstream technology behind these contracts; do not leak vendor or window-server details into native apps. One implementation must own each system truth or command.
3. Compatibility is an **end-to-end user promise**, not evidence that a process launches. Install, discover, open, interact, persist, update, uninstall, and recover from failure using the actual applications and desktop UI. Define which guarantees cover native SLOPOS, GTK/Qt/Slint, Electron, AppImage, Wine/Proton, and—only if independently demonstrated—alternative Windows runtimes.
4. **Windows applications:** on the existing Linux session, prefer Wine/Proton as the maintained default and test launchers, file associations, installation/prefix isolation, per-application configuration, restore/recovery and honest security boundaries. Keep a proposed **runtime-neutral launcher/manager** behind typed capabilities. Raven is a research candidate, not a shipped or assumed X11-compatible backend; establish its current host and display requirements in an isolated experiment before claiming support. Avoid developing a second PE/NT implementation inside SLOPOS.
5. **Hardware:** on Linux, inherit graphics/input/audio/network drivers from the supported Linux kernel/userspace stack; SLOPOS does not "port drivers" to be a DE. A future custom compositor must consume real DRM/KMS, Mesa and driver capabilities with separate physical-hardware verification (§21.7.4). PARAMA driver adaptation belongs in PARAMA's own gates, never silently in a Linux-DE milestone.

### Compatibility proof and comparative measurements

A compatibility task must declare a **bounded matrix** rather than claim "all apps work." Store the active matrix and its actual test evidence in the existing QA/evidence inventory (no competing Markdown status documents). At minimum, each target records:

| Field | Required evidence |
|---|---|
| Subject | App/runtime version, distro/kernel/session, display server/WM, hardware or VM configuration, relevant drivers, build SHA and test date |
| Lifecycle | Install → launch → UI interaction → save/exit → relaunch → uninstall/rollback where applicable |
| User interactions | Real keyboard/mouse, accessibility, menus/clipboard/drag-drop, dialogs, window geometry, scale and focus |
| Outcomes | `PASS`, `FAIL`, `BLOCKED` or `NOT_RUN` **per stage**, exact observation and logs/screenshots/traces; `PASS` never means a lower stage alone worked |
| Negative cases | Missing dependency, bad installer/input, permission denied, unavailable service, crash/restart, read/write failure and cleanup |
| Regression | Deterministic test seed/fixtures and rerun instructions, plus separate real-app graphical VM and physical-hardware evidence as relevant |

- Initial illustrative workload classes: native SLOPOS Files/settings/screenshot flows; ordinary GTK and Qt editors/file dialogs; a Slint/Loom app once available; a Windows native-API utility and installer under Wine; a game or GPU-intensive workload only with documented graphics hardware and driver support. **Do not assert any of these already pass.**
- For performance comparisons, pin the runtime/app binary, platform configuration, rendering mode and workload; capture baseline and candidate with multiple warm/cold runs, distributions (at least median and a tail statistic), peak memory, startup latency and failures. Include GPU/CPU utilization where meaningful. Do not generalize a single benchmark or a smaller compatibility surface into "faster than Wine/KDE/GNOME."
- A screenshot proves only the captured frame; a process listing proves only process existence; a test double proves only its stated contract. Use separate build/unit, protocol/integration, actual UI journey, recovery, hardware and install/remove gates. Keep failing results visible.

### Dependencies, licensing and AI-agent review

- Keep per-component provenance (source URL, exact version/commit, hash where distributable, license, redistribution obligations and update owner). Check licenses at source-file, library, model/asset and linked/distributed-artifact levels, not just the upstream website banner. GPL-licensed driver/runtime components require a reviewed architecture and redistribution decision; **no unreviewed code import or automatic change to SLOPOS's MIT license**.
- A coding agent must identify the authoritative abstraction and existing implementation first, propose the smallest coherent change, enumerate affected contracts and security boundaries, and provide independent tests, failure traces and reviewable diffs. Do not accept a generated checklist, stub, sample screenshot or unconditional success message as feature proof.
- Every task handoff and PR must state **scope, non-goals, dependencies reused, compatibility matrix rows exercised, exact commands/environment, observed failures, resource/benchmark data when relevant, license impact, and remaining `NOT_RUN` gates**. Keep current readiness in Part II; no separate truth file and no changing release gates to conceal a defect.
- Cross-project experiments with Loom or PARAMA must preserve their independent ABI/design requirements and roadmap locks (§21.7). Future optional runtime work starts only after explicit owner-scoped authorization and a portable interface design.

---

### 21.8.1 Lessons from Vinix and VitruvianOS — compatibility and low-latency integration

**Research direction only; no upstream source has been imported.** Sources to review: [Vinix](https://vinix-os.org/) (independent kernel with Linux/Alpine userspace binary-compatibility goals) and [VitruvianOS](https://v-os.dev/blog/) (BeOS-inspired Linux desktop with kernel/user-space integration). Check exact upstream commit, implementation and license before any dependency is added. These are complementary design examples, not proof that their stated compatibility or memory benchmarks generalize.

- **VitruvianOS lesson: make the desktop coherent without replacing Linux.** Keep SLOPOS Linux-facing APIs abstract and make system-service extensions through stable, permissioned user-space adapters. Prefer existing Linux logind/systemd --user, D-Bus, XDG portals, inotify/fanotify and extended attributes over a bespoke privileged kernel module unless an independently measured gap justifies one. Use one provider for each capability.
- **Janus-inspired session supervision, without copying Janus:** define a deterministic SLOPOS session lifecycle: authenticated login/seat, managed X11/Openbox or future compositor, session services, shell/application startup, health/restart, VT/seat handoff, orderly logout and resource cleanup. Validate display-manager and greeter paths equivalently. Reuse logind/libseat and user service managers where appropriate; no duplicated privilege escalation.
- **Metadata-aware Files:** optional typed file tags and user-controlled metadata queries using supported filesystem attributes plus bounded, resumable indexing. Never replace the filesystem or assume all mounts support xattrs. Disable or throttle indexing on Pi Zero 2 W/microSD; basic Files must work without a daemon, database, network or full-tree scan.
- **Unified media capabilities:** prefer PipeWire for capture/playback, GStreamer/FFmpeg where a verified adapter fits, and shared permissions for media conversion/transcription across SLOPOS and Loom/ClipKit/JWhisper/JVosk. Don't fork codecs just to share branding; keep runtime dependencies lazy and optional.
- **Vinix lesson: userspace compatibility and resource discipline matter.** SLOPOS on Linux already has Linux ABI support; do not reimplement it here. Make ELF native apps and Wine/Proton launch/install/persistence/rollback reliable through a test matrix. Test idle footprint and responsiveness of the **entire** desktop plus representative apps, not just one shell process; a lightweight shell does not guarantee a heavyweight suite can fit 512 MB.

**Future experimental gates (not current implementation authorization):** `INT-SESSION` real login/suspend/restart/VT/shutdown failure tests; `INT-META` metadata on ext4/xattr-capable and unsupported mounts with privacy/performance controls; `INT-MEDIA` one genuine audio/video capture-to-native-app workflow; `INT-COMPAT` real native and Wine install/launch/save/restart/uninstall comparison. Store evidence in existing Part II instead of a new truth document.

### 21.8.2 Curated first-party application policy — owned sources only

**First-party means independently authored/maintained SLOPOS ecosystem software with audited code/asset provenance, not merely a repository with `fork=false`.** Only owner-authored projects may be branded/curated as SLOPOS/PARAMA first-party applications. A GitHub fork or unreviewed third-party import may be used under its license as an *external dependency* or optional third-party package, never represented as original first-party software. Before inclusion, confirm source lineage, licenses, contributor notices, bundled models/media and redistributability. Retain project autonomy and upstream identities; SLOPOS provides desktop integration, not forced toolkit rewrites.

**Eligible candidate repositories, all still subject to real packaging, supported-host and owner-authorship reviews:**

| Candidate | Intended role | Edge/device policy |
|---|---|---|
| [Loom](https://github.com/palaashatri/rust-loom) | Writer/Sheets/Present productivity suite | Separately installable; small-document profiles; no default 512 MB preload; preserve Rust/Slint |
| [ClipKit](https://github.com/palaashatri/rust-clipkit) | Downloads/media tasks | Streaming transfers, bounded queues, adapter to existing ffmpeg; no forced media runtime at idle |
| [JWhisper](https://github.com/palaashatri/jwhisper) / [JVosk](https://github.com/palaashatri/jvosk) | Transcription/dictation engine providers | Small-model and streaming profiles; per-model memory preflight, optional disabled/offloaded work with informed consent |
| [JForge](https://github.com/palaashatri/jforge) / [PyForge](https://github.com/palaashatri/pyforge) | Local image/ML studios | Optional installs; no generative model loaded by default on 512 MB; opt-in remote only, never silently |
| [Donkey](https://github.com/palaashatri/donkey) | Reverse-engineering workbench | Optional developer app; lazy disassembly/indexing and bounded decompiler jobs |
| [ICStudio](https://github.com/palaashatri/icstudio) | EDA workbench | Optional professional app; lazy schematic rendering/simulation, cache caps |

**Not first-party desktop candidates (even as optional catalog entries):** [JTorch](https://github.com/palaashatri/jtorch) remains an independent Java tensor/autograd **library**; [Benchmarks](https://github.com/palaashatri/benchmarks) remains an independent JVM benchmarking/QA **tool**. Do not add branded launcher icons, first-party app entries, SLOPOS/PARAMA-specific integration milestones or default runtime dependencies for either. Their normal source distributions may remain usable on any independently supported platform, and their Pi Zero 2 W resource-efficiency goals remain valuable. An actual future GUI product would require a separate owner decision and full app acceptance; a library's demos or a CLI do not automatically qualify it.

**Explicit fork exclusions:** [Tricycle](https://github.com/palaashatri/tricycle) is a fork of `kmcclive/tricycle`; [texture-auto-upscaler](https://github.com/palaashatri/texture-auto-upscaler) is a fork of `veryCoolTimo/texture-auto-upscaler`. **Neither is an owner-authored first-party bundle candidate.** Avoid duplicate brand/product slots for their features; choose authored ClipKit's existing FFmpeg adapter for media tasks, and separately review licensing for any permitted upstream reuse. Other verified original projects may be proposed later through the same policy, never added to this list by name similarity alone.

**Distribution tiers:** (1) `core-minimal`: only fundamental SLOPOS shell, settings, files and recovery tools; (2) `first-party-catalog`: user-selectable supported applications, no implicit background services; (3) `desktop-full`: convenience meta-package on sufficiently capable hardware, subject to image size/RAM and opt-in review. Only apps proven functional and compatible are installable, and no unsupported PARAMA executables or 512 MB-heavy model stacks may be advertised as ready.

**Platform targets:** Linux/SLOPOS is the first real integration target. **PARAMA/SURYA is a future source-port/backend target** contingent on user processes, ABI, graphics, input, filesystem, security and a compatible runtime/toolkit. A Java Swing app implies future Java/JDK/AWT/Swing port; Python/Diffusers implies future interpreter/native dependency/graphics port; Rust/Iced/Slint implies platform backends. A written roadmap is not binary compatibility. Desktop entry, file dialogs, theme/icon roles, menu integration, accessibility and permissioned document actions must be tested per actual host.

**Acceptance:** build/package provenance, real launch+file/clipboard/menu flow and failure recovery under Linux SLOPOS; low-memory/offload profile as relevant; no bundled binary unless the exact platform/runtime is proven; first-party curated source inventory must pass a fork/original-asset/licensing audit. Track readiness without claiming shipped support.

---

## 21.9 Edge-first efficiency and Raspberry Pi Zero 2 W reference target

**Status: FUTURE PORTABILITY / PERFORMANCE GOAL, NOT VERIFIED SUPPORT AND NOT AN AUTOMATIC SLOPOS-I RELEASE BLOCKER.** The Raspberry Pi Zero 2 W is the deliberate **floor-reference efficiency device** for SLOPOS and its optional applications, not the only supported computer. Hardware reference: 4× 1 GHz Cortex-A53 (ARMv8-A/AArch64), **512 MB LPDDR2 shared with graphics**, VideoCore IV GPU, microSD I/O and 2.4 GHz Wi-Fi. The official hardware specification must be used rather than assumptions based on modern Pi 4/5 hardware: https://www.raspberrypi.com/documentation/computers/zero.html. Inference, driver and framework compatibility on this board require actual investigation; running a CI build or AArch64 VM is *not* proof of board compatibility.

### User-visible performance contract

- **Responsive small work:** ordinary desktop navigation, app menu operations, typing, window switching, settings changes, file browsing, opening a small note and calculator tasks must feel interactive with no UI-thread waits on network, disk, parsing, codecs, model loading or D-Bus. External network and storage latency may increase *completion time* but must not freeze unrelated input or misrepresent pending state.
- **Graceful large work:** batch operations, large archives/files, long transcoding, scans and AI jobs may take longer, but must stay bounded, cancellable, progress-reporting and recoverable. Streaming, bounded queues, incremental parsing/painting and backpressure are preferred over materializing entire large datasets. Distinguish cancellation from successful completion.
- **Feature/capability tiers:** a **minimum edge session** includes shell, Files, Control Panels, terminal, calculator, notes and basic services with a simple visual theme; a **standard session** may add richer media and document functionality as separately measured; **accelerated/high-end features** (HDR/VRR, animated wallpapers, demanding local ML, heavy app workloads) are optional and never start expensive background daemons by default on edge hardware. Give truthful unavailable/degraded explanations and permit remote/offloaded jobs only with explicit user consent. Core offline navigation must work without an AI model or network.
- **Implementation mechanisms:** lazy application/services startup; task-local bounded worker pools; memory-mapped or streamed data only where correct; texture/thumbnail/icon caches with explicit upper bounds and eviction; window/damage-based redraw; avoid idle animations, polling and excessive compositing; use efficient icon assets and lightweight fallbacks for blur/shadows. Do not replace a proven system library with homegrown code without benchmark and maintenance evidence (§21.8).
- **Architecture/platform:** SLOPOS-I remains Linux/X11/Openbox (§3). Research a **Linux AArch64** armv8 baseline (armhf optional if dependency/API and ABI tests justify it), cross-build and package dependencies, actual kernel GPU/display/graphics stack, and GTK/X11 functionality. Future Wayland and PARAMA ARM64 are independent gates. Don't label a Pi-compatible CPU binary a supported device without real display/input/audio/network integration.

### Instrumented edge benchmarks and provisional budgets

First record a clean **actual Pi Zero 2 W baseline**: OS image/32- or 64-bit ABI, kernel, Xorg or other session, storage type, resolution (begin with **1280×720**, then test 1920×1080), GPU memory reservation, CMA, swap/zram status, services, device clocks, temperature/throttling and free memory. Run both cold and warmed cases over repeated trials; keep machine-wide and process-tree PSS/RSS, `MemAvailable`, swap pressure, CPU usage, UI event latency, frame pacing, disk I/O and energy if measurable. VM/emulation results are separate evidence with limitations.

**Initial design targets only—subject to actual hardware calibration, not claimed passes:**

| Scenario | Provisional quality target |
|---|---|
| Minimal idle desktop | **≥128 MiB `MemAvailable`** with all required edge-session services running at 720p, on the declared OS image; stable steady-state idle CPU without unnecessary high-frequency timers; distinguish kernel/cache accounting from per-process PSS. |
| Local input/menus | Keyboard input/visible state **p95 ≤150 ms** and ordinary menu invocation **p95 ≤250 ms** during idle/light load, separately report worst-case and blocked operations. |
| Window/frame responsiveness | Damage-driven rendering, no continuously rendered frames while visually idle; prefer **30 fps or better when interaction warrants it** at 720p if the actual driver permits, with honest frame-time distribution instead of an empty animation benchmark. |
| Memory pressure and reliability | No application/system OOM, unbounded resident-growth loop, uncontrolled swap storm or unsaved-data loss in the declared small-workload suite; demonstrate cancellation/clean restart under constrained memory. |
| Heavy/unsupported operation | UI remains usable, task can be cancelled or safely offloaded, and the exact local CPU/GPU/model/codec or dependency limitation is reported. No fabricated "fast" or "supported" status. |

These are **aspirational candidate limits** until the hardware baseline and user-approved budgets are recorded. Missing hardware measurements remain `NOT_RUN`. Never weaken safety, output correctness, keyboard/accessibility, data integrity or a reproducible failure to meet a speed target.

### Future acceptance backlog

| ID | Depends on | Evidence to collect |
|---|---|---|
| EDGE-01 | QA-01, platform boot baseline | Real Pi Zero 2 W system inventory, instrumented reference workloads, memory/CPU/frame traces, temperature/power conditions and file/ABI/toolchain matrix. Record inability to test as `BLOCKED`, not pass. |
| EDGE-02 | Minimum shell/apps, EDGE-01 | Run real Files/Settings/Calculator/Notes, menu/Dock and lifecycle journeys within validated memory/interaction budgets. Verify idle, repeated launch, low-storage and cancellation/failure cases. |
| EDGE-03 | Optional media/Vision/Loom adapters | Capability-aware feature loading, small real document/media handoff, absence of idle ML/model loading and safe offload/unsupported cases under 512 MB. Report per-app compatibility separately. |
| EDGE-04 | Future compositor or PARAMA backend as applicable | Repeat hardware tests on the relevant *actual* backend; never transfer X11 performance or emulator success to Wayland/PARAMA by assumption. |

For *all future first-party apps*, require an **edge profile** (memory budget, normal startup and steady-state measurements, idle CPU/polling, no-network behavior, bounded input/file work, cancellation), plus a justified exception matrix where hardware or format constraints make some features unavailable. Optimize after profiling and preserve the modern and Classic design system/feature contracts; edge mode is an implementation capability tier, not a separate ugly, inaccessible product.

---

## 21.10 2027 readiness baseline — platform, toolkit, compatibility and supply chain

**Status: review recorded 2026-10-25. The facts below are dated and must be re-checked before each release.** This section does **not** reopen the settled SLOPOS-I decisions (X11 + Openbox, Rust, Platinum Classic, UI library first). It records what changed around them, what SLOPOS must do now to stay buildable and honest in 2027, and when the owner should be asked to revisit a decision. All `Y27-*` items are `PLANNED` until evidence exists in Part II, except `Y27-SUBSTRATE`, which the owner decided on 2026-10-25 (§21.10.2).

### 21.10.1 Platform landscape (as of 2026-10)

| Fact | Effect on SLOPOS-I |
|---|---|
| GNOME 50 removed GNOME's own X11 session. GDM can still start *other* desktops' X11 sessions through a per-user X server. ([GNOME blog](https://blogs.gnome.org/alatiera/2025/06/08/the-x11-session-removal/)) | The display-manager login path (F-PKG-01) must be tested against current GDM, LightDM and SDDM. A GDM that starts SLOPOS's X11 session is evidence for that GDM version only. |
| KDE Plasma 6.8 (October 2026) supports only Wayland sessions; KDE supports the Plasma X11 session only into early 2027. ([Phoronix](https://www.phoronix.com/news/KDE-Plasma-68-Wayland-Exclusive)) | Expect distributions to put less effort into Xorg server packaging and testing. |
| GTK 4 deprecated its X11 backend in 4.17.4 and plans to remove it in GTK 5. ([Phoronix](https://www.phoronix.com/news/GTK-X11-Now-Deprecated)) | GTK 4 does not secure SLOPOS-I's X11 future. `slopos-ui` already draws with its own stack (§21.10.2); GTK stays only in legacy donor crates and engine adapters (VTE, WebKitGTK). |
| The gtk3-rs crates (`gtk`, `gdk`, `atk` 0.18, …) carry RustSec "unmaintained" advisories (RUSTSEC-2024-0411 to -0420). Some were later withdrawn when the repository was briefly unarchived, so their status is disputed. ([RustSec](https://rustsec.org/advisories/RUSTSEC-2024-0411.html)) | The Dependabot bump of `glib` to 0.20 cannot build against `gtk` 0.18 (Part II.L, 2026-10-25). Fixes to the GTK 3 bindings may never arrive, so those bindings are confined to the donor crates listed in §21.10.2. |
| Openbox has had no upstream release since 3.6.1 (2015); distributions carry patches. | SLOPOS depends on distribution packaging and must be ready to carry a small, reviewed patch itself. |

**Rules that apply now:**

1. **Y27-PLAT: supported-distribution matrix.** Record the 2027 target set in the QA inventory: Ubuntu 24.04 LTS and 26.04 LTS, Debian 13 and Arch. Mark each `NOT_RUN` until tested. For each one, test the Xorg server package, Openbox, the display-manager X11 session launch, AT-SPI, PipeWire and xdg-desktop-portal. A distribution that cannot run the session is listed as `BLOCKED` with the reason. It is never dropped silently.
2. **Keep new interfaces window-system neutral.** New `slopos-appkit`, `slopos-services` and §21.6 interfaces must not expose `x11rb` or other X11 types. X11 stays inside `slopos-x11` and named adapters. This turns §21.7.4's future rule into a present one for new code only. Existing code is migrated when it is touched.
3. **Owner review triggers.** Ask the owner to re-time FUT-COMP (§21.7.4) when any of these happens: a target distribution stops packaging the Xorg server or Openbox; a supported display manager can no longer start an X11 session; a required engine (VTE, WebKitGTK, a portal backend) stops working on X11; or a security fix needed by SLOPOS is unavailable for X11. Until the owner decides, SLOPOS-I stays X11-only.

### 21.10.2 UI substrate decision (Y27-SUBSTRATE)

**Status: `DECIDED` (owner, 2026-10-25). Option (c), a SLOPOS-owned toolkit.** `slopos-ui` no longer depends on GTK. It renders with `tiny-skia`, shapes and lays out text with `cosmic-text`, rasterises SVG icons with `resvg`, hosts windows and receives input through `x11rb`, and publishes accessibility through AccessKit's AT-SPI adapter (`accesskit_unix`, default feature `atspi`). X11 types stay inside `slopos-ui::platform`; they are not part of the public toolkit API.

**Comparison against the stated criteria:**

- **(a) Stay on pinned gtk3-rs — rejected.** The bindings carry disputed "unmaintained" advisories, the `glib` 0.18 pin blocks upgrades, and fixes may never arrive. It keeps Classic pixel exactness hostage to a third-party theme engine and does nothing for the 2027 Wayland/PARAMA route.
- **(b) Move to GTK 4 through gtk4-rs — rejected.** GTK 4 deprecated its X11 backend and plans to remove it in GTK 5. It buys a newer dependency, not a secure X11 future, and still does not give SLOPOS ownership of text metrics or control geometry.
- **(c) SLOPOS-owned stack — selected.** It gives deterministic geometry, rendering and HiDPI, keeps X11 behind one adapter, and leaves a mapping path to a future host backend. Accessibility is delegated to AccessKit rather than reimplemented.

**Limitations accepted with the decision:**

- SLOPOS now owns text shaping, line breaking, input-method integration, widget coverage, theme rendering and the accessibility bridge. A missing control is a `slopos-ui` gap fixed in the toolkit, not a per-app workaround (AGENTS.md rule 3).
- The AT-SPI path uses AccessKit's Unix adapter. Named-surface, focus and Orca journeys still need recorded evidence (§25); building without the default `atspi` feature is not an acceptance configuration.
- X11 is the only host backend this generation. Adding a Wayland or PARAMA host is `platform` work and must not leak host types into controls.
- Legacy donor crates (`slopos-shell`, `slopos-settings`, `slopos-catalogue`) and future engine adapters such as VTE or WebKitGTK may still link GTK 3. They are outside `slopos-ui`; replacing them is a separate task under §2.2.
- Classic pixel exactness and integer HiDPI remain enforced by the §11 geometry tests; the toolkit does not inherit a third-party theme's metrics.

- **Rule now:** the public `slopos-ui` API must not expose `gtk`, `gdk` or `glib` types, and no new GTK surface is added. GTK use stays inside legacy donor crates and engine adapters, so a host-backend change does not alter app code.

### 21.10.3 Third-party application compatibility in 2027 (Y27-COMPAT)

The §21.8 compatibility matrix must include current application types, not only GTK 3:

- **Toolkits:** GTK 4/libadwaita, Qt 6, Electron/Chromium, Java Swing and Flatpak apps, each run under the SLOPOS X11 session.
- **Portals:** the session sets `XDG_CURRENT_DESKTOP=SLOPOS` and ships a `slopos-portals.conf` that selects working backends (for example `xdg-desktop-portal-gtk` for FileChooser). Portals with no working X11 backend, such as some Screenshot or ScreenCast paths, are reported as unavailable, not as working.
- **Global menu:** measure global-menu export per toolkit. Many GTK 4, libadwaita and Electron apps do not export menus over DBusMenu. Those apps keep their in-window menus, and the menu bar shows only the commands an app really exports (§14.1).
- **Input and display:** check fractional-scaling behavior per toolkit at SLOPOS's integer scales, and check XInput2 touchpad gestures where the X server provides them. Unsupported behavior is recorded per toolkit.

### 21.10.4 Accessibility reference (Y27-A11Y)

EN 301 549 V4.1.1 (adopted August 2026) aligns software requirements with WCAG 2.2 Level A and AA. The European Accessibility Act has applied since 28 June 2025. SLOPOS uses **WCAG 2.2 AA, as applied to non-web software by EN 301 549 clause 11**, as its accessibility *reference benchmark* for the shell and first-party apps. This is a design and test target. It is not a legal conformance claim. The design-level mapping is in §5.2.10, and the tests stay in §25.

### 21.10.5 Security, supply chain and regulation (Y27-SUPPLY)

- **Dependency audit:** run `cargo deny check` (advisories, bans, licenses, sources) against the existing `deny.toml` in CI. Each documented exception names an owner and a review date. Unmaintained-crate advisories are exceptions, not silent passes.
- **SBOM and provenance:** each `.deb`, `.pkg.tar.zst` and ISO ships an SPDX or CycloneDX SBOM and checksums. Package signing remains required before any public repository (README "Package repositories"). Reproducible package builds are a goal, and the evidence is recorded as it exists.
- **Vulnerability handling:** security reports go through GitHub private vulnerability reporting, if the owner enables it, or a contact recorded here. No separate policy Markdown file is added (§34). Fixes are tracked in Part II.
- **EU Cyber Resilience Act:** CRA reporting duties for manufacturers apply from 11 September 2026, and most obligations from 11 December 2027. SLOPOS is non-monetised MIT open source, which is generally outside the CRA's manufacturer duties. The current model is one owner maintaining the project without a systematic, sustained support commitment for commercial products, so on the facts recorded here the owner does not meet the open-source software steward definition. If a legal person begins providing systematic, sustained support for SLOPOS's development with the intention of commercial activity, the owner must first record here whether that makes it a steward and, if so, its cybersecurity policy, cooperation with market-surveillance authorities and Article 24(3) reporting duties (reports from 11 December 2027). If SLOPOS is ever distributed commercially (paid images, paid support bundles), the owner must review manufacturer obligations first. This is not legal advice.
- **Toolchain:** `rust-toolchain.toml` pins 1.99.0. Crates are on edition 2021. Migrating to edition 2024 is a separate, explicit task (Y27-EDITION) with the four cargo checks. It is never folded into a feature change.

| ID | Depends on | First evidence |
|---|---|---|
| Y27-PLAT | QA-01 | Matrix rows with `PASS`/`FAIL`/`BLOCKED`/`NOT_RUN` per distribution and display manager |
| Y27-SUBSTRATE | UI-01/UI-02 status | `DECIDED` 2026-10-25: option (c) recorded in §21.10.2; `slopos-ui` implementation and its stated limitations are the evidence |
| Y27-COMPAT | §21.8 matrix, session packaging | Per-toolkit launch, menu, portal and scaling rows in a graphical VM |
| Y27-A11Y | §5.2.10, §25 | Criterion-by-criterion mapping with test owners; Orca journey evidence |
| Y27-SUPPLY | CI | `cargo deny` job result, SBOM files attached to package artifacts |
| Y27-EDITION | Green CI on `main` | Edition 2024 migration commit with all four cargo checks passing |
