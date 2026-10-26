<!-- Part of the SLOPOS-I engineering contract; index: ../CONTRACT.md -->

# 0. Agent execution contract — read first

This file is deliberately comprehensive. A harness may supply only its beginning. Every coordinator reads the whole file; every worker explicitly reads this section, §§20–21, §§28–30, its assigned component sections and Part II before changing code. Do not assume an automatically injected excerpt contains the entire contract.

## 0.1 Decisions to preserve

| Decision | Current instruction |
|---|---|
| Product generation | SLOPOS-I is an X11 desktop environment. Keep Openbox as the current WM; no Wayland/Smithay revival or custom display server in this tranche. |
| Language and appearance | Primarily Rust; Platinum Classic geometry and interaction. Earlier Cheetah/Aqua, Cutefish, tucch and C/C++ explorations do not authorize a rename or rewrite of this tree. |
| UI first | Build and verify `slopos-ui`, then `slopos-appkit`; native applications and shell surfaces consume them. GTK3/GDK/Pango/GIO remain only in legacy donor crates and engine adapters, never in new UI. |
| Required shell | Dock, global menu, status applets, launcher, desktop, notifications, workspaces and volume/brightness OSDs. The later Dock requirement supersedes the former “no Dock” rule. |
| Applications | Preserve the full inventory in §§16–18. Start with the component gallery and small proof apps, then core daily-use apps; do not start every app at once. |
| Repository | One monorepo, agreed 2026-09-29. Use temporary task branches/worktrees, never permanent compositor/app branches that each contain a different product. |
| Execution | All local project compilation, tests, formatting, lint, runtime, packaging and visual QA run inside a Linux VM. A host container is not a substitute. |
| Documentation | This file is the central contract and evidence ledger; Git history archives superseded prose. |
| Later capabilities | Keep HDR/VRR/color-management and live-wallpaper ambitions visible, but require supported-backend/hardware evidence before exposure. They do not authorize Wayland work now or fake controls in an X11 VM. Kindle/AI/vision and office-suite expansion do not displace the core desktop. |

The September 2026 X11/UI-library decisions and the merged Dock amendment in PR #14 are the operative baseline. Do not revive an older architecture because it sounds more ambitious.

## 0.2 Start and resume procedure

1. Read `git status --short`, `git diff --stat`, `git log -5 --oneline`, `git rev-parse HEAD` and `git worktree list`. Preserve unrelated changes. Record the exact base SHA, branch and dirty state.
2. Read Part II and inspect the assigned source paths. Distinguish **existing**, **to create**, **historical** and **unverified**. No `slopos-ui`, `slopos-appkit`, gallery or `xtask` exists at the audited baseline.
3. The coordinator establishes the VM and resource record (§§28–29), selects a dependency-ready task from §21 and records ownership in Part II.M. No worker self-assigns overlapping paths.
4. Agree the narrow acceptance criteria, interface shape and test command before implementation. Missing UI atoms become UI tasks, not app-local workarounds.
5. On resume, recheck the coordinator's integration SHA, task ownership and upstream interface changes. Stop only the affected task if its dependency changed; continue independent authorized work.
6. A documentation-only audit may read/edit source and perform text consistency checks without a VM. It must not run repository code or claim project test/visual passes.

## 0.3 Coordinator and worker ownership

Use one coordinator/integrator, a small number of workers and an independent review pass. Start with two or three workers; increase only after measuring RAM, disk and build contention. Agent count is not permission to launch that many Cargo builds or graphical sessions.

The coordinator alone owns:

- this contract (`docs/CONTRACT.md`), the root `AGENTS.md` instruction sheet, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and the integration branch;
- shared public interfaces, crate/module registration (`lib.rs`, `mod.rs`), gallery registries and specification manifests;
- CI/workspace-wide policy, shared theme/token changes and packaging entry points;
- task assignment, resource reservations, integration order and final evidence reconciliation.

