# SLOPOS native application platform

Status: **target plan, not implementation truth**. The authoritative existing
source inventory is [architecture.md](architecture.md). Product behavior and
canonical app names come from [contract/06-applications.md](contract/06-applications.md)
(§§15–18); the rollout and integration rules are in
[swarm-execution.md](swarm-execution.md) and
[ui-toolkit-program.md](ui-toolkit-program.md).

## Owner decision

The SLOPOS monorepo is the **long-term default home** for the MIT-licensed,
Rust-first UI toolkit, SLOPOS shell and native first-party applications.
Build app features alongside the toolkit using dependency-ready vertical
slices. A separately versioned toolkit *repository* is **optional**, never
an automatic gate; versioned public APIs and independent external crate
usage remain required. The general-purpose toolkit and native apps must not
require an active SLOPOS desktop session.

SLOPOS-I remains X11 + Openbox; changes here do not authorize a compositor
rewrite, Wayland migration or a PARAMA port. Classic visual geometry and
existing daily-use behavior remain protected.

## Dependency boundaries

| Layer / owner | Responsible for | Forbidden dependency |
|---|---|---|
| Portable `slopos-ui` | Widgets, themes, scenes, layout, focus, animation, text, accessibility | Shell, D-Bus, system providers, X11 SDK types, app business state |
| Renderer adapters | CPU reference and optional GPU backend; bounded cache and rendering resources | Application-specific workflows |
| Platform host / future `slopos-appkit` | Window lifecycle, documents, menus, clipboard, drag/drop, chooser, platform integration | SLOPOS session for general-purpose apps |
| `slopos-services` | Typed authoritative state, capability discovery and permissioned actions | Widget implementations |
| Shell / Dock | Global shortcuts, menu bar, popovers, OSD, notifications, app launching | App-private models |
| `apps/*` | UI controllers, isolated app models and user journeys | Private shell internals and raw GTK UI |

All public toolkit primitives are platform-neutral. Hardware integrations
are adapters. Native macOS/Windows/Wayland/PARAMA hosts require their own
port and acceptance gates; no unsupported portability claims.

### Shared ownership — do not fork it in each app

- **UI controls:** toolkit owns Buttons, Entries, Menus, Dialogs, Lists,
  Trees, Tables, Scrollbars, layout, focus states, themes and accessibility.
  Apps compose controls and request missing atoms from the toolkit owner.
- **Chooser, clipboard, Open With, MIME dispatch and file actions:** shared
  appkit + typed platform services. Files consumes these APIs rather than
  being the hidden single implementation.
- **Network, Wi-Fi, Bluetooth, sound, power, brightness, date/time:** one
  `slopos-services` provider per domain. Settings and all shell widgets
  consume the exact same capability/state and write-readback contract.
  No hardcoded SSIDs, devices, clock values, GPU metrics or permissions.
- **System actions:** global shortcuts and notifications are shell-owned.
  Apps export named commands/actions and must not start competing daemons.
- **Graphics/media:** shared rendering/color/image/cache contracts. Image
  Viewer owns viewer interactions, Media owns player interactions. Engines
  (FFmpeg, decoders, graphics drivers) are independently audited dependencies.
- **Installation:** Software owns the curated catalog, download/verify,
  transaction, permission prompt and rollback flow; no fake package manager.

## Canonical first-party products and proof slices

These paths are target names from the product contract. A listed planned app
does not already exist just because it appears here. Existing GTK shell,
Settings and Software donors remain functional until replacements pass QA.

