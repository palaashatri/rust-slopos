# SLOPOS-I upstream browser integration

SLOPOS-I does not fork or patch Firefox, Chromium or Chrome. The integration
is layered in three deliberately bounded pieces:

1. `start-slopos-i` exports the normal X11 desktop identity and
   `GTK_THEME=slopos-gtk`. Browsers therefore inherit the SLOPOS GTK theme for
   native file pickers, permission dialogs, menus and other GTK-backed
   surfaces, and they see the SLOPOS XDG desktop identity and icon/data paths.
2. `chromium/manifest.json` is an optional unpacked Chromium theme. The
   `start-slopos-browser` wrapper loads it when Chromium is selected and the
   theme is installed. Chromium still owns its tab strip and browser behavior.
3. `firefox/userChrome.css` and `firefox/user.js` are an opt-in profile
   integration for Firefox. Run `scripts/install-browser-theme.sh firefox
   /absolute/profile` to back up an existing `userChrome.css`, enable the
   supported stylesheet preference, and add the SLOPOS frame/toolbar rules.

The helper never rewrites a browser binary, web content, or a user's profile
unless the user explicitly supplies a profile path. Firefox's `manifest.json`
is a reference for a signed upstream theme; it is not force-installed because
Firefox requires signed add-ons for normal distribution. Chromium's theme is
loaded through its documented extension mechanism, not a custom browser build.

## Scope and limits

Browser pages remain site-controlled. Browser-owned UI differs by engine and
release, so no-fork integration can align colors, borders, GTK dialogs and
desktop identity but cannot guarantee that every tab/omnibox surface matches
the SLOPOS shell. Upstream browser updates remain authoritative.

For an explicit profile install:

```bash
scripts/install-browser-theme.sh chromium /absolute/chromium-profile
scripts/install-browser-theme.sh firefox /absolute/firefox-profile
```

The Chromium profile install prints a launch command using
`SLOPOS_BROWSER_THEME_DIR`; Firefox must be restarted after the profile files
are changed.
