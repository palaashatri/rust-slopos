#!/usr/bin/env bash
# Run Stage 4 VM tests on multiple systems
# This script handles Arch and Ubuntu VMs for distribution chain validation

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SSH_KEY="${SSH_KEY:-$HOME/.ssh/id_rsa}"
SSH_TIMEOUT=30

# VMs to test (override with environment)
declare -a VMS=(
    "${VM_ARCH:-192.168.64.20:arch-test}"
    "${VM_UBUNTU:-192.168.64.21:ubuntu-test}"
)

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $*"
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Test SSH connectivity to a VM
test_vm_connectivity() {
    local vm_addr="$1"
    log_info "Testing SSH connectivity to $vm_addr..."

    if timeout $SSH_TIMEOUT ssh -i "$SSH_KEY" -o StrictHostKeyChecking=no "root@${vm_addr%:*}" \
        "echo 'SSH connection successful'" &>/dev/null; then
        log_pass "Connected to $vm_addr"
        return 0
    else
        log_fail "Cannot connect to $vm_addr"
        return 1
    fi
}

# Copy repo to VM and run tests
run_vm_tests() {
    local vm_spec="$1"
    local vm_addr="${vm_spec%:*}"
    local vm_name="${vm_spec#*:}"

    log_info "Starting Stage 4 tests on $vm_name ($vm_addr)..."

    # Copy repo to VM
    log_info "Syncing code to $vm_addr:/root/slopos-i..."
    rsync -az -e "ssh -i $SSH_KEY -o StrictHostKeyChecking=no" \
        --exclude=target --exclude=.git \
        "$REPO_ROOT/" "root@$vm_addr:/root/slopos-i/" || {
        log_fail "Failed to sync code to $vm_addr"
        return 1
    }

    # Run stage-4-verify.sh on the VM
    log_info "Running verification tests on $vm_name..."
    ssh -i "$SSH_KEY" -o StrictHostKeyChecking=no "root@$vm_addr" \
        "cd /root/slopos-i && bash packaging/vm/stage-4-verify.sh '$vm_name'" || {
        log_fail "Tests failed on $vm_name"
        return 1
    }

    # Retrieve results
    log_info "Collecting results from $vm_name..."
    mkdir -p /tmp/stage4-results
    scp -i "$SSH_KEY" -o StrictHostKeyChecking=no \
        "root@$vm_addr:/tmp/stage4-results-*.txt" \
        "/tmp/stage4-results/$vm_name-results.txt" 2>/dev/null || true

    log_pass "Tests completed on $vm_name"
    return 0
}

# Summarize all results
summarize_results() {
    log_info "Collecting final results..."

    if [ ! -d /tmp/stage4-results ]; then
        log_warn "No results directory found"
        return 1
    fi

    echo ""
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo -e "${BLUE}Stage 4 VM Test Results${NC}"
    echo -e "${BLUE}════════════════════════════════════════${NC}"

    local total_pass=0
    local total_fail=0

    for result_file in /tmp/stage4-results/*.txt; do
        if [ -f "$result_file" ]; then
            echo ""
            echo -e "${YELLOW}$(basename "$result_file")${NC}"
            echo "───────────────────────────────────────"

            # Count PASS/FAIL
            pass_count=$(grep -c "^PASS:" "$result_file" 2>/dev/null || echo 0)
            fail_count=$(grep -c "^FAIL:" "$result_file" 2>/dev/null || echo 0)

            echo "Passed: $pass_count"
            echo "Failed: $fail_count"

            total_pass=$((total_pass + pass_count))
            total_fail=$((total_fail + fail_count))

            # Show first failure if any
            if [ "$fail_count" -gt 0 ]; then
                echo ""
                echo "First failure:"
                grep "^FAIL:" "$result_file" | head -1 | sed 's/^FAIL: /  /'
            fi
        fi
    done

    echo ""
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo -e "Total Passed: ${GREEN}$total_pass${NC}"
    echo -e "Total Failed: ${RED}$total_fail${NC}"
    echo -e "${BLUE}════════════════════════════════════════${NC}"
    echo ""

    if [ "$total_fail" -eq 0 ]; then
        log_pass "All Stage 4 tests passed!"
        return 0
    else
        log_fail "Some tests failed"
        return 1
    fi
}

# Main execution
main() {
    log_info "Stage 4 VM Test Suite"
    log_info "Repository: $REPO_ROOT"
    echo ""

    local all_passed=true

    # Test each VM
    for vm_spec in "${VMS[@]}"; do
        if [ -z "$vm_spec" ]; then
            continue
        fi

        if test_vm_connectivity "${vm_spec%:*}"; then
            if ! run_vm_tests "$vm_spec"; then
                all_passed=false
            fi
        else
            log_warn "Skipping $vm_spec (unreachable)"
            all_passed=false
        fi
        echo ""
    done

    # Summary
    if ! summarize_results; then
        all_passed=false
    fi

    if [ "$all_passed" = true ]; then
        exit 0
    else
        exit 1
    fi
}

main "$@"
