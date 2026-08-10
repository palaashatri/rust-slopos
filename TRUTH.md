# TRUTH.md — SLOPOS-I Audit and Evidence Ledger

**Purpose:** This is the sole factual status, score and defect ledger for
SLOPOS-I. Final requirements and execution rules live in `AGENTS.md`.
`README.md` is the public introduction.

**Audited product implementation:**
`6013afcc82769ffc5d207d1a46de4add62bd2fe1`
**Audit date:** 2026-08-10
**Audit basis:** source review of this branch, plus the exact Ubuntu 26.04
x86_64 VM gates retained under
`artifacts/qa/coordination/current-wave-6013afc/` and the unchanged Rust gate
wave under `artifacts/qa/coordination/current-wave-c6e7f6a/`. `cargo fmt --all -- --check`,
`cargo check --workspace --all-targets --locked`,
`cargo test --workspace --locked`,
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`,
and `cargo build --workspace --release --locked` all exited 0 with
the Rust source tree at `c6e7f6a`; `git diff --quiet c6e7f6a..6013afc -- crates
Cargo.toml Cargo.lock` exited 0, proving this packaging-only wave did not alter
that tested Rust tree. The packaging VM syntax and canonical repository URL
checks at `6013afc` also exited 0. A read-only
`cargo metadata --locked --format-version 1 --no-deps` check and a clean
`git diff --exit-code -- Cargo.lock` also exited 0. The earlier baseline
additionally covered the app-bundle packaging path, session-file dry run, and
owned-artifact cleanup allow-list checks.

The viewport path now propagates a validated effective rational scale through
nested and DRM render/readback calls. The compositor rejects logical/physical
extent mismatches, and the atomic `viewport-state.json` contract records output
dimensions, requested/effective scale, layer configure serials and committed
frame revision. `scripts/viewport_gate.py` checks those fields, PNG hashes and
dimensions, and contiguous clear/unpainted edge bands; its deterministic fixture
passes, its three-pixel clear-edge fixture fails, and fixture provenance is
rejected by normal runtime mode. The Ubuntu VM could not produce runtime
evidence: nested Smithay X11 needs DRI3 (unavailable in the installed Xvfb/Weston
path) and the DRM node was busy. Fractional-scale pixel coverage and the live
resize matrix therefore remain unproven.

Portal operations without authoritative services now fail closed rather than
fabricating settings, file selections, remote launches, keyring values or
PipeWire node IDs. The frontend registration now uses the standard
`org.freedesktop.portal.Desktop` bus/path and Request/Response/Close lifecycle,
but it remains an incomplete backend; live PipeWire and browser consumer
evidence are absent. App Store suggestions from an empty or unsigned
catalog are labelled `FEATURED` and unknown entries are no longer reported as
available; the signed install/update/remove transaction service is still
incomplete. The shell no longer exposes commands that only opened placeholder
status windows. The D-Bus Print portal also returns an error until a job is
actually submitted to a print service.

The latest source wave also makes NetworkManager connection requests wait for
and report the real command result, requires a second explicit activation for
destructive session actions, and exposes safe installed-bundle update/remove
transactions with rollback-preserving installation paths. These are
source/build/test verified in the exact VM gate; they do not establish a full
distribution catalogue, authentication stack, or clean-machine release path.
The About dialog now uses the factual development-preview label rather than a
production/version claim. Unsupported Settings.ReadAll and ScreenCast
CreateSession calls now fail closed; standard portal registration and the live
PipeWire graph remain unimplemented. Session Recent Items now persists a bounded,
de-duplicated, versioned list atomically under the XDG data directory and reloads
it on startup; malformed, symlinked and wrong-version files fail closed. It has
no removal UI or retention policy beyond the bounded list.
Finder's sidebar and status-bar menu actions now toggle their real widgets and
reflow the file grid; the path bar remains hidden until a dedicated widget is
implemented. The default shell menu also no longer exposes unindexed Help
Search or fallback Rename actions, and their unavailable handlers are no
longer routed from shell dispatch.
First-party Finder, Settings, Terminal, TextEdit and App Store menus likewise
omit actions whose client handlers do not exist; their remaining visible
actions are backed by the tested application or compositor paths.
**Public target:** a 100/100 production Linux desktop environment that genuinely
competes with KDE Plasma and GNOME as a daily driver.
**Current verdict:** **63/100 — functional custom desktop alpha.**

### Open release blockers

- The compositor viewport producer and runtime driver are source/build tested,
  but a real nested/DRM framebuffer plus layer configure/ack/frame cycle is not
  yet observed in the available VM. Fractional scales and the required resize
  matrix remain release-blocking until that runtime gate passes.
- The portal frontend now uses standard names, but it has no live
  permission-mediated PipeWire graph or Firefox/Chromium consumer evidence;
  the service backends remain incomplete.
- First-party Settings, shell controls, software management, security/permission
  services, third-party compatibility, accessibility workflows, recovery and
  physical DRM/HDR/VRR gates remain incomplete or unverified.
- The stage-4 harness now validates all release ELF/session assets in a private
  clean-room prefix, but exits 2 while upgrade, rollback and uninstall
  transactions remain explicitly unverified. The Arch package is pinned to the
  audited main archive until a signed release tag is published.

### Historical implementation wave — full-width shell chrome and shaped label geometry

The following UTM material is retained historical evidence, not evidence for
the current branch head. Its cited source hash is not present in this checkout.

Implementation commits `4676455`, `2edabd0`, `c790465`, `fde021f`, `e137585`,
`7a01cba` and `6153e4e` remove the two regressions reported in the current UTM
captures. Anchored background, menu and Dock layer surfaces now request
compositor-sized width (`0`) instead of pinning themselves to a stale startup
width. Configure events resize the shell UI runtime and invalidate its pixels.
Shared `cosmic-text` shaped measurements now drive menu titles, SDK
status/menu advances, labels, tabs, dialogs, lock-screen text and button
geometry; UTF-8 byte/character estimates are no longer used for those layout
decisions. The final commit isolates test-only menu manifests with an RAII
temporary directory guard; production routing is unchanged. These changes fix
layout and hit geometry. They do not establish that every font family, script,
scale or text-editing path is production-complete, and this wave does not claim
a glyph-rasterizer rewrite.

The exact Ubuntu UTM checkout identified by the source-hash artefacts as
`6153e4e` is recorded under
`artifacts/qa/2026-08-10-utm-6153e4e-final/` and
`/home/ubuntu/rust-slopos-qa-7a01cba` on Ubuntu 26.04 aarch64. The locked UTM
format, workspace check, Clippy and release workspace build all exited `0`;
the workspace test log contains zero failing tests, including 347
`slopos-shell` unit tests and the repaired
`tests::global_menu_shortcut_opens_new_finder_window`. The DRM session reached
`Virtual-1` at 1280×800, scale 100%, sRGB 60 Hz; the visual runtime used the
UTM software-rendered path. Finder and Settings launched with active app IDs
`com.slopos.finder` and `com.slopos.settings`.

The fresher Classic-theme screenshots under
`artifacts/qa/2026-08-10-utm-label-metrics-6153e4e/` were manually inspected
and accepted for the desktop, Finder and Settings. They show the shell filling
the visible UTM output without the previous right-side gray strip, and show
toolbar, sidebar, theme-button, status, desktop-icon and Dock labels contained
by their measured geometry. The earlier menu-dropdown attempt in the build
gate is not accepted as evidence because UTM coordinate input did not open the
dropdown. The screenshots are visual runtime evidence only; they do not prove
all scripts, fractional scales, IME, grapheme editing, assistive-technology
workflows or hardware rendering.

The overall SLOPOS-I verdict remains **63/100**; no completion or renderer score
increase is justified from this wave.

### Current implementation wave — live widget accessibility bridge

Implementation commit `efaf2c9814690cb89271a811606fabbb590498b6` projects each
live SDK window recursively through the toolkit `Widget` tree into AT-SPI.
Generated nodes retain their live `WidgetId`, refreshed bounds, enabled,
visible and focused state, and stable object paths across sibling reorder,
addition and removal. Authored semantic child lists remain authoritative, so
custom controls are not duplicated. Nested stable descendants can receive
focus state and Focus/StateChanged events. SDK applications register after
initial layout, synchronize after dispatch, menu actions, layout, paint and
update, and retry registration at most once per second when a session/a11y bus
was not ready at startup. Finder, Settings and TextEdit now expose semantic
Window roots.

The host format/check/test/Clippy/release gates all exited `0` under
`artifacts/qa/2026-08-09-a11y-live-bridge-host/`. The exact Ubuntu UTM checkout
at this SHA also passed all five locked gates under
`artifacts/qa/2026-08-09-a11y-live-bridge-utm-r2/`; the workspace test log
contains zero failing tests, including 343 shell tests and the expanded live
accessibility suites.

The explicit runtime probe under
`artifacts/qa/2026-08-09-live-atspi-efaf2c9-r3/` ran the release TextEdit client
against an explicit headless Wayland socket inside `dbus-run-session`. The
dedicated AT-SPI bus exposed 17 stable widget objects; a text-field object
introspected with Accessible, Action, Text and Component interfaces, returned
role `text`, and served `GetText`. The Settings probe under
`artifacts/qa/2026-08-09-live-settings-atspi-efaf2c9/` exposed a root named
`Settings` and 28 live widget objects on the same explicit Wayland/D-Bus path.

This proves live AT-SPI object export and recursive widget projection on the
Ubuntu software/headless path. It does not prove Orca workflows (Orca is not
installed in the guest), keyboard-only completion of the entire desktop,
caret/selection synchronization under real typing, high-contrast or reduced-
motion acceptance, physical input, hardware, third-party accessibility, or
long-running reliability. Accessibility readiness advances conservatively from
38 to **46/100** and accessibility infrastructure from 55 to **64/100**; the
overall SLOPOS-I verdict remains **63/100**.

Documentation commits after the audited implementation do not change the
product score unless they are accompanied by implementation and evidence.

### Current implementation wave — compositor-owned focused-window output migration

Implementation commit `a669059be4e6a1f43a8e636a239aa89de356396d` adds a typed
`MoveActiveWindowToOutput` Spaces command. The nested compositor validates the
connector and focused native window before applying a pure, tested migration
plan that remaps/clamps normal and restored geometry, preserves fullscreen and
other presentation states, updates restore output identity, sends the new
configure and rebinds output membership. The DRM backend rejects unavailable
connectors and XWayland focus explicitly instead of mutating shell bookkeeping;
the single-connector DRM limitation remains honest.

The exact Ubuntu UTM checkout is
`/home/ubuntu/rust-slopos-qa-xwayland-current-2` on
`ubuntu@192.168.64.17`, detached at the implementation SHA. Exact locked
format, workspace check, focused bus protocol tests, focused compositor
output-migration tests, full workspace tests, Clippy with `-D warnings`, and
release workspace build all exited `0` under
`artifacts/qa/2026-08-09-exact-a669059-utm/`. The host dirty-source regression
set also exited `0` under
`artifacts/qa/2026-08-09-output-migration-host-r1/`.

This wave proves typed protocol and geometry policy plus build/test evidence;
it does not prove live multi-output migration, DRM/KMS connector hotplug,
XWayland output migration, physical displays, rendering pixels, or hardware.
The DRM path still supports only its current connector. The overall SLOPOS-I
verdict remains **63/100** and strict compositor completion remains **77/100**;
no score increase is justified from this software-only gate.

### Current implementation wave — Settings output-migration control

Implementation commit `9c88d17c8d1a5cf7018cd12e74d4f2e47be53184` exposes a
Settings Spaces action labelled `Move Active Window to Output`. It validates
empty and control-character connector identifiers and sends the typed
compositor-owned request; it does not maintain or apply a shell-side geometry
model. Host format, Settings tests and Settings Clippy all exited `0` under
`artifacts/qa/2026-08-09-output-migration-settings-host-r1/`. The exact-commit
UTM checkout at the same SHA passed format, all 24 Settings tests and Settings
Clippy under
`artifacts/qa/2026-08-09-output-migration-settings-ui-utm-r2/`; the exact
release Settings build exited `0` in
`artifacts/qa/2026-08-09-output-migration-settings-ui-utm-r3/`.

The Settings evidence proves request emission and validation only. It does not
prove that a physical or DRM multi-output session can perform the migration,
nor does it prove XWayland migration, third-party compatibility, rendering,
accessibility or hardware. The overall SLOPOS-I verdict remains **63/100**;
displays and Spaces remain below their completion gates.

### Current implementation wave — headless synthetic gesture runtime evidence

Implementation commit `7032c7dd1bad2583866d0e1172aa19fed1066a62` adds an
explicit, headless-only control path for `GestureSwipeBegin`,
`GestureSwipeUpdate` and `GestureSwipeEnd`. The nested/headless compositor feeds
those events through the same `WorkspaceSwipeRecognizer` and compositor-owned
Space reducer used by the DRM/libinput path. The retained runtime gate proves
Space 1 → Space 2 and Space 2 → Space 1 transitions, rejects a short swipe,
rejects a cancelled swipe, and isolates persisted user Spaces with a temporary
`XDG_DATA_HOME` fixture.

The exact Ubuntu UTM checkout is
`/home/ubuntu/rust-slopos-qa-xwayland-current-2` on
`ubuntu@192.168.64.17`, detached at
`7032c7dd1bad2583866d0e1172aa19fed1066a62`. The exact locked gates all exited
`0` under `artifacts/qa/2026-08-09-exact-7032c7d-utm/`: format, workspace
check, workspace tests, Clippy with `-D warnings`, and release workspace build.
The workspace log records 175 compositor library tests, 12 compositor binary
tests, 34 compositor contract tests, 17 compositor integration tests and 339
shell tests with zero failures. The exact synthetic runtime result is under
`artifacts/qa/2026-08-09-exact-7032c7d-utm/gesture-runtime/`; its machine-
readable fields are `synthetic_next_verified=true`,
`synthetic_previous_verified=true`, `short_gesture_rejected=true`,
`cancelled_gesture_rejected=true`, `physical_input_verified=false` and
`hardware_verified=false`.

This is deterministic headless policy/runtime evidence only. It does not prove
physical touchpad delivery, libinput device hotplug/resume, DRM/KMS, pixels or
rendering, third-party compatibility, accessibility workflows, or hardware.
The overall SLOPOS-I verdict remains **63/100** and strict compositor
completion remains **77/100**; no score increase or 100/100 claim is justified
from this synthetic gate.

### Current implementation wave — compositor-owned three-finger Space swipes

Implementation commit `b9ab5045107a230817d3814c0a70f680778b4c00` adds a
backend-neutral swipe reducer and wires real Smithay/libinput swipe events in
the DRM session. Three-or-more-finger gestures accumulate finite deltas and
commit exactly one next/previous Space action only when horizontal movement
passes the explicit distance and dominance thresholds; short, vertical,
cancelled, non-finite, and two-finger gestures are rejected. The DRM path
still forwards begin/update/end gesture events to the focused Wayland client,
and locked sessions cannot change Spaces through the compositor gesture path.

The exact Ubuntu UTM checkout is
`/home/ubuntu/rust-slopos-qa-xwayland-current-2` on
`ubuntu@192.168.64.17`, detached at
`b9ab5045107a230817d3814c0a70f680778b4c00`. The exact-commit locked gates
all exited `0` under `artifacts/qa/2026-08-09-exact-b9ab504-utm/` (format,
workspace check, workspace tests, Clippy with `-D warnings`, and release
workspace build). The workspace log records 178 compositor tests and 339
shell tests with zero failures; the focused pre-commit UTM run also passed 175
compositor library tests and 33 compositor contract tests under
`artifacts/qa/2026-08-09-gesture-focused-utm-precommit/`.

This is implementation and automated Linux-guest evidence only. The current
UTM run has no physical touchpad or DRM seat available, so no physical gesture
delivery, libinput device hotplug/resume, or pixel-inspected Space transition
was observed. The overall SLOPOS-I verdict remains **63/100** and strict
compositor completion remains **77/100**; no score increase or 100/100 claim
is justified from these gates.

### Current implementation wave — rootless XWayland scene and Spaces recovery

Implementation commit `3b6323a010138600296b82ff6f0e92c7413c82df` connects
rootless XWayland windows to a compositor-owned scene registry in both nested
and DRM paths. The registry now carries X11 geometry, map/unmap/destroy
lifecycle, Wayland-surface association, compositor-authoritative Spaces
membership, output-aware visibility, keyboard-focus selection, override-redirect
focus policy, and X11 move/resize grabs. XWayland disconnect cleanup removes
stale scene entries and window memberships. The same wave adds bounded
pre-Ready startup recovery and quarantines malformed persisted Spaces state
before restoring a valid default model.

The exact Ubuntu UTM checkout is
`/home/ubuntu/rust-slopos-qa-xwayland-current-2` on
`ubuntu@192.168.64.17`, detached at
`3b6323a010138600296b82ff6f0e92c7413c82df`. The exact-commit XWayland gate
exited `0` under
`artifacts/qa/2026-08-09-exact-3b6323a-utm/`; its JSON records scene mapping,
configure, keyboard-focus selection, unmap/destroy cleanup, compositor
survival after client exit, and startup-watchdog recovery as true. The same
JSON explicitly records `scene_rendered_verified`, `scene_hit_test_verified`,
`rendering_verified`, `physical_input_verified`, `drm_verified`,
`hardware_verified`, `nested_dri3_available`, and
`broad_x11_compatibility_verified` as false.

The exact-commit malformed-Spaces gate also exited `0` under that directory:
the invalid `{"spaces":[]}` file was quarantined byte-for-byte and the
compositor restored eight default Spaces with active Space 1. Before the
commit, the source-equivalent UTM focused run passed format, the 31-test
compositor completion contract, and the release compositor build under
`artifacts/qa/2026-08-09-latest-edit-utm/`; those are not substituted for
the exact-commit runtime provenance above.

This proves rootless XWayland scene lifecycle and persisted-Spaces recovery on
the Ubuntu headless/software path only. It does not prove X11 pixel rendering,
scene hit-testing, clipboard/DnD compatibility with third-party clients,
physical input, DRM/KMS, hardware, broad GTK/Qt/Electron/X11 compatibility,
accessibility, packaging, performance budgets, or long-running reliability.
The overall SLOPOS-I verdict remains **63/100**; strict compositor completion
remains **77/100**, and no 100/100 claim is justified.

### Current implementation wave — bounded XWayland crash recovery

Implementation commit `fa1f839b746f748c74d3f147dc28f4bbf3c26d30` adds a
session-scoped recovery budget to the Smithay XWayland bridge. When the X11 WM
connection closes, the compositor now clears stale X11 surfaces and
associations, removes the stale `SLOPOS_XWAYLAND_DISPLAY`, and starts a fresh
XWayland instance while three recovery attempts remain. Once the budget is
exhausted it records an explicit terminal state instead of entering an
unbounded crash loop. The behavior-level budget test is included in the
compositor unit suite. QA harness accounting was corrected by commit
`92dcd78ae8e6c8200af2a6bd32f8095b79cc834b` to count standalone disconnect
events rather than matching the restart log line twice.

The exact Ubuntu UTM checkout is `/home/ubuntu/rust-slopos-qa-d1ea823` on
`ubuntu@192.168.64.17`, detached at
`92dcd78ae8e6c8200af2a6bd32f8095b79cc834b`. The bounded recovery harness
repeatedly killed the ready XWayland child four times. Its machine-readable
result records four ready events, four standalone WM disconnects, three
replacement starts, compositor survival after each disconnect, and terminal
budget exhaustion with no fifth start. Evidence is under
`artifacts/qa/2026-08-09-xwayland-recovery-92dcd78-utm/`; the retained log
shows displays `:3` through `:6` and the terminal recovery error.

The exact-current-head Ubuntu UTM gates all exited `0`: `cargo fmt`, locked
workspace check, locked workspace tests, Clippy with `-D warnings`, and the
locked release workspace build. The full workspace log includes the new
`tests::xwayland_recovery_budget_is_session_scoped_and_bounded` test with 9/9
compositor binary tests passing. The canonical compositor contract rerun and
schema-12 headless Wayland protocol gate also exited `0`. Evidence is under
`artifacts/qa/2026-08-09-xwayland-gates-92dcd78-utm/`.

This proves bounded XWayland process/WM recovery on the Ubuntu headless
software path only. It does not prove rootless X11 scene integration, X11
application rendering or input, clipboard/DnD compatibility with third-party
clients, physical DRM/KMS, hardware, accessibility, packaging, performance
budgets or long-running reliability. The overall SLOPOS-I verdict remains
**63/100**; strict compositor completion advances conservatively from 76 to
**77/100** because the explicit restart/recovery contract is now runtime
observed, while the broader XWayland and compositor matrix remains open.

### Current implementation wave — DnD target-disconnect cancellation recovery

Implementation commits `c2d32bc62a0f74882ad9ef5b7604d4d304855ecd`,
`7fad997be76900943f4dcaa2d706a00515f5a202`,
`9af7fa8aa2ad9dd1b00a88cb0036fa7afcad31bb`,
`53c4412853e68aa7146ec1da0a4a3d165d8f4467` and
`ef1666faaa9066e419e71f9005f720a08e8471b3` add a deterministic DnD failure
path to the native headless protocol client and compositor contract. A target
can now accept a drag and intentionally disconnect before drop; the source
must receive the protocol `Cancelled` event, and the compositor must remain
alive. The QA script derives the source/target points from the last two
compositor-mapped window origins, keeps the target-only portion outside the
raised source buffer, and preserves the earlier invalid-serial marker in the
combined evidence log.

The exact Ubuntu UTM checkout at the audited SHA is
`/home/ubuntu/rust-slopos-qa-d1ea823` on `ubuntu@192.168.64.17` (detached at
`ef1666faaa9066e419e71f9005f720a08e8471b3`). The headless runtime gate passed
with status `passed`; its machine-readable JSON records
`dnd_target_disconnect_cancelled_verified: true`,
`dnd_target_disconnect_target_exit_verified: true`, the existing invalid
serial/text/URI/icon/drop fields as true, and hardware/DRM/rendering/input as
false. The JSON and retained source/target logs are under
`artifacts/qa/2026-08-09-dnd-utm-ef1666f/`. The target-disconnect source log
contains `SLOPOS_DND_SOURCE_CANCELLED`; the target log contains
`SLOPOS_DND_TARGET_ABORTING` and `SLOPOS_DND_TARGET_DISCONNECTED`.

The exact UTM locked gates all exited `0` under
`artifacts/qa/2026-08-09-dnd-final-gates-utm-ef1666f/` (format, locked check,
workspace tests, Clippy with `-D warnings`, and locked release build). That
workspace test log includes 24 compositor completion-contract tests, 170
compositor unit tests, 339 shell tests, 178 kit tests and 30 App Store tests
with no failures. The same five host gates exited `0` under
`artifacts/qa/2026-08-09-dnd-final-gates-host-ef1666f/`.

This is deterministic native Wayland/headless and Ubuntu VM evidence, not
physical input, GTK/Qt/Electron/XWayland compatibility, DRM/KMS, hardware,
accessibility, performance-budget, packaging/recovery or long-soak proof. The
overall SLOPOS-I verdict remains **63/100** and strict compositor completion
remains **76/100**; the broader compositor matrix is still open.

### Current implementation wave — authenticated App Store catalogs and packages

Implementation commit `4296882e1344e52dad082d4a377c9e9579928798` adds an
Ed25519 authenticity boundary to the local App Store installer. Catalogs and
archives now carry canonical signed metadata, trusted publisher/key identity,
revocation state and optional authenticated archive size. Verification occurs
before checksum validation or extraction; publisher identity is retained for
package details, and the production install path rejects unsigned, unknown,
revoked, tampered or metadata-mismatched inputs. The trust-store loader rejects
symlinked entries and requires restrictive Unix file permissions. The same
commit adds a compositor-authoritative `MoveActiveWindow` caller for the live
Spaces overview using stable Space IDs, with success/failure tests preserving
modal state and authoritative mirrors.

The exact Ubuntu UTM checkout at this SHA is `/home/ubuntu/rust-slopos-qa-d1ea823`
on `ubuntu@192.168.64.17` (detached at the SHA). Its locked workspace Clippy
run with `-D warnings` and locked release workspace build both exited `0`;
their combined output and exit marker are retained under
`artifacts/qa/2026-08-09-appstore-signing-utm-4296882/clippy-release.log` and
`clippy-release.exit`. The focused signed-catalog/archive suite passed 6/6
tests with 24 filtered out under the same directory's
`focused-signing.log` and `focused-signing.exit`. The preceding exact-commit
UTM locked workspace test run passed all suites, including 30/30 App Store
tests, and is retained in `test.log` and `test.exit` there.

The host exact-commit gates also exited `0` for formatting, locked workspace
check, locked workspace tests, Clippy with `-D warnings` and the locked release
workspace build under
`artifacts/qa/2026-08-09-appstore-signing-host-4296882/`. These results prove
authenticated local package handling and headless/VM behaviour only. They do
not prove a network catalogue/update service, publisher operations, sandbox or
permission enforcement, clean install/upgrade/rollback/uninstall lifecycle,
third-party application compatibility, physical hardware, accessibility
workflows or long-running reliability. The package-trust subscore can no
longer describe publisher authenticity as wholly absent, but the weighted
SLOPOS-I verdict remains **63/100** and strict compositor completion remains
**76/100**.

### Current implementation wave — typed display policy, live accessibility, output recovery and active-window Spaces moves

Implementation commit `78fa7de93aa0456efb467d0486c3936c1b0359b2` completes a
bounded set of compositor-authority gaps. The session bus now carries typed
display-policy requests and atomic capability snapshots. Nested/headless
backends apply only capability-supported HDR/VRR/refresh/colour requests and
reject unsupported values without advancing the policy revision; DRM publishes
its detected policy while correctly reporting that runtime mutation is not
available. Settings applies a policy to the live compositor before persisting
it and rolls back both the compositor and UI/config state when persistence or
session delivery fails.

The same commit adds live-tree projection and diff handling for AT-SPI dynamic
Spaces and shell widgets: stable IDs, labels, bounds, selection/focus, window counts, recursive
add/remove projection, and diff-based `ChildrenChanged`, `StateChanged`,
`Focus`, and `BoundsChanged` events are connected to the toolkit tree. Space
output assignment now validates the current connector inventory transactionally
and clears stale persisted assignments after topology changes. A typed
`MoveActiveWindow` command is implemented in both nested and DRM dispatch; it
validates the compositor's activated window against mapped windows before
mutating authoritative membership.

Exact-commit host gates all passed under
`artifacts/qa/2026-08-09-final-gates-host-78fa7de/` (format, locked workspace
check, locked workspace tests, Clippy with `-D warnings`, and locked release
build). The Ubuntu UTM checkout was detached at the exact audited SHA and all
five locked gates also passed under
`artifacts/qa/2026-08-09-final-gates-utm-78fa7de/`; the workspace log includes
170 `slopos-compositor` unit tests, 178 `slopos-kit` tests and 337
`slopos-shell` tests with zero failures. The canonical compositor contract
gate passed 170 unit, 8 binary, 23 contract and 17 integration tests under
`artifacts/qa/2026-08-09-compositor-contract-78fa7de/utm/`.

The exact-commit headless Wayland protocol smoke passed under
`artifacts/qa/2026-08-09-compositor-runtime-78fa7de/utm/`; its machine-readable
result explicitly leaves DRM/KMS, physical input, rendering, XWayland, HDR,
VRR and hardware compatibility false. The display-policy runtime harness under
`artifacts/qa/2026-08-09-display-policy-runtime-78fa7de/utm/` applied `120hz`
SRGB and rejected invalid refresh, HDR, VRR and colour requests without a
revision change. The active-window harness under
`artifacts/qa/2026-08-09-spaces-active-window-runtime-78fa7de/utm/` used a real
Preview client, moved it from Space 1 to Space 2 through the compositor-owned
command, and confirmed that an invalid Space 999 target preserved the revision.

These are software/headless and VM runtime results, not physical-hardware or
third-party compatibility proof. Live thumbnails, shell drag/gesture callers,
Orca workflows, physical DRM/input/output migration, broad GTK/Qt/Electron/
XWayland compatibility, renderer budgets, packaging/recovery, security trust,
and long-running soaks remain open. The overall score therefore remains
**63/100**; no weighted score increase is claimed from this bounded wave, and
strict compositor completion remains **76/100**.

### Current implementation wave — compositor keyboard focus for live Spaces overview

Implementation commits `9ab9aa98a0596b57e8a5a344e2e2aed73cae591c` and
`d7e2b7533c35844ea2b47c5b0f9abb4d779cbb94` close the layer-shell focus gap in
the live SLOPOS Spaces overview. While the overview is visible, the shell now
requests `Exclusive` keyboard interactivity on its exact Overlay surface and
returns it to `OnDemand` when the overview closes. Both nested and DRM
compositor backends read Smithay's cached layer keyboard-interactivity state on
commit, focus only the visible `slopos-i-spaces-overview` surface, and restore
the topmost visible ordinary client (or clear focus) when that surface shrinks
or is destroyed. Escape and successful Space selection also clear the shell's
temporary input filter. The SDK paints a visible accent ring around the local
focused cell; no compositor or shell window model is duplicated.

The exact Ubuntu UTM clone at `d7e2b7533c35844ea2b47c5b0f9abb4d779cbb94`
passed all five locked workspace gates with zero exit markers under
`artifacts/qa/2026-08-08-build-tests-d7e2b75/utm/`. The focused UTM suites
passed 9/9 `slopos-kit` workspace-grid tests and 2/2 live-shell overview tests;
both exit markers are retained under
`artifacts/qa/2026-08-08-spaces-keyboard-focus-d7e2b75/utm/`. The compositor
contract gate passed with 164/164 compositor unit tests, 8/8 compositor-binary
tests, 23/23 completion-contract tests and 17/17 compositor integration tests;
its machine-readable result is under
`artifacts/qa/2026-08-08-compositor-contract-d7e2b75/utm/`.

The exact-commit headless compositor protocol smoke passed with every protocol
flag true under `artifacts/qa/2026-08-08-compositor-runtime-d7e2b75/utm/`, and
the logical output topology rerun passed under
`artifacts/qa/2026-08-08-compositor-topology-d7e2b75/utm/`. Those runtime
artifacts explicitly keep `hardware_verified`, `drm_verified`,
`rendering_verified` and `input_verified` false. The guest had no active Xorg
session or `xdotool`/`wtype` injection path, so a pixel-inspected real
layer-shell keyboard session, physical input, DRM/KMS output behaviour,
third-party compatibility, accessibility, packaging, performance budgets and
long-running soaks remain unverified. The overall score therefore remains
**63/100**; no score increase is claimed from source/build evidence alone, and
strict compositor completion remains **76/100**.

### Current implementation wave — authoritative display topology through compositor control

Implementation commit `802ca6b4a0067d086a6cbf062e6ff1d6fe923ac1` routes Settings
display arrangement and scale requests through the typed `ReconfigureOutputs`
session-control command. The compositor now publishes an atomic
`outputs-state.json` projection containing backend, revision, connector names,
geometry and scale. Nested/headless sessions accept complete uniform-scale
logical layouts atomically; malformed layouts and mixed-scale requests are
rejected without advancing the authoritative revision. DRM publishes its
topology but rejects this runtime logical-reconfiguration path because physical
KMS reconfiguration is not implemented. Settings reads the compositor
projection instead of fabricating a fixed `eDP-1:1920x1080` state, sends the
typed request before persistence, and rolls back UI/config state when the
session is unavailable or persistence fails. Live scale changes remain
unavailable until mixed-scale rendering is implemented.

The exact Ubuntu UTM clone at this SHA passed all five locked gates: `cargo fmt
--all -- --check`, locked workspace check, locked workspace tests, Clippy with
`-D warnings`, and the locked release workspace build. Every exit marker is
`0` under `artifacts/qa/2026-08-08-build-tests-802ca6b/utm/`; the same log
contains 22/22 Settings tests, 331/331 shell tests, 13+3+6 bus tests,
164/164 compositor unit tests, the 23-test completion contract and 17 compositor
integration tests. The exact compositor source/contract gate also passed with
`runtime.exit=0` under `artifacts/qa/2026-08-08-compositor-contract-802ca6b/utm/`.

The extended exact-commit headless topology harness passed with
`runtime.exit=0` and `status: "passed"` under
`artifacts/qa/2026-08-08-compositor-output-topology-802ca6b/utm/`. It verified
logical output add, reorder and removal, Wayland output registry changes,
readiness dimensions, malformed-request rejection, uniform-scale mismatch
rejection and revision-preserving failure handling. The compositor log records
both rejection reasons and all three applied topology transactions.

This is headless logical-output and Settings failure-path evidence only. The
Settings tests and control datagram prove request ordering and delivery, while
the independent compositor harness proves logical application; they do not
prove a pixel-inspected Settings click through to a synchronous compositor
acknowledgement. Physical DRM/KMS hotplug, real output migration, mixed-scale
rendering, hardware input, broad third-party compatibility, accessibility,
packaging, performance budgets and long-running soaks remain unverified. The
overall score remains **63/100**; the bounded Settings functional slice advances
from 56 to **58/100**, while strict compositor completion remains **76/100**.

### Current implementation wave — Settings application-ID Spaces policy controls

Implementation commit `73cbd3f49269a5a4b7a3f10079e3064abef5c5b3` connects the
existing compositor-owned application-ID policy service to the Settings
Spaces panel and the shell's authoritative readback mirror. Settings now
accepts an application ID plus an arbitrary stable Space ID, `all`, or
`current`; it validates the latest compositor snapshot before sending the
typed `SetApplicationPolicy` request, reports malformed/unknown input without
IPC, and renders stored policy rows with real layout geometry. `current`
clears a stored policy. `WorkspaceManager` validates policy IDs, targets and
duplicates transactionally and retains only an accepted compositor snapshot,
including across a compositor session-epoch restart.

Host formatting, locked workspace check/tests, Clippy with `-D warnings`, and
the optimized workspace build passed after this change. The exact Ubuntu UTM
clone at this SHA passed all five locked gates with zero exit markers under
`artifacts/qa/2026-08-08-build-tests-73cbd3f/utm/`. Focused UTM suites passed
compositor 164/164 plus the 23-test completion contract, bus 12/12 plus the
6-test Spaces protocol suite, shell reconciliation 6/6, and Settings 20/20;
the combined log and `focused.exit=0` are retained there.

The exact release compositor and a real Preview client then passed the
application-policy harness at
`artifacts/qa/2026-08-08-spaces-application-policy-73cbd3f/utm/` with
`runtime.exit=0`, `logs/status.txt` reporting `qa_exit=0`, and
`logs/result.txt` reporting `qa_complete=true`. Snapshots show stable ID 2
mapping, `All` reassigning the existing window to all eight Spaces, `Current`
clearing the policy back to active Space 1, unchanged revisions for Space 999
and a newline-containing application ID, persistence of target Space 3,
compositor restart restoration, and a restarted Preview mapping to Space 3.
The first compositor log records both rejection errors.

This closes the missing Settings command/readback and shell reconciliation slice
for application policies, but the UTM runtime harness drives the typed control
socket rather than a pixel-inspected Settings click. It still does not prove
physical DRM/output, live thumbnails, drag/gesture, accessibility, broad
third-party compatibility, packaging, performance budgets or long-running
soaks. The overall score remains **63/100**; the evidence-backed Settings
functional slice advances from 54 to **56/100**, Spaces UX from 38 to
**40/100**, and strict compositor completion remains **76/100**.

### Previous implementation wave — application-ID Spaces policy runtime

Implementation commit `5f1867a1666e2f524a4146e2382300f013e9ebbe` adds the
compositor-owned application-ID policy path for SLOPOS Spaces. A validated
policy can target one stable Space ID, every Space, or `Current` (which clears
the stored policy and restores active-Space placement). The compositor applies
the policy both when a matching window is mapped and when a stored policy is
changed, publishes the policy in the atomic Spaces snapshot, persists it, and
restores it after restart. Invalid application IDs and unknown Space IDs are
rejected without a revision change.

The exact Ubuntu UTM clone at this SHA passed `cargo fmt --all -- --check`,
locked workspace check, locked workspace tests, Clippy with `-D warnings`, and
the locked release workspace build. All five exit markers are `0` under
`artifacts/qa/2026-08-08-build-tests-5f1867a-r1/utm/`. Focused UTM suites also
passed compositor 164/164 plus the 23-test completion contract, bus 12/12 plus
the 6-test Spaces protocol suite, shell Spaces reconciliation 5/5, and Settings
17/17; the combined log and `focused.exit=0` are retained there.

The exact release compositor and a real Preview client then passed the reusable
headless policy harness at
`artifacts/qa/2026-08-08-spaces-application-policy-5f1867a-r2/utm/` with
`runtime.exit=0`, `logs/status.txt` reporting `qa_exit=0`, and
`logs/result.txt` reporting `qa_complete=true`. Machine-readable snapshots show
`Id { id: 2 }` placing Preview on Space 2, `All` reassigning that existing
window to all eight Spaces, `Current` clearing the policy and returning it to
active Space 1, unchanged revisions for invalid Space 999 and an invalid
newline-containing application ID, persistence of target Space 3, compositor
restart restoration, and a restarted Preview mapping to Space 3. The first
compositor log retains both rejection errors and the persisted model JSON is
copied under the same evidence directory.

This is real headless software-renderer and compositor-authority evidence at
that earlier SHA. It did not prove a Settings or shell control for configuring
application policies; the later `73cbd3f` wave above closes that bounded UI and
mirror gap. It still did not prove physical DRM/output, live-thumbnail,
drag/gesture, accessibility, third-party compatibility, packaging,
performance-budget or long-running-soak claims.

### Current implementation wave — Settings-controlled Spaces output assignment

Implementation commit `fb194e4e6abf78854cb7b74e7380f4606d498a14` adds the
missing Settings controls for compositor-owned Space output assignment. The
Spaces panel now exposes an output-ID field, `Assign Output` and `Clear Output`
actions, renders the assigned output in each Space row, and enables assignment
only under the compositor's `independent_per_display` policy. The actions send
typed `SpacesControlCommand::AssignOutput` requests; Settings continues to
reconcile only the compositor's atomic snapshot rather than editing a local
window/Space model.

Host Settings tests and Clippy passed after the change. The exact Ubuntu UTM
clone at this SHA passed `cargo fmt --all -- --check`, locked workspace check,
locked workspace tests, Clippy with `-D warnings`, and the locked release
workspace build; all five exit markers are retained under
`artifacts/qa/2026-08-08-build-tests-fb194e4/utm/`. Focused UTM tests passed
Settings 16/16 and the bus unit/integration/Spaces protocol suites 12+3+6.

The exact release compositor then passed a fresh headless runtime harness under
`artifacts/qa/2026-08-08-spaces-output-fb194e4/utm/` (`harness.exit=0`). It
rejected assignment in shared-span mode without a revision change, accepted
the independent-per-display policy, applied `DP-1`, rejected invalid `" DP-2"`
without changing the assigned output, cleared the assignment, persisted `DP-2`,
and restored the independent policy plus `DP-2` after compositor restart. The
compositor log records both rejection errors; JSON snapshots retain every
state transition and the exact SHA is recorded in `provenance.txt`.

This closes the Settings output-assignment UI and its software-rendered
compositor/persistence path. It does not prove that a physical output named
`DP-1` or `DP-2` exists, migrate a live window across real outputs, or complete
live thumbnails, drag/gesture/accessibility workflows, third-party
compatibility, physical DRM/input/multi-monitor evidence, packaging,
performance budgets or long-running reliability. The overall score remains
**63/100**; the evidence-backed Settings functional slice advances from 52 to
**54/100** and Spaces UX from 35 to **38/100**. Strict compositor completion
remains **76/100**.

### Current implementation wave — compositor-authoritative dynamic SLOPOS Spaces

Implementation commit `cbd583e430762b4e902d213309dd628345421750` connects the
dynamic `SpacesModel` to both compositor backends and the live shell. The
compositor now owns stable Space IDs, creation/removal, naming, reorder,
selection, per-Space wallpaper/appearance/classification, display policy,
output assignment, live window membership and atomic persistence. It publishes
an atomic `spaces-state.json` snapshot with a session epoch and monotonic
revision; the shell validates and reconciles that projection, renders a
dynamic overview grid, and sends stable-ID selection requests without
optimistically changing its mirror. A dedicated layer-shell Overlay surface
keeps the live overview above ordinary application surfaces.

Window membership is deliberately cleared when persisted metadata is loaded:
compositor window IDs are session-scoped and must not become stale windows after
a restart. Shell reconciliation now accepts a lower revision when the
compositor session epoch changes, with a regression test through
`ShellDesktop::update()`.

The exact Ubuntu UTM clone at this SHA passed `cargo fmt --all -- --check`,
locked workspace check, locked workspace tests, Clippy with `-D warnings`, and
the locked release workspace build. Exit markers and logs are retained under
`artifacts/qa/2026-08-08-spaces-cbd583e/`; `commit.txt` records the exact SHA.

The same exact release binaries passed a fresh headless UTM runtime harness:
create, rename, reorder and select a Space; map a real Preview client; move
that client to another Space through the compositor control socket; reject an
unknown-window move without a revision change; remove a Space with safe active
fallback; persist state; restart the compositor; restore the active Space and
clear stale session-window membership. The machine-readable status is
`artifacts/qa/2026-08-08-spaces-cbd583e/runtime/status.txt` with `qa_exit=0`.

This is compositor-authority and headless software-renderer evidence. It does
not prove physical DRM/KMS presentation or output migration, application-ID
assignment/all-Spaces policy, live thumbnails, drag between Spaces/displays,
touchpad gestures, keyboard-only or assistive-technology operation, Settings
integration, reduced-motion behaviour, third-party compatibility, packaging,
performance budgets or long-running soaks. The DRM attempt reached libseat but
could not open the VM's `/dev/dri/card0` because the root kernel was holding the
device; its failure is retained under
`artifacts/qa/2026-08-08-spaces-drm-capability-current/`. The overall score
therefore remains **63/100** and strict compositor completion remains
**76/100**.

### Current implementation wave — authoritative Spaces Settings and session IPC security

Implementation commit `5553161ed9ac378602cbef5c510d32c0cab799b8` adds a real
Settings→compositor Spaces panel. Settings reads and validates the atomic
`spaces-state.json` projection, reconciles external revisions/session epochs,
and sends typed stable-ID requests for Space selection, creation, rename,
reorder, removal, wallpaper/appearance metadata, fullscreen classification and
multi-display policy. It does not optimistically edit its mirror; request
success is reported as pending until a new compositor snapshot arrives, and a
missing or invalid session is shown as unavailable. Focused tests cover all
request shapes and snapshot reconciliation.

The same commit hardens the session control datagram endpoint: the socket is
explicitly mode `0600`, Linux enables `SO_PASSCRED`, and the listener rejects
missing or mismatched sender UIDs before decoding a request. The exact Ubuntu
UTM guest passed the socket-mode, peer-credential and valid same-UID runtime
checks. An unprivileged mismatched-UID injection was not available in the
`ubuntu` guest, so that boundary remains covered by deterministic policy tests.

The exact guest at this SHA passed all five locked workspace gates; exit markers
and logs are retained under
`artifacts/qa/2026-08-08-settings-spaces-5553161/utm/gates-r1/`. Focused UTM
Settings and bus tests passed 16/16 and 12+3+6 respectively under
`.../utm/focused-r1/`. The exact release Spaces runtime harness passed with
`qa_exit=0` under `.../utm/runtime-r1/`; the exact release IPC runtime check
recorded `socket_mode=600`, a valid same-UID datagram and `active_space=2` under
`.../utm/ipc-r1/`.

This closes the Settings Spaces integration and local session-socket permission
gaps, but does not complete shell-side create/rename/reorder/remove callers,
Assign Output UI, live thumbnails, drag/gesture/accessibility workflows,
third-party compatibility, physical DRM/input/multi-monitor evidence,
packaging, performance budgets or long-running reliability. Settings remains a
partial service surface rather than an authoritative implementation of every
system domain. The overall score remains **63/100**; the evidence-backed
Settings functional slice advances from 48 to **52/100**, Spaces UX from 25 to
**35/100**, and security/release readiness from 52 to **55/100**. The strict
compositor score remains **76/100**.

### Current implementation wave — compositor-authoritative workspace switching

Implementation commit `d1ea8239621c0c761d0471caea2a1f042e9677b6` adds a typed
`SwitchWorkspace { index: u8 }` request to the exact session control socket.
Both the nested and DRM compositor backends validate the indexed workspace
through shared compositor policy, redraw and rebind focus after a valid switch,
and reject invalid indices without changing state. Live shell workspace
switches send this request before committing their local mirror; failed sends
leave that mirror unchanged. Fallback/in-process shell tests remain local-only.

Focused tests cover request JSON/socket delivery, valid 0..7 and rejected 8
activation, and live-shell success/failure ordering. Host formatting, locked
workspace check/tests, Clippy with `-D warnings`, and the optimized workspace
build passed after the change.

The exact Ubuntu UTM clone at this SHA passed the same five locked workspace
gates. The first test attempt stopped at the linker because the guest disk was
full; after removing only old generated Cargo `target/` output, the rerun
completed with `test_exit=0`. Both attempts are retained under
`artifacts/qa/2026-08-08-build-tests-d1ea823/`.

The repository headless compositor protocol gate and logical-output topology
gate both report `status: "passed"` at this SHA. Evidence is retained under
`artifacts/qa/2026-08-08-compositor-runtime-d1ea823/` and
`artifacts/qa/2026-08-08-compositor-topology-d1ea823/`; their JSON explicitly
keeps DRM/KMS, physical input, rendering and hardware flags false.

A separate UTM runtime control smoke launched two real release Preview clients
against the SLOPOS-owned headless compositor. Both clients mapped and observed
WGPU Vulkan `llvmpipe`; the compositor then logged
`workspace active=2/8 windows=2 visible=0` for the valid request and rejected
index 8. The request was sent by a retained control-plane harness, not by a
pixel-inspected shell menu click, so this proves compositor authority and
failure handling but not the complete dynamic Spaces product or visual shell
interaction. Evidence is under
`artifacts/qa/2026-08-08-spaces-control-d1ea823/r2/`.

This closes one shell/compositor mismatch but does not integrate the dynamic
`SpacesModel`, replace the fixed eight-workspace state, prove overview/drag/
keyboard/accessibility workflows, or cover hardware and third-party gates. The
overall score therefore remains **63/100** and strict compositor completion
remains **76/100**.

### Current implementation wave — bounded multi-page retained glyph atlas

Implementation commit `d42d09eb002502d70dad26b039299534e0ebf2cc` extends the
retained glyph path from one bounded atlas page to four bounded 1024×1024 R8
GPU texture-array pages. Each page has a 2048-entry limit, glyph vertices carry
their page index, dirty uploads are page-local, and glyphs remain available
after the first page fills until all bounded pages are exhausted. Focused SDK
regression tests cover overflow and same-frame page preservation.

The exact Ubuntu UTM guest clone at this SHA passed `cargo fmt --all --
--check`, `cargo check --workspace --all-targets --locked`, `cargo test
--workspace --locked`, `cargo clippy --workspace --all-targets --all-features
--locked -- -D warnings`, and `cargo build --release --workspace --locked`.
Logs, exit markers and guest provenance are retained under
`artifacts/qa/2026-08-08-build-tests-d42d09e/`.

A fresh guest headless compositor plus release Preview smoke used a real PNG
and remained alive until the bounded 30-second timeout (`preview_exit=124`).
The WGPU log observed the Vulkan adapter `llvmpipe (LLVM 21.1.8, 128 bits)`.
Evidence is retained under
`artifacts/qa/2026-08-08-renderer-atlas-d42d09e/`.

This is source, automated-test and software-renderer runtime evidence. It does
not prove pixel readback or screenshot acceptance, physical DRM/KMS
presentation, hardware input, authoritative use by every first-party surface,
scale-aware performance budgets, image colour/mipmap/animation handling,
third-party compatibility, accessibility workflows, packaging/recovery or
long-running reliability. The overall score therefore remains **63/100** and
strict compositor completion remains **76/100**.

### Current implementation wave — retained GPU images and Preview interactions

Implementation commits `bf341ef`, `8f3eba2`, `5724453` and
`698832658d240a3c82497a71f1d73262b974b8bc` replace Preview's panel-mosaic image
path with retained RGBA GPU tile uploads. The image path now uses 2048-pixel
tiles, a bounded retained cache, visible-tile eviction protection, EXIF
orientation application, alpha preservation, inverse clipped-tile selection,
quarter-turn geometry/UV transforms and source-byte reuse. Preview exposes
Fit, Fill, actual size, pan/scroll, rotate-left and rotate-right through its
toolbar, menus and keyboard paths. Fit/Fill use the rotated display
dimensions, and rotation/zoom changes clamp the ScrollView state.

The exact Ubuntu UTM guest at
`698832658d240a3c82497a71f1d73262b974b8bc` passed all five locked workspace
gates: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets
--locked`, `cargo test --workspace --locked`, Clippy with `-D warnings`, and
`cargo build --release --workspace --locked`. Raw logs and provenance are under
`artifacts/qa/2026-08-08-build-tests-6988326/`.

