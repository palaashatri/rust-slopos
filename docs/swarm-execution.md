# SLOPOS application-platform swarm execution playbook

**Operational plan, not a record of implementation.**
Read [AGENTS.md](../AGENTS.md), [architecture.md](architecture.md),
[ui-toolkit-program.md](ui-toolkit-program.md),
[application-platform.md](application-platform.md), and the existing
[contract §0](contract/00-agent-workflow.md) **in full** before dispatch.
The existing engineering contract governs VM safety, evidence, release
requirements, tests and file ownership; this document extends its §21
backlog with coordinated UI + application work, not a parallel authority.
Read [contract §§14–18](contract/05-shell.md) for shell work and
[contract/06-applications.md](contract/06-applications.md) for app scope.

## Non-negotiable decisions

- One SLOPOS monorepo contains shared MIT first-party UI, services, shell
  and apps. Toolkit reuse by other repos must work without a SLOPOS session.
  A separate toolkit repo is **optional, owner-approved**, not a worker goal.
- Do not revive a SLOPOS-II compositor track: current target remains Rust,
  X11 and Openbox. Maintain functional old GTK donor screens until native
  screens pass their acceptance tests.
- App tracks can prepare verified **domain models** while toolkit controls
  are built; an app UI slice only starts after its component/API dependencies
  are integrated. Parallel does **not** mean "scaffold all apps now".
- Never silently refactor working Calculator, Dock, X11 host, package
  scripts or UI foundations to accommodate another track's preferences.
- Keep contract §16/§17 app names: Files, Control Panels, Software, Image
  Viewer, Media (paths listed below); reuse one set of services and controls.
- Keep prototype code, art and dependencies license-audited. MIT source
  licenses apply to first-party files, not necessarily to dependencies.

## Roles and ownership

| Role | Exclusive authority | Scope delegated |
|---|---|---|
| Coordinator/integrator (one) | Root `AGENTS.md`; Cargo workspace/lock/toolchain; public UI APIs; crate/module registries; CI; app manifests; packages; shared theme tokens; integration branch | Assigns immutable task packets; reconciles feature dependencies; runs gates |
| UI control worker | A specifically assigned control implementation and unit tests | No public API/registry edits without coordinator patch |
| Renderer worker | Scene backend internals, scoped parity tests and benchmarks | No shell windowing, app-specific interfaces, or silent backend switch |
| Services workers | A *single domain* state/action backend and fixtures | No UI, overlapping device mutation or competing providers |
| App workers | A bounded model / widget composition / user-journey slice for one app | No ad hoc common controls, public API rewrites, package edits |
| Shell workers | Distinct popover/OSD/Dock modules, consuming typed service snapshots | No global shortcut ownership duplication, independent bus daemons |
| QA and design review | Isolated fixtures, screenshots, input/a11y/performance evidence; visual parity review | Cannot weaken golden files, CI, accessibility to mark task PASS |

Use distinct branches/worktrees and **non-overlapping exact paths**.
Read existing §0.3–0.7 for VM/resource leases, owner handoffs, integration
SHA, build concurrency and runtime-test isolation. Even with unlimited AI
tokens, number of build permits is constrained by actual RAM/disk and VM
isolation. Begin with one integrator plus two or three implementers; increase
only after demonstrated lack of collisions.

## Dependency-ready backlog

Task status begins **PLANNED**; completed work is proven by commits and
evidence, not this table. Existing toolkit, Calculator, Dock and providers
already contain implemented behavior; review current truth before choosing
which task actually needs work.

