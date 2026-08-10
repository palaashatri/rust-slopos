#!/usr/bin/env bash
# Task 3.10 — VM DoD verification script
# Installs a .app via the store and verifies it appears + launches on the VM
# Run this ON THE VM after Stage 3 code is synced

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Task 3.10 — Stage 3 DoD Verification ===${NC}"

# Step 1: Build the bundles
echo -e "${YELLOW}Step 1: Building all .app bundles...${NC}"
OUTDIR=$HOME/Applications bash packaging/apps/build-all-bundles.sh

# Step 2: Prepare one app for store installation
echo -e "${YELLOW}Step 2: Staging TextEdit.app as a store-installable package...${NC}"
STORE_DIR=$HOME/store
mkdir -p "$STORE_DIR"

# Create the tarball
TEXTEDIT_APP="$HOME/Applications/TextEdit.app"
if [ ! -d "$TEXTEDIT_APP" ]; then
  echo -e "${RED}ERROR: TextEdit.app not found at $TEXTEDIT_APP${NC}"
  exit 1
fi

TARBALL="$STORE_DIR/TextEdit.app.tar.gz"
if [ -f "$TARBALL" ]; then
  rm -f "$TARBALL"
fi

cd "$HOME/Applications" && tar czf "$TARBALL" TextEdit.app
CHECKSUM=$(sha256sum "$TARBALL" | awk '{print $1}')

echo -e "${GREEN}✓ Tarball created: $TARBALL${NC}"
echo -e "${GREEN}✓ Checksum: $CHECKSUM${NC}"

# Create the catalog
CATALOG="$HOME/Applications/catalog.json"
cat > "$CATALOG" <<EOF
[
  {
    "name": "TextEdit",
    "bundle_id": "com.slopos.textedit",
    "version": "0.1.0",
    "url": "$TARBALL",
    "sha256": "$CHECKSUM",
    "size": $(stat -f%z "$TARBALL" 2>/dev/null || stat -c%s "$TARBALL")
  }
]
EOF

echo -e "${GREEN}✓ Catalog written: $CATALOG${NC}"

# Step 3: Remove the pre-built TextEdit.app so the store must install it
echo -e "${YELLOW}Step 3: Removing pre-built TextEdit.app (store must install it)...${NC}"
rm -rf "$TEXTEDIT_APP"
echo -e "${GREEN}✓ Removed $TEXTEDIT_APP${NC}"

# Step 4: Launch the compositor and shell
echo -e "${YELLOW}Step 4: Starting compositor + shell...${NC}"
echo -e "${YELLOW}   Run this on the VM (tty1 or tmux):${NC}"
echo ""
echo "    export XDG_RUNTIME_DIR=/run/user/\$(id -u) \\"
echo "           LIBSEAT_BACKEND=seatd \\"
echo "           LIBGL_ALWAYS_SOFTWARE=1 \\"
echo "           GALLIUM_DRIVER=llvmpipe \\"
echo "           SLOPOS_LAYER_SHELL_CHROME=1"
echo "    cd ~/slopos-i"
echo "    ./target/release/slopos-compositor"
echo ""

# Step 5: Verification steps (to run in another terminal)
echo -e "${YELLOW}Step 5: In another terminal, run these to verify:${NC}"
echo ""
echo "  # Open the App Store (should appear after Shell launches)"
echo "  # Click \"App Store\" in the menu bar or dock"
echo "  # Search for or select TextEdit"
echo "  # Click Install"
echo ""
echo "  # After install completes, verify:"
echo "  test -x $HOME/Applications/TextEdit.app/bin/textedit && echo INSTALLED-VIA-STORE"
echo ""
echo "  # Check if it appears in Finder/Dock (visual verification)"
echo "  # Try launching it from Finder or dock"
echo ""

# Step 6: Screenshot instructions
echo -e "${YELLOW}Step 6: Capture screenshots:${NC}"
echo ""
echo "  For Xvfb method (if running headless):"
echo "    export DISPLAY=:99"
echo "    import -window root ~/slopos-i/artifacts/qa/screenshots/stage3-appstore-install.png"
echo "    import -window root ~/slopos-i/artifacts/qa/screenshots/stage3-textedit-launched.png"
echo ""
echo "  For VBox screenshot:"
echo "    VBoxManage controlvm slopos-i-arch screenshotpng ~/slopos-i/artifacts/qa/screenshots/stage3-appstore-install.png"
echo ""

# Final verification check (after manual steps)
echo -e "${YELLOW}Step 7: Run this final check after store install completes:${NC}"
echo ""
echo "  if test -x $HOME/Applications/TextEdit.app/bin/textedit; then"
echo "    echo INSTALLED-VIA-STORE"
echo "  else"
echo "    echo FAILED"
echo "    exit 1"
echo "  fi"
echo ""

echo -e "${GREEN}=== Task 3.10 setup complete ===${NC}"
echo -e "${YELLOW}Next: manually open store, install TextEdit, and verify${NC}"
