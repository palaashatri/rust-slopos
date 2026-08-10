#!/usr/bin/env bash
# In-guest QA for the SLOPOS-I VM: the first real DRM/KMS run of
# slopos-compositor. Fetch and run from the guest console:
#   curl -sL http://10.0.2.2:8000/qa-vm.sh | bash
#
# Installs the host's SSH key (so later QA can drive the VM over ssh),
# pulls the latest branch, rebuilds, then exercises the compositor on the
# real seat with libinput + KMS.
set -u
REPO=/home/retro/slopos-i
QA=/home/retro/qa
QA_BRANCH="${QA_BRANCH:-${REPO_BRANCH:-main}}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/home/retro/.cache/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
BIN="$CARGO_TARGET_DIR/release"
case "$CARGO_TARGET_DIR" in
  /*) ;;
  *) echo "CARGO_TARGET_DIR must be an absolute path: $CARGO_TARGET_DIR" >&2; exit 2 ;;
esac
mkdir -p "$QA"
exec > >(tee "$QA/qa-vm.log") 2>&1
step() { echo; echo "=== [$(date +%H:%M:%S)] $* ==="; }

step "install host SSH key for unattended QA"
mkdir -p /home/retro/.ssh && chmod 700 /home/retro/.ssh
if [ -n "${QA_PUBKEY:-}" ]; then
  echo "$QA_PUBKEY" >> /home/retro/.ssh/authorized_keys
elif curl -fsS http://10.0.2.2:8000/qa_key.pub -o /tmp/qa_key.pub 2>/dev/null; then
  cat /tmp/qa_key.pub >> /home/retro/.ssh/authorized_keys
fi
sort -u /home/retro/.ssh/authorized_keys -o /home/retro/.ssh/authorized_keys 2>/dev/null || true
chmod 600 /home/retro/.ssh/authorized_keys 2>/dev/null || true
chown -R retro:retro /home/retro/.ssh
wc -l < /home/retro/.ssh/authorized_keys 2>/dev/null | xargs echo "authorized_keys lines:"

step "environment facts"
uname -r
ls -l /dev/dri/ 2>&1
lsmod | grep -E "vmwgfx|drm" | head -5
systemctl is-active seatd
loginctl show-seat seat0 2>/dev/null | head -5 || true

step "pull latest + rebuild"
cd "$REPO"
if ! sudo -u retro git fetch --all -q > "$QA/git-fetch.log" 2>&1; then
  echo "git fetch failed" >&2
  tail -20 "$QA/git-fetch.log" >&2
  echo "QA_VM_STAGE1_DONE=NO"
  exit 1
fi
if ! sudo -u retro git checkout -q "$QA_BRANCH"; then
  echo "requested QA branch is unavailable: $QA_BRANCH" >&2
  echo "QA_VM_STAGE1_DONE=NO"
  exit 1
fi
if ! sudo -u retro git pull -q --ff-only > "$QA/git-pull.log" 2>&1; then
  echo "git pull --ff-only failed" >&2
  tail -20 "$QA/git-pull.log" >&2
  echo "QA_VM_STAGE1_DONE=NO"
  exit 1
fi
tail -2 "$QA/git-pull.log"
sudo -u retro git log --oneline -1
if sudo -u retro env HOME=/home/retro CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
  cargo build --release --workspace --locked > "$QA/build.log" 2>&1; then
  BUILD_EXIT=0
else
  BUILD_EXIT=$?
fi
echo "BUILD_EXIT=$BUILD_EXIT"
grep -E "^error" -A 6 "$QA/build.log" | head -20
if [ "$BUILD_EXIT" -ne 0 ]; then
  echo "QA_VM_STAGE1_DONE=NO"
  exit "$BUILD_EXIT"
fi

step "unit tests on real hardware"
if sudo -u retro env HOME=/home/retro CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
  cargo test --release --workspace --locked > "$QA/test.log" 2>&1; then
  TEST_EXIT=0
else
  TEST_EXIT=$?
fi
echo "TEST_EXIT=$TEST_EXIT"
grep -E "^test result" "$QA/test.log" | awk '{p+=$4; f+=$6} END {print "TOTAL passed="p" failed="f}'
if [ "$TEST_EXIT" -ne 0 ]; then
  echo "QA_VM_STAGE1_DONE=NO"
  exit "$TEST_EXIT"
fi

echo "QA_VM_STAGE1_DONE=YES"
