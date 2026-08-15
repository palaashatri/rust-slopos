# SLOPOS Platinum Gap Remediation Implementation Plan

> **Execution contract:** Use Luna Max implementation and review agents under the supervising main agent. Each implementation agent must try the repository-required Gemini/Antigravity delegation first and record whether it succeeded or exhausted quota. Use test-first development for behavior changes, Docker/Xvfb for software acceptance, and a separate Luna visual critic for screenshot scoring.

**Goal:** Close every software-controlled gap exposed by the 2026-08-15 audit, produce fresh exact-SHA functional and screenshot evidence, and report the highest score the unchanged `AGENTS.md` rubric objectively permits. Do not manufacture hardware, installed-VM, native-exporter, or independent visual evidence.

**Normative specification:** `/Users/palaashatri/Code/rust-slopos/AGENTS.md`

**Starting state:** branch `pivot`, commit `15baaddc419d934ca6189a3448e2a20e075a26f3`, documented score `78/100`. `origin/pivot` matched the starting commit after a fresh fetch. Preserve and never stage or delete `.qa-target-59615d2/`, `.qa-target-604425a/`, `crates/slopos-platform/`, `crates/slopos-platform-linux/`, or `crates/slopos-platform-freebsd/`.

**Architecture:** Keep the four-binary X11/Openbox product. Add integration only at supported X11, GTK/GMenu, freedesktop icon-theme, upstream utility, and package boundaries. The canonical light Platinum appearance remains the release target. SLOPOS Vision remains excluded because the normative contract forbids an AI/Vision platform requirement. Dark mode is not credited or enabled until the canonical light appearance passes its gate; dormant theme-token directories are not runtime support. A Doom scene, if produced, must use a mature engine plus freely redistributable Freedoom data and remain separate from the existing SuperTux game evidence.

**Primary technology:** Rust/GTK3, X11/X11RB, zbus, Openbox, freedesktop icon themes, Bash, Docker, Xvfb, ImageMagick/Pillow-based evidence inspection already present in the repository.

## Baseline evidence matrix

| Domain | Start | Missing credit and required evidence |
|---|---:|---|
| Platinum identity | 12/20 | Original upstream-consumable icons; compact exporter-backed menu bar; stronger window hierarchy; remove root/GTK leakage and sparse layouts; independent scene scores. |
| Shell | 11/15 | Real `org.gtk.Menus` layout import plus `org.gtk.Actions` activation from an unmodified upstream app; richer menu headings and state handling. |
| X11/Openbox | 9/10 | Current-head installed VM and physical display/timing interaction evidence. |
| Upstream apps | 9/10 | Visual convergence plus physical GPU/audio evidence. |
| Catalogue | 8/8 | Preserve fail-closed behavior while polishing visuals; no score work required. |
| Services | 6/8 | Genuine installed delegates scene and hardware-backed mutation/suspend/Bluetooth evidence. |
| Installer/session | 7/8 | Current-head installed-to-disk BIOS/UEFI/recovery evidence. |
| Functional QA | 7/7 | Preserve all current gates; native AppMenu must pass before claiming that workflow. |
| Visual QA | 3/5 | All 11 scenes at required sizes, separate critic, each at least 95 and mean at least 97. |
| Performance | 2/3 | Release cold/warm and hardware-target measurements. |
| Accessibility/i18n | 2/3 | RTL/CJK/long-string and independent/hardware accessibility evidence. |
| Recovery | 2/3 | Current installed-VM and physical recovery evidence. |

The starting `78/100` remains the working score until new exact-head results justify a change. Dark mode, Vision, and Doom are not scorecard requirements and must not be used to inflate it.

## Global constraints

- `AGENTS.md` remains unchanged except to correct factual inconsistency without weakening any requirement or threshold.
- No Apple/proprietary assets, fonts, icons, names, or copied Figma resources. Reference measurements inform only original implementation.
- Every visible menu/control must invoke a real action or render disabled/omitted.
- No Wayland, Smithay, wlroots, custom display server, custom window manager, first-party ordinary application, or reintroduced compositor-era crate.
- No production code before a failing behavior test or acceptance probe has been observed. Configuration/assets require an executable contract check before packaging changes.
- Agents do not trust earlier screenshots or another agent's success report. The supervisor independently inspects diffs, commands, artifacts, and images.
- Broad Cargo gates run sequentially. Docker is the default execution environment; do not install build dependencies on the macOS host.
- A screenshot is evidence only when bound to the exact tested SHA, non-empty, dimension-checked, visually inspected, and listed in a manifest with its digest.
- Commit coherent verified tranches directly to `pivot`; never create a branch, worktree, PR, issue, or rewrite history.

