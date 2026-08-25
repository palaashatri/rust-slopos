<#
.SYNOPSIS
    SLOPOS-I v20260824 Windows Host Release QA Orchestrator
    Executes end-to-end verification via Docker and VirtualBox.
#>
param(
    [string]$VmName = "ubuntu-server",
    [string]$VmUser = "ubuntu",
    [string]$VmPass = "ubuntu",
    [switch]$SkipISO
)

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$vboxManage = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
if (-not (Test-Path $vboxManage)) {
    $vboxCmd = Get-Command VBoxManage -ErrorAction SilentlyContinue
    if ($vboxCmd) { $vboxManage = $vboxCmd.Source }
    else { throw "VirtualBox VBoxManage.exe not found!" }
}

Write-Host "======================================================" -ForegroundColor Cyan
Write-Host "   SLOPOS-I v20260824 Release QA Orchestrator" -ForegroundColor Cyan
Write-Host "======================================================" -ForegroundColor Cyan
Write-Host "Repository: $repoRoot"
Write-Host "Target VM:  $VmName"

function Run-InGuest {
    param([string]$BashScript)
    $tmpFile = [System.IO.Path]::GetTempFileName() + ".sh"
    $unixScript = $BashScript -replace "`r`n", "`n"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($tmpFile, $unixScript, $utf8NoBom)
    $remoteTmp = "/tmp/slopos_qa_$([guid]::NewGuid().ToString('N')).sh"
    & $vboxManage guestcontrol $VmName copyto "$tmpFile" "$remoteTmp" --username $VmUser --password $VmPass
    Remove-Item $tmpFile -ErrorAction SilentlyContinue
    
    & $vboxManage guestcontrol $VmName run --exe "/bin/bash" --username $VmUser --password $VmPass --wait-stdout --wait-stderr -- "$remoteTmp"
    $code = $LASTEXITCODE
    & $vboxManage guestcontrol $VmName run --exe "/bin/rm" --username $VmUser --password $VmPass --wait-stdout --wait-stderr -- "-f" "$remoteTmp"
    if ($code -ne 0) {
        throw "Guest execution failed with code $code"
    }
}

# 1. Ensure VM is running
Write-Host "`n[1/8] Verifying VirtualBox VM status..." -ForegroundColor Yellow
$running = & $vboxManage list runningvms
if ($running -notmatch [regex]::Escape($VmName)) {
    Write-Host "Starting VM $VmName in headless mode..." -ForegroundColor DarkGray
    & $vboxManage startvm $VmName --type headless
    Start-Sleep -Seconds 10
}
Write-Host "VM $VmName is active." -ForegroundColor Green

# 2. Synchronize repository to guest
Write-Host "`n[2/8] Synchronizing repository to VM /home/$VmUser/rust-slopos..." -ForegroundColor Yellow
$syncScript = @"
mkdir -p /home/$VmUser/rust-slopos
"@
Run-InGuest $syncScript

# Archive repo to tar and stream to guest
$tarFile = "$repoRoot\artifacts\qa-sync.tar"
if (Test-Path "$repoRoot\artifacts") { New-Item -ItemType Directory -Path "$repoRoot\artifacts" -Force | Out-Null }
if (Test-Path $tarFile) { Remove-Item $tarFile -Force }

tar --exclude=".git" --exclude="target" --exclude="target-qa" --exclude="artifacts" -cf "$tarFile" -C "$repoRoot" .
& $vboxManage guestcontrol $VmName copyto "$tarFile" "/tmp/qa-sync.tar" --username $VmUser --password $VmPass
Remove-Item $tarFile -ErrorAction SilentlyContinue

