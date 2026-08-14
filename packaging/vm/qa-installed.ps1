# Run deterministic post-reboot QA against an installed SLOPOS-I VM.
#
# The installer is intentionally left running after provisioning so failures
# can be inspected. This helper waits for the forwarded SSH service, verifies
# the checked-out source commit, invokes the in-guest QA script, captures a
# VirtualBox screenshot, and writes a machine-readable status file.
[CmdletBinding()]
param(
    [string]$VmName      = "slopos-i-arch",
    [int]$SshPort        = 2222,
    [string]$SshUser     = "retro",
    [string]$SshKeyPath  = "$PSScriptRoot\qa_key",
    [string]$OutputDir   = "$PSScriptRoot\installed-vm-evidence",
    [string]$ExpectedCommit = "",
    [int]$WaitSec        = 900
)

$ErrorActionPreference = "Stop"
$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
$qaExit = $null
$sourceCommitExit = $null
$sourceCommit = ""
$screenshotExit = $null
$screenshotOk = $false
$qaMarker = $false
$failure = $null
$statusPath = Join-Path $OutputDir "status.json"
$qaLogPath = Join-Path $OutputDir "qa-vm.log"
$screenshotPath = Join-Path $OutputDir "installed-vm.png"

function Test-TcpPort {
    param([string]$HostName, [int]$Port)
    $client = New-Object System.Net.Sockets.TcpClient
    try {
        $pending = $client.ConnectAsync($HostName, $Port)
        if ($pending.Wait(2000) -and $client.Connected) { return $true }
        return $false
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
}

try {
    if (-not (Test-Path -LiteralPath $VBox -PathType Leaf)) {
        throw "VBoxManage not found at $VBox"
    }
    if ($SshUser -notmatch '^[a-z_][a-z0-9_-]*$') {
        throw "SshUser must be a simple Linux account name"
    }
    if ($SshPort -lt 1 -or $SshPort -gt 65535) {
        throw "SshPort must be between 1 and 65535"
    }
    if ([string]::IsNullOrWhiteSpace($ExpectedCommit)) {
        throw "ExpectedCommit is required; refusing to accept an unpinned installed VM"
    }
    if ($ExpectedCommit -notmatch '^[0-9a-fA-F]{40}$') {
        throw "ExpectedCommit must be a full 40-character commit SHA"
    }
    if ($WaitSec -lt 1) { throw "WaitSec must be positive" }
    if (-not (Test-Path -LiteralPath $SshKeyPath -PathType Leaf)) {
        throw "SSH private key not found: $SshKeyPath"
    }
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

    $sshCommand = Get-Command ssh.exe -ErrorAction SilentlyContinue
    if (-not $sshCommand) {
        throw "OpenSSH ssh.exe is required for installed-VM QA"
    }
    $sshPath = $sshCommand.Source
    $knownHostsSink = if ($env:OS -eq "Windows_NT") { "NUL" } else { "/dev/null" }
    $sshArgs = @(
        "-i", $SshKeyPath,
        "-p", $SshPort.ToString(),
        "-o", "BatchMode=yes",
        "-o", "ConnectTimeout=5",
        "-o", "LogLevel=ERROR",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=$knownHostsSink"
    )

    function Invoke-SshCapture {
        param([string[]]$Arguments)
        $stdoutPath = [System.IO.Path]::GetTempFileName()
        $stderrPath = [System.IO.Path]::GetTempFileName()
        $savedErrorActionPreference = $ErrorActionPreference
        try {
            # PowerShell 5 promotes native stderr to NativeCommandError when
            # ErrorActionPreference=Stop. Capture it explicitly so SSH host
            # key diagnostics cannot abort a probe that returned zero.
            $ErrorActionPreference = "Continue"
            & $sshPath @Arguments 1> $stdoutPath 2> $stderrPath
            $exitCode = $LASTEXITCODE
            return [pscustomobject]@{
                ExitCode = $exitCode
                Stdout  = @(Get-Content -LiteralPath $stdoutPath -ErrorAction SilentlyContinue)
                Stderr  = @(Get-Content -LiteralPath $stderrPath -ErrorAction SilentlyContinue)
            }
        }
        finally {
            $ErrorActionPreference = $savedErrorActionPreference
            Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
        }
    }

    $stateLine = & $VBox showvminfo $VmName --machinereadable |
        Select-String '^VMState=' | Select-Object -First 1
    if (-not $stateLine) { throw "VirtualBox VM not found: $VmName" }
    $state = $stateLine.ToString() -replace '^VMState="', '' -replace '"$', ''
    if ($state -ne "running") {
        throw "VM $VmName is not running (state: $state)"
    }

    $deadline = (Get-Date).ToUniversalTime().AddSeconds($WaitSec)
    $sshReady = $false
    while ((Get-Date).ToUniversalTime() -lt $deadline) {
        if (Test-TcpPort -HostName "127.0.0.1" -Port $SshPort) {
            $probe = Invoke-SshCapture -Arguments @($sshArgs + @("$SshUser@127.0.0.1", "true"))
            if ($probe.ExitCode -eq 0) {
                $sshReady = $true
                break
            }
        }
        Start-Sleep -Seconds 5
    }
    if (-not $sshReady) {
        throw "SSH did not become ready on 127.0.0.1:$SshPort within ${WaitSec}s"
    }

    $sourceResult = Invoke-SshCapture -Arguments @($sshArgs + @(
        "$SshUser@127.0.0.1",
        "git -C /home/$SshUser/slopos-i rev-parse HEAD"
    ))
    $sourceCommitExit = $sourceResult.ExitCode
    $sourceCommit = ($sourceResult.Stdout -join "").Trim()
    if ($sourceCommitExit -ne 0 -or $sourceCommit -notmatch '^[0-9a-fA-F]{40}$') {
        throw "Installed source checkout did not report a full commit SHA"
    }
    if ($sourceCommit -ne $ExpectedCommit) {
        throw "Installed source commit $sourceCommit does not match expected $ExpectedCommit"
    }

    $remoteQa = "DISPLAY=:0 XAUTHORITY=/home/$SshUser/.Xauthority bash /home/$SshUser/slopos-i/packaging/vm/qa-vm.sh"
    Write-Host "Running installed VM QA at source commit $sourceCommit"
    $qaResult = Invoke-SshCapture -Arguments @($sshArgs + @("$SshUser@127.0.0.1", $remoteQa))
    $qaExit = $qaResult.ExitCode
    $qaOutput = @($qaResult.Stdout + $qaResult.Stderr)
    $qaOutput | Tee-Object -FilePath $qaLogPath
    if ($qaExit -ne 0) {
        throw "qa-vm.sh failed with exit code $qaExit"
    }
    $qaText = $qaOutput -join [Environment]::NewLine
    $qaMarker = $qaText -match '(?m)^SLOPOS_X11_INSTALLED_VM_QA=PASS\s*$'
    if (-not $qaMarker) {
        throw "qa-vm.sh exited successfully without SLOPOS_X11_INSTALLED_VM_QA=PASS"
    }

    $savedErrorActionPreference = $ErrorActionPreference
    try {
        # VBoxManage may emit informational stderr on successful captures;
        # retain the real process exit code without PowerShell promoting it to
        # a terminating NativeCommandError.
        $ErrorActionPreference = "Continue"
        & $VBox controlvm $VmName screenshotpng $screenshotPath 1>$null 2>$null
        $screenshotExit = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $savedErrorActionPreference
    }
    $screenshotOk = $screenshotExit -eq 0 -and
        (Test-Path -LiteralPath $screenshotPath -PathType Leaf) -and
        ((Get-Item -LiteralPath $screenshotPath).Length -gt 0)
    if (-not $screenshotOk) {
        throw "VirtualBox screenshot capture failed: $screenshotPath"
    }

    Write-Host "INSTALLED_VM_QA_STATUS_0"
}
catch {
    $failure = $_.Exception.Message
    throw
}
finally {
    $status = [ordered]@{
        vm_name             = $VmName
        ssh_port            = $SshPort
        expected_commit     = $ExpectedCommit
        source_commit       = $sourceCommit
        source_commit_exit  = $sourceCommitExit
        qa_exit             = $qaExit
        qa_marker           = $qaMarker
        screenshot_exit     = $screenshotExit
        screenshot          = $screenshotPath
        qa_log              = $qaLogPath
        passed              = ($null -eq $failure -and $qaExit -eq 0 -and $qaMarker -and $screenshotOk)
        failure             = $failure
        completed_utc       = (Get-Date).ToUniversalTime().ToString("o")
    }
    try {
        New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
        $status | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $statusPath -Encoding UTF8
        Write-Host "Status: $statusPath"
    } catch {
        Write-Error "Unable to write installed VM status: $($_.Exception.Message)"
    }
}