### Task 1: Implement native GMenuModel AppMenu and compact menu headings

**Files:**
- Modify: `crates/slopos-shell/src/appmenu.rs`
- Modify: `crates/slopos-shell/src/topbar.rs`
- Modify: `crates/slopos-shell/Cargo.toml` and `Cargo.lock` only if a justified dependency is required
- Modify: `scripts/run-appmenu-qa.sh`
- Add or modify the smallest QA exporter/fixture files under `scripts/` needed to exercise `org.gtk.Menus` and `org.gtk.Actions`

**Red acceptance:** Extend unit/integration coverage so an `org.gtk.Menus` exporter is detected but the current DBusMenu-only importer fails to return headings or activate the advertised action. Run the focused test and capture the expected failure before production edits.

**Implementation:**
1. Introduce an explicit exporter protocol enum rather than treating every `_GTK_*_OBJECT_PATH` as canonical DBusMenu.
2. Parse bounded `org.gtk.Menus` groups and links, including nested sections/submenus, separators, enabled state, toggle/radio state when advertised, label limits, cycle protection, item/depth limits, and timeouts.
3. Resolve and activate only advertised `org.gtk.Actions` names and typed targets; never synthesize actions from labels.
4. Keep the existing canonical DBusMenu path and honest local-menu fallback.
5. Render compact top-level headings from the actual exporter instead of one generic `App` button. When no exporter exists, show only honest shell-owned menus/actions; do not paint fake File/Edit/View commands.
6. Make focus changes/exporter disappearance cancel stale menus safely and keep the GTK main loop unblocked.
7. Extend the real Mousepad Docker workflow to require native properties, imported headings, one successful action, and fallback after exporter disappearance.

**Verification:** Focused Rust tests, `cargo clippy -p slopos-shell --all-targets --locked -- -D warnings`, the synthetic DBusMenu fixture, a GMenu fixture, and the unmodified upstream Mousepad real-exporter path in Docker. Retain logs and a menu-open screenshot.

### Task 2: Package an original freedesktop SLOPOS Platinum icon theme

**Files:**
- Add: `themes/platinum/icon-theme/index.theme`
- Add: original SVG/PNG hierarchy under `themes/platinum/icon-theme/{scalable,16x16,22x22,32x32,48x48}/`
- Modify: `assets/config/gtk-3.0/settings.ini`
- Modify: `install.sh`
- Modify: packaging/ISO/session install scripts and package manifests that install theme assets
- Modify: `scripts/run-docker-qa.sh` and/or add a focused icon-theme QA script
- Modify: `THIRD_PARTY_LICENSES.txt` only for genuinely reused open-licensed inputs

**Red acceptance:** In an isolated GTK/PCManFM session, assert that the configured `SLOPOS-Platinum` theme cannot currently resolve representative folder, file, drive, trash, application, and MIME icons. Observe failure before adding assets.

**Implementation:**
1. Create an original redistributable freedesktop icon theme with a valid `index.theme`, cache-compatible directory declarations, SLOPOS role icons, common `places`, `mimetypes`, `devices`, `status`, and `actions` names, and explicit `hicolor`/Adwaita fallback only for unknown coverage.
2. Reuse existing original SLOPOS assets where appropriate; do not rename proprietary icons or misrepresent unknown applications.
3. Configure GTK3 to select the new theme and install it to the standard icon-theme path in custom-prefix and system-prefix layouts.
4. Run `gtk-update-icon-cache` opportunistically only when available; installation must remain deterministic without it.
5. Add runtime resolution checks through GTK, not source-text greps, and a PCManFM screenshot showing the shipping theme.

**Verification:** Focused theme resolver test, custom/default-prefix installer smokes, Docker PCManFM capture, image inspection, and source/license audit.

### Task 3: Prove Settings with genuine available delegates and preserve unavailable behavior

**Files:**
- Modify: `packaging/deps/arch.txt`
- Modify: `packaging/deps/ubuntu.txt`
- Modify: `packaging/vm/arch-install.sh`
- Modify: `scripts/run-settings-service-qa.sh`
- Modify: `scripts/run-docker-qa.sh` or `scripts/run-arch-app-qa.sh`
- Modify: `crates/slopos-settings/src/main.rs` only if tests expose incorrect resolution/labels

**Red acceptance:** Add package-provider and runtime-resolution checks showing that the current shipping lists do not supply every declared delegate (notably keyboard/mouse), and that the normal canonical Settings screenshot is captured in an all-disabled minimal environment.

