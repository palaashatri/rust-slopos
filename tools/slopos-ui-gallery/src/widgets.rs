//! `--component widgets`: every UI-02 control in one interactive window.
//! File ▸ Open… (Ctrl+O) shows the file chooser, File ▸ Rename… (F2) a text
//! dialog; the list has 10 000 rows to exercise virtualization.

use std::path::PathBuf;

use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::dialog::{Dialog, DialogOutcome};
use slopos_ui::controls::file_chooser::{ChooserOutcome, FileChooser};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::menu::{MenuBar, MenuBarOutcome, MenuItem, MENU_BAR_HEIGHT};
use slopos_ui::controls::range::{Checkbox, ProgressBar, Slider, ToggleOutcome, ValueOutcome};
use slopos_ui::event::{Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::Font;
use slopos_ui::theme::palette;

pub const SIZE: (u32, u32) = (480, 360);
const ROWS: usize = 10_000;

const LIST: u64 = 20;
const SLIDER: u64 = 21;
const CHECK: u64 = 22;

pub struct Widgets {
    window: (u32, u32),
    menu: MenuBar,
    list: ListView,
    slider: Slider,
    check: Checkbox,
    progress: ProgressBar,
    focus: FocusChain,
    dialog: Option<Dialog>,
    chooser: Option<FileChooser>,
    /// Last thing that happened, shown at the bottom (and to screen readers).
    pub status: String,
    font: Font,
}

fn row_label(index: usize) -> String {
    format!("Row {index:05}")
}

impl Widgets {
    pub fn new() -> Self {
        let menu = MenuBar::new(
            100,
            vec![
                (
                    "File",
                    vec![
                        MenuItem::new("Open…").shortcut("Ctrl+O"),
                        MenuItem::new("Rename…").shortcut("F2"),
                        MenuItem::separator(),
                        MenuItem::new("Disabled item").enabled(false),
                        MenuItem::new("Quit").shortcut("Ctrl+Q"),
                    ],
                ),
                ("View", vec![MenuItem::new("Show progress").checked(true)]),
            ],
        );
        let mut list = ListView::new(LIST, "Rows", 18);
        list.multiple = true;
        list.set_len(ROWS);
        let mut slider = Slider::new(SLIDER, "Volume", 0.0, 100.0, 5.0);
        slider.set_value(40.0);
        let mut progress = ProgressBar::new(23, "Progress");
        progress.fraction = Some(0.4);
        let mut widgets = Self {
            window: SIZE,
            menu,
            list,
            slider,
            check: Checkbox::new(CHECK, "Indeterminate progress", false),
            progress,
            focus: FocusChain::new(vec![LIST, SLIDER, CHECK]),
            dialog: None,
            chooser: None,
            status: "Ready".into(),
            font: Font::system(12.0),
        };
        widgets.layout();
        widgets.focus.focus(LIST);
        widgets.sync_focus();
        widgets
    }

    fn layout(&mut self) {
        let (width, height) = self.window;
        self.menu.bounds = Rect {
            x: 0,
            y: 0,
            width,
            height: MENU_BAR_HEIGHT,
        };
        let top = MENU_BAR_HEIGHT as i32 + 12;
        self.list.set_bounds(Rect {
            x: 12,
            y: top,
            width: 220,
            height: height.saturating_sub(top as u32 + 40),
        });
        self.slider.bounds = Rect {
            x: 252,
            y: top + 20,
            width: 200,
            height: 20,
        };
        self.check = self.check.clone().at(252, top + 60);
        self.progress.bounds = Rect {
            x: 252,
            y: top + 100,
            width: 200,
            height: 12,
        };
        if let Some(dialog) = &mut self.dialog {
            dialog.layout(self.window);
        }
        if let Some(chooser) = &mut self.chooser {
            chooser.layout(self.window);
        }
    }

    fn sync_focus(&mut self) {
        self.list.focused = self.focus.is_focused(LIST);
        self.slider.focused = self.focus.is_focused(SLIDER);
        self.check.focused = self.focus.is_focused(CHECK);
    }

    fn menu_command(&mut self, menu: usize, item: usize, control: &mut Control) {
        match (menu, item) {
            (0, 0) => self.open_chooser(),
            (0, 1) => self.open_rename(),
            (0, 4) => control.quit(),
            (1, 0) => {
                let shown = !self.menu.menus[1].items[0].checked;
                self.menu.menus[1].items[0].checked = shown;
                self.status = format!("Show progress: {shown}");
            }
            _ => {}
        }
    }

    fn open_chooser(&mut self) {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let mut chooser = FileChooser::new(300, "Open", &home, None);
        chooser.layout(self.window);
        self.chooser = Some(chooser);
    }

    fn open_rename(&mut self) {
        let name = self.list.cursor().map_or_else(String::new, row_label);
        let mut dialog = Dialog::new(
            400,
            "Rename",
            "Enter a new name for the selected row.",
            &["Cancel", "Rename"],
        )
        .with_field("Name", &name);
        dialog.layout(self.window);
        self.dialog = Some(dialog);
    }
}

impl App for Widgets {
    fn event(&mut self, event: &Event, control: &mut Control) {
        if let Event::Resized { width, height } = event {
            self.window = (*width, *height);
            self.layout();
            control.request_redraw();
            return;
        }
        if let Event::CloseRequested = event {
            control.quit();
            return;
        }
        if let Some(chooser) = &mut self.chooser {
            match chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    self.status = format!("Chose {}", path.display());
                    self.chooser = None;
                }
                ChooserOutcome::Cancelled => {
                    self.status = "Open cancelled".into();
                    self.chooser = None;
                }
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            control.request_redraw();
            return;
        }
        if let Some(dialog) = &mut self.dialog {
            match dialog.handle(event) {
                DialogOutcome::Chosen { index, text } => {
                    self.status = match (index, text) {
                        (1, Some(text)) => format!("Renamed to {text}"),
                        _ => "Rename cancelled".into(),
                    };
                    self.dialog = None;
                }
                DialogOutcome::Copy(text) => control.set_clipboard(&text),
                DialogOutcome::PasteRequested => control.request_paste(),
                _ => {}
            }
            control.request_redraw();
            return;
        }
        match self.menu.handle(event, self.window) {
            MenuBarOutcome::Chosen { menu, item } => {
                self.menu_command(menu, item, control);
                control.request_redraw();
                return;
            }
            MenuBarOutcome::Changed => {
                control.request_redraw();
                return;
            }
            MenuBarOutcome::Ignored => {}
        }
        if let Event::KeyPressed { key, modifiers } = event {
            let command = match key {
                Key::Character('o' | 'O') if modifiers.control => Some((0, 0)),
                Key::Function(2) => Some((0, 1)),
                Key::Character('q' | 'Q') if modifiers.control => Some((0, 4)),
                _ => None,
            };
            if let Some((menu, item)) = command {
                self.menu_command(menu, item, control);
                control.request_redraw();
                return;
            }
        }
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            control.request_redraw();
            return;
        }
        match self.list.handle(event, control.modifiers(), &row_label) {
            ListOutcome::Ignored => {}
            ListOutcome::FocusRequested => {
                self.focus.focus(LIST);
                self.sync_focus();
                control.request_redraw();
            }
            ListOutcome::Activated(index) => {
                self.status = format!("Opened {}", row_label(index));
                control.request_redraw();
            }
            ListOutcome::ContextMenu { index, .. } => {
                self.status = format!("Context menu for {index:?}");
                control.request_redraw();
            }
            ListOutcome::SelectionChanged => {
                self.status = format!("{} selected", self.list.selection().len());
                control.request_redraw();
            }
            ListOutcome::Changed => control.request_redraw(),
        }
        match self.slider.handle(event) {
            ValueOutcome::Ignored => {}
            ValueOutcome::FocusRequested => {
                self.focus.focus(SLIDER);
                self.sync_focus();
                control.request_redraw();
            }
            ValueOutcome::Value { value, .. } => {
                self.progress.fraction = Some(value / 100.0);
                self.status = format!("Volume {value:.0}");
                control.request_redraw();
            }
            ValueOutcome::Changed => control.request_redraw(),
        }
        match self.check.handle(event) {
            ToggleOutcome::Toggled(on) => {
                self.progress.fraction = if on {
                    None
                } else {
                    Some(self.slider.value() / 100.0)
                };
                self.focus.focus(CHECK);
                self.sync_focus();
                control.request_redraw();
            }
            ToggleOutcome::Changed => control.request_redraw(),
            _ => {}
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.paint_all(surface) {
            eprintln!("slopos-ui-gallery: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        let mut nodes = self.menu.accessibility_nodes();
        nodes.extend(self.list.accessibility_nodes(&row_label));
        nodes.push((SLIDER, self.slider.accessibility_node()));
        nodes.push((CHECK, self.check.accessibility_node()));
        nodes.push((self.progress.id, self.progress.accessibility_node()));
        let mut status = accesskit::Node::new(accesskit::Role::Label);
        status.set_label(self.status.as_str());
        nodes.push((24, status));
        let focus = if let Some(chooser) = &self.chooser {
            nodes.extend(chooser.accessibility_nodes());
            chooser.focus_node()
        } else if let Some(dialog) = &self.dialog {
            nodes.extend(dialog.accessibility_nodes());
            dialog.focus_node()
        } else if let Some(node) = self.menu.focus_node() {
            node
        } else if self.list.focused {
            self.list.focus_node()
        } else {
            self.focus.focused().unwrap_or(0)
        };
        Some(window_tree("Widgets", nodes, Some(focus)))
    }
}

impl Widgets {
    fn paint_all(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        surface.clear(colors.surface)?;
        self.menu.paint(surface)?;
        let font = &self.font;
        self.list.paint(surface, |surface, index, rect, _| {
            surface.draw_text(&row_label(index), rect.x + 6, rect.y + 1, font, colors.ink)
        })?;
        surface.draw_text("Volume", 252, self.slider.bounds.y - 16, font, colors.ink)?;
        self.slider.paint(surface)?;
        self.check.paint(surface)?;
        if self.menu.menus[1].items[0].checked {
            self.progress.paint(surface)?;
        }
        let bottom = self.window.1 as i32 - 22;
        surface.draw_text(&self.status, 12, bottom, font, colors.ink_secondary)?;
        if let Some(dialog) = &mut self.dialog {
            dialog.paint(surface)?;
        }
        if let Some(chooser) = &mut self.chooser {
            chooser.paint(surface)?;
        }
        self.menu.paint_menus(surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::Modifiers;

    fn key(key: Key, control: bool) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers {
                control,
                ..Modifiers::default()
            },
        }
    }

    #[test]
    fn keyboard_journey_through_menu_dialog_and_list() {
        let mut app = Widgets::new();
        let mut control = Control::default();
        app.event(&key(Key::End, false), &mut control);
        assert_eq!(app.list.cursor(), Some(ROWS - 1));
        app.event(&key(Key::Function(2), false), &mut control);
        assert!(app.dialog.is_some());
        app.event(&key(Key::Character('X'), false), &mut control);
        app.event(&key(Key::Enter, false), &mut control);
        assert!(app.dialog.is_none());
        assert_eq!(app.status, "Renamed to X");
        app.event(&key(Key::Function(10), false), &mut control);
        app.event(&key(Key::Right, false), &mut control);
        app.event(&key(Key::Enter, false), &mut control);
        assert_eq!(app.status, "Show progress: false");
        app.event(&key(Key::Tab, false), &mut control);
        app.event(&key(Key::Right, false), &mut control);
        assert_eq!(app.status, "Volume 45");
        let tree = app.accessibility().unwrap();
        assert_eq!(tree.focus, accesskit::NodeId(SLIDER));
    }

    #[test]
    fn chooser_cancel_returns_to_the_window() {
        let mut app = Widgets::new();
        let mut control = Control::default();
        app.event(&key(Key::Character('o'), true), &mut control);
        assert!(app.chooser.is_some());
        app.event(&key(Key::Escape, false), &mut control);
        assert!(app.chooser.is_none());
        assert_eq!(app.status, "Open cancelled");
    }
}
