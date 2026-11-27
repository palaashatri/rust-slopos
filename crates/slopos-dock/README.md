# SLOPOS Dock (`slopos-dock`)

The bar at the bottom of the screen. It shows pinned applications and the
applications that are running, and it can hide itself until the pointer reaches
the bottom edge.

![The Dock, cropped from the desktop capture](screenshots/dock.png)

*Captured 2026-11-27 from the desktop run: six pinned applications and no
windows open, so there are no window dots yet.*

## Features

- **Pinned apps:** read from `~/.config/slopos-i/dock-pins`, one desktop id per
  line. Without that file the Dock pins one installed app for each role: Files,
  browser, terminal, editor, Calculator, Media, Software and Control Panels.
  SLOPOS roles name only its own native apps, so a GTK donor file manager never
  stands in for Files.
- **Running apps:** unpinned running apps appear after a separator. Windows are
  matched to their app by `WM_CLASS` and `StartupWMClass`.
- **Click:** launches a pinned app. If it is already running, the Dock raises its
  top window, or cycles through its windows when it is already in front.
- **Middle-click:** starts another instance.
- **Window dots:** one dot per window, up to three. A dot turns amber when its
  window asks for attention.
- **Auto-hide (off by default):** turn it on or off from the right-click menu,
  or with `slopos-dock --autohide on|off`. Hidden, the Dock leaves a 4-pixel strip
  on the bottom edge and reserves no space, so maximised windows use the whole
  screen. The pointer reaching the strip shows it again. It hides 600 ms after the
  pointer leaves, and not while its menu is open.
- **Accessibility:** a toolbar of buttons for screen readers.

## How it works

- `src/main.rs`: the Dock window, its layout and the input.
- `src/model.rs`: pinned and running items, and the matching of windows to apps.
  Tested without X.
- `src/pins.rs` and `src/prefs.rs`: the pin file and `dock.conf`.
- `src/windows.rs`: its own X connection on a thread. It watches the client
  list, the active window and each window's title and state.
- `src/autohide.rs`: the auto-hide state machine, tested without X.
- The Dock reserves its strip with `_NET_WM_STRUT_PARTIAL`, and updates it when
  the window manager reports a new position.

## Running it

```sh
cargo run --locked -p slopos-dock
slopos-dock --pin-defaults          # write the default pins to the pin file
slopos-dock --autohide on           # save the auto-hide setting
slopos-dock --autohide off
```

The settings from the command line are saved to `~/.config/slopos-i/dock.conf`
(or `$XDG_CONFIG_HOME/slopos-i/dock.conf`), and the Dock applies them the next
time it starts. A running Dock applies the menu setting at once.

## Tests

```sh
cargo test -p slopos-dock
bash scripts/qa/dock-autohide.sh     # auto-hide under Xvfb
```

## Status and limits

- Not built: drag to reorder, pinning and unpinning from the Dock itself, other
  context-menu actions, multi-monitor placement, and a Settings panel.
