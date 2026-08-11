# SLOPOS-I — System 7 Platinum Linux Desktop Environment

SLOPOS-I is a lightweight, highly polished Macintosh System 7 / Platinum-inspired Linux desktop operating environment built exclusively on mature X11 infrastructure (Openbox WM, GTK3 theme system, X.Org, and curated AppImages).

> **"Own the experience. Do not unnecessarily own the infrastructure."**

---

## Visual & Architecture Highlights

- **System 7 Top Menu Bar**: 24px full-width bar (`#DDDDDD`) featuring SLOPOS system logo (``), active application name in bold, global menu bar (`File`, `Edit`, `View`, `Window`, `Help`), and compact status indicators.
- **System 7 Window Chrome**: Openbox theme (`slopos-openbox`) with square corners, 1px black outline, titlebar pinstripes (`#E6E6E6` to `#CDCDCD`), Close box (top-left), and Zoom box (top-right).
- **Application Strip**: Beveled 3D Platinum box container at bottom center with raised 3D icon buttons and running app indicators.
- **Desktop Atmosphere**: Classic Macintosh cool-gray background (`#758090`).
- **AppImage Catalogue (`slopos-catalogue`)**: Curated software store with HTTPS download, SHA-256 integrity verification, and desktop launcher creation.
- **System Settings (`slopos-settings`)**: Unified preference GTK utility for Displays (`xrandr`), Audio (`pavucontrol`), Network (`nmcli`), Bluetooth (`blueman`), and Power (`upower`).

---

## Workspace Crates

- [`crates/slopos-session`](crates/slopos-session): X11 session supervisor binary overseeing Openbox and `slopos-shell`.
- [`crates/slopos-shell`](crates/slopos-shell): Desktop shell providing top menu bar, Spotlight launcher (`Super+Space`), Application Strip, and notifications.
- [`crates/slopos-catalogue`](crates/slopos-catalogue): Curated AppImage application catalogue and installer.
- [`crates/slopos-settings`](crates/slopos-settings): Unified GTK system configuration utility.

---

## Building and Running inside Docker (Primary Dev & QA Environment)

To compile and run the full automated X11 + Xvfb desktop test suite inside an Ubuntu container:

```bash
docker run --rm -v "%CD%:/workspace" -w /workspace ubuntu:24.04 bash /workspace/scripts/run-docker-qa.sh
```

---

## Building ISO Image

To build the bootable SLOPOS-I live ISO:

```bash
bash packaging/iso/build-iso.sh
```

---

## Emergency Desktop Recovery

If session configurations become corrupted, run the recovery utility:

```bash
bash scripts/slopos-recovery.sh
```
