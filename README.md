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

Use **System Settings → Appearance** to choose the SLOPOS-I appearance, interface font and Application Strip behavior. Use **System Settings → Desktop** to choose a bundled background or your own image.

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

## Visual Evidence & Screenshot Gallery

All 51 screenshots below are generated from clean runtime sessions inside isolated Linux containers (`ubuntu:24.04`), verifying visual presentation, theme consistency, global menu hosting, game rendering, and multi-monitor/resolution adaptability.

### 1. Canonical Desktop & System Menus (Platinum Light)

| Clean Desktop Baseline (1280×800) | Top System Menu Open |
|:---:|:---:|
| ![Clean Desktop](docs/screenshots/01_clean_desktop_platinum_1280x800.png) | ![System Menu](docs/screenshots/02_system_menu_open_1280x800.png) |

| Application Search Palette (`Super + Space`) | Freedesktop Toast Notification |
|:---:|:---:|
| ![Application Search](docs/screenshots/03_search_palette_open_1280x800.png) | ![Toast Notification](docs/screenshots/04_notification_1280x800.png) |

---

### 2. Modals, Context Menus & Session Controls

| Modal About Dialog | Shut Down Modal Dialog |
|:---:|:---:|
| ![About Dialog](docs/screenshots/05_modal_about_dialog_1280x800.png) | ![Shutdown Dialog](docs/screenshots/17_modal_shutdown_dialog_1280x800.png) |

| Restart Modal Dialog | Switch User Modal Dialog |
|:---:|:---:|
| ![Restart Dialog](docs/screenshots/22_modal_restart_dialog_1280x800.png) | ![Switch User Dialog](docs/screenshots/18_modal_switch_user_dialog_1280x800.png) |

| Desktop Right-Click Context Menu | File Manager Right-Click Context Menu |
|:---:|:---:|
| ![Desktop Context Menu](docs/screenshots/25_desktop_right_click_context_menu_1280x800.png) | ![File Context Menu](docs/screenshots/26_file_manager_right_click_context_menu_1280x800.png) |

---

### 3. Complete Appearance Matrix

SLOPOS-I ships four coherent appearances sharing the same compact ergonomics and typography:

| Platinum Light (Canonical) | Classic Contrast |
|:---:|:---:|
| ![Platinum Light](docs/screenshots/01_clean_desktop_platinum_1280x800.png) | ![Classic Contrast](docs/screenshots/19_classic_contrast_desktop_1280x800.png) |

| Graphite Dark (Modern Dark Mode) | OLED Dark (Pure Black `#000000`) |
|:---:|:---:|
| ![Graphite Dark](docs/screenshots/12_graphite_dark_desktop_1280x800.png) | ![OLED Dark](docs/screenshots/34_oled_dark_desktop_1280x800.png) |

| Classic System Menu Open | Classic Modal About Dialog |
|:---:|:---:|
| ![Classic System Menu](docs/screenshots/20_classic_contrast_system_menu_1280x800.png) | ![Classic About Dialog](docs/screenshots/21_classic_contrast_about_dialog_1280x800.png) |

| Graphite Dark System Settings | OLED Dark System Settings |
|:---:|:---:|
| ![Graphite Settings](docs/screenshots/13_graphite_settings_1280x800.png) | ![OLED Settings](docs/screenshots/35_oled_dark_settings_1280x800.png) |

| Custom Color & Font Studio | Bundled Wallpaper Chooser Dialog |
|:---:|:---:|
| ![Appearance Studio](docs/screenshots/48_custom_color_font_studio_1280x800.png) | ![Wallpaper Chooser](docs/screenshots/33_wallpaper_chooser_dialog_1280x800.png) |

| Wallpaper: Classic System Gray | Wallpaper: Slate Blue | Wallpaper: Retro Teal Grid |
|:---:|:---:|:---:|
| ![Classic System Gray](docs/screenshots/30_wallpaper_classic_system_gray_1280x800.png) | ![Slate Blue](docs/screenshots/31_wallpaper_slate_blue_1280x800.png) | ![Retro Teal Grid](docs/screenshots/32_wallpaper_retro_teal_grid_1280x800.png) |

---

### 4. Window Management, Fullscreen & Dock Dodge

| Multi-Window Focus & Stacking | Active Text Editor (Global Menu Host) |
|:---:|:---:|
| ![Multi-Window Focus](docs/screenshots/07_multi_window_focus_1280x800.png) | ![Mousepad Active](docs/screenshots/06_active_app_mousepad_1280x800.png) |

| True Fullscreen Video Player (MPV) | Maximized Window (Dock Dodged) | Dock Hover Overlap Reveal |
|:---:|:---:|:---:|
| ![True Fullscreen Video](docs/screenshots/46_fullscreen_video_mpv_1280x800.png) | ![Dock Dodged Full Height](docs/screenshots/47_dock_dodge_maximized_1280x800.png) | ![Dock Hover Overlap](docs/screenshots/49_dock_dodge_hover_overlap_1280x800.png) |

