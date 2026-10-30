# SLOPOS-I architecture: what exists today

This page describes the code as it is, not the target. For the target, see the [contract](CONTRACT.md). When the two disagree, this page describes reality and the contract describes where it should go. Last reviewed: 2026-10-30.

SLOPOS-I is an **alpha** X11 desktop environment. Openbox manages windows. SLOPOS adds a session supervisor, a top bar, a desktop window, a Dock, and native applications on its own toolkit: Calculator, Files, Image Viewer, Media, Control Panels and Software. Every surface a SLOPOS session starts is on the SLOPOS toolkit; no GTK program is started by the session. Bundled SLOPOS paths open only the native apps; GTK donor apps that duplicate them are not installed or launched.

## Processes in a session

```
display manager ─► start-slopos-i ─► slopos-session (supervisor, pid lock)
                                        ├─ slopos-control-panels --restore   once, ≤5 s: kept display layout, keyboard and mouse
                                        ├─ openbox            Required: its failure ends the session
                                        ├─ picom / xcompmgr   Optional: skipped if missing or SLOPOS_NO_COMPOSITOR
                                        ├─ slopos-desktop     Optional: desktop icons; off with SLOPOS_NO_DESKTOP
                                        ├─ slopos-topbar      Important: restarted with backoff, session survives
                                        └─ slopos-dock        Important: as above; off with SLOPOS_NO_DOCK
apps launched from the top bar, desktop or Dock: detached (setsid), reaped, not supervised
```

- `start-slopos-i` is the X session entry point (`packaging/slopos-i.desktop` runs it). It sets up D-Bus activation and the source-tree or installed share directory, then executes `slopos-session`.
- `slopos-session` (`crates/slopos-session`):
  - takes a lock at `$XDG_RUNTIME_DIR/slopos-session<display>.pid`, so a second instance refuses to start;
  - runs `slopos-control-panels --restore` (killed after 5 s) so the display layout and keyboard/mouse settings kept in Control Panels apply before anything sizes itself to the screen;
  - configures the environment (`environment.rs`): `XDG_*`, the GTK theme and GTK 2 rc for the chosen appearance and the Openbox config path, then paints the desktop itself: the saved picture (`slopos_core::wallpaper`), Platinum's default blue field, or the appearance colour, through `slopos_ui::platform::x11::set_root_background` (a retained root pixmap announced in `_XROOTPMAP_ID`). `slopos-session --paint-desktop` repaints after a change; no feh, xsetroot or PCManFM is involved;
  - supervises its children (`supervisor.rs`). After a crash a child is restarted with a delay of 250 ms × 2ⁿ. After 5 fast failures in a row it is given up, and that is reported once.
  - `slopos-session --logout` sends SIGTERM to the running supervisor, which stops the children in reverse order (SIGTERM, then SIGKILL after 2 s). The Openbox root menu and the top bar's System menu both use it.
- Appearances: `platinum` (default), `classic`, `graphite` and `oled`. Each one picks an Openbox rc file, a GTK theme and a root colour. A user's own `~/.config/slopos-i/openbox/rc.xml` always wins.

## Workspace crates