| ID | Depends on | One reviewable outcome | Writable scope assigned by coordinator | Acceptance/evidence |
|---|---|---|---|---|
| SWARM-00 | Repo/VM access | Baseline and ownership manifest; resource ceilings and test permits | Integration scratch outside product docs | Head SHA, existing tests, hardware/guest info, no overlapping leases |
| UI-API-01 | SWARM-00, contract UI-01 | Freeze minimal widget/scene/resource contracts for app consumers | Coordinator-owned `crates/slopos-ui` public APIs | API tests, no shell/platform leakage, design tokens and accessibility |
| UI-TEXT-01 | UI-API-01 | Text input/search field: selection, clipboard, IME, focus | Assigned new text control files + tests | Unicode, long input, password-safe behavior where applicable |
| UI-MENU-01 | UI-API-01 | Menu/popover, submenu, keyboard dismissal | Assigned menu/control files + tests | Focus restoration, enabled/disabled actions, screen-reader semantics |
| UI-LIST-01 | UI-API-01 | Virtual list/tree with bounded viewport and selection | Assigned list/layout files + tests | Large fixture, keyboard/a11y and bounded allocations |
| UI-MODAL-01 | UI-TEXT-01, UI-MENU-01 | Dialogs, confirmation and modal focus | Assigned dialog controls + tests | Escape/default/overwrite, accessible live region |
| RENDER-01 | UI-API-01 | Scene/raster parity and invalidation baseline | Assigned renderer/tests, not widget public API | 1x/2x snapshot parity, clip/alpha/geometry, zero idle polling |
| RENDER-GPU-01 | RENDER-01 | Optional GPU renderer prototype | Separate optional renderer adapter | CPU parity, device loss/recovery, fallback, GPU + low-end measurements |
| KIT-ACT-01 | UI-MENU-01, existing contract KIT-01 | Named app actions and window/menu lifecycle | New `slopos-appkit` leaf modules | Keyboard/local/global command parity and no shell requirement |
| KIT-FILES-01 | KIT-ACT-01, UI-MODAL-01, UI-LIST-01 | Reusable open/save/file chooser API | Appkit chooser leaf modules | Cancellation, overwrite and MIME/path tests |
| SVC-PROOF-01 | SWARM-00 | Provider snapshots and mutation fixtures for Network, Bluetooth, audio and power | Individually allocated `slopos-services` domain/test files | Missing hardware, denial, hotplug, readback, redacted secrets |
| APP-FILES-MODEL-01 | SWARM-00 | File navigation/selection/MIME model | New `apps/files` domain model only | Tmpdir, symlink, unreadable/renamed/deleted file, cancel |
| APP-FILES-UI-01 | APP-FILES-MODEL-01, UI-LIST-01, UI-MENU-01, KIT-FILES-01 | Browse/back/up/select/open native Files window | Assigned `apps/files` UI leaf files | Keyboard+pointer, screenshot, error paths, real open |
| APP-IMAGE-MODEL-01 | SWARM-00 | Bounded image decode, transform and thumbnail model | New `apps/image-viewer` model only | Malformed/huge images, cache eviction, color/orientation |
| APP-IMAGE-UI-01 | APP-IMAGE-MODEL-01, UI-API-01, KIT-ACT-01 | Open/fit/zoom/pan image viewer | `apps/image-viewer` UI leaf files | End-to-end open/navigation, DPI, accessibility |
| APP-MEDIA-MODEL-01 | SWARM-00 | Media playback state + decoder adapter | New `apps/media` model/engine adapter only | No-codec, malformed input, playback state, legal codec inventory |
| APP-MEDIA-UI-01 | APP-MEDIA-MODEL-01, KIT-ACT-01, UI-API-01 | Play/pause/seek/volume + real display | `apps/media` UI leaf files | A/V time, subtitles, cancellation, CPU fallback |
| APP-PANELS-01 | SVC-PROOF-01, UI-TEXT-01, UI-MENU-01 | Real Network/Sound panel consuming providers | New `apps/control-panels` | Missing device, permission refusal, action readback |
| APP-STORE-MODEL-01 | SWARM-00 | Curated AppImage catalog + trusted transaction model | New `apps/software` model only | Hash/source authenticity, atomic staging, cancel/rollback |
| APP-STORE-UI-01 | APP-STORE-MODEL-01, KIT-ACT-01, UI-MODAL-01 | Catalog/detail/verified install UI | `apps/software` UI files | Real install in isolated guest; no fake progress |
| SHELL-STATUS-01 | SVC-PROOF-01, UI-MENU-01 | Wi-Fi/Bluetooth/clock popover surface | Assigned shell status UI leaf files | Real state + unavailable UX, keyboard, menu placement |
| SHELL-OSD-01 | SVC-PROOF-01, UI-API-01 | Volume/brightness OSD on provider state changes | Assigned shell OSD leaf files | Non-focus-stealing, reduced motion, timeout and readback |
| QA-PORTABLE-01 | SWARM-00 | Independent app without shell services + visual regression fixtures | Assigned QA samples/scripts | Standalone X11 guest launch, accessibility, no shell import |
| QA-EDGE-01 | QA-PORTABLE-01, RENDER-01 | Edge/high-end benchmark and profiling protocol | Assigned benchmark harness | Reproducible p95 latency, RSS, GPU, idle wakeups; no invented Pi results |
| RELEASE-APP-01 | verified app UI + QA | Register package/desktop entry for one app | Coordinator-only `Cargo.toml`, `packaging/`, `install.sh`, CI | Actual install/open/uninstall and package test; no broken nightly |

Task IDs here are **new namespace identifiers**; the contract §21 legacy
IDs remain valid, and the coordinator must link equivalent tasks rather than
schedule duplicates. In particular, use existing UI-03/UI-04/UI-05,
KIT-01/KIT-02 and SVC tasks as prerequisites if they already have owners or
accepted implementations. Reconcile this backlog with the current ledger
before issuing any new task packet.

## Parallel-start plan

Initial tranche (after SWARM-00, without speculative UI API changes):

1. **Coordinator** owns UI-API-01 and module/manifest registration.
2. **Renderer worker** takes RENDER-01, no public API changes.
3. **App backend workers** can separately take APP-FILES-MODEL-01,
   APP-IMAGE-MODEL-01, APP-MEDIA-MODEL-01 and APP-STORE-MODEL-01;
   start as many as resource/ownership budgets permit.
4. **Services worker** takes one narrowly allocated domain under
   SVC-PROOF-01, reusing already implemented providers.