- **True Fullscreen Experience**: When video players (MPV, VLC), games (SuperTux, Doom), or web browsers enter fullscreen (`_NET_WM_STATE_FULLSCREEN`), both the top menu bar and bottom Application Strip dock unmap, providing a 100% unobstructed full-screen experience.
- **Dock Dodge / Autohide**: When enabled, maximized windows utilize the full display height below the top bar. Moving the pointer to the bottom edge reveals the Application Strip floating over the window.

---

### 5. Hardware Configuration & System Service Controls

| Date & Time Settings GUI | Network & Wi-Fi Connections GUI |
|:---:|:---:|
| ![Date and Time Settings](docs/screenshots/36_datetime_control_panel_1280x800.png) | ![Network Connections](docs/screenshots/37_network_wifi_gui_1280x800.png) |

| Volume Control & Mixer GUI (PulseAudio/PipeWire) | Bluetooth Devices GUI (BlueZ) |
|:---:|:---:|
| ![Volume Control Mixer](docs/screenshots/39_sound_audio_pavucontrol_1280x800.png) | ![Bluetooth Devices](docs/screenshots/38_bluetooth_gui_1280x800.png) |

---

### 6. Upstream Applications & Software Catalogue Matrix

| GNU Image Manipulation Program (GIMP) | Inkscape Vector Graphics Editor |
|:---:|:---:|
| ![GIMP](docs/screenshots/40_app_gimp_1280x800.png) | ![Inkscape](docs/screenshots/41_app_inkscape_1280x800.png) |

| VLC Media Player (Platinum Frame) | LibreOffice Writer (Word Processor) |
|:---:|:---:|
| ![VLC Media Player](docs/screenshots/42_app_vlc_media_player_1280x800.png) | ![LibreOffice Writer](docs/screenshots/43_app_libreoffice_writer_1280x800.png) |

| SuperTux 2D Platformer (Windowed) | SuperTux 2D Platformer (Fullscreen) |
|:---:|:---:|
| ![SuperTux Windowed](docs/screenshots/44_app_supertux_1280x800.png) | ![SuperTux Fullscreen](docs/screenshots/50_fullscreen_game_supertux_1280x800.png) |

| Classic Doom (Freedoom Phase 2 Windowed) | Classic Doom (Freedoom Fullscreen) |
|:---:|:---:|
| ![Classic Doom Windowed](docs/screenshots/24_game_doom_freedoom_1280x800.png) | ![Classic Doom Fullscreen](docs/screenshots/51_fullscreen_game_doom_1280x800.png) |

| Web Browser (Firefox Integration) | PCManFM File Manager (Global Menu) |
|:---:|:---:|
| ![Web Browser](docs/screenshots/23_web_browser_firefox_1280x800.png) | ![PCManFM File Manager](docs/screenshots/08_file_manager_pcmanfm_1280x800.png) |

| Mousepad (Native GTK Global Menu) | Xfce4 Terminal (Platinum Chrome) |
|:---:|:---:|
| ![Mousepad Text Editor](docs/screenshots/06_active_app_mousepad_1280x800.png) | ![Xfce4 Terminal](docs/screenshots/09_terminal_xfce4_1280x800.png) |

| Galculator (Scientific & Basic Calculator) | Ristretto Image Viewer |
|:---:|:---:|
| ![Galculator](docs/screenshots/27_calculator_galculator_1280x800.png) | ![Ristretto](docs/screenshots/28_image_viewer_ristretto_1280x800.png) |

| Document Viewer (Zathura) | Mozilla Thunderbird (Mail & Calendar) |
|:---:|:---:|
| ![Document Viewer](docs/screenshots/29_document_viewer_zathura_1280x800.png) | ![Mozilla Thunderbird](docs/screenshots/45_app_thunderbird_1280x800.png) |

| System Settings Control Panels Hub | Curated AppImage Software Catalogue |
|:---:|:---:|
| ![System Settings](docs/screenshots/11_system_settings_control_panels_1280x800.png) | ![Software Catalogue](docs/screenshots/10_software_catalogue_1280x800.png) |

---

### 7. Multi-Resolution & Scale Robustness

| Ultrawide Display Layout (3440×1440) | HiDPI 2× Scale Layout (2560×1600) |
|:---:|:---:|
| ![Ultrawide Display](docs/screenshots/14_ultrawide_desktop_3440x1440.png) | ![HiDPI Scale 2x](docs/screenshots/15_hidpi_scale2_2560x1600.png) |

| Multi-Window Stacking Workspace (1920×1080) | Standard Resolution Baseline (1280×800) |
|:---:|:---:|
| ![Multi-Window Workspace](docs/screenshots/16_workspace_multi_window_1920x1080.png) | ![Standard Baseline](docs/screenshots/01_clean_desktop_platinum_1280x800.png) |

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