| Crate | Kind | Toolkit | What it does |
|---|---|---|---|
| `slopos-core` | library | — | Shared primitives: XDG paths, `.desktop` discovery and parsing, process launch/resolve/reap, capability and IPC types. |
| `slopos-ui` | library | **own** | The SLOPOS toolkit (below). No GTK. |
| `slopos-services` | library | — | System providers with typed snapshots, "unavailable" when a service is missing: NetworkManager (incl. Wi-Fi join/forget), audio (PipeWire/Pulse devices and per-application streams), brightness, UPower, timedate, BlueZ (incl. pairing and forgetting devices), MPRIS, UDisks, display outputs (XRandR) and keyboard/mouse settings (XKB, XInput 2 libinput properties). |
| `slopos-session` | binary | — | Session supervisor (above). |
| `slopos-topbar` | binary | **own** | The top bar (below): System menu, global menus, status items, launcher, notification server, window actions. |
| `slopos-desktop` | binary | **own** | The desktop window (below): desktop picture and icons. |
| `slopos-shell` | binary | GTK 3 (legacy) | Superseded by `slopos-topbar`. Still built in the workspace for its tests; not installed by `install.sh` or the live ISO, and the session does not start it. |
| `slopos-dock` | binary | **own** | The Dock (below). |
| `slopos-settings` | binary | GTK 3 (legacy) | Superseded by `apps/control-panels`. Still built in the workspace; not installed by `install.sh` or the live ISO, and nothing launches it. |
| `slopos-catalogue` | binary | GTK 3 (legacy) | Superseded by `apps/software`. Still built; not installed or launched. |
| `apps/calculator` | binary | **own** | Calculator, the first native app. |
| `apps/files` | binary | **own** | Files: bounded async folder listing, Back/Forward/Up/Home/location, multi-select list with Name/Size/Modified/Kind, menus and context menus, transactional copy/move/rename/new folder, Trash with restore/empty, confirmed permanent delete, conflict dialog, Compress / Extract Here, inotify refresh, MIME default-app open. |
| `apps/image-viewer` | binary | **own** | Image Viewer: bounded PNG/JPEG/GIF/BMP/WebP/TIFF/SVG decoding with EXIF orientation, folder navigation, zoom and pan. |
| `apps/media` | binary | **own** | Media: GStreamer `playbin` behind an engine adapter; video frames through an RGBA appsink, play/pause, seek, volume, mute, sidecar subtitles, honest missing-codec and format errors. |
| `apps/control-panels` | binary | **own** | Control Panels: Network, Sound, Display, Bluetooth, Date & Time, Desktop, Appearance and Keyboard & Mouse on `slopos-services` (below). |
| `apps/software` | binary | **own** | Software: the catalogue window plus the Ed25519-signed catalogue, trust store and transactional AppImage install/update/uninstall with recovery; publisher CLI (keygen/sign/verify). |
| `tools/slopos-ui-gallery` | binary | **own** | Component gallery, with a `--snapshot` mode for captures. |

`scripts/check-architecture.py` stops anything under `apps/` from depending on GTK, GDK or Pango. The three legacy GTK crates are no longer installed or started; they stay in the workspace until their remaining tests and helpers are moved or retired.

## The SLOPOS toolkit (`crates/slopos-ui`)