An owner may explicitly delegate one of these paths in a task packet; record the handoff and suspend other edits to it. Leaf-module workers send proposed registration/dependency changes to that owner. Only one owner changes a file at a time, even if worktrees would let Git merge it.

Workers own only the exact listed paths and tests. They do not rewrite shared APIs, reformat the workspace, update lockfiles, weaken goldens, edit another worker's files, change the product contract or push to `main`. A blocker report includes the smallest needed upstream change. The coordinator either schedules it or reassigns a non-conflicting task. Workers do not spawn additional implementers without coordinator allocation.

The coordinator updates Part II for the integrated change set. Workers return evidence and proposed ledger text instead of independently editing this file.

## 0.4 Task packet and handoff

Every assignment includes these fields; a broad role such as “finish Settings” is not a task.

| Field | Required content |
|---|---|
| Identity | Stable task ID from §21, named owner, branch/worktree and base SHA |
| Outcome | One reviewable user behavior or infrastructure contract; explicit exclusions |
| Dependencies | Integrated prerequisite SHAs, accepted spec/state IDs and public API signatures |
| Ownership | Exact writable files/directories, read-only references and coordinator-owned registration paths |
| Acceptance | Positive behavior, failure/cancellation behavior, accessibility where relevant and observable result |
| Verification | Existing exact commands; planned tests clearly marked as to be implemented; required VM/visual/hardware evidence |
| Resources | Build permit, maximum Cargo jobs, target/artifact paths and any exclusive runtime/system-service lease |
| Handoff | Commit SHA(s), changed paths, commands/exit codes, artifact paths/hashes, limitations and next dependency |

Task states are `PLANNED → READY → CLAIMED → IMPLEMENTED → VERIFIED → INTEGRATED`; `BLOCKED` records a precise missing dependency and unblock condition. `IMPLEMENTED` is not `VERIFIED`. `INTEGRATED` also requires review and the relevant integration gates. A planning row does not establish that a component exists.

A task is small enough when it can be reviewed, reverted and tested independently. Split provider state discovery from mutations, control rendering from interaction, and application commands from their views when that produces clear ownership. Do not split into empty crates or stub callbacks merely to increase the completed-task count.

## 0.5 Branches and worktrees

Keep all clones, worktrees, targets and test data on guest-local storage. Start each task from the coordinator's recorded integration commit, not an arbitrary latest remote branch.

Example **inside the established guest** (replace paths and SHA before use):

```bash
git -C /work/rust-slopos worktree add -b agent/ui-02-buttons /work/slopos-worktrees/ui-02-buttons <integration-sha>
```

Use `agent/<task-id>-<description>` and a temporary `integration/<tranche>` branch. The coordinator commits workspace/interface bootstrap first, then publishes that SHA to workers. Dependent tasks start only after their prerequisites integrate; do not independently invent the same API.

Each worker uses its own target directory if the disk reservation allows it. Existing QA scripts often hard-code `target/debug` or `target/release`; they must use the expected worktree-local target until QA-01 makes path handling explicit. Do not set a shared `CARGO_TARGET_DIR` and assume legacy launchers honor it. Sharing Cargo's download cache is acceptable; sharing mutable build outputs across simultaneous workers is not.

Workers deliver small commits. The coordinator reviews diffs, integrates in dependency order, resolves shared-file conflicts and reruns affected integration gates at the integrated SHA. No automatic force-push, broad reset, branch deletion or cleanup of unmerged work. Remove a worktree only after its changes and evidence are preserved. Keep branch/PR work reviewable; merge upstream only when authorized.

## 0.6 VM concurrency and runtime leases

Separate **editing concurrency**, **build concurrency** and **runtime-test concurrency**.

