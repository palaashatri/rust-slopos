<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 15. First-party application suite

A mature SLOPOS desktop needs a coherent set of native applications. The applications below are the canonical first-party product inventory.

Applications are divided into release-critical core applications and extended daily-use applications.

Every first-party application must use `slopos-ui` through `slopos-appkit` unless explicitly documented as a lower-level system utility.

# 16. Release-critical core applications

These applications are required for a first-class SLOPOS desktop release.

## 16.1 SLOPOS Files

Target path:

```text
apps/files
```

Purpose: first-party file management and desktop filesystem interaction.

Required functionality:

- directory navigation;
- back/forward/up;
- location/path navigation;
- icon view;
- list view if included;
- keyboard navigation;
- single/multiple selection;
- open;
- default-application/MIME launch;
- rename;
- new folder;
- create supported basic files where exposed;
- copy;
- cut/move;
- paste;
- duplicate if exposed;
- Trash;
- restore from Trash if exposed;
- empty Trash;
- permanent-delete confirmation;
- file/folder properties;
- context menus;
- drag-and-drop;
- hidden files;
- filesystem change watching;
- error handling;
- removable volumes;
- mount/unmount/eject where supported;
- thumbnailing where appropriate;
- network mounts if exposed.

Suggested mature backends:

- GIO/GVfs;
- freedesktop MIME database;
- udisks/GIO volume APIs.

PCManFM is a temporary migration dependency only if it cannot meet the final visual/interaction contract.

## 16.2 SLOPOS Control Panels

Target path:

```text
apps/control-panels
```

This replaces the current first-party Settings UI architecture.

Required panels for production daily use:

### Desktop and appearance
- Appearance
- Desktop / Wallpaper
- Dock
- Menu Bar / Status Items
- Fonts
- Notifications
- Default Applications

### Hardware
- Displays
- Brightness where hardware supports it
- Sound
- Keyboard
- Mouse
- Touchpad where present
- Bluetooth
- Printers

### Connectivity
- Network
- Wi-Fi
- Wired
- VPN integration through NetworkManager where available
- Proxy if supported by platform policy

### System
- Power
- Date & Time
- Region & Language
- Users
- Storage
- Removable Media
- Autostart / Session
- Keyboard Shortcuts
- Accessibility
- About / System Information entry point

Every panel reads real provider state.

No sample SSIDs, IPs, device names, volume values or hardware state may appear in production.

## 16.3 SLOPOS Terminal

Target path:

```text
apps/terminal
```

Use a mature terminal engine such as VTE.

SLOPOS owns:

- window;
- menus;
- tabs if supported;
- preferences;
- keyboard shortcut model;
- appearance;
- dialogs;
- search UI;
- profile UI.

Required:

- shell launch;
- terminal resize;
- scrollback;
- copy/paste;
- selection;
- search;
- Unicode;
- IME compatibility;
- links;
- working-directory launch;
- command exit behavior;
- accessibility;
- tabs only if completed coherently.

Do not write a terminal emulator parser from scratch.

## 16.4 SLOPOS Notes

Target path:

```text
apps/notes
```

Purpose: lightweight first-party writing and text-note application.

Required:

- new/open;
- save/save-as;
- plain text;
- Unicode;
- undo/redo;
- cut/copy/paste;
- find;
- basic formatting only if explicitly supported;
- modified-document warning;
- autosave/recovery if adopted;
- recent documents;
- drag/drop file opening;
- print only when genuinely implemented.

It may later support Markdown, but unsupported formatting controls must not be exposed.

## 16.5 SLOPOS Calculator

Target path:

```text
apps/calculator
```

Calculator is both a real utility and an early conformance proving ground.

Required:

- keyboard and pointer input;
- basic arithmetic;
- decimal;
- sign;
- clear;
- backspace;
- operator precedence policy documented/tested;
- divide-by-zero/error state;
- copy result;
- predictable focus.

Its UI must be composed entirely from `slopos-ui`.

## 16.6 SLOPOS Software

Target path:

```text
apps/software
```

This evolves from `slopos-catalogue`.

It manages SLOPOS-curated application delivery according to the release policy.

Required:

- catalogue search;
- application detail;
- install;
- integrity verification;
- launch;
- uninstall;
- version display;
- error reporting;
- background/cancellable downloads;
- progress;
- architecture validation.

Initial delivery policy: evolve the existing curated AppImage catalogue. Installed user applications and desktop entries live in user-owned XDG locations; downloads require an explicitly trusted source, integrity verification, architecture checks, atomic installation, cancellation cleanup and uninstall that preserves unrelated user data. A checksum alone does not establish publisher trust. Do not invent a public SLOPOS package repository.

