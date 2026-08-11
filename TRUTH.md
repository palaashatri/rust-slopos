# TRUTH.md — SLOPOS-I Factual Audit & Readiness Ledger

**Audit date:** 2026-08-11  
**Branch:** `pivot`  
**Pre-remediation audited commit:** `78578e6e5c3fb709f8f12c4154973f7d2e7a8c5a`  
**Current evidence-backed readiness:** **40/100**

The previous `100/100` claim was invalid. A Docker/Xvfb script completing and producing screenshots does not establish production readiness or visual fidelity. The canonical screenshots supplied on 2026-08-11 visibly fail the System 7 / Platinum product-quality bar, and code review found several functional and security defects that invalidate full-credit claims.

## Scorecard

| Domain | Weight | Score | Evidence-based status |
|---|---:|---:|---|
| System 7 / Platinum visual identity | 20 | **8/20** | Basic gray palette, Openbox theme and shell framing exist, but the desktop still reads as a GTK/Openbox prototype. Icon language, title chrome, notifications, settings/catalogue composition and control vocabulary are inconsistent. |
| Desktop shell / interaction | 15 | **8/15** | Top bar, launcher, application strip and local notification UI exist. Active-app integration/status state and global menu semantics are incomplete; several menu actions were inert or misleading. |
| X11 window-management integration | 10 | **6/10** | Openbox stacking foundation and X11 session path exist. Hotkey/session integration and geometry handling need hardening and real multi-resolution validation. |
| Upstream application integration | 10 | **6/10** | PCManFM/Xfce Terminal/etc. are used in QA, but theming and behavioral integration remain visibly inconsistent. |
| Software Catalogue / AppImage | 8 | **2/8** | Browse UI exists, but audited catalogue metadata used placeholder empty-file hashes and installer code could create a fake shell-script “AppImage” after download failure. Production installation is not accepted until it fails closed with verified metadata. |
| System services integration | 8 | **2/8** | Settings UI launches some upstream tools, but audited controls for display, audio, power, appearance and input were partly hard-coded or unwired. |
| Installer / boot / session | 8 | **2/8** | X11 session launcher exists, but the audited root installer still referenced the removed compositor, removed first-party apps and Wayland session paths. |
| Functional QA | 7 | **2/7** | Xvfb screenshot harness exists but asserted very little and tolerated screenshot failures. Old shell tests reference deleted compositor-era APIs. |
| Visual regression QA | 5 | **1/5** | Canonical scenes are captured, but no independent visual acceptance gate justified the previous full score. |
| Performance | 3 | **1/3** | Lightweight architecture is plausible; no current reproducible benchmark ledger supports full credit. |
| Accessibility / localization | 3 | **1/3** | Keyboard basics exist; no complete AT-SPI, focus-order, scaling, locale or international-input acceptance evidence. |
| Recovery / resilience | 3 | **1/3** | Recovery script exists, but failure-injection evidence is incomplete. |
| **Total** | **100** | **40/100** | **Prototype / integration alpha. Not production-ready.** |

## Release-blocking findings from the 2026-08-11 audit

1. **False completion accounting:** `TRUTH.md` awarded 100/100 despite visibly unfinished canonical screenshots and unverified claims.
2. **Catalogue fail-open behavior:** download failure created and installed an executable shell-script stub instead of failing; checksum verification was bypassed for the placeholder SHA-256 used by every catalogue entry.
3. **Decorative Settings controls:** several UI controls displayed values but did not apply the selected value or change the corresponding system state.
4. **Broken launcher hotkey contract:** Openbox executed `slopos-shell --toggle-launcher`, while the shell had no such command mode; this can start a second shell instead of toggling the existing launcher.
5. **Misleading global menu:** multiple menu items had no action or launched unrelated generic commands.
6. **Stale installer:** `install.sh` still required `slopos-compositor`, removed first-party binaries, Wayland session directories and Wayland/X11 dual-stack language.
7. **Stale runtime dependency manifests:** distro manifests still installed Wayland, DRM/GBM, XWayland and Wayland terminal/clipboard packages instead of the X11/Openbox product stack.
8. **Stale CI:** compositor/Wayland contract jobs remained after the X11 pivot.
9. **Stale tests:** shell integration tests referenced removed `slopos_bus`, workspace-manager and compositor-era APIs.
10. **Visual-system drift:** `themes/platinum/*.toml`, GTK CSS, Openbox theme and screenshots used different colors, metrics and font assumptions; typography referenced Apple SF fonts that are not redistributable project assets.

## Canonical visual findings

The 2026-08-11 screenshots are retained as evidence of the baseline, not proof of acceptance. Major visible defects include:

- generic symbolic icons mixed with small custom bitmap icons;
- application strip reading as a toolbar rather than a coherent classic launcher;
- notification card with clipped/overlong text and weak classic-Mac composition;
- GTK HeaderBar/CSD presentation in Catalogue and Settings conflicting with Openbox/System 7 chrome;
- Settings page dominated by empty space and modern notebook treatment;
- file manager/terminal retaining substantial upstream visual identity leakage;
- weak active/inactive hierarchy and limited classic frame vocabulary;
- missing evidence for menus, launcher, dialog states, disabled states, check/radio/slider/scrollbar states and HiDPI scenes.

## Acceptance accounting rules

- A passing build is not visual acceptance.
- Xvfb screenshot generation is not visual acceptance.
- A control receives functional credit only if the selected value is applied to real system state or the control honestly delegates to an upstream settings utility.
- AppImage installation receives credit only for HTTPS download + mandatory trusted digest/signature validation + atomic placement + desktop integration. No synthetic fallback payloads are allowed.
- Visual credit requires review of real screenshots at required resolutions, not source inspection.
- No domain receives full credit while its canonical scene or functional scenario has a known release-blocking defect.
- `100/100` requires a bootable/installable release candidate plus independent functional and visual evidence.

## Current remediation tranche

The 2026-08-11 deep audit initiates a corrective tranche that:

- makes the catalogue installer fail closed;
- converts Settings from fake controls to an honest upstream-tool hub;
- fixes the existing-shell launcher hotkey path;
- removes stale Wayland/compositor installer and CI assumptions;
- replaces obsolete shell tests with current X11 contract tests;
- consolidates the Platinum palette/typography around redistributable fonts;
- reworks shell geometry, menu actions, notifications, launcher and Application Strip styling;
- strengthens Docker/Xvfb QA so evidence capture failures fail the run.

**Score remains 40/100 until the remediation commit is built and its new screenshots/flows are independently reviewed.**
