use crate::{
    event::{KeyCode, MouseButton},
    text_byte_offset_at_x,
    theme::ThemeContext,
    AccessibilityNode, AccessibilityRole, AccessibleTextState, Event, EventResult,
    LayoutConstraint, Rect, Size, Widget, WidgetState, TEXT_FIELD_TEXT_INSET,
};

pub struct TextField {
    state: WidgetState,
    pub text: String,
    pub placeholder: String,
    pub is_password: bool,
    pub multiline: bool,
    pub expands_horizontally: bool,
    pub on_change: Option<Box<dyn FnMut(String) + Send>>,
    cursor_position: usize,
    /// The fixed end of the active selection, stored as a UTF-8 byte offset.
    /// The other end is [`cursor_position`]. `None` means there is no active
    /// selection. Keeping offsets on character boundaries makes all editing
    /// operations safe for multi-byte Unicode text.
    selection_anchor: Option<usize>,
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

impl TextField {
    pub fn new() -> Self {
        Self {
            state: WidgetState::new(),
            text: String::new(),
            placeholder: String::new(),
            is_password: false,
            multiline: false,
            expands_horizontally: false,
            on_change: None,
            cursor_position: 0,
            selection_anchor: None,
        }
    }

    pub fn with_placeholder<S: Into<String>>(mut self, text: S) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn set_multiline(&mut self, multiline: bool) {
        self.multiline = multiline;
    }

    pub fn set_expands_horizontally(&mut self, expands: bool) {
        self.expands_horizontally = expands;
    }
    pub fn set_text<S: Into<String>>(&mut self, text: S) {
        self.text = text.into();
        self.cursor_position = self.text.len();
        self.selection_anchor = None;
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Return the active selection as an ordered UTF-8 byte range.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selection_anchor?;
        let range = if anchor <= self.cursor_position {
            (anchor, self.cursor_position)
        } else {
            (self.cursor_position, anchor)
        };
        (range.0 != range.1).then_some(range)
    }

    /// Return the selected text, if a non-empty selection is active.
    pub fn selected_text(&self) -> Option<&str> {
        self.selection_range()
            .map(|(start, end)| &self.text[start..end])
    }

    /// Set an ordered-independent selection using UTF-8 byte offsets.
    ///
    /// Both endpoints are clamped to valid character boundaries. `cursor` is
    /// retained as the active end so Shift+Arrow can continue extending from
    /// the same side of the selection.
    pub fn set_selection(&mut self, anchor: usize, cursor: usize) {
        let anchor = self.clamp_position(anchor);
        let cursor = self.clamp_position(cursor);
        self.cursor_position = cursor;
        self.selection_anchor = (anchor != cursor).then_some(anchor);
    }

    /// Select the complete document. Empty text intentionally has no range.
    pub fn select_all(&mut self) {
        self.cursor_position = self.text.len();
        self.selection_anchor = (!self.text.is_empty()).then_some(0);
    }

    /// Clear the active selection while retaining the current caret position.
    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    /// Replace the active selection, or insert at the caret when there is no
    /// selection. Returns whether the text was updated.
    pub fn replace_selection(&mut self, replacement: &str) -> bool {
        let (start, end) = self
            .selection_range()
            .unwrap_or((self.cursor_position, self.cursor_position));
        self.text.replace_range(start..end, replacement);
        self.cursor_position = start + replacement.len();
        self.selection_anchor = None;
        self.notify_change();
        true
    }

    /// Delete the active selection and return the removed text. With no
    /// selection this leaves the document unchanged.
    pub fn delete_selection(&mut self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let removed = self.text[start..end].to_string();
        self.replace_selection("");
        Some(removed)
    }

    fn notify_change(&mut self) {
        if let Some(cb) = &mut self.on_change {
            (cb)(self.text.clone());
        }
    }

    fn clamp_position(&self, pos: usize) -> usize {
        let mut pos = pos.min(self.text.len());
        while pos > 0 && !self.text.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    /// Clamps to the text length and snaps down to the nearest UTF-8 char
    /// boundary — the cursor is a byte offset and must never sit inside a
    /// multi-byte character.
    pub fn set_cursor_position(&mut self, pos: usize) {
        self.cursor_position = self.clamp_position(pos);
        self.selection_anchor = None;
    }

    /// Move the cursor back one full character (may be multi-byte) — the
    /// same char-boundary logic `Backspace` uses, just without deleting.
    fn previous_boundary(&self) -> usize {
        if self.cursor_position == 0 {
            return 0;
        }
        self.text[..self.cursor_position]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Move the cursor forward one full character (may be multi-byte).
    /// `cursor_position` is always on a char boundary already (the
    /// invariant `set_cursor_position` maintains), so slicing from it and
    /// reading the next `char` can never panic.
    fn next_boundary(&self) -> usize {
        if self.cursor_position >= self.text.len() {
            return self.text.len();
        }
        self.text[self.cursor_position..]
            .chars()
            .next()
            .map(|c| self.cursor_position + c.len_utf8())
            .unwrap_or(self.cursor_position)
    }

    fn move_horizontally(&mut self, right: bool, extend: bool) {
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_position);
            }
            self.cursor_position = if right {
                self.next_boundary()
            } else {
                self.previous_boundary()
            };
            if self.selection_anchor == Some(self.cursor_position) {
                self.selection_anchor = None;
            }
            return;
        }