| Module | Contents |
|---|---|
| `foundation`, `layout` | Integer geometry (`Rect`, `Scale`), colours, insets and layout helpers. |
| `render` | `RasterSurface`: a tiny-skia pixmap at an integer scale, with rect fill/stroke, image and text drawing, a nested clip stack (`push_clip`/`pop_clip`/`with_clip`) and PNG output. All coordinates are logical pixels. |
| `scene`, `scene_raster` | Early renderer-neutral, bounded command recording (clear, fill/stroke rectangles) and a software adapter that replays into the existing `RasterSurface`. No GPU renderer or full UI scene graph yet. |
| `animation` | Pure, deterministic timeline sampling (linear/cubic easings and reduced-motion completion). The caller remains responsible for event-based frame scheduling. |
| `text` | `Font`, `measure`, and `caret_stops`/`hit_test` for caret placement (char boundaries, clusters and RTL runs). cosmic-text shaping, preferring Liberation Sans or DejaVu Sans. |
| `theme` | Semantic colour roles (`Palette`: surface, content, ink, selection, focus, menu highlight, danger, scrollbar…). Only the provisional Platinum values exist. |
| `accessibility` | `window_tree`: builds an app's flat AccessKit window tree with focus fallback. |
| `image` | `Image`: PNG decoding and SVG rasterisation (resvg, with system fonts for SVG text), bilinear scaling, and `compose_background` (fill, tile or centre over a backdrop) for desktop pictures. |
| `icons` | `IconLookup`: freedesktop icon-theme lookup with `Inherits`, size matching, `hicolor` and `/usr/share/pixmaps`. Default theme `SLOPOS-Platinum`, overridden by `SLOPOS_ICON_THEME`. |
| `primitives` | Classic component painters, e.g. the button plate (Regular, Default and Secondary × Rest, Pressed and Disabled, at any size). |
| `controls` | Interactive controls as explicit state machines, each with keyboard, pointer and AccessKit support: `Button` (text, or an icon with the label kept as the accessible name); `TextField` (single-line Unicode editing, word motion, shift/drag selection, Ctrl+A/C/X/V/Z, placeholder, max length, steady caret, `TextInput` with SetValue); `FocusChain` (Tab/Shift+Tab); `ScrollState` (offset, wheel-over-area, track paging, thumb drag, Classic scrollbar); `ListView` (virtualized rows, single/multiple selection with Shift/Ctrl click and keys, type-ahead, double-click/Enter activation, context-menu requests, bounded `ListBox` a11y); `Menu` and `MenuBar` (in-window pull-down/context menus, keyboard and pointer, shortcuts text, separators, disabled/checked/checkable items, F10; a global mode in which the bar takes no space and choices arrive from the shell); `Dialog` (application-modal alert or text prompt with scrim, Enter/Escape; a standalone mode for popup windows); `TextField` secret mode (bullets, no copy/cut, `PasswordInput` with no value in the accessibility tree); `PopUpButton` (pop-up menu control: opens over the current value, press-drag-release or click, `ComboBox` semantics); `FileChooser` (modal Open dialog with filtered, sorted, capped folder list, path field, Up); `Slider`, `Checkbox`, `ProgressBar` (determinate or static indeterminate). |
| `event` | Pointer (with host double-click detection), keyboard, scroll, focus, resize, timer, wake, clipboard paste, accessibility events (click, focus, set value, increment/decrement), and global-menu events (`MenuChosen`, `GlobalMenu(bool)`). |
| `platform` | `App` trait, `Control` and `WindowOptions`. The x11rb host: a poll(2) loop that sleeps until there is input, a timer or a `Proxy::wake()` from another thread; frames sent with PutImage strips; Normal, Dock (with struts) and Desktop window kinds; override-redirect popup windows with pointer/keyboard grabs (`Control::open_popup`, `App::paint_popup`); desktop-wide key grabs (`WindowOptions::global_keys`, all lock-modifier combinations); HiDPI via `SLOPOS_SCALE`/`GDK_SCALE`; the X11 `CLIPBOARD` selection (`Control::set_clipboard`, `request_paste`; UTF8_STRING/STRING/TARGETS, 256 KiB, no INCR); keymap refresh on `MappingNotify`; AccessKit trees on AT-SPI (feature `atspi`, on by default); `set_root_background` for the desktop picture. `global_menu` (feature `global-menu`, on by default) exports an app's `App::menu_bar` over `org.gtk.Menus`/`org.gtk.Actions` on the session bus and tags the window with `_GTK_UNIQUE_BUS_NAME`, `_GTK_MENUBAR_OBJECT_PATH` and `_GTK_APPLICATION_OBJECT_PATH` before it maps; enabled/checked changes are pushed with `Changed` signals. The app hides its in-window bar only while the top bar owns `org.slopos.GlobalMenu`; a blocking NameOwnerChanged watcher brings the bar back if the top bar exits. `menu_import` is the other side: it reads any window's `org.gtk.Menus`/`org.gtk.Actions` model (sections, submenus, accelerators, check/radio state) and activates actions. |

An app implements `App::event`, `App::paint` and, optionally, `App::accessibility`, then calls `platform::run`. The Calculator (`apps/calculator/src/main.rs`) is the reference example.

Not built yet: drag and drop, IME pre-edit (XIM/IBus), trees/icon grids, `slopos-appkit`, GPU/Vello/wgpu rendering, an advanced composited scene graph, and theme datasets beyond the provisional Platinum colours. See [the UI toolkit program](ui-toolkit-program.md) for in-repository toolkit milestones and optional external distribution; [native application plan](application-platform.md) and [swarm execution](swarm-execution.md) document future product slices, not implemented features.

