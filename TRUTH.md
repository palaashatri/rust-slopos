# TRUTH.md — SLOPOS-I Factual Audit & Readiness Ledger

**Audit date:** 2026-08-12
**Branch:** `pivot`
**Audit scope:** remediation working tree after the X11 packaging, catalogue, shell and QA tranche
**Current evidence-backed readiness:** **65/100**

The previous `100/100` claim was invalid. This audit credits the fixes that have objective source, test and Docker/Xvfb evidence, while withholding credit for VM/boot, hardware, independent visual, performance, accessibility, localization and recovery evidence that has not been produced.

## Scorecard

| Domain | Weight | Score | Evidence-based status |
|---|---:|---:|---|
| System 7 / Platinum visual identity | 20 | **12/20** | Fresh 1280x800 captures now show the slate desktop, Platinum bar, framed Application Strip and coherent shell windows. The visual gate is still below the contract because icon fallback, upstream-window leakage and several canonical states lack independent review. |
| Desktop shell / interaction | 15 | **10/15** | Top bar, live focus/time/status updates, singleton Search, keyboard navigation, explicit unavailable menu feedback and local notifications are covered by source/tests and Xvfb interaction. Multi-resolution and full accessibility interaction are unverified. |
| X11 window-management integration | 10 | **7/10** | Openbox, X11 margins, four desktops, launcher hotkey and visible window geometry pass the Docker smoke path. No installed VM or multi-resolution runtime evidence is available. |
| Upstream application integration | 10 | **7/10** | PCManFM and Xfce Terminal launch in the Docker scene; Settings and Catalogue windows are visible and tied to their launched PIDs. Broader upstream-app behavior and theming remain unreviewed. |
| Software Catalogue / AppImage | 8 | **7/8** | Metadata now requires complete control-free fields, HTTPS host URLs, trusted SHA-256 and a valid ELF payload; redirects are HTTPS-only and bounded. Kdenlive 24.05.0, Inkscape 1.3.2, GIMP 3.2.4 and Audacity 3.7.7 now have pinned official assets and trusted digests; OBS remains browse-only because its official release has no Linux AppImage. |
| System services integration | 8 | **4/8** | Settings is an honest upstream-tool hub and shell status reads available NetworkManager/PipeWire/battery state. Hardware mutation, suspend, Bluetooth and absence/error behavior are not VM-tested. |
| Installer / boot / session | 8 | **5/8** | Arch/Debian/ISO/root manifests now contain only the four binaries and required assets; the root installer supports standard `/usr/share/xsessions` discovery plus custom-prefix resource forwarding. No real install, DM login, ISO boot or VM evidence is recorded. |
| Functional QA | 7 | **6/7** | The fresh Docker/Xvfb gate passes release build, 12 catalogue tests, 5 shell unit tests, 11 shell integration tests, 3 spaces tests, launcher singleton behavior, visible window/PID checks and five non-empty 1280x800 captures. VM functional evidence is still absent. |
| Visual regression QA | 5 | **3/5** | Canonical desktop, application, multi-window, Catalogue and Settings scenes are captured and manually inspected at 1280x800. The contract's independent 95/100-per-scene visual review and additional resolutions are not complete. |
| Performance | 3 | **1/3** | Lightweight architecture is plausible; no current reproducible benchmark ledger supports full credit. |
| Accessibility / localization | 3 | **1/3** | Keyboard basics exist; no complete AT-SPI, focus-order, scaling, locale or international-input acceptance evidence. |
| Recovery / resilience | 3 | **2/3** | The Docker gate kills and observes replacement Openbox and shell processes, proving the session supervisor's bounded child-restart path. Broader failure injection and recovery-script validation remain open. |
| **Total** | **100** | **65/100** | **Verified development milestone; not production-ready.** |

## Current release-blocking findings

