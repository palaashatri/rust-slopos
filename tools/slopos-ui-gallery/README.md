# SLOPOS UI Gallery

Initial surface-rendering fixture, not a complete component gallery or a
reference-conformance pass. It uses the SLOPOS UI boundary rather than building
GTK presentation in this crate.

Run inside the established Linux VM's X11 session:

```sh
GDK_SCALE=1 cargo run --locked -p slopos-ui-gallery -- --scale 1
GDK_SCALE=2 cargo run --locked -p slopos-ui-gallery -- --scale 2
GDK_SCALE=1 cargo run --locked -p slopos-ui-gallery -- --component button-plate --scale 1
GDK_SCALE=2 cargo run --locked -p slopos-ui-gallery -- --component button-plate --scale 2
```

The button-plate view is a 320×136 logical-pixel matrix: rows are Regular,
Default and Secondary; columns are Rest, Pressed and Disabled. It draws only
plate geometry, not text or interactive controls. Both fixtures support scales
1 and 2. Escape or the window close action exits. Window-manager decorations
are outside the client capture region. Remaining components, interaction
matrices and accessibility/conformance gates are still pending.

Guest execution evidence and task ownership live in [`docs/CONTRACT.md`](../../docs/CONTRACT.md).
