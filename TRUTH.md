# TRUTH.md — SLOPOS-I Factual Audit & Readiness Ledger

**Audit date:** 2026-08-13
**Branch:** `pivot`
**Audit scope:** remediation working tree after the X11 packaging, catalogue, shell and QA tranche
**Current evidence-backed readiness:** **78/100**

The previous `100/100` claim was invalid. This audit credits the fixes that have objective source, test, Docker/Xvfb, installed-VM and live-ISO/QEMU evidence, while withholding credit for hardware, independent visual, performance, accessibility and localization evidence that has not been produced.

## Scorecard

| Domain | Weight | Score | Evidence-based status |
|---|---:|---:|---|
| System 7 / Platinum visual identity | 20 | **12/20** | Fresh 1280x800 captures now show the slate desktop, Platinum bar, framed Application Strip and coherent shell windows. The visual gate is still below the contract because icon fallback, upstream-window leakage and several canonical states lack independent review. |
| Desktop shell / interaction | 15 | **12/15** | Top bar, live focus/time/status updates, singleton Search, keyboard navigation, explicit unavailable menu feedback and local notifications are covered by source/tests and Xvfb interaction. Target-dependent global commands now disable when no focused application/helper exists; topbar/dock geometry passes 1366x768 and 1920x1080 checks. Full accessibility interaction remains unverified. |
| X11 window-management integration | 10 | **9/10** | Openbox, X11 margins, four desktops, launcher hotkey and visible window geometry pass the Docker smoke path; disposable Xvfb checks also pass at 1366x768 and 1920x1080. A fresh installed-to-disk VirtualBox guest now boots Xorg/Openbox and passes the in-guest shell/window QA at 1280x800; exhaustive drag/resize, Alt+Tab and hardware-display coverage remain open. |
| Upstream application integration | 10 | **9/10** | The Arch/X11 matrix launches five distinct upstream roles—PCManFM, Xfce Terminal, Mousepad, Ristretto and Chromium—and captures fresh 1280x800 scenes. The current gate also launches SuperTux on a packaged world1 level, sends movement/jump input while the live window is verified, observes its PulseAudio sink input and exits through the game's own input path. Chromium inherits the SLOPOS GTK/XDG identity and optional Platinum frame theme without a fork; Firefox has an explicit, backed-up profile stylesheet path. Hardware audio and full upstream-window visual convergence remain unreviewed. |
| Software Catalogue / AppImage | 8 | **8/8** | Metadata now requires complete control-free fields, HTTPS host URLs, trusted SHA-256 and a valid ELF payload; redirects are HTTPS-only and bounded. The four curated entries (Kdenlive 24.05.0, Inkscape 1.3.2, GIMP 3.2.4 and Audacity 3.7.7) are installable only with pinned official assets/digests; OBS is omitted because its official release has no Linux AppImage. |
| System services integration | 8 | **4/8** | Settings is an honest upstream-tool hub and shell status reads available NetworkManager/PipeWire/battery state. Hardware mutation, suspend, Bluetooth and absence/error behavior are not VM-tested. |
| Installer / boot / session | 8 | **8/8** | Arch/Debian/ISO/root manifests contain only the four binaries and required assets; disposable Ubuntu smokes validate custom-prefix and privileged default-prefix installs. The rebuilt Arch ISO contains UID-1000 `slopos`, executable session binaries, primary LightDM autologin/session keys, and a 1280x800 QEMU boot reaches the SLOPOS top bar and launcher after LightDM. The installed-to-disk VM also verifies the on-disk session handoff; EFI diversity and installer recovery remain open. |
| Functional QA | 7 | **7/7** | The current Rust gate passes metadata, fmt, check, 13 catalogue tests, 5 shell unit tests, 19 shell integration tests and 3 spaces tests with zero failures; Docker/Xvfb passes all eight stages with recovery and mandatory screenshots. A separate Docker/Xvfb click-through downloaded Kdenlive, verified its 204,903,616-byte payload hash and desktop entry. The QEMU ISO guest reaches the live SLOPOS session at 1280x800, and the installed-to-disk guest passes its in-guest QA; hardware behavior remains open. |
| Visual regression QA | 5 | **3/5** | Canonical desktop, application, multi-window, Catalogue and Settings scenes are captured and manually inspected at 1280x800. The contract's independent 95/100-per-scene visual review and additional resolutions are not complete. |
| Performance | 3 | **2/3** | `scripts/benchmark-x11-session.sh` records reproducible Xvfb session startup and process-tree RSS: 556 ms / 114,472 KiB at 1280x800 and 555 ms / 116,072 KiB at 1920x1080 in disposable Ubuntu containers. Long-run, cold-cache and hardware-target budgets remain unverified. |
| Accessibility / localization | 3 | **1/3** | Keyboard basics exist; no complete AT-SPI, focus-order, scaling, locale or international-input acceptance evidence. |
| Recovery / resilience | 3 | **3/3** | The Docker gate proves one restart of each child; a separate disposable run completed three consecutive shell and Openbox kill/respawn cycles with fresh PIDs, visible shell windows and the supervisor still alive. Hardware/VM recovery remains outside this score. |
| **Total** | **100** | **78/100** | **Verified development milestone; not production-ready.** |

