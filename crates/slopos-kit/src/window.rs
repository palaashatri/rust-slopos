use crate::{
    design_tokens::{ClassicMetrics, CLASSIC_METRICS, WINDOW_TITLE_BAR_HEIGHT},
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, Event, EventResult, Layout, LayoutConstraint, Point,
    Rect, Size, Widget, WidgetState,
};

/// The semantic target under a local pointer position in native window chrome.
///
/// This is intentionally a classification only.  A compositor or SDK owns
/// the resulting close, zoom, move, or resize policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowChromeHit {
    Content,
    Titlebar,
    Close,
    Zoom,
    ResizeSouthEast,
}

/// Classify a local window coordinate using the canonical Classic metrics.
pub fn hit_test_window_chrome(point: Point, size: Size) -> WindowChromeHit {
    hit_test_window_chrome_with_metrics(point, size, CLASSIC_METRICS)
}

/// Classify a local window coordinate using caller-supplied metrics.
///
/// Coordinates outside the window, borders, and any non-control area are
/// reported as [`WindowChromeHit::Content`].  The south-east grow box wins
/// first when regions overlap, matching classic direct-manipulation chrome.
pub fn hit_test_window_chrome_with_metrics(
    point: Point,
    size: Size,
    metrics: ClassicMetrics,
) -> WindowChromeHit {
    let width = size.width.max(0.0);
    let height = size.height.max(0.0);
    let border = metrics.window_border_width.max(0.0);
    let control_size = metrics.window_control_size.max(0.0);
    let control_top = metrics.window_control_top.max(0.0);
    let control_inset = metrics.window_control_inset.max(0.0);

    let grip_size = metrics
        .window_resize_grip_size
        .max(0.0)
        .min(width)
        .min(height);
    if contains_half_open(
        point,
        width - grip_size,
        height - grip_size,
        grip_size,
        grip_size,
    ) {
        return WindowChromeHit::ResizeSouthEast;
    }

    if contains_half_open(
        point,
        control_inset,
        control_top,
        control_size,
        control_size,
    ) {
        return WindowChromeHit::Close;
    }

    let zoom_left = (width - control_inset - control_size).max(0.0);
    if contains_half_open(point, zoom_left, control_top, control_size, control_size) {
        return WindowChromeHit::Zoom;
    }

    let titlebar_width = (width - 2.0 * border).max(0.0);
    let titlebar_height = metrics.window_title_bar_height.max(0.0);
    if contains_half_open(point, border, border, titlebar_width, titlebar_height) {
        return WindowChromeHit::Titlebar;
    }

    WindowChromeHit::Content
}

fn contains_half_open(point: Point, x: f32, y: f32, width: f32, height: f32) -> bool {
    width > 0.0
        && height > 0.0
        && point.x >= x
        && point.x < x + width
        && point.y >= y
        && point.y < y + height
}

pub struct Window {
    state: WidgetState,
    pub title: String,
    pub content: Option<Box<dyn Widget>>,
    pub layout: Layout,
    pub is_dark: bool,
    pub has_toolbar: bool,
    pub is_active: bool,
}

impl Window {
    pub fn new<S: Into<String>>(title: S) -> Self {
        Self {
            state: WidgetState::new(),
            title: title.into(),
            content: None,
            layout: Layout::vertical(0.0),
            is_dark: false,
            has_toolbar: false,
            is_active: true,
        }
    }

    pub fn set_content(&mut self, widget: Box<dyn Widget>) {
        self.content = Some(widget);
    }

    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_title<S: Into<String>>(&mut self, title: S) {
        self.title = title.into();
    }

    /// Hit-test a local point against this window's canonical chrome.
    pub fn hit_test_chrome(&self, point: Point) -> WindowChromeHit {
        hit_test_window_chrome(point, Size::new(self.rect().width, self.rect().height))
    }
}

