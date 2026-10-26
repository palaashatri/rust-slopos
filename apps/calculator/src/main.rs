//! SLOPOS Calculator: the first native application on the SLOPOS toolkit.

mod engine;

use engine::{Calculator, Input, Operator};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::RasterSurface;
use slopos_ui::text::{measure, Font};

/// Platinum window body. Provisional until the Classic colour tokens are
/// extracted into qa/spec/classic/colors.json.
const WINDOW_BACKGROUND: Color = Color {
    red: 0xdd,
    green: 0xdd,
    blue: 0xdd,
    alpha: 0xff,
};

const MARGIN: i32 = 10;
const KEY_WIDTH: u32 = 41;
const KEY_HEIGHT: u32 = 28;
const GAP: i32 = 5;
const DISPLAY_HEIGHT: u32 = 34;
pub const WIDTH: u32 = 2 * MARGIN as u32 + 4 * KEY_WIDTH + 3 * GAP as u32;
pub const HEIGHT: u32 =
    2 * MARGIN as u32 + DISPLAY_HEIGHT + GAP as u32 + 5 * KEY_HEIGHT + 4 * GAP as u32;

const DISPLAY_NODE: u64 = 1;
const ROOT_NODE: u64 = 0;

/// (label shown, accessible name, input, column, row, columns spanned)
const KEYS: &[(&str, &str, Input, i32, i32, u32)] = &[
    ("C", "Clear", Input::Clear, 0, 0, 1),
    ("±", "Change sign", Input::Negate, 1, 0, 1),
    ("%", "Percent", Input::Percent, 2, 0, 1),
    ("÷", "Divide", Input::Operator(Operator::Divide), 3, 0, 1),
    ("7", "7", Input::Digit(7), 0, 1, 1),
    ("8", "8", Input::Digit(8), 1, 1, 1),
    ("9", "9", Input::Digit(9), 2, 1, 1),
    (
        "×",
        "Multiply",
        Input::Operator(Operator::Multiply),
        3,
        1,
        1,
    ),
    ("4", "4", Input::Digit(4), 0, 2, 1),
    ("5", "5", Input::Digit(5), 1, 2, 1),
    ("6", "6", Input::Digit(6), 2, 2, 1),
    (
        "−",
        "Subtract",
        Input::Operator(Operator::Subtract),
        3,
        2,
        1,
    ),
    ("1", "1", Input::Digit(1), 0, 3, 1),
    ("2", "2", Input::Digit(2), 1, 3, 1),
    ("3", "3", Input::Digit(3), 2, 3, 1),
    ("+", "Add", Input::Operator(Operator::Add), 3, 3, 1),
    ("0", "0", Input::Digit(0), 0, 4, 2),
    (".", "Decimal point", Input::Point, 2, 4, 1),
    ("=", "Equals", Input::Equals, 3, 4, 1),
];

struct Key_ {
    button: Button,
    input: Input,
    accessible_name: &'static str,
}

pub struct CalculatorApp {
    calculator: Calculator,
    keys: Vec<Key_>,
    display: Rect,
    key_font: Font,
    display_font: Font,
    focused_key: Option<usize>,
}

impl CalculatorApp {
    pub fn new() -> Self {
        let keys_top = MARGIN + DISPLAY_HEIGHT as i32 + GAP;
        let keys = KEYS
            .iter()
            .enumerate()
            .map(|(index, (label, name, input, column, row, span))| {
                let bounds = Rect {
                    x: MARGIN + column * (KEY_WIDTH as i32 + GAP),
                    y: keys_top + row * (KEY_HEIGHT as i32 + GAP),
                    width: KEY_WIDTH * span + GAP as u32 * (span - 1),
                    height: KEY_HEIGHT,
                };
                Key_ {
                    button: Button::new(10 + index as u64, label, ButtonPlateVariant::Regular)
                        .with_bounds(bounds),
                    input: *input,
                    accessible_name: name,
                }
            })
            .collect();
        Self {
            calculator: Calculator::default(),
            keys,
            display: Rect {
                x: MARGIN,
                y: MARGIN,
                width: WIDTH - 2 * MARGIN as u32,
                height: DISPLAY_HEIGHT,
            },
            key_font: Font::system(13.0),
            display_font: Font::system(20.0),
            focused_key: None,
        }
    }

    pub fn calculator(&self) -> &Calculator {
        &self.calculator
    }

    /// Give keyboard focus to one key, or to nothing. Reports whether the
    /// visible focus moved.
    fn focus_key(&mut self, index: Option<usize>) -> bool {
        let mut changed = false;
        for (position, key) in self.keys.iter_mut().enumerate() {
            let focused = index == Some(position);
            if key.button.focused != focused {
                key.button.focused = focused;
                changed = true;
            }
        }
        self.focused_key = index;
        changed
    }

