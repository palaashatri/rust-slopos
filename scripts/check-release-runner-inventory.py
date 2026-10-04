from __future__ import annotations

import argparse
import json
import re
import stat
import sys
from pathlib import Path
from typing import Any


MANIFEST_RELATIVE_PATH = Path("qa/release-runner-inventory.json")
RUNNER_RELATIVE_PATH = Path("scripts/run-release-qa.sh")
GATE_ID = re.compile(r"[a-z][a-z0-9]*(?:-[a-z0-9]+)*")
GATE_LABEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9 -]*")
RUNNER_PATH = re.compile(r"scripts/[A-Za-z0-9][A-Za-z0-9._-]*\.sh")
GATE_CALLS_MARKER = "__REQUIRED_GATE_CALLS__"
CORE_GATE_CALLS = (
    'run_gate "Required release runner inventory" python3 '
    'scripts/check-release-runner-inventory.py --root "$REPO_ROOT"',
    'run_gate "Rust formatting" cargo fmt --all -- --check',
    'run_gate "Workspace Clippy" bash -c '
    "'cargo clippy --workspace --all-targets --locked -- -D warnings'",
    'run_gate "Workspace tests" cargo test --workspace --locked',
)
EXPECTED_RUNNER_TEMPLATE = r'''#!/usr/bin/env bash
# SLOPOS-I release evidence runner.
# This script executes objective gates and records their results. It does not
# assign product-readiness or visual scores to itself; AGENTS.md Part II owns that audit.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

SOURCE_SHA="${SOURCE_SHA:-$(git rev-parse HEAD 2>/dev/null || printf '%s' unknown)}"
STARTED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
OUT_DIR="$REPO_ROOT/artifacts/qa/release"
REPORT_FILE="$OUT_DIR/report.md"
mkdir -p "$OUT_DIR"

PASS_COUNT=0
TOTAL_COUNT=0
CURRENT_GATE=""

run_gate() {
  local name="$1"
  shift
  TOTAL_COUNT=$((TOTAL_COUNT + 1))
  CURRENT_GATE="$name"
  printf '\n>>> %s\n' "$name"
  "$@"
  PASS_COUNT=$((PASS_COUNT + 1))
  printf 'PASS: %s\n' "$name"
}

on_error() {
  local status=$?
  {
    printf '# SLOPOS-I Release QA Evidence\n\n'
    printf -- '- Source commit: `%s`\n' "$SOURCE_SHA"
    printf -- '- Started: %s\n' "$STARTED_UTC"
    printf -- '- Finished: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf -- '- Result: **FAIL**\n'
    printf -- '- Failed gate: `%s`\n' "$CURRENT_GATE"
    printf -- '- Completed gates: %d / %d attempted\n' "$PASS_COUNT" "$TOTAL_COUNT"
    printf '\nThis report is execution evidence only. It is not a product-readiness score.\n'
  } > "$REPORT_FILE"
  cat "$REPORT_FILE"
  exit "$status"
}
trap on_error ERR

printf 'SLOPOS-I release QA\nSource: %s\n' "$SOURCE_SHA"

__REQUIRED_GATE_CALLS__

FINISHED_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat > "$REPORT_FILE" <<EOF
# SLOPOS-I Release QA Evidence

- Source commit: \`$SOURCE_SHA\`
- Started: $STARTED_UTC
- Finished: $FINISHED_UTC
- Result: **PASS**
- Objective gates completed: **$PASS_COUNT / $TOTAL_COUNT**

All commands selected by this runner completed successfully. This report is execution evidence only; it does **not** prove consumer release readiness, cross-architecture support, package-repository availability, hardware compatibility or a visual-quality score.

Visual screenshots, when captured, are stored under \`artifacts/qa/canonical-visual/\` for independent review.
EOF

cat "$REPORT_FILE"
printf '\nRELEASE_QA_EVIDENCE_OK\n'
'''


