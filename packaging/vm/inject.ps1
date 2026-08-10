# Type a command into the VM's console via PS/2 scancodes and press Enter.
# Used to bootstrap access before SSH keys are installed.
#   pwsh -File inject.ps1 -Command "curl -sL http://10.0.2.2:8000/qa-vm.sh | bash"
param(
    [string]$VmName = "slopos-i-arch",
    [Parameter(Mandatory=$true)][string]$Command,
    [switch]$NoEnter,
    [int]$PreEnter = 1
)

$ErrorActionPreference = "Stop"
$VBox = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"

$Map = @{
    '1'=0x02;'2'=0x03;'3'=0x04;'4'=0x05;'5'=0x06;'6'=0x07;'7'=0x08;'8'=0x09;'9'=0x0A;'0'=0x0B
    '-'=0x0C;'='=0x0D;'q'=0x10;'w'=0x11;'e'=0x12;'r'=0x13;'t'=0x14;'y'=0x15;'u'=0x16;'i'=0x17
    'o'=0x18;'p'=0x19;'['=0x1A;']'=0x1B;'a'=0x1E;'s'=0x1F;'d'=0x20;'f'=0x21;'g'=0x22;'h'=0x23
    'j'=0x24;'k'=0x25;'l'=0x26;';'=0x27;"'"=0x28;'`'=0x29;'\'=0x2B;'z'=0x2C;'x'=0x2D;'c'=0x2E
    'v'=0x2F;'b'=0x30;'n'=0x31;'m'=0x32;','=0x33;'.'=0x34;'/'=0x35;' '=0x39
}
$Shifted = @{
    '!'='1';'@'='2';'#'='3';'$'='4';'%'='5';'^'='6';'&'='7';'*'='8';'('='9';')'='0'
    '_'='-';'+'='=';'{'='[';'}'=']';':'=';';'"'="'";'~'='`';'|'='\';'<'=',';'>'='.';'?'='/'
}

function Send-Scancodes([int[]]$codes) {
    if ($codes.Count -eq 0) { return }
    $hex = ($codes | ForEach-Object { '{0:x2}' -f $_ })
    & $VBox controlvm $VmName keyboardputscancode @hex | Out-Null
}
function Send-Enter { Send-Scancodes @(0x1C, 0x9C) }

function Send-Text([string]$text) {
    foreach ($ch in $text.ToCharArray()) {
        $c = [string]$ch
        if ($c -cmatch '^[A-Z]$') {
            $b = $Map[$c.ToLower()]; Send-Scancodes @(0x2A, $b, ($b -bor 0x80), 0xAA)
        } elseif ($Shifted.ContainsKey($c)) {
            $b = $Map[$Shifted[$c]]; Send-Scancodes @(0x2A, $b, ($b -bor 0x80), 0xAA)
        } elseif ($Map.ContainsKey($c)) {
            $b = $Map[$c]; Send-Scancodes @($b, ($b -bor 0x80))
        } else {
            Write-Warning "no scancode for '$c' - skipped"
        }
        Start-Sleep -Milliseconds 12
    }
}

for ($i = 0; $i -lt $PreEnter; $i++) { Send-Enter; Start-Sleep -Milliseconds 400 }
Send-Text $Command
if (-not $NoEnter) { Send-Enter }
Write-Host "typed: $Command"
