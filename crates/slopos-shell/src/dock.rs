//! Compact SLOPOS Platinum Application Strip.

use crate::launcher::Launcher;
use gdk_pixbuf::Pixbuf;
use gtk::atk::prelude::AtkObjectExt;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, IconSize, Image, Label, Orientation, Separator, Window, WindowPosition,
    WindowType,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

pub struct Dock {
    _window: Window,
}

#[derive(Clone, Copy)]
struct LaunchSpec {
    program: &'static str,
    args: &'static [&'static str],
}

impl Dock {
    pub fn new(launcher: Rc<Launcher>) -> Rc<Self> {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("SLOPOS Application Strip");
        let (screen_width, screen_height) = screen_geometry();
        // Keep enough room for the optional upstream game launcher while
        // remaining compact at the smallest supported desktop widths.
        let width = 540;
        let height = 54;
        window.set_default_size(width, height);
        window.set_position(WindowPosition::None);
        window.move_(
            (screen_width - width).max(0) / 2,
            (screen_height - height - 6).max(28),
        );
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        set_accessible_name(&window, "SLOPOS application strip");

        let dock_box = GtkBox::new(Orientation::Horizontal, 3);
        dock_box.style_context().add_class("slopos-dock-container");
        dock_box.set_hexpand(true);
        dock_box.set_vexpand(true);

        let strip_label = Label::new(Some("Apps"));
        strip_label.style_context().add_class("slopos-dock-label");
        strip_label.set_xalign(0.5);
        strip_label.set_yalign(0.5);
        set_accessible_name(&strip_label, "Application launchers");
        dock_box.pack_start(&strip_label, false, false, 2);

        let label_separator = Separator::new(Orientation::Vertical);
        label_separator
            .style_context()
            .add_class("slopos-dock-separator");
        dock_box.pack_start(&label_separator, false, false, 1);

        add_action_item(
            &dock_box,
            "search.svg",
            "system-search-symbolic",
            "Search applications (Super+Space)",
            {
                let launcher = launcher.clone();
                move || launcher.toggle()
            },
        );
        add_launch_item(
            &dock_box,
            "folder.svg",
            "folder-symbolic",
            "Files",
            &[
                LaunchSpec {
                    program: "pcmanfm",
                    args: &[],
                },
                LaunchSpec {
                    program: "thunar",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "terminal.svg",
            "utilities-terminal-symbolic",
            "Terminal",
            &[
                LaunchSpec {
                    program: "xfce4-terminal",
                    args: &[],
                },
                LaunchSpec {
                    program: "xterm",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "textedit.svg",
            "accessories-text-editor-symbolic",
            "Text Editor",
            &[
                LaunchSpec {
                    program: "mousepad",
                    args: &[],
                },
                LaunchSpec {
                    program: "xed",
                    args: &[],
                },
                LaunchSpec {
                    program: "gedit",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "browser.svg",
            "web-browser-symbolic",
            "Web Browser",
            &[
                LaunchSpec {
                    program: "start-slopos-browser",
                    args: &[],
                },
                LaunchSpec {
                    program: "firefox",
                    args: &[],
                },
                LaunchSpec {
                    program: "chromium",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "game.svg",
            "applications-games-symbolic",
            "SuperTux",
            &[
                LaunchSpec {
                    program: "supertux2",
                    args: &[],
                },
                LaunchSpec {
                    program: "supertux",
                    args: &[],
                },
            ],
        );
        add_launch_item(
            &dock_box,
            "software.svg",
            "system-software-install-symbolic",
            "Software Catalogue",
            &[LaunchSpec {
                program: "slopos-catalogue",
                args: &[],
            }],
        );
        add_launch_item(
            &dock_box,
            "settings.svg",
            "preferences-system-symbolic",
            "System Settings",
            &[LaunchSpec {
                program: "slopos-settings",
                args: &[],
            }],
        );

        let separator = Separator::new(Orientation::Vertical);
        separator.style_context().add_class("slopos-dock-separator");
        dock_box.pack_start(&separator, false, false, 1);

        add_launch_item(
            &dock_box,
            "trash.svg",
            "user-trash-symbolic",
            "Trash",
            &[LaunchSpec {
                program: "pcmanfm",
                args: &["trash:///"],
            }],
        );

        window.add(&dock_box);
        window.show_all();
        Rc::new(Self { _window: window })
    }
}

fn add_action_item<F>(
    dock: &GtkBox,
    custom_icon: &str,
    fallback_icon: &str,
    tooltip: &str,
    action: F,
) where
    F: Fn() + 'static,
{
    let button = dock_button(custom_icon, fallback_icon, tooltip);
    button.connect_clicked(move |_| action());
    dock.pack_start(&button, false, false, 0);
}

fn add_launch_item(
    dock: &GtkBox,
    custom_icon: &str,
    fallback_icon: &str,
    tooltip: &str,
    candidates: &[LaunchSpec],
) {
    let button = dock_button(custom_icon, fallback_icon, tooltip);
    let resolved = candidates.iter().find_map(|candidate| {
        resolve_program(candidate.program).map(|program| {
            (
                program,
                candidate
                    .args
                    .iter()
                    .map(|argument| (*argument).to_string())
                    .collect::<Vec<_>>(),
            )
        })
    });

    if let Some((program, args)) = resolved {
        button.connect_clicked(move |_| {
            if let Err(error) = Command::new(&program).args(&args).spawn() {
                log::warn!("Failed to launch {}: {error}", program.display());
            }
        });
    } else {
        button.set_sensitive(false);
        button.set_tooltip_text(Some(&format!("{tooltip} — not installed")));
    }
    dock.pack_start(&button, false, false, 0);
}

fn dock_button(custom_icon: &str, fallback_icon: &str, tooltip: &str) -> Button {
    let button = Button::new();
    button.style_context().add_class("slopos-dock-btn");
    button.set_relief(gtk::ReliefStyle::None);
    button.set_tooltip_text(Some(tooltip));
    set_accessible_name(&button, tooltip);
    button.set_image(Some(&load_icon(custom_icon, fallback_icon)));
    button
}

/// Icon-only launchers need an explicit ATK name; a tooltip is useful for
/// pointer users but is not a reliable accessible label for screen readers.
fn set_accessible_name<W>(widget: &W, name: &str)
where
    W: IsA<gtk::Widget>,
{
    let Some(accessible) = widget.accessible() else {
        return;
    };
    let Ok(accessible) = accessible.downcast::<gtk::atk::Object>() else {
        return;
    };
    accessible.set_name(name);
}

fn load_icon(file_name: &str, fallback: &str) -> Image {
    let mut candidates = Vec::new();
    if let Ok(share_dir) = env::var("SLOPOS_SHARE_DIR") {
        candidates.push(format!(
            "{share_dir}/slopos-i/themes/platinum/icons/{file_name}"
        ));
    }
    candidates.extend([
        format!("themes/platinum/icons/{file_name}"),
        format!("/usr/local/share/slopos-i/themes/platinum/icons/{file_name}"),
        format!("/usr/share/slopos-i/themes/platinum/icons/{file_name}"),
    ]);
    for path in candidates {
        if Path::new(&path).exists() {
            if let Ok(pixbuf) = Pixbuf::from_file_at_scale(&path, 32, 32, true) {
                return Image::from_pixbuf(Some(&pixbuf));
            }
        }
    }
    Image::from_icon_name(Some(fallback), IconSize::LargeToolbar)
}

fn resolve_program(program: &str) -> Option<PathBuf> {
    if program.starts_with("slopos-") {
        if let Ok(executable) = env::current_exe() {
            if let Some(parent) = executable.parent() {
                let sibling = parent.join(program);
                if sibling.is_file() {
                    return Some(sibling);
                }
            }
        }
    }

    if program.contains('/') {
        return Path::new(program).is_file().then(|| PathBuf::from(program));
    }

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}

fn screen_geometry() -> (i32, i32) {
    let Ok(output) = Command::new("xrandr").arg("--current").output() else {
        return (1280, 800);
    };
    if !output.status.success() {
        return (1280, 800);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let Some(after_current) = line.split("current ").nth(1) else {
            continue;
        };
        let Some(dimensions) = after_current.split(',').next() else {
            continue;
        };
        let mut parts = dimensions.split('x').map(str::trim);
        let (Some(width), Some(height)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(width), Ok(height)) = (width.parse::<i32>(), height.parse::<i32>()) {
            let scale = env::var("GDK_SCALE")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .filter(|scale| *scale > 0)
                .unwrap_or(1);
            return ((width / scale).max(1), (height / scale).max(1));
        }
    }
    (1280, 800)
}