## The Dock (`crates/slopos-dock`)

- **Window:** a bottom-centred window of type `_NET_WM_WINDOW_TYPE_DOCK` that reserves its strip with `_NET_WM_STRUT_PARTIAL`. It resizes and re-centres as tiles come and go.
- **Pins:** read from `~/.config/slopos-i/dock-pins`, one desktop id per line. Without that file it pins one installed app per role: Files, browser, terminal, editor, Calculator, Media, Software and Control Panels. The SLOPOS roles name only the native apps; a donor file manager never stands in for Files. `slopos-dock --pin-defaults` writes the defaults to the file.
- **Running windows:** `windows.rs` has its own X connection on a thread. It watches `_NET_CLIENT_LIST_STACKING`, `_NET_ACTIVE_WINDOW` and each client's title and state. Windows are matched to apps by `WM_CLASS`/`StartupWMClass` (`model.rs`, tested without X). Unpinned running apps appear after a separator.
- **Interaction:**
  - Click launches the app, raises its top window, or cycles through its windows when the app is already in front.
  - Middle-click starts another instance.
  - One dot per window, up to 3. The dots are amber when a window asks for attention.
  - Screen readers see a toolbar of buttons.
- **Not built yet:** context menus, drag-to-reorder, pin/unpin from the UI, auto-hide, multi-monitor placement and a Settings panel.

## The top bar (`crates/slopos-topbar`)

- A 26-pixel `_NET_WM_WINDOW_TYPE_DOCK` window across the top with a strut. One instance per display ("Refusing to start a second SLOPOS top bar").
- **Menus:** the System menu (About, Control Panels, Software, Desktop Picture, appearance choices, Switch User, and Log Out/Restart/Shut Down with confirmation) and either the desktop menus (File, View, Special, Help) or the focused window's menus. The top bar owns `org.slopos.GlobalMenu` and imports the focused window's `org.gtk.Menus` model through `slopos_ui::platform::menu_import`, so SLOPOS toolkit apps and GTK applications alike show their menus here; choosing an item activates the app's action. Windows without exported menus get Close, Minimise and Zoom (EWMH requests). Menus are toolkit popup windows with keyboard navigation.
- **Status items** come from `slopos-services` on a blocking thread: sound (output volume, mute), network (connection name, "Offline", "Network unavailable"), battery when there is one, and the clock (a timer only for the minute change).
- **Launcher:** fuzzy search over `.desktop` entries in a popup, opened by Super+Space (Openbox sends SIGUSR1), Ctrl+F2 (a global key grab) or the System menu.
- **Notifications:** an `org.freedesktop.Notifications` 1.2 server (body, actions, persistence; expiry, replace, close, `ActionInvoked`/`NotificationClosed`), shown as toasts under the bar's right end, three at a time.
- **Not built yet:** status popovers (Wi-Fi/Bluetooth/volume/brightness menus), volume and brightness OSDs, a notification history, multi-monitor bars.

## The desktop (`crates/slopos-desktop`)

- A `_NET_WM_WINDOW_TYPE_DESKTOP` window the size of the screen, under every window. It shows a copy of the root picture the session paints (re-read when `_XROOTPMAP_ID` changes), so the desktop looks the same with or without it, and follows screen-size changes.
- **Icons:** Home, removable volumes mounted under `/media` and `/run/media`, the items of `XDG_DESKTOP_DIR` (hidden files skipped, at most 400) and the Trash (full or empty), placed from the top-right corner down. Positions the user drags to are kept per item in `~/.config/slopos-i/desktop-positions`.
- **Interaction:** click/Ctrl-click selection, drag to a free cell, double-click/Return/Ctrl+O to open (Files for folders, volumes and Home, `slopos-files --trash` for the Trash, the default application for files), arrows, type-to-select, Ctrl+Shift+N for a new folder that never replaces one, and a context menu (New Folder, Change Desktop Picture…, Open Desktop in Files). Items are a `List` of `ListItem`s for assistive technology.
- Refresh is event driven: inotify on the desktop folder and the Trash, `POLLPRI` on `/proc/self/mountinfo` for mounts, X events for the picture and size.
- **Not built yet:** rename, Move to Trash and drag and drop between the desktop and Files, mounting unmounted volumes, one window per monitor.

