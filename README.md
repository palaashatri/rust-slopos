# SLOPOS-I — X11 Platinum & Classic Macintosh Desktop

SLOPOS-I is a lightweight, highly polished classic-Macintosh / System 7 / Platinum-inspired Linux desktop environment built strictly on mature X11 infrastructure.

> **Own the experience. Do not unnecessarily own the infrastructure.**

![SLOPOS-I Clean Desktop](docs/screenshots/01_clean_desktop_platinum_1280x800.png)

## Current Status

SLOPOS-I has achieved **100/100 Docker-validated product readiness** in [`TRUTH.md`](TRUTH.md) under the normative product contract defined in [`AGENTS.md`](AGENTS.md). All 12 evaluation domains are verified with deterministic, reproducible test suites inside containerized X11 environments.

---

## Complete UI/UX Functionality & Architecture Guide

SLOPOS-I delivers a coherent, deliberate operating system experience while delegating low-level plumbing to proven Linux system daemons. Every button, menu, dialog, and slider is connected to real functionality with a strict **Zero Fake Functionality** release policy.

```text
Linux + systemd/logind/udev
  ├─ NetworkManager (nm-connection-editor)
  ├─ PipeWire / PulseAudio (pavucontrol)
  ├─ BlueZ (blueman-manager)
  ├─ UPower (xfce4-power-manager)
  ├─ systemd-timesyncd / timedatectl (slopos-settings --datetime)
  └─ distro package manager (pacman / apt)
        ↓
X.Org-compatible X11 server
        ↓
Openbox stacking/floating window manager
        ↓
SLOPOS session supervisor (slopos-session)
  ├─ top menu/system bar (slopos-shell)
  ├─ application Search palette (Super+Space)
  ├─ compact Application Strip
  ├─ desktop wallpaper engine (slopos-wallpaper)
  ├─ freedesktop D-Bus notifications
  ├─ Software Catalogue (slopos-catalogue)
  └─ Control Panels hub (slopos-settings)
        ↓
Mature upstream applications + verified AppImages
```

---

## Complete Visual Gallery & UI/UX Showcase

### 1. Desktop & System Navigation

| System Menu (`Ctrl+F2`) | Application Search Palette (`Super+Space`) |
|:---:|:---:|
| ![System Menu Open](docs/screenshots/02_system_menu_open_1280x800.png) | ![Search Palette Open](docs/screenshots/03_search_palette_open_1280x800.png) |

- **Top Bar (`slopos-shell`)**: 22px fixed top system bar containing the SLOPOS mark, active window focus tracking label, dynamic global application menu host, and live system status indicators (Audio, Network, Battery, Local Time).
- **System Menu**: Triggered by clicking the mark at the top-left or pressing `Ctrl+F2`. Provides immediate access to *About SLOPOS-I*, *Control Panels…*, *Appearance* submenu, *Lock Screen*, *Switch User…*, *Sleep*, *Log Out…*, *Restart…*, and *Shut Down…*.
- **Application Search Palette (`Super+Space`)**: Fast keyboard-driven fuzzy launcher that parses all XDG desktop entries across the system, ranking exact name matches above description keywords. Dismissible with `Escape`.

---

### 2. Desktop Right-Click & Context Menus

| Desktop Right-Click Menu (Submenus & Shortcuts) | File Manager Context Menu (Desktop Folder) |
|:---:|:---:|
| ![Desktop Right Click Menu](docs/screenshots/25_desktop_right_click_context_menu_1280x800.png) | ![File Manager Context Menu](docs/screenshots/26_file_manager_right_click_context_menu_1280x800.png) |

- **Desktop Right-Click Menu**:
  - **New Folder**: Instantly creates a new directory on `~/Desktop` and opens it in PCManFM.
  - **New Text Document**: Creates `~/Desktop/Untitled.txt` and opens it directly in Mousepad.
  - **Open With… Submenu**: Fast access to Mousepad, Ristretto, Zathura, Firefox, and MPV.
  - **Desktop Wallpapers Submenu**: Instant preset switching between authentic retro patterns (Classic Gray, Platinum Slate, Vintage Blue, Retro Teal, OLED Pure Dark).
  - **Direct System Controls**: Quick links to *Desktop & Wallpaper…*, *Date & Time Settings…*, *Display Settings…*, *Volume & Audio…*, *Network Connections…*, *System Settings Hub*, and *Software Catalogue*.

---

### 3. Desktop Wallpaper Engine & Retro Wallpapers Showcase

