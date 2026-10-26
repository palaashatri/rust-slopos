<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 14. SLOPOS shell

`slopos-shell` is a first-party consumer of the same design system, not a separate visual universe.

It must consume `slopos-ui` and typed provider/applet APIs.

The shell owns:

- global top menu bar;
- application-menu host;
- menu-bar status area;
- first-party Dock;
- desktop surface integration;
- desktop objects;
- launcher/search;
- notification service/presentation;
- OSD manager;
- workspace UI;
- session actions;
- global keyboard shortcuts;
- shell-level media-key routing.

The shell must not create duplicate versions of controls already implemented in `slopos-ui`.

## 14.1 Global menu bar

The menu bar has two regions:

| Region | Content/authority |
|---|---|
| Application region | Focused application's real App/File/Edit/View/Window/Help actions, where exported |
| System/status region | Provider-backed Wi-Fi, Bluetooth, volume, brightness, battery and clock applets |

The left region is backed by the focused application's real action/menu model.

There must be exactly one application-menu bridge.

Native SLOPOS applications export their real `slopos-appkit` action model.

Third-party applications may be integrated only when they expose a supported real menu/action protocol.

If a third-party app does not export one:

- leave its local menu intact; or
- expose only guaranteed shell/window actions.

Do not fabricate Cut/Copy/Paste/Select All by blindly injecting shortcuts.

The right region hosts system applets backed by shared typed providers.

## 14.2 First-party Dock

The Dock is a required shell component and replaces the retired Application Strip.

Required behavior:

- pinned applications;
- running applications;
- active/focused application state;
- multiple-window indication;
- launch;
- raise/focus existing window;
- minimize where the interaction design specifies;
- application/window context menus;
- drag-to-reorder;
- pin/unpin;
- attention state;
- badges/progress only where a real source exists;
- workspace awareness;
- multi-monitor policy;
- configurable auto-hide, dodge and always-visible behavior;
- keyboard accessibility;
- drag/drop where meaningful;
- correct work-area reservation only when visible policy requires it.

The Dock must use `slopos-ui` and a dedicated SLOPOS Dock specification. It must not copy modern macOS Dock appearance or behavior blindly.

Control Panels must expose a Dock panel for at least:

- visibility mode;
- position if multiple positions are supported;
- size/density where supported;
- animation/reveal timing if configurable;
- running-app indicators;
- pinned application management;
- monitor policy where supported.

## 14.3 System applet framework

System applets are reusable projections of `slopos-services` state into the menu bar.

Required initial applets:

- Wi-Fi / Network;
- VPN where NetworkManager exposes it;
- Bluetooth;
- Volume / microphone state;
- Brightness;
- Battery / Power;
- Displays;
- Keyboard layout/input state where applicable;
- Media playback;
- Removable media;
- Notifications / Do Not Disturb where implemented;
- Clock / Date.

Every applet must support a consistent contract equivalent to:

~~~text
identity
current icon/state
accessible label/tooltip
summary state
popup/menu model
real actions
deep link to the relevant Control Panel
availability/unavailable state
~~~

No applet may shell out independently to implement state already owned by a provider.

## 14.4 On-screen display manager

SLOPOS requires a shell-level OSD manager.

Initial OSD events include:

- volume up/down;
- mute/unmute;
- microphone mute;
- display brightness;
- keyboard backlight where available;
- Caps Lock / Num Lock where useful;
- touchpad enable/disable where supported;
- airplane/network mode where supported;
- display-switching feedback where supported.

OSDs must:

- never steal keyboard focus from the active application;
- never appear in Alt+Tab;
- never appear in normal task lists;
- preserve the active X11 window/focus;
- work over fullscreen applications where the X11/compositor stack permits;
- coalesce repeated key presses into one updating overlay;
- dismiss automatically;
- be monitor-aware;
- be accessible without behaving like a normal focus-taking window;
- use actual provider state after the requested change;
- be click-through unless an OSD is intentionally interactive.

Implementation must explicitly test `_NET_ACTIVE_WINDOW`/focus preservation around OSD mapping. Do not accept an OSD implementation that causes a fullscreen game/video/application to lose focus.

## 14.5 Desktop

The desktop supports:

- desktop objects;
- selection;
- keyboard navigation;
- drag/drop;
- Trash;
- mounted volumes where exposed;
- context menus;
- wallpaper;
- multi-monitor placement.

PCManFM may remain temporarily during migration but is not the final source of first-party desktop UI if it prevents conformance.

## 14.6 Launcher/search

The launcher must support:

- application discovery from desktop entries;
- keyboard-first invocation;
- incremental search;
- launch;
- focus/raise existing applications where policy calls for it;
- recent or suggested items only when backed by real data;
- accessibility;
- no fake search categories.

Search providers may expand later, but application launch must remain fast and deterministic.

## 14.7 Notifications

Implement the freedesktop notification protocol truthfully.

Support:

- stable notification IDs;
- replacement/update;
- expiry;
- close;
- application attribution;
- actions only where correctly supported;
- monitor-aware placement;
- Do Not Disturb when implemented;
- notification history only if persistence and privacy behavior are explicitly designed.

## 14.8 Global shortcuts and media keys

One shell-level shortcut service owns global hotkeys.

It routes:

- launcher;
- screenshots;
- volume;
- brightness;
- media playback;
- workspace switching;
- window switching;
- lock/session shortcuts;
- user-configurable shortcuts.

Do not scatter global shortcut registration across unrelated applications.

## 14.9 Shell isolation and restartability

Dock, applets, OSD, notifications and launcher may initially share one executable, but must have clear module/state boundaries.

A failure in one shell surface must not corrupt provider state.

Long-term process separation is permitted when it materially improves reliability.
