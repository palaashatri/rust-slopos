# P1 Files — live X11 visual and interaction acceptance handoff

**This file is a test plan, not evidence that QA passed.** It exists so a
less capable coding agent can work against known runnable code instead
of redesigning the feature. Start with [GOAL.md](../GOAL.md) and the
[SLOPOS Design Bible](contract/02-design.md). No test log or screenshot
may be described as PASS unless actually executed/inspected.

## What's already coded

- `apps/files/src/model.rs`: bounded, sorted, nonrecursive read-only
  listing, symlink type identification, navigation history and errors.
- `apps/files/src/main.rs`: event-driven X11 App with a dedicated bounded
  async directory-reading worker, toolbar, keyboard/pointer navigation,
  per-row accessible nodes and native `xdg-open` handoff.
- `crates/slopos-ui/src/controls/list.rs` (replaced the PR's `list_view.rs`): reusable visible-row
  rendering/selection, scroll and accessible list. Review behavior;
  do not duplicate it inside Files or other apps.
- `packaging/slopos-files.desktop`, Debian/Arch packaging and
  `install.sh`: installed entry points.
- `apps/files/src/ops.rs`, `trash.rs`, `worker.rs` (FILES-01/02): transactional
  copy/move/rename/new folder, Trash with restore, confirmed permanent
  delete, operation worker with Cancel, inotify refresh.

A scripted live run of these journeys (copy, paste, conflict Keep Both,
F2 rename, context menu, Delete to Trash, MIME open of a PNG in the Image
Viewer, Go ▸ Trash) at 1× and 2× is recorded in the ledger (FILES-02). The
manual checklist below still applies to an installed package.

## Reproducible test setup (Linux guest only)

Use one disposable isolated Linux guest under macOS or Windows; never
run the actual desktop QA against the host user session.

~~~sh
export CARGO_TARGET_DIR="$PWD/.work/p1/target"
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=0
python3 scripts/p1_goal.py preflight
cargo test -p slopos-ui -p slopos-files --locked
cargo clippy -p slopos-ui -p slopos-files --all-targets --locked -- -D warnings
mkdir -p .work/p1/evidence/files
cargo run -p slopos-files --locked -- --snapshot \
  .work/p1/evidence/files/browser-1x.png /tmp
~~~

Open the saved PNG using the IDE's actual image viewer and note font,
spacing, overflows, row selection and contrast. Re-run at
`SLOPOS_SCALE=2` in a separate output filename. Screenshot renders
without an X11 server, but **does not prove any real user interaction**.

For the interactive pass, boot the full desktop in an isolated guest and
launch `slopos-files /tmp` with visible X11 display. Capture actual
user interactions using xdotool/AT-SPI and screenshots, not synthetic
claims. Each operation needs an exit status and screenshots before/after.

## Mandatory real interaction matrix

| Case | Input | Observe |
|---|---|---|
| Empty folder | Open fresh empty temp directory | Accurate zero items, no crash, no phantom rows |
| Folders/files | Two subdirectories and files | Folders first; names not silently modified |
| Navigation | Click folder row, Open; Back; Up; Home | Current path and count update, history exact |
| Keyboard | Tab and Shift-Tab, arrow keys, Home/End, PageUp/Down, Enter | Focus and selection visible, action works |
| Pointer | Select several rows, scroll using wheel, Open | Selected row stable; scroll has no runaway polling |
| Resize/scale | 640×480, 1280×720, 1×/2× scaling | No clipped text/buttons, overlay, illegible row labels |
| Massive list | >8192 files | Honest limit error, not partial/silent/truncated success |
| Rename race | Delete or rename directory while scan starts | Error or current view preserved, no panic |
| Permission denied | Directory user cannot read | Status shows an actual error, no invented entries |
| Symlinks | Link to directory outside fixture | Does not traverse link as if it were a normal directory |
| Unicode | UTF-8 filename, accented, RTL, long name | Legible/truncated text, correct logical selection |
| Unknown file | Select regular file with no association | Missing opener/error reported rather than fake success |
| Accessibility | Inspect AT-SPI in actual guest | List role, row labels, accessible focus/action |
| Launcher | Install in disposable image, open via desktop file association | Actual binary starts, correct WM_CLASS, package includes launcher |
| Regression | Calculator, Dock, logout, notifications | No change in existing user journeys |

The X11 session and package builders require actual guest evidence. If
the model lacks computer-use or cannot open the captured screenshot,
mark visual/runtime NOT_RUN and do not integrate the affected task.

## Next coding work, in recommended order

1. Tighten list focus, row highlighting, viewport geometry and
   Accessibility/AT-SPI behavior found during real screenshots.
2. Extend the toolkit with **reusable** context menu, text input,
   file dialog, table/icon grid and paginated list APIs, with unit tests.
3. Introduce separate **transactional filesystem operations**
   behind explicit user confirmation: never destroy a destination on
   partial copy, never follow symlink escapes unexpectedly, implement
   Trash and Undo with comprehensive negative fixtures.
4. Add MIME icons, thumbnails, sort/filter, drag/drop and async
   recursive/cross-device operations only once path safety tests pass.
5. Include Files in installed-VM session smoke and verified Debian/Arch
   payload checks, then update `qa/p1-progress.json` with **real**
   proof artifacts through `scripts/p1_goal.py`.

Nothing here authorizes replacing the running desktop's default file
manager until equivalent behavior has verified evidence.