| Classic System Gray (50% 1-Bit Dither) | Vintage Mac Blue (Classic Tweed) |
|:---:|:---:|
| ![Classic Gray Wallpaper](docs/screenshots/30_wallpaper_classic_system_gray_1280x800.png) | ![Vintage Blue Wallpaper](docs/screenshots/31_wallpaper_vintage_mac_blue_1280x800.png) |

| Retro Teal Grid (90s Matrix) | Desktop & Wallpaper Chooser GUI |
|:---:|:---:|
| ![Retro Teal Wallpaper](docs/screenshots/32_wallpaper_retro_teal_grid_1280x800.png) | ![Wallpaper Chooser Dialog](docs/screenshots/33_wallpaper_chooser_dialog_1280x800.png) |

- **Original Retro Wallpaper Collection**:
  1. `01_classic_system_gray.png`: Classic System 6/7 50% 1-bit monochrome checkerboard dither.
  2. `02_platinum_cool_slate.png`: Canonical Platinum cool slate (`#758090`) fine matrix grid.
  3. `03_vintage_mac_blue.png`: Classic Mac OS 8/9 vintage blue tweed pattern (`#3A5F8B`).
  4. `04_retro_teal_grid.png`: 1990s retro desktop teal matrix (`#008080`).
  5. `05_oled_pure_dark.png`: OLED obsidian constellation (`#000000`).
- **Wallpaper Management (`slopos-wallpaper`)**: CLI and GUI tool allowing users to get, set, list, and apply background images with `fill`, `tile`, or `center` modes.
- **Session Persistence**: Active wallpaper choice is saved to `~/.config/slopos-i/wallpaper` and automatically restored by `slopos-session` on login.

---

### 4. Quadruple Appearance System & Windows XP-Style Custom RGB Color Studio

| Classic Macintosh Desktop (6-Stripe Titlebars) | Classic System Menu (Inverted Black Selection) |
|:---:|:---:|
| ![Classic Mac Desktop](docs/screenshots/19_classic_mac_desktop_1280x800.png) | ![Classic System Menu](docs/screenshots/20_classic_mac_system_menu_1280x800.png) |

| Platinum Light Appearance (Canonical) | Graphite Dark Theme Presentation |
|:---:|:---:|
| ![Platinum Light](docs/screenshots/01_clean_desktop_platinum_1280x800.png) | ![Graphite Dark](docs/screenshots/12_graphite_dark_desktop_1280x800.png) |

| OLED Pure Dark Desktop (`#000000`) | OLED Dark Settings Presentation |
|:---:|:---:|
| ![OLED Dark Desktop](docs/screenshots/34_oled_dark_desktop_1280x800.png) | ![OLED Dark Settings](docs/screenshots/35_oled_dark_settings_1280x800.png) |

| Custom Colors & Fonts Studio (Windows XP Style) | Desktop & Wallpaper Chooser with Previews |
|:---:|:---:|
| ![Custom Color & Font Studio](docs/screenshots/48_custom_color_font_studio_1280x800.png) | ![Wallpaper Chooser Dialog](docs/screenshots/33_wallpaper_chooser_dialog_1280x800.png) |

1. **Classic Macintosh (System 6/7)**:
   - Iconic 6-stripe horizontal pinstripe titlebars with centered white cutout title box.
   - 4px rounded-rectangle push buttons with 1px black outline.
   - Distinctive 3px thick rounded outer ring on default modal buttons (`Return` action).
   - Inverted pure black (`#000000`) hover and selection highlights with pure white (`#FFFFFF`) text.
2. **Platinum (Light - Canonical Default)**:
   - System 7/8 Platinum neutral gray face (`#D9D9D9` to `#DDDDDD`) with restrained 1px raised/sunken 3D bevels.
   - Classic navy selection (`#000080`) with white text.
   - Muted slate desktop background (`#758090`).
3. **Graphite (Dark)**:
   - Sleek dark charcoal surfaces (`#25272B` to `#2C2E33`) with high-contrast active window frames.
4. **OLED Dark (Pure Black `#000000`)**:
   - True deep pitch-black (`#000000`) surfaces across all windows, top bar, and Application Strip dock.
   - Crisp `#3A3C45` borders with vibrant high-contrast `#2563EB` selection accents and pure white typography.
5. **Windows XP-Style Custom RGB Color & Typography Studio**:
   - Complete RGB color customization for Accent/Selection Color, Selection Text Color, Window & Panel Surface, Main Text Color, and Desktop Background.
   - 8 Quick Accent color swatches (Navy, Azure, Teal, Slate, Purple, Crimson, Forest, Amber).
   - User interface font family and size chooser with system-wide instant application.

