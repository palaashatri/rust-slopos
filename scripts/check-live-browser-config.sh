#!/usr/bin/env bash
# check-live-browser-config.sh: static checks for the live image's default
# browser. Covers Mozilla's Firefox apt repository and snap block (Ubuntu),
# firefox-esr (Debian), the default-application entries, and the launcher's
# candidate order.
#
# This script does not start a browser. A browser running in the live session
# is NOT covered here and must be recorded as NOT_RUN until a VM run shows it.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 2

failures=0
pass() { printf 'PASS %s\n' "$1"; }
fail() { printf 'FAIL %s\n' "$1" >&2; failures=$((failures + 1)); }

# expect DESCRIPTION COMMAND...: passes when COMMAND succeeds.
expect() {
  local desc="$1"
  shift
  if "$@" >/dev/null 2>&1; then pass "$desc"; else fail "$desc"; fi
}

# absent DESCRIPTION PATTERN FILE...: passes when no FILE matches PATTERN (ERE).
absent() {
  local desc="$1" pattern="$2"
  shift 2
  if grep -Eq -- "$pattern" "$@"; then fail "$desc"; else pass "$desc"; fi
}

# stanza_has FILE LINE...: passes when one blank-line-separated apt stanza
# contains every LINE exactly (comment lines and trailing blanks ignored).
stanza_has() {
  local file="$1"
  shift
  local IFS=$'\t' needles
  needles="$*"
  awk -v needles="$needles" '
    BEGIN { n = split(needles, need, "\t") }
    function judge(   i, ok) {
      ok = 1
      for (i = 1; i <= n; i++) if (!(need[i] in seen)) { ok = 0; break }
      if (ok) found = 1
      for (k in seen) delete seen[k]
    }
    /^[ \t]*#/ { next }
    /^[ \t]*$/ { judge(); next }
    { sub(/[ \t]+$/, ""); seen[$0] = 1 }
    END { judge(); exit(found ? 0 : 1) }
  ' "$file"
}

helper=packaging/iso/mozilla-apt.sh
src=packaging/iso/overlay/etc/apt/sources.list.d/mozilla.sources
pref=packaging/iso/overlay/etc/apt/preferences.d/mozilla-firefox
live=packaging/iso/build-live-iso.sh
ubuntu=packaging/iso/build-ubuntu-iso.sh
debian=packaging/iso/build-debian-iso.sh
mimeapps=assets/config/mimeapps.list
desktop=packaging/slopos-browser.desktop
launcher=scripts/start-slopos-browser
self=scripts/check-live-browser-config.sh

echo "== files"
for f in "$helper" "$src" "$pref" "$live" "$ubuntu" "$debian" "$mimeapps" "$desktop" "$launcher" "$self"; do
  expect "present: $f" test -f "$f"
done

echo "== syntax"
for f in "$helper" "$live" "$ubuntu" "$debian" "$launcher" "$self"; do
  expect "bash -n: $f" bash -n "$f"
done

echo "== Mozilla apt source (Ubuntu live builds)"
expect "source: Firefox .deb repo over HTTPS, suite mozilla, main" \
  stanza_has "$src" 'Types: deb' 'URIs: https://packages.mozilla.org/apt' \
  'Suites: mozilla' 'Components: main' \
  'Signed-By: /etc/apt/keyrings/packages.mozilla.org.asc'

echo "== apt pin and snap block"
expect "pin: Mozilla origin preferred at 1000" \
  stanza_has "$pref" 'Package: *' 'Pin: origin packages.mozilla.org' 'Pin-Priority: 1000'
expect "pin: Ubuntu firefox (snap wrapper) blocked at -1" \
  stanza_has "$pref" 'Package: firefox*' 'Pin: release o=Ubuntu' 'Pin-Priority: -1'
expect "pin: Ubuntu chromium-browser (snap wrapper) blocked at -1" \
  stanza_has "$pref" 'Package: chromium-browser*' 'Pin: release o=Ubuntu' 'Pin-Priority: -1'

echo "== signing key verification"
expect "key: fingerprint constant is 40 upper-case hex digits" \
  grep -Eq '^MOZILLA_APT_KEY_FPR="[0-9A-F]{40}"$' "$helper"
