//! Single-line text field with Unicode editing, selection and clipboard.
//!
//! The field is a state machine like [`super::button::Button`]. Feed it
//! events with [`TextField::handle`]. It never talks to the clipboard itself:
//! copy and cut return the text for the app to pass to
//! [`crate::platform::Control::set_clipboard`], and paste returns
//! [`TextFieldOutcome::PasteRequested`] so the app can call
//! [`crate::platform::Control::request_paste`] and later hand the
//! [`crate::event::Event::Paste`] text to [`TextField::insert`].
//!
//! Positions are byte offsets that always sit on char boundaries. The caret
//! does not blink: a steady caret needs no timer, which keeps idle windows at
//! zero wakeups and respects reduced motion.

use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{caret_stops, hit_test, measure, Font};

const BULLET: &str = "•";
use crate::theme::palette;

/// Horizontal padding between the border and the text.
const PADDING: i32 = 4;
/// How many edits Ctrl+Z can undo.
const UNDO_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextFieldOutcome {
    /// The event did not concern this field.
    Ignored,
    /// Caret, selection or focus changed; repaint.
    Changed,
    /// The text changed.
    Edited,
    /// The user pressed Enter.
    Submitted,
    /// The user pressed Escape.
    Cancelled,
    /// A pointer press landed in the field; give it keyboard focus.
    FocusRequested,
    /// Put this text on the clipboard (copy; for cut the text was also
    /// removed and the field reports the edit through [`TextField::text`]).
    Copy(String),
    /// Ctrl+V: request clipboard text and pass it to [`TextField::insert`].
    PasteRequested,
}

#[derive(Debug, Clone)]
pub struct TextField {
    pub id: u64,
    pub bounds: Rect,
    pub enabled: bool,
    pub focused: bool,
    /// Accessible name, e.g. "File name".
    pub label: String,
    /// Shown greyed out while the field is empty.
    pub placeholder: String,
    /// Maximum number of chars; longer input is truncated.
    pub max_chars: usize,
    /// A password: drawn as bullets, never copied or cut to the clipboard,
    /// and its value is not exposed to assistive technology.
    pub secret: bool,
    pub font: Font,
    text: String,
    caret: usize,
    anchor: Option<usize>,
    scroll: f32,
    dragging: bool,
    undo: Vec<(String, usize)>,
}

