# Native desktop plan: retire the last GTK surfaces (PR #36)

Owner direction (2026-10-30): build native replacements for every donor app
removed in PR #36 *in that PR*, and move the top bar off GTK 3 as the next
priority. amd64 is the development focus; arm64 ISOs are built by the
`live-iso` workflow on GitHub's native arm64 runners, not locally.

This is a plan, not a status report. What exists is in
[architecture.md](architecture.md); evidence is in the
[ledger](contract/ledger.md). Every item lands with tests, a live Xvfb check
and a ledger row, following AGENTS.md (toolkit controls go into
`slopos-ui`, no polling, truthful state, no GTK under `apps/`).

## What is still not native

| Surface or capability | Today | Native replacement |
|---|---|---|
| Top bar: menu bar, System menu, status items, clock | `slopos-shell` (GTK 3) | `slopos-topbar` on `slopos-ui` (phase B) |
| Launcher (Super+Space) and notifications daemon | `slopos-shell` (GTK 3) | part of `slopos-topbar` (phase B) |
| Wi-Fi join with a password, hidden networks, forget | removed with the NM applet | Control Panels › Network + network-secret dialog (phase C1) |
| Bluetooth discovery, pairing, connect | removed with blueman | Control Panels › Bluetooth + pairing agent (phase C2) |
| Display resolution, rotation, arrangement, primary | removed with arandr | Control Panels › Displays on XRandR, with revert timer (phase C3) |
| Output/input device choice, per-app volume | removed with pavucontrol | Control Panels › Sound (phase C4) |
| Keyboard and mouse settings | removed with xfce4-settings | Control Panels › Keyboard & Mouse (phase C5) |
| Desktop icons (Home, Trash, volumes, `~/Desktop`) | removed with PCManFM | `slopos-desktop` desktop window (phase D) |
| Archives (zip/tar) | removed with PCManFM integration | Files › Compress / Extract (phase E) |
| Terminal, Text editor, power manager | donor apps still shipped | later phases (F), listed for completeness |

## Phase A — toolkit prerequisites (`crates/slopos-ui`)

1. **Popup windows.** Menus and popovers drawn in their own
   override-redirect windows, so they can extend outside a small parent
   (a 26 px top bar). `Control::open_popup(id, rect)` / `close_popup(id)`,
   `App::paint_popup(id, surface)`, events tagged with the popup id; a
   pointer and keyboard grab while a menu is open, released on close.
   Tests: geometry clamping to the screen, event routing, grab release,
   no leaked windows after close.
2. **Global key grabs.** `WindowOptions::global_keys` (keysym + modifiers,
   grabbed with every lock-mask combination) delivered as
   `Event::GlobalKey`. Needed for Super+Space, Ctrl+F2 and media keys.
3. **GMenu client.** The counterpart of `platform::global_menu`: a zbus
   client for `org.gtk.Menus`/`org.gtk.Actions` that turns an exported
   menu into toolkit menu data, follows `Changed` signals and activates
   actions. Tested against our own exporter on a private bus.

## Phase B — `slopos-topbar` replaces the GTK shell

A new crate on `slopos-ui`, run by `slopos-session` in place of
`slopos-shell` once it reaches parity; `slopos-shell` is then dropped from
`install.sh` and the ISO (kept in the workspace until removed by a later
change).

- Dock-type window across the primary monitor's top edge, `_NET_WM_STRUT`.
- **System menu** (About, Control Panels, Software, appearance, Sleep,
  Restart, Shut Down, Log Out with confirmations through logind and
  `slopos-session --logout`).
- **Menus:** desktop menus (File, View, Special, Help) when no app is
  focused; the focused window's exported menus through the GMenu client;
  window actions for windows that export none. Keyboard navigation, F10/
  Ctrl+F2, accelerators shown.
- **Status items** from `slopos-services`: sound (popover with volume
  slider, mute, output choice), network (popover with Wi-Fi list and
  join), Bluetooth, battery, clock (popover with a month calendar).
  Missing providers say "unavailable".
- **Launcher** window (Super+Space): fuzzy search over desktop entries,
  keyboard-only operation, launches through `slopos_core::process`.
- **Notifications:** `org.freedesktop.Notifications` daemon with toasts
  in popup windows, actions, expiry by timer only while a toast is shown.
- Acceptance: the shell tests' guarantees carried over (truthful status,
  no fake strings), smoke test against `slopos-topbar`, ISO boot test.

## Phase C — Control Panels replacements

1. **Network:** join open and WPA-PSK networks (password dialog, show
   password, error from NetworkManager shown verbatim), hidden network,
   forget saved network; a network-secret agent so NetworkManager's own
   password requests get the same dialog.
2. **Bluetooth:** discovery with a live device list, pair (agent for
   passkey confirmation / PIN display), connect, disconnect, trust,
   remove.
3. **Displays:** outputs from XRandR, resolution, refresh, rotation,
   primary, on/off, side-by-side arrangement (pointer and keyboard), mirror;
   apply with a 15-second "Keep this arrangement?" revert; saved and
   reapplied at session start.
4. **Sound:** output/input device choice, per-application volume and mute
   from the sound server's streams.
5. **Keyboard & Mouse:** key repeat delay/rate (XKB), layout (localed),
   pointer speed, natural scrolling, tap to click (libinput properties via
   XInput 2).

## Phase D — `slopos-desktop`

A `_NET_WM_WINDOW_TYPE_DESKTOP` window per monitor that paints the desktop
picture and icons for Home, Trash, mounted volumes and `~/Desktop` items:
selection, keyboard navigation, double-click/Enter to open, context menu
(New Folder, Change Desktop Picture, Open in Files), inotify refresh,
remembered positions. The session keeps painting the root window as a
fallback.

## Phase E — Files: archives

Compress (zip, tar.gz) and Extract Here (zip, tar, tar.gz/xz/zst) as
cancellable Files operations with the same no-overwrite commit rules as
copy, and protection against path traversal and archive bombs.

## Phase F — later (not in PR #36 unless time allows)

Native Terminal (PTY, VT parser), Text editor, power-management daemon
(lid, idle, battery actions), lock screen and polkit agent, System Monitor,
Screenshot, Disks. These are contract §16–18 items with no donor removed
in PR #36.

## Order of work

A1 → A2 → A3 → B (bar, System menu, desktop menus, global menus, status
items, clock, launcher, notifications) → C1 → C2 → C3 → C4 → C5 → D → E.
Each step is committed and pushed on its own with its tests.

## arm64

`.github/workflows/live-iso.yml` builds both ISOs on native runners and
boot-tests amd64. It runs for pull requests that change the image, so
arm64 is built remotely while local work stays on amd64.
