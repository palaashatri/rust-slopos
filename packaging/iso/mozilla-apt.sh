# mozilla-apt.sh: sourced by the Ubuntu live-image builders. Not executable.
#
# Gives a build chroot Mozilla's official Firefox .deb repository instead of
# Ubuntu's snap-transitional firefox package. The sources entry and apt
# preferences are overlay files under packaging/iso/overlay. The signing key is
# downloaded here and accepted only if its primary fingerprint matches the
# constant below.

MOZILLA_APT_KEY_URL="https://packages.mozilla.org/apt/repo-signing-key.gpg"
# TODO(verify): primary fingerprint of Mozilla's APT repository signing key.
# Matched against the key served by packages.mozilla.org on 2026-11-21. Mozilla's
# install page (support.mozilla.org/kb/install-firefox-linux) could not be
# reached from the build environment, so cross-check it before a release build.
MOZILLA_APT_KEY_FPR="35BAA0B33E9EB396F59CA838C0BA5CE6DC6315A3"

# install_mozilla_apt CHROOT OVERLAY_DIR
# Requires curl and gpg on the host and ca-certificates inside CHROOT (the
# repository is HTTPS). Returns non-zero, after printing why, on any mismatch.
install_mozilla_apt() {
  local chroot="$1" overlay="$2" tmp listing pubs fpr
  tmp="$(mktemp -d)" || return 1
  if ! curl -fsSL --retry 3 "$MOZILLA_APT_KEY_URL" -o "$tmp/key.asc"; then
    echo "mozilla-apt: cannot download $MOZILLA_APT_KEY_URL" >&2
    rm -rf "$tmp"
    return 1
  fi
  listing="$(gpg --no-default-keyring --homedir "$tmp" --batch --with-colons \
    --show-keys "$tmp/key.asc" 2>/dev/null || true)"
  pubs="$(printf '%s\n' "$listing" | grep -c '^pub:' || true)"
  # The first fpr record after the pub record is the primary key's fingerprint.
  fpr="$(printf '%s\n' "$listing" | awk -F: '$1 == "fpr" { print $10; exit }')"
  if [[ "$pubs" != 1 || "$fpr" != "$MOZILLA_APT_KEY_FPR" ]]; then
    echo "mozilla-apt: signing key mismatch: got '${fpr:-none}' (${pubs} keys), expected $MOZILLA_APT_KEY_FPR" >&2
    rm -rf "$tmp"
    return 1
  fi
  install -d -m 0755 "$chroot/etc/apt/keyrings"
  install -m 0644 "$tmp/key.asc" "$chroot/etc/apt/keyrings/packages.mozilla.org.asc"
  rm -rf "$tmp"
  install -Dm644 "$overlay/etc/apt/sources.list.d/mozilla.sources" \
    "$chroot/etc/apt/sources.list.d/mozilla.sources"
  install -Dm644 "$overlay/etc/apt/preferences.d/mozilla-firefox" \
    "$chroot/etc/apt/preferences.d/mozilla-firefox"
}