1. **False completion accounting:** `TRUTH.md` awarded 100/100 despite visibly unfinished canonical screenshots and unverified claims.
2. **Catalogue availability:** the OBS entry remains intentionally browse-only because its official release does not provide a Linux AppImage; fabricating a third-party binary or hash would violate the fail-closed contract.
3. **Release evidence:** no bootable ISO, installed display-manager login, or VM screenshot has been captured in this tranche.
4. **Visual acceptance:** only 1280x800 Docker scenes have been reviewed; the independent 95/100 scene gate, 1366x768/1920x1080/HiDPI scenes and notification/modal states remain open.
5. **System and quality evidence:** hardware-backed service mutation, AT-SPI/focus-order/scaling/localization checks, reproducible performance measurements and failure-injection recovery runs remain open.
6. **Packaging policy:** the root installer now defaults the session descriptor to `/usr/share/xsessions`; an actual privileged install and display-manager discovery run is still required.

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

## Verified remediation tranche

The 2026-08-11 deep audit initiates a corrective tranche that:

- makes catalogue metadata, redirects and AppImage payload validation fail closed;
- converts Settings to an honest upstream-tool hub and reports unavailable global commands;
- keeps the existing shell instance for the launcher hotkey;
- removes stale Wayland/compositor assumptions from shipping manifests, CI and notices;
- adds current X11 package/asset/prefix contract tests;
- bounds the packaged identity mark and restores the canonical slate desktop after Openbox starts;
- strengthens Docker/VM QA with fresh visible windows, launched-PID checks and mandatory non-empty screenshots;
- adds the SVG loader dependency required for packaged Platinum icons.

## Evidence ledger

- `docker run --rm -v <repo>:/workspace -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh` completed all eight stages, including shell/Openbox child-restart checks, with `SLOPOS-I Docker/Xvfb functional evidence PASS` and exit 0 in a fresh Ubuntu 24.04 snapshot.
- In a fresh Rust 1.97 Linux container, `cargo metadata --locked`, `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings` all exited 0. The test run reports 12 catalogue, 5 shell unit, 11 shell integration and 3 spaces tests, with 0 failures.
- The official Audacity 3.7.7 release asset was downloaded from `https://github.com/audacity/audacity/releases/download/Audacity-3.7.7/audacity-linux-3.7.7-x64-22.04.AppImage`; its local SHA-256 was `45c4445fb6670cc5fe40d31c7cea979724d2605bca53b554c32520acbf901ef0`, matching the release-published checksum.
- The official KDE archive serves `https://download.kde.org/Attic/stable/kdenlive/24.05/linux/kdenlive-24.05.0-x86_64.AppImage` and publishes SHA-256 `b2ea1c3cc5af7eda58c5a19bfd35cde9a050fb70c5f2526117c9cc69a46576f0` in its mirrorlist.
- The official GIMP v3.2 archive serves `https://download.gimp.org/gimp/v3.2/linux/GIMP-3.2.4-x86_64.AppImage` and publishes SHA-256 `f1ce6dc671ef1c4aad87a0db9d7462e8ca9c0b5f899456337803c6ba32d0babe` in `SHA256SUMS`; the official Inkscape 1.3.2 media asset was downloaded as 125,195,456 bytes and independently rehashed to `351deaea3fa391c56e0c6401dadcf83f7c9c8f82faa47bdb07024e99b92f9b5c`.
- A disposable Ubuntu smoke install of `scripts/install-session-files.sh --prefix /workspace/artifacts/qa/prefix-test --session-dir /workspace/artifacts/qa/xsessions-test` produced an X11 descriptor whose `Exec` and `TryExec` both point to the prefix's `bin/slopos-session`.
- `cargo fmt --all -- --check` passes in the Rust 1.97 container after the remediation formatting pass.
- `git diff --check` and `bash -n` for the changed installer, session, Docker, ISO and VM scripts pass.
- The Docker gate intentionally prints that visual acceptance remains a separate human/vision review gate; this ledger does not turn generated screenshots into a 100/100 claim.