impl TextField {
    pub fn new(id: u64, label: &str) -> Self {
        Self {
            id,
            bounds: Rect {
                x: 0,
                y: 0,
                width: 160,
                height: 22,
            },
            enabled: true,
            focused: false,
            label: label.to_owned(),
            placeholder: String::new(),
            max_chars: 4096,
            secret: false,
            font: Font::system(12.0),
            text: String::new(),
            caret: 0,
            anchor: None,
            scroll: 0.0,
            dragging: false,
            undo: Vec::new(),
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replace the whole text (not undoable); the caret moves to the end.
    pub fn set_text(&mut self, text: &str) {
        self.text = sanitize(text, self.max_chars);
        self.caret = self.text.len();
        self.anchor = None;
        self.undo.clear();
        self.scroll = 0.0;
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected byte range, if any.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        (anchor != self.caret).then(|| (anchor.min(self.caret), anchor.max(self.caret)))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.text[start..end])
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    /// Select a byte range (clamped to char boundaries).
    pub fn select(&mut self, start: usize, end: usize) {
        self.anchor = Some(self.floor(start));
        self.caret = self.floor(end);
    }

    /// Insert text at the caret, replacing the selection. Newlines and other
    /// control characters become spaces or are dropped.
    pub fn insert(&mut self, inserted: &str) -> TextFieldOutcome {
        if !self.enabled {
            return TextFieldOutcome::Ignored;
        }
        self.remember();
        self.delete_selection();
        let room = self.max_chars.saturating_sub(self.text.chars().count());
        let clean = sanitize(inserted, room);
        self.text.insert_str(self.caret, &clean);
        self.caret += clean.len();
        TextFieldOutcome::Edited
    }

    pub fn handle(&mut self, event: &Event) -> TextFieldOutcome {
        if !self.enabled {
            return TextFieldOutcome::Ignored;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                let index = self.index_at(*x);
                self.caret = index;
                self.anchor = Some(index);
                self.dragging = true;
                if self.focused {
                    TextFieldOutcome::Changed
                } else {
                    TextFieldOutcome::FocusRequested
                }
            }
            Event::PointerMoved { x, .. } if self.dragging => {
                let index = self.index_at(*x);
                if index != self.caret {
                    self.caret = index;
                    return TextFieldOutcome::Changed;
                }
                TextFieldOutcome::Ignored
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } if self.dragging => {
                self.dragging = false;
                TextFieldOutcome::Ignored
            }
            Event::KeyPressed { key, modifiers } if self.focused => self.key(key, *modifiers),
            Event::Paste(text) if self.focused => self.insert(text),
            _ => TextFieldOutcome::Ignored,
        }
    }

    fn key(&mut self, key: &Key, modifiers: Modifiers) -> TextFieldOutcome {
        let shift = modifiers.shift;
        let word = modifiers.control;
        match key {
            Key::Character(c) if modifiers.control && !modifiers.alt => {
                match c.to_ascii_lowercase() {
                    'a' => {
                        self.select_all();
                        TextFieldOutcome::Changed
                    }
                    'c' | 'x' if self.secret => TextFieldOutcome::Ignored,
                    'c' => match self.selected_text() {
                        Some(text) => TextFieldOutcome::Copy(text.to_owned()),
                        None => TextFieldOutcome::Ignored,
                    },
                    'x' => match self.selected_text().map(str::to_owned) {
                        Some(text) => {
                            self.remember();
                            self.delete_selection();
                            TextFieldOutcome::Copy(text)
                        }
                        None => TextFieldOutcome::Ignored,
                    },
                    'v' => TextFieldOutcome::PasteRequested,
                    'z' => match self.undo.pop() {
                        Some((text, caret)) => {
                            self.text = text;
                            self.caret = caret;
                            self.anchor = None;
                            TextFieldOutcome::Edited
                        }
                        None => TextFieldOutcome::Ignored,
                    },
                    _ => TextFieldOutcome::Ignored,
                }
            }
            Key::Character(c) if !modifiers.control && !modifiers.alt && !modifiers.logo => {
                self.insert(c.encode_utf8(&mut [0; 4]))
            }
            Key::Space if !modifiers.control && !modifiers.alt => self.insert(" "),
            Key::Backspace | Key::Delete => {
                if self.selection().is_none() {
                    let forward = *key == Key::Delete;
                    let target = match (forward, word) {
                        (false, false) => self.previous_char(self.caret),
                        (false, true) => self.previous_word(self.caret),
                        (true, false) => self.next_char(self.caret),
                        (true, true) => self.next_word(self.caret),
                    };
                    if target == self.caret {
                        return TextFieldOutcome::Ignored;
                    }
                    self.anchor = Some(target);
                }
                self.remember();
                self.delete_selection();
                TextFieldOutcome::Edited
            }
            Key::Left | Key::Right | Key::Home | Key::End => {
                let target = match (key, word) {
                    (Key::Left, false) => match self.selection() {
                        Some((start, _)) if !shift => start,
                        _ => self.previous_char(self.caret),
                    },
                    (Key::Right, false) => match self.selection() {
                        Some((_, end)) if !shift => end,
                        _ => self.next_char(self.caret),
                    },
                    (Key::Left, true) => self.previous_word(self.caret),
                    (Key::Right, true) => self.next_word(self.caret),
                    (Key::Home, _) => 0,
                    _ => self.text.len(),
                };
                self.move_caret(target, shift);
                TextFieldOutcome::Changed
            }
            Key::Enter => TextFieldOutcome::Submitted,
            Key::Escape => TextFieldOutcome::Cancelled,
            _ => TextFieldOutcome::Ignored,
        }
    }

    fn move_caret(&mut self, target: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = target;
    }

    fn remember(&mut self) {
        if self.undo.len() == UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.undo.push((self.text.clone(), self.caret));
    }

    fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection() {
            self.text.replace_range(start..end, "");
            self.caret = start;
        }
        self.anchor = None;
    }

    fn floor(&self, index: usize) -> usize {
        let mut index = index.min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn previous_char(&self, from: usize) -> usize {
        self.text[..from]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_char(&self, from: usize) -> usize {
        self.text[from..]
            .chars()
            .next()
            .map_or(from, |c| from + c.len_utf8())
    }

    fn previous_word(&self, from: usize) -> usize {
        let before = &self.text[..from];
        let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric());
        trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_alphanumeric())
            .map_or(0, |(index, c)| index + c.len_utf8())
    }

