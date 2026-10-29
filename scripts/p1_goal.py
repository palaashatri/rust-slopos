#!/usr/bin/env python3
"""Fail-closed, cross-host P1 task controller. Python stdlib only.

Coordinator state lives in qa/p1-progress.json. Scratch artifacts are restricted
to .work/p1/. This tool organizes evidence; it does not replace actual QA.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
GOAL = ROOT / "qa/p1-goal.json"
PROGRESS = ROOT / "qa/p1-progress.json"
SCRATCH = ROOT / ".work/p1"
EVIDENCE = SCRATCH / "evidence"
STATES = {"PLANNED", "CLAIMED", "IMPLEMENTED", "VERIFIED", "INTEGRATED", "BLOCKED"}
PASS_KINDS = {"unit", "visual", "runtime", "security", "package"}
SHA = re.compile(r"[a-f0-9]{40}\Z")
DIGEST = re.compile(r"[a-f0-9]{64}\Z")


def fail(message: str) -> None:
    raise ValueError(message)


def utc() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def read_json(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        fail(f"cannot read valid JSON from {path}: {exc}")
    if not isinstance(data, dict):
        fail(f"{path} must contain a JSON object")
    return data


def manifest() -> dict:
    m = read_json(GOAL)
    if m.get("schema_version") != 1 or not isinstance(m.get("tasks"), list):
        fail("unsupported qa/p1-goal.json schema")
    if len(m["tasks"]) < 10:
        fail("P1 task graph is unexpectedly incomplete")
    seen = set()
    for t in m["tasks"]:
        if not isinstance(t, dict) or not re.fullmatch(r"[A-Z0-9]+(?:-[A-Z0-9]+)*", t.get("id", "")):
            fail("bad P1 task ID")
        if t["id"] in seen:
            fail("duplicate task ID: " + t["id"])
        seen.add(t["id"])
        if not t.get("title") or not t.get("acceptance") or not isinstance(t.get("deps"), list):
            fail(f"missing title/acceptance/deps: {t['id']}")
        proof = t.get("proof")
        if not isinstance(proof, list) or not proof or len(proof) != len(set(proof)) or not set(proof) <= PASS_KINDS:
            fail(f"bad proof requirements: {t['id']}")
    graph = {t["id"]: t["deps"] for t in m["tasks"]}
    visited, visiting = set(), set()

    def visit(task_id: str) -> None:
        if task_id not in graph:
            fail(f"unknown dependency {task_id}")
        if task_id in visiting:
            fail(f"dependency cycle at {task_id}")
        if task_id in visited:
            return
        visiting.add(task_id)
        for dep in graph[task_id]:
            if dep == task_id:
                fail(f"self dependency at {task_id}")
            visit(dep)
        visiting.remove(task_id)
        visited.add(task_id)

    for task_id in graph:
        visit(task_id)
    settings = m.get("settings", {})
    for field in ("min_free_gib", "max_target_gib", "max_evidence_mib",
                  "max_agents", "max_parallel_cargo_builds", "max_shared_x11_sessions", "max_worktrees"):
        if type(settings.get(field)) is not int or settings[field] <= 0:
            fail("invalid positive resource budget: " + field)
    return m


def progress(m: dict) -> dict:
    if not PROGRESS.exists():
        return {"schema_version": 1, "tasks": {}}
    state = read_json(PROGRESS)
    if state.get("schema_version") != 1 or not isinstance(state.get("tasks"), dict):
        fail("unsupported progress schema")
    tasks = {t["id"] for t in m["tasks"]}
    for task_id, info in state["tasks"].items():
        if task_id not in tasks or not isinstance(info, dict) or info.get("status") not in STATES:
            fail(f"invalid progress record: {task_id}")
    return state


def write_progress(state: dict) -> None:
    PROGRESS.parent.mkdir(parents=True, exist_ok=True)
    state["updated_utc"] = utc()
    fd, temporary = tempfile.mkstemp(prefix=".p1-progress-", suffix=".json", dir=PROGRESS.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as out:
            json.dump(state, out, indent=2, sort_keys=True)
            out.write("\n")
            out.flush()
            os.fsync(out.fileno())
        os.replace(temporary, PROGRESS)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def current_sha() -> str:
    try:
        result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                check=True, timeout=8)
        commit = result.stdout.strip()
        if SHA.fullmatch(commit):
            return commit
    except (OSError, subprocess.SubprocessError):
        pass
    fail("cannot determine exact Git HEAD; open a complete repository checkout")


def task_status(state: dict, task_id: str) -> str:
    return state["tasks"].get(task_id, {}).get("status", "PLANNED")


def dependency_ready(t: dict, state: dict) -> bool:
    return all(task_status(state, dep) == "INTEGRATED" for dep in t["deps"])


def next_tasks(m: dict, state: dict) -> list[dict]:
    return [t for t in m["tasks"] if task_status(state, t["id"]) == "PLANNED" and dependency_ready(t, state)]


def bytes_under(folder: Path) -> int:
    if not folder.is_dir() or folder.is_symlink():
        return 0
    size = 0
    for parent, _, files in os.walk(folder, followlinks=False):
        for f in files:
            p = Path(parent, f)
            try:
                if not p.is_symlink():
                    size += p.stat().st_size
            except OSError:
                pass
    return size


def disk_check(m: dict) -> list[str]:
    settings = m["settings"]
    base = SCRATCH if SCRATCH.exists() else ROOT
    usage = shutil.disk_usage(base)
    target_bytes = bytes_under(SCRATCH / "target")
    evidence_bytes = bytes_under(EVIDENCE)
    issues = []
    if usage.free < settings["min_free_gib"] * (1024 ** 3):
        issues.append(f"free space {usage.free / 1024**3:.1f} GiB is below {settings['min_free_gib']} GiB")
    if target_bytes > settings["max_target_gib"] * (1024 ** 3):
        issues.append(f"P1 target dir {target_bytes / 1024**3:.1f} GiB exceeds budget")
    if evidence_bytes > settings["max_evidence_mib"] * (1024 ** 2):
        issues.append(f"P1 evidence {evidence_bytes / 1024**2:.1f} MiB exceeds budget")
    print(f"Disk: {usage.free / 1024**3:.1f} GiB free; P1 target {target_bytes / 1024**3:.2f} GiB; "
          f"evidence {evidence_bytes / 1024**2:.1f} MiB")
    if issues:
        for issue in issues:
            print("BLOCKED disk:", issue)
    return issues


def safe_evidence_path(raw: str) -> Path:
    if not isinstance(raw, str) or not raw:
        fail("evidence artifact path is missing")
    p = (ROOT / raw).resolve()
    root = EVIDENCE.resolve()
    if not p.is_relative_to(root) or not p.is_file() or p.is_symlink():
        fail("artifact must be an existing regular file under .work/p1/evidence")
    if p.stat().st_size > 80 * 1024 * 1024:
        fail("one evidence artifact exceeds 80 MiB; downsample or retain a small representative sample")
    return p


def check_evidence(path: Path, requirements: list[str]) -> dict:
    if not path.is_file() or path.stat().st_size > 64 * 1024:
        fail("evidence JSON must exist and be <=64 KiB")
    data = read_json(path)
    if not SHA.fullmatch(str(data.get("source_sha", ""))):
        fail("evidence needs an exact 40-character Git SHA")
    if data["source_sha"] != current_sha():
        fail("evidence SHA does not match the checkout HEAD; retest the exact code")
    checks = data.get("checks")
    if not isinstance(checks, list):
        fail("evidence needs checks list")
    by_kind = {}
    for check in checks:
        if not isinstance(check, dict) or check.get("kind") not in PASS_KINDS:
            fail("unknown evidence kind")
        if check["kind"] in by_kind or check.get("result") != "PASS":
            fail("duplicate or non-PASS proof cannot verify a task")
        if not isinstance(check.get("command"), str) or not check["command"].strip():
            fail("evidence requires the actual executed command")
        artifact = safe_evidence_path(check.get("artifact"))
        expected = str(check.get("sha256", ""))
        if not DIGEST.fullmatch(expected):
            fail("artifact SHA-256 must be a lowercase 64-digit digest")
        actual = hashlib.sha256(artifact.read_bytes()).hexdigest()
        if actual != expected:
            fail(f"artifact bytes changed: {artifact}")
        if check["kind"] == "visual":
            if not check.get("review") or not check.get("resolution"):
                fail("visual proof needs an explicit screenshot review and resolution")
            if artifact.suffix.lower() != ".png" or not artifact.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
                fail("visual proof must reference a real PNG capture, not a text log")
        by_kind[check["kind"]] = check
    missing = set(requirements) - set(by_kind)
    if missing:
        fail("required proof missing: " + ", ".join(sorted(missing)))
    return data


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("validate", help="validate task graph and progress schema")
    sub.add_parser("status", help="show states and next ready work")
    sub.add_parser("next", help="output ready task packets as JSON")
    sub.add_parser("preflight", help="fail if the disk budget is unsafe")
    sub.add_parser("host-check", help="detect macOS/Windows/Linux tools without installing anything")
    claim = sub.add_parser("claim", help="claim one dependency-ready task")
    claim.add_argument("id")
    record = sub.add_parser("record", help="checkpoint a task (single coordinator only)")
    record.add_argument("id")
    record.add_argument("--to", required=True, choices=["IMPLEMENTED", "VERIFIED", "INTEGRATED", "BLOCKED"])
    record.add_argument("--summary", required=True)
    record.add_argument("--evidence", help="local JSON with SHA and real test/screenshot logs")
    sub.add_parser("release-check", help="fail closed if any required task/proof is incomplete")
    cleanup = sub.add_parser("cleanup", help="preview or explicitly remove ONLY P1 temp/build output")
    cleanup.add_argument("--area", choices=["tmp", "target"], default="tmp")
    cleanup.add_argument("--confirm", action="store_true")
    args = ap.parse_args()
    m = manifest()
    state = progress(m)
    task_map = {t["id"]: t for t in m["tasks"]}
    if args.command == "validate":
        print(f"VALID: {len(task_map)} acyclic P1 tasks, {len(state['tasks'])} progress records")
        return 0
    if args.command == "status":
        for t in m["tasks"]:
            st = task_status(state, t["id"])
            ready = " READY" if st == "PLANNED" and dependency_ready(t, state) else ""
            print(f"{t['id']:14s} {st:11s}{ready} | {t['title']}")
        return 0
    if args.command == "next":
        print(json.dumps(next_tasks(m, state), indent=2))
        return 0
    if args.command == "host-check":
        print(f"Host: {platform.system()} {platform.machine()} / Python {platform.python_version()}")
        for binary in ("git", "cargo", "rustc", "utmctl", "VBoxManage",
                       "qemu-system-aarch64", "qemu-system-x86_64", "wsl.exe", "brew", "winget"):
            found = shutil.which(binary)
            if found:
                print(f"  {binary}: {found}")
        print("Required: Git and Python 3.10+ on host; disposable Linux VM for all project builds/GUI QA.")
        print("No host software, VMs, toolchains, or packages have been installed.")
        return 2 if disk_check(m) else 0
    if args.command == "preflight":
        return 2 if disk_check(m) else 0
    if args.command == "claim":
        if args.id not in task_map:
            fail("unknown task: " + args.id)
        t = task_map[args.id]
        if task_status(state, args.id) not in ("PLANNED", "BLOCKED") or not dependency_ready(t, state):
            fail("task is not ready or has unmet dependencies")
        if disk_check(m):
            fail("disk preflight failed; do not build or create a VM")
        state["tasks"][args.id] = {"status": "CLAIMED", "base_sha": current_sha(), "updated_utc": utc()}
        write_progress(state)
        print(f"CLAIMED {args.id}: {t['acceptance']}")
        return 0
    if args.command == "record":
        if args.id not in task_map:
            fail("unknown task: " + args.id)
        previous = task_status(state, args.id)
        allowed = {
            "CLAIMED": {"IMPLEMENTED", "BLOCKED"},
            "IMPLEMENTED": {"VERIFIED", "BLOCKED"},
            "VERIFIED": {"INTEGRATED", "BLOCKED"},
        }
        if args.to not in allowed.get(previous, set()):
            fail(f"illegal status transition {previous} -> {args.to}")
        if not args.summary.strip():
            fail("summary must explain work or the actual blocker")
        old = state["tasks"][args.id]
        updated = {**old, "status": args.to, "summary": args.summary, "updated_utc": utc()}
        if args.to == "VERIFIED":
            if not args.evidence:
                fail("cannot verify without file-backed test/visual evidence")
            evidence = check_evidence(Path(args.evidence), task_map[args.id]["proof"])
            updated["evidence"] = evidence
        if args.to == "INTEGRATED" and "evidence" not in old:
            fail("cannot integrate a task with no verified evidence")
        state["tasks"][args.id] = updated
        write_progress(state)
        print(f"{args.id}: {previous} -> {args.to}")
        return 0
    if args.command == "release-check":
        missing = [t["id"] for t in m["tasks"] if task_status(state, t["id"]) != "INTEGRATED"]
        if missing:
            print("NOT READY: missing integrated task proofs: " + ", ".join(missing))
            return 2
        if disk_check(m):
            return 2
        for t in m["tasks"]:
            record = state["tasks"][t["id"]]
            proof = record.get("evidence")
            if not proof or set(t["proof"]) - {c["kind"] for c in proof["checks"]}:
                fail("missing evidence in " + t["id"])
        print("Task ledger complete. NOT a release verdict: now run the Linux VM release runner")
        print("  bash scripts/run-release-qa.sh")
        print("and independently review its screenshots, package output and hardware NOT_RUN items.")
        return 0
    if args.command == "cleanup":
        target = SCRATCH / args.area
        if target.is_symlink() or not target.resolve().is_relative_to(SCRATCH.resolve()):
            fail("unsafe cleanup path")
        size = bytes_under(target)
        print(f"Cleanup target: {target}; reclaimable {size / 1024**2:.1f} MiB")
        print("Protected: all evidence, git history, user files, VMs, downloads, other worktrees")
        if args.confirm and target.is_dir():
            shutil.rmtree(target)
            print("Removed only: " + str(target))
        else:
            print("DRY RUN: no files removed; add --confirm for this exact scoped path")
        return 0
    return 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError) as exc:
        print(f"P1 controller BLOCKED: {exc}", file=sys.stderr)
        sys.exit(2)
