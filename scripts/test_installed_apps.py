#!/usr/bin/env python3
"""
test_installed_apps.py — Interactive Third-Party App Execution Audit for SLOPOS-I.

Executes installed Linux packages (gtk3-demo, mpv, xclock, etc.) within the SLOPOS-I Wayland session.
Records launch command, package version, process tree, protocol, screenshots, and result.json.
"""

import json
import os
import subprocess
import time
from datetime import datetime

def run_app_test(app_id, pkg_name, cmd):
    timestamp = datetime.utcnow().strftime("%Y%m%d_%H%M%S")
    app_dir = os.path.join("artifacts", "de-readiness", timestamp, "applications", app_id)
    os.makedirs(app_dir, exist_ok=True)

    print(f"=== Testing Application: {app_id} ===")
    
    # Package version
    pkg_ver = subprocess.getoutput(f"dpkg -s {pkg_name} 2>/dev/null | grep Version || echo 'Version: unknown'")

    # Environment
    env = os.environ.copy()
    env["WAYLAND_DISPLAY"] = env.get("WAYLAND_DISPLAY", "wayland-1")
    env["GDK_BACKEND"] = "wayland"

    # Launch process
    log_file = os.path.join(app_dir, "app.log")
    with open(log_file, "w") as out:
        proc = subprocess.Popen(cmd, env=env, stdout=out, stderr=out)
    
    time.sleep(3)

    # Process tree & PID metadata
    ps_tree = subprocess.getoutput(f"ps aux | grep {app_id} | grep -v grep")
    with open(os.path.join(app_dir, "process-tree.txt"), "w") as f:
        f.write(ps_tree)

    # Launch screenshot
    shot_path = os.path.join(app_dir, "launch.png")
    os.system(f"grim {shot_path} 2>/dev/null || true")

    # Result JSON
    res = {
        "timestamp": timestamp,
        "application": app_id,
        "package": pkg_name,
        "package_version": pkg_ver.replace("Version: ", "").strip(),
        "launch_cmd": " ".join(cmd),
        "pid": proc.pid,
        "protocol": "wayland-1",
        "status": "PASS" if proc.poll() is None else "FAILED",
        "process_tree_recorded": True,
        "screenshot_captured": os.path.exists(shot_path),
    }

    with open(os.path.join(app_dir, "result.json"), "w") as f:
        json.dump(res, f, indent=2)

    # Terminate process cleanly
    proc.terminate()
    try:
        proc.wait(timeout=2)
    except Exception:
        proc.kill()

    print(f"Application test for {app_id} finished: {res['status']}")
    print(f"Artifacts saved to: {app_dir}")

def main():
    run_app_test("gtk3-demo", "gtk-3-examples", ["gtk3-demo"])
    run_app_test("mpv", "mpv", ["mpv", "--player-operation-mode=pseudo-gui"])

if __name__ == "__main__":
    main()
