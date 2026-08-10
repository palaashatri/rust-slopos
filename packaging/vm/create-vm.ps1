# Create the SLOPOS-I verification VM in VirtualBox.
#
# Why VirtualBox + VMSVGA: the guest gets a real vmwgfx DRM device with
# KMS *and* a render node, so slopos-compositor's DRM/KMS session path and
# its nested-X11 path (DRI3 via glamor) can both actually run. Xvfb, WSLg
# and Docker-on-mac provide none of that, which is why the compositor has
# never been exercised anywhere.
#
# Usage:  pwsh -File create-vm.ps1 [-IsoPath <path>] [-Recreate]
param(
    [string]$VmName  = "slopos-i-arch",
    [string]$IsoPath = "",
    [int]$MemoryMB   = 8192,
    [int]$Cpus       = 4,
    [int]$DiskMB     = 61440,
    [int]$SshPort    = 2222,
    [switch]$Recreate
)

$ErrorActionPreference = "Stop"
$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
if (-not (Test-Path $VBox)) { throw "VBoxManage not found at $VBox" }

function VB { & $VBox @args; if ($LASTEXITCODE -ne 0) { throw "VBoxManage $($args -join ' ') failed ($LASTEXITCODE)" } }

$existing = & $VBox list vms | Select-String -SimpleMatch "`"$VmName`""
if ($existing) {
    if ($Recreate) {
        Write-Host "Removing existing VM $VmName"
        & $VBox controlvm $VmName poweroff 2>$null | Out-Null
        Start-Sleep -Seconds 2
        VB unregistervm $VmName --delete
    } else {
        Write-Host "VM $VmName already exists (use -Recreate to rebuild)"
        exit 0
    }
}

Write-Host "Creating VM $VmName"
VB createvm --name $VmName --ostype ArchLinux_64 --register

# Firmware + core sizing. EFI matches the GPT/GRUB layout arch-install.sh builds.
VB modifyvm $VmName --memory $MemoryMB --cpus $Cpus --firmware efi `
    --ioapic on --rtcuseutc on --pae off --nested-hw-virt off

# Graphics: VMSVGA is the only VirtualBox adapter with a Linux DRM driver
# (vmwgfx) that exposes KMS + a render node. 3D on = GBM/EGL work in guest.
VB modifyvm $VmName --graphicscontroller vmsvga --vram 128 --accelerate3d on

# Networking: NAT + host port-forward so QA can drive the VM over SSH
# instead of scraping the console.
VB modifyvm $VmName --nic1 nat
VB modifyvm $VmName --natpf1 "ssh,tcp,127.0.0.1,$SshPort,,22"

# Storage
$vmDir = (& $VBox showvminfo $VmName --machinereadable | Select-String '^CfgFile=' ) -replace '^CfgFile="' -replace '"$'
$vmDir = Split-Path $vmDir -Parent
$disk  = Join-Path $vmDir "$VmName.vdi"
VB createmedium disk --filename $disk --size $DiskMB --format VDI
VB storagectl $VmName --name "SATA" --add sata --controller IntelAhci --portcount 2
VB storageattach $VmName --storagectl "SATA" --port 0 --device 0 --type hdd --medium $disk

if ($IsoPath -and (Test-Path $IsoPath)) {
    VB storageattach $VmName --storagectl "SATA" --port 1 --device 0 --type dvddrive --medium $IsoPath
    VB modifyvm $VmName --boot1 dvd --boot2 disk --boot3 none --boot4 none
    Write-Host "Attached ISO: $IsoPath"
} else {
    Write-Host "No ISO attached (pass -IsoPath to attach one)"
}

Write-Host ""
Write-Host "VM '$VmName' created."
Write-Host "  memory : $MemoryMB MB   cpus: $Cpus   disk: $DiskMB MB"
Write-Host "  gfx    : VMSVGA + 3D (vmwgfx KMS + render node in guest)"
Write-Host "  ssh    : ssh -p $SshPort retro@127.0.0.1"
Write-Host ""
Write-Host "Start headless:  & '$VBox' startvm $VmName --type headless"
Write-Host "Screenshot:      & '$VBox' controlvm $VmName screenshotpng shot.png"
