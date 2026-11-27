# slopos-core

Shared, platform-independent building blocks used by the SLOPOS programs. It
has no window, no X11 and no GTK. It is a library.

## What it provides

- **XDG paths** (`src/xdg.rs`): data, config, runtime and cache directories, and
  the SLOPOS-specific paths under them.
- **Desktop entries** (`src/desktop_entry.rs`): finds and parses `.desktop` files
  in the XDG application directories.
- **MIME defaults** (`src/mime.rs`): reads `mimeapps.list` and the desktop
  entries' `MimeType` values, so Files and the desktop open files with the right
  application.
- **Process launch** (`src/process.rs`): `launch` detaches a program, resolves
  its path against the candidate lists, and reaps it. SLOPOS programs start other
  programs through this function (AGENTS.md rule 5).
- **Wallpaper** (`src/wallpaper.rs`): the saved picture and the bundled patterns.
- **Capabilities, IPC and identity** (`src/capability.rs`, `src/ipc.rs`,
  `src/identity.rs`): small typed values shared between programs.
- **Errors** (`src/error.rs`): the common error type.

## Using it

```toml
[dependencies]
slopos-core = { path = "../../crates/slopos-core" }
```

## Tests

```sh
cargo test -p slopos-core
```

These are unit tests for the parsers, path rules and launch resolution. They do
not need a display.