**Implementation:**
1. Map every Settings command candidate to a real package on each supported image, using distro-correct package names and omitting unsupported alternatives honestly.
2. Ensure the release image installs the selected display, audio, network, Bluetooth, power, appearance, desktop, and keyboard/mouse delegates.
3. Retain a hermetic absent-utility test proving all rows disable cleanly.
4. Add a separate installed-utility test using real packaged executables—not stubs—to prove rows are enabled and launch their delegate process/window.
5. Capture both `settings-available.png` and `settings-unavailable.png`; neither scene proves hardware state mutation.

**Verification:** Bash syntax, focused Settings Rust tests, unavailable fixture, installed-package Docker/Arch run, PID/window matching for every available delegate, and both screenshots.

### Task 4: Tighten the canonical light Platinum visual system

**Files:**
- Modify: `assets/config/gtk-3.0/gtk.css`
- Modify: `crates/slopos-shell/src/topbar.rs`, `launcher.rs`, `dock.rs`, and `notifications.rs` only where layout changes require it
- Modify: `crates/slopos-catalogue/src/main.rs`
- Modify: `crates/slopos-settings/src/main.rs`
- Modify: `themes/slopos-openbox/openbox-3/themerc` and mirrored Openbox theme source
- Modify: `themes/platinum/{Colors.toml,Metrics.toml,Typography.toml,Theme.toml}` to keep tokens truthful
- Modify: QA assertions/screenshots required by the affected scenes

**Red acceptance:** Encode the observed visual defects as measurable layout/image checks where possible: bar height/edge alignment, off-screen/clipped widgets, headerbar duplication, active/inactive frame distinction, root-warning leakage, required menu labels, and non-empty content regions. Record current failures and retain the starting screenshots as defect evidence.

**Implementation:**
1. Use original compact Platinum proportions informed by the reference: a roughly 19–22 px 1x menu bar where readable, crisp one-pixel outlines, square geometry, restrained bevels, `#000080` selections, and explicit active/inactive hierarchy.
2. Remove avoidable GTK CSD/headerbar conflict from first-party Settings/Catalogue windows while preserving accessibility and Openbox ownership.
3. Strengthen window titlebar distinction and classic frame vocabulary without unlicensed patterns or illegible dense striping.
4. Tighten Settings/Catalogue/Search/notification/dialog spacing so each scene is intentional rather than sparse, while retaining functional states.
5. Keep every palette source aligned and keep the shipping desktop at `#758090` or an original muted SLOPOS pattern based on it.
6. Check 1x and 2x readability; no arbitrary rounded cards, pills, glass, or modern-blue secondary system.

**Verification:** Focused layout/unit checks, Rust format/clippy/tests for touched crates, Openbox/GTK theme parse checks, Docker captures for all canonical states, and direct visual inspection at required scales.

### Task 5: Add a legal, separate Freedoom QA scene

**Files:**
- Modify: `packaging/deps/arch.txt` only if the mature engine/data packages belong in the optional app matrix
- Modify: `scripts/run-arch-app-qa.sh`
- Modify: QA manifest generation and documentation for optional applications

**Red acceptance:** Add a separate `doom.png` requirement for the opt-in Doom leg and observe that the current SuperTux-only script cannot produce it.

**Implementation:**
1. Detect distro-provided Freedoom data and a maintained Doom-compatible engine. Never download or package a proprietary WAD.
2. Launch a real playable level in X11, PID/window-match the engine, send bounded movement/fire input, and require a visually non-static frame.
3. Write `doom.png`, logs, package/version/license provenance, and manifest entries without renaming or replacing `game.png`/SuperTux evidence.
4. If the required legal packages are unavailable from configured repositories, fail the opt-in leg honestly and record the exact blocker rather than fabricating a screenshot.

**Verification:** Package provenance check, process/window/input assertions, PNG dimension/content checks, and human inspection.

### Task 6: Strengthen release, security, accessibility, performance, and recovery evidence

**Files:**
- Modify: `.github/workflows/ci.yml` and related release workflow files
- Add/modify: focused scripts under `scripts/` for dependency policy, SBOM/checksums/build metadata, RTL/CJK/long-label layout, release performance, and bounded recovery
- Modify: packaging scripts only where a failing release contract requires it

**Red acceptance:** Inventory requested gates and execute each current command. Record absent tools/gates or failing behavior as baseline; do not add checks that only grep for their own source text.

**Implementation:**
1. Add a pinned dependency/license/advisory policy using repository-appropriate tooling, documenting allowed advisories rather than hiding them.
2. Produce a machine-readable SBOM/dependency manifest, release build metadata, and SHA-256 sums for release artifacts.
3. Add RTL and CJK stress locales, long translated strings, and focused keyboard/AT-SPI/Orca checks where Xvfb can prove them.
4. Measure release cold/warm launch, shell/process-tree RSS, idle CPU, bounded liveness, notification/application stress, and DBus disappearance/reconnection without substituting these for hardware proof.
5. Exercise repeatable child/session recovery and preserve configuration backups.

