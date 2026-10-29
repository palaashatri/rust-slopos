# Shared UI toolkit program (proposed architecture and gates)

Status: **early foundation**, not a released general-purpose toolkit.
Owner direction: one Rust-first, MIT-licensed first-party UI system for
SLOPOS and independently runnable desktop applications (including Loom,
ClipKit, Donkey and small utilities). PARAMA is an eventual source-port
target, not a supported host. This program does not authorize a SLOPOS-II
compositor rewrite or invalidate the existing X11/Openbox roadmap.

## Decision: incubate in SLOPOS, then version independently

SLOPOS has a real, tested UI toolkit and will be the first consumer and
reference host. Introduce portable foundations here to preserve Calculator,
Dock, gallery, Classic pixel output, input and accessibility behavior.
**Do not make the other apps depend on the SLOPOS desktop session.**

Once the acceptance gates below are satisfied, extract portable crates into
an independently versioned MIT-licensed toolkit repository. SLOPOS will
consume released toolkit versions like any other app. Until then, the SLOPOS
monorepo is the integration venue; this is not a claim that extraction is
implemented or that an independent repository already exists.

Target dependency direction:

    App/desktop shell
       -> portable widget/layout/state/theme/animation packages
       -> renderer-neutral command/scene packages
       -> platform host (X11, Wayland, Win32, macOS, future PARAMA)
       -> reference software renderer OR optional GPU renderer

An app can also use a dedicated editor canvas while sharing the same
typography, events, theme and accessibility primitives. No app-specific
reverse dependencies in the portable toolkit. Native platform adapters
may depend on the toolkit, never the reverse.

## Current milestone M0 (implemented as an initial experiment)

- Existing \`slopos-ui\` keeps \`RasterSurface\` and Classic bitmap fidelity.
- \`scene::Scene\` records a bounded, ordered list of clear, fill-rectangle
  and stroke-rectangle commands; \`scene::Renderer\` is backend-neutral.
- \`scene_raster::RasterRenderer\` replays the list to the existing raster
  renderer; the gallery Surface fixture uses this path.
- \`animation\` offers pure elapsed-time easing and reduced-motion
  completion without creating timers or threads.
- Headless unit tests cover ordering, command-budget overflow, scale,
  clipping boundary pixels, invalid geometry and animation endpoints.
- No GPU renderer, new text/layout engine, compositor, generic widget tree,
  mobile support or PARAMA port is implied by M0.

The current scene representation uses existing SLOPOS geometry and \`std\`.
That is acceptable for incubation; a later independent portable core must
define its exact public geometry, allocations and optional \`no_std + alloc\`
compatibility contract.

## Requirements for future milestones

### M1: reusable foundation, controls, and desktop host

- Extend scene commands with nested clips, transforms, images, text runs,
  opacity layers and bounded resource handles; retain ordered semantics.
- Distinguish logical/device pixels and support fractional DPI without
  compromising the separate integer-scaled Classic pixel-art mode.
- Add invalidation/dirty regions, virtualized containers, event-driven
  wakeups, frame cancellation, theme tokens, deterministic input focus and
  lifecycle handling.
- Each control must support keyboard, pointer, accessibility (including
  semantic roles/names/actions), high-contrast and reduced-motion behavior.
- Build a small independently runnable sample app without SLOPOS runtime
  services. On Linux X11 remains the first host; Windows/macOS/Wayland are
  separate ports with specific acceptance tests.

### M2: optional GPU renderer

- Investigate wgpu and Vello GPU with documented upstream feature support,
  license inventory, fallback behavior and platform compatibility.
- Retain a software path (currently tiny-skia; Vello CPU may be evaluated)
  as a supported backend and reference for geometry and clipping.
- Compare captured output at 1x/2x and fractional DPI, with tolerances for
  text antialiasing specified separately. Do not hide mismatches.
- GPU device loss must trigger controlled recovery or a real software
  fallback, not a frozen UI. HDR/color management need explicit contracts.
- Avoid eagerly allocating expensive GPU resources in small utilities.

### M3: professional editors and accessibility parity

- Stress-tests: 10,000+ item virtual lists, rich text, tabular views,
  docking/splitters, timelines and zoomable vector canvases.
- Measure p50/p95/p99 frame time, input latency, resident/private memory,
  wakeups at idle, CPU utilization and GPU memory under fixed workloads.
- Cover resize/DPI/theme changes, IME, RTL, screen readers, long sessions,
  memory pressure, renderer loss and cancellation.
- Begin adoption with an independent small utility, then a demanding
  application; preserve existing formats and real user workflows.

### M4: extraction and versioning gate

Extract to a separate repository only after ALL are demonstrated:

1. At least two non-SLOPOS applications use the portable public API without
   shell/session dependencies.
2. Headless tests and real desktop integration tests pass on Linux, Windows
   and macOS in CI.
3. Software and GPU rendering achieve measured acceptance on the supported
   OS/device matrix; unsupported backends are declared honestly.
4. API migration policy, SemVer, MSRV, feature matrix and contribution
   documentation are accepted.
5. License/asset/SBOM review is complete, including any non-MIT dependencies.
6. The extraction preserves SLOPOS Classic screenshots, accessibility and
   keyboard/input behavior.

At that point SLOPOS consumes versioned releases; the first-party toolkit
may retain MIT source licensing regardless of the applications' own licenses.

## Baseline hardware and non-goals

- **Low end:** Raspberry Pi Zero 2 W (512 MB) is an *aspirational test
  target*, not yet verified. Budget memory and idle CPU, skip gratuitous
  effects, keep render lists bounded, and lazy-load heavyweight resources.
- **High end:** use GPU acceleration when it produces measured improvement
  on powerful Windows/Linux/macOS computers. High refresh-rate rendering
  must not require busy polling while the UI is idle.
- **Animation:** event-driven scheduling, reduced-motion preference,
  interruptible transitions and bounded duration. Frame-rate targets are
  acceptance goals to benchmark, not guaranteed capabilities.
- **AI/ML inference, FFmpeg and hardware drivers:** existing mature native
  libraries may remain external. "One first-party language" is not a
  promise to rewrite every dependency in Rust.
- **PARAMA:** needs separate input, allocator, surface, event loop and
  capability/IPC adapters, plus proven userspace isolation and graphics;
  Linux code cannot simply be recompiled for PARAMA.
- **License:** our new source is MIT. Dependencies, firmware, icons, fonts
  and model files retain their own licenses; review redistributability and
  source attribution before packaging.

## Verification for each UI PR

- Run \`cargo fmt --all -- --check\`.
- Run \`cargo clippy --workspace --all-targets --locked -- -D warnings\`.
- Run \`cargo test --workspace --locked\`.
- Run the architecture checks and Xvfb smoke suite documented in
  [AGENTS.md](../AGENTS.md); report unavailable checks as not run.
- Add pixel baselines and direct user interaction tests when presentation
  or input semantics change; do not rely exclusively on compilation.
- Record actual hardware and frame/memory measurements before calling a
  backend "faster" or "lightweight."
