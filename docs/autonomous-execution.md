# Autonomous one-goal execution: SLOPOS X11 P1

**Audience:** a fast/low-reasoning coding model inside Antigravity,
Claude Code or other tool-capable IDE. **Entry:** repository-root
[GOAL.md](../GOAL.md). This document is operational, not new product scope.

The orchestration is **harness-agnostic**. We do not depend on an
undocumented Antigravity slash command, permission bypass, background
continuation, scheduler or provider-specific agent API. A single goal
can launch the workflow, but quota/context resets require the IDE to
resume a saved session or the user to submit the same goal again.

## 0. Exact command sequence (from repository root)

macOS and inside a Linux guest:

~~~sh
python3 scripts/p1_goal.py validate
python3 scripts/p1_goal.py host-check
python3 scripts/p1_goal.py status
python3 scripts/p1_goal.py next
python3 scripts/p1_goal.py preflight
~~~

Windows PowerShell:

~~~powershell
py -3 scripts\p1_goal.py validate
py -3 scripts\p1_goal.py host-check
py -3 scripts\p1_goal.py status
py -3 scripts\p1_goal.py next
py -3 scripts\p1_goal.py preflight
~~~

**No auto-installation occurs from these commands.** If Python/Git/VM
tools are missing, print exactly what is missing and request host
installation approval if needed. Don't guess package paths or install
from untrusted URLs.

To start a task (only coordinator updates state):

~~~sh
python3 scripts/p1_goal.py claim P1-00
# Work, test, inspect screenshots, commit, collect proof under .work/p1/evidence/
python3 scripts/p1_goal.py record P1-00 --to IMPLEMENTED --summary "baseline assembled"
python3 scripts/p1_goal.py record P1-00 --to VERIFIED --summary "tests and VM checked" --evidence .work/p1/proof-P1-00.json
python3 scripts/p1_goal.py record P1-00 --to INTEGRATED --summary "reviewed and incorporated"
python3 scripts/p1_goal.py next
~~~

Use **actual** task IDs and requirement kinds from
[qa/p1-goal.json](../qa/p1-goal.json). This example demonstrates the
order, **not a claim P1-00 is already verified**.

If a dependency or permission prevents work, record
`--to BLOCKED --summary "exact blocker + alternate ready work"`
from CLAIMED/IMPLEMENTED/VERIFIED. Work through other ready tasks;
resume a blocked task when its dependency becomes available. Never
convert a blocked row to integrated or forge a pass.

### Task selection algorithm

1. Read current SHA, Git status, active PRs, running VMs and disk
   budgets; preserve unrelated changes.
2. The first item from `next` is the default ready task. If several
   are ready, choose one small task using current source and dependencies.
3. Claim ONE task, with exclusive file ownership. At most **two**
   separate implementers may work concurrently; only one coordinator
   changes `Cargo.toml`, `Cargo.lock`, central public UI API, docs,
   packaging and the progress ledger. Use only one Cargo build permit.
4. Make the smallest production-quality change that satisfies the
   described user journey. Do not increase the task count with stubs.
5. Run targeted tests, capture logs, exercise real GUI behavior, inspect
   screenshots, repair failures, then run affected integration gates.
6. Make a small commit and checkpoint the verified evidence. Rebase
   safely against current integration SHA; never force-push or delete
   historical or other agents' branches.
7. Repeat. At most **two repair attempts for the same failure without
   new evidence**; then mark BLOCKED with logs and continue unrelated
   ready work. This avoids token-burning infinite patch loops.

## 1. Host and Linux test guest

The running SLOPOS product is **Linux/X11 + Openbox**. macOS and
Windows are controller/IDE hosts, not places to test X11 runtime.
Follow [contract §0](contract/00-agent-workflow.md): local project
build/test/runtime/package/visual QA lives in an **isolated Linux VM**
with a dedicated runtime/VM lease. Linux CI is additional evidence,
not proof of VM hardware compatibility.

| Host | Approved approach | Important distinctions |
|---|---|---|
| Apple Silicon macOS | Run Antigravity on Mac; a disposable ARM64 Linux VM under UTM/Parallels/QEMU for Cargo and Xvfb/Openbox | ARM64 VM tests AArch64; x86_64 distro artifacts and actual hardware need their own gates |
| Intel macOS | Same, with compatible x86_64 Linux guest under a VM provider | Do not build the X11 runtime as a macOS native app |
| Windows 11 | Run Antigravity on Windows; use disposable Ubuntu/Debian VM under Hyper-V/VirtualBox/QEMU | WSL2 can assist with preliminary Linux compilation, but it alone does **not** establish installed desktop/session/package QA |
| Linux guest | Use its own cloned repo, unique XDG/runtime directories, Xvfb/Openbox, system bus fixtures where needed | Do not mutate host display/network, main login/session or an unisolated physical device |

