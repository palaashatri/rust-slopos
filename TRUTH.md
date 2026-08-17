# TRUTH.md — SLOPOS-I Factual Audit & Readiness Ledger

**Audit date:** 2026-08-17  
**Branch:** `pivot`  
**Current code-evidence anchor:** `f206c5a69f1f17303e1cf87f003d8853f69b4ec7`  
**Hosted evidence:** CI run [#32032380198](https://github.com/palaashatri/rust-slopos/actions/runs/32032380198), attempt 2, successful  
**Current evidence-backed readiness:** **78/100**

SLOPOS-I is a verified X11/Openbox desktop-environment development milestone, not a production-ready KDE/GNOME-class desktop. The score remains intentionally unchanged by the latest UI/UX tranche: keyboard menu access, native GTK global-menu support, icon/theme work and accessibility fixes improve the product, but they do not close the independent visual, current installed-system, physical-hardware, service-mutation or performance evidence gaps required for additional readiness credit.

## Scorecard

| Domain | Weight | Score | Current evidence |
|---|---:|---:|---|
| System 7 / Platinum visual identity | 20 | **12/20** | Platinum and Graphite shell/theme states, SLOPOS icon assets, framed shell surfaces and Settings/Catalogue styling are implemented and screenshot-tested. Independent canonical visual acceptance remains below contract. |
| Desktop shell / interaction | 15 | **11/15** | Top bar, Application Strip, Search, notifications, live status, native GTK global menus and keyboard-reachable SLOPOS menu are implemented. `Ctrl+F2` is owned by the shell through an X11 passive grab rather than by Openbox. |
| X11 window-management integration | 10 | **9/10** | Openbox session ownership, margins, four desktops, focus switching, restart recovery and retained-resolution geometry are exercised in Xvfb. Physical high-refresh and exhaustive hardware coverage remain open. |
| Upstream application integration | 10 | **9/10** | Existing Arch QA launches representative upstream applications, Chromium/Firefox and SuperTux. Current GTK GtkApplication windows can export their actual menu hierarchy into the SLOPOS top bar. Physical GPU/audio and independent chrome review remain open. |
| Software Catalogue / AppImage | 8 | **8/8** | Curated AppImage metadata and payload handling remain fail-closed with HTTPS, trusted digest and ELF checks. |
| System services integration | 8 | **6/8** | Settings delegates to real system utilities and reports unavailable services honestly. Hardware-backed mutation, suspend and Bluetooth behavior remain unverified. |
| Installer / boot / session | 8 | **7/8** | X11 session/package/ISO contracts are covered, but current-head installed-to-disk/EFI evidence is still missing. Historical installed-VM evidence is not silently promoted to current-head proof. |
| Functional QA | 7 | **7/7** | Exact code head passes locked build/test/lint, release, X11 contract, Xvfb/Openbox smoke, Settings, AT-SPI, Orca, translated locales, long-run and resolution jobs. |
| Visual regression QA | 5 | **3/5** | Automated screenshots exist across shell states and resolutions. The most recent independent retained-set review remains below the required per-scene/average acceptance threshold; the newest UI tranche has not been independently rescored. |
| Performance | 3 | **2/3** | Bounded X11 startup/RSS and long-run liveness checks pass. True cold-cache and physical-target budgets remain open. |
| Accessibility / localization | 3 | **2/3** | Named AT-SPI surfaces, UTF-8 Search input, reversible focus, `fr_FR.UTF-8`, `de_DE.UTF-8`, Orca speech evidence and keyboard system-menu access pass hosted QA. Physical accessibility review remains open. |
| Recovery / resilience | 3 | **2/3** | Shell/Openbox supervision and restart QA are covered in disposable sessions; hardware/installed-system recovery remains open. |
| **Total** | **100** | **78/100** | **Verified development milestone; not production-ready.** |

## Verified UI/UX state

The current shell no longer fabricates generic File/Edit/View/Window/Help commands for arbitrary focused applications. For GTK GtkApplication exporters, `slopos-shell` reads `_GTK_UNIQUE_BUS_NAME`, `_GTK_MENUBAR_OBJECT_PATH` / `_GTK_APP_MENU_OBJECT_PATH`, `_GTK_APPLICATION_OBJECT_PATH` and `_GTK_WINDOW_OBJECT_PATH` from the focused X11 window. It then uses GIO `DBusMenuModel` and `DBusActionGroup` proxies to render the application's real `GMenuModel` and route `app.*` / `win.*` actions back to the owning application. Applications that do not expose a compatible exporter retain their normal local menu; SLOPOS does not invent commands for them.

The classic SLOPOS system menu is keyboard reachable. `Ctrl+F2` is registered directly by the shell with an X11 passive key grab, including common lock-modifier variants. The keyboard path opens the real GTK system menu, waits until the popup is mapped, selects the first item, and is acceptance-tested by activating `About SLOPOS-I` using Return without pointer input. This path passes default AT-SPI, `fr_FR.UTF-8`, `de_DE.UTF-8` and Orca-backed runs.

The current visual/theme tranche also includes the SLOPOS-Platinum freedesktop icon theme, SLOPOS-native top-bar status icons, Platinum light appearance, Graphite dark appearance, persistent appearance switching, Openbox/GTK/shell theme propagation, a compact Control Panels Settings layout, shipping Settings delegates and Graphite-aware Settings/Catalogue presentation.

## Exact-head CI statement

CI run `32032380198` targets code commit `f206c5a69f1f17303e1cf87f003d8853f69b4ec7`. Build/test/clippy/rustfmt, release build, X11-only contract, default AT-SPI, translated AT-SPI (`fr_FR.UTF-8`, `de_DE.UTF-8`), Orca, Settings delegation, bounded long-run QA and all retained-resolution matrix jobs passed.

The Xvfb/Openbox visual-smoke job failed once while waiting for `xdotool --onlyvisible` to observe the top bar even though diagnostics showed the shell/Openbox processes and the top-bar/application-strip X11 windows. The same exact commit was rerun without source changes and the visual-smoke job passed. The final workflow conclusion for attempt 2 is `success`; the initial transient failure is retained here rather than hidden.

Manual workflow-dispatch jobs are not treated as passing evidence when they were skipped by the normal push run.

## Current release blockers

1. **Independent visual acceptance is still open.** Automated screenshots and source-level polish are not substitutes for the contract's independent review threshold. The newest keyboard/global-menu/icon/theme tranche has not earned extra visual points merely by compiling or rendering.
2. **Current installed-system evidence is still open.** Existing installed-VM/ISO evidence predates the current code anchor. A new installed-to-disk run must prove the current package/session/runtime tree rather than borrowing historical artifacts.
3. **Physical hardware coverage is still open.** High-refresh timing, GPU behavior, physical multi-display behavior, audio, suspend/resume, Bluetooth, network mutation and accessibility need real hardware evidence.
4. **Non-GTK global-menu coverage is intentionally bounded.** Native GTK GtkApplication exporters are supported through GIO. Applications without the relevant GTK exporter properties keep local menus; SLOPOS does not claim universal cross-toolkit global-menu compatibility.
5. **Visual convergence of upstream applications remains open.** SLOPOS icon/theme integration improves GTK applications, but PCManFM, terminals, browsers and other upstream software still require independent review against the intended classic-Mac desktop language.
6. **Performance evidence is synthetic/bounded.** Hosted Xvfb startup, RSS and long-run checks are useful regressions, not proof of physical cold-cache or high-resolution/high-refresh performance.
7. **Recovery evidence is disposable-session evidence.** Current child restart checks do not replace installed-system recovery, login-manager failure, filesystem-full or interrupted-upgrade testing.

## Acceptance accounting rules

- A passing build is not visual acceptance.
- Xvfb screenshot generation is not visual acceptance.
- A UI control receives functional credit only when it changes real state, invokes a real supported service, or clearly reports that the service is unavailable.
- A global menu receives credit only when commands originate from the owning application exporter and activation is routed back to that application; fabricated proxy commands are forbidden.
- Historical VM/ISO artifacts are not current-head evidence.
- Manual jobs that were skipped are not counted as passing tests.
- `100/100` requires an installable release candidate plus independent functional, visual, accessibility, hardware and performance evidence with no release-blocking defects.

## Next evidence tranche

The next score-changing tranche should target evidence rather than another nominal completion claim: rebuild/install the current head in a clean VM, retain exact-source boot/session artifacts, perform fresh independent screenshot review of Platinum and Graphite canonical scenes, and run hardware-backed display/audio/network/power/accessibility checks. Until those gates pass, the truthful readiness remains **78/100**.