Distribution package management remains owned by the base distribution unless the product contract explicitly expands scope. Native distro-package management is not silently part of the AppImage store task.

## 16.7 SLOPOS System Monitor

Target path:

~~~text
apps/system-monitor
~~~

This is SLOPOS's full task-manager/system-observation application.

Required views:

- Processes;
- CPU;
- Memory;
- Storage activity where practical;
- Network activity where practical;
- GPU activity where a stable provider exists.

Process view must expose at least:

- name;
- PID;
- CPU usage;
- memory usage;
- state;
- user;
- command/details on inspection.

Required actions:

- search/filter;
- inspect process;
- end/terminate process;
- force kill with explicit confirmation;
- process priority adjustment only when correctly authorized;
- refresh without UI stalls.

No process action may silently claim success. Read back process state.

Use procfs/sysfs and stable system APIs.



## 16.8 SLOPOS Screenshot & Recorder

Target path:

~~~text
apps/screenshot
~~~

Screenshot is a first-party shell-integrated capture utility.

Required screenshot modes:

- entire screen;
- selected monitor where useful;
- selected window;
- selected region.

Required output paths:

- save;
- clipboard;
- open captured image in SLOPOS Image Viewer.

Required behavior:

- configurable delay;
- multi-monitor awareness;
- predictable filenames;
- visible success/error feedback;
- global keyboard shortcuts owned by the shortcut service;
- non-destructive cancellation.

Recording is enabled only when functional.

Target recording modes:

- entire screen;
- selected monitor;
- selected window where technically reliable;
- selected region;
- optional microphone;
- optional system audio.

Use a mature capture/encoding stack such as PipeWire, FFmpeg or GStreamer as appropriate for X11. Do not implement codecs.

Do not ship a Record control until the selected recording path passes real VM tests. The required “open captured image in SLOPOS Image Viewer” path also makes APP-08's minimum viewer slice a capture-release dependency; an unimplemented handoff is not a completed action.



## 16.9 SLOPOS System Information / About

Target path:

```text
apps/system-information
```

Required:

- SLOPOS version;
- source/build revision where appropriate;
- OS;
- kernel;
- architecture;
- CPU;
- memory;
- storage summary;
- graphics/display information where available;
- X11/session information;
- license/credits.

Every displayed value must be real.

This app also provides the canonical compact About composition required for visual QA.

# 17. Extended first-party daily-use applications

These are part of the target SLOPOS application suite. They may be implemented after the release-critical core is stable, but they must follow the same architecture.

## 17.1 SLOPOS Image Viewer

Target:

```text
apps/image-viewer
```

Required:

- common image formats through mature loaders;
- next/previous;
- zoom;
- fit;
- actual pixels;
- rotate where supported;
- fullscreen;
- file metadata;
- copy;
- set as wallpaper integration where appropriate.

## 17.2 SLOPOS Archive Utility

Target:

```text
apps/archive-utility
```

Use mature engines such as `libarchive`.

Required:

- inspect common archive formats;
- extract;
- create supported archive formats;
- drag/drop integration;
- overwrite/conflict handling;
- path traversal protection;
- progress/cancel;
- error reporting.

Files should invoke it transparently for archive operations.

## 17.3 SLOPOS Disks

Target:

```text
apps/disks
```

Purpose: storage inspection and safe user-level disk operations.

Use mature storage APIs such as UDisks2.

Required scope may include:

- device/partition information;
- mount/unmount;
- eject;
- SMART information where safely available;
- filesystem usage;
- formatting only when properly authorized and guarded by explicit destructive confirmations.

Destructive functionality must not be enabled until thoroughly tested.

## 17.4 SLOPOS Fonts

Target:

```text
apps/fonts
```

Required:

- browse installed fonts;
- preview;
- metadata;
- install user font;
- remove user-installed font;
- refresh font cache;
- clear distinction between system-owned and user-owned fonts.

## 17.5 SLOPOS Help

Target:

```text
apps/help
```

Required:

- offline-first SLOPOS help;
- searchable topics;
- keyboard navigation;
- deep links from applications/settings;
- release-appropriate troubleshooting;
- no stale generated feature claims.

## 17.6 SLOPOS Media

Target:

```text
apps/media
```

Use a mature media engine such as libmpv or GStreamer.

SLOPOS owns:

- menus;
- transport controls;
- playlist;
- open dialogs;
- subtitles/audio track UI;
- fullscreen UI;
- preferences.

Do not implement codecs from scratch.

## 17.7 SLOPOS Documents

Target:

```text
apps/documents
```

