use slopos_kit::*;

#[test]
fn test_widget_creation() {
    let button = Button::new("Click me");
    assert_eq!(button.label, "Click me");
    assert!(button.enabled());
}

#[test]
fn test_status_bar() {
    let mut bar = StatusBar::new();
    bar.add_item("Running", StatusBarAlignment::Left, 100.0);
    assert_eq!(bar.items.len(), 1);
}

#[test]
fn test_tab_view() {
    let mut tab_view = TabView::new();
    let btn = Button::new("Inner");
    tab_view.add_tab("tab-1", "Tab 1", Box::new(btn));
    assert_eq!(tab_view.tabs.len(), 1);
    assert!(tab_view.selected_content().is_some());
}

#[test]
fn test_popup_button() {
    let mut pop = PopupButton::new();
    pop.add_item("Option 1");
    pop.add_item("Option 2");
    assert_eq!(pop.items.len(), 2);
    assert_eq!(pop.selected_index, 0);
    assert!(pop.select_item(1));
    assert_eq!(pop.selected_index, 1);
}

#[test]
fn test_clipboard() {
    Clipboard::copy("Hello Clipboard");
    assert_eq!(Clipboard::paste(), "Hello Clipboard");
    Clipboard::clear();
    assert_eq!(Clipboard::paste(), "");
}

#[test]
fn test_dnd() {
    let session = DragSession {
        payload: DragData::Text("DnD text".to_string()),
        current_position: Point::new(10.0, 20.0),
    };
    match session.payload {
        DragData::Text(t) => assert_eq!(t, "DnD text"),
        _ => panic!("Expected text payload"),
    }
}

#[test]
fn test_text_field_set_text_places_cursor_at_end() {
    let mut field = TextField::new();
    field.set_text("abc");
    // `TextField` now gates keyboard input on focus (see
    // AGENTS.md P2 / `text_field.rs`) — this test
    // predates that fix and simulates an already-focused field the way a real
    // `MouseDown` or `FocusManager::focus()` would leave it.
    field.widget_state_mut().focused = true;
    let result = field.handle_event(&Event::Char { character: 'd' });
    assert!(matches!(result, EventResult::Handled));
    assert_eq!(field.text(), "abcd");

    let result = field.handle_event(&Event::KeyDown {
        key: slopos_kit::event::KeyCode::Backspace,
        modifiers: slopos_kit::event::Modifiers::NONE,
    });
    assert!(matches!(result, EventResult::Handled));
    assert_eq!(field.text(), "abc");
}