The same guest passed the SLOPOS-owned headless Wayland protocol runtime
(schema 12) and logical output topology runtime. Registry/readiness, XDG
toplevel and popup lifecycle, pointer constraints, native clipboard and
primary selection, text/URI DnD, text-input-v3/input-method-v2 and abrupt
disconnect recovery are all reported `true` in
`artifacts/qa/2026-08-08-compositor-runtime-6988326/698832658d240a3c82497a71f1d73262b974b8bc.json`.
Logical output add, reorder, remove and surface-migration source-contract
checks are reported `true` in
`artifacts/qa/2026-08-08-compositor-topology-6988326/`.

A separate guest Preview smoke launched the release binary against that
SLOPOS headless compositor with a real PNG, remained alive until the bounded
30-second timeout (`preview_exit=124`), and logged the WGPU Vulkan adapter
`llvmpipe (LLVM 21.1.8, 128 bits)`. Its retained log is under
`artifacts/qa/2026-08-08-renderer-image-6988326/`.

This is source, build/test and software-renderer runtime evidence. It does not
prove pixel readback or screenshot acceptance, physical DRM/KMS presentation,
hardware input, mipmaps/area filtering, colour-profile handling, animation,
complete metadata/document workflows, performance budgets, third-party
compatibility or long-running reliability. The compositor reports
`hardware_verified`, `drm_verified` and `rendering_verified` as false for the
headless protocol gate. The overall score therefore remains **63/100** and
strict compositor completion remains **76/100**.