## Current release-blocking findings

1. **False completion accounting:** `TRUTH.md` awarded 100/100 despite visibly unfinished canonical screenshots and unverified claims.
2. **Release evidence:** a 1,974,534,144-byte Arch ISO now builds reproducibly, was booted through LightDM in QEMU, and was installed to a fresh 40 GiB VirtualBox disk. The installed guest reported the pinned source SHA and passed `qa-vm.sh` at 1280x800; physical hardware, EFI diversity and installer recovery paths remain open.
3. **Visual acceptance:** all eleven contract scene types now have fresh 1280x800 captures and have been manually inspected; the independent 95/100 scene gate and retained 1366x768/1920x1080/HiDPI visual scenes remain open.
4. **System and quality evidence:** hardware-backed service mutation, AT-SPI/focus-order/scaling/localization checks and long-run/cold-cache performance budgets remain open; bounded child-failure recovery is now evidenced.
5. **Packaging policy:** privileged default-prefix/custom-prefix installs verify the X11 descriptor and absolute `Exec`/`TryExec` paths; the installed VirtualBox guest also verifies the on-disk four-binary session handoff. EFI diversity and installer recovery remain open.
6. **Browser/game boundary:** the Arch gate now passes a deterministic local browser DOM/visible-title fixture and a real SuperTux `world1/frosted_fields.stl` level with movement/jump input, live-window checks and a clean process exit. A browser can only inherit the SLOPOS GTK/frame language through upstream extension/profile mechanisms. Physical game audio, GPU behavior and complete browser-content styling still require hardware/independent review.

## Canonical visual findings

The 2026-08-12 Docker/Xvfb screenshots are retained as fresh evidence of the baseline, not proof of acceptance. Major visible defects and open review areas include:

- Settings still uses generic symbolic panel icons while the Catalogue now uses the packaged Platinum software mark consistently;
- application strip reading as a toolbar rather than a coherent classic launcher;
- notification card with clipped/overlong text and weak classic-Mac composition;
- GTK HeaderBar/CSD presentation in Catalogue and Settings conflicting with Openbox/System 7 chrome;
- Settings page dominated by empty space and modern notebook treatment;
- file manager/terminal retaining substantial upstream visual identity leakage;
- weak active/inactive hierarchy and limited classic frame vocabulary;
- missing independent review of the now-captured menus, launcher, notification/dialog states, disabled states, check/radio/slider/scrollbar states and HiDPI scenes.

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
- fixes CRLF-safe Arch ISO package-list normalization and adds the missing Arch GTK/X11 build libraries;
- replaces Catalogue missing-icon placeholders with a packaged Platinum fallback mark;
- bounds the packaged identity mark and restores the canonical slate desktop after Openbox starts;
- strengthens Docker/VM QA with fresh visible windows, launched-PID checks and mandatory non-empty screenshots;
- adds the SVG loader dependency required for packaged Platinum icons.

