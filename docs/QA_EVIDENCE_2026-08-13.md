# QA evidence manifest — 2026-08-13

This is a provenance index for the evidence inspected during the 2026-08-13
audit. The files under `artifacts/` are ignored by Git. The hashes in the first
table identify the local files at the initial audit snapshot; later QA reruns
overwrote some of those ignored files, so those hashes are historical and are
not a source-controlled release bundle.

## Audit baseline

- The initial audit snapshot was taken at `a0088dff666aa879e0d9566e447fbfb5d98911bd`.
- The inspected source tree is now clean at pushed commit `63de816` on
  `pivot`. Hosted CI run [#568](https://github.com/palaashatri/rust-slopos/actions/runs/31754694669)
  has a succeeded build/test/lint job for this revision; its remaining matrix
  was still in progress when this manifest was updated.
- The initial audit commands were read-only PowerShell checks (`Get-Content`,
  `Select-String`, `Get-Item`, `Get-FileHash`, `git status`, `git rev-parse`).
- The parent run subsequently reran the verified current-tree Rust, AppMenu,
  Docker, Arch application and resolution gates; those markers are recorded
  below.

## Observed artifacts

| Scope | Observed file(s) and SHA-256 | What was observed | Limit |
|---|---|---|---|
| AppMenu capability | `artifacts/qa/session.log` — `0aecfd1c5fc90c3346fa8d171dd61d3a74f8c91704ad643f2be147888a533b96` (initial snapshot) | The initial full Docker session log contained `Focused X11 application exports AppMenu bus=:1.9 path=/org/xfce/mousepad/menus/menubar; bounded DBusMenu importer enabled`. | No `GetLayout` or `Event` result was present in that initial log. The dedicated AppMenu smoke used a synthetic X11-property fixture and no DBusMenu service. Later Docker evidence is listed below. |
| Browser | `artifacts/qa/app-matrix/browser.png` — `fa92bb898e61eb5d261f80d1b10694b86e547e7612f96c8c126ad778973778c8`; `browser-dom.html` — `fcbb2b3e360d9d88866d5fb46baaf4efc912069d24dd1dc8e4dd1c86898a1f38` | The DOM file contains `SLOPOS_BROWSER_QA_MARKER`; the visible Chromium frame exists. | `browser-firefox.log` says `Firefox runtime leg skipped: package is not present in this pre-provisioned image.` No current Firefox frame is evidenced. |
| Game/audio artifacts | `artifacts/qa/app-matrix/game.png` — `bc3aab87d4067adaade6524f417b568cdc4513092e6783f9e34faae575aca130`; `game-audio.raw` — `c9a0da59644db01a6ff69451d751e796ac41f437961d228d3227e5e2f3de1f9c`; `sink-inputs.txt` — `fe5ad873e0fe3875c3162d7c80c135a3336c1684bd4da66925f946b0c48c95f8` | The three files were present (395,906; 1,723,524; and 1,035 bytes respectively), and the sink listing names `SuperTux 2` with `media.role = "game"`. | This audit did not recompute PCM non-silence or replay input; retain the original Arch gate output as the semantic result. |
| Installed VM | `artifacts/qa/installed-vm-evidence/status.json` — `5467668de959f2188797d8bfa268602ad6e104f95e6d52f4180c2be67cb59d42`; `qa-vm.log` — `20cad238946b15e8cfe7bf8ab3df1d4005f4a781af91ea67995a5f94cf3d1368` | Status recorded `expected_commit`/`source_commit` `206c456e0ae02a5a9543ff15fce413618f222fd8`, `qa_exit: 0`, `passed: true`, and completion `2026-08-12T21:52:45.3473597Z`. | This is ignored, pinned-commit installation evidence. It predates the verified milestone `c74ce88` and does not validate the current tree. |

## Current-tree gate rerun

The parent run reran the verified current tree after the initial snapshot. The
reported markers were:

- Rust: `FMT_STATUS_0`, `CHECK_STATUS_0`, `TEST_STATUS_0`,
  `CLIPPY_STATUS_0`, and `RELEASE_BUILD_STATUS_0`; the current hosted build/test
  job reports 14 catalogue,
  14 shell unit, 29 shell integration, 3 spaces, and 1 Settings test.
- AppMenu: `APPMENU_QA_STATUS_0`.
- Docker/Xvfb: `DOCKER_QA_STATUS_0`; real Mousepad X11 AppMenu properties were
  detected, the App-button click produced the fail-closed fallback marker
  `APPMENU_MOUSEPAD_FALLBACK_STATUS_0`, and
  `artifacts/qa/screenshots/appmenu_fallback_mousepad_1280x800.png` was
  captured and checked as 1280x800. The explicit `fallback` name records the
  capability-aware result rather than implying an imported menu. The current
  session log records the concrete failure:
  `UnknownMethod: No such interface “com.canonical.dbusmenu”`
  at `/org/xfce/mousepad/menus/menubar`.
- Arch upstream application/game: `ARCH_APP_QA_STATUS_0`,
  `BROWSER_CHROMIUM_STATUS_0`, and
  `BROWSER_FIREFOX_STATUS_SKIPPED_OPTIONAL_PACKAGE`; Chromium and the game
  were exercised, while Firefox remained absent from the provisioned image.
- Resolution QA: `RESOLUTION_QA_STATUS_0` for 3440x1440, 3840x2160,
  5120x2880 and 7680x4320 at scale 1, plus 2560x1600 at scale 2. These are
  Xvfb geometry/render checks, not physical display evidence.

The reruns above do not change the installed-VM boundary: the only VM result
inspected remains the ignored run pinned to `206c456e`, not the verified
milestone `9c073d8`.

The hosted CI run for `9c073d8` completed successfully. It passed the locked
workspace build/test/lint, rustfmt, Xvfb/Openbox smoke, AT-SPI named surfaces,
Orca, Settings delegation, benchmark, release build, two locale legs and five
resolution legs. GitHub reported cache/artifact-upload warnings for some jobs
and no-files-found warnings for several resolution artifact directories; those
warnings do not provide extra visual evidence. The run is therefore strong
current hosted CI evidence, but it does not close the current-tree VM/EFI,
physical hardware, real AppMenu visual/action review, Firefox runtime, or
independent visual-gate boundaries below.

Hosted CI run [#588](https://github.com/palaashatri/rust-slopos/actions/runs/31760566548)
for `3e2e3d7` then completed successfully. It passed the current locked
build/test/lint, rustfmt, Xvfb/Openbox smoke, benchmark, release, accessibility
and Settings jobs, plus all five retained-resolution legs: 3440x1440,
3840x2160, 5120x2880, 7680x4320 and 2560x1600 with `GDK_SCALE=2`. These are
hosted Xvfb geometry/render results, not physical monitor timing or independent
visual acceptance.

After that rerun, the nested AT-SPI and Settings service D-Bus shells received
inner EXIT traps for their child PIDs. The Rust static contracts, `bash -n`,
fmt/check/test/clippy, cache-only AppMenu smoke and full Docker/Xvfb gate all
passed again; no new package or image downloads were used.

The cache-only Arch application/game matrix was then rerun after its harness
installed `themes/slopos-openbox/openbox-3/themerc` and exported the explicit
SLOPOS Openbox config. It passed `ARCH_APP_QA_THEME_STATUS_0`, captured fresh
1280x800 PCManFM, terminal, Mousepad, Ristretto, Chromium and SuperTux scenes,
and reported `game_audio_bytes=1675860` with `nonzero_audio_bytes=1591119`.

The post-`3287d00` source tranche adds a QA-only low-level DBusMenu exporter
(`scripts/qa-dbusmenu-exporter.c`) and a `SLOPOS_QA_REQUIRE_REAL_APPMENU=1`
hard-fail switch to the Docker harness. It also hardens recovery config
preservation, VM installer partition cleanup and the Openbox active titlebar
gradient. The local Docker image/volume cache is currently empty, so no local
post-tranche Docker runtime result is claimed here. The hosted CI run above
does provide current Linux/X11 acceptance for `9c073d8`; the fixture itself was
compiled in the existing WSL Ubuntu environment (`C_FIXTURE_COMPILE_STATUS_0`)
and its direct session-bus `GetLayout`/`Event` round-trip recorded
`DBUSMENU_FIXTURE_ROUNDTRIP_STATUS_0`; this does not replace the full GTK/Xvfb
click-through.

## Release accounting boundary

The inspected artifacts and current rerun markers support bounded capability,
Chromium, game/audio-path, Xvfb geometry and pinned-installation statements
only. They do not close the independent visual review, real AppMenu layout
action delivery (the current Mousepad path fails closed on `UnknownMethod`),
Firefox runtime leg, hardware-backed services/accessibility, physical
audio/GPU, or current-tree VM gates. Do not use this manifest or generated
screenshots as a 100/100 claim. The readiness ledger conservatively records
78/100: one Installer point and one Recovery point are withheld for
current-tree VM/EFI and hardware-recovery evidence that is not present.