### Current implementation wave — retained glyph atlas

Implementation commit `1c603bec2ff55614cbc4d09912e5be882d9801ef` adds a bounded
1024×1024 R8 glyph atlas, textured glyph vertices, retained shaped-glyph
geometry, atlas reuse and a deterministic fallback path while preserving draw
order. The focused Ubuntu UTM SDK gate at that commit passed formatting,
workspace check, workspace tests and Clippy with `-D warnings`; evidence is
retained under `artifacts/qa/2026-08-08-renderer-gate-1c603be-r1/` and recorded
by QA commit `32ed35eeaddf141d7e4f95f9a6d736c717c9bb0d`.

This is source/build/test evidence for the retained text path, not a complete
visual or performance acceptance result. It does not prove that every
first-party surface uses the path, that image rendering is GPU-backed, or that
high-DPI, mixed-scale, font-fallback, emoji, grapheme and bidi behaviour meet
the production contract. Preview still uses its existing low-resolution
panel-mosaic image path. The overall score remains **63/100**.

### Current implementation wave — unavailable application actions removed

Implementation commit `d47ad0420b9c105587893d328158eca29ac503ef` removes
Preview's unavailable Open action and App Store's stub Remove, Update and
Confirm controls. The exact focused Ubuntu UTM gate recorded by QA commit
`5577f3659691a9789a0c6e374a4bdd4dabb3d7ae` passed 14 Preview tests, 23 App
Store tests, formatting and Clippy; evidence is under
`artifacts/qa/2026-08-08-ui-gate-d47ad04/`.