Purpose: PDF/document viewing.

Use a mature rendering engine such as Poppler for PDF.

Required:

- open;
- page navigation;
- zoom;
- search;
- thumbnails if implemented;
- print only if fully integrated;
- metadata;
- keyboard navigation.

## 17.8 SLOPOS Browser — optional ecosystem application

A first-party browser shell may be built only around a maintained secure engine such as WebKitGTK.

Do not build a browser engine.

Until this application exists and meets security/update expectations, SLOPOS must integrate established browsers such as Firefox/Chromium cleanly.

A browser is not allowed to delay core desktop correctness.

# 18. System utilities and background infrastructure

A production desktop depends on many components that users do not launch as ordinary applications. These are first-class requirements, not afterthoughts.

## 18.1 Authentication agent

Target:

~~~text
utilities/polkit-agent
~~~

Provide a SLOPOS-native Polkit authentication UI backed by the real Polkit agent API.

Required:

- requesting application/action identity;
- password prompt;
- cancellation;
- error state;
- correct secure entry;
- no credential logging.

## 18.2 Lock screen

Target:

~~~text
utilities/lock-screen
~~~

SLOPOS may initially use a mature external locker if necessary.

A first-party lock screen becomes acceptable only when:

- PAM/authentication path is correct;
- secure keyboard handling is reviewed;
- failure cannot expose the session;
- multi-monitor behavior is correct;
- suspend/resume behavior is correct.

Security takes priority over ownership. A working, tested locker integration is mandatory for daily-use release readiness even when a mature external locker supplies it; an unavailable Lock item is an honest alpha limitation, not a production pass.

## 18.3 File chooser

Native SLOPOS apps use a SLOPOS file chooser implemented through `slopos-appkit` and `slopos-ui`.

It must support:

- open;
- save;
- folder selection;
- filters;
- recent locations;
- filename validation;
- overwrite confirmation;
- keyboard operation.

Third-party GTK apps may continue using GTK dialogs unless a safe toolkit integration mechanism exists.

## 18.4 Open With / Default Apps

Provide:

- application chooser;
- default association selection;
- MIME-aware candidate list;
- persistent default update;
- one-time open without changing default.

## 18.5 Global shortcut service

One service owns shell-level shortcuts and media keys.

It must provide conflict detection, configured bindings, provider dispatch and real feedback.

## 18.6 Clipboard manager

Provide base X11 clipboard/selection correctness first.

Persistent clipboard history is optional until explicitly designed with:

- privacy policy;
- exclusion rules;
- secret/sensitive content behavior;
- storage lifecycle.

## 18.7 Removable-media handler

Respond to real media events and integrate with:

- Files;
- notifications;
- menu-bar applet;
- safe unmount/eject;
- configured autorun/open behavior.

## 18.8 Wallpaper manager

Own:

- configured wallpaper;
- per-monitor policy where supported;
- static wallpaper application;
- live wallpaper only when explicitly implemented and performance-tested.

## 18.9 Display-change confirmation

Risky display changes must offer an automatic rollback countdown so an unusable mode does not strand the user.

## 18.10 Network-secret UI

Network authentication/secrets must use NetworkManager's supported secret-agent mechanism or another appropriate stable API.

Do not log secrets.

## 18.11 Bluetooth pairing UI

Pairing requests must present:

- device identity;
- PIN/passkey/confirmation state;
- cancel/failure;
- trusted/connected result.

## 18.12 Printer authentication UI

Where CUPS/IPP requires credentials or authorization, present a truthful native prompt with no credential logging.

## 18.13 Low-battery and hardware-state notifications

Power/provider events may trigger:

- low battery;
- critical battery;
- device removal;
- network loss;
- other actionable hardware state.

Rate-limit repetitive notifications.

## 18.14 URI opener and desktop-entry launcher

Provide one safe launch path for:

- URIs;
- MIME/default applications;
- desktop entries;
- files.

Avoid every application inventing process-spawn rules.

## 18.15 Session dialogs

Provide SLOPOS-native:

- Log Out;
- Restart;
- Shut Down;
- Sleep confirmation where appropriate.

Actions must use real logind/session provider capability checks.

## 18.16 Crash/error presentation

Unexpected first-party application failures should produce useful logs and recoverable user-facing errors where practical.

Do not silently restart forever or hide repeated failure.

## 18.17 Infrastructure inventory rule

When a later requirement is remembered, first classify it as one of:

- provider;
- applet;
- shell surface;
- Control Panel;
- native application;
- background utility.

Add it through the appropriate stable interface instead of putting miscellaneous code into `slopos-shell`.
