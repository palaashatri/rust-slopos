# TRUTH.md — SLOPOS-I Audit & Readiness Ledger

**Current work branch:** `chatgpt/classic-ui-parity`  
**Implementation baseline audited:** `5dffc5680a7e5d61e7028f950c987f313c772f19`  
**Visual contract:** `AGENTS.md` + `qa/reference/slopos-classic-reference.svg` + `qa/reference/slopos-classic-reference.json`  
**Audit date:** 2026-08-27  
**Evidence-backed readiness:** **UNSCORED pending full re-audit against the current contract**  
**Release status:** **NOT RELEASE READY**

> **Anti-hallucination rule:** If a visual or product goal is not explicitly written in `AGENTS.md` or directly observable in the canonical SLOPOS reference, the reviewer must mark it **UNKNOWN** rather than inventing a requirement.

## Executive summary

The previous `100/100` release-ready claim is invalid for the current tree and has been retired.

The classic-UI branch has made meaningful progress toward the new canonical desktop target: the bottom dock/Application Strip is removed from the runtime composition, the global top bar and Openbox chrome have moved toward compact platinum styling, desktop objects are present, and Settings has been changed toward a Control Panels-style icon surface.

However, the branch has **not yet passed the new canonical vision review**, and the current CI set contains known failures/noise. Therefore no visual-parity score, release-ready score, or `100/100` claim is valid at this point.

## Canonical visual target

The canonical visual reference is:

- `qa/reference/slopos-classic-reference.svg`
- review manifest: `qa/reference/slopos-classic-reference.json`

The reference is a clean-room derivative of the user-provided late-1990s platinum desktop screenshot. Third-party logos, product names, proprietary icon artwork, and non-goal elements were removed or replaced.

It defines:

- composition;
- window geometry;
- chrome density;
- top-bar behavior and visual hierarchy;
- icon-view density;
- classic widget depth;
- desktop-object placement;
- compact utility-window proportions;
- explicit non-goals such as **no bottom dock** and **no modern rounded/card-heavy redesign**.

It does **not** authorize copying proprietary assets.

## Current visual truth

**Status:** **UNKNOWN / NOT YET ACCEPTED**

Reason: existing screenshots have not yet been reviewed against the newly committed canonical reference using the strict vision rubric in `AGENTS.md`.

No contributor or model may claim "classic parity", "close enough", "production visual quality", or equivalent wording until a current screenshot set is compared against the canonical reference and recorded below.

### Required evidence set

| Scene | Required | Current accepted evidence |
|---|---:|---|
| Empty composed desktop | Yes | **UNKNOWN — fresh canonical review required** |
| System/file-browser window | Yes | **UNKNOWN — fresh canonical review required** |
| Settings / Control Panels | Yes | **UNKNOWN — fresh canonical review required** |
| About This Computer-style window | Yes | **MISSING / not yet implemented as canonical SLOPOS surface** |
| Accessory window (Calculator/Notes equivalent) | Yes | **MISSING / not yet implemented as canonical SLOPOS surface** |
| 1280×800 set | Yes | **UNKNOWN — evidence must be regenerated/reviewed** |
| 3440×1440 set | Yes | **UNKNOWN — evidence must be regenerated/reviewed** |

### Visual score ledger

The score remains **UNSCORED** until the required scenes exist and a vision-capable reviewer performs the exact rubric from `AGENTS.md`.

| Category | Weight | Current |
|---|---:|---:|
| Menu bar fidelity | 15 | UNKNOWN |
| Desktop composition | 10 | UNKNOWN |
| Window chrome fidelity | 20 | UNKNOWN |
| Widget/control fidelity | 15 | UNKNOWN |
| File-browser/icon-view fidelity | 15 | UNKNOWN |
| Typography, density, spacing | 10 | UNKNOWN |
| Non-goal compliance | 15 | UNKNOWN |
| **Total** | **100** | **UNSCORED** |

A visual PASS requires at least **85/100**, no critical failure, window chrome at least **15/20**, and non-goal compliance at least **13/15**.

## What is already materially improved on the classic branch

The following observations are supported by the current implementation history and earlier composed-session screenshots, but they are **not substitutes for the new canonical visual review**:

- the bottom Application Strip has been removed from the runtime desktop;
- the bottom reserved work area has been removed;
- the top bar has been reduced toward a compact global-menu strip;
- Openbox chrome uses compact hard-edged classic styling rather than the prior rounded interpretation;
- the desktop composition includes right-side desktop objects and Trash placement;
- the visible desktop uses the blue classic-oriented background path rather than the old gray fallback;
- Settings has moved from large modern cards toward a Control Panels-style icon grid;
- desktop-folder integration uses an isolated SLOPOS PCManFM profile rather than silently modifying unrelated user configuration.

## Known visual gaps

These are current blockers to a canonical visual PASS:

1. **System/file-browser parity is incomplete.** Opening a folder still exposes too much generic PCManFM visual language. The private SLOPOS profile needs denser icon layout, classic chrome-compatible spacing, and removal/suppression of visually incompatible modern/default elements where supported.
2. **Settings still needs a strict canonical review.** Its icon-folder direction is correct, but spacing, natural window height, widget treatment, and control-panel detail views need comparison against the reference.
3. **Canonical About window is missing.** A first-party SLOPOS information window is required to prove compact utility-window styling and branding discipline.
4. **Canonical accessory window is missing.** Calculator or Notes-equivalent evidence is required to prove small-window chrome and control styling.
5. **Representative upstream application screenshots remain necessary.** The shell must not look coherent only in first-party surfaces.
6. **Fresh multi-resolution screenshots are required.** Existing captures do not automatically pass the newly introduced visual contract.