    fn move_focus(&mut self, forward: bool) {
        let count = self.keys.len();
        if count == 0 {
            return;
        }
        let next = match self.focused_key {
            Some(current) if forward => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if forward => 0,
            None => count - 1,
        };
        self.focus_key(Some(next));
    }

    fn input_for_key(key: &Key) -> Option<Input> {
        Some(match key {
            Key::Character(c @ '0'..='9') => Input::Digit(*c as u8 - b'0'),
            Key::Character('.' | ',') => Input::Point,
            Key::Character('+') => Input::Operator(Operator::Add),
            Key::Character('-') => Input::Operator(Operator::Subtract),
            Key::Character('*' | 'x' | 'X') => Input::Operator(Operator::Multiply),
            Key::Character('/') => Input::Operator(Operator::Divide),
            Key::Character('%') => Input::Percent,
            Key::Character('=') | Key::Enter => Input::Equals,
            Key::Escape | Key::Delete | Key::Character('c' | 'C') => Input::Clear,
            Key::Backspace => Input::Backspace,
            Key::Function(9) => Input::Negate,
            _ => return None,
        })
    }
}

impl Default for CalculatorApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for CalculatorApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::KeyPressed { key, modifiers } if !modifiers.control && !modifiers.alt => {
                match key {
                    Key::Tab => {
                        self.move_focus(!modifiers.shift);
                        control.request_redraw();
                    }
                    Key::Space | Key::Enter => {
                        if let Some(index) = self.focused_key {
                            let input = self.keys[index].input;
                            self.calculator.press(input);
                            control.request_redraw();
                        } else if matches!(key, Key::Enter) {
                            self.calculator.press(Input::Equals);
                            control.request_redraw();
                        }
                    }
                    _ => {
                        if let Some(input) = Self::input_for_key(key) {
                            self.calculator.press(input);
                            control.request_redraw();
                        }
                    }
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => {
                if let Some(key) = self.keys.iter().find(|key| key.button.id == *id) {
                    self.calculator.press(key.input);
                    control.request_redraw();
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                if let Some(index) = self.keys.iter().position(|key| key.button.id == *id) {
                    if self.focus_key(Some(index)) {
                        control.request_redraw();
                    }
                }
            }
            _ => {
                for key in &mut self.keys {
                    match key.button.handle(event) {
                        ButtonOutcome::Activated => {
                            self.calculator.press(key.input);
                            control.request_redraw();
                        }
                        ButtonOutcome::Changed => control.request_redraw(),
                        ButtonOutcome::Ignored => {}
                    }
                }
            }
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let _ = surface.clear(WINDOW_BACKGROUND);
        let _ = surface.fill_rect(self.display, Color::WHITE);
        let _ = surface.stroke_rect(self.display, Color::BLACK);
        let text = self.calculator.display();
        let extent = measure(text, &self.display_font);
        let x = self.display.x + self.display.width as i32 - 8 - extent.width as i32;
        let y = self.display.y + (self.display.height as i32 - extent.height as i32) / 2;
        let _ = surface.draw_text(text, x, y, &self.display_font, Color::BLACK);
        for key in &self.keys {
            let _ = key.button.paint(surface, &self.key_font);
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
        let mut root = Node::new(Role::Window);
        root.set_label("Calculator");
        let mut display = Node::new(Role::Label);
        display.set_label(format!("Result {}", self.calculator.display()));
        let mut nodes = vec![(NodeId(DISPLAY_NODE), display)];
        let mut children = vec![NodeId(DISPLAY_NODE)];
        for key in &self.keys {
            let mut node = key.button.accessibility_node();
            node.set_label(key.accessible_name);
            nodes.push((NodeId(key.button.id), node));
            children.push(NodeId(key.button.id));
        }
        root.set_children(children);
        nodes.insert(0, (NodeId(ROOT_NODE), root));
        let focus = self
            .focused_key
            .map(|index| NodeId(self.keys[index].button.id))
            .unwrap_or(NodeId(ROOT_NODE));
        Some(TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(ROOT_NODE))),
            tree_id: TreeId::ROOT,
            focus,
        })
    }
}

fn main() {
    env_logger_init();
    if let Some(path) = snapshot_path() {
        // `--snapshot FILE`: render one frame headless (for QA and docs).
        let scale = platform::scale_from_environment();
        let mut app = CalculatorApp::new();
        let surface = RasterSurface::new(WIDTH, HEIGHT, scale).expect("surface");
        app.paint(&surface);
        surface.save_png(&path).expect("write snapshot");
        return;
    }
    let mut options = WindowOptions::new("Calculator", "slopos-calculator", WIDTH, HEIGHT);
    options.resizable = false;
    if let Err(error) = platform::run(options, CalculatorApp::new(), |_| {}) {
        eprintln!("slopos-calculator: {error}");
        std::process::exit(1);
    }
}