Install a compatible Linux guest according to the VM provider's
official installer/docs (ask before modifying host hypervisor settings).
Inside that VM, install the repo's documented Linux build requirements
from [docs/development.md](development.md) using the guest's package
manager only. Never implicitly run `brew upgrade`, `winget upgrade --all`,
`wsl --unregister`, `docker system prune`,
`git clean -fdx`, `rm -rf ~` or delete downloaded ISOs/VM snapshots.

Recommended local guest environment:

~~~sh
export CARGO_TARGET_DIR="$PWD/.work/p1/target"
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
python3 scripts/p1_goal.py preflight || exit 2
~~~

Cargo target and artifact bytes live **inside the guest's repository**;
do not put mutable Cargo output on a synced/network host folder or
reuse one target across parallel worktrees. Prefer one guest and one
small Rust build over multiple isolated clones unless measured storage
allows otherwise. Antigravity's terminal permissions and sandbox vary
by platform; do not switch to unrestricted/Turbo mode merely to avoid
approval prompts.

## 2. Disk, memory and quota discipline

[qa/p1-goal.json](../qa/p1-goal.json) is the machine-readable resource
policy. Default *pre-build* stop limits:

| Resource | Maximum/threshold | Action |
|---|---:|---|
| Free space | At least **8 GiB** | If lower, stop builds and report disk blocker |
| One guest Cargo target | **12 GiB** | Don't start another build; diagnose |
| Retained `.work/p1/evidence` | **512 MiB** | Compress small logs, keep a representative set |
| Single evidence artifact | **80 MiB** | Downsample/trim; keep capture metadata |
| Simultaneous Cargo builds | **1** | Coordinator assigns a single build permit |
| Parallel coding workers | **2** (+ one coordinator) | One exact owner per path |
| Worktrees | **2** | Reuse or retire after preserving commits |
| Shared X11 session/VM runtime lease | **1** | No competing global hotkeys/session D-Bus |
| Disposable artifacts | **7 days** | Review manually before scoped cleanup |

After every build or major artifact capture, run
`python3 scripts/p1_goal.py preflight` **inside the guest**.
A Python check cannot enforce a disk cap while a subprocess writes;
also monitor disk during long package builds. Reserve extra headroom
for VM expansion on the host. If host/guest free space is below
threshold, **do not keep downloading packages**, installing toolchains
or creating VM snapshots.

Safe preview of narrowly allowed cleanup:

~~~sh
python3 scripts/p1_goal.py cleanup --area tmp
python3 scripts/p1_goal.py cleanup --area target
~~~

Cleanup is dry-run unless `--confirm` is explicitly supplied.
Only the designated `.work/p1/tmp` or `.work/p1/target` directories
are eligible. **Never automatically delete evidence, source, Git refs,
user media, cache outside the repo, any VM or a downloaded installer.**
Before even deleting a P1 Cargo target, stop builds and retain logs.

For token efficiency, read one task's relevant source, tests and
section of the contract. Avoid repeatedly ingesting the entire
repository, generating giant plans, duplicate reviews or rewriting
AGENTS/README each task. At every context reset, read only GOAL,
progress ledger, last 3 commits, current task and blocker summary.

## 3. Anti-hallucination and UI test protocol

**There is no "looks good" shortcut.** Every implemented user-facing
journey has at least:

1. **Static/code:** Rust fmt/Clippy and actual unit/integration commands
   with exact exit statuses and relevant negative tests.
2. **Model/data:** inputs, errors, cancellations, overflow/memory/hostile
   files, permission denied, daemon missing, and no hard-coded demo state.
3. **Real UI:** launch installed binary in disposable X11 guest, interact
   with **pointer and keyboard**, confirm focus, dialogs, correct
   selection/action, window close, error reporting and accessibility.
4. **Visual:** capture screenshot from the real app at 1× and 2× with
   specified font/theme/resolution, **open and inspect the pixels**,
   compare against the binding Classic Figma/spec and previous golden.
   Test clipping, layout, text truncation, active/inactive window
   states, menus, dialogs, contrast, reduced motion.
5. **Independent check:** a different worker/reviewer reviews the small
   diff and evidence where available. Without that ability, explicitly
   record single-agent verification; don't invent a second reviewer.
6. **Integrated:** unaffected Calculator, Dock, logout/launcher,
   notifications, file associations and package paths continue to work
   in Xvfb/Openbox smoke tests; no regression to donor GTK surfaces.
7. **Security/legal:** old Finder file operations and installers are
   *research*, not verified safe implementations. No silent host
   package changes, production credentials or unaudited codec assets.