struct FixedWidget {
    state: WidgetState,
    size: Size,
    handled_events: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl FixedWidget {
    fn new(width: f32, height: f32) -> Self {
        Self {
            state: WidgetState::new(),
            size: Size::new(width, height),
            handled_events: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn with_counter(
        width: f32,
        height: f32,
        handled_events: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            state: WidgetState::new(),
            size: Size::new(width, height),
            handled_events,
        }
    }
}

impl Widget for FixedWidget {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }

    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let size = constraint.clamp(self.size);
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    fn handle_event(&mut self, _event: &Event) -> EventResult {
        self.handled_events
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        EventResult::Handled
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn test_layout_padding_is_applied_once() {
    let mut horizontal = Layout::horizontal(5.0).padding(10.0);
    horizontal.add(Box::new(FixedWidget::new(50.0, 20.0)));
    horizontal.add(Box::new(FixedWidget::new(50.0, 20.0)));
    let size = horizontal.layout_size(LayoutConstraint::UNBOUNDED);
    assert_eq!(size.width, 125.0);
    assert_eq!(size.height, 40.0);

    let mut vertical = Layout::vertical(5.0).padding(10.0);
    vertical.add(Box::new(FixedWidget::new(50.0, 20.0)));
    vertical.add(Box::new(FixedWidget::new(50.0, 20.0)));
    let size = vertical.layout_size(LayoutConstraint::UNBOUNDED);
    assert_eq!(size.width, 70.0);
    assert_eq!(size.height, 65.0);
}

#[test]
fn test_grid_edge_cases_do_not_panic_or_underflow() {
    let mut zero_column_grid = Layout::grid(0, 4.0).padding(3.0);
    zero_column_grid.add(Box::new(FixedWidget::new(50.0, 20.0)));
    let size = zero_column_grid.layout_size(LayoutConstraint::UNBOUNDED);
    assert_eq!(size.width, 6.0);
    assert_eq!(size.height, 6.0);
    zero_column_grid.arrange(Rect::new(0.0, 0.0, 100.0, 100.0));

    let mut empty_grid = Layout::grid(3, 4.0).padding(3.0);
    let size = empty_grid.layout_size(LayoutConstraint::UNBOUNDED);
    assert_eq!(size.width, 6.0);
    assert_eq!(size.height, 6.0);
}

#[test]
fn test_scroll_view_forwards_child_events() {
    let handled_events = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut scroll = ScrollView::new();
    scroll.set_content(Box::new(FixedWidget::with_counter(
        50.0,
        20.0,
        handled_events.clone(),
    )));

    let result = scroll.handle_event(&Event::MouseEnter);
    assert!(matches!(result, EventResult::Handled));
    assert_eq!(handled_events.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn test_layout_arrange_reflows_nested_layout_view_children() {
    let mut inner = Layout::vertical(0.0);
    inner.add(Box::new(FixedWidget::new(20.0, 10.0)));

    let mut outer = Layout::vertical(0.0).padding(5.0);
    outer.add(Box::new(LayoutView::new(inner)));

    let _ = outer.layout_size(LayoutConstraint::tight(Size::new(100.0, 100.0)));
    outer.arrange(Rect::new(10.0, 20.0, 100.0, 100.0));

    let Layout::Vertical { children, .. } = &outer else {
        panic!("outer layout remains vertical");
    };
    let nested = children[0]
        .as_any()
        .downcast_ref::<LayoutView>()
        .expect("nested layout view");
    let Layout::Vertical { children, .. } = &nested.layout else {
        panic!("inner layout remains vertical");
    };

    assert_eq!(nested.rect().x, 15.0);
    assert_eq!(nested.rect().y, 25.0);
    assert_eq!(children[0].rect().x, 15.0);
    assert_eq!(children[0].rect().y, 25.0);
}

// ---------------------------------------------------------------------------
// Accessibility tests
// ---------------------------------------------------------------------------

#[test]
fn accessibility_node_new_sets_role_and_label() {
    let node = AccessibilityNode::new(AccessibilityRole::Button, "OK");
    assert_eq!(node.role, AccessibilityRole::Button);
    assert_eq!(node.label, "OK");
    assert!(node.description.is_empty());
    assert_eq!(node.index, 0);
    assert!(node.parent.is_none());
    assert!(node.children.is_empty());
}

#[test]
fn accessibility_role_name_returns_expected_strings() {
    assert_eq!(AccessibilityRole::Window.role_name(), "frame");
    assert_eq!(AccessibilityRole::Button.role_name(), "push button");
    assert_eq!(AccessibilityRole::Checkbox.role_name(), "check box");
    assert_eq!(AccessibilityRole::RadioButton.role_name(), "radio button");
    assert_eq!(AccessibilityRole::TextField.role_name(), "text");
    assert_eq!(AccessibilityRole::Label.role_name(), "label");
    assert_eq!(AccessibilityRole::List.role_name(), "list");
    assert_eq!(AccessibilityRole::ListItem.role_name(), "list item");
    assert_eq!(AccessibilityRole::Menu.role_name(), "menu");
    assert_eq!(AccessibilityRole::MenuItem.role_name(), "menu item");
    assert_eq!(AccessibilityRole::MenuBar.role_name(), "menu bar");
    assert_eq!(AccessibilityRole::Dialog.role_name(), "dialog");
    assert_eq!(AccessibilityRole::Tab.role_name(), "page tab");
    assert_eq!(AccessibilityRole::TabGroup.role_name(), "page tab list");
    assert_eq!(AccessibilityRole::ComboBox.role_name(), "combo box");
    assert_eq!(AccessibilityRole::Notification.role_name(), "alert");
    assert_eq!(AccessibilityRole::Desktop.role_name(), "desktop frame");
    assert_eq!(AccessibilityRole::Unknown.role_name(), "unknown");
}

#[test]
fn accessibility_role_is_focusable_for_interactive_roles() {
    // Interactive (should be focusable)
    assert!(AccessibilityRole::Button.is_focusable());
    assert!(AccessibilityRole::Checkbox.is_focusable());
    assert!(AccessibilityRole::RadioButton.is_focusable());
    assert!(AccessibilityRole::TextField.is_focusable());
    assert!(AccessibilityRole::ListItem.is_focusable());
    assert!(AccessibilityRole::TreeItem.is_focusable());
    assert!(AccessibilityRole::MenuItem.is_focusable());
    assert!(AccessibilityRole::Tab.is_focusable());
    assert!(AccessibilityRole::Slider.is_focusable());
    assert!(AccessibilityRole::ComboBox.is_focusable());
    assert!(AccessibilityRole::Link.is_focusable());

    // Non-interactive (should NOT be focusable)
    assert!(!AccessibilityRole::Window.is_focusable());
    assert!(!AccessibilityRole::Label.is_focusable());
    assert!(!AccessibilityRole::StaticText.is_focusable());
    assert!(!AccessibilityRole::Image.is_focusable());
    assert!(!AccessibilityRole::Group.is_focusable());
    assert!(!AccessibilityRole::Desktop.is_focusable());
    assert!(!AccessibilityRole::Unknown.is_focusable());
}

#[test]
fn accessibility_node_role_name_delegates_to_role() {
    let node = AccessibilityNode::new(AccessibilityRole::Dialog, "Confirm");
    assert_eq!(node.role_name(), "dialog");
}

#[test]
fn accessibility_node_is_focusable_delegates_to_role() {
    let focusable = AccessibilityNode::new(AccessibilityRole::TextField, "Name");
    assert!(focusable.is_focusable());

    let not_focusable = AccessibilityNode::new(AccessibilityRole::Label, "Name:");
    assert!(!not_focusable.is_focusable());
}

#[test]
fn accessibility_tree_add_and_clear() {
    let mut tree = AccessibilityTree::new();
    assert!(tree.to_atspi_objects().is_empty());

    tree.add(AccessibilityNode::new(AccessibilityRole::Button, "Save"));
    tree.add(AccessibilityNode::new(AccessibilityRole::Button, "Cancel"));
    assert_eq!(tree.to_atspi_objects().len(), 2);

    tree.clear();
    assert!(tree.to_atspi_objects().is_empty());
}

#[test]
fn accessibility_tree_to_atspi_objects_format() {
    let mut tree = AccessibilityTree::new();
    tree.add(AccessibilityNode::new(AccessibilityRole::Button, "OK"));
    tree.add(AccessibilityNode::new(
        AccessibilityRole::TextField,
        "Username",
    ));

    let objects = tree.to_atspi_objects();
    assert_eq!(objects[0], "role:push button label:OK");
    assert_eq!(objects[1], "role:text label:Username");
}
