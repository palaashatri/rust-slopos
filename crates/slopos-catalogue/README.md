# SLOPOS Application Catalogue, GTK 3 (`slopos-catalogue`), legacy

The first AppImage catalogue window, built with GTK 3. It is **superseded** by
[`apps/software`](../../apps/software/README.md), which uses the SLOPOS toolkit
and adds the signed catalogue and the transactional installer.

## Status

- **Not launched and not installed.** Nothing starts this program. The session
  and the live ISO do not install it.
- **Still built** in the workspace, so its tests keep running until they are moved
  or removed.
- **No new work here** (AGENTS.md rule 2). New software features go into
  `apps/software`.

## What is in the crate

- `src/model.rs`: the catalogue entries.
- `src/installer.rs`: the AppImage installer.
- `src/main.rs`: the GTK 3 window.

No screenshots are provided, because the program is not shipped. See the
Software README for the screenshots of its replacement.

## Tests

```sh
cargo test -p slopos-catalogue
```