5. **QA worker** begins QA-PORTABLE-01 fixture/harness preparation with no
   shared app-runtime lease overlap.

After UI API and control tasks integrate, enable app-facing UI workers.
Central toolkit control owners remain separate from application UI owners.
The integrator can accept independent ready changes while waiting on one
blocked component. A blocked track cannot invent or vendor its own control
or bypass required tests.

## Required task packet — copy this for every worker

~~~yaml
id: APP-FILES-MODEL-01
status: PLANNED          # PLANNED/READY/CLAIMED/IMPLEMENTED/VERIFIED/INTEGRATED/BLOCKED
integrator_base_sha: "<exact accepted SHA>"
owner: "<agent id>"
branch: "agent/app-files-model-01"
worktree: "<exclusive guest-local path>"
objective: "Deliver one observable, testable file navigation model"
depends_on:
  - "SWARM-00:<accepted SHA>"
writable_paths:
  - "apps/files/src/model.rs"   # coordinator verifies exact path ownership
  - "apps/files/tests/model.rs"
read_only_paths:
  - "crates/slopos-ui/"
  - "docs/"
public_interfaces: ["<agreed function signatures and errors>"]
exclusions: ["no GUI", "no Cargo workspace edits", "no real delete/move UI"]
positive_cases: ["directory listing, back/up, selection"]
negative_cases: ["permission denied, disappearing file, symlink loop"]
accessibility: "N/A (headless model)"
verification_commands: ["<commands agreed with coordinator>"]
runtime_lease: "none"
build_permits: 1
max_cargo_jobs: 2
evidence_destination: "<unique per-task guest-local path>"
handoff: "<commit SHA, diff paths, exit codes, test logs, limitations>"
~~~

Treat packet paths as an *illustrative reservation*, not permission to
create overlapping modules while another owner edits those paths.
Every packet includes expected UI/fixture behavior if applicable, exact
dependency SHAs, budgets, and a stop condition. Blockers identify a
specific missing API, responsible owner and independent work that can
still continue.

## Integration and review gates

1. Rebase/reconcile the task branch against the coordinator's integration
   SHA (never overwrite unrelated work). Review each changed file against
   the worker's declared ownership; reject silent shared-file edits.
2. Run `cargo metadata --locked --format-version 1 --no-deps`, format,
   `cargo clippy --workspace --all-targets --locked -- -D warnings`,
   `cargo test --workspace --locked`, architecture/spec checks and
   changed-shell-script syntax checks. See AGENTS for exact commands.
3. For UI, confirm keyboard, pointer, access tree, Classic 1x/2x snapshots,
   theme/DPI changes and no use of fabricated provider state.
4. For session-affecting changes, run smoke in an **isolated Linux guest**
   using the dedicated runtime lease and check exit/restart behavior.
   Separate unit, fixture, VM, hardware and cross-platform validation.
5. Assess third-party dependency licenses and production asset provenance.
   MIT does **not** waive notice/GPL/linking requirements.
6. Record task ID, integrated SHA, guest/hardware info, commands, exit
   codes, screenshot artifacts/hashes, benchmark settings and known gaps.
7. Only the coordinator changes workspace manifests, shared public APIs,
   CI/test baselines, central docs and packaging. Integrate in dependency
   order; rerun affected gates at the final tree.
8. Never make screenshot goldens smaller, delete failing cases, disable CI,
   claim a mock test verifies physical hardware, or merge unfinished apps.

## First-wave success criteria

- A real toolkit control is reused in at least two independently named
  first-party surfaces (Calculator and a new app/shell surface can qualify).
- Two standalone model tasks deliver tested failure/cancellation behavior.
- One native app/shell user journey runs end-to-end and matches shared
  accessibility/visual requirements.
- The existing Calculator, Dock, legacy menu, X11 logout, notifications
  and packaging journeys do not regress.
- Every step is attributable to a small PR with linked task packet and
  test evidence. A folder, placeholder widget or mock SSID is not progress.

## Coordinator kickoff prompt

> Read the full SLOPOS AGENTS.md, docs/architecture.md, docs/CONTRACT.md
> and contract/00-agent-workflow.md, then ui-toolkit-program.md,
> application-platform.md, and this playbook. Inspect main's HEAD, status,
> existing crates, tests, VM rules and contract ledger. Create an exact
> source/ownership and dependency report before scheduling work. Maintain
> X11/Openbox and existing Classic UI behavior. Select the smallest
> dependency-ready tranche from this backlog; issue one packet per worker
> with distinct paths, exact base SHA, success/failure cases and resource
> lease. Develop shared UI and app/domain models concurrently but do not
> implement dependent UI against imagined APIs. Use real provider state,
> MIT source plus audited dependencies, bounded idle/memory cost, and
> keyboard/pointer/accessibility proof. Integrate only verified work
> through PRs; document any missing VM or hardware QA as NOT_RUN.
> Stop or rescope blocked tasks rather than fake completion.

No swarm is running merely because this playbook exists. A coordinator
with authenticated repository and guest/test access must execute it.
