# SLOPOS UI toolkit program (in-monorepo application platform)

Status: **early foundation**, not a released general-purpose toolkit.
Owner direction: one Rust-first, MIT-licensed first-party UI system
**developed and maintained inside SLOPOS** for its shell and native apps;
reusable by independent desktop applications (including Loom, ClipKit,
Donkey and small utilities). PARAMA is an eventual source-port
target, not a supported host. This program does not authorize a SLOPOS-II
compositor rewrite or invalidate the existing X11/Openbox roadmap.

## Decision: SLOPOS owns the permanent toolkit by default

SLOPOS is the source repository and first compatibility host. Native
first-party SLOPOS apps, widgets and shell surfaces are designed alongside
portable toolkit features: each product must exercise real user journeys,
accessibility and constrained-device behavior. Existing Calculator, Dock,
Classic snapshots, input and accessibility must remain correct.

The toolkit remains independently **usable**: public UI APIs do not require
an active SLOPOS session, X11 or Linux-specific providers. Its portable
crates can be consumed through paths, Git dependencies, or eventual releases.
A separate toolkit **repository is optional**, never a prerequisite or
automatic milestone. Only the owner may authorize extraction after
demonstrated multi-app adoption and a measured maintenance benefit.

[application-platform.md](application-platform.md) defines native app,
service, appkit and shell ownership. [swarm-execution.md](swarm-execution.md)
defines parallel agent dispatch and review/merge gates. Do not launch
dependent UI work before its shared widget contract is integrated.

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

- Existing `slopos-ui` keeps `RasterSurface` and Classic bitmap fidelity.
- `scene::Scene` records a bounded, ordered list of clear, fill-rectangle
  and stroke-rectangle commands; `scene::Renderer` is backend-neutral.
- `scene_raster::RasterRenderer` replays the list to the existing raster
  renderer; the gallery Surface fixture uses this path.
- `animation` offers pure elapsed-time easing and reduced-motion
  completion without creating timers or threads.
- Headless unit tests cover ordering, command-budget overflow, scale,
  clipping boundary pixels, invalid geometry and animation endpoints.
- No GPU renderer, new text/layout engine, compositor, generic widget tree,
  mobile support or PARAMA port is implied by M0.

The current scene representation uses existing SLOPOS geometry and `std`.
That is acceptable for incubation; a later independent portable core must
define its exact public geometry, allocations and optional `no_std + alloc`
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

### M4: stable external usage and optional extraction review

Establish a stable, usable public toolkit API inside the SLOPOS monorepo
once ALL of the following are demonstrated:

1. At least two non-SLOPOS applications use the portable public API without
   shell/session dependencies.
2. Headless tests and real desktop integration tests pass on Linux, Windows
   and macOS in CI.
3. Software and GPU rendering achieve measured acceptance on the supported
   OS/device matrix; unsupported backends are declared honestly.
4. API migration policy, SemVer, MSRV, feature matrix and contribution
   documentation are accepted.
5. License/asset/SBOM review is complete, including any non-MIT dependencies.
6. Any crate boundary/versioning change preserves SLOPOS Classic screenshots,
   accessibility and keyboard/input behavior.

At this point consider an independent release process *within the monorepo*.
Extraction into another repository is **not required** and needs a separate
owner decision based on an actual maintenance and distribution advantage.
No worker is permitted to create that repository as part of M4.

## Parallel application demand

Build the toolkit against real SLOPOS application consumers: Files,
Control Panels, Software, Image Viewer, Media, Calculator and the shell's
Wi-Fi/Bluetooth/clock popovers, notifications and volume/brightness OSDs.
For the exact app paths, service contracts, dependency waves and task packets,
see [application-platform.md](application-platform.md) and
[swarm-execution.md](swarm-execution.md). Domain models and provider tests
may proceed in parallel with toolkit work; app UI must not copy missing
controls or force unreviewed public API changes.

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

- Run `cargo fmt --all -- --check`.
- Run `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Run `cargo test --workspace --locked`.
- Run the architecture checks and Xvfb smoke suite documented in
  [AGENTS.md](../AGENTS.md); report unavailable checks as not run.
- Add pixel baselines and direct user interaction tests when presentation
  or input semantics change; do not rely exclusively on compilation.
- Record actual hardware and frame/memory measurements before calling a
  backend "faster" or "lightweight."
