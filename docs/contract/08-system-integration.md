<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 22. System providers and truthful state

An enabled control must execute the behavior it advertises.

A stateful control must display real state.

Providers are the single owners of system state. Menu-bar applets, OSDs and Control Panels observe the same provider rather than implementing separate backends.

## 22.1 Network

Use NetworkManager APIs/D-Bus.

Support real:

- Ethernet adapters;
- Wi-Fi adapters;
- Wi-Fi enabled state;
- AP enumeration;
- SSID;
- signal;
- security;
- active connection;
- IP information;
- connect/disconnect;
- errors;
- VPN entries where supported.

No synthetic production networks.

## 22.2 Audio

Use the active PipeWire/WirePlumber/PulseAudio-compatible APIs.

Support:

- outputs;
- default output;
- volume;
- mute;
- inputs;
- default input;
- microphone level;
- microphone mute;
- device hotplug.

Read back changes.

The Volume applet, volume OSD, Sound Control Panel and media keys all use this provider.

## 22.3 Brightness

Provide one brightness abstraction over the correct platform facility, such as backlight sysfs/logind-compatible mechanisms, with explicit capability detection.

Support:

- current brightness;
- min/max or normalized range;
- increment/decrement;
- direct set where supported;
- read-back;
- unavailable state.

The Brightness applet, brightness OSD, Display/Power settings and brightness keys all use this provider.

Do not fabricate brightness support on displays without a controllable backlight.

## 22.4 Bluetooth

Use BlueZ D-Bus.

Support:

- adapter availability;
- power;
- discovery;
- device list;
- pairing;
- connect/disconnect;
- trusted/paired state where exposed.

The applet and Control Panel share this provider.

## 22.5 Power

Use UPower/logind.

Support:

- battery status;
- charge;
- power source;
- suspend capability;
- lid behavior where supported;
- critical/low-battery events;
- power profiles only when provider support is real.

## 22.6 Media

Provide a shared media-session abstraction over supported mechanisms such as MPRIS.

Expose:

- current player;
- title/artist where available;
- play/pause;
- next/previous;
- playback state.

The menu-bar media applet and media keys use this provider.

## 22.7 Date/time

Prefer `org.freedesktop.timedate1`.

Support:

- current timezone;
- current NTP state;
- real date/time;
- authorized changes;
- permission errors.

The menu-bar clock and Control Panel use shared time/locale state where applicable.

## 22.8 Displays

Use XRandR/X11 state.

Support:

- real outputs;
- modes;
- current mode;
- layout;
- primary display;
- rotation where supported;
- apply;
- automatic rollback/confirmation for risky changes;
- hotplug.

## 22.9 Printers

Use CUPS/IPP.

Support:

- printer discovery;
- queue status;
- default printer;
- jobs;
- add/remove only when correctly authorized.

## 22.10 Removable media

Use GIO/UDisks2 as appropriate.

Support:

- mount;
- unmount;
- eject;
- errors;
- volume labels;
- safe removal.

The Files app, removable-media applet and notifications share this state.

## 22.11 Input and hardware keys

Provide a single capability/routing layer for:

- media keys;
- brightness keys;
- keyboard backlight;
- Caps/Num state where surfaced;
- touchpad toggles where supported;
- layout/input-source state.

Global shortcut handling must not be duplicated across apps.

# 23. Window management and X11

Openbox provides ICCCM/EWMH window management.

SLOPOS integration must support:

- overlapping windows;
- predictable focus;
- drag and resize;
- minimize;
- maximize with accessible title-bar controls and correct menu/Dock work area;
- restore to the previous usable geometry;
- fullscreen as a distinct state from maximize;
- transient/modal relationships;
- Alt+Tab;
- workspaces;
- top-bar reserved area;
- multi-monitor placement;
- dynamic monitor changes;
- integer HiDPI.

If Openbox prevents a required behavior:

1. reproduce the limitation;
2. document it;
3. test alternatives;
4. change architecture only with evidence.

Never lower the UI specification merely to match Openbox limitations.

# 24. Multi-monitor and scaling

Maintain an explicit monitor model.

Define which monitor owns:

- primary menu bar;
- launcher;
- notifications;
- desktop objects;
- new first-party dialogs.

Respond to RandR topology changes without restarting the session.

Normal panels and the Dock must yield to fullscreen applications according to policy. Transient hardware OSDs may overlay fullscreen only as specified in §14.4, without focus theft.

Canonical geometry is 1×.

Support integer scaling predictably.

# 25. Accessibility

Production quality requires accessibility parity as a core feature.

At minimum:

- AT-SPI names/roles/states;
- keyboard-only navigation;
- logical focus order;
- visible focus;
- screen-reader-compatible labels;
- accessible collection selection;
- disabled-state semantics;
- high-contrast mode only if fully specified;
- scalable text policy without layout corruption.

Accessibility must be tested in the UI library and again in application workflows.

# 26. Application interoperability

SLOPOS must remain a good Linux citizen.

Support:

- `.desktop` entries;
- XDG MIME applications;
- default applications;
- XDG user directories;
- XDG autostart;
- clipboard;
- drag/drop;
- notifications;
- X11 ICCCM/EWMH;
- standard D-Bus services;
- portals only where useful on X11 and supported.

Third-party apps must remain usable even when they cannot adopt SLOPOS global menus or styling.