This removes misleading controls but does not complete the advertised Preview
or software-manager workflows. The overall score remains **63/100**.

### Current implementation wave — exact-current-head Ubuntu UTM regression

At audited implementation commit `5577f3659691a9789a0c6e374a4bdd4dabb3d7ae`,
the Ubuntu UTM guest ran the mandatory locked workspace gates: `cargo fmt`,
locked workspace check, locked workspace tests, Clippy with `-D warnings`,
and the locked release workspace build. Each command exited zero. The
timestamped provenance and command logs are retained under
`artifacts/qa/2026-08-08-build-tests-5577f36-r5/`.

The same exact head passed the SLOPOS-owned headless Wayland runtime gate;
schema-12 JSON reports `status: "passed"` for registry/readiness, XDG
toplevel and popup lifecycle, pointer constraints, native clipboard and
primary selection, text/URI DnD, text-input-v3 and input-method-v2, with
abrupt-disconnect recovery. Evidence is under
`artifacts/qa/2026-08-08-compositor-runtime-5577f36/`.

The current-head logical-output topology gate also reports `status: "passed"`:
add, reorder and removal of logical outputs succeeded, including readiness
dimensions and output identity checks. Evidence is under
`artifacts/qa/2026-08-08-compositor-topology-5577f36/`.