    fn next_word(&self, from: usize) -> usize {
        let after = &self.text[from..];
        let skipped = after.len()
            - after
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .len();
        let rest = &after[skipped..];
        let word = rest
            .char_indices()
            .find(|(_, c)| !c.is_alphanumeric())
            .map_or(rest.len(), |(index, _)| index);
        from + skipped + word
    }

    fn text_left(&self) -> f32 {
        (self.bounds.x + PADDING) as f32 - self.scroll
    }

    fn index_at(&self, x: i32) -> usize {
        if self.secret {
            let stops = self.stops();
            let target = x as f32 - self.text_left();
            return stops
                .iter()
                .min_by(|a, b| (a.1 - target).abs().total_cmp(&(b.1 - target).abs()))
                .map_or(0, |stop| stop.0);
        }
        hit_test(&self.text, &self.font, x as f32 - self.text_left())
    }

    /// Caret positions: shaped text, or one bullet width per char.
    fn stops(&self) -> Vec<(usize, f32)> {
        if !self.secret {
            return caret_stops(&self.text, &self.font);
        }
        let bullet = measure(BULLET, &self.font).width;
        let mut stops: Vec<(usize, f32)> = self
            .text
            .char_indices()
            .enumerate()
            .map(|(n, (index, _))| (index, n as f32 * bullet))
            .collect();
        stops.push((self.text.len(), self.text.chars().count() as f32 * bullet));
        stops
    }

    /// What is drawn: the text, or a bullet per char.
    fn shown(&self) -> String {
        if self.secret {
            BULLET.repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    /// Keep the caret visible by scrolling the text horizontally.
    fn scroll_to_caret(&mut self, stops: &[(usize, f32)]) {
        let inner = (self.bounds.width as f32 - 2.0 * PADDING as f32).max(1.0);
        let caret_x = stops
            .iter()
            .find(|stop| stop.0 == self.caret)
            .map_or(0.0, |stop| stop.1);
        if caret_x - self.scroll > inner {
            self.scroll = caret_x - inner;
        } else if caret_x < self.scroll {
            self.scroll = caret_x;
        }
        let end = stops.last().map_or(0.0, |stop| stop.1);
        self.scroll = self.scroll.clamp(0.0, (end - inner).max(0.0));
    }

    /// Paint the field. Takes `&mut self` only to keep the caret scrolled
    /// into view.
    pub fn paint(&mut self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let bounds = self.bounds;
        surface.fill_rect(bounds, colors.content)?;
        surface.stroke_rect(
            bounds,
            if self.enabled {
                colors.bevel_shadow
            } else {
                colors.ink_disabled
            },
        )?;
        if self.focused && self.enabled {
            // Classic keyboard focus: a second, darker inner border.
            surface.stroke_rect(inset(bounds, 1), colors.focus)?;
        }
        let stops = self.stops();
        self.scroll_to_caret(&stops);
        let line_height = measure("Ag", &self.font).height as i32;
        let text_y = bounds.y + (bounds.height as i32 - line_height) / 2;
        let left = self.text_left();
        let x_of = |index: usize| {
            stops
                .iter()
                .find(|stop| stop.0 == index)
                .map_or(0.0, |stop| stop.1)
        };
        surface.with_clip(inset(bounds, 2), || {
            if let Some((start, end)) = self.selection() {
                let x0 = (left + x_of(start)).floor() as i32;
                let x1 = (left + x_of(end)).ceil() as i32;
                surface.fill_rect(
                    Rect {
                        x: x0,
                        y: text_y,
                        width: (x1 - x0).max(1) as u32,
                        height: line_height as u32,
                    },
                    if self.focused {
                        colors.selection
                    } else {
                        colors.selection_inactive
                    },
                )?;
            }
            if self.text.is_empty() && !self.placeholder.is_empty() {
                surface.draw_text(
                    &self.placeholder,
                    left as i32,
                    text_y,
                    &self.font,
                    colors.ink_secondary,
                )?;
            } else {
                let ink = if self.enabled {
                    colors.ink
                } else {
                    colors.ink_disabled
                };
                surface.draw_text(&self.shown(), left.round() as i32, text_y, &self.font, ink)?;
            }
            if self.focused && self.enabled && self.selection().is_none() {
                let x = (left + x_of(self.caret)).round() as i32;
                surface.fill_rect(
                    Rect {
                        x,
                        y: text_y,
                        width: 1,
                        height: line_height as u32,
                    },
                    colors.ink,
                )?;
            }
            Ok(())
        })
    }

    pub fn accessibility_node(&self) -> accesskit::Node {
        let mut node = accesskit::Node::new(if self.secret {
            accesskit::Role::PasswordInput
        } else {
            accesskit::Role::TextInput
        });
        node.set_label(self.label.as_str());
        if !self.secret {
            node.set_value(self.text.as_str());
        }
        if !self.placeholder.is_empty() {
            node.set_placeholder(self.placeholder.as_str());
        }
        node.set_bounds(super::bounds_of(self.bounds));
        if self.enabled {
            node.add_action(accesskit::Action::Focus);
            node.add_action(accesskit::Action::SetValue);
        } else {
            node.set_disabled();
        }
        node
    }
}

fn inset(rect: Rect, by: u32) -> Rect {
    Rect {
        x: rect.x + by as i32,
        y: rect.y + by as i32,
        width: rect.width.saturating_sub(2 * by),
        height: rect.height.saturating_sub(2 * by),
    }
}

/// Make text safe for a single line: newlines and tabs become spaces, other
/// control characters are dropped, and at most `max_chars` chars are kept.
fn sanitize(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn with(key: Key, shift: bool, control: bool) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers {
                shift,
                control,
                ..Modifiers::default()
            },
        }
    }

