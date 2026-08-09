use crate::measure_text_width;
use crate::{
    design_tokens::{
        MENU_BAR_LEADING, MENU_DROPDOWN_VERTICAL_PADDING, MENU_ITEM_GAP, MENU_ITEM_HEIGHT,
        MENU_TITLE_HORIZONTAL_PADDING,
    },
    event::MouseButton,
    menu::{Menu, MenuItemKind},
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, Event, EventResult, LayoutConstraint, Rect, Size, Widget,
    WidgetState,
};
use std::any::Any;

pub struct MenuBar {
    state: WidgetState,
    pub menus: Vec<Menu>,
    pub open_menu: Option<usize>,
    pub hovered_menu: Option<usize>,
    pub hovered_item: Option<usize>,
    pub last_action: Option<String>,
    menu_rects: Vec<Rect>,
    /// When true, SDK paint draws only the open dropdown at (0,0) for an Overlay layer.
    pub layer_popup_origin: bool,
    /// When true, skip drawing the dropdown on the bar (Overlay owns it).
    pub suppress_dropdown_paint: bool,
}

impl MenuBar {
    pub fn new(menus: Vec<Menu>) -> Self {
        Self {
            state: WidgetState::new(),
            menus,
            open_menu: None,
            hovered_menu: None,
            hovered_item: None,
            last_action: None,
            menu_rects: vec![],
            layer_popup_origin: false,
            suppress_dropdown_paint: false,
        }
    }

    pub fn menu_rects(&self) -> &[Rect] {
        &self.menu_rects
    }

    pub fn open_menu(&mut self, index: usize) {
        let _ = self.open_menu_at(index);
    }

    /// Open the top-level menu at `index`. Returns `true` if the index was valid.
    ///
    /// Pure state update (no layout/event side effects) — suitable for a11y
    /// invoke and keyboard activation.
    pub fn open_menu_at(&mut self, index: usize) -> bool {
        if index < self.menus.len() {
            self.open_menu = Some(index);
            self.hovered_menu = Some(index);
            self.hovered_item = None;
            true
        } else {
            false
        }
    }

    /// Open the first top-level menu (typically the system / app menu).
    /// Returns `true` if the menu bar has at least one menu.
    pub fn open_first_menu(&mut self) -> bool {
        self.open_menu_at(0)
    }

    pub fn close(&mut self) {
        self.open_menu = None;
        self.hovered_item = None;
    }

    pub fn dropdown_rect(&self, index: usize) -> Option<Rect> {
        let menu = self.menus.get(index)?;
        let menu_rect = *self.menu_rects.get(index)?;
        let item_width = menu
            .items
            .iter()
            .filter(|item| !matches!(item.kind, MenuItemKind::Separator))
            .map(|item| {
                let shortcut_width = item
                    .shortcut
                    .map(|(key, modifiers)| {
                        measure_text_width(&shortcut_label(key, modifiers)) + 18.0
                    })
                    .unwrap_or(0.0);
                measure_text_width(&item.label) + shortcut_width + 44.0
            })
            .fold(180.0, f32::max);
        Some(Rect::new(
            menu_rect.x,
            self.rect().y + self.rect().height - 1.0,
            item_width,
            menu.items.len() as f32 * MENU_ITEM_HEIGHT + MENU_DROPDOWN_VERTICAL_PADDING,
        ))
    }

    pub fn item_rect(&self, menu_index: usize, item_index: usize) -> Option<Rect> {
        let dropdown = self.dropdown_rect(menu_index)?;
        Some(Rect::new(
            dropdown.x + 2.0,
            dropdown.y + 2.0 + item_index as f32 * MENU_ITEM_HEIGHT,
            dropdown.width - 4.0,
            MENU_ITEM_HEIGHT,
        ))
    }

    fn menu_at_point(&self, point: crate::Point) -> Option<usize> {
        self.menu_rects
            .iter()
            .position(|menu_rect| menu_rect.contains(point))
    }

    fn item_at_point(&self, point: crate::Point) -> Option<(usize, usize)> {
        let menu_index = self.open_menu?;
        let menu = self.menus.get(menu_index)?;
        menu.items.iter().enumerate().find_map(|(item_index, _)| {
            self.item_rect(menu_index, item_index)
                .filter(|rect| rect.contains(point))
                .map(|_| (menu_index, item_index))
        })
    }
}

fn shortcut_label(key: crate::event::KeyCode, modifiers: crate::event::Modifiers) -> String {
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.meta {
        parts.push("Cmd");
    }
    parts.push(key_label(key));
    parts.join("+")
}

