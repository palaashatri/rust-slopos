#!/usr/bin/env bash
# Install the optional, no-fork SLOPOS browser integration into an explicit
# user profile. The caller chooses the profile so an existing browser profile
# is never guessed or silently overwritten.
set -euo pipefail

usage() {
  echo "Usage: $0 firefox|chromium|chromium-browser|chrome|google-chrome|google-chrome-stable PROFILE_DIR" >&2
  exit 2
}

[[ $# -eq 2 ]] || usage
BROWSER="$1"
PROFILE="$2"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_PREFIX="$(cd -- "$SCRIPT_DIR/.." && pwd)"
SLOPOS_SHARE_DIR="${SLOPOS_SHARE_DIR:-$INSTALL_PREFIX/share}"
SOURCE_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
if [[ -d "$SOURCE_ROOT/packaging/browser" ]]; then
  BROWSER_RESOURCE_DIR="$SOURCE_ROOT/packaging/browser"
else
  BROWSER_RESOURCE_DIR="$SLOPOS_SHARE_DIR/slopos-i/browser"
fi
STAMP="$(date +%Y%m%d-%H%M%S)"

case "$PROFILE" in
  /*) ;;
  *) echo "PROFILE_DIR must be an absolute path: $PROFILE" >&2; exit 2 ;;
esac

mkdir -p "$PROFILE"

case "$BROWSER" in
  chromium|chromium-browser|chrome|google-chrome|google-chrome-stable)
    target="$PROFILE/slopos-browser-theme"
    # Never recursively delete an existing profile-owned theme. Preserve it
    # under a unique timestamped backup before replacing the unpacked theme.
    if [[ -e "$target" || -L "$target" ]]; then
      backup="$PROFILE/slopos-browser-theme.backup.$STAMP"
      backup_index=0
      while [[ -e "$backup" || -L "$backup" ]]; do
        backup_index=$((backup_index + 1))
        backup="$PROFILE/slopos-browser-theme.backup.$STAMP.$backup_index"
      done
      mv -- "$target" "$backup"
      printf 'Existing Chromium theme moved to:\n  %s\n' "$backup" >&2
    fi
    mkdir -p "$target"
    cp -a "$BROWSER_RESOURCE_DIR/chromium/." "$target/"
    chmod -R u+rwX,go+rX "$target"
    cat <<EOF
Installed the Chromium theme files at:
  $target

Launch the upstream browser through SLOPOS with:
  SLOPOS_BROWSER_THEME=1 SLOPOS_BROWSER_THEME_DIR="$target" start-slopos-browser
EOF
    ;;
  firefox)
    chrome_dir="$PROFILE/chrome"
    mkdir -p "$chrome_dir"
    css="$chrome_dir/userChrome.css"
    slopos_css="$chrome_dir/slopos-i.css"
    if [[ -f "$css" ]] && ! grep -Fq 'slopos-i.css' "$css"; then
      cp -p "$css" "$css.slopos-backup.$STAMP"
      tmp="$css.tmp.$STAMP"
      {
        printf '%s\n' '@import url("slopos-i.css");'
        cat "$css"
      } > "$tmp"
      mv "$tmp" "$css"
    elif [[ ! -f "$css" ]]; then
      printf '%s\n' '@import url("slopos-i.css");' > "$css"
    fi
    install -m644 "$BROWSER_RESOURCE_DIR/firefox/userChrome.css" "$slopos_css"

    user_js="$PROFILE/user.js"
    firefox_pref='user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);'
    firefox_pref_re='^[[:space:]]*user_pref\([[:space:]]*"toolkit\.legacyUserProfileCustomizations\.stylesheets"[[:space:]]*,'
    firefox_pref_true_re="${firefox_pref_re}[[:space:]]*true[[:space:]]*\\);"
    firefox_pref_false_re="${firefox_pref_re}[[:space:]]*false[[:space:]]*\\);"
    if [[ ! -f "$user_js" ]]; then
      printf '%s\n' "$firefox_pref" > "$user_js"
    elif grep -Eq "$firefox_pref_false_re" "$user_js" || \
         ! grep -Eq "$firefox_pref_true_re" "$user_js"; then
      # Normalize any existing true/false declaration so a stale `false`
      # cannot win by appearing later in user.js. Keep a byte-for-byte backup
      # before rewriting the explicit profile the caller supplied.
      backup="$user_js.slopos-backup.$STAMP"
      backup_index=0
      while [[ -e "$backup" || -L "$backup" ]]; do
        backup_index=$((backup_index + 1))
        backup="$user_js.slopos-backup.$STAMP.$backup_index"
      done
      cp -p "$user_js" "$backup"
      tmp="$user_js.tmp.$STAMP"
      sed -E "/$firefox_pref_re/d" "$user_js" > "$tmp"
      printf '%s\n' "$firefox_pref" >> "$tmp"
      mv -- "$tmp" "$user_js"
    fi
    install -m644 "$BROWSER_RESOURCE_DIR/firefox/manifest.json" "$PROFILE/slopos-platinum-theme-manifest.json"
    cat <<EOF
Installed the optional Firefox chrome integration at:
  $chrome_dir

Restart Firefox to load the profile CSS. The manifest at
  $PROFILE/slopos-platinum-theme-manifest.json
is a reference theme for a signed upstream WebExtension; it is not force-installed.
EOF
    ;;
  *) usage ;;
esac
