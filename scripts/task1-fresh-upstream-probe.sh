#!/usr/bin/env bash
set -euxo pipefail
export DISPLAY=:99
export GDK_BACKEND=x11
export GTK_MODULES=appmenu-gtk-module
export UBUNTU_MENUPROXY=1
Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &
xvfb=$!
trap 'kill "$xvfb" 2>/dev/null || true' EXIT
sleep 1
dbus-run-session -- bash -s <<'PROBE'
set -euxo pipefail
export DISPLAY=:99
export GDK_BACKEND=x11
export GTK_MODULES=appmenu-gtk-module
export UBUNTU_MENUPROXY=1
/usr/libexec/vala-panel/appmenu-registrar >/tmp/registrar.log 2>&1 &
reg=$!
mousepad >/tmp/mousepad.log 2>&1 &
mp=$!
trap 'kill "$mp" "$reg" 2>/dev/null || true' EXIT
win=
for _ in $(seq 1 80); do
  win="$(xdotool search --onlyvisible --class mousepad 2>/dev/null | tail -n 1 || true)"
  if [[ -n "$win" ]]; then break; fi
  sleep .25
done
test -n "$win"
echo "WINDOW=$win"
xprop -id "$win" | grep -E '_GTK_(UNIQUE_BUS_NAME|APP_MENU_OBJECT_PATH|MENUBAR_OBJECT_PATH|APPLICATION_OBJECT_PATH|WINDOW_OBJECT_PATH)' || true
bus="$(xprop -id "$win" _GTK_UNIQUE_BUS_NAME | awk -F'"' '{print $2}' || true)"
menu_path="$(xprop -id "$win" _GTK_MENUBAR_OBJECT_PATH | awk -F'"' '{print $2}' || true)"
app_path="$(xprop -id "$win" _GTK_APPLICATION_OBJECT_PATH | awk -F'"' '{print $2}' || true)"
win_path="$(xprop -id "$win" _GTK_WINDOW_OBJECT_PATH | awk -F'"' '{print $2}' || true)"
echo "BUS=$bus MENU=$menu_path APP=$app_path WIN=$win_path"
gdbus call --session --dest "$bus" --object-path "$menu_path" --method org.gtk.Menus.Start '[uint32 0]'
for group in 1 2 3 4 5 6; do
  gdbus call --session --dest "$bus" --object-path "$menu_path" --method org.gtk.Menus.Start "[uint32 $group]"
done
echo APP_DESCRIBE_ALL
gdbus call --session --dest "$bus" --object-path "$app_path" --method org.gtk.Actions.DescribeAll
echo WIN_DESCRIBE_ALL
gdbus call --session --dest "$bus" --object-path "$win_path" --method org.gtk.Actions.DescribeAll
gdbus introspect --session --dest "$bus" --object-path "$app_path" | sed -n '1,180p'
gdbus introspect --session --dest "$bus" --object-path "$win_path" | sed -n '1,180p'
PROBE