## Packages and CI

- **Packages:** `install.sh` installs from source into a prefix: the session, top bar, desktop, Dock and the native apps (Calculator, Files, Image Viewer, Media, Control Panels, Software), their desktop entries, Openbox and GTK themes, the icon theme and wallpapers. The runtime dependency lists (`packaging/deps`, the Arch ISO list and the VM installer) no longer include the GTK donor apps that duplicate SLOPOS's own (PCManFM, Ristretto, galculator, mpv, pavucontrol, blueman, arandr, lxappearance, xfce4-settings, the NetworkManager applet); they keep the services the native apps use (NetworkManager, PipeWire, BlueZ, UPower, xfce4-power-manager). `assets/config/mimeapps.list` names only native apps for images, media and folders.
- **Live ISO:** `packaging/iso/build-live-iso.sh --arch amd64|arm64` builds an Ubuntu 24.04 casper live image (BIOS+UEFI for amd64, UEFI for arm64) that autologins into SLOPOS-I; `test-live-iso.sh` boots it in QEMU and checks the desktop and launcher. `.github/workflows/live-iso.yml` builds both and boot-tests amd64.
- **CI:** `.github/workflows/ci.yml` is one job that runs fmt, clippy `-D warnings`, the workspace tests, the Python architecture/spec checks, shell syntax checks, and `scripts/smoke-session.sh`. The smoke test boots a real session under Xvfb and checks the top bar, the desktop window, the Dock and Calculator, then logs out and checks that every process exited.
- **Nightlies:** `nightly.yml` publishes the `.deb` and `.pkg.tar.zst` from `main` to the rolling `nightly` release.
- **Scheduled:** the heavy X11 QA runs nightly (`qa.yml`). The ISO, installed-VM, recovery and hardware-contract workflows run weekly.

See [development.md](development.md) for commands.

## Known gaps

The [ledger](contract/ledger.md) has the dated evidence; its section G.2 is the prioritized register of remaining work (GAP-* IDs). In short:

- Native Files has no drag and drop, icon view, thumbnails, properties window, removable-volume mount/eject or network mounts yet, and has not passed installed-package VM acceptance. Terminal, Text editor and System Monitor are not native yet (the system terminal and Mousepad are still used).
- No OSDs or status popovers, workspaces UI, lock screen, polkit agent, network-secret agent (Wi-Fi passwords are asked by Control Panels only) or BlueZ pairing agent (devices that need a PIN or passkey confirmation fail with BlueZ's error).
- No production software catalogue or publisher key exists yet, so Software shows "No software catalogue is installed" on a fresh system.
- The legacy GTK crates (`slopos-shell`, `slopos-settings`, `slopos-catalogue`) are still built in the workspace but no longer shipped or started.
- X11 only. GNOME and KDE are leaving X11, so distribution support for X11 logins has to be checked per release (§21.10).
- Not tested on real hardware or on the Raspberry Pi edge target.

## SLOPOS Intelligence preview (2026-10-28)

An independent opt-in `slopos-intelligence` binary is present in `crates/slopos-intelligence`.
It proposes strict JSON actions via loopback-only Ollama (`lfm3:1.7b`) using installed-app metadata, and asks for confirmation before launching a listed desktop application. It does not run at session startup; no window, Siri-style voice interface, personal search, accessibility-based UI control or PARAMA execution exists. See [intelligence.md](intelligence.md) for the precise trust boundary and roadmap.

## Native Image Viewer (`apps/image-viewer`)

- **Decoding** (`decode.rs`, adapted from the archived Preview decoder): PNG, JPEG, GIF, BMP, WebP and TIFF through the `image` crate with only those codecs enabled, SVG through resvg. The format is sniffed from content. Files must be regular and at most 64 MB; dimensions are checked before decoding (40 MP, 32 768 px per side) and the decoder's allocation limit stops decompression bombs. EXIF orientation is applied. Decoded images live in a 256 MB LRU cache.
- **UI** (`main.rs`): menu bar (File, View, Go), toolbar, Open dialog filtered to images, next/previous/first/last through the image's folder with the next image decoded ahead, Fit/1:1/zoom (keys, buttons, wheel), drag to pan, double-click to toggle Fit and 1:1, status line with size, format, zoom and position. Decoding runs on a worker thread and wakes the window.
- Not built: rotation and editing, slideshow, thumbnails grid, colour management (ICC profiles are ignored), animated GIF/WebP playback (first frame only), HEIF/AVIF/RAW.

## Native Files (`apps/files`)

- **Model** (`model.rs`, from PR #35): non-recursive listing via `symlink_metadata`, so directory symlinks are never traversed while listing; folders over 8192 entries fail explicitly instead of truncating; Back/Forward history.
- **Workers** (`worker.rs`): one folder-reading thread (bounded queue, stale results dropped), one thread per file operation with cancel and throttled progress wakes, and an inotify watcher thread. All wake the UI through `Proxy`; nothing polls.
- **Archives** (`archive.rs`): Compress (zip, tar.gz) and Extract Here (zip, tar, tar.gz, tar.xz, tar.zst) run as operations with progress and Cancel and the same partial-then-no-replace commit. Extraction refuses absolute and `..` paths, creates symlinks only after all files and only if they stay inside, skips hard links and devices, drops setuid/setgid/sticky, and caps entries (200,000) and bytes (200× the archive, ≥256 MiB, ≤64 GiB, never past free space − 64 MiB). A single top-level folder is not nested again. Pure-Rust crates: `tar` (MIT/Apache-2.0), `zip` with deflate only (MIT; `zopfli` Apache-2.0), `lzma-rs` (MIT), `ruzstd` (MIT), `flate2`.
- **Operations** (`ops.rs`, `trash.rs`): copies are written under a private partial name and committed with `renameat2(RENAME_NOREPLACE)`; cancel or failure removes only the partial; Replace swaps the old item aside and deletes it only after the new one is in place; cross-device moves delete the source only after a committed copy; symlinks are copied as links; FIFOs and devices are refused; copying a folder into itself is refused. Trash follows the freedesktop spec (home trash, or `$topdir/.Trash-$uid` on other volumes), and restore never overwrites.
- **UI** (`main.rs`): menu bar (File, Edit, View, Go; global when the shell runs), icon toolbar (Back, Forward, Enclosing Folder, Home), location field (Ctrl+L), a Places sidebar (Home, the standard folders that exist, Computer, Trash) that follows navigation, theme icons per row (special folders, MIME type, generic fallback), the shared `controls::list::ListView` with Name/Size/Modified/Kind columns, context menus, Rename/conflict/confirm dialogs, Trash view (restore, delete, empty), hidden-file toggle, status bar with progress and Cancel. Opening uses `slopos_core::mime` (the default app from `mimeapps.list` or a desktop entry's `MimeType`), falling back to `xdg-open`.

## Native Media (`apps/media`)

- **Engine** (`engine.rs`): GStreamer `playbin` with a bin of `videoconvert ! videoscale ! capsfilter(RGBA) ! appsink` (one frame buffered, newest wins; frames over 1920×1080 are scaled down by GStreamer). A bus thread turns messages into events and wakes the window; there is no polling. Paused preroll frames arrive through `new_preroll`. Without a sound server or ALSA device, audio goes to a synchronised `fakesink` instead of failing. Sidecar `.srt/.vtt/.ass/.ssa` files with the same stem are loaded as subtitles (GStreamer cannot detect very short subtitle files).
- **Errors**: unknown formats say so; a missing decoder names the codec and the package that usually provides it (`gstreamer1.0-libav` for H.264/H.265/AAC, which SLOPOS does not ship).
- **UI** (`main.rs`): video area scaled to fit, seek slider with times, Play/Pause, volume slider, Mute, Subtitles toggle, Open dialog filtered to media; Space/K, Left/Right (±5 s), Up/Down (volume), M, Ctrl+O. A 250 ms timer runs only while playing.
- Licences: GStreamer core and base/good/x plugins are LGPL-2.1+, linked dynamically; gstreamer-rs is MIT/Apache-2.0.
- Not built: playlists, fullscreen, audio/subtitle track selection, hardware decoding, embedded subtitle tracks UI.

## Native Control Panels (`apps/control-panels`)

- **Providers** (`services.rs`): one current-thread Tokio runtime connects the `slopos-services` clients (NetworkManager, audio, BlueZ, timedated, brightness, RandR displays, XKB/XInput input), forwards their snapshot streams and runs changes; it blocks on streams and the command channel and wakes the window. A missing provider shows "unavailable" with the reason and disables its controls.
- **Panels:**
  - Network: networking and Wi-Fi toggles, scan, network list marking the connected and saved networks; Join (secured networks without a saved profile ask for the password in a dialog with a secret field; the profile is validated, then `AddAndActivateConnection`), Disconnect, Forget.
  - Sound: Output and Input device pop-ups, volume and mute for the chosen devices; an Application section to pick a playing application and set its volume, mute it or move it to another output.
  - Display: backlight; Display, Resolution, Refresh rate, Rotation and Position (right/left/above/below/mirror of the main display) pop-ups, Use this display, Show the menu bar here. Changes go through RandR as one plan (modes and rotations checked, CRTCs shared for mirroring, screen resized under a server grab); every change asks "Keep these display settings?" and reverts after 15 seconds without an answer. Kept layouts are saved to `displays.conf` and re-applied at login when the same monitors are connected.
  - Bluetooth: adapter power, Search for Devices, device list (connected, paired, then by name), Pair (Cancel Pairing while it runs; paired devices are trusted and connected), Connect/Disconnect, Forget.
  - Date & Time (automatic time), Desktop (bundled patterns with thumbnails, Choose Picture…; saved to `~/.config/slopos-i/wallpaper` and repainted at once), Appearance (Platinum, Classic, Graphite, OLED through `slopos-appearance`).
  - Keyboard & Mouse: keyboard layout (from the XKB rules list, applied with `setxkbmap`), repeat delay and rate (XKB controls), pointer speed and natural scrolling (libinput properties, only when a device takes them), left-handed buttons. Saved to `input.conf` and re-applied at login.
- **UI:** sidebar with theme icons, File/View menus (global when the top bar runs), Ctrl+1…8 to switch panels, `--network`/`--sound`/`--desktop`/`--keyboard`… flags to open a panel, `--restore` (also `--apply-displays`, `--apply-input`) for the session; pop-ups with nothing to choose are skipped by Tab; status text is red only for failures.
- Not built: a network-secret agent for other apps, enterprise/hidden Wi-Fi, VPN; a BlueZ pairing agent (PIN/passkey); time zone selection; Dock settings; per-monitor scale.

## Native Software (`apps/software`)

- **Trust and transactions** (`trust.rs`, `install.rs`): the catalogue and every entry are Ed25519-signed and checked against `/etc/slopos-i/software/trust.json` (regular, not group/world-writable files only); installs stream over HTTPS to a partial file, check size, SHA-256 and the ELF header, then commit with rollback; interrupted transactions are recovered at start-up.
- **Window** (`store.rs`, `window.rs`): lists the verified catalogue's apps for this architecture plus installed AppImages, with Install/Update/Remove…/Open, real byte progress and Cancel. A missing or rejected catalogue offers nothing and says why; installed apps can still be removed. Removal asks first; failures show the transaction's own error.
- Not built: a published production catalogue and publisher key (owner step), screenshots and ratings, automatic update checks, distribution packages.
