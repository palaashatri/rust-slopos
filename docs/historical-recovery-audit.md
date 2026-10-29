# Historical branch and recovered-work audit

**Snapshot:** 2026-10-30, compared with `main` at
`1be93eff168d50c113cc01307ffdd24573c44042`.
**Method:** GitHub branch enumeration, `main...branch` compare, branch trees,
selected source/test/README reads and PR history. This is **source inspection**,
not a build, executable QA, license clearance or visual acceptance.
The branch list can change after this snapshot.

See [architecture.md](architecture.md) for what actually exists on main,
[application-platform.md](application-platform.md) for native app targets,
[swarm-execution.md](swarm-execution.md) for agent dispatch, and the original
[contract §0](contract/00-agent-workflow.md) for VM execution constraints.

## Branch inventory (16 pre-archive branch refs seen)

| Branch | Relationship to main | Audit outcome |
|---|---|---|
| `main` | baseline | Current X11/Openbox authoritative runtime |
| `codex/shared-ui-foundation` | 18 ahead / 0 behind at audit | Active draft PR #32; toolkit prototype and application/swarms docs |
| `agent/recovered-codex-spec-stash` | 1 ahead / 569 behind | Closed unmerged PR #24; tiny compositor-QA/CI/ledger changes on a **large, historical Wayland tree** |
| `agent/recovered-pivot-stash` | 1 ahead / 560 behind | Closed unmerged PR #23; hardcoded slate root background + Docker QA, obsolete |
| `agent/sinty-modern-theme-and-platform-desktop` | 3 ahead / 97 behind | Old docs-only SLOPOS Modern direction; already covered in newer Design Bible |
| `design/brand-identity-20261029` | 4 ahead / 0 behind | Closed unmerged PR #30; editable SVG identity kit, rejected visual direction |
| `design/identity-icons-20261029` | 6 ahead / 0 behind | Draft PR #31; dimensional raster identity assets, visual/packaging review needed |
| `agent/spec-01-schema-inventory` | 0 ahead | Already ancestor of main |
| `agent/qa-01-release-runner-inventory` | 0 ahead | Already ancestor of main |
| `agent/qa-01-required-runner-map` | 0 ahead | Already ancestor of main |
| `agent/pi-zero2w-edge-performance-contract` | 0 ahead | Already ancestor of main (PR #20 merged) |
| `agent/pr22-review-fixes` | 0 ahead | Already ancestor of main (PR #25 integrated in PR #22 history) |
| `agent/pr22-followup-fixes` | 0 ahead | Already ancestor of main (PR #26 integrated) |
| `agent/pr22-checklist-lockfile` | 0 ahead | Already ancestor of main (PR #27 integrated) |
| `agent/pr22-rereview-fixes` | 0 ahead | Already ancestor of main (PR #28 integrated) |
| `claude/rewrite-agents-md` | 0 ahead | Already ancestor of main (PR #16 merged) |

Five redundant archive branch refs were subsequently created and pinned;
see [branch-preservation.md](branch-preservation.md). These add references,
not code changes, to the repository. The original 16-branch table is
kept as the dated audit snapshot.

The nine zero-ahead branches have **no unique commits to cherry-pick**.
Do not delete them on the basis of this audit alone. The old PR #22
and its patch PRs are already in main's history.

### Why the compositor is not visible in the one-commit diff

The `agent/recovered-codex-spec-stash` tip points to an old SLOPOS Wayland
source tree: more than 1,900 tracked files, including
`crates/slopos-compositor/`, `crates/slopos-render/`,
`crates/slopos-kit/`, `crates/slopos-sdk/`,
`crates/slopos-bus/`, `crates/slopos-vision/` and six old native app
directories. Its *one ahead commit* only changes
`.github/workflows/ci.yml`, `TRUTH.md` and
`scripts/verify-compositor-completion.sh`. The compositor and apps are
historical **snapshot contents**, not additions in that commit.
Meanwhile current main has moved to X11/Openbox and no longer contains
`crates/slopos-compositor`. Do not equate a short diff with no historic code.

The old compositor was developed in merged historical PRs #3, #4, #5 and
later work prior to the X11 pivot. Historical merged PRs do not imply their
old components remain active on today's main.

## Deep recovery review

### R1 — Old Smithay compositor: preserve as research, not current runtime

Reference:
`agent/recovered-codex-spec-stash:crates/slopos-compositor/`.
This contains Smithay 0.7 with nested-X11, DRM/KMS and protocol-only headless
paths; Wayland input and XDG toplevel/popup work, Spaces, XWayland,
layer-shell, screenshot transport, output assignment and frame-timing logic.
Core `src/main.rs` is ~307 KB and `src/lib.rs` ~130 KB, so extraction should
reduce coupling and test interfaces rather than revive the large binary.
Reference tests include compositor policy contracts and headless clients
covering clipboard/primary selection, drag/drop and text input.

**Evidence limit:** old `TRUTH.md` reported build/headless and protocol
coverage but *no verified production DRM framebuffer or complete physical
HDR/VRR acceptance*. Its 63/100 claim is historical, not a rating of main.
Old branch has not been built against main's toolchain/dependency set.
The recorded Xvfb/DRM restrictions mean screenshots do not prove
end-to-end compositor compatibility.

**Action:** archive the source/ref and extract *pure policy algorithms/tests*
that remain useful: frame scheduler, reversible presentation/zoom/tiling,
output migration, Spaces model and compositor QA fixture ideas. Port them
only after matching current platform contracts. Keep compositor code out of
current X11 runtime. Starting a Wayland/Smithay compositor track requires
an **explicit owner decision and an isolated future architecture PR**.

### R2 — Old GPU renderer and UI toolkit: selective algorithm/test salvage

References: `crates/slopos-render/`, `crates/slopos-kit/` and
`crates/slopos-sdk/` at the recovered-code branch.

- `slopos-render` has wgpu surface/present-mode policy, shader files,
  draw-command queues, text and texture utilities. Port unit-tested
  renderer policies or fixtures as independent candidates **only if**
  more appropriate than the new bounded scene API and retained CPU path.
- `slopos-kit` contains controls for text, menus, lists, tabs, scroll,
  splits, focus, DnD and accessibility. The old TextField has UTF-8-aware
  selection routines and tests that may be valuable.
- **Warning:** at least `slopos-kit/src/list_view.rs` has an empty
  `draw()` implementation; the legacy SDK is highly coupled to wgpu,
  winit and its Wayland/bus stack. API names do not prove render-ready
  controls. The old `slopos-sdk/src/lib.rs` is ~223 KB.

**Action:** import *conformance tests and well-isolated algorithms first*,
then implement new controls in the current toolkit with real render and
keyboard/a11y parity. Do not transplant the full widget tree, old SDK,
rendering engine, or global state into `slopos-ui`.

### R3 — Native application models: substantial candidates, security review

References in the historic tree:
`apps/finder/`, `apps/preview/`, `apps/terminal/`,
`apps/textedit/`, `apps/settings/` and `apps/appstore/`.

- **Finder:** navigation and file operation implementation, selection and
  tests. The old `file_ops.rs` handles cross-device moves by copy/delete,
  but code calls `fs::copy` and does cleanup on error without fully
  proving no existing destination is clobbered; fresh tests for
  overwrite, symlinks, mount boundaries and interruption are mandatory.
  Do **not** ship its destructive operations by cherry-pick.
- **Preview:** real image decoding/zoom/rotation and Vision job/model
  code, with substantial tests. Bounded decode and image-transform logic
  are good research candidates for new `apps/image-viewer`.
- **Terminal:** PTY and parser code to evaluate, but validate escape
  sequences, Unicode/IME, subprocess lifecycle and terminal security
  against a mature engine rather than assume parity.
- **App Store:** signed catalog/Ed25519 publisher verification, transactional
  archive install and testable trust-store types: strong candidate for a
  targeted security design review. Rebuild through the current Software
  service contract; do not silently change host packages.
- **TextEdit, Settings:** review models and user-journey tests as references.
  The old GUI depends on old toolkit/SDK, not current APIs.
- **Vision:** optional OCR/segmentation engine and protocol code can be
  evaluated separately, with model/weight licenses, CPU memory budget
  and per-feature consent boundaries.

**Action:** map historical behavior to the canonical *new* app names:
`apps/files`, `apps/image-viewer`, `apps/media`,
`apps/control-panels`, `apps/software`. No duplicate old app trees.

### R4 — Recovered unfinished compositor QA stash: don't merge

PR #24's only ahead commit:
- changes CI by dropping a redundant-looking `git diff` lockfile check;
- extends obsolete `TRUTH.md` with compositor viewport QA claims;
- calls `verify-compositor-output-topology-runtime.sh` and
  `verify-compositor-viewport-correctness.sh` from the completion script.

Those scripts reference the removed `slopos-compositor` stack. PR #24 was
closed unmerged, and its authors explicitly reported it was not tested
against current main.

**Action:** retain historical viewport/scale/capture test *ideas* and
individual test fixtures. Do not apply the CI edit or legacy QA gate to
the X11 platform; maintain current locked-Cargo/CI policy.

### R5 — Recovered X11 root color stash: already superseded

PR #23's ahead commit sets `#758090` via `xsetroot` and modifies old Docker
QA. Current main already calls `environment::apply_root_background()`,
which selects theme-specific root colors and honors wallpapers.
Hardcoding slate gray would regress Platinum/Classic/Graphite/OLED
appearance and user wallpaper preferences.

**Action: no cherry-pick**. At most use it as a historical screenshot color
fixture if a new QA case needs a fixed scene.

### R6 — Sinty/Modern theme documentation: already in current vision

Branch `agent/sinty-modern-theme-and-platform-desktop` changes only
`AGENTS.md`, `README.md` and `docs/CONTRACT.md` relative to its old base.
Current `docs/contract/02-design.md` already describes Sinty as future
Modern inspiration, distinct from the binding Classic geometry and clean-room
asset rules. SLOPOS Modern and XP-style theming remain planned.

**Action: no direct merge**. Only migrate a demonstrably missing design
requirement, in a small contract-specific PR, if a side-by-side comparison
establishes a gap. No source or renderer to recover from this branch.

### R7 — Design variants: resolve, never combine indiscriminately

- **PR #30 / `design/brand-identity-20261029`:** closed unmerged,
  primarily SVG vector identity and multi-size exports. Useful editable
  asset-generation methods, but rejected visual direction.
- **PR #31 / `design/identity-icons-20261029`:** open *draft* alternative
  with polished raster icon/wordmark/ICO/ICNS. Original crop is ~248 px;
  nominal 512/1024px files are upscaled, not native-detail masters.
  Its review checklist still requires small-icon legibility, alpha,
  native container integration and packaging checks.

**Action:** approve one direction on visual evidence; regenerate
production masters if needed. No automatic merge, and no preference for
file count over visual quality. Keep license/provenance records.

## Recovery tasks for the coordinator (PLANNED, not started)

| Task | Depends | Result required |
|---|---|---|
| RECOVER-01 | Read this audit + current architecture | Exact ref/hash manifest, ownership of candidate files, current-vs-old API mapping and license scan |
| RECOVER-02 | Current toolkit scene/tests | Import/select isolated UTF-8 text, focus and event tests; new working widget parity |
| RECOVER-03 | Current rendering contract | Benchmark old wgpu present-mode/command ideas against current CPU scene and optional GPU adapter |
| RECOVER-04 | Files/Software model tasks | Targeted security reviews of old Finder file ops and signed catalog installer; reproduce invariant tests on new app models |
| RECOVER-05 | Image Viewer/Terminal models | Image zoom/decode and PTY/parser behavior tests with malformed/large input cases |
| RECOVER-06 | Explicit owner approval for future compositor | Historical compositor subsystem threat model, DRM/Wayland API feasibility and isolated VM/hardware QA plan (no current runtime replacement) |
| RECOVER-07 | Visual owner review | Choose approved identity branch, verify exports at real desktop sizes and package integration |

Do not assign two workers to the same old/new public API path.
The coordinator owns Cargo/module registrations; all salvage goes into
small feature-specific PRs from current `main`, **not merges of old branch
heads**. A recovery PR must list exact historic source SHA, code origin,
changed behavioral contract, old/new tests, license attribution, VM evidence,
and work intentionally **not** imported.

## Explicitly not verified by this audit

No source tree was checked out or compiled here; no tests were executed.
The old Wayland stack was not booted in QEMU/VirtualBox/real DRM, no old
screenshots were independently validated, no GPU performance was measured,
and no copyright/third-party license inventory was fully completed.
Historical passing test descriptions remain historical evidence for those
commits only. Any implementation/production claim requires an exact-SHA
rebuild plus end-to-end functional and security QA.
