#!/usr/bin/env bash
# Full comprehensive screenshot generator and QA suite for SLOPOS-I.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Installing dependencies for complete visual screenshot capture ==="
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq software-properties-common curl

# Add Mozilla Team PPA for real Firefox and Thunderbird native .deb packages
add-apt-repository -y ppa:mozillateam/ppa
cat << 'EOF' > /etc/apt/preferences.d/mozilla-firefox
Package: *
Pin: release o=LP-PPA-mozillateam
Pin-Priority: 1001
EOF
apt-get update -qq

apt-get install -y -qq --no-install-recommends \
  build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev \
  libssl-dev libdbus-1-dev libpulse-dev \
  xvfb openbox picom pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
  arandr pavucontrol network-manager-gnome blueman xfce4-power-manager xfce4-settings \
  python3 python3-gi scrot imagemagick x11-utils x11-xserver-utils xdotool wmctrl dbus-x11 librsvg2-common git \
  ca-certificates adwaita-icon-theme fonts-liberation fonts-dejavu-core libnotify-bin feh \
  gimp inkscape libreoffice-writer vlc firefox thunderbird supertux chocolate-doom freedoom \
  appmenu-gtk2-module appmenu-gtk3-module || true

# Setup command aliases/symlinks for browser, games, and root-safe execution
mkdir -p /usr/local/bin
if [ -f /usr/bin/vlc ]; then
  sed -i 's/geteuid/getppid/' /usr/bin/vlc 2>/dev/null || true
fi
if ! command -v supertux >/dev/null 2>&1 && command -v supertux2 >/dev/null 2>&1; then
  ln -sf /usr/games/supertux2 /usr/local/bin/supertux || true
elif [ -f /usr/games/supertux ]; then
  ln -sf /usr/games/supertux /usr/local/bin/supertux || true
fi
if ! command -v chocolate-doom >/dev/null 2>&1 && [ -f /usr/games/chocolate-doom ]; then
  ln -sf /usr/games/chocolate-doom /usr/local/bin/chocolate-doom || true
fi

echo "=== Installing stable Rust via rustup if needed ==="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  export PATH="$HOME/.cargo/bin:$PATH"
fi

echo "=== Building release binaries ==="
export CARGO_TARGET_DIR="$REPO_ROOT/target-docker"
cargo build --release --workspace --locked
mkdir -p target/release
cp "$CARGO_TARGET_DIR/release"/slopos-* target/release/ 2>/dev/null || true

OUT_DIR="$REPO_ROOT/docs/screenshots"
mkdir -p "$OUT_DIR"

# Install themes into system and user directories
mkdir -p /usr/share/themes/slopos-openbox/openbox-3 \
  /usr/share/themes/slopos-openbox-classic/openbox-3 \
  /usr/share/themes/slopos-openbox-graphite/openbox-3 \
  /usr/share/themes/slopos-openbox-oled/openbox-3 \
  /usr/share/themes/slopos-gtk/gtk-3.0 \
  /usr/share/themes/slopos-gtk-classic/gtk-3.0 \
  /usr/share/themes/slopos-gtk-graphite/gtk-3.0 \
  /usr/share/themes/slopos-gtk-oled/gtk-3.0 \
  /usr/share/icons/SLOPOS-Platinum

cp themes/slopos-openbox/openbox-3/themerc /usr/share/themes/slopos-openbox/openbox-3/themerc
cp themes/slopos-openbox-classic/openbox-3/themerc /usr/share/themes/slopos-openbox-classic/openbox-3/themerc
cp themes/slopos-openbox-graphite/openbox-3/themerc /usr/share/themes/slopos-openbox-graphite/openbox-3/themerc
cp themes/slopos-openbox-oled/openbox-3/themerc /usr/share/themes/slopos-openbox-oled/openbox-3/themerc