---

### 5. True Fullscreen & Dock Dodge Window Management

| True Fullscreen Video & Gaming (MPV/VLC/Games) | Dock Dodge on Maximized Windows |
|:---:|:---:|
| ![True Fullscreen Video](docs/screenshots/46_fullscreen_video_mpv_1280x800.png) | ![Dock Dodge Maximized](docs/screenshots/47_dock_dodge_maximized_1280x800.png) |

- **True Fullscreen Experience**: When any video player (MPV, VLC), game (Doom, SuperTux), or web browser enters fullscreen (`_NET_WM_STATE_FULLSCREEN`), both the top menu bar and bottom Application Strip dock automatically unmap and hide, delivering a 100% unobstructed full-screen experience.
- **Dock Dodge / Autohide**: When enabled in Control Panels, the Application Strip dock automatically dodges (slides down) when the active window is maximized, maximizing usable screen real estate. Hovering near the bottom 18px of the screen smoothly brings the dock back into view.

---

### 6. Hardware Configuration & System Service Controls

| Date & Time Settings GUI | Network & Wi-Fi Connections GUI |
|:---:|:---:|
| ![Date and Time Settings](docs/screenshots/36_datetime_control_panel_1280x800.png) | ![Network Connections](docs/screenshots/37_network_wifi_gui_1280x800.png) |

| Volume Control & Mixer GUI (PulseAudio/PipeWire) | Bluetooth Devices GUI (BlueZ) |
|:---:|:---:|
| ![Volume Control Mixer](docs/screenshots/39_sound_audio_pavucontrol_1280x800.png) | ![Bluetooth Devices](docs/screenshots/38_bluetooth_gui_1280x800.png) |

- **Date & Time Control Panel**: Accessible by clicking the top bar clock, via System Settings, or from the desktop context menu. Displays active system time, timezone selector, and Network Time Protocol (NTP) toggle.
- **Network Connections (`nm-connection-editor`)**: Full Wi-Fi, Ethernet, VPN, and cellular profile configuration.
- **Sound & Volume Mixer (`pavucontrol`)**: Live audio playback/recording levels, output device switching, and per-application volume sliders.
- **Bluetooth Manager (`blueman-manager`)**: Bluetooth adapter pairing, audio sink connection, and device management.

---

### 7. Upstream Applications & Software Catalogue Matrix

| GNU Image Manipulation Program (GIMP) | Inkscape Vector Graphics Editor |
|:---:|:---:|
| ![GIMP](docs/screenshots/40_app_gimp_1280x800.png) | ![Inkscape](docs/screenshots/41_app_inkscape_1280x800.png) |

| VLC Media Player (Platinum Frame) | LibreOffice Writer (Word Processor) |
|:---:|:---:|
| ![VLC Media Player](docs/screenshots/42_app_vlc_media_player_1280x800.png) | ![LibreOffice Writer](docs/screenshots/43_app_libreoffice_writer_1280x800.png) |

| SuperTux Classic 2D Platformer | Classic Doom (Freedoom Phase 2) |
|:---:|:---:|
| ![SuperTux](docs/screenshots/44_app_supertux_1280x800.png) | ![Classic Doom](docs/screenshots/24_game_doom_freedoom_1280x800.png) |

| Mozilla Firefox (Native Titlebar & Global Menu) | PCManFM File Manager (Custom Icons) |
|:---:|:---:|
| ![Mozilla Firefox](docs/screenshots/23_web_browser_firefox_1280x800.png) | ![PCManFM File Manager](docs/screenshots/08_file_manager_pcmanfm_1280x800.png) |

| Mousepad (Native GTK Global Menu) | Xfce4 Terminal (Platinum Chrome) |
|:---:|:---:|
| ![Mousepad Text Editor](docs/screenshots/06_active_app_mousepad_1280x800.png) | ![Xfce4 Terminal](docs/screenshots/09_terminal_xfce4_1280x800.png) |

| Galculator (Scientific & Basic Calculator) | Ristretto Image Viewer |
|:---:|:---:|
| ![Galculator](docs/screenshots/27_calculator_galculator_1280x800.png) | ![Ristretto](docs/screenshots/28_image_viewer_ristretto_1280x800.png) |

| Mozilla Thunderbird (Mail, Calendar & RSS) | Curated AppImage Software Catalogue |
|:---:|:---:|
| ![Mozilla Thunderbird](docs/screenshots/45_app_thunderbird_1280x800.png) | ![Software Catalogue](docs/screenshots/10_software_catalogue_1280x800.png) |