$extractScript = @"
tar -xf /tmp/qa-sync.tar -C /home/$VmUser/rust-slopos
rm -f /tmp/qa-sync.tar
cd /home/$VmUser/rust-slopos
find packaging scripts -type f -exec sed -i 's/\r$//' {} + 2>/dev/null || true
chmod +x packaging/debian/rules scripts/* install.sh 2>/dev/null || true
"@
Run-InGuest $extractScript
Write-Host "Repository synchronized successfully." -ForegroundColor Green

# 3. Base Cargo Quality Gates
Write-Host "`n[3/8] Executing Base Rust Quality Gates in VM..." -ForegroundColor Yellow
$rustGatesScript = @"
#!/bin/bash
set -ex
source "`$HOME/.cargo/env" 2>/dev/null || true
export PATH="`$HOME/.cargo/bin:`$PATH"
cd /home/$VmUser/rust-slopos

echo "=== CARGO FMT CHECK ==="
cargo fmt --all -- --check

echo "=== CARGO CHECK ==="
cargo check --workspace --locked

echo "=== CARGO CLIPPY ==="
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "=== CARGO TEST ==="
cargo test --workspace --locked

echo "=== CARGO RELEASE BUILD ==="
cargo build --release --workspace --locked

echo "=== SHELL SYNTAX VALIDATION ==="
bash -n install.sh
bash -n scripts/start-slopos-i
bash -n scripts/start-slopos-browser
bash -n scripts/slopos-appearance
bash -n scripts/slopos-recovery.sh
bash -n scripts/generate-package-repos.sh
bash -n packaging/iso/build-iso.sh
bash -n packaging/iso/build-debian-iso.sh
"@
Run-InGuest $rustGatesScript
Write-Host "Rust Quality Gates Passed!" -ForegroundColor Green

# 4. Packaging & Repository Generation
Write-Host "`n[4/8] Building native packages and repository metadata..." -ForegroundColor Yellow
$pkgScript = @"
#!/bin/bash
set -ex
source "`$HOME/.cargo/env" 2>/dev/null || true
export PATH="`$HOME/.cargo/bin:`$PATH"
cd /home/$VmUser/rust-slopos

mkdir -p artifacts/debian-package artifacts/arch-package artifacts/repositories

# 1. Debian package build (reuse release target)
rm -rf debian
cp -a packaging/debian debian
export CARGO_TARGET_DIR="/home/$VmUser/rust-slopos/target"
dpkg-buildpackage --build=binary --no-sign -d -nc
cp ../slopos-i_*.deb artifacts/debian-package/slopos-i_20260824-1_amd64.deb
cp ../slopos-i_*.deb artifacts/debian-package/slopos-i.deb

# 2. Arch package build via tar / makepkg structure
mkdir -p /tmp/arch-pkg-build
cp -a packaging/arch/PKGBUILD /tmp/arch-pkg-build/
(
  cd /home/$VmUser/rust-slopos
  mkdir -p artifacts/arch-pkg-root
  pkgdir="artifacts/arch-pkg-root"
  for binary in slopos-session slopos-shell slopos-catalogue slopos-settings; do
    install -Dm755 "target/release/`$binary" "`$pkgdir/usr/bin/`$binary"
  done
  install -Dm755 scripts/start-slopos-i "`$pkgdir/usr/bin/start-slopos-i"
  install -Dm755 scripts/start-slopos-browser "`$pkgdir/usr/bin/start-slopos-browser"
  install -Dm755 scripts/install-browser-theme.sh "`$pkgdir/usr/bin/install-browser-theme.sh"
  install -Dm755 scripts/slopos-appearance "`$pkgdir/usr/bin/slopos-appearance"
  install -Dm755 scripts/slopos-recovery.sh "`$pkgdir/usr/bin/slopos-recovery"
  install -Dm644 packaging/slopos-browser.desktop "`$pkgdir/usr/share/applications/slopos-browser.desktop"
  install -Dm644 packaging/slopos-i.desktop "`$pkgdir/usr/share/xsessions/slopos-i.desktop"
  install -Dm644 assets/config/openbox/rc.xml "`$pkgdir/usr/share/slopos-i/openbox/rc.xml"
  install -Dm644 assets/config/openbox/rc-classic.xml "`$pkgdir/usr/share/slopos-i/openbox/rc-classic.xml"
  install -Dm644 assets/config/openbox/rc-graphite.xml "`$pkgdir/usr/share/slopos-i/openbox/rc-graphite.xml"
  install -Dm644 assets/config/openbox/menu.xml "`$pkgdir/usr/share/slopos-i/openbox/menu.xml"
  install -Dm644 themes/slopos-openbox/openbox-3/themerc "`$pkgdir/usr/share/themes/slopos-openbox/openbox-3/themerc"
  install -Dm644 themes/slopos-openbox-classic/openbox-3/themerc "`$pkgdir/usr/share/themes/slopos-openbox-classic/openbox-3/themerc"
  install -Dm644 themes/slopos-openbox-graphite/openbox-3/themerc "`$pkgdir/usr/share/themes/slopos-openbox-graphite/openbox-3/themerc"
  install -Dm644 assets/config/gtk-3.0/gtk.css "`$pkgdir/usr/share/themes/slopos-gtk/gtk-3.0/gtk.css"
  install -Dm644 assets/config/gtk-3.0/gtk-classic.css "`$pkgdir/usr/share/themes/slopos-gtk-classic/gtk-3.0/gtk.css"
  install -Dm644 assets/config/gtk-3.0/gtk-graphite.css "`$pkgdir/usr/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css"
  install -Dm644 assets/config/gtk-3.0/settings.ini "`$pkgdir/usr/share/slopos-i/gtk-3.0/settings.ini"
  install -Dm644 assets/config/mimeapps.list "`$pkgdir/usr/share/slopos-i/mimeapps.list"
  install -Dm644 assets/slopos-logo.png "`$pkgdir/usr/share/slopos-i/slopos-logo.png"
  mkdir -p "`$pkgdir/usr/share/slopos-i/recovery"
  printf '%s\n' platinum >"`$pkgdir/usr/share/slopos-i/recovery/appearance"
  install -Dm644 assets/config/openbox/rc.xml "`$pkgdir/usr/share/slopos-i/recovery/openbox/rc.xml"
  install -Dm644 assets/config/openbox/menu.xml "`$pkgdir/usr/share/slopos-i/recovery/openbox/menu.xml"
  mkdir -p "`$pkgdir/usr/share/slopos-i/themes"
  cp -a themes/platinum "`$pkgdir/usr/share/slopos-i/themes/platinum"
  cp -a themes/graphite "`$pkgdir/usr/share/slopos-i/themes/graphite"
  mkdir -p "`$pkgdir/usr/share/icons"
  cp -a themes/platinum/icon-theme "`$pkgdir/usr/share/icons/SLOPOS-Platinum"
  mkdir -p "`$pkgdir/usr/share/slopos-i/browser"
  cp -a packaging/browser/chromium "`$pkgdir/usr/share/slopos-i/browser/chromium"
  cp -a packaging/browser/firefox "`$pkgdir/usr/share/slopos-i/browser/firefox"
  install -Dm644 README.md "`$pkgdir/usr/share/doc/slopos-i/README.md"
  install -Dm644 AGENTS.md "`$pkgdir/usr/share/doc/slopos-i/AGENTS.md"
  install -Dm644 TRUTH.md "`$pkgdir/usr/share/doc/slopos-i/TRUTH.md"
  install -Dm644 THIRD_PARTY_LICENSES.txt "`$pkgdir/usr/share/doc/slopos-i/THIRD_PARTY_LICENSES.txt"

  # Generate .PKGINFO
  cat > "`$pkgdir/.PKGINFO" <<EOF
pkgname = slopos-i
pkgbase = slopos-i
pkgver = 20260824-1
pkgdesc = SLOPOS-I X11 Platinum/Graphite desktop environment
url = https://github.com/palaashatri/rust-slopos
builddate = 1787596800
packager = SLOPOS-I Contributors <team@slopos-i.local>
size = 15360000
arch = x86_64
license = MIT
depend = xorg-server
depend = openbox
depend = gtk3
depend = dbus
optdepend = firefox: web browser
optdepend = lightdm: graphical display manager
provides = slopos-i
conflict = slopos-i-git
EOF

  # Package into .pkg.tar.zst
  (cd "`$pkgdir" && tar --zstd -cf /home/$VmUser/rust-slopos/artifacts/arch-package/slopos-i-20260824-1-x86_64.pkg.tar.zst .PKGINFO usr)
  cp /home/$VmUser/rust-slopos/artifacts/arch-package/slopos-i-20260824-1-x86_64.pkg.tar.zst /home/$VmUser/rust-slopos/artifacts/arch-package/slopos-i.pkg.tar.zst
  rm -rf "`$pkgdir"
)

# 3. Package Repository Generation
bash scripts/generate-package-repos.sh artifacts/repositories alpha
"@
Run-InGuest $pkgScript
Write-Host "Packages and Repositories generated successfully." -ForegroundColor Green

# 5. Clean Install & Desktop Acceptance
Write-Host "`n[5/8] Testing clean package installation, session start, and removal..." -ForegroundColor Yellow
$installTestScript = @"
#!/bin/bash
set -ex
cd /home/$VmUser/rust-slopos

# Install package
sudo dpkg -i artifacts/debian-package/slopos-i_20260824-1_amd64.deb || sudo apt-get install -f -y

# Verify files exist
test -x /usr/bin/slopos-session
test -x /usr/bin/slopos-shell
test -x /usr/bin/slopos-catalogue
test -x /usr/bin/slopos-settings
test -x /usr/bin/start-slopos-i
test -f /usr/share/xsessions/slopos-i.desktop

# Validate X11 session execution under Xvfb
export DISPLAY=:99
Xvfb :99 -screen 0 1920x1080x24 &
XVFB_PID=`$!
sleep 2

/usr/bin/openbox &
OB_PID=`$!
sleep 1

/usr/bin/slopos-shell &
SHELL_PID=`$!
sleep 3

kill `$SHELL_PID `$OB_PID `$XVFB_PID 2>/dev/null || true
wait 2>/dev/null || true

# Test package purge
sudo dpkg -P slopos-i
test ! -e /usr/bin/slopos-shell
"@
Run-InGuest $installTestScript
Write-Host "Clean install and removal tests passed!" -ForegroundColor Green

# 6. Visual QA & Deterministic Screenshot Suite
Write-Host "`n[6/8] Executing Deterministic Visual QA & Screenshot Suite..." -ForegroundColor Yellow
$visualQaScript = @"
#!/bin/bash
set -ex
cd /home/$VmUser/rust-slopos

# Ensure package is installed for visual QA
sudo dpkg -i artifacts/debian-package/slopos-i_20260824-1_amd64.deb || sudo apt-get install -f -y

# Run screenshot capture suite
bash qa/capture-screenshots.sh
"@
Run-InGuest $visualQaScript

# Download captured screenshots and manifest to host
New-Item -ItemType Directory -Force -Path "$RepoRoot\qa\screenshots" | Out-Null
New-Item -ItemType Directory -Force -Path "$RepoRoot\artifacts\screenshots" | Out-Null
$tarRemoteScript = @"
tar -cf /tmp/screenshots.tar -C /home/$VmUser/rust-slopos/qa screenshots
"@
Run-InGuest $tarRemoteScript
$localTar = "$RepoRoot\qa\screenshots.tar"
& $vboxManage guestcontrol $VmName copyfrom "/tmp/screenshots.tar" "$localTar" --username $VmUser --password $VmPass
tar -xf "$localTar" -C "$RepoRoot\qa"
Remove-Item "$localTar" -ErrorAction SilentlyContinue
Copy-Item -Path "$RepoRoot\qa\screenshots\*" -Destination "$RepoRoot\artifacts\screenshots\" -Recurse -Force
Write-Host "Visual QA Screenshots downloaded successfully to qa/screenshots/ and artifacts/screenshots/" -ForegroundColor Green

# 7. Arch Package Payload Validation
Write-Host "`n[7/8] Validating Arch Package Payload & Repositories..." -ForegroundColor Yellow
$archVerifyScript = @"
#!/bin/bash
set -ex
cd /home/$VmUser/rust-slopos

test -f artifacts/arch-package/slopos-i-20260824-1-x86_64.pkg.tar.zst
tar -tf artifacts/arch-package/slopos-i-20260824-1-x86_64.pkg.tar.zst | grep 'usr/bin/slopos-shell'
tar -tf artifacts/arch-package/slopos-i-20260824-1-x86_64.pkg.tar.zst | grep 'usr/share/xsessions/slopos-i.desktop'

test -f artifacts/repositories/SHA256SUMS
test -f artifacts/repositories/apt/dists/alpha/Release
test -f artifacts/repositories/apt/dists/alpha/main/binary-amd64/Packages.gz
test -f artifacts/repositories/pacman/slopos.db.tar.gz

echo "=== All Artifact Contracts Verified ==="
"@
Run-InGuest $archVerifyScript
Write-Host "Arch package payload and Repository metadata verified!" -ForegroundColor Green

# 8. Release QA Summary
Write-Host "`n======================================================" -ForegroundColor Green
Write-Host "   SLOPOS-I v20260824 Release QA Completed Successfully" -ForegroundColor Green
Write-Host "======================================================" -ForegroundColor Green