fn snapshot_path() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--snapshot" {
            return args.next().map(Into::into);
        }
    }
    None
}

fn env_logger_init() {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slopos_ui::event::PointerButton;
    use slopos_ui::foundation::geometry::Scale;

    fn click(app: &mut CalculatorApp, label: &str) {
        let bounds = app
            .keys
            .iter()
            .find(|key| key.button.label == label)
            .map(|key| key.button.bounds)
            .unwrap();
        let (x, y) = (bounds.x + 5, bounds.y + 5);
        let mut control = Control::default();
        for event in [
            Event::PointerMoved { x, y },
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            },
        ] {
            app.event(&event, &mut control);
        }
    }

    #[test]
    fn clicking_keys_computes_results() {
        let mut app = CalculatorApp::new();
        for label in ["1", "2", "×", "3", "="] {
            click(&mut app, label);
        }
        assert_eq!(app.calculator().display(), "36");
    }

    #[test]
    fn typing_on_the_keyboard_works_like_clicking() {
        let mut app = CalculatorApp::new();
        let mut control = Control::default();
        for c in ['7', '/', '2', '='] {
            app.event(
                &Event::KeyPressed {
                    key: Key::Character(c),
                    modifiers: Default::default(),
                },
                &mut control,
            );
        }
        assert_eq!(app.calculator().display(), "3.5");
    }

    #[test]
    fn screen_readers_can_press_keys_and_read_the_result() {
        let mut app = CalculatorApp::new();
        let seven = app
            .keys
            .iter()
            .find(|key| key.button.label == "7")
            .unwrap()
            .button
            .id;
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(seven)),
            &mut Control::default(),
        );
        let tree = app.accessibility().unwrap();
        let labels: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect();
        assert!(labels.contains(&"Result 7"));
        assert!(labels.contains(&"Divide"));
        assert_eq!(tree.nodes.len(), 2 + KEYS.len());
    }

    #[test]
    fn keyboard_focus_moves_and_activates_the_selected_key() {
        let mut app = CalculatorApp::new();
        let mut control = Control::default();
        let press = |key| Event::KeyPressed {
            key,
            modifiers: Default::default(),
        };
        app.event(&press(Key::Tab), &mut control);
        assert!(app.keys[0].button.focused);
        // Five tabs from no focus reach "7": Clear, ±, %, ÷, 7.
        for _ in 0..4 {
            app.event(&press(Key::Tab), &mut control);
        }
        assert_eq!(app.focused_key, Some(4));
        app.event(&press(Key::Space), &mut control);
        assert_eq!(app.calculator().display(), "7");
        // F9 is the keyboard shortcut for Change Sign.
        app.event(&press(Key::Function(9)), &mut control);
        assert_eq!(app.calculator().display(), "-7");
        app.event(
            &Event::KeyPressed {
                key: Key::Tab,
                modifiers: slopos_ui::event::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
            &mut control,
        );
        assert_eq!(app.focused_key, Some(3));
        assert!(app.keys[3].button.focused);
    }

    #[test]
    fn accessibility_focus_selects_a_key() {
        let mut app = CalculatorApp::new();
        let seven = app
            .keys
            .iter()
            .find(|key| key.button.label == "7")
            .unwrap()
            .button
            .id;
        let mut control = Control::default();
        app.event(
            &Event::Accessibility(AccessibilityRequest::Focus(seven)),
            &mut control,
        );
        assert!(app
            .keys
            .iter()
            .any(|key| key.button.id == seven && key.button.focused));
        let tree = app.accessibility().unwrap();
        assert_eq!(tree.focus.0, seven);
        app.event(
            &Event::KeyPressed {
                key: Key::Enter,
                modifiers: Default::default(),
            },
            &mut control,
        );
        assert_eq!(app.calculator().display(), "7");
    }

    #[test]
    fn keys_fit_inside_the_window() {
        let app = CalculatorApp::new();
        for key in &app.keys {
            let right = key.button.bounds.x + key.button.bounds.width as i32;
            let bottom = key.button.bounds.y + key.button.bounds.height as i32;
            assert!(
                right <= WIDTH as i32 - MARGIN,
                "{} overflows",
                key.button.label
            );
            assert!(bottom <= HEIGHT as i32 - MARGIN + 1);
        }
    }

    #[test]
    fn snapshot_renders_display_and_keys() {
        let mut app = CalculatorApp::new();
        let surface = RasterSurface::new(WIDTH, HEIGHT, Scale::new(1).unwrap()).unwrap();
        app.paint(&surface);
        // Display border and a key plate border are black.
        assert_eq!(
            surface.pixel(MARGIN as u32, MARGIN as u32 + 5),
            Some(0xff000000)
        );
        let first_key = &app.keys[0].button.bounds;
        assert_eq!(
            surface.pixel(first_key.x as u32, (first_key.y + 10) as u32),
            Some(0xff000000)
        );
    }
}
