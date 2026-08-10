# Drive the unattended Arch install in the SLOPOS-I VM.
#
# archiso autologins to root on tty1. We serve arch-install.sh from the host
# and type a single curl|bash line into the guest via keyboard scancodes;
# everything after that is scripted inside the guest.
param(
    [string]$VmName    = "slopos-i-arch",
    [string]$ScriptDir = "$PSScriptRoot",
    [int]$HttpPort     = 8000,
    [int]$BootWaitSec  = 75,
    [switch]$SkipStart
)

$ErrorActionPreference = "Stop"
$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"

# --- PS/2 set-1 scancodes for the ASCII we need ------------------------------
$Map = @{
    '1'=0x02;'2'=0x03;'3'=0x04;'4'=0x05;'5'=0x06;'6'=0x07;'7'=0x08;'8'=0x09;'9'=0x0A;'0'=0x0B
    '-'=0x0C;'='=0x0D;'q'=0x10;'w'=0x11;'e'=0x12;'r'=0x13;'t'=0x14;'y'=0x15;'u'=0x16;'i'=0x17
    'o'=0x18;'p'=0x19;'['=0x1A;']'=0x1B;'a'=0x1E;'s'=0x1F;'d'=0x20;'f'=0x21;'g'=0x22;'h'=0x23
    'j'=0x24;'k'=0x25;'l'=0x26;';'=0x27;"'"=0x28;'`'=0x29;'\'=0x2B;'z'=0x2C;'x'=0x2D;'c'=0x2E
    'v'=0x2F;'b'=0x30;'n'=0x31;'m'=0x32;','=0x33;'.'=0x34;'/'=0x35;' '=0x39
}
# Characters that need Shift
$Shifted = @{
    '!'='1';'@'='2';'#'='3';'$'='4';'%'='5';'^'='6';'&'='7';'*'='8';'('='9';')'='0'
    '_'='-';'+'='=';'{'='[';'}'=']';':'=';';'"'="'";'~'='`';'|'='\';'<'=',';'>'='.';'?'='/'
}

function Send-Scancodes([int[]]$codes) {
    if ($codes.Count -eq 0) { return }
    $hex = ($codes | ForEach-Object { '{0:x2}' -f $_ })
    & $VBox controlvm $VmName keyboardputscancode @hex | Out-Null
}

function Send-Text([string]$text) {
    foreach ($ch in $text.ToCharArray()) {
        $c = [string]$ch
        if ($c -cmatch '^[A-Z]$') {
            $base = $Map[$c.ToLower()]
            Send-Scancodes @(0x2A, $base, ($base -bor 0x80), 0xAA)   # shift down, key, key up, shift up
        } elseif ($Shifted.ContainsKey($c)) {
            $base = $Map[$Shifted[$c]]
            Send-Scancodes @(0x2A, $base, ($base -bor 0x80), 0xAA)
        } elseif ($Map.ContainsKey($c)) {
            $base = $Map[$c]
            Send-Scancodes @($base, ($base -bor 0x80))
        } else {
            Write-Warning "no scancode for '$c' - skipped"
        }
        Start-Sleep -Milliseconds 12
    }
}

function Send-Enter { Send-Scancodes @(0x1C, 0x9C) }

# --- serve the installer from the host --------------------------------------
Write-Host "Serving $ScriptDir on port $HttpPort (guest reaches it at 10.0.2.2)"
$http = Start-Process -FilePath "python" `
    -ArgumentList "-m", "http.server", "$HttpPort", "--bind", "0.0.0.0", "--directory", "`"$ScriptDir`"" `
    -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 2

try {
    if (-not $SkipStart) {
        Write-Host "Starting VM headless"
        & $VBox startvm $VmName --type headless | Out-Null
    }

    Write-Host "Waiting ${BootWaitSec}s for the archiso live environment to reach a root prompt"
    Start-Sleep -Seconds $BootWaitSec
    & $VBox controlvm $VmName screenshotpng "$ScriptDir\boot-prompt.png" 2>$null | Out-Null

    # Wake the console, then type the one bootstrap line.
    Send-Enter
    Start-Sleep -Seconds 2
    $cmd = "curl -sL http://10.0.2.2:$HttpPort/arch-install.sh -o /root/i.sh && bash /root/i.sh 2>&1 | tee /root/install.log"
    Write-Host "Typing bootstrap: $cmd"
    Send-Text $cmd
    Send-Enter

    Write-Host ""
    Write-Host "Install is running inside the guest (pacstrap + cargo build; expect 20-40 min)."
    Write-Host "Watch it with:"
    Write-Host "  & '$VBox' controlvm $VmName screenshotpng shot.png"
    Write-Host "When it reboots, SSH becomes available:  ssh -p 2222 retro@127.0.0.1  (password: retro)"
}
finally {
    Write-Host "(host HTTP server pid $($http.Id) still running; stop it with: Stop-Process -Id $($http.Id))"
}
