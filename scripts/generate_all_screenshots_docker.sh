#!/usr/bin/env bash
# Full comprehensive screenshot generator and QA suite for SLOPOS-I.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Installing dependencies for complete visual screenshot capture ==="
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential pkg-config libgtk-3-dev libx11-dev libxrandr-dev \
  libssl-dev libdbus-1-dev libpulse-dev \
  xvfb openbox pcmanfm xfce4-terminal mousepad ristretto zathura mpv galculator \
  arandr pavucontrol network-manager-gnome blueman xfce4-power-manager xfce4-settings \
  python3 python3-gi scrot imagemagick x11-utils x11-xserver-utils xdotool wmctrl dbus-x11 librsvg2-common curl git \
  ca-certificates adwaita-icon-theme fonts-liberation fonts-dejavu-core libnotify-bin feh

echo "=== Installing stable Rust via rustup if needed ==="
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
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

REPO_ROOT = "/workspace"
OUT_DIR = os.path.join(REPO_ROOT, "docs/screenshots")
os.makedirs(OUT_DIR, exist_ok=True)

def run(cmd, env, check=True):
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
            "PATH": f"{REPO_ROOT}/scripts:{REPO_ROOT}/target/release:{self.env.get('PATH', '')}",
        })

        # Copy configs
        cfg = f"{self.home}/.config"
        os.makedirs(f"{cfg}/gtk-3.0", exist_ok=True)
        os.makedirs(f"{cfg}/openbox", exist_ok=True)
        shutil.copy(f"{REPO_ROOT}/assets/config/gtk-3.0/gtk.css", f"{cfg}/gtk-3.0/gtk.css")
        shutil.copy(f"{REPO_ROOT}/assets/config/gtk-3.0/settings.ini", f"{cfg}/gtk-3.0/settings.ini")
        shutil.copy(f"{REPO_ROOT}/assets/config/openbox/rc.xml", f"{cfg}/openbox/rc.xml")

        # Start Xvfb
        self.xvfb_proc = subprocess.Popen([
            "Xvfb", self.display, "-screen", "0", f"{w}x{h}x24", "-nolisten", "tcp"
        ], env=self.env)
        time.sleep(1.0)
        run("xsetroot -solid '#758090'", env=self.env, check=False)

        # Set default wallpaper
        if os.path.exists(f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"):
            run(f"feh --bg-fill '{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png'", env=self.env, check=False)

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
        run("xdotool mousemove 1270 790", env=self.env, check=False)
        time.sleep(0.2)
        path = os.path.join(OUT_DIR, name)
        run(f"scrot -zo '{path}'", env=self.env)
        if os.path.exists(path) and os.path.getsize(path) > 0:
            print(f"Captured: {name} ({os.path.getsize(path)} bytes)")
        else:
            print(f"FAILED capture: {name}")

    def stop(self):
        if self.session_proc:
            self.session_proc.terminate()
            try:
                self.session_proc.wait(timeout=2)
            except Exception:
                self.session_proc.kill()
        run("pkill -9 -x slopos-shell slopos-settings slopos-catalogue openbox pcmanfm mousepad xfce4-terminal galculator ristretto zathura mpv || true", env=self.env, check=False)
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
run("pkill -USR2 -x slopos-shell", env=s.env, check=False)
time.sleep(0.4)
s.capture("02_system_menu_open_1280x800.png")
run("xdotool key Escape", env=s.env, check=False)
time.sleep(0.3)

# 03 Search Palette Open
run("pkill -USR1 -x slopos-shell", env=s.env, check=False)
time.sleep(0.4)
run("xdotool type --delay 25 'Terminal'", env=s.env, check=False)
time.sleep(0.2)
s.capture("03_search_palette_open_1280x800.png")
run("xdotool key Escape", env=s.env, check=False)
time.sleep(0.3)

# 04 Toast Notification
run("notify-send -t 6000 -a 'SLOPOS-I' 'Welcome to SLOPOS-I' 'Press Super+Space or choose Search to find applications.'", env=s.env, check=False)
time.sleep(0.6)
s.capture("04_notification_1280x800.png")

# 05 Modal About Dialog
run("pkill -USR2 -x slopos-shell", env=s.env, check=False)
time.sleep(0.4)
run("xdotool key Down Return", env=s.env, check=False)
time.sleep(0.6)
s.capture("05_modal_about_dialog_1280x800.png")
run("xdotool key Escape || xdotool key Return", env=s.env, check=False)
time.sleep(0.3)

# 06 Active App Mousepad
p_mouse = subprocess.Popen(["mousepad", f"{REPO_ROOT}/README.md"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class mousepad | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 80 80 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("06_active_app_mousepad_1280x800.png")

# 07 Multi-window focus (PCManFM + Terminal)
p_term = subprocess.Popen(["xfce4-terminal"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 560 360 windowmove {} 380 180 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("07_multi_window_focus_1280x800.png")

p_mouse.terminate()
p_term.terminate()
time.sleep(0.3)

# 08 File Manager PCManFM
p_fm = subprocess.Popen(["pcmanfm", REPO_ROOT], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class pcmanfm | tail -1 | xargs -I{} xdotool windowsize {} 680 480 windowmove {} 120 70 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("08_file_manager_pcmanfm_1280x800.png")
p_fm.terminate()
time.sleep(0.3)

# 09 Terminal Xfce4
p_term = subprocess.Popen(["xfce4-terminal"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 640 420 windowmove {} 160 80 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("09_terminal_xfce4_1280x800.png")
p_term.terminate()
time.sleep(0.3)

# 10 Software Catalogue
p_cat = subprocess.Popen(["slopos-catalogue"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Software Catalogue$' | tail -1 | xargs -I{} xdotool windowsize {} 660 480 windowmove {} 140 70 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("10_software_catalogue_1280x800.png")
p_cat.terminate()
time.sleep(0.3)

# 11 System Settings Control Panels
p_set = subprocess.Popen(["slopos-settings"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("11_system_settings_control_panels_1280x800.png")
p_set.terminate()
time.sleep(0.3)

# 27 Calculator
p_calc = subprocess.Popen(["galculator"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class galculator | tail -1 | xargs -I{} xdotool windowmove {} 200 120 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("27_calculator_galculator_1280x800.png")
p_calc.terminate()
time.sleep(0.3)

# 28 Image viewer Ristretto
if os.path.exists(f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"):
    p_img = subprocess.Popen(["ristretto", f"{REPO_ROOT}/assets/wallpapers/01_classic_system_gray.png"], env=s.env)
    time.sleep(0.8)
    run("xdotool search --onlyvisible --class ristretto | tail -1 | xargs -I{} xdotool windowsize {} 620 440 windowmove {} 160 80 windowactivate {}", env=s.env, check=False)
    time.sleep(0.4)
    s.capture("28_image_viewer_ristretto_1280x800.png")
    p_img.terminate()
    time.sleep(0.3)

# 33 Wallpaper chooser dialog
p_wall = subprocess.Popen(["slopos-settings", "--wallpaper"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Desktop & Wallpaper$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("33_wallpaper_chooser_dialog_1280x800.png")
p_wall.terminate()
time.sleep(0.3)

# 36 Date & Time control panel
p_dt = subprocess.Popen(["slopos-settings", "--datetime"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Date & Time$' | tail -1 | xargs -I{} xdotool windowsize {} 520 380 windowmove {} 200 100 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("36_datetime_control_panel_1280x800.png")
p_dt.terminate()
time.sleep(0.3)

# 37 Network GUI
p_net = subprocess.Popen(["nm-connection-editor"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class nm-connection-editor | tail -1 | xargs -I{} xdotool windowsize {} 520 400 windowmove {} 180 90 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("37_network_wifi_gui_1280x800.png")
p_net.terminate()
time.sleep(0.3)

# 38 Bluetooth GUI
p_blue = subprocess.Popen(["blueman-manager"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class blueman-manager | tail -1 | xargs -I{} xdotool windowsize {} 560 400 windowmove {} 180 90 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("38_bluetooth_gui_1280x800.png")
p_blue.terminate()
time.sleep(0.3)

# 39 Sound Audio Pavucontrol
p_snd = subprocess.Popen(["pavucontrol"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class pavucontrol | tail -1 | xargs -I{} xdotool windowsize {} 580 420 windowmove {} 170 80 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("39_sound_audio_pavucontrol_1280x800.png")
p_snd.terminate()
time.sleep(0.3)

# 48 Appearance custom color & font studio
p_app = subprocess.Popen(["slopos-settings", "--appearance"], env=s.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^Appearance$' | tail -1 | xargs -I{} xdotool windowsize {} 600 440 windowmove {} 180 80 windowactivate {}", env=s.env, check=False)
time.sleep(0.4)
s.capture("48_custom_color_font_studio_1280x800.png")
p_app.terminate()
time.sleep(0.3)

# Wallpapers
for idx, wp, name in [
    ("30", "01_classic_system_gray.png", "30_wallpaper_classic_system_gray_1280x800.png"),
    ("31", "03_slate_blue.png", "31_wallpaper_slate_blue_1280x800.png"),
    ("32", "02_retro_teal_grid.png", "32_wallpaper_retro_teal_grid_1280x800.png"),
]:
    wp_path = f"{REPO_ROOT}/assets/wallpapers/{wp}"
    if os.path.exists(wp_path):
        run(f"feh --bg-fill '{wp_path}'", env=s.env, check=False)
        time.sleep(0.3)
        s.capture(name)

s.stop()

# 2. Graphite Dark Session
print("--- Scene 2: Graphite Dark ---")
s_dark = XSession(resolution="1280x800", appearance="graphite")
s_dark.start()
run("xsetroot -solid '#1e222a'", env=s_dark.env, check=False)
s_dark.capture("12_graphite_dark_desktop_1280x800.png")

p_dark_set = subprocess.Popen(["slopos-settings"], env=s_dark.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s_dark.env, check=False)
time.sleep(0.4)
s_dark.capture("13_graphite_settings_1280x800.png")
p_dark_set.terminate()
s_dark.stop()

# 3. OLED Dark Session
print("--- Scene 3: OLED Dark ---")
s_oled = XSession(resolution="1280x800", appearance="oled")
s_oled.start()
run("xsetroot -solid '#000000'", env=s_oled.env, check=False)
s_oled.capture("34_oled_dark_desktop_1280x800.png")

p_oled_set = subprocess.Popen(["slopos-settings"], env=s_oled.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --name '^System Settings$' | tail -1 | xargs -I{} xdotool windowsize {} 640 460 windowmove {} 150 70 windowactivate {}", env=s_oled.env, check=False)
time.sleep(0.4)
s_oled.capture("35_oled_dark_settings_1280x800.png")
p_oled_set.terminate()
s_oled.stop()

# 4. Classic Contrast Session
print("--- Scene 4: Classic Contrast ---")
s_classic = XSession(resolution="1280x800", appearance="classic")
s_classic.start()
s_classic.capture("19_classic_contrast_desktop_1280x800.png")

run("pkill -USR2 -x slopos-shell", env=s_classic.env, check=False)
time.sleep(0.4)
s_classic.capture("20_classic_contrast_system_menu_1280x800.png")
run("xdotool key Escape", env=s_classic.env, check=False)
time.sleep(0.3)

s_classic.stop()

# 5. Multi-resolution: 1920x1080 Full HD
print("--- Scene 5: 1920x1080 Full HD ---")
s_fhd = XSession(resolution="1920x1080", appearance="platinum")
s_fhd.start()
p_fm1 = subprocess.Popen(["pcmanfm", REPO_ROOT], env=s_fhd.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class pcmanfm | tail -1 | xargs -I{} xdotool windowsize {} 720 520 windowmove {} 140 100", env=s_fhd.env, check=False)
p_term1 = subprocess.Popen(["xfce4-terminal"], env=s_fhd.env)
time.sleep(0.8)
run("xdotool search --onlyvisible --class xfce4-terminal | tail -1 | xargs -I{} xdotool windowsize {} 680 440 windowmove {} 540 220 windowactivate {}", env=s_fhd.env, check=False)
time.sleep(0.4)
s_fhd.capture("16_workspace_multi_window_1920x1080.png")
p_fm1.terminate()
p_term1.terminate()
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
