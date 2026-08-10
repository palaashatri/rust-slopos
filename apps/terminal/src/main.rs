use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::window::Window;
use slopos_kit::{Event, Widget};
use slopos_sdk::{build_menu, Application};

mod pty;
mod tabs;
mod terminal;
mod vt_parser;

use tabs::TabManager;

fn main() {
    let _ = tracing_subscriber::fmt::try_init();
    let mut app = Application::new("Terminal", "com.slopos.terminal");

    let mut shell_menu = build_menu("Shell");
    {
        let item = shell_menu.add_action("New Window");
        item.with_shortcut(
            KeyCode::N,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }
    {
        let item = shell_menu.add_action("New Tab");
        item.with_shortcut(
            KeyCode::T,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }
    shell_menu.add_separator();
    {
        let item = shell_menu.add_action("Close Tab");
        item.with_shortcut(
            KeyCode::W,
            Modifiers {
                shift: true,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }
    {
        let item = shell_menu.add_action("Close Window");
        item.with_shortcut(
            KeyCode::W,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }

    let mut edit_menu = build_menu("Edit");
    {
        let item = edit_menu.add_action("Copy");
        item.with_shortcut(
            KeyCode::C,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }
    {
        let item = edit_menu.add_action("Paste");
        item.with_shortcut(
            KeyCode::V,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }
    edit_menu.add_separator();
    {
        let item = edit_menu.add_action("Select All");
        item.with_shortcut(
            KeyCode::A,
            Modifiers {
                shift: false,
                control: false,
                alt: false,
                meta: true,
            },
        );
    }

    let mut view_menu = build_menu("View");
    view_menu.add_action("Zoom In");
    view_menu.add_action("Zoom Out");

    let mut window_menu = build_menu("Window");
    window_menu.add_action("Minimize");
    window_menu.add_action("Zoom");

    let mut help_menu = build_menu("Help");
    help_menu.add_action("Terminal Help");

    app.set_menus(vec![
        shell_menu,
        edit_menu,
        view_menu,
        window_menu,
        help_menu,
    ]);

    // Menu presentation belongs to the shell; tab and terminal commands are
    // handled by this client over its private application endpoint.
    app.on_menu_action(|action, window| {
        let Some(content) = window.content.as_mut() else {
            return;
        };
        let Some(tabs) = content.as_any_mut().downcast_mut::<TabManager>() else {
            return;
        };
        let action = action
            .strip_prefix("com.slopos.terminal.")
            .unwrap_or(action);
        match action {
            "shell.new_tab" => {
                let _ = tabs.open_tab(80, 24);
            }
            "shell.close_tab" => {
                let _ = tabs.close_tab(tabs.active_tab_index());
            }
            "edit.copy" => {
                let _ = tabs.handle_event(&Event::KeyDown {
                    key: KeyCode::C,
                    modifiers: Modifiers {
                        meta: true,
                        ..Modifiers::NONE
                    },
                });
            }
            "edit.paste" => {
                let _ = tabs.handle_event(&Event::KeyDown {
                    key: KeyCode::V,
                    modifiers: Modifiers {
                        meta: true,
                        ..Modifiers::NONE
                    },
                });
            }
            "edit.select_all" => {
                let _ = tabs.handle_event(&Event::KeyDown {
                    key: KeyCode::A,
                    modifiers: Modifiers {
                        meta: true,
                        ..Modifiers::NONE
                    },
                });
            }
            _ => {}
        }
    });

    let mut tab_manager = TabManager::new();
    tab_manager.set_event_loop_waker(app.event_waker());
    if let Err(e) = tab_manager.open_tab(80, 24) {
        tracing::error!("Failed to open initial tab: {}", e);
    }

    let mut window = Window::new("Terminal");
    window.set_content(Box::new(tab_manager));
    app.set_main_window(window);
    app.run();
}
