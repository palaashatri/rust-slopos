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
use crate::input_method::InputMethodEvent;
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

/// An input method's composition: text that is shown at the caret,
/// underlined, but is not part of [`TextField::text`] until it is committed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preedit {
    text: String,
    /// Byte offset of the insertion point inside `text`, on a char boundary.
    cursor: usize,
}

/// What is drawn for a field: its text with the composition inserted at the
/// caret, the stop of every caret position, the caret index inside the drawn
/// text, and the byte range of the composition when there is one.
struct Shaped {
    shown: String,
    stops: Vec<(usize, f32)>,
    caret: usize,
    composition: Option<(usize, usize)>,
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
    preedit: Option<Preedit>,
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
            preedit: None,
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
    /// Any composition in progress is dropped.
    pub fn set_text(&mut self, text: &str) {
        self.preedit = None;
        self.text = sanitize(text, self.max_chars);
        self.caret = self.text.len();
        self.anchor = None;
        self.undo.clear();
        self.scroll = 0.0;
    }

    /// The composition in progress and the byte offset of the insertion point
    /// inside it, if an input method is composing.
    pub fn preedit(&self) -> Option<(&str, usize)> {
        self.preedit
            .as_ref()
            .map(|preedit| (preedit.text.as_str(), preedit.cursor))
    }

    /// Show `text` as the composition at the caret, with the insertion point
    /// at byte `cursor` inside it. Replaces the previous composition; empty
    /// text ends the composition without committing. Secret fields and
    /// disabled fields show no composition.
    pub fn set_preedit(&mut self, text: &str, cursor: usize) -> TextFieldOutcome {
        if !self.enabled || self.secret {
            return TextFieldOutcome::Ignored;
        }
        if text.is_empty() {
            return self.cancel_preedit();
        }
        let mut at = cursor.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        let clean = sanitize(text, self.max_chars);
        let cursor = sanitize(&text[..at], self.max_chars).len().min(clean.len());
        self.preedit = Some(Preedit {
            text: clean,
            cursor,
        });
        TextFieldOutcome::Changed
    }

    /// End the composition without committing it. Ignored when nothing is
    /// composing.
    pub fn cancel_preedit(&mut self) -> TextFieldOutcome {
        match self.preedit.take() {
            Some(_) => TextFieldOutcome::Changed,
            None => TextFieldOutcome::Ignored,
        }
    }

    /// The input method committed `text`: the composition ends and the text
    /// is inserted at the caret, replacing any selection (undoable).
    pub fn commit(&mut self, text: &str) -> TextFieldOutcome {
        if !self.enabled {
            return TextFieldOutcome::Ignored;
        }
        let composing = self.preedit.take().is_some();
        if text.is_empty() {
            return if composing {
                TextFieldOutcome::Changed
            } else {
                TextFieldOutcome::Ignored
            };
        }
        self.insert(text)
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
        self.preedit = None;
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
            self.preedit = None;
            return TextFieldOutcome::Ignored;
        }
        // A composition belongs to the focused field only.
        if !self.focused {
            self.preedit = None;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.bounds.contains(*x, *y) => {
                // A click moves the caret, which ends any composition.
                self.preedit = None;
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
            Event::InputMethod(InputMethodEvent::Preedit { text, cursor }) if self.focused => {
                self.set_preedit(text, *cursor)
            }
            Event::InputMethod(InputMethodEvent::Commit(text)) if self.focused => self.commit(text),
            Event::InputMethod(InputMethodEvent::PreeditHidden) if self.focused => {
                self.cancel_preedit()
            }
            _ => TextFieldOutcome::Ignored,
        }
    }

