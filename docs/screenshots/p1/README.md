# P1 review screenshots

Captured on 2026-10-30 from real sessions, for reviewing the native SLOPOS
X11 desktop. They show the software running; they are **not** visual
conformance, accessibility or hardware evidence (Design Bible §5.2.7,
anti-pattern 19).

## Session captures

Taken with `scripts/capture-p1-screenshots.sh .work/p1/target/release`
(release build of this branch): Xvfb 1280×800, Openbox with the SLOPOS
Platinum theme, `slopos-shell` (top bar), `slopos-dock`, the bundled
SLOPOS-Platinum icon theme and wallpapers, and a throwaway home folder. The
container has no NetworkManager, sound server or BlueZ, so status items and
panels honestly show those as unavailable.

| File | What it shows |
|---|---|
| `desktop.png` | The desktop: Slate Blue picture painted by `slopos-session --paint-desktop`, the top bar's desktop menus, the Dock with Files, Terminal, Calculator, Media, Software and Control Panels. |
| `files.png` | Files: Places sidebar, theme icons, icon toolbar; its File/Edit/View/Go menus are in the top bar, not the window. |
| `files-view-menu.png` | Files' View menu opened from the top bar (Show Hidden Files is a check item that follows Files' state). |
| `image-viewer.png` | Image Viewer on a 1600×1000 PNG, fitted, with its File/View/Go menus in the top bar. |
| `media.png` | Media playing a VP8 test pattern (generated with GStreamer's `videotestsrc`); File/Playback menus in the top bar. |
| `control-panels-desktop.png` | Control Panels, Desktop page: bundled patterns with thumbnails. |
| `control-panels-sound.png` | Control Panels, Sound page with no sound server: the reason is shown and the controls are disabled. |
| `software.png` | Software with a QA catalogue signed by a key generated for this capture (not a real publisher key). |
| `calculator.png` | Calculator after typing `12*12=`. Calculator exports no menus yet, so the top bar shows window actions. |

## Live ISO captures

`iso-amd64-*.png` and `iso-arm64-*.png` come from `packaging/iso/test-live-iso.sh`:
the live ISO booted in QEMU (TCG, no KVM), autologin into SLOPOS-I, then
Super+Space, typing "files" and Return.