| Product | Canonical path | First honest end-to-end slice | Shared toolkit/service demand |
|---|---|---|---|
| Calculator (existing) | `apps/calculator` | Preserve keyboard/pointer arithmetic and accessible screenshot parity | Buttons, focus, Classic appearance |
| Files | `apps/files` | Open/navigation/back/up/select/file-open and failure handling | Virtual list, icon grid, menu, MIME, filesystem |
| Control Panels | `apps/control-panels` | Live Network and Sound state, supported mutations with readback | Text entry, form, status, typed service |
| Software | `apps/software` | Trusted catalog/detail/verified install/cancel/remove | Cards, progress, job lifecycle, permission prompts |
| Image Viewer | `apps/image-viewer` | Open/fit/zoom/pan/next-previous, bounded thumbnail cache | Scalable image surface, transform, color |
| Media (video/audio) | `apps/media` | Open/play/pause/seek/volume/subtitles with real decoder errors | Video surface, clock sync, overlays |
| File dialogs | Future `slopos-appkit` | Open/save/overwrite/cancel usable by any app | Focus, modal state, filesystem model |
| Wi-Fi/Bluetooth/clock popovers | Shell components | Live snapshot, accessible panel, supported actions only | Popover, layout, provider, locale |
| Volume/brightness OSD | Shell components | Hotkey → provider readback → transient non-focus-stealing OSD | Motion, overlay, service, global shortcut |
| Notifications | Shell donor/native transition | Deliver/replace/expire/close; correct source/action | Overlays, animation, accessibility |
| Dock / launcher | Existing Dock / shell donor | Preserve real window mapping, launch, search; later menus/reorder | Drag/drop, animations, app discovery |
| Terminal/Notes/System Monitor/Screenshot | See contract §16 | Backend/model and proof journeys when prerequisites exist | PTY, text, tables, capture, export |

No informal duplicate `apps/photos` or `apps/video` projects: maintain
the contract's `apps/image-viewer` and `apps/media`. Calculator is an
existing regression fixture, not a request to rewrite it.

## Parallel vertical slices

**Wave 0 — start in parallel, headless or toolkit-independent:**
CPU renderer and widget conformance; Files safe filesystem model/MIME tests;
Image Viewer decoding/cache tests; Media decoder capability/state-machine
tests; Software trust and transaction model; provider fixtures/permission
tests; isolated QA and performance harness. All workers can implement/test
models in distinct owned files. App UI waits for agreed widget contracts.

**Wave 1 — toolkit-enabled end-user journeys:**
Files navigation, Image Viewer opening, real Settings network/audio panels,
Software catalog detail/installation, first Wi-Fi popover and OSD, native
chooser. Each slice needs real input, success, failure and accessibility QA
before replacing a donor or being called complete.

**Wave 2 — high-performance UI and playback:**
Optional GPU renderer with pixel/focus parity and CPU fallback; video and
subtitle playback; virtualized huge lists, richer image previews; theme and
motion integration into shell. Do not block CPU-only apps on GPU milestone.

**Wave 3 — platform portability/extended suite:**
Native host adapters and on-platform Windows/macOS smoke tests; advanced
Files/media features; Terminal, Notes, System Monitor, Screenshot and other
contract apps; a *separately approved* PARAMA study. No implicit rewrite of
the current compositor.

## Definition of done per vertical slice

1. One real pointer **and** keyboard user journey reaches persistent/visible
   state; error/permission/cancellation behavior is observable.
2. Reusable control semantics live in the toolkit; test focus, selection,
   accessible names/roles/actions, high contrast and reduced motion.
3. All slow I/O/decoder/provider operations are off the UI thread; use
   bounded resources, cancellation/backpressure, and event-based wakeups.
4. Test missing network/Bluetooth/monitor/hardware and hostile input; report
   unavailable/unsupported truthfully.
5. Headless tests, UI snapshots (scale/font/theme/backend recorded), Xvfb
   integration tests and package/launch/quit tests are distinct evidence.
6. New dependencies, codec binaries, fonts, icons, model files, shaders,
   firmware and redistributable assets receive source-license/provenance
   review. MIT first-party code does not make bundled components MIT.
7. For Files: atomic copy/rename, symlink traversal, trash/permissions, avoid
   unintended deletes and protect in-use files. For Software: trust source,
   authenticity, correct architecture, safe staging, consent, rollback.
   For media: malformed file/decoder limits, timing/color/subtitles.
   For system settings: read-after-write, elevation denial, hotplug/reconnect
   and no secret disclosure.
8. Actual performance results name hardware, OS, backend, input workload,
   idle CPU/memory, p95 frame/input latency and test SHA. Raspberry Pi
   Zero 2 W is *aspirational*, not verified.

Follow [swarm-execution.md](swarm-execution.md) for task IDs, file leases,
dependency gates and the coordinator's integration procedure. The original
contract (§§15–18) retains authority over complete feature requirements.
