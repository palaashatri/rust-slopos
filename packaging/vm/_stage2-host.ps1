# Stage 2 host orchestration — VBox keyboard + screenshots.
param(
    [string]$VmName = "slopos-i-arch",
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
)

$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
$SshKey = Join-Path $RepoRoot "packaging/vm/qa_key"
$KnownHosts = Join-Path $RepoRoot "packaging/vm/known_hosts"
$ScDir = Join-Path $RepoRoot "artifacts/qa/screenshots"
New-Item -ItemType Directory -Force -Path $ScDir | Out-Null

function Invoke-Ssh([string]$Cmd) {
    ssh -i $SshKey -p 2222 -o "UserKnownHostsFile=$KnownHosts" retro@127.0.0.1 $Cmd
}

function Send-Scan([string[]]$Codes) {
    & $VBox controlvm $VmName keyboardputscancode @Codes | Out-Null
}

function Send-Text([string]$Text) {
    & $VBox controlvm $VmName keyboardputstring $Text | Out-Null
}

function Send-MouseClick([int]$X, [int]$Y) {
    $cmd = "export YDOTOOL_SOCKET=/tmp/.ydotool_socket; sudo ydotool mousemove --absolute -x $X -y $Y; sudo ydotool click 0xC0"
    Invoke-Ssh $cmd | Out-Null
    Start-Sleep -Milliseconds 400
}

function Send-MouseDblClick([int]$X, [int]$Y) {
    Send-MouseClick $X $Y
    Start-Sleep -Milliseconds 120
    Send-MouseClick $X $Y
}

function Capture([string]$File) {
    $path = Join-Path $ScDir $File
    & $VBox controlvm $VmName screenshotpng $path | Out-Null
    Write-Host "captured $File ($(if (Test-Path $path) { (Get-Item $path).Length } else { 'missing' }) bytes)"
}

$startSh = Join-Path $RepoRoot "packaging/vm/_stage2-start.sh"
$c = [IO.File]::ReadAllText($startSh) -replace "`r`n", "`n" -replace "`r", "`n"
[IO.File]::WriteAllText($startSh, $c)
& scp -i $SshKey -P 2222 -o "UserKnownHostsFile=$KnownHosts" $startSh "retro@127.0.0.1:~/"
$out = Invoke-Ssh "chmod +x ~/_stage2-start.sh && bash ~/_stage2-start.sh"
Write-Host $out
if ($out -notmatch "SESSION_READY") { throw "compositor session failed to start: $out" }
Start-Sleep -Seconds 3

# 2.0 — focus foot and type proof line
Send-MouseClick 120 700
Start-Sleep -Seconds 1
Send-Text "echo STAGE2_INPUT_OK"
Start-Sleep -Milliseconds 300
Send-Scan @("1c", "9c")
Start-Sleep -Seconds 4
Capture "stage2-input.png"

# 2.1 — Super+O spawns Finder
Send-Scan @("e0", "5b", "18", "98", "e0", "db")
Start-Sleep -Seconds 6
$f1 = Invoke-Ssh "pgrep -xc finder || echo 0"
Write-Host "finder after Super+O: $f1"
Capture "stage2-superO-finder.png"

# 2.2 — focus Finder, click NEW FOLDER (item count should increment)
Send-MouseClick 400 320
Start-Sleep -Seconds 1
Capture "stage2-button-before-back.png"
Send-MouseClick 300 72
Start-Sleep -Seconds 2
Capture "stage2-button.png"

# 2.3–2.5 — lock session
Send-Scan @("e0", "5b", "26", "a6", "e0", "db")
Start-Sleep -Seconds 6
$lk = Invoke-Ssh "pgrep -xc slopos-lock || echo 0"
Write-Host "slopos-lock after Super+L: $lk"
Capture "stage2-locked.png"

# bypass attempt while locked
Send-Scan @("e0", "5b", "18", "98", "e0", "db")
Start-Sleep -Seconds 4
$f2 = Invoke-Ssh "pgrep -xc finder || echo 0"
Write-Host "finder while locked: $f2"
Capture "stage2-lock-nobypass.png"

# 2.6 — unlock via slopos-lock password
Send-Text "slopos-i"
Start-Sleep -Milliseconds 300
Send-Scan @("1c", "9c")
Start-Sleep -Seconds 6
Capture "stage2-unlocked.png"

# post-unlock Super+O
Send-Scan @("e0", "5b", "18", "98", "e0", "db")
Start-Sleep -Seconds 6
$f3 = Invoke-Ssh "pgrep -xc finder || echo 0"
Write-Host "finder after unlock Super+O: $f3"

Invoke-Ssh "grep -E 'spawned client|locked|unlock|finder' ~/qa-stage2/compositor.log 2>/dev/null | tail -25"
Write-Host "Stage 2 host orchestration complete."