These are Ubuntu UTM build and headless/logical runtime results only. The
artifacts explicitly keep `hardware_verified`, DRM/KMS, rendering, physical
input and physical multi-monitor flags false. They do not prove display-manager
login, real GPU presentation, GTK/Qt/Electron or XWayland compatibility,
third-party IME/DnD, HDR/VRR, accessibility, packaging/recovery or long soaks.
The overall score remains **63/100**, and strict compositor completion remains
**76/100**.

### Current implementation wave — native text-input-v3 and input-method-v2

Implementation commits `b037a307cdc186621a48809ddcd2977530afd2c8`,
`e31ecd7b5ae5d2cf2b34f7d5a553c317391ed4d7`, and the gate-fix commit
`77f0598e5382162cbe0deb1a4b21a7ec03071fae` are **BUILD VERIFIED** and **TEST
VERIFIED**. The DRM backend now creates the same environment-gated text-input
and input-method manager states as the nested backend, retains input-method
popup state, and applies the same compositor focus policy.

The exact-current-head Ubuntu UTM gates passed at
`77f0598e5382162cbe0deb1a4b21a7ec03071fae`. The locked workspace regression
(`fmt`, `check`, `test`, `clippy -D warnings`, and release build) and logical
output topology gate all exited zero. Evidence is retained under
`artifacts/qa/2026-08-08-build-tests-77f0598/`,
`artifacts/qa/2026-08-08-compositor-runtime-77f0598/`, and
`artifacts/qa/2026-08-08-compositor-topology-77f0598/`. The runtime JSON uses
schema 12 and separate app/IME logs. Two independent native Wayland clients
proved:

- both text-input-v3 and input-method-v2 globals are present;
- the focused app receives text-input enter and done events;
- the IME receives activation, surrounding text and content type;
- the IME sends a serial-checked commit, preedit and surrounding-text delete;
- the app receives and validates the committed and preedit strings; and
- disabling the text input causes one IME deactivation.

The lint correction in `77f0598` changes no protocol behaviour; it closes the
guest's `clippy -D warnings` release gate. This is first-party headless
protocol/runtime evidence only. It does not prove
GTK/Qt/Electron IME compatibility, candidate-popup rendering, physical input,
DRM/KMS, hardware, XWayland or complete text-editor grapheme/bidi integration.
The overall score remains **63/100** and strict compositor completion remains
**76/100**.

---

## 1. Evidence language

| Label | Meaning |
|---|---|
| **PLANNED** | Accepted requirement only |
| **SOURCE PRESENT** | Relevant code exists, but the user-visible/runtime behaviour is not proved |
| **BUILD VERIFIED** | The named target compiled at the recorded commit |
| **TEST VERIFIED** | Named automated tests passed at the recorded commit |
| **RUNTIME OBSERVED** | A real process or interaction produced retained evidence |
| **HARDWARE VERIFIED** | Behaviour ran on applicable graphics, display or input hardware |

A type, helper, menu entry, test fixture, generated table, screenshot mock or
successful build does not by itself prove a production feature.

### Audit confidence

- source architecture and exact CI: high confidence, approximately ±3 points;
- functional runtime ratings: moderate confidence, approximately ±5 points;
- visual-polish ratings: lower confidence, approximately ±7 points because the
  exact current product head was not freshly reviewed through a complete human
  screenshot and interaction matrix during this documentation pass.

---

## 2. Exact verified baseline

GitHub Actions run `30779578542` completed successfully against implementation
commit `db6cc01` and included:

- workspace build with all targets;
- workspace tests;
- workspace Clippy;
- Linux release build;
- lockfile consistency;
- rustfmt;
- exact-commit compositor source/build contract;
- SLOPOS-owned headless Wayland runtime protocol gate;
- compositor evidence upload.

The headless runtime gate verifies:

- a private compositor-owned socket;
- authenticated readiness;
- registry access;
- abrupt client-disconnect recovery;
- XDG toplevel configure;
- maximize, fullscreen and restore transitions;
- XDG popup configure;
- popup reposition acknowledgement.

It does **not** prove physical DRM/KMS rendering, real input devices,
multi-monitor hardware, popup pointer grabs, broad application compatibility,
XWayland, HDR, VRR or long-running stability.

Retained earlier VM/UTM evidence demonstrates a real SLOPOS DRM session,
compositor-owned shell layers, first-party windows, visible cursor, focus/global
menu changes, Fill, minimize and restore. That evidence remains valid for the
recorded commits but is not silently promoted to current-head hardware proof.

### Current implementation wave — output-aware presentation

Implementation commit `c6ce17e161ea9749cf7dd01dfa1c0f2a43f2f9ea` is **BUILD VERIFIED**, **TEST
VERIFIED** and covered by the existing **RUNTIME OBSERVED** headless compositor
gate. The new multi-output geometry itself remains unverified on physical
multi-monitor hardware.

This wave:

- normalises negative/offset nested output layouts while preserving relative
  monitor placement;
- computes true output-union bounds without assuming an origin of `(0, 0)`;
- assigns windows deterministically by greatest output overlap and nearest-output
  fallback;
- constrains XDG popups to the output that owns their root surface;
- applies Smart Zoom, Fill and fullscreen to one selected output instead of the
  complete multi-monitor canvas;
- stores the real connector/synthetic output name in restore state;
- clamps normal windows against the selected output's exclusive work area;
- adds pure tests for negative layouts, gaps, off-screen windows, overlap ties and
  integer-boundary safety;
- regenerates the workspace lockfile and passes compositor check, test, source
  contract and headless runtime gates before commit.

The overall product score remains **63/100**. The compositor score advances from
66 to **67/100**; physical hotplug, mixed-scale/refresh, per-output layer-shell
targeting, direct scanout and hardware evidence remain release blockers.

### Current implementation wave — per-output layer-shell ownership

Implementation commit `c9b74951ee2a167967d807f64677a5064c8fc118` is **BUILD VERIFIED**, **TEST
VERIFIED** and covered by the existing **RUNTIME OBSERVED** headless compositor
gate. Physical multi-monitor placement and hotplug remain unverified.

This wave:

- resolves a layer-shell client's requested `wl_output` back to the exact Smithay
  output and stores that owner on the mapped layer;
- computes menu-bar, Dock, notification and other layer geometry relative to the
  owning output rather than the full compositor canvas;
- scopes exclusive zones and normal-window work-area clamping to the owning
  output only;
- emits compositor-managed `wl_surface.enter` and `wl_surface.leave` membership
  as windows move or resize across outputs;
