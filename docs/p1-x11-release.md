# P1 release program — ship SLOPOS X11

**Scope:** SLOPOS-I Linux/X11/Openbox, custom MIT first-party UI toolkit,
practical lightweight daily-use applications and shell features.

**Not in P1:** PARAMA, custom Wayland compositor, Loom, Donkey, JWhisper,
JVosk, JForge or other independent repositories. Historic source work is
preserved and selectively salvaged but does not become the active compositor.

**Product truth:** [architecture.md](architecture.md) documents what runs.
[contract/06-applications.md](contract/06-applications.md) defines app
requirements; [contract/02-design.md](contract/02-design.md) §5.2 is the
existing binding **SLOPOS Design Bible**. Don't create a second Design Bible.

## P1 exit criteria (no exceptions for impressive screenshots)

A clean Linux VM/image must start an X11/Openbox session reliably, offer a
working Dock/menu/launcher/clock and real network/audio status, and provide
these useful native SLOPOS toolkit apps:

1. **Calculator** — already implemented, regression protected.
2. **Files** — browse, navigate, select, open, copy/move/rename/Trash with
   explicit overwrite/permission error paths and no data loss.
3. **Image Viewer** — PNG, JPEG and common image formats, fit/zoom/pan,
   navigation, thumbnails or lightweight directory handoff, orientation
   and reliable color. The initial PNG/SVG-only app in this PR is a
   **proof slice, not P1 completion**.
4. **Media** — video and audio playback with pause/seek/volume, subtitles,
   reliable codec discovery, and video synchronization; report missing
   codecs, no silent fake UI. GPU/zero-copy is opportunistic, CPU works.
5. **Control Panels** — real display, appearance, network, sound and
   Bluetooth capability/state/actions, plus graceful unsupported behavior.
6. **Software** — honest curated trusted installer with verified source,
   safe atomic transaction, progress, cancel, uninstall/rollback.
7. **Small shell surfaces** — Wi-Fi/Bluetooth/clock/menu popovers, volume
   and brightness OSD, notifications, and Dock, integrated with the same
   typed service providers.

Do not call a prototype "shipped" until installed-package workflows,
keyboard/pointer/a11y, error-path handling and screenshot/restart tests
pass for actual release artifacts. The existing GTK Settings/Catalogue
donors may serve users until equivalent native replacements are proven,
but P1 release requires replacing the first-party GTK UI in the chosen
core app/shell surfaces.

## Implementation milestones

| Milestone | Work | Gate / unlock |
|---|---|---|
| P1-00 — preserve + baseline | Archive divergent historic sources, inventory main, CI, current runtime and contractual QA | Exact SHA/asset provenance and a clean Xvfb baseline; no deleted branches |
| P1-01 — toolkit essentials | Owned buttons, scroll/list/tree/icon grid, search/text input, menu/popover, dialog, clipboard, drag/drop, accessible focus/action; resource budgets | Each control has keyboard/pointer/a11y tests, Classic 1x/2x baseline and no GTK in new apps |
| P1-02 — file and image journeys | Native Files (read-only browse first, then safe operations) + Image Viewer (expand proof to real JPEG/common formats) | Open/zoom/navigate/cancel/failure on real files; safe path handling; packaged and launched on VM |
| P1-03 — media and native settings | Media player with optional external codec engine; network/sound/display/appearance panels via real providers | Playback and subtitle timing; daemon-loss/permission tests, no blocking UI thread |
| P1-04 — Software and shell replacement | Signed curated installs, progress, rollback; native popovers/OSD/launcher and theme fidelity | No sample service values; negative authorization and install tests; donor replacement only after parity |
| P1-05 — P1 shipping gate | Installed DEB/Arch image, clean-user session QA, hardware-dependent capability checks, performance and soak | All required core journeys, repair/recovery, accessibility, package install/update/remove and resource evidence |

Milestones are a **dependency graph**, not a demand to serialize all work:
renderer/widget QA, filesystem model, decoder probes, service adapters and
installer transaction models should run concurrently on separated paths.
Only each dependent native widget/screen waits for its shared control.

### In-scope application phases

| Application | Current status at this planning revision | First shippable gate |
|---|---|---|
| Calculator | Existing functional Rust toolkit app | Regression smoke and packaging |
| Image Viewer | Initial PNG/SVG-only app added to integration PR, CI outcome pending | JPEG and typical formats, limits, open/navigation, packaged binary |
| Files | Planned; PCManFM donor | Native navigation + tested nondestructive file ops |
| Media | Planned | Real playback/decode, subtitle/timing, errors and platform codec licensing |
| Control Panels | Legacy GTK settings | Real native panels for core providers |
| Software | Legacy GTK catalogue | Native UI and trusted install lifecycle |
| Shell widgets | Legacy GTK pieces and some Rust Dock | Shared UI, provider-backed status, OSD and native overlay interactions |

### Testing matrix

- **Code:** cargo metadata --locked, fmt, clippy -D warnings, tests,
  architecture checks, script syntax.
- **Headless:** component pixel goldens (Classic 1x/2x), keyboard/
  accessibility trees, provider fixtures, bounded memory and cancel.
- **VM integrated:** Xvfb/Openbox startup, click/keyboard/launch/window
  focus/resize/quit, actual directory/image/media files, app install,
  shell logout/restart, screenshots, package install/remove.
- **Hardware:** low-end Pi Zero 2 W CPU/RAM profile where available;
  high-end GPU accelerated path separately; Wi-Fi/Bluetooth/display,
  HDR/VRR and input peripherals need explicit test hardware.
- **Performance:** idle CPU/wakeups; cold launch; resident/private memory;
  p95/p99 input/frame time and scroll; hardware and exact git SHA recorded.

A missing hardware capability is **NOT_RUN** or **UNAVAILABLE**, never PASS.

## Native-app implementation rules

- Keep portable app state models and shared widgets as separate modules.
  Never call filesystem, network or media codecs synchronously from
  high-frequency UI callbacks. Use bounded queues, explicit cancellation
  and event-based redraw.
- Viewer photo decoding must check encoded *and decoded* pixel budgets,
  image metadata, orientation and malformed input. A file-size limit is
  not sufficient to prevent decompression bombs.
- Files must never delete an existing target while rolling back a partial
  copy. Source removal occurs only after atomic destination completion
  and verified state; cross-volume semantics and safe Trash handling
  get dedicated tests.
- Media decoders/players are behind adapters. Don't assume all FFmpeg
  codec builds are MIT-redistributable, and don't couple the UI to a codec
  subprocess or engine's event loop.
- Use freedesktop standards for file associations, authentication and
  device integrations where practical. Installer may not silently
  mutate distribution-managed packages.
- Keep UI contracts stable and review dependency licenses. Original
  SLOPOS first-party code is MIT; bundled assets and dependencies
  require their own license audit.

## Swarm dispatch

The coordinator uses [swarm-execution.md](swarm-execution.md) for exact
task ownership, dependencies and handoffs and checks
[historical-recovery-audit.md](historical-recovery-audit.md) for old
work prior to writing new algorithms. Restore **pure models/tests**
from archived snapshots under a separate new module only if tests and
API compatibility prove useful; never bulk-merge the old Wayland tree
into current X11. Maintain one integrator for Cargo/lock/packaging/CI
changes and one exclusive VM runtime lease per shared test environment.

Next recommended tranche after this integration PR:
UI text/scroll/menu control parity, Files read-only browser model,
image-format decoder limits, Media playback engine capability
probes and Software trust transaction tests, concurrently.