fn key_label(key: crate::event::KeyCode) -> &'static str {
    match key {
        crate::event::KeyCode::A => "A",
        crate::event::KeyCode::B => "B",
        crate::event::KeyCode::C => "C",
        crate::event::KeyCode::D => "D",
        crate::event::KeyCode::E => "E",
        crate::event::KeyCode::F => "F",
        crate::event::KeyCode::G => "G",
        crate::event::KeyCode::H => "H",
        crate::event::KeyCode::I => "I",
        crate::event::KeyCode::J => "J",
        crate::event::KeyCode::K => "K",
        crate::event::KeyCode::L => "L",
        crate::event::KeyCode::M => "M",
        crate::event::KeyCode::N => "N",
        crate::event::KeyCode::O => "O",
        crate::event::KeyCode::P => "P",
        crate::event::KeyCode::Q => "Q",
        crate::event::KeyCode::R => "R",
        crate::event::KeyCode::S => "S",
        crate::event::KeyCode::T => "T",
        crate::event::KeyCode::U => "U",
        crate::event::KeyCode::V => "V",
        crate::event::KeyCode::W => "W",
        crate::event::KeyCode::X => "X",
        crate::event::KeyCode::Y => "Y",
        crate::event::KeyCode::Z => "Z",
        crate::event::KeyCode::Backspace => "Del",
        crate::event::KeyCode::Escape => "Esc",
        crate::event::KeyCode::Enter => "Ret",
        crate::event::KeyCode::Space => "Space",
        crate::event::KeyCode::ArrowUp => "Up",
        crate::event::KeyCode::ArrowDown => "Down",
        crate::event::KeyCode::ArrowLeft => "Left",
        crate::event::KeyCode::ArrowRight => "Right",
        _ => "Key",
    }
}