- Start with one build/test permit; cap `CARGO_BUILD_JOBS` explicitly for the guest. Raise parallel build permits only after measuring available memory and aggregate target growth (§29).
- One coordinator-owned runtime lease covers all graphical/Xvfb sessions, global hotkeys, session D-Bus ownership, audio/network/display mutations, package installs and session restart/lock tests in a shared guest.
- At the audited baseline, run existing QA suites **serially in a dedicated test account/session**. Fixed displays, shared output paths and name-based `pkill` make separate worktrees alone insufficient isolation.
- New harnesses must use unique display allocation, per-run XDG/runtime/config/data directories, isolated session buses, explicit child PID/process-group cleanup and unique evidence directories. A private session bus does not isolate the system bus or physical devices.
- Do not run crash/restart QA against a guest desktop someone is using. Retain a recovery console and coordinate any network/display/locker tests that may disconnect the agent.
- Never “clean up” with global `pkill`, `killall`, `cargo clean`, removal of shared caches or another agent's targets. Before using a legacy script with those behaviors, QA-01 must isolate or repair it; until then use only a disposable, exclusive test session.
- The lease records owner, command, guest/session and start time outside versioned product files. Release it after owned children exit; reclaim an abandoned lease only after checking those processes. Parallel runtime testing is allowed only after isolation is demonstrated.

Locks and task messages may be kept in a coordinator's scratch directory outside the checkout. They are transient orchestration state, not another project roadmap.

## 0.7 Evidence and completion

For each gate retain: task ID, source SHA, clean/dirty state, command, exit code, UTC timestamp, guest/hypervisor/OS, toolchain, relevant dependency versions, artifact location/checksum and explicit result. Visual evidence additionally records resolution, scale, fonts, backend and spec/golden revision. Dirty-tree experiments are provisional until rerun from the recorded commit.

Use `PASS`, `FAIL`, `NOT_RUN`, `BLOCKED` and `NOT_APPLICABLE` precisely. A skip needs a reason and cannot become PASS. Fixture-backed protocol tests, real guest-service tests, graphical VM tests and physical-device tests are separate evidence categories. A VM without Wi-Fi, Bluetooth or a backlight can verify unavailability and test fixtures; it cannot prove real-device support.

Avoid the self-referencing-commit trap: test code commit A, then add a documentation-only evidence commit B that explicitly cites A. The coordinator may retain A's evidence if a diff proves B changes only documentation. Never relabel it as a run of B; runtime/build/spec/golden changes require the affected gates again. Release candidates require evidence for the actual candidate code.

Scoped tasks may finish while the overall product remains incomplete. Report partial work as `IMPLEMENTED / NOT VERIFIED`, and external dependencies as `BLOCKED`; do not run indefinitely or invent evidence to satisfy a demand for “100%.” Completion of this contract edit is not completion of SLOPOS.

## 0.8 Reading map

| Work | Read before editing |
|---|---|
| Every task | §0, §§20–21, §§28–30, §33–34 and Part II |
| UI/spec/gallery | §§2, 4–5 (including the §5.2 Design Bible), 7–11, 25, 27, 35 |
| Appkit and native apps | §§6, 12, 15–19, 26, 31, 36 |
| Services, X11 and shell | §§6, 13–14, 18, 22–26, 31 |
| Integration/release | §§28–32, 37 and the complete ledger |

## 0.9 Coordinator launch instruction

Use this as the initial instruction to an agent running with access to the intended VM:

> Read the full contract (`docs/CONTRACT.md`) and the root `AGENTS.md` in this checkout, and audit the current SHA before acting. Coordinate a bounded implementation tranche using §21, starting with VM-00, SPEC-01 and QA-00 where access permits. Use one coordinator and up to three workers with non-overlapping task packets/worktrees. Integrate the bootstrap interfaces before dispatching dependent code. Preserve Rust/X11/Openbox, Platinum Classic, the UI-library-first architecture and the full required inventory. Serialize builds/runtime tests until resource and isolation evidence allows more concurrency. Keep implementing ready tasks, report exact blockers for unavailable dependencies, and update Part II through the coordinator with verified commits and artifacts. Do not claim desktop completion from a task pass.

If a subagent facility is unavailable, perform the same dependency-ready tasks sequentially. The plan does not require a particular model, agent vendor or VM automation tool.