**Never fabricate** successful screenshots, SSIDs, Bluetooth devices,
benchmark scores, installer progress, signature checks, physical-device
QA, terminal output or successful tool calls. A test failure must be
reproduced and fixed, not disabled. If a screenshot tool cannot display
the actual captured image for review, mark visual gate NOT_RUN and
do not VERIFY the UI task.

### Evidence payload for `record --to VERIFIED`

Use `.work/p1/proof-TASK-ID.json`, referencing small logs and images
stored under `.work/p1/evidence/`. A proof for a GUI task might be:

~~~json
{
  "source_sha": "replace-with-real-40-character-commit-sha",
  "checks": [
    {
      "kind": "unit",
      "result": "PASS",
      "command": "cargo test -p slopos-image-viewer --locked",
      "artifact": ".work/p1/evidence/image-unit.log",
      "sha256": "replace-with-real-64-character-file-digest"
    },
    {
      "kind": "visual",
      "result": "PASS",
      "command": "bash scripts/smoke-session.sh",
      "artifact": ".work/p1/evidence/image-1x.png",
      "sha256": "replace-with-real-64-character-file-digest",
      "resolution": "640x480, scale=1, Platinum Classic",
      "review": "actual screenshot manually inspected for clipping and font legibility"
    },
    {
      "kind": "runtime",
      "result": "PASS",
      "command": "bash scripts/smoke-session.sh",
      "artifact": ".work/p1/evidence/image-x11.log",
      "sha256": "replace-with-real-64-character-file-digest"
    }
  ]
}
~~~

Additional kinds: `security` and `package`, as required per task.
The controller checks source SHA, Git ancestry at release, existence,
paths, sizes, real PNG signatures and SHA-256 digests; it **does not
independently determine** whether a screenshot
is aesthetically correct or a log demonstrates a complete user flow.
The agent must actually inspect both. Never generate fake proof JSON
just to advance a state. Upload/retain important QA images and logs
as short-lived CI artifacts or release attachments before cleaning.

## 4. Durable work and automatic-resume contract

- The **only writable canonical task state** is
  [qa/p1-progress.json](../qa/p1-progress.json). One coordinator
  updates it using the controller; workers never edit that file directly.
- Status transitions are enforced; default tasks are PLANNED, not
  falsely completed because their crates already exist.
- Every work cycle produces (when possible) one small source commit,
  a test record, a visual capture where relevant, and a checkpoint
  commit on a feature/integration branch. Avoid 20 simultaneous
  long-running worktrees and write conflicts.
- On quota loss/session interruption, restart with:
  **"Continue GOAL.md from qa/p1-progress.json and current Git HEAD;
  review the last task's evidence and resume the next dependency-ready
  work. Do not replan or redo completed work."**
- Reconcile dirty status, active worker branches and current SHA before
  continuing; don't blindly reset or delete other work.
- The coordinator may delegate **no more than two** independent leaf
  tasks if the harness supports it; it must not recursively spawn a
  swarm, exceed build permits or overlap paths.
- If tools cannot run a Linux VM, the agent may complete model and
  documentation subwork, but must mark runtime/visual/package gates
  NOT_RUN and finish with a reproducible blocker report.
- Do not promise that Antigravity or any model will resume itself
  when its quota resets. The workflow is *resumable*, not a guaranteed
  unattended scheduler.

## 5. Actual delivery gate

`python3 scripts/p1_goal.py release-check` fails unless all
declared P1 tasks are INTEGRATED and have recorded proof.
It is a ledger precondition, **not independent verification**.

On a disposable Linux guest, an authorized coordinator runs the
**one-command fail-closed wrapper** (which calls the existing release QA):

~~~sh
export SLOPOS_P1_GUEST=1    # only inside the disposable test VM
bash scripts/verify-p1-release.sh
~~~

It refuses dirty tracked source, missing task evidence, wrong OS, missing
explicit guest opt-in and insufficient disk headroom before expensive tests.
Do **not** set SLOPOS_P1_GUEST on a macOS/Windows controller or unisolated
Linux host.

The existing runner's green result is **objective gate evidence**, not
a license to claim all P1 apps exist. The release candidate must also
pass *real* installed user journeys for all seven P1 surface categories,
independent screenshot inspection and a reviewed hardware/NOT_RUN
matrix, with source SHA and package artifact provenance. No incomplete
task, missing real service, or unavailable VM gets waved through.

**Stop conditions:** P1 exit all passes; external permission/security
blocker; no safe disk headroom; no guest for required QA; quota
interruption; repeated failure without new evidence. In the last four
cases, checkpoint and report exactly what remains instead of claiming
success or looping indefinitely.
