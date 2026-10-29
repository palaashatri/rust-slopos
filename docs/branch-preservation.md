# Preserved historical SLOPOS branches

Created on 2026-10-30 after auditing repository references. These are
**permanent-intent archive branch names**, protecting old source independent
of the original development branches. On 2026-10-30, a one-time GitHub Actions
cleanup verified exact branch SHAs, checked that divergent sources had
matching archive refs and that merged branches were already ancestors of
`main`, checked open-PR heads, then deleted **14 redundant branch refs**.
The cleanup run succeeded and its temporary workflow file was removed.
No historical source was force-pushed or bulk-merged. The exact SHAs below
identify the preserved code even if branch names later change.

**Important:** ordinary GitHub branches are **not deletion-proof**.
These new refs reduce accidental-loss risk but can still be deleted by
someone with repository write access. For stronger protection, an owner
should configure branch rulesets or signed archival tags (where
permissions allow), and maintain a tested offline `git bundle --all`
backup. GitHub's UI deletion prompts are not a backup policy.

| Original tip/source | Immutable commit SHA | Redundant archive branch | What survives |
|---|---|---|---|
| `agent/recovered-codex-spec-stash` | `6bbd168c179f75096ddb50dfcb914db4bfedf4ba` | `archive/2026-10-30-wayland-compositor-and-apps` | Smithay compositor and headless QA, old wgpu renderer, toolkit, SDK, Finder/Preview/Terminal/Software/Settings/TextEdit/Vision |
| `agent/recovered-pivot-stash` | `2e63fc35ace1f3f7b7c5ceaaaa9d4ea1e0f28529` | `archive/2026-10-30-x11-pivot-recovered-stash` | Early X11 transition and background QA edits |
| `agent/sinty-modern-theme-and-platform-desktop` | `4ff7d5881f13199d71b2ed3b2e44ccbb0127d2a5` | `archive/2026-10-30-sinty-modern-design` | Modern desktop requirements |
| `design/brand-identity-20261029` | `9a36872754aaa4513a98289605b638cbf2a94930` | `archive/2026-10-30-vector-brand-proposal` | Vector logo, icon generator, native exports |
| `design/identity-icons-20261029` | `a24de39d68981367d71ecd2e19d84b38b7dd4092` | `archive/2026-10-30-dimensional-icon-proposal` | Dimensional icon art/export sources (PR #31) |

Other historical branch refs that were zero commits ahead of `main`
were already ancestors of the current repository and were deleted after
verification; their histories remain in `main`.

## Completed cleanup (2026-10-30)

**Removed (14 original branch refs):**

- Merged into `main` (10): `agent/pi-zero2w-edge-performance-contract`,
  `agent/pr22-checklist-lockfile`, `agent/pr22-followup-fixes`,
  `agent/pr22-rereview-fixes`, `agent/pr22-review-fixes`,
  `agent/qa-01-release-runner-inventory`,
  `agent/qa-01-required-runner-map`, `agent/spec-01-schema-inventory`,
  `claude/rewrite-agents-md`, `codex/shared-ui-foundation`.
- Exact copy preserved in an archive (4):
  `agent/recovered-codex-spec-stash`, `agent/recovered-pivot-stash`,
  `agent/sinty-modern-theme-and-platform-desktop`,
  `design/brand-identity-20261029`.

**Remaining (9 refs at the time of cleanup):** `main`; six
`archive/2026-10-30-*` branches (including the pre-P1 baseline);
the open-PR heads `dependabot/cargo/cargo-48b1db0ec5` (PR #33) and
`design/identity-icons-20261029` (PR #31).

**Evidence:** [one-time GitHub Actions cleanup run #40961495270](https://github.com/palaashatri/rust-slopos/actions/runs/40961495270)
completed successfully; the maintenance workflow was subsequently
deleted from `main`. GitHub branch enumeration returned 9 refs.
The archive branches are *redundant references*, not an offline backup.

## Recovery procedure (work only in the intended Linux VM)

1. Read [historical-recovery-audit.md](historical-recovery-audit.md).
2. Pin an exact SHA and verify `git show --stat <sha>`.
3. Compare the source/tests with the current toolkit and service APIs.
4. Create a **new feature branch from current main**, never from the
   archived Wayland snapshot; select isolated tests/pure algorithms,
   and track their provenance in the PR.
5. Do not overwrite active X11/Openbox implementation, workspace
   manifests or release packaging. Preserve legacy alpha evidence
   separately from current runtime QA.
6. Run current CI and relevant isolated VM user flows. Do not merge
   recovered code into main merely because old QA reported a pass.
7. Never delete archive refs until backup and recovery has been
   demonstrated and the repository owner explicitly approves.

For independent backup, from a full authorized clone:

~~~bash
git fetch origin --prune
git bundle create slopos-2026-10-30.bundle --all
git bundle verify slopos-2026-10-30.bundle
~~~

Store the resulting bundle securely outside the repository. This guide
does **not** claim that a bundle has been created or validated.
