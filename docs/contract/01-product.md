<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 1. Product mission

SLOPOS-I is a first-party Linux desktop environment designed to be viable for normal daily use and engineered to the quality level expected of mature environments such as GNOME and KDE while retaining its own deliberately compact Classic-Macintosh-inspired interaction and visual language.

Build one coherent desktop platform: shared design specification and UI components, reusable application services, real platform adapters, native applications and an integrated shell/session. The dependency graph in §6 is authoritative; providers and UI foundations can be developed independently.

The system must feel intentional from login to shutdown:

- session startup and recovery;
- desktop composition;
- windows, focus and workspaces;
- a first-party Dock;
- a global application menu bar;
- menu-bar system applets;
- non-focus-stealing on-screen displays for volume, brightness and related hardware state;
- application launching and search;
- file browsing and removable media;
- network, Wi-Fi, VPN, Bluetooth, audio, brightness, displays and power;
- settings and defaults;
- clipboard and drag-and-drop;
- notifications;
- screenshots and screen recording;
- file dialogs and application choosers;
- authentication and session dialogs;
- accessibility;
- first-party utilities;
- normal third-party X11 application compatibility;
- packaging, installation, upgrade, removal and recovery.

SLOPOS is a desktop environment, not merely a window manager plus themed applications. The Dock, menu bar, applets, OSD layer, notification service, global shortcuts, system providers and native daily-use applications are first-class platform components.

The product must never contain an enabled control that merely looks functional. Preserve the existing MIT license, COPYRIGHT attribution and third-party notices.

# 2. Non-negotiable architectural principle

## 2.1 The UI library comes first

The first architectural dependency of every first-party SLOPOS surface is the SLOPOS UI component library. Backend and QA work may proceed independently under §21; application UI may consume only the verified atoms it needs, and missing atoms must be completed in the library.

The project must create:

```text
crates/slopos-ui
```

before attempting to finish or visually polish the native application suite.

Every visible first-party component is designed as a SLOPOS component.

First-party applications must not independently invent:

- buttons;
- checkboxes;
- radio controls;
- text fields;
- menus;
- list rows;
- scrollbars;
- dialogs;
- alerts;
- toolbars;
- status bars;
- icon grids;
- file items;
- spacing;
- typography;
- focus treatment;
- selection treatment;
- window-content chrome.

Applications consume the component system; they do not define it.

## 2.2 No raw-GTK first-party UI architecture

GTK3/GDK/Pango/GIO may remain only in the legacy donor crates (`slopos-shell`, `slopos-settings`, `slopos-catalogue`) and in engine adapters. `slopos-ui` does not use them.

They are not the first-party design system.

First-party apps depend on `slopos-appkit`/`slopos-ui`; those SLOPOS abstractions own presentation over the SLOPOS rendering, text, host and accessibility stack (tiny-skia, cosmic-text, x11rb and AccessKit), not over GTK. Follow the explicit “depends on” graph in §6.3.

A first-party application crate must not directly build its user interface from arbitrary `gtk::Button`, `gtk::Entry`, `gtk::Dialog`, `gtk::ListBox`, CSS fragments, or ad-hoc layout constants.

Direct GTK/GDK/Pango UI dependencies are permitted only in explicitly designated boundary crates:

- legacy donor crates under replacement;
- low-level engine adapters where required;
- compatibility adapters whose API is hidden behind SLOPOS-owned abstractions.

CI must enforce this architectural boundary.

When a native app needs a missing control, stop the dependent UI slice and request a UI task from the coordinator. The assigned UI owner adds it to `slopos-ui` and verifies conformance before app integration resumes. Continue independent app model/test work within the existing packet; do not edit another owner's component or bypass it.

## 2.3 "Designed from scratch" definition

"Designed from scratch" means SLOPOS owns:

- geometry;
- measurement;
- padding;
- visual hierarchy;
- colors;
- state appearance;
- interaction state machines;
- focus behavior;
- accessible semantics;
- keyboard behavior;
- hit testing;
- drawing composition;
- public widget API;
- conformance tests.

It does **not** require reimplementing invisible mature infrastructure such as:

- Unicode shaping;
- text rasterization;
- input-method protocols;
- AT-SPI transport;
- terminal escape parsing;
- PDF rendering engines;
- video codecs;
- TLS;
- filesystem syscalls.

SLOPOS owns the user-facing component. Mature engines may remain underneath it.

# 3. Scope and non-goals

## 3.1 In scope

SLOPOS-I is:

- Linux-only;
- X11-only for this generation;
- Openbox-based unless an evidence-backed limitation requires another solution;
- implemented primarily in Rust;
- built around the first-party `slopos-ui` component library and `slopos-appkit`;
- allowed to use GTK3/GDK/Pango/GIO only in legacy donor crates and engine adapters beneath the SLOPOS component system;
- allowed to use mature engines such as VTE, WebKitGTK, Poppler, libarchive and libmpv where appropriate;
- allowed to delegate system ownership to NetworkManager, PipeWire/WirePlumber or PulseAudio-compatible APIs, BlueZ, UPower, systemd/logind, udev, CUPS/IPP, UDisks2/GIO and XRandR;
- required to provide a coherent first-party shell including Dock, global menu bar, status applets, OSDs, launcher, notifications, desktop surface and session UI.

## 3.2 Out of scope

Do not introduce:

- a Wayland session or fallback in SLOPOS-I;
- a custom display server merely for ownership;
- a custom kernel;
- a speculative future SLOPOS generation;
- a general-purpose GUI toolkit unrelated to SLOPOS;
- the retired legacy Application Strip;
- a Dock copied from modern macOS or any other vendor;
- Aqua / modern macOS visual language;
- GNOME/libadwaita card-heavy design;
- KDE Breeze design;
- Windows Fluent design;
- touch-first spacing as the default desktop density;
- fake compatibility;
- fake state;
- fake application actions.

A proper first-party **SLOPOS Dock is required**. The former "no dock" rule applied to the retired ad-hoc Application Strip and is superseded by this contract.

Wayland work remains paused until the X11 product is mature.

# 4. Source-of-truth precedence

Normative precedence is: current explicit user direction (subject to higher-priority environment rules), this contract, the approved machine-readable design specification, approved clean-room assets and component contracts, then implementation conventions. Resolve a genuine product conflict with the user; the coordinator resolves ordinary implementation choices.

Evidence is not beneath prose in this hierarchy. A requirement cannot overrule a failing test, missing component or measured runtime behavior. Part II describes facts separately from Part I's target. Static presence is not runtime verification.

Existing `qa/reference/` files predate the atomic reset and are historical composition references. They cannot override extracted Figma geometry or serve as current conformance goldens. Example measurements in §5 are not a complete or freshly re-extracted specification.

No old numeric readiness score is authoritative. Scoped task verification and product readiness are different claims.
