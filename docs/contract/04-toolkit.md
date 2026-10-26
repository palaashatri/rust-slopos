<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 9. slopos-ui — first-party component library

`slopos-ui` is the foundation of every native SLOPOS interface.

Its public API must make the correct visual language easier than bypassing it.

## 9.1 UI library responsibilities

`slopos-ui` owns:

- all first-party visual tokens;
- all first-party component geometry;
- state rendering;
- input state machines;
- focus behavior;
- accessibility semantics;
- layout primitives;
- icon rendering;
- menu presentation;
- dialog presentation;
- list/table/icon views;
- common content chrome;
- deterministic test rendering.

Applications supply data, actions and domain logic.

Applications do not draw their own design language.

## 9.2 Foundation modules

Required foundation concepts:

```text
foundation/
├── geometry.rs
├── scale.rs
├── metrics.rs
├── insets.rs
├── color.rs
├── typography.rs
├── font_metrics.rs
├── patterns.rs
├── bevel.rs
├── border.rs
├── focus.rs
├── selection.rs
├── icon_metrics.rs
├── animation.rs
└── theme.rs
```

Canonical 1× geometry is the primary coordinate system.

Supported integer HiDPI scaling must preserve deterministic geometry.

Fractional scaling is not required until explicitly added to this contract.

## 9.3 Rendering

Use mature rendering/text infrastructure underneath the SLOPOS API.

The native `slopos-ui` implementation uses the SLOPOS-owned rendering, text, host and accessibility stack defined in §2.2 of the product contract: tiny-skia, cosmic-text, x11rb and AccessKit.

GTK/GDK/Pango/GIO are permitted only in the documented legacy donor crates and engine adapters. They must not be dependencies of `slopos-ui` or new first-party UI.

SLOPOS-owned components must not rely on arbitrary distro GTK theme defaults for their canonical appearance.

Pixel-oriented artwork must use pixel-preserving scaling.

Do not use bilinear filtering for reference pixel art where it produces blur.

## 9.4 Required primitive components

At minimum:

- `Surface`
- `Text`
- `Icon`
- `Image`
- `Separator`
- `Rule`
- `PatternFill`
- `Bevel`
- `FocusRing`
- `SelectionHighlight`
- `Spacer`

## 9.5 Required layout components

At minimum:

- `Row`
- `Column`
- `Grid`
- `Overlay`
- `Stack`
- `SplitPane`
- `ScrollView`
- `GroupBox`
- `Inset`
- `Align`
- `FixedCanonicalLayout` for exact reference compositions where appropriate.

Do not use arbitrary magic padding in application crates.

## 9.6 Required input/control components

At minimum:

### Buttons

- Regular Button
- Default Button
- Secondary Button
- Icon Button
- Repeat/Arrow Button where referenced

Required states:

- rest;
- hover if the platform uses hover feedback;
- pressed;
- focused;
- disabled;
- default;
- destructive only if explicitly defined by the SLOPOS language.

### Selection controls

- Checkbox
- Radio Button
- Toggle where a real use case exists
- Disclosure Triangle

Required states include:

- unchecked/unselected;
- checked/selected;
- pressed;
- focused;
- disabled;
- mixed/indeterminate where semantically required.

### Text entry

- Text Field
- Search Field
- Password Field
- Multi-line Text Area
- Numeric Field
- Editable Label where required for file rename

Text-editing implementation may reuse approved mature infrastructure, but `slopos-ui` and new first-party UI must not depend on GTK text infrastructure. Visible geometry, focus, selection, disabled state and surrounding chrome remain SLOPOS-owned.

### Choice controls

- Pop-up Button
- Combo/Choice control
- Stepper
- Slider
- segmented control only if the canonical specification explicitly requires one.

### Progress/state controls

- determinate progress;
- indeterminate progress;
- busy indicator if required;
- meter where required.

## 9.7 Required collection/view components

At minimum:

- List View;
- List Row;
- Table View;
- Table Header;
- Tree/Outline View;
- Icon View;
- File Item;
- Desktop Item;
- Empty State;
- Status Row.

Collection components must support keyboard navigation and accessible selection semantics.

## 9.8 Required scrolling components

At minimum:

- Vertical Scrollbar;
- Horizontal Scrollbar;
- Scroll Thumb;
- Scroll Arrow;
- Track;
- Corner/resize affordance where applicable.

Scrolling behavior must be consistent across all native apps.

## 9.9 Required menu components

At minimum:

- Menu Bar;
- Menu;
- Menu Item;
- Hierarchical/Submenu Item;
- Checked Menu Item;
- Radio Menu Item where needed;
- Disabled Menu Item;
- Separator;
- Shortcut/Command label;
- Application menu host;
- Context menu.

Menu state and geometry must derive from the Figma specification.

## 9.10 Required dialog/window-content components

Window-manager decorations are owned by the WM/theme integration, but native window content uses SLOPOS components.

Required:

- Alert;
- Confirmation Dialog;
- Error Dialog;
- Information Dialog;
- Progress Dialog;
- File Open Dialog;
- File Save Dialog;
- Open With Dialog;
- Properties Dialog;
- Preferences/Control Panel content;
- About/System Information content;
- utility-window layout;
- modal/transient host behavior.

## 9.11 Required navigation/content components

Where needed:

- Toolbar;
- Status Bar;
- Path/Location Bar;
- Breadcrumb/Path control if adopted by Files;
- Sidebar only where an explicit application workflow requires it and where it can be reconciled with the canonical visual language;
- Tab strip only where product requirements justify tabs;
- Inspector/Info panel;
- split-view divider.

Do not default to modern sidebars or cards merely because GTK makes them convenient.

## 9.12 Icon system

SLOPOS must define:

- canonical icon sizes;
- device icons;
- folder icons;
- file-type icons;
- action icons;
- status icons;
- application icons;
- high-DPI variants.

Assets must be original or license-compatible.

The UI library controls icon alignment and scaling.

## 9.13 Typography

The library must define shared roles:

- global menu text;
- window/title text;
- body text;
- secondary text;
- list text;
- icon-label text;
- section title;
- utility display text;
- monospace text.

Each role defines:

- family;
- size;
- weight;
- baseline;
- line height;
- scaling rule.

First-party applications may not invent unrelated font sizes.

## 9.14 Accessibility

Every public interactive component must expose:

- accessible role;
- accessible name;
- state;
- selection where applicable;
- keyboard reachability;
- logical focus order;
- disabled semantics.

Accessibility is part of component completion, not a later application patch.

## 9.15 UI Gallery

Create:

```text
tools/slopos-ui-gallery
```

The gallery must display every component and every required state without needing to launch a production application.

It is the canonical component-development environment.

The gallery must support:

- deterministic window size;
- deterministic component fixture data;
- 1× capture;
- integer HiDPI capture;
- keyboard navigation;
- pointer-state tests;
- accessibility inspection;
- stable screenshot names.

No component is complete until it appears in the gallery and its state matrix passes.

# 10. Figma-derived machine-readable design specification

Create:

```text
qa/spec/classic/
```

The exact format may be JSON, TOML or another reviewable text format.

It must include:

```text
qa/spec/classic/
├── manifest.*
├── colors.*
├── typography.*
├── metrics.*
├── patterns.*
├── icons.*
├── buttons.*
├── selection-controls.*
├── text-fields.*
├── menus.*
├── scrollbars.*
├── lists.*
├── windows.*
├── file-items.*
└── dialogs.*
```

For each Figma component classify it:

- `REQUIRED`
- `OPTIONAL_VARIANT`
- `NOT_USED`

`NOT_USED` requires a reason.

Each required component/state records as appropriate:

- Figma node/source;
- width;
- height;
- padding;
- margins;
- text baseline;
- icon box;
- border;
- bevel;
- pattern;
- colors;
- state transitions;
- hit target;
- keyboard behavior;
- accessibility semantics.

A completed release may not have required `UNKNOWN` specification entries.

# 11. slopos-ui conformance testing

For every required atom/state produce:

- specification fixture;
- rendered fixture;
- geometry assertion;
- state assertion;
- accessibility assertion;
- visual comparison where useful.

Geometry has zero tolerance unless a documented platform-specific exception exists.

Anti-aliased text may use a fixed justified raster threshold, but geometry remains exact.

Do not increase tolerance merely to pass a regression.

Goldens must never auto-update during normal CI.

Changing a golden requires an explicit reviewed specification change.

# 12. slopos-appkit — native application framework

`slopos-appkit` is the common application layer above `slopos-ui`.

It prevents every native application from separately reinventing normal desktop behavior.

## 12.1 Application lifecycle

Provide:

- stable application identity;
- single-instance policy where appropriate;
- open-file/open-URI dispatch;
- new-window behavior;
- quit lifecycle;
- session shutdown handling;
- state restoration hooks;
- clean error propagation.

## 12.2 Action/command model

All native apps use a real action model.

Actions define:

- ID;
- label;
- enabled state;
- checked/stateful value where needed;
- shortcut;
- callback;
- menu visibility.

The same action must back:

- menu entries;
- toolbar entries;
- keyboard shortcuts;
- contextual commands.

Do not duplicate action logic across UI surfaces.

## 12.3 Global menu export

Native SLOPOS apps must export their menu/action model through the shared appkit transport so `slopos-shell` displays the same real actions in the global menu bar. Follow the protocol contract in §21.4; keep GTK/GIO transport mechanics behind SLOPOS-owned APIs. Verify enabled/checked state, window-specific actions, exporter disappearance and focus switching with a real producer/consumer test.

No guessed keyboard injection is allowed.

## 12.4 Standard application services

Provide shared APIs for:

- clipboard;
- drag-and-drop;
- undo/redo;
- recent files;
- MIME/default apps;
- file open/save dialogs;
- open-with dialogs;
- error presentation;
- background jobs;
- progress/cancellation;
- notifications;
- preferences;
- persistent window geometry;
- restoration of documents/windows;
- application help;
- About integration.

## 12.5 Document model

For document-based apps define a reusable document lifecycle:

```text
new
→ edited
→ save/save-as
→ clean
→ external modification handling
→ close
```

Support:

- unsaved-change confirmation;
- autosave/recovery where appropriate;
- atomic writes;
- recent documents;
- reopen;
- failure recovery.

## 12.6 Asynchronous work

Long-running work must not block the UI event loop.

Use cancellable jobs for:

- filesystem operations;
- network operations;
- installation;
- archive operations;
- thumbnail generation;
- media metadata;
- document rendering.

Standardize progress/error/cancellation presentation.
