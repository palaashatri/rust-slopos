# SLOPOS UI Gallery

Development gallery for the SLOPOS toolkit (`crates/slopos-ui`). Each
component opens in its own window through the toolkit's X11 host; no GTK.

```sh
cargo run -p slopos-ui-gallery -- --component surface
cargo run -p slopos-ui-gallery -- --component button-plate --scale 2
cargo run -p slopos-ui-gallery -- --component buttons
# Headless capture, no X server needed:
cargo run -p slopos-ui-gallery -- --component buttons --scale 2 --snapshot buttons.png
```

| Component      | Size (logical) | Shows                                                        |
|----------------|----------------|--------------------------------------------------------------|
| `surface`      | 160×90         | A black rectangle: the raster/present path only.             |
| `button-plate` | 320×136        | Plate geometry: rows Regular/Default/Secondary, columns Rest/Pressed/Disabled. |
| `buttons`      | 320×120        | Live `Button` controls with labels, focus ring and a disabled button. |

`--scale` accepts 1 to 4 and defaults to `SLOPOS_SCALE`/`GDK_SCALE`, else 1.
Escape or the window's close action exits.
