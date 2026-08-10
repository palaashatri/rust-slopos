#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Reproducible source/build/test gate for the SLOPOS-I compositor.
#
# This script deliberately does not claim runtime or hardware verification.
# It produces a machine-readable record for the exact checked-out commit and
# exits non-zero on the first failed source/build/test condition.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  printf 'verify-compositor-completion: Linux is required\n' >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in cargo git grep sed date; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'verify-compositor-completion: missing required tool: %s\n' "$tool" >&2
    exit 2
  fi
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
artifact_dir="${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/compositor-contract}"
mkdir -p "$artifact_dir"
artifact="$artifact_dir/${commit_sha}.json"
log="$artifact_dir/${commit_sha}.log"

# Refuse to record a clean result for a dirty checkout: evidence must identify
# the exact source that was tested.
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  printf 'verify-compositor-completion: tracked working tree is dirty\n' >&2
  exit 2
fi

status="running"
failed_step=""
started_epoch="$(date +%s)"

write_artifact() {
  local finished_epoch duration
  finished_epoch="$(date +%s)"
  duration="$((finished_epoch - started_epoch))"
  cat >"$artifact.tmp" <<JSON
{
  "schema": 1,
  "component": "slopos-compositor",
  "commit": "$commit_sha",
  "branch": "$branch",
  "started_at_utc": "$timestamp",
  "duration_seconds": $duration,
  "status": "$status",
  "failed_step": "$failed_step",
  "evidence_level": "build_and_test_only",
  "runtime_verified": false,
  "hardware_verified": false,
  "log": "$(basename "$log")"
}
JSON
  mv "$artifact.tmp" "$artifact"
}

on_error() {
  local exit_code=$?
  status="failed"
  write_artifact
  printf 'verify-compositor-completion: failed at %s (exit %d)\n' "$failed_step" "$exit_code" >&2
  exit "$exit_code"
}
trap on_error ERR

exec > >(tee "$log") 2>&1

printf 'SLOPOS-I compositor completion gate\n'
printf 'commit=%s\nbranch=%s\nstarted=%s\n' "$commit_sha" "$branch" "$timestamp"

failed_step="forbidden third-party compositor fallback scan"
if grep -RInE --exclude-dir=.git --exclude='*.md' \
  '(exec|spawn|command|Command::new)[^\n]*(labwc|sway|weston)' \
  crates/slopos-session scripts packaging 2>/dev/null; then
  printf 'third-party compositor fallback command found\n' >&2
  false
fi

failed_step="compositor package check"
cargo check -p slopos-compositor --all-targets --locked

failed_step="compositor contract tests"
cargo test -p slopos-compositor --all-targets --locked

failed_step="compositor clippy"
cargo clippy -p slopos-compositor --all-targets --locked

failed_step="release compositor build"
cargo build -p slopos-compositor --release --locked

failed_step="session supervisor package check"
cargo check -p slopos-session --all-targets --locked

failed_step="presentation and Spaces contract presence"
test -f crates/slopos-compositor/tests/compositor_completion_contract.rs
grep -q 'presentation_round_trip_preserves_the_original_normal_frame' \
  crates/slopos-compositor/tests/compositor_completion_contract.rs
grep -q 'independent_display_spaces_migrate_without_changing_identity_or_order' \
  crates/slopos-compositor/tests/compositor_completion_contract.rs

failed_step="per-output layer-shell ownership contract"
grep -q 'output_index: usize' crates/slopos-compositor/src/main.rs
grep -q 'Output::from_resource' crates/slopos-compositor/src/main.rs
grep -q 'sync_surface_to_output' crates/slopos-compositor/src/main.rs
grep -q 'intersecting_output_indices' crates/slopos-compositor/src/output_assignment.rs

failed_step="runtime output topology contract"
grep -q 'ReconfigureOutputs' crates/slopos-bus/src/session_control.rs
grep -q 'validated_runtime_output_layout' crates/slopos-compositor/src/output_assignment.rs
grep -q 'disable_global::<SloposCompositor>' crates/slopos-compositor/src/main.rs
grep -q 'runtime output topology applied' crates/slopos-compositor/src/main.rs
test -x scripts/verify-compositor-output-topology-runtime.sh

status="passed"
failed_step=""
write_artifact
trap - ERR

printf '\nBuild/test gate passed for %s\n' "$commit_sha"
printf 'Evidence: %s\n' "$artifact"
printf 'This result does not prove runtime interaction, hardware output, HDR, VRR, or external-client compatibility.\n'