- constrains layer surfaces to one output membership and clears it on destroy;
- sends frame callbacks using each window or layer's selected output instead of
  routing every surface through the first output;
- adds pure multi-output membership tests and a permanent source/build contract;
- regenerates the workspace lockfile and passes workspace build/test/Clippy plus
  exact compositor source, release and headless runtime gates before commit.

The overall product score remains **63/100**. The compositor score advances from
67 to **68/100**. Runtime topology mutation, connector removal, mixed-scale
rendering, physical output evidence and DRM/KMS hotplug remain open.

### Current implementation wave — runtime logical-output topology

Implementation commit `8874741833794bf398516d5add5af4daf836fb15` is **BUILD VERIFIED**, **TEST
VERIFIED** and **RUNTIME OBSERVED** by dedicated GitHub Actions run
`31153723640`. This evidence is headless logical-output hotplug, not physical
DRM/KMS connector proof.

This wave:

- adds a typed session-control request for atomic output-layout replacement;
- strictly validates complete layouts, unique connector names, dimensions,
  origins, output count and the currently supported uniform scale;
- adds, reorders, resizes and disables `wl_output` globals at runtime;
- preserves existing globals for retained connector identities;
- disables removed globals while keeping them safe for already-bound clients;
- migrates layer surfaces by connector identity and recomputes per-output
  exclusive work areas;
- proportionally remaps normal and restore geometry to surviving/fallback outputs;
- reapplies fullscreen, Fill, Smart Zoom and tiling geometry after topology change;
- refreshes `wl_surface.enter`/`leave`, frame routing, pointer bounds and session
  readiness dimensions;
- rejects nested topology changes that would desynchronise the fixed host X11
  canvas instead of corrupting rendering;
- permanently runs a headless add/reorder/remove registry and readiness gate in
  compositor CI.

The overall product score remains **63/100**. The compositor score advances from
68 to **70/100**. Physical DRM/KMS connector hotplug, mixed-scale rendering,
nested host resize, current-head multi-monitor hardware evidence and long soak
cycles remain open.

### Current implementation wave — relative pointer protocol

Implementation commit `28f06fc0ee1c70552de3d475496bbbfdd7b2827e` is **BUILD VERIFIED** and **TEST VERIFIED**.
Protocol availability is **RUNTIME OBSERVED** by dedicated GitHub Actions run
`31156279030` through the permanent SLOPOS-owned headless registry gate. This does not
claim physical mouse, touchpad or raw-input hardware verification.

This wave:

- advertises `zwp_relative_pointer_manager_v1` from both the nested and DRM
  compositor states through Smithay's relative-pointer implementation;
- forwards relative motion on the DRM/libinput path with both accelerated and
  unaccelerated device deltas;
- keeps ordinary `wl_pointer` absolute focus/motion delivery in parallel;
- derives relative deltas from consecutive absolute samples on the nested X11
  backend, explicitly using the same value for unaccelerated motion because that
  backend does not expose raw deltas;
- keeps the protocol manager alive for the compositor lifetime; and
- extends the permanent headless runtime gate so CI fails if the relative-pointer
  global disappears.

The overall product score remains **63/100**. The compositor score advances from
70 to **71/100**, with Input correctness advancing from 7 to **8/10**. Pointer
constraints, physical multi-device/input evidence, touch, gesture delivery and
device hotplug remain open.

### Current implementation wave — pointer constraints

Implementation commit `23e09fe708a4ddb3ac21309f0a408faac2d1ead2` is **BUILD VERIFIED** and **TEST VERIFIED**.
Protocol advertisement plus lock/confine request-destroy lifecycle are
**RUNTIME OBSERVED** by dedicated GitHub Actions run `31159001350` through the
permanent SLOPOS-owned headless runtime gate. Physical pointer enforcement is
not claimed because that gate has no input hardware.

This wave:

- advertises `zwp_pointer_constraints_v1` from nested and DRM compositor states;
- activates client lock/confine constraints only while the requested surface has
  pointer focus and the pointer is inside the optional committed region;
- keeps `zwp_relative_pointer_v1` delivery active while a locked pointer remains
  stationary;
- enforces lock and confinement for nested X11 absolute samples and DRM/libinput
  relative or absolute motion;
- preserves accelerated and raw libinput deltas for relative-pointer clients;
- treats the compositor's session lock as higher priority than application
  pointer constraints;
- tests backend-independent free, locked and per-axis confined motion policy; and
- extends the permanent headless gate to exercise persistent lock and confinement
  object creation, commit and destruction on the exact compositor build.

The protocol-defined cursor-position hint remains intentionally non-warping: the
specification permits a compositor to ignore this hint, and SLOPOS does not
fabricate a host-pointer warp on the nested backend.

The overall product score remains **63/100**. The strict compositor score
advances from 71 to **72/100**, with Input correctness advancing from 8 to
**9/10**. Physical multi-device input, touch, gestures and hotplug remain open.

### Current implementation wave — exact-current-head VM gate provenance

Implementation commit `a688ba527495da2fd2172ab8ccc1effd9d631eed` is **BUILD
VERIFIED** and **TEST VERIFIED** in the Ubuntu UTM guest. The complete guest
regression run passed formatting, workspace check, workspace tests, Clippy with
`-D warnings` and the locked release build. The retained logs and binary SHA-256
provenance are under
`artifacts/qa/2026-08-08-compositor-runtime-a688ba5/build-tests/`.

This wave fixes a QA-gate provenance defect rather than a desktop capability:
`verify-compositor-headless-runtime.sh` now builds the named
`slopos-compositor` binary explicitly alongside its examples before executing
`target/debug/slopos-compositor`. A source-contract test fails if that target is
omitted again.

Fresh UTM runtime evidence at the same commit is retained under
`artifacts/qa/2026-08-08-compositor-runtime-a688ba5/headless/` and
`artifacts/qa/2026-08-08-compositor-runtime-a688ba5/topology/`. The headless
gate passed private-socket readiness, registry access, persistent pointer lock
and confinement request lifecycles, 64 abrupt disconnect cycles, XDG toplevel
configure/maximize/fullscreen/restore, and popup configure/reposition. The
logical-output gate passed add, reorder and remove of `LEFT`/`RIGHT` outputs.

The artifacts explicitly leave `hardware_verified`, DRM/KMS, rendering and
physical multi-monitor verification false. These gates therefore do not raise
the product score or claim XWayland, physical input, rendering, application
compatibility, HDR/VRR or long-soak completion. The overall product score
remains **63/100** and the strict compositor score remains **72/100**.

### Current implementation wave — VM-portable daily-driver and current-head QA

Implementation commit `361d32f15f439c84a9ce8b49decf362173f67309` is **BUILD
VERIFIED** and **TEST VERIFIED** in the Ubuntu UTM guest. The session wrapper
now preserves the validated `SLOPOS_OUTPUTS_LAYOUT` environment contract and
logs the selected compositor backend and layout. The daily-driver checklist no
longer requires `rg`, which is absent from the guest, and its source contract
is covered by two shell integration tests.

The guest checklist passed its packaging/session checks and three unit-test
groups (158, 162 and 327 tests) at this SHA. The complete mandatory gate set
also passed: `cargo fmt --all -- --check`, locked workspace check, locked
workspace tests, Clippy with `-D warnings`, and the locked release workspace
build. Exact guest logs and environment provenance are retained under
`artifacts/qa/2026-08-08-daily-driver-361d32f/`.

Fresh current-head headless protocol and logical-output topology gates passed
at the same SHA. Their JSON/log evidence is under
`artifacts/qa/2026-08-08-compositor-runtime-361d32f/headless/` and
`artifacts/qa/2026-08-08-compositor-runtime-361d32f/topology/`. These remain
headless/logical evidence only: they do not prove live display-manager login,
DRM/KMS rendering, physical input or multi-monitor hardware, XWayland,
third-party application compatibility, HDR/VRR or long-running stability.

This wave changes no product capability score. Overall SLOPOS-I remains
**63/100** and strict compositor completion remains **72/100**.

### Current implementation wave — native cross-client clipboard runtime

Implementation commit `1941ed0d8411b4d3b4fed07411df3eb25f44c1c4` is **BUILD
VERIFIED** and **TEST VERIFIED** in the Ubuntu UTM guest. It adds a real
two-process Wayland clipboard client: the source owns a focused XDG toplevel,
offers `text/plain;charset=utf-8` and `text/plain`, and services asynchronous
`wl_data_source.send` requests; the sink owns a separate focused toplevel,
receives the offer, reads the exact UTF-8 payload, and verifies EOF for an
unsupported MIME request.

The exact-head headless runtime gate passed the existing registry, pointer
constraint, 64-cycle abrupt disconnect, XDG toplevel/popup lifecycle checks and
the new clipboard markers: `SLOPOS_CLIPBOARD_OFFER_VERIFIED`,
`SLOPOS_CLIPBOARD_TRANSFER_VERIFIED` (47 bytes), and
`SLOPOS_CLIPBOARD_MISSING_MIME_EOF_VERIFIED`. The logical-output topology gate
also passed add/reorder/remove at this SHA. The complete Ubuntu mandatory gate
set and daily-driver checklist passed; retained evidence is under
`artifacts/qa/2026-08-08-compositor-runtime-1941ed0/` and
`artifacts/qa/2026-08-08-compositor-gates-1941ed0/`.

This is native Wayland clipboard runtime evidence only. It does not prove
large-payload limits, cancellation/source-death recovery, primary selection,
drag-and-drop, IME, GTK/Qt/Electron compatibility, XWayland bridging or
hardware input. Clipboard/DnD/IME therefore advances conservatively from 4 to
**5/8**, strict compositor completion from 72 to **73/100**, and overall
SLOPOS-I remains **63/100**.

### Current implementation wave — source-death clipboard proof and exact current-head QA

Implementation commits `0a3772a0e3d108ef54fd27c92a196ad1ded396d8` and
`c6235bed5dc5713f7d6f3e690e48455c27b00954` are **BUILD VERIFIED** and **TEST
VERIFIED** in the Ubuntu UTM guest. The native clipboard QA client now
distinguishes `wl_data_device.selection(Some(offer))` from the protocol's
explicit `selection(NULL)` clear event and requires the latter for the
source-disconnect failure path. The clear marker was also made parseable by
the shell gate; this fixes an evidence/client-assertion defect rather than
adding a new production application feature.

At `c6235be`, the exact Ubuntu UTM headless runtime JSON reports `status:
passed`, `clipboard_large_transfer_verified: true`,
`clipboard_missing_mime_eof_verified: true`, and
`clipboard_source_death_cleared: true`. The same commit passed the logical
output add/reorder/remove gate, the daily-driver packaging/unit checklist
(158, 162 and 327 tests), and the complete locked workspace fmt/check/test,
Clippy `-D warnings` and release build set. Retained evidence is under
`artifacts/qa/2026-08-08-compositor-runtime-c6235be/`,
`artifacts/qa/2026-08-08-compositor-topology-c6235be-rerun/`,
`artifacts/qa/2026-08-08-daily-driver-c6235be/`, and
`artifacts/qa/2026-08-08-build-tests-c6235be/`.

These remain headless/logical and packaging/unit evidence only. They do not
prove primary selection, cancellation, drag-and-drop, IME, GTK/Qt/Electron or
XWayland compatibility, physical DRM/KMS/input/multi-monitor behaviour,
HDR/VRR, live display-manager login, or long-running stability. This QA wave
therefore changes no product capability score: overall SLOPOS-I remains
**63/100**, and strict compositor completion remains **73/100**.

### Current implementation wave — native primary-selection runtime

Implementation commit `0f4dac76be7cf0a439e586cbd4ad5cfe0035ac8f` is **BUILD
VERIFIED** and **TEST VERIFIED** in the Ubuntu UTM guest. The native Wayland QA
client now exercises `zwp_primary_selection_device_manager_v1` with separate
source and sink processes. The source advertises
`text/plain;charset=utf-8` and `text/plain`, transfers the exact 46-byte UTF-8
payload, and the sink verifies EOF for an unsupported MIME request.