cp assets/config/gtk-3.0/gtk.css /usr/share/themes/slopos-gtk/gtk-3.0/gtk.css
cp assets/config/gtk-3.0/gtk-classic.css /usr/share/themes/slopos-gtk-classic/gtk-3.0/gtk.css
cp assets/config/gtk-3.0/gtk-graphite.css /usr/share/themes/slopos-gtk-graphite/gtk-3.0/gtk.css
cp assets/config/gtk-3.0/gtk-oled.css /usr/share/themes/slopos-gtk-oled/gtk-3.0/gtk.css
cp -a themes/platinum/icon-theme/. /usr/share/icons/SLOPOS-Platinum/

echo "=== Running Master Screenshot Capture Suite ==="
python3 - <<'PY'
import os
import subprocess
import time
import signal
import shutil

REPO_ROOT = os.environ.get("REPO_ROOT", os.getcwd())
OUT_DIR = os.path.join(REPO_ROOT, "docs/screenshots")
os.makedirs(OUT_DIR, exist_ok=True)

def run(cmd, env, check=False):
    return subprocess.run(cmd, shell=True, env=env, check=check)

class XSession:
    def __init__(self, resolution="1280x800", scale=1, appearance="platinum"):
        self.resolution = resolution
        self.scale = scale
        self.appearance = appearance
        self.display = ":99"
        self.home = f"/tmp/slopos-qa-home-{int(time.time()*1000)}"
        self.env = os.environ.copy()
        self.xvfb_proc = None
        self.session_proc = None
        self.dbus_addr = None

    def start(self):
        os.makedirs(self.home, exist_ok=True)
        w, h = self.resolution.split("x")

        # Start dbus daemon
        dbus_out = subprocess.check_output([
            "dbus-daemon", "--session", "--fork", "--print-address"
        ]).decode().strip()
        self.dbus_addr = dbus_out

        self.env.update({
            "DISPLAY": self.display,
            "HOME": self.home,
            "DBUS_SESSION_BUS_ADDRESS": self.dbus_addr,
            "XDG_CONFIG_HOME": f"{self.home}/.config",
            "XDG_DATA_HOME": f"{self.home}/.local/share",
            "XDG_CACHE_HOME": f"{self.home}/.cache",
            "XDG_CURRENT_DESKTOP": "SLOPOS",
            "XDG_SESSION_DESKTOP": "slopos",
            "SLOPOS_SESSION_MANAGED": "1",
            "SLOPOS_QA_NO_WELCOME": "1",
            "SLOPOS_APPEARANCE": self.appearance,
            "GDK_BACKEND": "x11",
            "GDK_SCALE": str(self.scale),
            "GTK_MODULES": "appmenu-gtk-module",
            "UBUNTU_MENUPROXY": "1",
            "MOZ_DISABLE_CONTENT_SANDBOX": "1",
            "MOZ_DISABLE_GMP_SANDBOX": "1",
            "MOZ_DISABLE_RDD_SANDBOX": "1",
            "PATH": f"/usr/local/bin:/usr/games:{REPO_ROOT}/scripts:{REPO_ROOT}/target/release:{self.env.get('PATH', '')}",
        })

        # Appearance configs
        cfg = f"{self.home}/.config"
        os.makedirs(f"{cfg}/gtk-3.0", exist_ok=True)
        os.makedirs(f"{cfg}/openbox", exist_ok=True)
        os.makedirs(f"{cfg}/slopos-i", exist_ok=True)
        with open(f"{cfg}/slopos-i/appearance", "w") as f:
            f.write(f"{self.appearance}\n")

        # Setup Firefox profile
        ff_prof = f"{self.home}/.mozilla/firefox/qa.default"
        os.makedirs(ff_prof, exist_ok=True)
        with open(f"{self.home}/.mozilla/firefox/profiles.ini", "w") as f:
            f.write("[Profile0]\nName=qa\nIsRelative=1\nPath=qa.default\nDefault=1\n\n[General]\nStartWithLastProfile=1\nVersion=2\n")
        prefs_content = (
            'user_pref("browser.shell.checkDefaultBrowser", false);\n'
            'user_pref("datareporting.policy.dataSubmissionPolicyAccepted", true);\n'
            'user_pref("browser.tabs.warnOnClose", false);\n'
            'user_pref("browser.tabs.inTitlebar", 0);\n'
            'user_pref("browser.tabs.drawInTitlebar", false);\n'
            'user_pref("security.sandbox.content.level", 0);\n'
            'user_pref("security.sandbox.warn_unsupported_configuration", false);\n'
            'user_pref("browser.startup.homepage", "https://example.com");\n'
            'user_pref("browser.startup.page", 1);\n'
        )
        with open(f"{ff_prof}/user.js", "w") as f:
            f.write(prefs_content)
        with open(f"{ff_prof}/prefs.js", "w") as f:
            f.write(prefs_content)

        gtk_css = "gtk.css"
        ob_theme = "slopos-openbox"
        bg_color = "#758090"
        if self.appearance == "graphite":
            gtk_css = "gtk-graphite.css"
            ob_theme = "slopos-openbox-graphite"
            bg_color = "#1e222a"
        elif self.appearance == "oled":
            gtk_css = "gtk-oled.css"
            ob_theme = "slopos-openbox-oled"
            bg_color = "#000000"
        elif self.appearance == "classic":
            gtk_css = "gtk-classic.css"
            ob_theme = "slopos-openbox-classic"
            bg_color = "#758090"

        if self.appearance in ["platinum", "classic"]:
            with open(f"{cfg}/slopos-i/wallpaper", "w") as f:
                f.write(f"{REPO_ROOT}/assets/wallpapers/04_retro_teal_grid.png\n")

        shutil.copy(f"{REPO_ROOT}/assets/config/gtk-3.0/{gtk_css}", f"{cfg}/gtk-3.0/gtk.css")
        shutil.copy(f"{REPO_ROOT}/assets/config/gtk-3.0/{gtk_css}", f"{cfg}/gtk-3.0/{gtk_css}")
        
        is_dark = 1 if self.appearance in ["graphite", "oled"] else 0
        theme_name = f"slopos-gtk-{self.appearance}" if self.appearance != "platinum" else "slopos-gtk"
        with open(f"{cfg}/gtk-3.0/settings.ini", "w") as f:
            f.write(f"""[Settings]
gtk-theme-name = {theme_name}
gtk-icon-theme-name = SLOPOS-Platinum
gtk-cursor-theme-name = DMZ-White
gtk-font-name = Liberation Sans 9
gtk-application-prefer-dark-theme = {is_dark}
gtk-shell-shows-menubar = 1
gtk-shell-shows-app-menu = 0
gtk-button-images = 1
gtk-menu-images = 1
gtk-enable-event-sounds = 1
gtk-enable-input-feedback-sounds = 1
gtk-xft-antialias = 1
gtk-xft-hinting = 1
gtk-xft-hintstyle = hintslight
gtk-xft-rgba = rgb
""")

        # Openbox config with selected theme
        with open(f"{REPO_ROOT}/assets/config/openbox/rc.xml") as f:
            rc_xml = f.read()
        rc_xml = rc_xml.replace("<name>slopos-openbox</name>", f"<name>{ob_theme}</name>")
        with open(f"{cfg}/openbox/rc.xml", "w") as f:
            f.write(rc_xml)

        # Start Xvfb
        self.xvfb_proc = subprocess.Popen([
            "Xvfb", self.display, "-screen", "0", f"{w}x{h}x24", "-nolisten", "tcp"
        ], env=self.env)
        time.sleep(1.0)
        run(f"xsetroot -solid '{bg_color}'", env=self.env)

        # Set default wallpaper
        if self.appearance in ["platinum", "classic"]:
            if os.path.exists(f"{REPO_ROOT}/assets/wallpapers/04_retro_teal_grid.png"):
                run(f"feh --bg-fill '{REPO_ROOT}/assets/wallpapers/04_retro_teal_grid.png'", env=self.env)
            elif os.path.exists(f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"):
                run(f"feh --bg-fill '{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png'", env=self.env)
        else:
            run(f"xsetroot -solid '{bg_color}'", env=self.env)

        # Start slopos-session
        self.session_proc = subprocess.Popen([
            f"{REPO_ROOT}/target/release/slopos-session"
        ], env=self.env)

        # Wait for Top Bar and Application Strip
        for _ in range(60):
            res1 = subprocess.run("xdotool search --onlyvisible --name '^SLOPOS Top Bar$'", shell=True, env=self.env, capture_output=True)
            res2 = subprocess.run("xdotool search --onlyvisible --name '^SLOPOS Application Strip$'", shell=True, env=self.env, capture_output=True)
            if res1.returncode == 0 and res2.returncode == 0:
                break
            time.sleep(0.2)
        time.sleep(0.5)

    def capture(self, name, delay=0.4):
        time.sleep(delay)
        run("xdotool mousemove 1270 790", env=self.env)
        time.sleep(0.2)
        path = os.path.join(OUT_DIR, name)
        run(f"scrot -zo '{path}'", env=self.env)
        if os.path.exists(path) and os.path.getsize(path) > 0:
            print(f"Captured: {name} ({os.path.getsize(path)} bytes)")
        else:
            print(f"FAILED capture: {name}")

    def spawn(self, cmd_list):
        cmd = list(cmd_list)
        if cmd[0] == "supertux" and not shutil.which("supertux", path=self.env.get("PATH")):
            for candidate in ["supertux2", "/usr/games/supertux2", "/usr/games/supertux"]:
                if shutil.which(candidate, path=self.env.get("PATH")) or os.path.exists(candidate):
                    cmd[0] = candidate
                    break
        exe = shutil.which(cmd[0], path=self.env.get("PATH")) or (cmd[0] if os.path.exists(cmd[0]) else None)
        if not exe:
            print(f"WARN: Executable {cmd[0]} not found in PATH")
            return None
        return subprocess.Popen([exe] + cmd[1:], env=self.env)

    def clean_client_windows(self):
        try:
            out = subprocess.check_output("wmctrl -l", shell=True, env=self.env).decode()
            for line in out.splitlines():
                parts = line.split(None, 3)
                if len(parts) >= 4:
                    wid = parts[0]
                    title = parts[3]
                    if not any(k in title for k in ["SLOPOS Top Bar", "SLOPOS Application Strip", "SLOPOS Search"]):
                        run(f"wmctrl -i -c {wid}", env=self.env)
        except Exception:
            pass
        time.sleep(0.15)
        run("pkill -9 -f 'pcmanfm|mousepad|terminal|galculator|ristretto|zathura|mpv|gimp|inkscape|soffice|libreoffice|vlc|thunderbird|supertux|doom|epiphany|firefox|slopos-settings|slopos-catalogue|nm-connection-editor|blueman-manager|pavucontrol' || true", env=self.env)
        run("xdotool search --onlyvisible --name 'SLOPOS Notification' | xargs -I{} xdotool windowclose {} || true", env=self.env)
        run("xdotool search --onlyvisible --class Dialog | xargs -I{} xdotool windowclose {} || true", env=self.env)
        time.sleep(0.2)

    def stop(self):
        if self.session_proc:
            self.session_proc.terminate()
            try:
                self.session_proc.wait(timeout=2)
            except Exception:
                self.session_proc.kill()
        self.clean_client_windows()
        run("pkill -9 -x slopos-shell openbox || true", env=self.env)
        if self.xvfb_proc:
            self.xvfb_proc.terminate()
            try:
                self.xvfb_proc.wait(timeout=2)
            except Exception:
                self.xvfb_proc.kill()
        shutil.rmtree(self.home, ignore_errors=True)

# 1. Canonical 1280x800 Platinum Session
print("--- Scene 1: Platinum Desktop ---")
s = XSession(resolution="1280x800", appearance="platinum")
s.start()

# 01 Clean Desktop
s.capture("01_clean_desktop_platinum_1280x800.png")

# 02 System Menu Open
run("pkill -USR2 -x slopos-shell", env=s.env)
time.sleep(0.4)
s.capture("02_system_menu_open_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)

# 03 Search Palette Open
run("pkill -USR1 -x slopos-shell", env=s.env)
time.sleep(0.4)
run("xdotool type --delay 25 'Terminal'", env=s.env)
time.sleep(0.2)
s.capture("03_search_palette_open_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)

# 04 Toast Notification
run("notify-send -t 1500 -a 'SLOPOS-I' 'Welcome to SLOPOS-I' 'Press Super+Space or choose Search to find applications.'", env=s.env)
time.sleep(0.6)
s.capture("04_notification_1280x800.png")
run("xdotool search --onlyvisible --name 'SLOPOS Notification' | xargs -I{} xdotool windowclose {} || true", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 05 Modal About Dialog
run("pkill -USR2 -x slopos-shell", env=s.env)
time.sleep(0.4)
run("xdotool key Down Return", env=s.env)
time.sleep(0.6)
s.capture("05_modal_about_dialog_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 17 Modal Shutdown Dialog
run("pkill -USR2 -x slopos-shell", env=s.env)
time.sleep(0.4)
run("xdotool key Down Down Down Down Down Down Down Down Down Down Down Return", env=s.env)
time.sleep(0.6)
s.capture("17_modal_shutdown_dialog_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 18 Modal Switch User Dialog
run("pkill -USR2 -x slopos-shell", env=s.env)
time.sleep(0.4)
run("xdotool key Down Down Down Down Down Down Down Return", env=s.env)
time.sleep(0.6)
s.capture("18_modal_switch_user_dialog_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 22 Modal Restart Dialog
run("pkill -USR2 -x slopos-shell", env=s.env)
time.sleep(0.4)
run("xdotool key Down Down Down Down Down Down Down Down Down Down Return", env=s.env)
time.sleep(0.6)
s.capture("22_modal_restart_dialog_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 25 Desktop right-click context menu
run("xdotool mousemove 640 400 click 3", env=s.env)
time.sleep(0.4)
s.capture("25_desktop_right_click_context_menu_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 06 Active App Mousepad
s.clean_client_windows()

# 06 Active App Mousepad
p_mouse = s.spawn(["mousepad", f"{REPO_ROOT}/README.md"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class mousepad | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 80 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("06_active_app_mousepad_1280x800.png")

# 07 Multi-window focus (PCManFM + Terminal)
s.clean_client_windows()
p_fm = s.spawn(["pcmanfm", REPO_ROOT])
time.sleep(0.6)
run("xdotool search --onlyvisible --class pcmanfm | tail -1 | xargs -I{} xdotool windowsize {} 560 380 windowmove {} 100 80", env=s.env)
p_term = s.spawn(["xfce4-terminal"])
time.sleep(0.6)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 540 340 windowmove {} 380 180 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("07_multi_window_focus_1280x800.png")
s.clean_client_windows()

# 08 File Manager PCManFM
p_fm = s.spawn(["pcmanfm", REPO_ROOT])
time.sleep(0.8)
run("xdotool search --onlyvisible --class pcmanfm | tail -1 | xargs -I{} xdotool windowsize {} 680 480 windowmove {} 120 70 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("08_file_manager_pcmanfm_1280x800.png")

# 26 File Manager right-click context menu
run("xdotool mousemove 300 200 click 3", env=s.env)
time.sleep(0.4)
s.capture("26_file_manager_right_click_context_menu_1280x800.png")
run("xdotool key Escape", env=s.env)
time.sleep(0.3)
s.clean_client_windows()

# 09 Terminal Xfce4
p_term = s.spawn(["xfce4-terminal"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 640 420 windowmove {} 160 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("09_terminal_xfce4_1280x800.png")
s.clean_client_windows()

# 10 Software Catalogue
p_cat = s.spawn(["slopos-catalogue"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Software Catalogue$' | tail -1 | xargs -I{} xdotool windowsize {} 660 480 windowmove {} 140 70 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("10_software_catalogue_1280x800.png")
s.clean_client_windows()

# 11 System Settings Control Panels
p_set = s.spawn(["slopos-settings"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("11_system_settings_control_panels_1280x800.png")
s.clean_client_windows()

# 23 Web Browser Firefox
p_ff = s.spawn(["firefox", "--no-remote", "-P", "qa", "https://example.com"])
time.sleep(4.5)
run("WID=$(xdotool search --onlyvisible --class firefox | tail -1); if [ -n \"$WID\" ]; then wmctrl -i -r $WID -b remove,fullscreen,maximized_vert,maximized_horz; sleep 0.2; xdotool windowsize $WID 740 480 windowmove $WID 100 55 windowactivate $WID; fi", env=s.env)
time.sleep(0.6)
s.capture("23_web_browser_firefox_1280x800.png")
s.clean_client_windows()

# 24 Game Doom (Freedoom Phase 2) Windowed
wad_file = "/usr/share/games/doom/freedoom2.wad"
if not os.path.exists(wad_file):
    wad_file = "/usr/share/games/doom/freedoom1.wad"
p_doom = s.spawn(["chocolate-doom", "-iwad", wad_file, "-window"])
time.sleep(1.0)
run("xdotool search --onlyvisible --class chocolate-doom | tail -1 | xargs -I{} xdotool windowmove {} 200 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("24_game_doom_freedoom_1280x800.png")
s.clean_client_windows()

# 27 Calculator
p_calc = s.spawn(["galculator"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class galculator | tail -1 | xargs -I{} xdotool windowmove {} 200 120 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("27_calculator_galculator_1280x800.png")
s.clean_client_windows()

# 28 Image viewer Ristretto
if os.path.exists(f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"):
    p_img = s.spawn(["ristretto", f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"])
    time.sleep(0.8)
    run("xdotool search --onlyvisible --class ristretto | tail -1 | xargs -I{} xdotool windowsize {} 620 440 windowmove {} 160 80 windowactivate {}", env=s.env)
    time.sleep(0.4)
    s.capture("28_image_viewer_ristretto_1280x800.png")
    s.clean_client_windows()

# 29 Document viewer Zathura
p_zath = s.spawn(["zathura"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class zathura | tail -1 | xargs -I{} xdotool windowsize {} 600 440 windowmove {} 180 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("29_document_viewer_zathura_1280x800.png")
s.clean_client_windows()

# 33 Wallpaper chooser dialog
p_wall = s.spawn(["slopos-settings", "--wallpaper"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Desktop & Wallpaper$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("33_wallpaper_chooser_dialog_1280x800.png")
s.clean_client_windows()

# 36 Date & Time control panel
p_dt = s.spawn(["slopos-settings", "--datetime"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Date & Time$' | tail -1 | xargs -I{} xdotool windowsize {} 520 480 windowmove {} 180 60 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("36_datetime_control_panel_1280x800.png")
s.clean_client_windows()

# 37 Network & Wi-Fi GUI
p_net = s.spawn(["slopos-settings", "--network"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name 'Network & Wi-Fi' | tail -1 | xargs -I{} xdotool windowsize {} 520 440 windowmove {} 180 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("37_network_wifi_gui_1280x800.png")
s.clean_client_windows()

# 38 Bluetooth GUI
p_blue = s.spawn(["blueman-manager"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class blueman-manager | tail -1 | xargs -I{} xdotool windowsize {} 560 400 windowmove {} 180 90 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("38_bluetooth_gui_1280x800.png")
s.clean_client_windows()

# 39 Sound Audio & Volume Control
p_snd = s.spawn(["slopos-settings", "--sound"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name 'Sound & Volume' | tail -1 | xargs -I{} xdotool windowsize {} 520 440 windowmove {} 180 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("39_sound_audio_pavucontrol_1280x800.png")
s.clean_client_windows()

# 40 GIMP
p_gimp = s.spawn(["gimp"])
time.sleep(1.5)
run("xdotool search --onlyvisible --class gimp | tail -1 | xargs -I{} xdotool windowsize {} 720 500 windowmove {} 100 60 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("40_app_gimp_1280x800.png")
s.clean_client_windows()

# 41 Inkscape
p_ink = s.spawn(["inkscape"])
time.sleep(1.5)
run("xdotool search --onlyvisible --class inkscape | tail -1 | xargs -I{} xdotool windowsize {} 720 500 windowmove {} 100 60 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("41_app_inkscape_1280x800.png")
s.clean_client_windows()

# 42 VLC Media Player
p_vlc = s.spawn(["vlc"])
time.sleep(1.2)
run("xdotool search --onlyvisible --class vlc | tail -1 | xargs -I{} xdotool windowsize {} 640 440 windowmove {} 160 70 windowactivate {}", env=s.env)
time.sleep(0.5)
s.capture("42_app_vlc_media_player_1280x800.png")
s.clean_client_windows()

# 43 LibreOffice Writer
p_lo = s.spawn(["libreoffice", "--writer", "--norestore", "--nologo"])
time.sleep(3.0)
run("WID=$(xdotool search --onlyvisible --class 'soffice|libreoffice' | tail -1); if [ -n \"$WID\" ]; then wmctrl -i -r $WID -b remove,fullscreen,maximized_vert,maximized_horz; sleep 0.2; xdotool windowsize $WID 720 450 windowmove $WID 120 55 windowactivate $WID; fi", env=s.env)
time.sleep(0.6)
s.capture("43_app_libreoffice_writer_1280x800.png")
s.clean_client_windows()

# 44 SuperTux Windowed
p_st = s.spawn(["supertux", "-w"])
time.sleep(1.2)
run("xdotool search --onlyvisible --class 'supertux|supertux2' | tail -1 | xargs -I{} xdotool windowmove {} 200 70 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("44_app_supertux_1280x800.png")
s.clean_client_windows()

# 45 Thunderbird (Mail & Calendar)
p_tb = s.spawn(["thunderbird", "--no-remote"])
time.sleep(4.0)
run("WID=$(xdotool search --onlyvisible --class 'thunderbird|Thunderbird' | tail -1); if [ -n \"$WID\" ]; then wmctrl -i -r $WID -b remove,fullscreen,maximized_vert,maximized_horz; sleep 0.2; xdotool windowsize $WID 740 480 windowmove $WID 100 55 windowactivate $WID; fi", env=s.env)
time.sleep(0.6)
s.capture("45_app_thunderbird_1280x800.png")
s.clean_client_windows()

# 46 Fullscreen Video MPV
p_mpv = s.spawn(["mpv", "--fullscreen", "--idle=yes", "--force-window=yes"])
time.sleep(1.0)
s.capture("46_fullscreen_video_mpv_1280x800.png")
s.clean_client_windows()

# 47 Dock Dodge Maximized
with open(f"{s.home}/.config/slopos-i/dock_dodge", "w") as f:
    f.write("1\n")
p_dodge = s.spawn(["mousepad", f"{REPO_ROOT}/README.md"])
time.sleep(1.5)
run("wmctrl -r 'Mousepad' -b add,maximized_vert,maximized_horz", env=s.env)
run("wmctrl -a 'Mousepad'", env=s.env)
time.sleep(0.8)
s.capture("47_dock_dodge_maximized_1280x800.png")

# 49 Dock Dodge Hover Overlap
run("xdotool mousemove 640 799", env=s.env)
time.sleep(0.6)
s.capture("49_dock_dodge_hover_overlap_1280x800.png")
s.clean_client_windows()
try:
    os.remove(f"{s.home}/.config/slopos-i/dock_dodge")
except Exception:
    pass

# 50 Fullscreen Game SuperTux
p_stux_full = s.spawn(["supertux2", "-f"])
time.sleep(2.0)
s.capture("50_fullscreen_game_supertux_1280x800.png")
s.clean_client_windows()

# 51 Fullscreen Game Doom
p_doom_full = s.spawn(["chocolate-doom", "-iwad", wad_file, "-geometry", "1280x800", "-fullscreen"])
time.sleep(1.5)
s.capture("51_fullscreen_game_doom_1280x800.png")
s.clean_client_windows()

# 48 Appearance custom color & font studio
p_app = s.spawn(["slopos-settings", "--appearance"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Appearance$' | tail -1 | xargs -I{} xdotool windowsize {} 600 440 windowmove {} 180 80 windowactivate {}", env=s.env)
time.sleep(0.4)
s.capture("48_custom_color_font_studio_1280x800.png")
s.clean_client_windows()

# Wallpapers
for idx, wp, name in [
    ("30", "01_classic_system_gray.png", "30_wallpaper_classic_system_gray_1280x800.png"),
    ("31", "03_slate_blue.png", "31_wallpaper_slate_blue_1280x800.png"),
    ("32", "04_retro_teal_grid.png", "32_wallpaper_retro_teal_grid_1280x800.png"),
]:
    wp_path = f"{REPO_ROOT}/assets/wallpapers/{wp}"
    if os.path.exists(wp_path):
        run(f"feh --bg-fill '{wp_path}'", env=s.env)
        time.sleep(0.3)
        s.capture(name)

s.stop()

# 2. Graphite Dark Session
print("--- Scene 2: Graphite Dark ---")
s_dark = XSession(resolution="1280x800", appearance="graphite")
s_dark.start()
s_dark.capture("12_graphite_dark_desktop_1280x800.png")

p_dark_set = s_dark.spawn(["slopos-settings"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s_dark.env)
time.sleep(0.4)
s_dark.capture("13_graphite_settings_1280x800.png")
s_dark.clean_client_windows()
s_dark.stop()

# 3. OLED Dark Session
print("--- Scene 3: OLED Dark ---")
s_oled = XSession(resolution="1280x800", appearance="oled")
s_oled.start()
s_oled.capture("34_oled_dark_desktop_1280x800.png")

p_oled_set = s_oled.spawn(["slopos-settings"])
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s_oled.env)
time.sleep(0.4)
s_oled.capture("35_oled_dark_settings_1280x800.png")
s_oled.clean_client_windows()
s_oled.stop()

# 4. Classic Contrast Session
print("--- Scene 4: Classic Contrast ---")
s_classic = XSession(resolution="1280x800", appearance="classic")
s_classic.start()
s_classic.capture("19_classic_contrast_desktop_1280x800.png")

run("pkill -USR2 -x slopos-shell", env=s_classic.env)
time.sleep(0.4)
s_classic.capture("20_classic_contrast_system_menu_1280x800.png")

run("xdotool key Down Return", env=s_classic.env)
time.sleep(0.6)
s_classic.capture("21_classic_contrast_about_dialog_1280x800.png")
run("xdotool search --onlyvisible --name 'About' | xargs -I{} xdotool windowclose {} || true", env=s_classic.env)
time.sleep(0.3)
s_classic.clean_client_windows()
s_classic.stop()

# 5. Multi-resolution: 1920x1080 Full HD
print("--- Scene 5: 1920x1080 Full HD ---")
s_fhd = XSession(resolution="1920x1080", appearance="platinum")
s_fhd.start()
p_fm1 = s_fhd.spawn(["pcmanfm", REPO_ROOT])
time.sleep(0.8)
run("xdotool search --onlyvisible --class pcmanfm | tail -1 | xargs -I{} xdotool windowsize {} 720 520 windowmove {} 140 100", env=s_fhd.env)
p_term1 = s_fhd.spawn(["xfce4-terminal"])
time.sleep(0.8)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 680 440 windowmove {} 540 220 windowactivate {}", env=s_fhd.env)
time.sleep(0.4)
s_fhd.capture("16_workspace_multi_window_1920x1080.png")
s_fhd.clean_client_windows()
s_fhd.stop()

# 6. Multi-resolution: 3440x1440 Ultrawide
print("--- Scene 6: 3440x1440 Ultrawide ---")
s_ultra = XSession(resolution="3440x1440", appearance="platinum")
s_ultra.start()
s_ultra.capture("14_ultrawide_desktop_3440x1440.png")
s_ultra.stop()

# 7. Multi-resolution: 2560x1600 HiDPI Scale 2
print("--- Scene 7: 2560x1600 HiDPI Scale 2 ---")
s_hidpi = XSession(resolution="2560x1600", scale=2, appearance="platinum")
s_hidpi.start()
s_hidpi.capture("15_hidpi_scale2_2560x1600.png")
s_hidpi.stop()

print("ALL SCENES CAPTURED SUCCESSFULLY!")
PY

echo "=== Checking all generated screenshots ==="
ls -lh "$OUT_DIR"/*.png
