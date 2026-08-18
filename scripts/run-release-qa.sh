#!/usr/bin/env bash
# SLOPOS-I release evidence runner.
# This script executes objective gates and records their results. It does not
# assign product-readiness or visual scores to itself; TRUTH.md owns that audit.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

SOURCE_SHA="$(git rev-parse HEAD)"
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

run_gate "Rust formatting" cargo fmt --all -- --check
run_gate "Workspace Clippy" bash -c 'cargo clippy --workspace --all-targets --locked -- -D warnings'
run_gate "Workspace tests" cargo test --workspace --locked

if [[ -x scripts/run-clean-install-qa.sh ]]; then
  run_gate "Clean installation and session startup" bash scripts/run-clean-install-qa.sh
fi
if [[ -x scripts/run-catalogue-qa.sh ]]; then
  run_gate "Software Catalogue integrity and lifecycle" bash scripts/run-catalogue-qa.sh
fi
if [[ -x scripts/run-virtual-services-qa.sh ]]; then
  run_gate "Virtual system-service integration" bash scripts/run-virtual-services-qa.sh
fi
if [[ -x scripts/run-settings-service-qa.sh ]]; then
  run_gate "Settings delegated-service integration" bash scripts/run-settings-service-qa.sh
fi
if [[ -x scripts/run-multimonitor-qa.sh ]]; then
  run_gate "Multi-monitor geometry" bash scripts/run-multimonitor-qa.sh
fi
if [[ -x scripts/run-resolution-qa.sh ]]; then
  run_gate "Resolution and scale coverage" bash scripts/run-resolution-qa.sh
fi
if [[ -x scripts/run-recovery-qa.sh ]]; then
  run_gate "Configuration recovery" bash scripts/run-recovery-qa.sh
fi
if [[ -x scripts/run-security-failure-qa.sh ]]; then
  run_gate "Security and failure handling" bash scripts/run-security-failure-qa.sh
fi
if [[ -x scripts/benchmark-x11-session.sh ]]; then
  run_gate "Session performance evidence" bash scripts/benchmark-x11-session.sh
fi
if [[ -x scripts/run-atspi-qa.sh ]]; then
  run_gate "Accessibility tree" bash scripts/run-atspi-qa.sh
fi
if [[ -x scripts/run-debian-package-qa.sh ]]; then
  run_gate "Debian package payload" bash scripts/run-debian-package-qa.sh
fi
if [[ -x scripts/run-canonical-visual-qa.sh ]]; then
  run_gate "Canonical visual evidence capture" bash scripts/run-canonical-visual-qa.sh
fi

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