expect "key: fetched from packages.mozilla.org" \
  grep -Fq 'https://packages.mozilla.org/apt/repo-signing-key.gpg' "$helper"
expect "key: primary fingerprint compared before anything is installed" \
  grep -Fq '"$fpr" != "$MOZILLA_APT_KEY_FPR"' "$helper"
expect "key: installed at the path the sources entry's Signed-By names" \
  grep -Fq 'keyrings/packages.mozilla.org.asc"' "$helper"
expect "key: fingerprint still marked TODO until Mozilla's page is checked" \
  grep -Fq 'TODO(verify)' "$helper"

echo "== Ubuntu live builders"
for b in "$live" "$ubuntu"; do
  expect "$b: loads mozilla-apt.sh" grep -Fq 'source "$ROOT/packaging/iso/mozilla-apt.sh"' "$b"
  expect "$b: rejects Ubuntu's snap firefox build" grep -Fq '*snap*)' "$b"
  expect "$b: installs SLOPOS mimeapps as system defaults" grep -Fq 'mimeapps.list' "$b"
done
expect "live build: installs the verified Mozilla repo into the chroot" \
  grep -Fq 'install_mozilla_apt "$CHROOT" "$ROOT/packaging/iso/overlay"' "$live"
expect "live build: installs firefox from that repo" grep -Fq '${RUNTIME[*]} firefox' "$live"
expect "ubuntu build: installs the verified Mozilla repo into the chroot" \
  grep -Fq 'install_mozilla_apt chroot "$ROOT/packaging/iso/overlay"' "$ubuntu"
expect "ubuntu build: installs firefox from that repo" grep -Fxq '    firefox' "$ubuntu"
absent "no snap or Chromium packages requested by the ISO builders" \
  'snapd|snap install|chromium' "$live" "$ubuntu" "$debian"

echo "== Debian live builder"
expect "debian build: firefox-esr from Debian's repository" grep -Fxq 'firefox-esr' "$debian"
expect "debian build: installs SLOPOS mimeapps as system defaults" \
  grep -Fq 'config/includes.chroot/etc/xdg/mimeapps.list' "$debian"

echo "== default applications"
expect "mimeapps: text/html -> slopos-browser.desktop" \
  grep -Fxq 'text/html=slopos-browser.desktop' "$mimeapps"
expect "mimeapps: x-scheme-handler/http -> slopos-browser.desktop" \
  grep -Fxq 'x-scheme-handler/http=slopos-browser.desktop' "$mimeapps"
expect "mimeapps: x-scheme-handler/https -> slopos-browser.desktop" \
  grep -Fxq 'x-scheme-handler/https=slopos-browser.desktop' "$mimeapps"
expect "desktop entry: runs start-slopos-browser" grep -Fxq 'Exec=start-slopos-browser %U' "$desktop"
expect "desktop entry: TryExec start-slopos-browser" grep -Fxq 'TryExec=start-slopos-browser' "$desktop"
expect "desktop entry: MimeType covers text/html" grep -Eq '^MimeType=.*text/html;' "$desktop"
expect "desktop entry: MimeType covers http and https" \
  grep -Eq '^MimeType=.*x-scheme-handler/http;.*x-scheme-handler/https;' "$desktop"

echo "== launcher"
expect "launcher: candidate order tries firefox before firefox-esr" \
  grep -Fq 'for candidate in firefox firefox-esr chromium' "$launcher"
stub="$(mktemp -d)" || exit 2
printf '#!/bin/sh\necho "STUB firefox $*"\n' >"$stub/firefox"
chmod 755 "$stub/firefox"
got="$(env -i PATH="$stub:/usr/bin:/bin" HOME="$stub" /bin/bash "$ROOT/$launcher" https://example.invalid/ 2>&1)"
rm -rf "$stub"
expect "launcher: runs the firefox on PATH (stub, no browser started)" \
  test "$got" = "STUB firefox https://example.invalid/"

echo
if ((failures)); then
  echo "check-live-browser-config: $failures check(s) FAILED"
  exit 1
fi
echo "check-live-browser-config: all checks passed (runtime browser start is NOT_RUN)"
