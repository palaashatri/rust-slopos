#!/usr/bin/env bash
# P1 release acceptance *precondition*: only run in a disposable Linux guest.
# This cannot replace real screenshot, accessibility or hardware review.
set -Eeuo pipefail
cd -- "$(dirname -- "$0")/.."

if [[ "$(uname -s)" != "Linux" || "${SLOPOS_P1_GUEST:-}" != "1" ]]; then
  echo >&2 "BLOCKED: only run inside an explicitly selected disposable Linux VM with SLOPOS_P1_GUEST=1"
  exit 2
fi
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  echo >&2 "BLOCKED: source checkout has uncommitted tracked edits; verify an exact commit first"
  exit 2
fi

python3 scripts/p1_goal.py validate
python3 scripts/p1_goal.py release-check || {
  echo >&2 "BLOCKED: missing P1 integrated task/evidence; not executing expensive release QA"
  exit 2
}
python3 scripts/p1_goal.py preflight || {
  echo >&2 "BLOCKED: insufficient guest disk headroom"
  exit 2
}

# Store all build outputs in a bounded, guest-local single target.
export CARGO_TARGET_DIR="$PWD/.work/p1/target"
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0

bash scripts/run-release-qa.sh
python3 scripts/p1_goal.py preflight

echo "OBJECTIVE_GATES_RAN: evidence is in artifacts/qa/release/"
echo "INDEPENDENT_REVIEW_REQUIRED: inspect installed app journeys, screenshots,"
echo " accessibility, hardware NOT_RUN records and final package artifacts."
echo "Do not claim P1 shipped on the strength of this script alone."
