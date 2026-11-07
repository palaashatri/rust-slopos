# P1 release program — ship SLOPOS X11

**Autonomous agent entry point:** [GOAL.md](../GOAL.md).
The validated machine-readable work graph is
[qa/p1-goal.json](../qa/p1-goal.json) and the durable checkpoint ledger is
[qa/p1-progress.json](../qa/p1-progress.json). See
[autonomous execution](autonomous-execution.md) for Mac/Windows
controller + Linux guest, evidence, review and disk-budget rules.
The presence of a task or screenshot does not establish P1 readiness.


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
   and reliable color. The initial PNG/SVG-only app and package entries in this PR are a
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

## Status on PR #37 (2026-11-07)

Evidence for each item is in [the ledger](contract/ledger.md): rows
DOCK-AUTOHIDE, VOLUME-MIXER, REVIEW-37 and PR37-REVIEW-THREADS.

**Done on this branch**

- **Dock auto-hide:** right-click menu, `slopos-dock --autohide on|off`, saved
  in `dock.conf`, a 4-pixel reveal strip, an accessibility checkbox, and the
  strut follows the window manager's placement. `scripts/qa/dock-autohide.sh`.
- **Volume Mixer:** the top-bar sound popover and the standalone app share
  `MixerView`. The sound server is connected again on opening the popover, on
  R, and on "Try again"; an unavailable server is shown as text.
  `scripts/qa/volume-mixer.sh` and `scripts/qa/topbar-mixer.sh`.
- **Packaging:** the Volume Mixer and Software binaries, desktop entries and
  payload checks in the Debian, Arch and ISO builders; the GStreamer tools the
  Debian media tests need.
- **Smoke test:** `scripts/smoke-session.sh` runs the Dock, Volume Mixer and
  popover checks after its session checks.

**Pending, by P1 item**

1. **Files:** pagination over 8192 rows, drag and drop, icon view,
   thumbnails, properties and removable volumes. None is in this PR.
2. **Image Viewer:** still the PNG/SVG proof slice (item 3 above).
3. **Software:** no trusted catalogue or publisher key is shipped
   (`SOFTWARE-SHIPPED-CATALOGUE`, MISSING).
4. **Installer and Secure Boot:** there is no installer and no Secure Boot
   support (`INSTALLER-SECURE-BOOT`, MISSING).
5. **Shell surfaces:** Wi-Fi, Bluetooth, battery and clock popovers; volume
   and brightness OSDs; a notification history.
6. **Dock:** keyboard focus; applying `--autohide` to a running Dock (the CLI
   says it takes effect at the next start); a 4-pixel strip on an odd physical
   height at scale 2 leaves the last device row uncovered (the toolkit's
   geometry is logical); drag-to-reorder and pin/unpin.
7. **Not run (NOT_RUN):** event delivery after a sound-server reconnect; the
   provider against a server that fails writes but answers reads; recovery
   when the server is lost while the mixer is open; a Dock moved by the window
   manager; real hardware; multi-monitor; HiDPI at run time; an AT-SPI
   walkthrough; the Design Bible review of visible UI.
8. **Packaging CI on the current head:** the Debian and Arch package builds
   are run by CI; their result is recorded on the PR, not here.

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
| Image Viewer | PNG/SVG proof app plus desktop/package entries added to integration PR; end-to-end QA pending | JPEG and typical formats, limits, open/navigation, packaged binary |
| Files | Read-only, toolkit-native Files bootstrap proposed in PR #35; PCManFM remains donor | Complete safe copy/move/rename/Trash, dialogs, massive dirs, real installed QA |
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

## Automated one-goal release preflight

[GOAL.md](../GOAL.md) is the entry point. The task graph and durable state
are `qa/p1-goal.json` and `qa/p1-progress.json`. From the isolated Linux
VM only, `SLOPOS_P1_GUEST=1 bash scripts/verify-p1-release.sh` refuses
an incomplete/untested ledger, checks storage and runs the existing
release QA gates. Even its success requires a final human/independent
visual and hardware readiness review. Do not use its summary as evidence
of features that have not been implemented.
