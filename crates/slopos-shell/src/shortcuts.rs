//! Shell-owned global X11 keyboard shortcuts.
//!
//! The classic menu bar must remain keyboard reachable even though its dock
//! window deliberately does not accept normal application focus.  Own Ctrl+F2
//! here instead of delegating it to Openbox so the desktop-shell contract does
//! not depend on a particular window-manager command runner.

use std::thread;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, GrabMode, ModMask};
use x11rb::protocol::Event;

const XK_F2: u32 = 0xffbf;

pub fn install_system_menu_shortcut() {
    let spawn = thread::Builder::new()
        .name("slopos-system-menu-shortcut".to_string())
        .spawn(run_system_menu_shortcut);
    if let Err(error) = spawn {
        log::warn!("Could not start Ctrl+F2 shortcut listener: {error}");
    }
}

fn run_system_menu_shortcut() {
    let (connection, screen_number) = match x11rb::connect(None) {
        Ok(value) => value,
        Err(error) => {
            log::warn!("Ctrl+F2 shortcut could not connect to X11: {error}");
            return;
        }
    };

    let setup = connection.setup();
    let Some(screen) = setup.roots.get(screen_number) else {
        log::warn!("Ctrl+F2 shortcut could not resolve the active X11 screen");
        return;
    };
    let root = screen.root;
    let first_keycode = setup.min_keycode;
    let keycode_count = setup
        .max_keycode
        .saturating_sub(first_keycode)
        .saturating_add(1);

    let mapping = match connection.get_keyboard_mapping(first_keycode, keycode_count) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => reply,
            Err(error) => {
                log::warn!("Ctrl+F2 shortcut could not read the X11 keymap: {error}");
                return;
            }
        },
        Err(error) => {
            log::warn!("Ctrl+F2 shortcut could not request the X11 keymap: {error}");
            return;
        }
    };

    let keysyms_per_keycode = usize::from(mapping.keysyms_per_keycode);
    if keysyms_per_keycode == 0 {
        log::warn!("Ctrl+F2 shortcut received an empty X11 keymap");
        return;
    }
    let Some(offset) = mapping
        .keysyms
        .chunks(keysyms_per_keycode)
        .position(|symbols| symbols.contains(&XK_F2))
    else {
        log::warn!("Ctrl+F2 shortcut could not find F2 in the X11 keymap");
        return;
    };
    let Ok(offset) = u8::try_from(offset) else {
        log::warn!("Ctrl+F2 shortcut resolved an invalid X11 keycode offset");
        return;
    };
    let Some(f2_keycode) = first_keycode.checked_add(offset) else {
        log::warn!("Ctrl+F2 shortcut resolved an invalid X11 keycode");
        return;
    };

    // Passive grabs require an exact modifier state.  Accept the normal lock
    // modifiers as well so Caps Lock / Num Lock do not make the menu bar
    // inaccessible. M5 is included for X11 layouts that map Scroll Lock there.
    let lock_variants = [
        ModMask::CONTROL,
        ModMask::CONTROL | ModMask::LOCK,
        ModMask::CONTROL | ModMask::M2,
        ModMask::CONTROL | ModMask::M5,
        ModMask::CONTROL | ModMask::LOCK | ModMask::M2,
        ModMask::CONTROL | ModMask::LOCK | ModMask::M5,
        ModMask::CONTROL | ModMask::M2 | ModMask::M5,
        ModMask::CONTROL | ModMask::LOCK | ModMask::M2 | ModMask::M5,
    ];

    for modifiers in lock_variants {
        if let Err(error) = connection.grab_key(
            false,
            root,
            modifiers,
            f2_keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            log::warn!("Could not register Ctrl+F2 X11 grab ({modifiers:?}): {error}");
            return;
        }
    }
    if let Err(error) = connection.flush() {
        log::warn!("Could not flush Ctrl+F2 X11 grabs: {error}");
        return;
    }

    log::info!("SLOPOS_SYSTEM_MENU_SHORTCUT_READY keycode={f2_keycode}");
    loop {
        match connection.wait_for_event() {
            Ok(Event::KeyPress(event)) if event.detail == f2_keycode => {
                // Reuse the top-bar's async-signal-safe bridge. The GTK work
                // remains on its main thread and this listener owns no widgets.
                unsafe {
                    libc::kill(libc::getpid(), libc::SIGUSR2);
                }
            }
            Ok(_) => {}
            Err(error) => {
                log::warn!("Ctrl+F2 X11 shortcut listener stopped: {error}");
                break;
            }
        }
    }
}