def render_expected_runner(runners: list[tuple[str, str]]) -> str:
    gate_calls = "\n".join(CORE_GATE_CALLS)
    if runners:
        manifest_calls = "\n".join(
            f'run_gate "{label}" bash {script}' for label, script in runners
        )
        gate_calls += "\n\n" + manifest_calls
    return EXPECTED_RUNNER_TEMPLATE.replace(
        GATE_CALLS_MARKER, gate_calls
    )


def reject_duplicate_json_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON object key {key!r}")
        result[key] = value
    return result


def validate_release_runner_inventory(root: Path) -> list[str]:
    root = root.resolve(strict=True)
    manifest_path = root / MANIFEST_RELATIVE_PATH
    runner_path = root / RUNNER_RELATIVE_PATH
    manifest: Any = json.loads(
        manifest_path.read_text(encoding="utf-8"),
        object_pairs_hook=reject_duplicate_json_keys,
    )
    runner_source = runner_path.read_text(encoding="utf-8")
    errors: set[str] = set()

    if not isinstance(manifest, dict):
        return ["release runner inventory must be a JSON object"]
    if set(manifest) != {"schema_version", "runners"}:
        errors.add(
            "release runner inventory must contain only schema_version and runners"
        )
    if type(manifest.get("schema_version")) is not int or manifest.get(
        "schema_version"
    ) != 1:
        errors.add("release runner inventory schema_version must be 1")

    runner_entries = manifest.get("runners")
    if not isinstance(runner_entries, list) or not runner_entries:
        return sorted(
            errors | {"release runner inventory runners must be a non-empty list"}
        )

    seen_ids: set[str] = set()
    seen_scripts: set[str] = set()
    expected_calls: list[tuple[str, str]] = []
    for index, entry in enumerate(runner_entries):
        if not isinstance(entry, dict) or set(entry) != {"id", "label", "script"}:
            errors.add(f"runner entry {index} must contain id, label, and script")
            continue

        gate_id = entry["id"]
        label = entry["label"]
        script = entry["script"]
        if not isinstance(gate_id, str) or GATE_ID.fullmatch(gate_id) is None:
            errors.add(f"runner entry {index} has an invalid id")
        elif gate_id in seen_ids:
            errors.add(f"release runner inventory repeats id {gate_id!r}")
        else:
            seen_ids.add(gate_id)

        valid_label = (
            isinstance(label, str)
            and label == label.strip()
            and GATE_LABEL.fullmatch(label) is not None
        )
        if not valid_label:
            errors.add(f"runner entry {index} has an invalid label")

        if not isinstance(script, str) or RUNNER_PATH.fullmatch(script) is None:
            errors.add(f"runner entry {index} has an unsafe script path")
            continue

        if script in seen_scripts:
            errors.add(f"release runner inventory repeats script {script!r}")
        else:
            seen_scripts.add(script)

        candidate = root / script
        current_path = root
        has_symlink_component = False
        for component in Path(script).parts:
            current_path /= component
            has_symlink_component |= current_path.is_symlink()
        if has_symlink_component:
            errors.add(f"{script}: symlinked release runner paths are not supported")
        elif not candidate.is_file():
            errors.add(f"{script}: required release runner is missing or not a file")
        elif (
            candidate.stat().st_mode
            & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        ) == 0:
            errors.add(f"{script}: required release runner is not executable")

        if valid_label:
            expected_calls.append((label, script))

    expected_source = render_expected_runner(expected_calls)
    if runner_source != expected_source:
        errors.add(
            "release runner does not match the verified static template and inventory"
        )

    return sorted(errors)


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate required release QA runners")
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root containing qa/ and scripts/",
    )
    arguments = parser.parse_args()

    try:
        errors = validate_release_runner_inventory(arguments.root)
    except (OSError, ValueError, UnicodeError) as error:
        print(f"Cannot validate release runner inventory: {error}", file=sys.stderr)
        return 2

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(
        "Release runner inventory passed: all registered scripts are required "
        "and executable."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
