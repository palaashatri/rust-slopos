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
BIN=$REPO/target/release
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
sudo -u retro git fetch --all -q
sudo -u retro git checkout -q fix/qa-2026-07-26-compositor-and-panics 2>/dev/null || true
sudo -u retro git pull -q --ff-only 2>&1 | tail -2
sudo -u retro git log --oneline -1
sudo -u retro cargo build --release --workspace > "$QA/build.log" 2>&1
echo "BUILD_EXIT=$?"
grep -E "^error" -A 6 "$QA/build.log" | head -20

step "unit tests on real hardware"
sudo -u retro cargo test --release --workspace > "$QA/test.log" 2>&1
echo "TEST_EXIT=$?"
grep -E "^test result" "$QA/test.log" | awk '{p+=$4; f+=$6} END {print "TOTAL passed="p" failed="f}'

echo "QA_VM_STAGE1_DONE"