**Verification:** Each focused script, then the full CI command set. Retain raw logs and machine-readable results bound to the tested source SHA.

### Task 7: Run the full exact-head Docker/Xvfb acceptance and screenshot matrix

**Files:**
- Modify: QA orchestration only for defects found by execution
- Add: ignored exact-SHA evidence under `artifacts/qa/exact-<sha>/`
- Add: a tracked human-readable visual review only after the independent critic reports its scores

**Execution:**
1. Run sequentially: locked metadata, build/all targets, tests, clippy `-D warnings`, rustfmt check, release build, source contract, installer smokes, Settings, AppMenu, AT-SPI, Orca, locale, performance, recovery, and Docker/Xvfb session QA.
2. Capture all 11 canonical scenes from one source commit at 1280x800, plus representative 1366x768, 1920x1080, and 2560x1600/GDK_SCALE=2 evidence. Retain ultrawide/4K/5K/8K geometry checks without treating them as physical display proof.
3. Validate every PNG's dimensions, color diversity, content bounds, digest, and exact-SHA manifest entry.
4. The supervisor views every image. Then a Luna Max critic who did not implement the visuals scores each scene against the exact 10x10 rubric and writes both JSON and Markdown reports.
5. If any scene is below 95, the mean is below 97, or obvious leakage/clipping/placeholders remain, return the exact findings to the relevant implementation agent and repeat only after fixes.

**Verification:** Fresh command exit codes and retained artifacts only. No stale-SHA evidence, hidden failures, or fixture-only native AppMenu claim.

### Task 8: Attempt current-head ISO/installed-VM acceptance without fabricating unavailable hardware

**Files:**
- Modify packaging/VM scripts only for reproducible failures found during the attempt
- Add ignored evidence under the existing ISO/VM evidence directories

**Execution:**
1. Build the exact-head release ISO in the supported isolated environment and produce checksum/build metadata.
2. If QEMU/VirtualBox and required virtualization support are available, boot live BIOS/UEFI, install to a fresh virtual disk, reboot, run in-guest QA, exercise session recovery/shutdown/reboot, capture screenshots, and require guest SHA equality.
3. If host tooling or nested virtualization blocks installed-VM work, retain the exact error and leave the point uncredited.
4. Never translate VM results into physical GPU, monitor, audio, Wi-Fi, Bluetooth, suspend, battery, or accessibility claims. Produce a deterministic hardware checklist for those external gates.

**Verification:** Exact-SHA status JSON, logs, screenshots, boot mode, ISO/image checksums, and guest/local SHA equality when the run is possible.

### Task 9: Re-audit truth, review the whole branch, commit, and push

**Files:**
- Modify: `TRUTH.md`
- Modify: `README.md`
- Modify: `AGENTS.md` only if factual audit wording is inconsistent; do not change architecture, rubric weights, thresholds, or definition of done
- Add: tracked evidence indexes/reports justified by Task 7/8 outputs

**Execution:**
1. Independently map every credited point to fresh command output and artifacts; keep older evidence explicitly historical.
2. Update exact source/runtime anchors, score rows, blockers, and all answers about icons, AppMenu, Settings, Figma distance, dark mode, Vision, and Doom.
3. Dispatch a Luna Max whole-branch spec/code/evidence reviewer. Fix all load-bearing findings and re-run affected gates.
4. Re-run the final full verification after the last edit. Do not claim 100 unless every `AGENTS.md` stop condition is proven.
5. Commit coherent verified tranches on `pivot`, push to `origin/pivot`, fetch, and require local HEAD, upstream tracking ref, and remote branch SHA equality.

**Final report:** Starting/final SHA and score; commits; score delta by domain; exact commands/results; screenshot/visual scores; ISO/VM evidence; unresolved external/hardware evidence; direct answers to the user's seven product questions; final release verdict.

## Pre-authorized rulings

- **No SLOPOS Vision implementation:** the normative X11 contract explicitly rejects a Vision/AI daemon or platform requirement. Reintroducing the deleted compositor-era subsystem would be an unauthorized architecture reversal.
- **Canonical light before dark:** dark mode is not current runtime support and will be described honestly. The contract explicitly requires one canonical light appearance before additional themes; visual remediation therefore targets Platinum light first.
- **Freedoom only:** the requested Doom screenshot is optional application evidence and may use only free data from distro repositories. It cannot replace the existing SuperTux scene or contribute score by itself.
- **Truthful stopping score:** Docker can close software gaps but cannot prove physical hardware or an unavailable installed VM. Any such points remain withheld with exact closure procedures.