impl Widget for MenuBar {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(Size::new(
            constraint.max_width,
            crate::design_tokens::MENU_BAR_HEIGHT,
        ));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));

        // Classic Mac–style spacing: padding inside each title hit target, plus
        // a clear gap between adjacent menu titles so they don't read as one word.
        const LEADING: f32 = MENU_BAR_LEADING;
        const TITLE_PAD: f32 = MENU_TITLE_HORIZONTAL_PADDING;
        const ITEM_GAP: f32 = MENU_ITEM_GAP;
        self.menu_rects.clear();
        let mut x = self.rect().x + LEADING;
        for (i, menu) in self.menus.iter().enumerate() {
            // Apple / first menu needs room for the icon before the title text.
            let icon_extra = if i == 0 { 14.0 } else { 0.0 };
            let width = measure_text_width(&menu.title) + TITLE_PAD + icon_extra;
            self.menu_rects
                .push(Rect::new(x, self.rect().y, width, size.height));
            x += width + ITEM_GAP;
        }

        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { point, .. } => {
                self.hovered_menu = self.menu_at_point(*point);
                if self.open_menu.is_some() {
                    if let Some(menu_index) = self.hovered_menu {
                        self.open_menu = Some(menu_index);
                        self.hovered_item = None;
                    } else {
                        self.hovered_item =
                            self.item_at_point(*point).map(|(_, item_index)| item_index);
                    }
                }
                EventResult::RequestRedraw
            }
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                ..
            } => {
                if let Some(menu_index) = self.menu_at_point(*point) {
                    if self.open_menu == Some(menu_index) {
                        self.close();
                    } else {
                        self.open_menu(menu_index);
                    }
                    return EventResult::Handled;
                }

                if let Some((menu_index, item_index)) = self.item_at_point(*point) {
                    if let Some(item) = self.menus[menu_index].items.get(item_index) {
                        if !matches!(item.kind, MenuItemKind::Separator) && item.enabled {
                            self.last_action = Some(if item.action_id.is_empty() {
                                item.label.clone()
                            } else {
                                item.action_id.clone()
                            });
                        }
                    }
                    self.close();
                    return EventResult::Handled;
                }

                if self.open_menu.is_some() {
                    self.close();
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::MouseLeave => {
                self.hovered_menu = None;
                self.hovered_item = None;
                EventResult::RequestRedraw
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::MenuBar,
            "menu bar",
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::Modifiers, Point};
    use slopos_render::font::{shape_text, TextLayoutOptions};

    fn test_menu_bar() -> MenuBar {
        let mut file = Menu::new("File");
        file.add_action("Open").with_action("open");
        file.add_separator();
        file.add_action("Close").with_action("close");

        let mut edit = Menu::new("Edit");
        edit.add_action("Copy").with_action("copy");

        MenuBar::new(vec![file, edit])
    }

    #[test]
    fn menu_bar_opens_switches_and_closes() {
        let mut menu_bar = test_menu_bar();
        menu_bar.layout(LayoutConstraint::tight(Size::new(640.0, 19.0)));

        let file_point = Point::new(16.0, 10.0);
        let edit_point = Point::new(menu_bar.menu_rects()[1].x + 6.0, 10.0);

        assert!(matches!(
            menu_bar.handle_event(&Event::MouseDown {
                button: MouseButton::Left,
                point: file_point,
                modifiers: Modifiers::NONE,
            }),
            EventResult::Handled
        ));
        assert_eq!(menu_bar.open_menu, Some(0));

        let _ = menu_bar.handle_event(&Event::MouseMove {
            point: edit_point,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(menu_bar.open_menu, Some(1));

        let _ = menu_bar.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: edit_point,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(menu_bar.open_menu, None);
    }

    #[test]
    fn menu_bar_records_action_from_dropdown_click() {
        let mut menu_bar = test_menu_bar();
        menu_bar.layout(LayoutConstraint::tight(Size::new(640.0, 19.0)));
        menu_bar.open_menu(0);

        let open_rect = menu_bar.item_rect(0, 0).unwrap();
        let _ = menu_bar.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(open_rect.x + 8.0, open_rect.y + 8.0),
            modifiers: Modifiers::NONE,
        });

        assert_eq!(menu_bar.last_action.as_deref(), Some("open"));
        assert_eq!(menu_bar.open_menu, None);
    }

    #[test]
    fn open_menu_at_opens_by_index_and_rejects_oob() {
        let mut menu_bar = test_menu_bar();
        assert!(menu_bar.open_menu.is_none());

        assert!(menu_bar.open_menu_at(1));
        assert_eq!(menu_bar.open_menu, Some(1));
        assert_eq!(menu_bar.hovered_menu, Some(1));
        assert!(menu_bar.hovered_item.is_none());

        assert!(!menu_bar.open_menu_at(99));
        // Invalid index leaves the previously open menu alone.
        assert_eq!(menu_bar.open_menu, Some(1));

        assert!(menu_bar.open_menu_at(0));
        assert_eq!(menu_bar.open_menu, Some(0));
    }

    #[test]
    fn open_first_menu_opens_index_zero() {
        let mut menu_bar = test_menu_bar();
        assert!(menu_bar.open_first_menu());
        assert_eq!(menu_bar.open_menu, Some(0));
        assert_eq!(menu_bar.hovered_menu, Some(0));

        let mut empty = MenuBar::new(vec![]);
        assert!(!empty.open_first_menu());
        assert!(empty.open_menu.is_none());
    }

    #[test]
    fn menu_title_geometry_uses_shaped_width_for_unicode_and_variable_glyphs() {
        let mut menu_bar = MenuBar::new(vec![Menu::new("SLOPOS"), Menu::new("日本語")]);
        menu_bar.layout(LayoutConstraint::tight(Size::new(640.0, 19.0)));

        let expected = shape_text("日本語", TextLayoutOptions::new(13.0, 1.0)).first_line_width()
            + MENU_TITLE_HORIZONTAL_PADDING;
        let actual = menu_bar.menu_rects()[1].width;
        assert!((actual - expected).abs() < 0.01);

        let byte_count_estimate = "日本語".len() as f32 * 7.0 + MENU_TITLE_HORIZONTAL_PADDING;
        assert!(
            (actual - byte_count_estimate).abs() > 0.5,
            "Unicode title must not use a UTF-8 byte-count width estimate"
        );
    }

    #[test]
    fn dropdown_geometry_measures_unicode_labels_and_shortcuts() {
        let label = "日本語".repeat(8);
        let mut menu = Menu::new("File");
        menu.add_action(label.clone()).with_shortcut(
            crate::event::KeyCode::A,
            Modifiers {
                meta: true,
                ..Modifiers::NONE
            },
        );
        let mut menu_bar = MenuBar::new(vec![menu]);
        menu_bar.layout(LayoutConstraint::tight(Size::new(640.0, 19.0)));

        let expected_label =
            shape_text(&label, TextLayoutOptions::new(13.0, 1.0)).first_line_width();
        let expected_shortcut =
            shape_text("Cmd+A", TextLayoutOptions::new(13.0, 1.0)).first_line_width();
        let expected = (expected_label + expected_shortcut + 18.0 + 44.0).max(180.0);
        let actual = menu_bar.dropdown_rect(0).expect("laid out menu").width;
        assert!((actual - expected).abs() < 0.01);
    }
}
