# SLOPOS-I

SLOPOS-I is an experimental Linux desktop environment with its own shell, settings hub, application launcher, software catalogue, themes, wallpapers and session experience.

It is built on **X11** and deliberately reuses mature Linux components for low-level jobs such as window management, networking, audio, Bluetooth and power management. The goal is to make those pieces feel like one coherent desktop rather than asking users to assemble them manually.

![SLOPOS-I desktop](docs/screenshots/01_clean_desktop_platinum_1280x800.png)

> **Project status: alpha.** SLOPOS-I is usable for development and testing, but it is not yet a finished consumer distribution. In particular, downloadable installation media, public package repositories and non-x86 image validation are still release blockers.

## What is a desktop environment?

If you are new to Linux, the desktop environment is the part of the system you see and interact with after signing in: the top bar, application launcher, window appearance, settings screens, notifications and desktop background.

SLOPOS-I does **not** replace Linux itself. It runs on top of a Linux distribution and provides a different desktop experience.

## What SLOPOS-I includes

- A top system bar with the active application, menus, search and status information.
- An Application Strip for launching commonly used programs.
- Application Search with `Super + Space`.
- A Settings hub for display, audio, network, Bluetooth, power, appearance and desktop preferences.
- A curated AppImage Software Catalogue with integrity checks.
- Light, dark, high-contrast and OLED-oriented appearances.
- Wallpapers and desktop personalization.
- Notifications, session controls and recovery tooling.
- Integration with normal Linux applications such as Firefox, PCManFM, Xfce Terminal, Mousepad, MPV, VLC, GIMP, Inkscape and LibreOffice when they are installed.

SLOPOS-I currently uses **Openbox** for window management. NetworkManager, PipeWire/WirePlumber, BlueZ, UPower and other established Linux services continue to manage the hardware and operating-system state underneath the desktop.

## Current installation status

There are three different ideas that are easy to confuse:

1. **Source installation** — available now for developers and testers.
2. **Installable package files** — GitHub Actions contains workflows for Debian-family `.deb` and Arch package artifacts.
3. **Normal package-manager installation** — **not available yet**. There is no public SLOPOS-I APT or Pacman repository, so commands such as `sudo apt install slopos-i` are not yet a supported installation path.

Bootable media is also still alpha work. The repository contains x86_64 Arch and Debian live-image builders, but the release process does not yet provide a validated public ISO matrix. ARM64 and RISC-V images are not currently validated release artifacts.

### Install from source on Ubuntu or Debian-family systems

You will need Git and an internet connection for the initial setup.

```bash
git clone https://github.com/palaashatri/rust-slopos.git
cd rust-slopos
sudo ./install.sh --distro ubuntu
```

### Install from source on Arch Linux

```bash
git clone https://github.com/palaashatri/rust-slopos.git
cd rust-slopos
sudo ./install.sh --distro arch
```

The installer installs the required dependencies, builds SLOPOS-I and registers an X11 session with your display manager.

After installation:

1. Log out of your current desktop session.
2. On the login screen, open the session/desktop chooser.
3. Select **SLOPOS-I**.
4. Sign in normally.

The source installer is intended for testing. It does not yet provide the polished upgrade and uninstall lifecycle expected from a normal distribution package repository.

## Basic usage

| Action | Shortcut |
|---|---|
| Search for an application | `Super + Space` |
| Open the SLOPOS system menu | `Ctrl + F2` |
| Switch between windows | `Alt + Tab` |
| Close the active window | `Super + Q` |

### Application Search

Press `Super + Space`, type the name of an installed application, use the arrow keys to select a result and press `Enter` to launch it. Press `Escape` to close Search.

### Settings

Open **System Settings** from the Application Strip or system menu. Some panels are SLOPOS-I interfaces, while others open the established Linux utility that actually controls that subsystem.

If a required utility is missing, SLOPOS-I should show that panel as unavailable rather than pretending a setting works.

### Software Catalogue

The Software Catalogue is for curated **AppImage** applications. It is separate from your distribution's package manager. SLOPOS-I verifies trusted metadata and file integrity before completing a catalogue installation.

### Appearance and wallpaper

Use **System Settings → Appearance** to choose an appearance and customize supported colors and typography. Use the desktop/wallpaper controls to choose a background.

## Recovery

If the desktop configuration becomes unusable, run:

```bash
slopos-recovery
```

This restores the known SLOPOS-I defaults for the configuration files managed by the recovery tool. It does not reinstall Linux or erase your personal files.

## Supported platform

SLOPOS-I is currently:

- Linux-only.
- X11-only.
- Developed primarily around Arch and Debian/Ubuntu-family packaging.
- Most thoroughly exercised on x86_64.

ARM64 and RISC-V are intended targets, but package declarations alone are not treated as proof of support. They need native or emulated build, boot and desktop acceptance coverage before they are advertised as supported release platforms.

Wayland is not part of the SLOPOS-I product contract at this stage.

## Architecture in plain language

```text
Linux
  ↓
X11 display server
  ↓
Openbox window manager
  ↓
SLOPOS-I session and shell
  ├─ top bar
  ├─ application search
  ├─ Application Strip
  ├─ notifications
  ├─ Settings
  └─ Software Catalogue
  ↓
Your normal Linux applications
```

SLOPOS-I tries to **own the desktop experience without unnecessarily reimplementing mature system infrastructure**. That keeps the project smaller and lets existing Linux applications continue to work normally.

## Screenshots

| Search | Settings | Software Catalogue |
|:---:|:---:|:---:|
| ![Application Search](docs/screenshots/03_search_palette_open_1280x800.png) | ![System Settings](docs/screenshots/11_system_settings_control_panels_1280x800.png) | ![Software Catalogue](docs/screenshots/10_software_catalogue_1280x800.png) |

| File Manager | Terminal | Dark appearance |
|:---:|:---:|:---:|
| ![File Manager](docs/screenshots/08_file_manager_pcmanfm_1280x800.png) | ![Terminal](docs/screenshots/09_terminal_xfce4_1280x800.png) | ![Dark desktop](docs/screenshots/12_graphite_dark_desktop_1280x800.png) |

Screenshots are development evidence, not a substitute for install/boot validation on real target systems.

## For contributors

The public README is intentionally written for users. Development rules, architecture constraints and completion criteria live in [`AGENTS.md`](AGENTS.md). Evidence-backed readiness and known gaps live in [`TRUTH.md`](TRUTH.md).

Useful checks include:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/run-release-qa.sh
```

## License

SLOPOS-I is licensed under the MIT License. See [`LICENSE`](LICENSE).

Third-party software and assets keep their own licenses. See [`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt) for repository-specific notices.