#!/bin/sh
# Remove only SLOPOS build and explicitly-owned QA scratch data.
#
# The default is a read-only inventory. Pass --apply to remove the repository's
# local target tree and artifacts/qa/coordination/scratch. The allow-list is
# deliberately narrow: this script never follows a caller-supplied path and
# never removes user data, Cargo's registry/git cache, or retained QA evidence.

set -eu

SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd "$SCRIPT_DIR/.." && pwd)
TARGET_DIR=${CARGO_TARGET_DIR:-"$REPO_ROOT/target"}
SCRATCH_DIR="$REPO_ROOT/artifacts/qa/coordination/scratch"
APPLY=0

if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--apply" ]; }; then
  printf '%s\n' "usage: $0 [--apply]" >&2
  exit 2
fi
[ "$#" -eq 1 ] && APPLY=1

is_owned_target() {
  case "$1" in
    "$REPO_ROOT"/target|"$REPO_ROOT"/target/*) return 0 ;;
    "$HOME"/.cache/slopos-i/cargo-target|"$HOME"/.cache/slopos-i/cargo-target/*) return 0 ;;
    *) return 1 ;;
  esac
}

report_path() {
  path=$1
  if [ -e "$path" ]; then
    size_blocks=$(du -s "$path" 2>/dev/null | awk '{print $1}')
    printf '%s\t%s blocks\n' "$path" "${size_blocks:-unknown}"
  else
    printf '%s\tabsent\n' "$path"
  fi
}

printf '%s\n' "SLOPOS owned-artifact cleanup (apply=$APPLY)"
report_path "$TARGET_DIR"
report_path "$SCRATCH_DIR"

if [ "$APPLY" -eq 1 ]; then
  if ! is_owned_target "$TARGET_DIR"; then
    printf '%s\n' "refusing to remove non-allow-listed CARGO_TARGET_DIR: $TARGET_DIR" >&2
    exit 1
  fi
  if [ -e "$TARGET_DIR" ]; then
    rm -rf "$TARGET_DIR"
    printf '%s\n' "removed $TARGET_DIR"
  fi
  if [ -e "$SCRATCH_DIR" ]; then
    rm -rf "$SCRATCH_DIR"
    printf '%s\n' "removed $SCRATCH_DIR"
  fi
fi