    fn key(&mut self, key: &Key, modifiers: Modifiers) -> TextFieldOutcome {
        // While composing, the input method owns the keys. Escape is the one
        // the field handles itself: it cancels the composition.
        if self.preedit.is_some() {
            return match key {
                Key::Escape => self.cancel_preedit(),
                _ => TextFieldOutcome::Ignored,
            };
        }
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

    /// What is drawn, with any composition inserted at the caret.
    fn shaped(&self) -> Shaped {
        if self.secret {
            return Shaped {
                shown: self.shown(),
                stops: self.stops(),
                caret: self.caret,
                composition: None,
            };
        }
        match &self.preedit {
            None => Shaped {
                shown: self.text.clone(),
                stops: caret_stops(&self.text, &self.font),
                caret: self.caret,
                composition: None,
            },
            Some(preedit) => {
                let mut shown = String::with_capacity(self.text.len() + preedit.text.len());
                shown.push_str(&self.text[..self.caret]);
                shown.push_str(&preedit.text);
                shown.push_str(&self.text[self.caret..]);
                Shaped {
                    stops: caret_stops(&shown, &self.font),
                    shown,
                    caret: self.caret + preedit.cursor,
                    composition: Some((self.caret, self.caret + preedit.text.len())),
                }
            }
        }
    }

    /// Map a byte index of `text` to the drawn text (a composition shifts the
    /// indexes after the caret).
    fn drawn_index(&self, index: usize, shaped: &Shaped) -> usize {
        match shaped.composition {
            Some((_, end)) if index > self.caret => index + (end - self.caret),
            _ => index,
        }
    }

    /// The caret as a 1-pixel-wide rectangle in logical pixels, for the input
    /// method's candidate window. Scroll position from the last paint applies.
    pub fn caret_bounds(&self) -> Rect {
        let shaped = self.shaped();
        let line_height = measure("Ag", &self.font).height as i32;
        let text_y = self.bounds.y + (self.bounds.height as i32 - line_height) / 2;
        let caret_x = shaped
            .stops
            .iter()
            .find(|stop| stop.0 == shaped.caret)
            .map_or(0.0, |stop| stop.1);
        Rect {
            x: (self.text_left() + caret_x).round() as i32,
            y: text_y,
            width: 1,
            height: line_height as u32,
        }
    }

    /// Keep the caret visible by scrolling the text horizontally.
    fn scroll_to_caret(&mut self, stops: &[(usize, f32)], caret: usize) {
        let inner = (self.bounds.width as f32 - 2.0 * PADDING as f32).max(1.0);
        let caret_x = stops
            .iter()
            .find(|stop| stop.0 == caret)
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
        let shaped = self.shaped();
        self.scroll_to_caret(&shaped.stops, shaped.caret);
        let line_height = measure("Ag", &self.font).height as i32;
        let text_y = bounds.y + (bounds.height as i32 - line_height) / 2;
        let left = self.text_left();
        let x_of = |index: usize| {
            shaped
                .stops
                .iter()
                .find(|stop| stop.0 == index)
                .map_or(0.0, |stop| stop.1)
        };
        let composing = shaped.composition.is_some();
        surface.with_clip(inset(bounds, 2), || {
            if let Some((start, end)) = self.selection() {
                let start = self.drawn_index(start, &shaped);
                let end = self.drawn_index(end, &shaped);
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
            if self.text.is_empty() && self.preedit.is_none() && !self.placeholder.is_empty() {
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
                surface.draw_text(&shaped.shown, left.round() as i32, text_y, &self.font, ink)?;
            }
            if let Some((start, end)) = shaped.composition {
                // The composition is underlined on the line's last pixel row.
                let x0 = (left + x_of(start)).floor() as i32;
                let x1 = (left + x_of(end)).ceil() as i32;
                surface.fill_rect(
                    Rect {
                        x: x0,
                        y: text_y + line_height - 1,
                        width: (x1 - x0).max(1) as u32,
                        height: 1,
                    },
                    colors.ink,
                )?;
            }
            if self.focused && self.enabled && (self.selection().is_none() || composing) {
                let x = (left + x_of(shaped.caret)).round() as i32;
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

    fn preedit_event(text: &str, cursor: usize) -> Event {
        Event::InputMethod(InputMethodEvent::Preedit {
            text: text.into(),
            cursor,
        })
    }

    fn commit_event(text: &str) -> Event {
        Event::InputMethod(InputMethodEvent::Commit(text.into()))
    }

    #[test]
    fn composition_shows_without_changing_the_value() {
        let mut field = focused();
        typed(&mut field, "ab");
        assert_eq!(field.preedit(), None);
        assert_eq!(
            field.handle(&preedit_event("ni", 2)),
            TextFieldOutcome::Changed
        );
        assert_eq!(field.preedit(), Some(("ni", 2)));
        assert_eq!(field.text(), "ab");
        assert_eq!(field.caret(), 2);
        // The accessibility value is the committed text only.
        assert_eq!(field.accessibility_node().value(), Some("ab"));
    }

    #[test]
    fn a_new_preedit_replaces_the_old_one() {
        let mut field = focused();
        field.handle(&preedit_event("n", 1));
        assert_eq!(
            field.handle(&preedit_event("ni hao", 3)),
            TextFieldOutcome::Changed
        );
        assert_eq!(field.preedit(), Some(("ni hao", 3)));
        assert_eq!(field.text(), "");
    }

    #[test]
    fn an_empty_preedit_ends_the_composition_without_committing() {
        let mut field = focused();
        field.handle(&preedit_event("ni", 2));
        assert_eq!(
            field.handle(&preedit_event("", 0)),
            TextFieldOutcome::Changed
        );
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "");
        assert_eq!(
            field.handle(&preedit_event("", 0)),
            TextFieldOutcome::Ignored
        );
    }

    #[test]
    fn preedit_cursor_is_clamped_to_a_char_boundary() {
        let mut field = focused();
        // Byte 1 is inside the two-byte "你".
        field.handle(&preedit_event("你好", 1));
        assert_eq!(field.preedit(), Some(("你好", 0)));
        field.handle(&preedit_event("你好", 99));
        assert_eq!(field.preedit(), Some(("你好", 6)));
    }

    #[test]
    fn preedit_drops_control_characters_and_keeps_cursor_in_step() {
        let mut field = focused();
        field.handle(&preedit_event("a\u{7}b", 2));
        assert_eq!(field.preedit(), Some(("ab", 1)));
    }

    #[test]
    fn commit_ends_the_composition_and_inserts_at_the_caret() {
        let mut field = focused();
        typed(&mut field, "ab");
        field.handle(&key(Key::Left));
        field.handle(&preedit_event("ni", 2));
        assert_eq!(field.handle(&commit_event("你")), TextFieldOutcome::Edited);
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "a你b");
        assert_eq!(field.caret(), "a你".len());
        // Undo removes the committed text, as it does for typed text.
        field.handle(&with(Key::Character('z'), false, true));
        assert_eq!(field.text(), "ab");
    }

    #[test]
    fn commit_replaces_the_selection() {
        let mut field = focused();
        typed(&mut field, "one two");
        field.handle(&with(Key::Left, true, true));
        assert_eq!(field.selected_text(), Some("two"));
        field.handle(&preedit_event("tw", 2));
        field.handle(&commit_event("2"));
        assert_eq!(field.text(), "one 2");
    }

    #[test]
    fn commit_without_a_composition_just_inserts() {
        let mut field = focused();
        assert_eq!(field.handle(&commit_event("é")), TextFieldOutcome::Edited);
        assert_eq!(field.text(), "é");
        assert_eq!(field.handle(&commit_event("")), TextFieldOutcome::Ignored);
    }

    #[test]
    fn empty_commit_still_ends_a_composition() {
        let mut field = focused();
        field.handle(&preedit_event("x", 1));
        assert_eq!(field.handle(&commit_event("")), TextFieldOutcome::Changed);
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "");
    }

    #[test]
    fn commit_respects_max_chars() {
        let mut field = focused();
        field.max_chars = 3;
        typed(&mut field, "ab");
        field.handle(&commit_event("cdef"));
        assert_eq!(field.text(), "abc");
    }

    #[test]
    fn escape_cancels_and_other_keys_wait_for_the_input_method() {
        let mut field = focused();
        typed(&mut field, "ab");
        field.handle(&preedit_event("ni", 2));
        assert_eq!(field.handle(&key(Key::Left)), TextFieldOutcome::Ignored);
        assert_eq!(
            field.handle(&key(Key::Backspace)),
            TextFieldOutcome::Ignored
        );
        assert_eq!(field.handle(&key(Key::Enter)), TextFieldOutcome::Ignored);
        assert_eq!(field.preedit(), Some(("ni", 2)));
        assert_eq!(field.text(), "ab");
        assert_eq!(field.handle(&key(Key::Escape)), TextFieldOutcome::Changed);
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "ab");
        // With nothing composing, Escape is the field's own cancel again.
        assert_eq!(field.handle(&key(Key::Escape)), TextFieldOutcome::Cancelled);
    }

    #[test]
    fn explicit_hide_and_cancel_end_the_composition() {
        let mut field = focused();
        field.handle(&preedit_event("ni", 2));
        assert_eq!(
            field.handle(&Event::InputMethod(InputMethodEvent::PreeditHidden)),
            TextFieldOutcome::Changed
        );
        assert_eq!(field.preedit(), None);
        assert_eq!(field.cancel_preedit(), TextFieldOutcome::Ignored);
        field.handle(&preedit_event("ni", 2));
        assert_eq!(field.cancel_preedit(), TextFieldOutcome::Changed);
        assert_eq!(field.preedit(), None);
    }

    #[test]
    fn a_click_ends_the_composition() {
        let mut field = focused();
        field.set_text("hello");
        field.handle(&preedit_event("ni", 2));
        let press = Event::PointerPressed {
            x: 12,
            y: 15,
            button: PointerButton::Primary,
        };
        field.handle(&press);
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "hello");
    }

    #[test]
    fn losing_focus_or_being_disabled_drops_the_composition() {
        let mut field = focused();
        field.handle(&preedit_event("ni", 2));
        field.focused = false;
        assert_eq!(
            field.handle(&Event::Paste("x".into())),
            TextFieldOutcome::Ignored
        );
        assert_eq!(field.preedit(), None);
        // Input method events for an unfocused field are ignored.
        assert_eq!(
            field.handle(&preedit_event("ni", 2)),
            TextFieldOutcome::Ignored
        );
        assert_eq!(field.preedit(), None);
        field.focused = true;
        field.handle(&preedit_event("ni", 2));
        field.enabled = false;
        field.handle(&Event::Wake);
        assert_eq!(field.preedit(), None);
        assert_eq!(field.set_preedit("x", 1), TextFieldOutcome::Ignored);
        assert_eq!(field.commit("x"), TextFieldOutcome::Ignored);
    }

    #[test]
    fn set_text_and_paste_drop_a_composition() {
        let mut field = focused();
        field.handle(&preedit_event("ni", 2));
        field.set_text("new");
        assert_eq!(field.preedit(), None);
        field.handle(&preedit_event("ni", 2));
        field.handle(&Event::Paste("p".into()));
        assert_eq!(field.preedit(), None);
        assert_eq!(field.text(), "newp");
    }

    #[test]
    fn secret_fields_show_no_composition() {
        let mut field = TextField::new(3, "PIN");
        field.secret = true;
        field.focused = true;
        assert_eq!(
            field.handle(&preedit_event("ni", 2)),
            TextFieldOutcome::Ignored
        );
        assert_eq!(field.preedit(), None);
    }

    #[test]
    fn caret_stays_inside_the_composition_and_is_reported() {
        let mut field = focused();
        typed(&mut field, "ab");
        field.handle(&preedit_event("你好", "你".len()));
        let shaped = field.shaped();
        assert_eq!(shaped.shown, "ab你好");
        assert_eq!(shaped.caret, 2 + "你".len());
        assert_eq!(shaped.composition, Some((2, 2 + "你好".len())));
        // The caret rectangle moves with the insertion point.
        let before = field.caret_bounds().x;
        field.handle(&preedit_event("你好", 0));
        assert!(field.caret_bounds().x < before);
    }

    #[test]
    fn composition_paints_an_underline_under_the_preedit_at_both_scales() {
        use crate::foundation::geometry::Scale;
        for factor in [1, 2] {
            let surface = RasterSurface::new(240, 40, Scale::new(factor).unwrap()).unwrap();
            surface.clear(palette().surface).unwrap();
            let mut field = focused();
            field.bounds = Rect {
                x: 10,
                y: 10,
                width: 200,
                height: 22,
            };
            field.handle(&preedit_event("nihao", 5));
            field.paint(&surface).unwrap();
            let shaped = field.shaped();
            let (start, end) = shaped.composition.unwrap();
            let x_of = |index: usize| {
                shaped
                    .stops
                    .iter()
                    .find(|stop| stop.0 == index)
                    .map_or(0.0, |stop| stop.1)
            };
            let line_height = measure("Ag", &field.font).height as i32;
            let text_y = field.bounds.y + (field.bounds.height as i32 - line_height) / 2;
            let underline_row = ((text_y + line_height - 1) * factor as i32) as u32;
            // The field's text starts 4 px into the border; its underline runs
            // under the composition at the line's last pixel row.
            let mid = (field.text_left() + (x_of(start) + x_of(end)) / 2.0) as i32;
            assert_eq!(
                surface.pixel((mid * factor as i32) as u32, underline_row),
                Some(0xff000000),
                "underline at scale {factor}"
            );
            // Nothing is underlined past the composition.
            let after = (field.text_left() + x_of(end)).ceil() as i32 + 3;
            assert_ne!(
                surface.pixel((after * factor as i32) as u32, underline_row),
                Some(0xff000000)
            );
        }
    }
}