## Evidence ledger

- `docker run --rm -v <repo>:/workspace -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh` completed all eight stages, including shell/Openbox child-restart checks, with `SLOPOS-I Docker/Xvfb functional evidence PASS` and exit 0 in a fresh Ubuntu 24.04 snapshot.
- In a fresh Rust 1.97 Linux container, `cargo metadata --locked`, `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings` all exited 0. The test run reports 13 catalogue, 5 shell unit, 19 shell integration and 3 spaces tests, with 0 failures.
- The official Audacity 3.7.7 release asset was downloaded from `https://github.com/audacity/audacity/releases/download/Audacity-3.7.7/audacity-linux-3.7.7-x64-22.04.AppImage`; its local SHA-256 was `45c4445fb6670cc5fe40d31c7cea979724d2605bca53b554c32520acbf901ef0`, matching the release-published checksum.
- The official KDE archive serves `https://download.kde.org/Attic/stable/kdenlive/24.05/linux/kdenlive-24.05.0-x86_64.AppImage` and publishes SHA-256 `b2ea1c3cc5af7eda58c5a19bfd35cde9a050fb70c5f2526117c9cc69a46576f0` in its mirrorlist.
- The official GIMP v3.2 archive serves `https://download.gimp.org/gimp/v3.2/linux/GIMP-3.2.4-x86_64.AppImage` and publishes SHA-256 `f1ce6dc671ef1c4aad87a0db9d7462e8ca9c0b5f899456337803c6ba32d0babe` in `SHA256SUMS`; the official Inkscape 1.3.2 media asset was downloaded as 125,195,456 bytes and independently rehashed to `351deaea3fa391c56e0c6401dadcf83f7c9c8f82faa47bdb07024e99b92f9b5c`.
- OBS is not seeded because the official 30.1.2 release does not provide a Linux AppImage; no third-party substitute is used.
- A disposable Ubuntu/Xvfb click-through installed Kdenlive from the Catalogue: the resulting executable was 204,903,616 bytes, its SHA-256 matched `b2ea1c3cc5af7eda58c5a19bfd35cde9a050fb70c5f2526117c9cc69a46576f0`, and `slopos-appimage-kdenlive.desktop` was created.
- A disposable Ubuntu full-install smoke with `XSESSION_DIR` and `--prefix` installed all four binaries, `start-slopos-i`, the X11 descriptor, Openbox/GTK assets, mimeapps and the SLOPOS logo; descriptor `Exec`/`TryExec` both resolved to the custom prefix (`INSTALL_LAYOUT_STATUS_0`, exit 0).
- A disposable Ubuntu 24.04 container ran the default-prefix installer as root with `--no-deps --no-build --distro ubuntu`; all five installed executables were present, `/usr/share/xsessions/slopos-i.desktop` was non-empty, its `Exec` and `TryExec` both resolved to `/usr/local/bin/slopos-session`, and no Wayland session descriptor was installed (`INSTALL_DEFAULT_PREFIX_STATUS_0`).
- An Arch Linux `archiso` container built the live ISO after installing the declared Arch GTK/X11 build libraries; `mkarchiso` completed all six stages and produced `artifacts/iso/archlinux-2026.08.12-x86_64.iso` (1,974,534,144 bytes, SHA-256 `41c69ad74422b9e2dca69e7bb658da8f051d25e80b5bbc30d0de41c03feb10f0`, `ISO_BUILD=0`). This earlier media-build record is retained for provenance; the later ISO hash and installed-VM evidence below are the current release evidence.
- The fresh Catalogue scene now renders the original Platinum software mark for all four verified rows instead of GTK missing-icon placeholders; the source contract and full Docker/Xvfb gate pass after the change.
- Disposable Xvfb geometry checks passed at 1366x768 and 1920x1080: the topbar reported full screen width, the Application Strip remained centered, and `scrot`/ImageMagick reported exact matching screenshot dimensions.
- A separate disposable Xvfb session completed three consecutive shell and Openbox kill/respawn cycles; each cycle produced a fresh PID and visible topbar/dock, and the supervisor remained alive (`RECOVERY_REPEAT_STATUS_0`, exit 0).
- `scripts/benchmark-x11-session.sh` produced `BENCHMARK_STATUS_0` twice: 556 ms startup / 114,472 KiB RSS at 1280x800 and 555 ms / 116,072 KiB at 1920x1080.
- A disposable Ubuntu smoke install of `scripts/install-session-files.sh --prefix /workspace/artifacts/qa/prefix-test --session-dir /workspace/artifacts/qa/xsessions-test` produced an X11 descriptor whose `Exec` and `TryExec` both point to the prefix's `bin/slopos-session`.
- The current Arch ISO build completed with `mkarchiso` in a privileged Arch container. Its SHA-256 is `318ffc19ee0289c0e9c7d279ee34113546ff926065ef0d7685384e37aa6afa22`; extracted squashfs checks show UID-1000 `slopos`, mode-755 SLOPOS session binaries, primary LightDM autologin/session keys and no leftover customization hook. QEMU booted that ISO through LightDM into the SLOPOS top bar and launcher; the retained 1280x800 screenshot is `artifacts/qa/iso-boot-final-360s.png` (PNG IHDR verified as 1280x800). The QEMU helper's exact window-name probe did not emit a machine PASS, so this is visual live-ISO evidence, not installed-VM acceptance.
- The fresh Arch upstream application gate launched PCManFM, Xfce Terminal, Mousepad, Ristretto and Chromium through `start-slopos-browser`, then launched SuperTux on the packaged `world1/frosted_fields.stl` level at 960x540. Each launched window matched its PID; six fresh 1280x800 PNGs were non-empty, the game received Right/jump input while alive, `pactl list sink-inputs` contained an identifiable SuperTux stream on the disposable PulseAudio null sink, and the game exited through its own input path without a fatal log. The browser fixture's DOM marker and visible title were both asserted. This remains bounded container evidence, not physical speaker or GPU evidence.
- A fresh disk-only VirtualBox boot of the installed Arch guest matched pinned commit `206c456e0ae02a5a9543ff15fce413618f222fd8`; `qa-installed.ps1` captured `qa_exit=0`, `screenshot_exit=0`, `passed=true`, and the guest emitted `SLOPOS_X11_INSTALLED_VM_QA=PASS` with `shipping source contract clean (16 files)`. The generated `artifacts/qa/installed-vm-evidence/status.json`, `qa-vm.log` and screenshot are local ignored evidence, not source-controlled release artifacts.
- The no-fork browser integration was checked in a disposable profile install: Chromium receives the optional unpacked SLOPOS theme through the upstream extension mechanism; Firefox receives backed-up `userChrome.css`/`user.js` changes only when an explicit absolute profile path is supplied. The wrapper exports SLOPOS X11/GTK identity and never rewrites a browser binary or web content. The Figma Classic Macintosh UI Kit supplied by the owner was captured and used as the visual comparison reference; upstream browser chrome remains only best-effort aligned.
- Canonical screenshot capture now removes every prior PNG and uses scrot overwrite mode, preventing `_000` suffixed stale images from satisfying a later run; the fresh Docker/Xvfb gate produced eleven expected base-named 1280x800 captures for every contract scene, including a real D-Bus notification and functional About modal. The scene set is evidence for review, not an automatic visual score.
- `cargo fmt --all -- --check` passes in the Rust 1.97 container after the remediation formatting pass.
- `git diff --check` and `bash -n` for the changed installer, session, Docker, ISO and VM scripts pass.
- The Docker gate intentionally prints that visual acceptance remains a separate human/vision review gate; this ledger does not turn generated screenshots into a 100/100 claim.
