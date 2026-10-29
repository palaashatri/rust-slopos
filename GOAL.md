# GOAL — Deliver SLOPOS-I X11

**This is the single entry point for an autonomous coding agent.**
Objective: ship a usable **Linux X11/Openbox desktop** with SLOPOS's
MIT-first-party Rust UI toolkit, Calculator, Files, Image Viewer, Media,
Control Panels, Software and real shell status/OSD/notifications.
**Definition of "ship":** every gate in
[docs/p1-x11-release.md](docs/p1-x11-release.md) and the real installed
Linux QA passes, not a collection of compiling placeholder windows.

This goal **excludes** a Wayland/Smithay compositor, PARAMA, Loom,
Donkey and all other external repositories. Historic code is preserved
under archive refs; selectively adapt only audited source/tests.

## Agent instructions — act, test, checkpoint, continue

1. Read **this file first**, then [AGENTS.md](AGENTS.md) and
   [docs/autonomous-execution.md](docs/autonomous-execution.md).
   The P1 release plan overrides larger deferred-feature lists; the
   existing [Design Bible](docs/contract/02-design.md) §5.2 governs
   appearance. Do not dump all docs into context every iteration.
2. Check Git status/disk/VM, then run **on the host**:
   `python3 scripts/p1_goal.py validate`,
   `python3 scripts/p1_goal.py status`,
   `python3 scripts/p1_goal.py next`. For Windows, use
   `py -3 scripts\p1_goal.py ...` instead. These require Python 3.10+
   and only the standard library.
3. Create or identify an **isolated, disposable Linux guest** on macOS
   or Windows. All Cargo builds, runtime, package and GUI tests belong
   in that Linux guest. The host is the IDE/controller only. Respect the
   existing VM contract and never install packages system-wide on the
   host without approval.
4. Have **one coordinator** claim the highest-priority ready task using
   `scripts/p1_goal.py claim TASK-ID`. Deliver its acceptance criterion.
   Subagents (if available) get exclusive files and immutable base SHA;
   use at most two implementers in addition to the coordinator. If the
   current Antigravity model cannot delegate, operate sequentially.
5. Write a small independent test **before declaring implemented**;
   exercise positive, negative, cancellation, input, real file/service
   and failure handling. For GUI work run it in a Linux VM, capture
   screenshots at 1×/2×, **actually inspect the screenshot pixels**,
   keyboard/pointer behavior and accessibility, and fix mismatches.
   Use `docs/contract/02-design.md` rather than inventing visuals.
6. Commit and record durable progress. Transition states in order:
   `PLANNED → CLAIMED → IMPLEMENTED → VERIFIED → INTEGRATED`.
   Use the evidence/checkpoint format in the execution manual.
   **A failed or absent test is FAIL/NOT_RUN; never fabricate PASS.**
   Do not mark work VERIFIED by simply writing JSON or self-review.
7. At each iteration run the safe disk preflight. Keep one Cargo build
   target, two worktrees maximum, no redundant VM snapshots or giant
   logs; clear only explicitly allowlisted local scratch after checks.
   Stop building if free space or target/evidence budgets are exceeded.
8. After integration, select the next dependency-ready task and repeat.
   Verify end-to-end installed packages and entire desktop journeys
   before declaring **SLOPOS X11 P1 delivered**. Record blockers with
   the exact missing capability and continue independent ready work.
   If all work is blocked, stop with a useful blocker report—not fake
   success, infinite retries or endless plans.

## One-sentence prompt (paste into Antigravity)

> Deliver SLOPOS X11 P1 autonomously. Read `GOAL.md` first and obey its
> execution, QA, storage, platform and checkpoint rules. Start with
> `python3 scripts/p1_goal.py next` (Windows: `py -3`). Implement and
> verify each dependency-ready vertical slice in the Linux test guest,
> visually inspect real screenshots, commit evidence and resume after
> interruptions; never skip failures or declare unfinished apps complete.

The prompt is **one request**, not a guarantee the AI harness can work
indefinitely, reset model quotas, run unavailable tools or bypass permission
prompts. Host changes, secrets, credential prompts, destructive actions,
system installations and package publishing require explicit approval.

## Who is authoritative?

- **Current implementation** — `docs/architecture.md` + actual code/tests.
- **P1 scope/exit** — `docs/p1-x11-release.md`.
- **Task selection/dependencies** — `qa/p1-goal.json`; progress in
  `qa/p1-progress.json` and concrete evidence artifacts.
- **System safety, VM and QA** — `docs/contract/00-agent-workflow.md`
  and `docs/autonomous-execution.md`.
- **Visual style** — `docs/contract/02-design.md` §5.2.
- **Historical code** — `docs/historical-recovery-audit.md` and
  `docs/branch-preservation.md`.

When two instructions conflict, prioritize host safety, real evidence,
P1 scope, current architecture, then older/deferred ambitions.