impl Widget for Window {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        if self.content.is_some() {
            let proposed = constraint.clamp(Size::new(constraint.max_width, constraint.max_height));
            self.set_rect(Rect::new(
                self.rect().x,
                self.rect().y,
                proposed.width,
                proposed.height,
            ));
            let rect = self.rect();
            if let Some(content) = &mut self.content {
                let content_rect = if self.title == "SLOPOS-I Desktop" {
                    rect
                } else {
                    Rect::new(
                        rect.x + 1.0,
                        rect.y + WINDOW_TITLE_BAR_HEIGHT + 5.0,
                        (rect.width - 2.0).max(0.0),
                        (rect.height - 26.0).max(0.0),
                    )
                };
                content.set_rect(content_rect);
                content.layout(LayoutConstraint::tight(Size::new(
                    content_rect.width,
                    content_rect.height,
                )))
            } else {
                proposed
            }
        } else {
            let size = self.layout.layout_size(constraint);
            self.set_rect(Rect::new(
                self.rect().x,
                self.rect().y,
                size.width,
                size.height,
            ));
            self.layout.arrange(self.rect());
            size
        }
    }

    fn draw(&self, theme: &ThemeContext) {
        let _bg = theme.color(crate::ThemeToken::WindowBackground);
        let _border = theme.color(crate::ThemeToken::WindowBorder);
        if let Some(content) = &self.content {
            content.draw(theme);
        } else {
            self.layout.draw(theme);
        }
    }

    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::FocusIn => {
                self.is_active = true;
                EventResult::Handled
            }
            Event::FocusOut => {
                self.is_active = false;
                EventResult::Handled
            }
            // Positional events: a window is an opaque surface. Outside its
            // rect it is not a target at all. Inside, the *content* owns
            // routing — an app root sitting in an SDK window runs its own
            // whole event pipeline (the shell's WM policy + dispatcher), so
            // the window must delegate via `handle_event`, never walk the
            // content's subtree itself. Whatever the content ignores, the
            // window swallows — a click on a window's empty area must never
            // fall through to whatever is stacked underneath (the shell's
            // old click-through-to-desktop bug).
            Event::MouseDown { point, .. }
            | Event::MouseUp { point, .. }
            | Event::MouseMove { point, .. }
            | Event::DoubleClick { point, .. } => {
                if !self.rect().contains(*point) {
                    return EventResult::Ignored;
                }
                let result = if let Some(content) = &mut self.content {
                    content.handle_event(event)
                } else {
                    self.layout.handle_event(event)
                };
                match result {
                    EventResult::Ignored => EventResult::Handled,
                    other => other,
                }
            }
            _ => {
                if let Some(content) = &mut self.content {
                    content.handle_event(event)
                } else {
                    self.layout.handle_event(event)
                }
            }
        }
    }

    fn update(&mut self) {
        if let Some(content) = &mut self.content {
            content.update();
        } else {
            self.layout.update();
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(AccessibilityNode::new(
            AccessibilityRole::Window,
            &self.title,
        ))
    }

    fn children(&self) -> Vec<&dyn Widget> {
        match &self.content {
            Some(c) => vec![c.as_ref()],
            None => self.layout.children().iter().map(|c| c.as_ref()).collect(),
        }
    }

    fn children_mut(&mut self) -> Vec<&mut dyn Widget> {
        match &mut self.content {
            Some(c) => vec![c.as_mut()],
            None => self
                .layout
                .children_mut()
                .iter_mut()
                .map(|c| &mut **c as &mut dyn Widget)
                .collect(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{hit_test_window_chrome, hit_test_window_chrome_with_metrics, WindowChromeHit};
    use crate::{design_tokens::CLASSIC_METRICS, Point, Size};

    #[test]
    fn classic_chrome_hit_test_prioritizes_controls_and_grow_box() {
        let size = Size::new(200.0, 100.0);

        assert_eq!(
            hit_test_window_chrome(Point::new(17.0, 11.0), size),
            WindowChromeHit::Close
        );
        assert_eq!(
            hit_test_window_chrome(Point::new(182.0, 11.0), size),
            WindowChromeHit::Zoom
        );
        assert_eq!(
            hit_test_window_chrome(Point::new(80.0, 11.0), size),
            WindowChromeHit::Titlebar
        );
        assert_eq!(
            hit_test_window_chrome(Point::new(80.0, 40.0), size),
            WindowChromeHit::Content
        );
        assert_eq!(
            hit_test_window_chrome(Point::new(190.0, 90.0), size),
            WindowChromeHit::ResizeSouthEast
        );
    }

    #[test]
    fn chrome_hit_test_uses_half_open_edges_and_custom_metrics() {
        let size = Size::new(200.0, 100.0);
        let mut metrics = CLASSIC_METRICS;
        metrics.window_control_inset = 20.0;
        metrics.window_resize_grip_size = 10.0;

        assert_eq!(
            hit_test_window_chrome_with_metrics(Point::new(25.0, 11.0), size, metrics),
            WindowChromeHit::Close
        );
        assert_eq!(
            hit_test_window_chrome_with_metrics(Point::new(33.0, 11.0), size, metrics),
            WindowChromeHit::Titlebar
        );
        assert_eq!(
            hit_test_window_chrome_with_metrics(Point::new(195.0, 95.0), size, metrics),
            WindowChromeHit::ResizeSouthEast
        );
        assert_eq!(
            hit_test_window_chrome_with_metrics(Point::new(189.0, 79.0), size, metrics),
            WindowChromeHit::Content
        );
    }
}
