# Screenshot

`slopos-screenshot` takes a picture of the screen and saves it as a PNG.

## Use

- **Mode:** *Screen* captures the whole screen. *Window* captures the active
  window with its frame. *Region* captures the screen and lets you drag a
  rectangle over it; *Save selection* writes only that rectangle.
- **Delay:** no delay, 3 seconds or 10 seconds, counted down before the capture.
- **Take Screenshot** saves the file into the Pictures folder:
  `$XDG_PICTURES_DIR`, then the `XDG_PICTURES_DIR` line of `user-dirs.dirs`,
  then `~/Pictures`. Files are named `Screenshot-YYYY-MM-DD-HHMMSS.png` in UTC.
- Keyboard: Tab and Shift+Tab move focus; Space or Enter presses the focused
  control. In the region picker, Enter saves the selection and Escape cancels.

*Screen* and *Region* hide the Screenshot window while the screen is read, so
the app does not appear in its own capture. *Window* captures whichever window
is active when the countdown ends, so use a delay to make that window active.
If Screenshot itself is the active window, the capture is refused with a message.

Recording is not available in this build. The window says so.

## Command line

    slopos-screenshot --capture-screen /path/to/file.png

Writes the whole screen to the file as PNG and exits, without opening a window.

## Limits

- Captures read the X server's pixels. Only 24-bit colour (depth 24 or 32, with
  32 bits per pixel) is supported; other visuals return an error.
- Timestamps in file names are UTC, not local time.

## Code

- `src/capture.rs`: the only X11 module (`Backend` trait and `X11Backend`).
- `src/geometry.rs`, `src/model.rs`, `src/paths.rs`: plain logic, tested without a display.
- `src/main.rs`: the toolkit window, the state machine and its tests, which use a fake backend.