The exact-head headless runtime JSON reports `status: passed` with
`primary_selection_offer_verified`, `primary_selection_transfer_verified` and
`primary_selection_missing_mime_eof_verified` all true. The same run retains
the normal clipboard offer/transfer, 1 MiB transfer, unsupported-MIME EOF and
source-death clear markers; the logical-output topology and locked Ubuntu
workspace regression gates also passed. Evidence is retained under
`artifacts/qa/2026-08-08-compositor-runtime-0f4dac7/`,
`artifacts/qa/2026-08-08-compositor-topology-0f4dac7/`,
`artifacts/qa/2026-08-08-daily-driver-0f4dac7/` and
`artifacts/qa/2026-08-08-build-tests-0f4dac7/`.

This is native headless Wayland selection evidence only. It does not prove
clipboard cancellation, target death, drag-and-drop, IME preedit/commit,
GTK/Qt/Electron or XWayland compatibility, physical DRM/KMS/input or
multi-monitor behaviour, HDR/VRR, live display-manager login, or long-running
stability. Clipboard/DnD/IME therefore advances conservatively from 5 to
**6/8**, strict compositor completion from 73 to **74/100**, and overall
SLOPOS-I remains **63/100**.

### Current implementation wave — clipboard cancellation and target death

Implementation commits `da853a258f22f860eebe644ee17834abe2bc7ade`,
`04222003f68f8ea06251947ef4000c1841dea8fa` and
`0579025157954f26d95e5e5c9764dc30f9089b6b` are **BUILD VERIFIED** and **TEST
VERIFIED** in the Ubuntu UTM guest. The native QA client now covers the two
failure paths that were previously unproved: replacing a live clipboard source
must deliver exactly one `wl_data_source.cancelled` marker, and a target that
closes its receive pipe after a partial 1 MiB transfer must not take down the
source or compositor. The source-owned send path records
`SLOPOS_SELECTION_TARGET_DISCONNECTED` for the observed `Broken pipe`; the
nested and DRM compositor-owned send paths retain the same asynchronous marker
for server-set selections.

The exact-current-head (`0579025`) schema-10 runtime JSON reports `status:
passed` with `clipboard_source_cancelled_verified`,
`clipboard_target_death_recovered_verified` and
`selection_target_disconnected_verified` all true, alongside the existing
clipboard, primary-selection, pointer-constraint, XDG/popup and 64-cycle
disconnect markers. Exact-head topology, daily-driver packaging/unit, locked
workspace fmt/check/test/Clippy/release and the 14-test compositor contract
evidence are retained under
`artifacts/qa/2026-08-08-compositor-runtime-0579025/`,
`artifacts/qa/2026-08-08-compositor-topology-0579025/`,
`artifacts/qa/2026-08-08-daily-driver-0579025/` and
`artifacts/qa/2026-08-08-build-tests-0579025/`.

This remains headless native Wayland evidence, not GTK/Qt/Electron, XWayland,
drag-and-drop, IME preedit/commit, physical DRM/KMS/input/multi-monitor,
display-manager, HDR/VRR or long-soak evidence. Clipboard/DnD/IME therefore
advances conservatively from 6 to **7/8**, strict compositor completion from 74
to **75/100**, and overall SLOPOS-I remains **63/100**.

### Current implementation wave — native cross-client DnD runtime

Implementation commits `8ecad10afd05ff5238d8d51d6bcc796297757da`,
`69d2df1b330addb8cfa8a5aaa9c64ebd32ab4bcf`,
`cb7e432101d45b662c91d91fd9cc2208ed365d38` and
`d5c0e0bd4117f0c0eba9259636bd18b6439c6620` add and harden an explicit,
headless-only input test path and a separate two-process native Wayland DnD
client. The source receives a genuine Smithay `wl_pointer.button` serial,
starts a drag with an icon, and remains alive until both requested MIME sends
complete. The target accepts the offer, verifies `text/plain;charset=utf-8`
and `text/uri-list`, reads the exact 45-byte and 28-byte payloads, accepts the
normal post-drop leave, and verifies a validated drop. The invalid-serial smoke
uses an unmapped origin surface so it cannot perturb the positive test's window
cascade.

The exact-current-head Ubuntu UTM runtime gate at `4ed9a6b59386501127b83be92abed7118368e972`
passed with schema 11. Its JSON records `status: passed`, native DnD offer,
text, URI, source-start, drag-icon, client-drop and validated-drop markers, the
invalid-serial rejection marker, the existing clipboard/primary-selection
failure paths, pointer constraints, XDG/popup lifecycle and abrupt-disconnect
recovery. The output-topology gate and daily-driver checklist also passed at
the same SHA. Fresh exact-head build/test/Clippy/release logs are retained
under `artifacts/qa/2026-08-08-build-tests-4ed9a6b/`; runtime evidence is under
`artifacts/qa/2026-08-08-compositor-runtime-4ed9a6b/`,
`artifacts/qa/2026-08-08-compositor-topology-4ed9a6b/` and
`artifacts/qa/2026-08-08-daily-driver-4ed9a6b/`.

This remains env-gated synthetic headless protocol evidence. The JSON keeps
`hardware_verified: false` and `input_verified: false`; it does not prove
physical input, GTK/Qt/Electron, XWayland, IME preedit/commit, DnD cancellation
or target-death recovery, images, display-manager login, DRM/KMS, rendering,
HDR/VRR or long-running stability. The native cross-client DnD point therefore
advances Clipboard/DnD/IME from 7 to **8/8** and strict compositor completion
from 75 to **76/100**; overall SLOPOS-I remains **63/100**.

---

## 3. Production scoring model

| Score | Meaning |
|---:|---|
| 0–19 | Requirement, experiment or placeholder |
| 20–39 | Early implementation or disconnected subsystem |
| 40–59 | Credible prototype |
| 60–74 | Functional alpha |
| 75–84 | Strong beta |
| 85–91 | Release candidate |
| 92–99 | Production-ready with bounded known gaps |
| 100 | Frozen acceptance contract completely satisfied |

The public target is 100/100. The current score remains evidence-based rather
than aspirational.

---

## 4. Executive scorecard

| Perspective | Score | Current truth |
|---|---:|---|
| Engineering foundation | **76** | Strong Rust workspace, session/compositor ownership, useful tests and exact Linux CI |
| UI and UX | **59** | Distinctive and coherent; the GPU image path is real, but typography, colour/scale polish, animation and integration remain alpha-grade |
| Product functionality | **61** | Real shell, compositor, applications and Vision paths; Preview image interaction is broader, but many daily-driver workflows are incomplete |
| Linux daily-driver readiness | **51** | Suitable for controlled development and QA, not yet for a non-technical user’s only desktop |
| Compositor strict completion | **77** | Native clipboard, primary-selection, text/URI DnD, first-party text-input/IME and bounded XWayland restart recovery are runtime-observed; hardware, input, displays, broad XWayland and compatibility gates remain |
| Security and release readiness | **52** | Good session/filesystem hardening, incomplete sandbox, signing, packaging, upgrades and recovery |
| Accessibility readiness | **46** | Live recursive AT-SPI export is runtime-observed; Orca and full desktop workflows remain unproven |
| POSIX/FreeBSD portability | **22** | Direction is defined; implementation and native evidence remain early |
| **Overall SLOPOS-I** | **63** | Strong custom desktop alpha with substantial productionisation remaining |

The overall score is a maturity judgment, not a percentage of code written.

---

## 5. Why SLOPOS-I is not production-level today

A production desktop must remain dependable across hardware, applications,
input methods, displays, upgrades, crashes and days of continuous use. Current
SLOPOS-I still has release-blocking gaps in each of those areas.

The most important blockers are:

1. incomplete physical compositor/input/multi-monitor coverage;
2. incomplete retained rendering acceptance: complete text authority, image colour/mipmap/animation paths and measured scale/performance proof;
3. incomplete third-party DnD compatibility and application-level IME integration;
4. partial XWayland and third-party application compatibility;
5. SLOPOS Spaces now has compositor and partial Settings mutation paths,
   including user-configurable application-ID policies, but the complete Spaces
   experience is unfinished;
6. Settings is authoritative for the new Spaces slice, but not for all system
   services;
7. first-party applications remain incomplete for normal daily use;
8. accessibility is live for the tested first-party widget paths but not yet Orca-complete;
9. sandbox, permissions, publisher trust and package signing are incomplete;
10. installation, upgrade, rollback, recovery and long soaks are not proven.

Passing CI proves engineering health. It does not erase these product gaps.

---

## 6. UI and UX audit

| UI area | Score | Current truth |
|---|---:|---|
| Visual identity | 77 | Recognisable classic Macintosh/System 7/Platinum lineage without generic GTK styling |
| Design-system consistency | 71 | Semantic palette and compact metrics exist; use is not universal |
| Window chrome | 73 | Native controls and typed compositor actions are real; policy UI is incomplete |
| Global menu | 66 | Focus-driven ownership exists; command completeness varies |
| Typography quality | 60 | Shaping APIs exist, but font roles and profiles are not authoritative everywhere |
| Text-rendering performance | 45 | Retained glyph-atlas work is present, but authoritative use, scale-aware caches and measured budgets remain incomplete |
| Image rendering | 56 | Retained RGBA GPU tiles, bounded cache, orientation/alpha, zoom, pan, Fit, Fill and quarter-turn rotation are implemented; mipmaps, colour profiles, animation, pixel acceptance and budgets remain open |
| Layout and resizing | 62 | Core layout works; many applications retain fixed sizes and hand-authored geometry |
| Keyboard navigation | 72 | Shared focus management and keyboard activation are substantive |
| Pointer dispatch and capture | 72 | Shared dispatcher and capture are real; compositor interaction evidence remains incomplete |
| Editing interaction | 64 | UTF-8-safe selections and caret insertion exist; graphemes, bidi and app-level IME remain open |
| Accessibility UX | 46 | Live recursive AT-SPI tree and first-party export are observed; assistive workflows remain incomplete |
| Animation and motion | 27 | No production transition system for Spaces, windows, Dock and notifications |
| Scaling polish | 53 | Logical scaling exists; mixed-scale visual matrix is incomplete |
| Theme/font customisation | 57 | Themes exist; font profiles and live Settings integration are incomplete |
| Visual polish | 52 | Distinctive but visibly an engineering alpha |
| **UI/UX overall** | **59** | Strong identity, incomplete production rendering and productisation |

### UI release blockers

- make the retained glyph atlas authoritative across every first-party surface,
  with grapheme/bidi/IME geometry and measured scale-aware budgets;
- finish Preview image filtering, colour-profile handling, animation/document
  support where advertised, and a current-head pixel/screenshot matrix;
- make `slopos-fonts` authoritative across shell, SDK and applications;
- implement shaped grapheme/bidi caret geometry and IME;
- add restrained, reduced-motion-aware window and Spaces transitions;
- run a current-head screenshot matrix at 1.0, 1.25, 1.5 and 2.0 scale;
- remove clipping, rigid geometry, contrast and focus-ring inconsistencies.

---

## 7. Compositor and session audit

### Strict 100-point compositor contract

| Domain | Current | Target | Main remaining work |
|---|---:|---:|---|
| Session sovereignty and lifecycle | 9 | 10 | Display-manager, suspend/resume, lid and longer failure coverage |
| Core Wayland lifecycle | 12 | 14 | Broader popup, subsurface, transient and modal compatibility |
| Input correctness | 9 | 10 | Physical multi-device, touch, gestures and hotplug |
| Clipboard, DnD and IME | 8 | 8 | GTK/Qt/Electron/XWayland compatibility, DnD failure paths and application-level IME integration |
| Rendering and frame scheduling | 9 | 12 | Direct scanout, occlusion, GPU recovery and physical pacing evidence |
| Displays and scaling | 9 | 12 | Hotplug, mixed scale/refresh, rotation, migration and topology recovery |
| External Wayland compatibility | 6 | 12 | GTK, Qt, Electron, browsers, office, media, games and popup-heavy apps |
| XWayland | 5 | 8 | Rootless scene, override-redirect, clipboard/DnD, DPI and representative application matrix; bounded restart recovery is now runtime-observed |
| HDR, VRR and colour | 3 | 6 | Physical capable hardware, metadata/presentation proof and full colour path |
| Security, stability and release QA | 7 | 8 | Soaks, resource plateaus, fuzzing and hostile-client breadth |
| **Total** | **77** | **100** | Native first-party text-input/IME and bounded XWayland restart recovery are runtime-observed; external compatibility, hardware and reliability remain |

