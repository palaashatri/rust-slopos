# SLOPOS shell, GTK 3 (`slopos-shell`), legacy

The first desktop shell: a GTK 3 top bar, launcher and notification code. It
is **superseded** by [`slopos-topbar`](../slopos-topbar/README.md), which uses
the SLOPOS toolkit.

## Status

- **Not started by the session.** `slopos-session` starts `slopos-topbar`.
- **Not installed.** `install.sh` and the live ISO do not install this binary.
- **Still built** in the workspace, so its tests keep running until the
  remaining parts are moved or removed.
- **No new work here.** AGENTS.md (rule 2) allows only bug fixes to the GTK 3
  crates. New UI goes into `slopos-topbar` and the SLOPOS toolkit.

## What is in the crate

- `src/topbar.rs`, `src/menu/`, `src/launcher.rs`, `src/app_finder.rs`,
  `src/app_index.rs`, `src/desktop_files.rs`: the GTK 3 top bar, its menus,
  and the application search.
- `src/notifications.rs`, `src/services/`: notifications and the service
  adapters for the GTK bar.
- `src/x11/`, `src/shortcuts.rs`, `src/theme/`: X11 integration, key shortcuts and
  the GTK theme.

Screenshots are not provided: this program is not shipped, and the top bar
that replaces it has its own (see the top bar's README).

## Tests

```sh
cargo test -p slopos-shell
```