## Current CI truth

The Actions page is noisy because routine pushes trigger too many independent jobs and matrices. Many jobs are green, but the number of routine jobs makes the repository look less healthy than the actual failure count warrants.

### Known current failures from the latest audited run

1. **Settings delegation QA is stale.** The QA script still assumes panels such as Sound remain delegated/unavailable under the old model. Current Settings exposes built-in or changed panel behavior, so the test reports `unavailable delegated panel is still enabled: Sound settings`.
2. **Rust formatting check is stale against recent Settings edits.** Build/tests/Clippy reached green in the audited run, while `cargo fmt --check` still needs the current tree formatted.
3. **Some QA jobs install an excessively broad desktop stack** for narrow checks, increasing runtime and failure surface without increasing signal.
4. **Routine CI is over-matrixed.** Full resolution, packaging, ISO, installed-VM, hardware-contract, and recovery-style lanes should not all behave as mandatory per-push development checks.

### Desired CI shape

Routine push/PR CI should converge on a small high-signal set:

- workspace build/test/lint/format;
- one representative composed-X11 runtime + visual smoke;
- accessibility acceptance.

Expensive package, ISO/media, installed-VM, full-resolution, and release-provenance lanes should remain available but run on release candidates, manual dispatch, or relevant-path changes rather than every normal UI commit.

Until that cleanup is implemented and green, CI health must not be described as fully healthy.

## Architecture truth

### Overall stack

The current X11/Openbox architecture remains appropriate for SLOPOS-I:

```text
Linux services
  ↓
X11
  ↓
Openbox
  ↓
slopos-session
  ↓
slopos-shell
  ├─ top global menu/system bar
  ├─ application search
  ├─ notifications
  └─ desktop integration
  ↓
slopos-settings / slopos-catalogue / normal Linux applications
```

The bottom dock/Application Strip is **not** part of the current product target.

### Event-driven integration

The product contract requires hot-path X11 and system state to be event-driven rather than subprocess-polled. Any remaining frequent polling path is technical debt and must be listed rather than hidden behind a completion score.

### Settings

Settings is intentionally a Control Panels-style shell. It may combine native SLOPOS panels and delegated mature utilities. Every enabled control must perform a real action, and unavailable delegated utilities must fail closed.

### File browsing

The current strategy is to use a private SLOPOS profile around a mature file manager while visual parity is developed. A custom file manager is not automatically required. If upstream constraints make the canonical experience impossible, that limitation must be demonstrated before accepting the maintenance cost of a first-party replacement.

## Release engineering truth

The repository contains package/media infrastructure, but release readiness requires current successful execution, not file existence.

A package/image target may be called supported only when reproducible evidence proves:

1. dependencies resolve;
2. workspace compiles;
3. package/image is constructed;
4. installation or boot succeeds;
5. X11 starts;
6. SLOPOS session and shell start;
7. Settings and Catalogue pass smoke tests;
8. representative applications launch;
9. required screenshots/logs are captured;
10. the exact checksummed artifact is published.

ARM64 and RISC-V support claims remain evidence-gated; manifest declarations alone are insufficient.

## P0 blockers

1. Generate the full canonical visual evidence set at 1280×800 and 3440×1440.
2. Run a vision-capable comparison against `qa/reference/slopos-classic-reference.svg` using the exact rubric in `AGENTS.md`.
3. Fix every visual critical failure before claiming parity.
4. Finish System/file-browser visual integration.
5. Finish Settings/Control Panels visual integration.
6. Add canonical About and accessory utility surfaces/evidence.
7. Fix stale Settings service QA.
8. Run `cargo fmt` and restore routine Rust CI to green.
9. Reduce routine GitHub Actions to the high-signal development gates and move expensive release/media matrices out of every normal push.
10. Re-run packaging/media/installed-VM release gates on an actual release candidate before any release-ready claim.

## P1 quality work

- strengthen representative upstream application theming/integration;
- add explicit screenshot naming and artifact manifests for each visual scene;
- retain historical screenshot evidence by commit without treating old evidence as proof of new code;
- add protocol fixtures for exported global menus;
- add dynamic display hotplug tests;
- continue modularizing shell and Settings state from GTK presentation;
- add release provenance/SBOM generation where practical.

## Visual truth policy

- Missing evidence = **UNKNOWN**, never PASS.
- Ambiguous evidence = **UNKNOWN**, never assumed solved.
- Old screenshots do not prove a newer commit.
- A regression lowers the score.
- A reviewer may not preserve an earlier score simply because another model wrote it.
- A reviewer may not invent goals beyond `AGENTS.md` and the canonical reference.
- Visual QA proves appearance only, not functionality, packaging, or hardware support.

## What 100/100 means now

`100/100` is reserved for shipping evidence, not aspiration.

It requires all of the following simultaneously:

- canonical visual review passes at **>=85/100** with no critical visual failure;
- first-party and representative upstream surfaces are visually coherent;
- no fake or enabled no-op controls remain;
- hot-path X11/system integration is event-driven;
- global-menu behavior is protocol-backed and truthful;
- multi-monitor behavior is correct;
- package install/upgrade/removal lifecycle is clean;
- signed package repositories exist;
- release-candidate consumer media is CI-built and boot-tested;
- architecture support claims are backed by build/boot/session evidence;
- artifacts are tied to exact commits and checksums;
- README, AGENTS, and TRUTH describe the same shipping reality;
- no unresolved release blocker is hidden behind an automatically generated or self-awarded score.

Until those gates pass, this ledger must remain below 100 or explicitly UNSCORED rather than fabricate precision.