        if let Some((start, end)) = self.selection_range() {
            self.cursor_position = if right { end } else { start };
            self.selection_anchor = None;
        } else {
            self.cursor_position = if right {
                self.next_boundary()
            } else {
                self.previous_boundary()
            };
        }
    }

    fn move_to_boundary(&mut self, end: bool, extend: bool) {
        let target = if end { self.text.len() } else { 0 };
        if extend {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor_position);
            }
            self.cursor_position = target;
            if self.selection_anchor == Some(self.cursor_position) {
                self.selection_anchor = None;
            }
        } else {
            self.cursor_position = target;
            self.selection_anchor = None;
        }
    }

    fn delete_backward(&mut self) {
        if self.delete_selection().is_some() {
            return;
        }
        if self.cursor_position == 0 {
            return;
        }
        let previous = self.previous_boundary();
        self.text.replace_range(previous..self.cursor_position, "");
        self.cursor_position = previous;
        self.notify_change();
    }

    fn delete_forward(&mut self) {
        if self.delete_selection().is_some() {
            return;
        }
        if self.cursor_position >= self.text.len() {
            return;
        }
        let next = self.next_boundary();
        self.text.replace_range(self.cursor_position..next, "");
        self.notify_change();
    }
}

impl Widget for TextField {
    fn widget_state(&self) -> &WidgetState {
        &self.state
    }
    fn widget_state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }

    // Was: no override, so every `TextField` was focusable-in-spirit but
    // never actually joined the tab order and nothing ever gated input on
    // it (see AGENTS.md, P2). Text input is exactly the case
    // `focusable()` exists for. Hidden or disabled fields (e.g. a closed
    // find bar) stay out of the tab order.
    fn focusable(&self) -> bool {
        self.state.enabled && self.state.visibility == crate::Visibility::Visible
    }

    fn wants_click_focus(&self) -> bool {
        self.focusable()
    }

    fn layout(&mut self, constraint: LayoutConstraint) -> Size {
        let (width, height) = if self.multiline {
            (
                constraint.max_width.max(constraint.min_width),
                constraint.max_height.max(constraint.min_height),
            )
        } else if self.expands_horizontally {
            (constraint.max_width.max(constraint.min_width), 26.0)
        } else {
            (constraint.max_width.min(200.0), 26.0)
        };
        let size = constraint.clamp(Size::new(width, height));
        self.set_rect(Rect::new(
            self.rect().x,
            self.rect().y,
            size.width,
            size.height,
        ));
        size
    }

    fn draw(&self, _theme: &ThemeContext) {}

    // Was: `Char`/`Backspace` mutated `text` unconditionally, with no rect
    // check and no focus gate at all — every `TextField` in the tree
    // consumed every keystroke (see AGENTS.md, P2). Now:
    // `MouseDown` inside the rect click-to-focuses (and only this field —
    // nothing else on the tree loses focus here, that's `FocusManager`'s
    // job once an app wires it up), and every keyboard branch refuses to
    // act unless `focused` is already set.
    fn handle_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseDown {
                button: MouseButton::Left,
                point,
                modifiers,
                ..
            } => {
                if !self.rect().contains(*point) {
                    return EventResult::Ignored;
                }
                self.widget_state_mut().focused = true;
                // The SDK painter starts text six logical pixels inside the
                // field. Map the click through the same shaped layout so
                // proportional glyphs, ligatures and Unicode never collapse
                // to a fixed-width character estimate.
                let clicked = text_byte_offset_at_x(
                    &self.text,
                    point.x - self.rect().x - TEXT_FIELD_TEXT_INSET,
                );
                if modifiers.shift {
                    if self.selection_anchor.is_none() {
                        self.selection_anchor = Some(self.cursor_position);
                    }
                    self.cursor_position = clicked;
                    if self.selection_anchor == Some(self.cursor_position) {
                        self.selection_anchor = None;
                    }
                } else {
                    self.cursor_position = clicked;
                    self.selection_anchor = None;
                }
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::Backspace,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.delete_backward();
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::Delete,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.delete_forward();
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::ArrowLeft,
                modifiers,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.move_horizontally(false, modifiers.shift);
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::ArrowRight,
                modifiers,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.move_horizontally(true, modifiers.shift);
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::Home,
                modifiers,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.move_to_boundary(false, modifiers.shift);
                EventResult::Handled
            }
            Event::KeyDown {
                key: KeyCode::End,
                modifiers,
                ..
            } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                self.move_to_boundary(true, modifiers.shift);
                EventResult::Handled
            }
            Event::Char { character } => {
                if !self.widget_state().focused {
                    return EventResult::Ignored;
                }
                let character = character.to_string();
                self.replace_selection(&character);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    fn accessibility(&self) -> Option<AccessibilityNode> {
        Some(
            AccessibilityNode::new(AccessibilityRole::TextField, &self.text)
                .with_description(&self.placeholder),
        )
    }

    fn accessibility_text(&self) -> Option<AccessibleTextState> {
        let mut state = AccessibleTextState::new(self.text.clone());
        state.set_caret(self.cursor_position);
        if let Some((start, end)) = self.selection_range() {
            state.set_selection(start, end);
        }
        Some(state)
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
    use super::*;
    use crate::{event::Modifiers, measure_text_width, Point};
    use std::sync::{Arc, Mutex};

    fn key(key: KeyCode) -> Event {
        Event::KeyDown {
            key,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn focusable_is_true() {
        assert!(TextField::new().focusable());
    }

    #[test]
    fn char_and_backspace_are_ignored_when_not_focused() {
        let mut field = TextField::new();
        field.set_text("abc");
        assert!(!field.widget_state().focused);

        assert!(matches!(
            field.handle_event(&Event::Char { character: 'x' }),
            EventResult::Ignored
        ));
        assert_eq!(field.text(), "abc");

        assert!(matches!(
            field.handle_event(&key(KeyCode::Backspace)),
            EventResult::Ignored
        ));
        assert_eq!(field.text(), "abc");
    }

    #[test]
    fn arrow_keys_are_ignored_when_not_focused() {
        let mut field = TextField::new();
        field.set_text("abc");
        field.set_cursor_position(1);

        assert!(matches!(
            field.handle_event(&key(KeyCode::ArrowLeft)),
            EventResult::Ignored
        ));
        assert_eq!(field.cursor_position(), 1);
    }

    #[test]
    fn click_inside_rect_focuses_and_places_cursor_near_the_click() {
        let mut field = TextField::new();
        field.set_rect(Rect::new(100.0, 0.0, 200.0, 26.0));
        assert!(!field.widget_state().focused);

        let text = "héllo";
        field.set_text(text);
        let he_width = measure_text_width("hé");
        let hel_width = measure_text_width("hél");
        // Click just past the `hé` caret, still well before the next `l`.
        let point = Point::new(
            100.0 + TEXT_FIELD_TEXT_INSET + he_width + (hel_width - he_width) * 0.2,
            10.0,
        );
        let result = field.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point,
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert!(field.widget_state().focused);
        assert_eq!(
            field.cursor_position(),
            3,
            "lands after 'h' + 'é' (3 bytes), never mid-character"
        );
    }

    #[test]
    fn click_uses_shaped_width_for_proportional_text() {
        let mut field = TextField::new();
        let text = "Wiii";
        field.set_text(text);
        field.set_rect(Rect::new(100.0, 0.0, 200.0, 26.0));

        let layout = slopos_render::font::shape_text(
            text,
            slopos_render::font::TextLayoutOptions::new(13.0, 1.0),
        );
        let first = layout
            .glyphs()
            .iter()
            .find(|glyph| glyph.cluster_start == 0)
            .expect("first glyph");
        let next = layout
            .glyphs()
            .iter()
            .find(|glyph| glyph.cluster_start == first.cluster_end)
            .expect("second glyph");
        let first_end = first.x + first.advance;
        let next_end = next.x + next.advance;
        let click_x = first_end + (next_end - first_end) * 0.2;

        let result = field.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(100.0 + TEXT_FIELD_TEXT_INSET + click_x, 10.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(field.cursor_position(), first.cluster_end);
    }

    #[test]
    fn click_outside_rect_is_ignored_and_does_not_focus() {
        let mut field = TextField::new();
        field.set_rect(Rect::new(100.0, 0.0, 200.0, 26.0));

        let result = field.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(5.0, 5.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Ignored));
        assert!(!field.widget_state().focused);
    }

    #[test]
    fn click_far_past_the_text_clamps_cursor_to_the_end() {
        let mut field = TextField::new();
        field.set_text("hi");
        field.set_rect(Rect::new(0.0, 0.0, 200.0, 26.0));

        let result = field.handle_event(&Event::MouseDown {
            button: MouseButton::Left,
            point: Point::new(190.0, 10.0),
            modifiers: Modifiers::NONE,
        });

        assert!(matches!(result, EventResult::Handled));
        assert_eq!(field.cursor_position(), field.text().len());
    }

    #[test]
    fn arrow_keys_move_by_whole_characters_not_bytes() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;
        field.set_text("héllo"); // cursor starts at the end (byte 6)
        assert_eq!(field.cursor_position(), 6);

        assert!(matches!(
            field.handle_event(&key(KeyCode::ArrowLeft)),
            EventResult::Handled
        ));
        assert_eq!(field.cursor_position(), 5, "before the trailing 'o'");

        assert!(matches!(
            field.handle_event(&key(KeyCode::ArrowLeft)),
            EventResult::Handled
        ));
        assert_eq!(
            field.cursor_position(),
            4,
            "steps over one 'l', not into the middle of 'é'"
        );

        assert!(matches!(
            field.handle_event(&key(KeyCode::ArrowRight)),
            EventResult::Handled
        ));
        assert_eq!(field.cursor_position(), 5);
    }

    #[test]
    fn home_and_end_move_cursor_to_bounds() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;
        field.set_text("héllo");
        field.set_cursor_position(3);

        let _ = field.handle_event(&key(KeyCode::Home));
        assert_eq!(field.cursor_position(), 0);

        let _ = field.handle_event(&key(KeyCode::End));
        assert_eq!(field.cursor_position(), field.text().len());
    }

    #[test]
    fn multibyte_insert_and_backspace_does_not_panic_and_tracks_byte_cursor() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;

        // 'é' is 2 bytes in UTF-8: insertion must advance by full
        // characters and backspace must remove exactly one, never leaving
        // the cursor mid-codepoint.
        let _ = field.handle_event(&Event::Char { character: 'h' });
        let _ = field.handle_event(&Event::Char { character: 'é' });
        assert_eq!(field.text(), "hé");
        assert_eq!(field.cursor_position(), 3); // 1 + 2 bytes

        let _ = field.handle_event(&key(KeyCode::Backspace));
        assert_eq!(field.text(), "h");
        assert_eq!(field.cursor_position(), 1);
    }

    #[test]
    fn shift_navigation_selects_unicode_without_splitting_bytes() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;
        field.set_text("aé🙂z");

        // End is byte 8; first move before the final `z`, then Shift+Left
        // extends over the four-byte emoji.
        let _ = field.handle_event(&key(KeyCode::ArrowLeft));
        let _ = field.handle_event(&Event::KeyDown {
            key: KeyCode::ArrowLeft,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        });
        assert_eq!(field.selection_range(), Some((3, 7)));
        assert_eq!(field.selected_text(), Some("🙂"));

        let _ = field.handle_event(&Event::KeyDown {
            key: KeyCode::ArrowRight,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(field.selection_range(), None);
        assert_eq!(field.cursor_position(), 7);
    }

    #[test]
    fn replacing_selection_preserves_unicode_boundaries_and_calls_on_change() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;
        field.set_text("aé🙂z");
        field.set_selection(1, 7); // `é🙂`, both endpoints are boundaries.

        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_for_callback = Arc::clone(&changes);
        field.on_change = Some(Box::new(move |text| {
            changes_for_callback.lock().unwrap().push(text)
        }));
        assert_eq!(field.selected_text(), Some("é🙂"));
        field.replace_selection("X");

        assert_eq!(field.text(), "aXz");
        assert_eq!(field.cursor_position(), 2);
        assert_eq!(field.selection_range(), None);
        assert_eq!(*changes.lock().unwrap(), vec!["aXz"]);
    }

    #[test]
    fn accessibility_snapshot_tracks_live_utf8_caret_and_selection() {
        let mut field = TextField::new();
        field.set_text("A😀B");
        field.set_selection("A".len(), "A😀".len());

        let snapshot = field.accessibility_text().expect("text snapshot");
        assert_eq!(snapshot.text, "A😀B");
        assert_eq!(snapshot.caret_offset, "A😀".len());
        assert_eq!(snapshot.selected_text(), "😀");
    }

    #[test]
    fn backspace_and_delete_remove_selection_as_one_edit() {
        let mut field = TextField::new();
        field.widget_state_mut().focused = true;
        field.set_text("abcd");
        field.set_selection(1, 3);

        let _ = field.handle_event(&key(KeyCode::Backspace));
        assert_eq!(field.text(), "ad");
        assert_eq!(field.cursor_position(), 1);
        assert_eq!(field.selection_range(), None);

        field.set_selection(0, 1);
        let _ = field.handle_event(&key(KeyCode::Delete));
        assert_eq!(field.text(), "d");
        assert_eq!(field.cursor_position(), 0);
    }
}