- **Curated AppImage Software Catalogue (`slopos-catalogue`)**: Fail-closed application installer with cryptographic SHA-256 verification and atomic installation for productivity and creative tools (Thunderbird, Firefox ESR, Chocolate Doom, SuperTux, Kdenlive, Inkscape, GIMP, Audacity). Provides live asynchronous status updates, single-click launching of installed applications, and clean uninstallation.

---

### 8. Multi-Resolution & Scale Robustness

| Ultrawide Layout (3440×1440) | HiDPI 2× Scale Layout (2560×1600) |
|:---:|:---:|
| ![Ultrawide Display](docs/screenshots/14_ultrawide_desktop_3440x1440.png) | ![HiDPI Scale 2x](docs/screenshots/15_hidpi_scale2_2560x1600.png) |

| Multi-Window Stacking Workspace State (1920×1080) | Standard Resolution Baseline (1280×800) |
|:---:|:---:|
| ![Multi-Window Workspace](docs/screenshots/16_workspace_multi_window_1920x1080.png) | ![Standard Baseline](docs/screenshots/01_clean_desktop_platinum_1280x800.png) |

- **Geometry Adaptability**: Seamless execution across display resolutions from 1280×800 and 1366×768 up to 3440×1440 Ultrawide, 3840×2160 4K, and 5120×2880 5K.
- **HiDPI Support**: Full integer scaling (`GDK_SCALE=2`) with crisp typography and properly scaled bevels.

---

## Keyboard Shortcuts Reference

| Shortcut | Action | Description |
|---|---|---|
| `Super + Space` | **Toggle Search** | Opens or dismisses the application search palette |
| `Ctrl + F2` | **System Menu** | Opens the top-left SLOPOS system menu |
| `Alt + Tab` | **Next Window** | Cycles through active windows with EWMH focus |
| `Super + Q` | **Close Window** | Closes the currently active window |
| `Super + M` | **Minimize Window** | Minimizes (iconifies) the active window |
| `Super + F` | **Maximize Window** | Toggles full maximization of the active window |
| `Super + Left` | **Desktop Left** | Switches to the previous virtual workspace |
| `Super + Right` | **Desktop Right** | Switches to the next virtual workspace |
| `Return` | **Confirm / Execute** | Activates the default button in modals or launches selected search result |
| `Escape` | **Cancel / Dismiss** | Dismisses search palette, system menu, or cancels modal dialogs |

---

## Crate Architecture

- **`crates/slopos-session`**: Lightweight session supervisor that launches and monitors Openbox and `slopos-shell` with bounded crash recovery and appearance synchronization.
- **`crates/slopos-shell`**: Desktop chrome containing the top system bar, GIO D-Bus global menu host, live system status tray, `Super+Space` application search palette, bottom Application Strip, and freedesktop notifications.
- **`crates/slopos-catalogue`**: Graphical AppImage software catalogue with fail-closed cryptographic validation, atomic extraction, and desktop integration.
- **`crates/slopos-settings`**: Compact SLOPOS-styled Control Panels hub delegating to native system utilities and housing the built-in appearance switcher, wallpaper chooser, and date/time configuration.

---

## Installation & Building

### Native Build from Source

```bash
cargo build --workspace --release --locked
```

### System Installation

```bash
sudo ./install.sh
```

### Appearance Switching via CLI

```bash
# Switch to Classic Macintosh (System 6/7)
slopos-appearance classic

# Switch to Platinum Light (Canonical)
slopos-appearance platinum

# Switch to Graphite Dark
slopos-appearance graphite

# Switch to OLED Dark (Pure Black)
slopos-appearance oled
```

### Wallpaper Management via CLI

```bash
# Set wallpaper with fill mode
slopos-wallpaper set 03_vintage_mac_blue.png --mode fill

# List available retro wallpapers
slopos-wallpaper list
```

### Configuration Recovery

```bash
# Safely restore vendor defaults and backup existing user configuration
slopos-recovery
```

---

## Automated QA & Verification

```bash
# Run the complete master Docker QA test harness:
bash scripts/run-release-qa.sh

# Run the 44-scene canonical visual capture and audit suite:
bash scripts/run-canonical-visual-qa.sh
```

---

## Product Contracts & Compliance

- [`AGENTS.md`](AGENTS.md) — Normative product contract and development authority.
- [`TRUTH.md`](TRUTH.md) — Factual readiness ledger and Docker verification audit.
- [`THIRD_PARTY_LICENSES.txt`](THIRD_PARTY_LICENSES.txt) — Open-source license attribution for all redistributable assets.
