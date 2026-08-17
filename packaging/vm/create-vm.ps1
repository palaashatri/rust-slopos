# Create the SLOPOS-I X11 verification VM in VirtualBox.
#
# The VM exists to prove installer, boot, Xorg/Openbox session and full-desktop
# behavior outside Xvfb. VMSVGA is used for broad Linux guest compatibility;
# SLOPOS itself does not own the display server or graphics driver.
#
# Usage: pwsh -File create-vm.ps1 [-IsoPath <path>] [-IsoSha256 <sha256>] [-Recreate]
param(
    [string]$VmName  = "slopos-i-arch",
    [string]$IsoPath = "",
    [string]$IsoSha256 = "",
    [int]$MemoryMB   = 4096,
    [int]$Cpus       = 4,
    [int]$DiskMB     = 40960,
    [int]$SshPort    = 2222,
    [switch]$Recreate
)

$ErrorActionPreference = "Stop"
$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
if (-not (Test-Path $VBox)) { throw "VBoxManage not found at $VBox" }
if ([string]::IsNullOrWhiteSpace($VmName)) { throw "VmName must not be empty" }
if ($MemoryMB -lt 1) { throw "MemoryMB must be positive" }
if ($Cpus -lt 1) { throw "Cpus must be positive" }
if ($DiskMB -lt 1) { throw "DiskMB must be positive" }
if ($SshPort -lt 1 -or $SshPort -gt 65535) {
    throw "SshPort must be between 1 and 65535"
}
if (-not [string]::IsNullOrWhiteSpace($IsoSha256) -and
    $IsoSha256 -notmatch '^[0-9a-fA-F]{64}$') {
    throw "IsoSha256 must be a 64-character SHA-256 digest"
}

if (-not [string]::IsNullOrWhiteSpace($IsoPath)) {
    if (-not (Test-Path -LiteralPath $IsoPath -PathType Leaf)) {
        throw "ISO must be a regular file: $IsoPath"
    }
    $isoInfo = Get-Item -LiteralPath $IsoPath
    if ($isoInfo.Length -le 0) {
        throw "ISO must be non-empty: $IsoPath"
    }
    $IsoPath = $isoInfo.FullName
    $actualIsoSha256 = (Get-FileHash -LiteralPath $IsoPath -Algorithm SHA256).Hash
    if (-not [string]::IsNullOrWhiteSpace($IsoSha256) -and
        $actualIsoSha256 -ine $IsoSha256) {
        throw "ISO SHA-256 $actualIsoSha256 does not match expected $IsoSha256"
    }
    Write-Host "ISO SHA-256: $actualIsoSha256"
}

function VB {
    & $VBox @args
    if ($LASTEXITCODE -ne 0) {
        throw "VBoxManage $($args -join ' ') failed ($LASTEXITCODE)"
    }
}

$existing = & $VBox list vms | Select-String -SimpleMatch "`"$VmName`""
if ($existing) {
    if ($Recreate) {
        Write-Host "Removing existing VM $VmName"
        $vmState = (& $VBox showvminfo $VmName --machinereadable |
            Select-String '^VMState="([^\"]+)"$').Matches.Groups[1].Value
        if ([string]::IsNullOrWhiteSpace($vmState)) {
            throw "Unable to determine the state of existing VM $VmName; refusing --delete"
        }
        switch ($vmState) {
            { $_ -in @('running', 'paused', 'stuck', 'starting', 'stopping') } {
                $previousErrorAction = $ErrorActionPreference
                $ErrorActionPreference = 'Continue'
                & $VBox controlvm $VmName poweroff 2>&1 | Out-Null
                $poweroffExit = $LASTEXITCODE
                $ErrorActionPreference = $previousErrorAction
                if ($poweroffExit -ne 0) {
                    throw "VBoxManage controlvm $VmName poweroff failed ($poweroffExit)"
                }
                Start-Sleep -Seconds 2
                break
            }
            'saved' {
                VB controlvm $VmName discardstate
                break
            }
            'poweroff' { break }
            'aborted' { break }
            default {
                throw "Refusing to delete VM $VmName in unsupported state '$vmState'"
            }
        }
        VB unregistervm $VmName --delete
    } else {
        Write-Host "VM $VmName already exists (use -Recreate to rebuild)"
        exit 0
    }
}

Write-Host "Creating VM $VmName"
VB createvm --name $VmName --ostype ArchLinux_64 --register
VB modifyvm $VmName --memory $MemoryMB --cpus $Cpus --firmware efi `
    --ioapic on --rtcuseutc on --pae off --nested-hw-virt off

# VMSVGA is the normal VirtualBox Linux graphics adapter. 3D acceleration is
# useful for realistic upstream applications but is not a SLOPOS requirement.
VB modifyvm $VmName --graphicscontroller vmsvga --vram 128 --accelerate3d on

VB modifyvm $VmName --nic1 nat
VB modifyvm $VmName --natpf1 "ssh,tcp,127.0.0.1,$SshPort,,22"

$cfg = (& $VBox showvminfo $VmName --machinereadable | Select-String '^CfgFile=') `
    -replace '^CfgFile="' -replace '"$'
$vmDir = Split-Path $cfg -Parent
$disk = Join-Path $vmDir "$VmName.vdi"
VB createmedium disk --filename $disk --size $DiskMB --format VDI
VB storagectl $VmName --name "SATA" --add sata --controller IntelAhci --portcount 2
VB storageattach $VmName --storagectl "SATA" --port 0 --device 0 --type hdd --medium $disk

if ($IsoPath) {
    VB storageattach $VmName --storagectl "SATA" --port 1 --device 0 --type dvddrive --medium $IsoPath
    VB modifyvm $VmName --boot1 dvd --boot2 disk --boot3 none --boot4 none
    Write-Host "Attached ISO: $IsoPath"
} else {
    VB modifyvm $VmName --boot1 disk --boot2 none --boot3 none --boot4 none
    Write-Host "No ISO attached. Pass -IsoPath for a fresh Arch installation."
}

Write-Host ""
Write-Host "VM '$VmName' created for SLOPOS-I X11 verification."
Write-Host "  memory : $MemoryMB MB   cpus: $Cpus   disk: $DiskMB MB"
Write-Host "  gfx    : VMSVGA + 3D"
Write-Host "  ssh    : ssh -p $SshPort retro@127.0.0.1"
Write-Host ""
Write-Host "Start:       & '$VBox' startvm $VmName --type gui"
Write-Host "Screenshot:  & '$VBox' controlvm $VmName screenshotpng shot.png"
