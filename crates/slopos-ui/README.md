# slopos-ui: the SLOPOS toolkit

The UI toolkit that every native SLOPOS program is built on. It is written in
Rust and owned by SLOPOS. It does not use GTK. It draws with tiny-skia, shapes
text with cosmic-text, rasterises SVG with resvg, and talks to X11 with x11rb.
Accessibility goes through AccessKit.

The toolkit is also meant for applications outside SLOPOS. Its public API uses
toolkit types, and X11 is confined to the `platform` module.

## What it provides

| Module | Contents |
|---|---|
| `foundation`, `layout` | Integer geometry (`Rect`, `Scale`), colours, insets and layout helpers |
| `render` | `RasterSurface`: a software pixmap at an integer scale, with fills, strokes, images, text, a clip stack and PNG output |
| `scene`, `scene_raster` | Renderer-neutral command recording and a software adapter |
| `animation` | Deterministic timelines with easing and reduced-motion handling |
| `text` | Fonts, measurement, caret placement and hit testing |
| `theme` | Semantic colour roles (`Palette`). Only the provisional Platinum values exist |
| `accessibility` | Builds an app's AccessKit window tree, with focus fallback |
| `image` | PNG decoding, SVG rasterising, scaling, and background composition |
| `icons` | Freedesktop icon-theme lookup, defaulting to `SLOPOS-Platinum` |
| `primitives` | Classic component painters, such as the button plate |
| `controls` | Interactive controls as state machines, each with keyboard, pointer and accessibility support |
| `event` | Pointer, keyboard, scroll, focus, timer, wake, clipboard and accessibility events |
| `platform` | The X11 host: the event loop, windows (normal, dock, desktop, popup), global menus and AT-SPI |

Controls include `Button`, `TextField` (with a secret mode), `ListView`,
`Menu` and `MenuBar`, `Dialog`, `FileChooser`, `PopUpButton`, `Slider`,
`Checkbox`, `ProgressBar`, `ScrollState` and `FocusChain`.

## Writing an app

An app implements `App::event`, `App::paint` and, optionally,
`App::accessibility`, then calls `platform::run`. The Calculator
([`apps/calculator`](../../apps/calculator/src/main.rs)) is the reference example.

A control that is missing goes into this crate, with a state machine, tests and
an accessibility node. Apps do not build their own private versions.

## Tools

`tools/slopos-ui-gallery` shows the controls in one window, and it can write
snapshots for review.

## Tests

```sh
cargo test -p slopos-ui
```

Controls are tested by sending events and checking the state and the
accessibility tree. Painting is tested by checking pixels in the software
renderer.

## Status and limits

- Not built yet: drag and drop, input methods (XIM and IBus pre-edit), tree and
  icon-grid views, a GPU renderer, and a full retained scene graph.
- Only the provisional Platinum colour set exists. Classic, Graphite and OLED
  colour tokens are not extracted yet.
