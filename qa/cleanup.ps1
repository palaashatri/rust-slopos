<#
.SYNOPSIS
    Cleanup disposable SLOPOS QA infrastructure and artifacts safely.
#>
param(
    [switch]$Force
)

$ErrorActionPreference = "Continue"

$vboxManage = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
if (-not (Test-Path $vboxManage)) {
    $vboxCmd = Get-Command VBoxManage -ErrorAction SilentlyContinue
    if ($vboxCmd) {
        $vboxManage = $vboxCmd.Source
    }
}

Write-Host "=== SLOPOS QA Lab Cleanup ===" -ForegroundColor Cyan

# 1. Clean disposable VirtualBox VMs named 'slopos-qa-*'
if (Test-Path $vboxManage) {
    $vms = & $vboxManage list vms
    $qaVms = $vms | Select-String '"(slopos-qa-[^"]+)"'
    foreach ($match in $qaVms) {
        $vmName = $match.Matches.Groups[1].Value
        Write-Host "Found QA VM: $vmName" -ForegroundColor Yellow
        # Stop if running
        & $vboxManage controlvm $vmName poweroff 2>$null
        Start-Sleep -Seconds 1
        # Unregister and delete
        & $vboxManage unregistervm $vmName --delete
        Write-Host "Deleted QA VM: $vmName" -ForegroundColor Green
    }
} else {
    Write-Host "VirtualBox VBoxManage not found; skipping VM cleanup." -ForegroundColor DarkGray
}

# 2. Clean repository temporary target artifacts if requested
$repoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$cleanPaths = @(
    "$repoRoot\artifacts\qa-temp",
    "$repoRoot\target-qa"
)
foreach ($p in $cleanPaths) {
    if (Test-Path $p) {
        Write-Host "Removing temporary artifact directory: $p" -ForegroundColor Yellow
        Remove-Item -Recurse -Force $p -ErrorAction SilentlyContinue
    }
}

Write-Host "=== Cleanup Complete ===" -ForegroundColor Green