    fn typed(field: &mut TextField, text: &str) {
        for c in text.chars() {
            let event = if c == ' ' {
                key(Key::Space)
            } else {
                key(Key::Character(c))
            };
            assert_eq!(field.handle(&event), TextFieldOutcome::Edited);
        }
    }

    fn focused() -> TextField {
        let mut field = TextField::new(7, "Name").with_bounds(Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 22,
        });
        field.focused = true;
        field
    }

    #[test]
    fn typing_unicode_and_editing_keeps_char_boundaries() {
        let mut field = focused();
        typed(&mut field, "héllo wörld");
        assert_eq!(field.text(), "héllo wörld");
        field.handle(&key(Key::Left));
        field.handle(&key(Key::Backspace));
        assert_eq!(field.text(), "héllo wörd");
        field.handle(&key(Key::Home));
        field.handle(&key(Key::Right));
        field.handle(&key(Key::Delete));
        assert_eq!(field.text(), "hllo wörd");
        assert!(field.text().is_char_boundary(field.caret()));
        field.handle(&key(Key::End));
        field.handle(&with(Key::Backspace, false, true));
        assert_eq!(field.text(), "hllo ");
    }

    #[test]
    fn shift_selection_replace_and_word_motion() {
        let mut field = focused();
        typed(&mut field, "one two three");
        field.handle(&with(Key::Left, true, true));
        assert_eq!(field.selected_text(), Some("three"));
        typed(&mut field, "3");
        assert_eq!(field.text(), "one two 3");
        field.handle(&key(Key::Home));
        field.handle(&with(Key::Right, false, true));
        assert_eq!(field.caret(), 3);
        field.handle(&with(Key::Right, false, true));
        assert_eq!(field.caret(), 7);
    }

    #[test]
    fn clipboard_copy_cut_paste_and_undo() {
        let mut field = focused();
        typed(&mut field, "abc def");
        field.handle(&with(Key::Character('a'), false, true));
        assert_eq!(
            field.handle(&with(Key::Character('c'), false, true)),
            TextFieldOutcome::Copy("abc def".into())
        );
        field.handle(&key(Key::End));
        field.handle(&with(Key::Left, true, true));
        assert_eq!(
            field.handle(&with(Key::Character('x'), false, true)),
            TextFieldOutcome::Copy("def".into())
        );
        assert_eq!(field.text(), "abc ");
        assert_eq!(
            field.handle(&with(Key::Character('v'), false, true)),
            TextFieldOutcome::PasteRequested
        );
        assert_eq!(
            field.handle(&Event::Paste("x\ny\u{7}z".into())),
            TextFieldOutcome::Edited
        );
        assert_eq!(field.text(), "abc x yz");
        field.handle(&with(Key::Character('z'), false, true));
        assert_eq!(field.text(), "abc ");
        field.handle(&with(Key::Character('z'), false, true));
        assert_eq!(field.text(), "abc def");
        // Nothing selected: copy does nothing.
        field.handle(&key(Key::End));
        assert_eq!(
            field.handle(&with(Key::Character('c'), false, true)),
            TextFieldOutcome::Ignored
        );
    }

    #[test]
    fn unfocused_and_disabled_fields_ignore_keys_and_max_chars_holds() {
        let mut field = focused();
        field.focused = false;
        assert_eq!(
            field.handle(&key(Key::Character('a'))),
            TextFieldOutcome::Ignored
        );
        field.focused = true;
        field.max_chars = 3;
        typed(&mut field, "abc");
        field.handle(&key(Key::Character('d')));
        assert_eq!(field.text(), "abc");
        field.enabled = false;
        assert_eq!(
            field.handle(&key(Key::Backspace)),
            TextFieldOutcome::Ignored
        );
        assert_eq!(field.handle(&key(Key::Enter)), TextFieldOutcome::Ignored);
    }

    #[test]
    fn enter_escape_and_pointer_focus() {
        let mut field = focused();
        assert_eq!(field.handle(&key(Key::Enter)), TextFieldOutcome::Submitted);
        assert_eq!(field.handle(&key(Key::Escape)), TextFieldOutcome::Cancelled);
        field.focused = false;
        field.set_text("hello");
        let press = Event::PointerPressed {
            x: 12,
            y: 15,
            button: PointerButton::Primary,
        };
        assert_eq!(field.handle(&press), TextFieldOutcome::FocusRequested);
        assert_eq!(field.caret(), 0);
        field.focused = true;
        field.handle(&Event::PointerMoved { x: 500, y: 15 });
        assert_eq!(field.selected_text(), Some("hello"));
        let outside = Event::PointerPressed {
            x: 0,
            y: 0,
            button: PointerButton::Primary,
        };
        field.handle(&Event::PointerReleased {
            x: 500,
            y: 15,
            button: PointerButton::Primary,
        });
        assert_eq!(field.handle(&outside), TextFieldOutcome::Ignored);
    }

    #[test]
    fn long_text_scrolls_and_paint_stays_inside_the_field() {
        for factor in [1, 2] {
            let surface = RasterSurface::new(240, 40, Scale::new(factor).unwrap()).unwrap();
            surface.clear(palette().surface).unwrap();
            let mut field = focused();
            typed(&mut field, &"wide text ".repeat(20));
            field.paint(&surface).unwrap();
            assert!(field.scroll > 0.0);
            let f = factor;
            // Right of the field: untouched surface colour.
            assert_eq!(surface.pixel(220 * f, 20 * f), Some(0xffdddddd));
            // Focus border is drawn.
            assert_eq!(surface.pixel(11 * f, 20 * f), Some(0xff000000));
            // Empty field with a placeholder paints secondary ink only.
            let mut empty = focused();
            empty.placeholder = "Search".into();
            empty.paint(&surface).unwrap();
        }
    }

    #[test]
    fn accessibility_exposes_value_and_actions() {
        let mut field = focused();
        field.set_text("report.txt");
        let node = field.accessibility_node();
        assert_eq!(node.role(), accesskit::Role::TextInput);
        assert_eq!(node.value(), Some("report.txt"));
        assert!(node.supports_action(accesskit::Action::SetValue));
    }

    #[test]
    fn secret_fields_mask_and_never_copy() {
        use crate::foundation::geometry::Scale;
        let mut field = TextField::new(9, "Password");
        field.secret = true;
        field.focused = true;
        field.set_text("hunter2");
        field.select_all();
        let ctrl = |c| Event::KeyPressed {
            key: Key::Character(c),
            modifiers: crate::event::Modifiers {
                control: true,
                ..Default::default()
            },
        };
        assert_eq!(field.handle(&ctrl('c')), TextFieldOutcome::Ignored);
        assert_eq!(field.handle(&ctrl('x')), TextFieldOutcome::Ignored);
        assert_eq!(field.text(), "hunter2");
        let node = field.accessibility_node();
        assert_eq!(node.role(), accesskit::Role::PasswordInput);
        assert_eq!(node.value(), None);
        assert_eq!(field.shown(), "•••••••");
        assert_eq!(field.stops().len(), 8);
        let surface = RasterSurface::new(200, 30, Scale::new(1).unwrap()).unwrap();
        field.bounds = Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 22,
        };
        field.paint(&surface).unwrap();
    }
}