### Strong current compositor work

- private session runtime and socket ownership;
- verified readiness token and process identity;
- Wayland display polling and client dispatch;
- reversible presentation states;
- output-change geometry clamping;
- popup configuration and reposition testing;
- abrupt-client-disconnect recovery;
- frame-pacing and work-area tests;
- capability-driven HDR/VRR policy rather than fabricated support;
- bounded XWayland restart recovery with explicit session budget and terminal
  crash-loop state;
- exact-commit CI contract.

### Remaining proof before 100

- physical DRM/input/multi-monitor matrix on current code;
- touch, touchpad gestures and multiple-device hotplug;
- third-party DnD failure paths and application-level IME integration;
- broad Wayland client matrix;
- first-class XWayland scene/application compatibility beyond the recovery
  process;
- HDR/VRR on capable displays;
- direct scanout and GPU-reset recovery;
- 24-hour idle and mixed-workload soaks;
- memory/file-descriptor plateaus and fuzzing.

---

## 8. Shell and desktop product audit

| Component | Score | Current truth |
|---|---:|---|
| Session supervisor | 86 | One of the strongest components; broader lifecycle and platform abstraction remain |
| Desktop shell | 66 | Real layer surfaces, menu, Dock and overlays; integration and polish remain incomplete |
| Global menu routing | 68 | Focus-driven ownership is credible; app command coverage varies |
| Dock | 59 | Launcher/minimize foundations exist; indicators, ordering, DnD and multi-monitor policy need work |
| Notifications | 48 | Infrastructure exists; actions, grouping, history and quiet modes are incomplete |
| Lock/session UX | 55 | UI and session actions exist; production authentication and lifecycle proof remain |
| Search/launcher | 45 | Early local functionality; indexing, ranking and actions remain incomplete |
| Portals | 47 | Source exists; compatibility and permission behaviour are not broadly proved |
| Clipboard | 65 | Real selection paths; large/cancelled/format-diverse transfers need QA |
| Cross-app drag-and-drop | 51 | Native first-party text/URI DnD is runtime-observed; third-party and shell workflows remain |
| SLOPOS Spaces model | 71 | Dynamic model, persistence and output policy are substantive |
| SLOPOS Spaces UX | 40 | Live overview, snapshot-backed Settings mutations, output assignment and application-policy controls/runtime exist; gestures, drag-between-Spaces, thumbnails and accessibility remain |
| Multi-monitor desktop UX | 43 | Policy/types exist; complete live topology behaviour is not established |
| **Shell/desktop overall** | **57** | Real custom shell alpha, not finished product |

---

## 9. Renderer, toolkit, text, fonts and accessibility

| Area | Score | Current truth |
|---|---:|---|
| Widget toolkit | 67 | Real layout, focus, dispatch, capture and controls; inconsistent depth remains |
| SDK/application framework | 65 | Real window/menu/event routing; central presenter and platform leakage remain |
| General renderer | 49 | WGPU immediate path works; retained resources, image textures and batching remain incomplete |
| Unicode shaping API | 66 | `cosmic-text` shaping exists; it is not yet the sole authoritative editing/layout path |
| Text editing model | 61 | UTF-8-safe selection and insertion; grapheme, bidi, IME and visual lines remain |
| Font infrastructure | 70 | Discovery, install, hashes, duplicates, enable state, roles and profiles are substantial |
| Font product integration | 36 | No complete Font Manager or live role resolution across the desktop |
| Accessibility infrastructure | 64 | Recursive live AT-SPI roles, actions, events, component and text export are runtime-observed |
| Accessibility daily-driver usability | 45 | First-party live export works on the tested Wayland/D-Bus path; Orca, keyboard-only completion and full desktop workflows remain |
| **Platform layer overall** | **58** | Good foundations, major release-blocking integration work |

The current accessibility source itself records best-effort D-Bus events and
recursive live trees, while live caret/selection synchronization and
assistive-technology workflows remain incomplete. Production claims are
prohibited until those workflows are demonstrated.

---

## 10. First-party application audit

| Application | Functional | UI/UX | Overall | Current truth |
|---|---:|---:|---:|---|
| File manager | 63 | 59 | **61** | Real navigation, file operations, trash and drag-to-folder; missing mature views, search, mounts, thumbnails, associations, conflicts and undo |
| Settings | 58 | 55 | **58** | Spaces and bounded display topology now read compositor state, send typed mutations, assign outputs and configure application policies; most service domains, Fonts and zoom policy remain disconnected |
| TextEdit | 67 | 61 | **64** | Selection-aware clipboard, caret insertion, find, save/recovery and undo/redo; no production multiline shaping, IME, rich text or scalable transactions |
| Terminal | 72 | 65 | **69** | Real PTY, parser, tabs, alternate screen, selection, resize and child shutdown; cell model lacks complete CJK/combining/grapheme correctness |
| Software manager | 46 | 48 | **47** | Hardened local archive installation now authenticates signed catalogues and packages with publisher identity; network delivery, updates, removal and transaction recovery remain incomplete |
| Preview | 64 | 44 | **54** | Real retained GPU image tiles, orientation/alpha, zoom/pan, Fit/Fill, rotation and Vision client paths; metadata/document workflows, colour management and pixel acceptance remain incomplete |
| **Application suite** | **59** | **54** | **58** | Useful native alpha applications, not daily-driver replacements |

### Naming defect

The visible `AirDrop` label must be removed. The native nearby-transfer feature
is **SLOPOS Share**. It requires independent SLOPOS-to-SLOPOS discovery,
authenticated encryption, consent, resume, integrity checking and atomic save.

Other inherited application names require deliberate public naming and legal
review before release.

---

## 11. SLOPOS Vision audit

| Area | Score | Current truth |
|---|---:|---|
| OCR/segmentation core | 74 | Real preprocessing, inference, decoding, masks and compositing |
| Model integrity | 68 | Hash and manifest validation exist; acquisition and redistributability workflow remain incomplete |
| Protocol | 72 | Typed local job/asset/error protocol |
| Client | 70 | Reusable local client and polling paths |
| Daemon | 72 | Session-local socket, bounded jobs and local-only operation are substantive |
| Preview integration | 60 | Real asynchronous request/result paths; polished successful workflow is not broadly proved |
| File-manager integration | 18 | Native context actions and output workflow remain incomplete |
| Accuracy/evaluation | 22 | No sufficient labelled benchmark and documented calibration |
| Performance/acceleration | 31 | CPU path exists; production acceleration and memory/cancellation benchmarks remain |
| Model distribution | 34 | No complete clean-install model-pack workflow |
| **Vision product overall** | **58** | Serious local subsystem alpha; distribution and measured product proof remain bottlenecks |

---

## 12. Security, quality and release engineering

| Area | Score | Current truth |
|---|---:|---|
| Rust architecture | 72 | Clear crate intent; several central files remain large |
| Error handling | 72 | Session, filesystem, installer, Vision and compositor handling improved |
| Filesystem safety | 78 | Atomic writes, path bounds, symlink checks, hashes and rollback are common |
| Session isolation | 85 | Private runtime/socket/token/process-group design is strong; the control socket now enforces mode 0600 and Linux peer credentials |
| Application sandbox/permissions | 27 | No mature general sandbox or capability permission product |
| Package trust/signing | 61 | Ed25519 catalog/archive signatures, trusted publisher/key identity, revocation, authenticated size and restrictive trust-store validation are implemented; network trust updates, signed release distribution and lifecycle recovery remain open |
| Automated testing | 82 | Broad tests and exact compositor contract |
| CI quality | 87 | Strong Linux build/test/release/fmt/lockfile/runtime gates at audited product head |
| Runtime QA breadth | 61 | Useful VM/UTM evidence; current-head hardware/app matrix incomplete |
| Performance engineering | 54 | Frame scheduling improved; renderer remains expensive |
| Packaging/install/upgrade | 45 | Artefacts exist; clean lifecycle and recovery need current evidence |
| Documentation discipline | 84 | Three-file structure and production target are now explicit |
| **Quality/release overall** | **66** | Engineering discipline is ahead of product maturity; IPC hardening is verified, while packaging and recovery remain open |

---

## 13. POSIX and FreeBSD truth

| Area | Score | Current truth |
|---|---:|---|
| Normative portability architecture | 90 | Required boundary is documented |
| Implemented platform abstraction | 24 | No complete `slopos-platform` boundary yet |
| POSIX-shell release surface | 20 | Existing release/QA scripts still contain Bash/GNU assumptions |
| Linux backend | 68 | Real implementation, still incomplete as a production desktop |
| FreeBSD compile support | 10 | No current native build matrix |
| FreeBSD runtime backend | 3 | No native compositor/session/application evidence |
| Cross-platform non-regression suite | 5 | Not established |
| **Portability implementation** | **22** | Correct direction, early implementation |

Linux remains the first production target. Portability work must not weaken or
fork the Linux desktop.

---

## 14. Required path from 63 to 100

### Gate 1 — Compositor 100

Complete and prove physical input, DnD/IME, multi-output, scaling, XWayland,
third-party applications, HDR/VRR hardware, direct scanout, GPU recovery, soaks
and fuzzing.

### Gate 2 — Production renderer

Ship glyph atlas, authoritative shaping, grapheme/bidi/IME editing geometry,
real image textures, retained resources, batching and scale-aware caches.

### Gate 3 — Complete desktop product

Connect SLOPOS Spaces, font profiles, zoom policy, Dock, notifications, search,
lock/session and multi-monitor policies to authoritative compositor/services.

### Gate 4 — Authoritative Settings and services

Replace preference-only or shell-command paths with typed state, application,
failure reporting and rollback for displays, input, audio, network, Bluetooth,
power, accessibility, fonts, Spaces, permissions and defaults.

### Gate 5 — Finish first-party applications

Complete the file manager, text editor, Terminal, Preview and software manager
for every advertised workflow; hide commands that are not implemented.

### Gate 6 — Ecosystem compatibility

Pass GTK, Qt, Electron, browsers, office, media, communication, development,
gaming, portals, Flatpak and XWayland matrices.

### Gate 7 — Accessibility and localisation

Live AT-SPI tree, Orca workflows, keyboard-only completion, high contrast,
reduced motion, locale extraction, bidi UI and translation QA.

### Gate 8 — Security and trust

Sandbox/permission strategy, portal enforcement, signed bundles, publisher
identity, safe received files, threat model and security regression suite.

### Gate 9 — Reliability and release

Performance budgets, 24-hour soaks, leak plateaus, crash recovery, clean install,
upgrade, rollback, uninstall, configuration migration and signed release
artefacts.

### Gate 10 — Production declaration

The product may call itself production-ready only when the weighted score reaches
at least 92, no release-blocking domain remains incomplete, normal users can
install and operate it without development tools, and this file contains current
exact evidence without contradiction.

The aspirational endpoint remains 100/100.

---

## 15. Immediate next implementation order

1. finish the Linux compositor acceptance matrix;
2. implement production text and image rendering;
3. connect Spaces, fonts and zoom policy to the live desktop;
4. make Settings authoritative;
5. complete first-party applications;
6. finish portals, XWayland and third-party compatibility;
7. complete accessibility and localisation;
8. finish security, packaging, performance, recovery and long soaks;
9. implement the POSIX platform boundary and native FreeBSD support.

Do not divert core effort into decorative features while an earlier
release-blocking invariant remains broken.

---

## 16. Bottom line

SLOPOS-I has enough real implementation to be taken seriously as a custom Linux
desktop project. It has a sovereign compositor/session foundation, a distinct
interface, useful first-party applications, local Vision functionality and
strong Linux CI.

It is still **63/100** because production readiness is determined by complete
user workflows, hardware/application compatibility, accessibility, security,
installation, recovery and long-term reliability—not by repository size or the
number of implemented types.

The public mission is now unambiguous: **finish SLOPOS-I to a genuine 100/100
production desktop environment competitive with KDE Plasma and GNOME, while
keeping every progress claim tied to evidence.**
