//! Multi-line text area: caret, selection, scrolling and editing over a text
//! that the caller owns.
//!
//! Like [`super::text_field::TextField`] this is a state machine: feed it
//! events with [`TextArea::handle`] and paint it with [`TextArea::paint`].
//! Unlike `TextField` it does not own the text. Each call takes the current
//! text, and every change comes back as a [`TextEdit`] that the caller applies
//! to its own buffer. That keeps undo history, file state and size limits in
//! the app, where they can be tested without a window.
//!
//! Lines are not wrapped: long lines scroll horizontally, and the view
//! scrolls vertically through [`ScrollState`]. The caret does not blink, so an
//! idle editor needs no timer. Tab inserts a tab character, so keyboard focus
//! must leave the area another way.

use crate::controls::scroll::{ScrollOutcome, ScrollState};
use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::foundation::geometry::Rect;
use crate::render::{RasterSurface, RenderError};
use crate::text::{caret_stops, hit_test, measure, Font};
use crate::theme::palette;
use unicode_segmentation::UnicodeSegmentation;

/// Space between the border and the text, in logical pixels.
const PADDING: i32 = 4;

/// A replacement of the bytes `start..end` with `inserted`. An insertion has
/// `start == end`; a deletion has an empty `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub inserted: String,
}

impl TextEdit {
    /// The caret position once this edit has been applied.
    pub fn caret_after(&self) -> usize {
        self.start + self.inserted.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAreaOutcome {
    /// The event did not concern this area.
    Ignored,
    /// Caret, selection, scroll or focus changed; repaint.
    Changed,
    /// The caller must replace this range of its text.
    Edit(TextEdit),
    /// Cut: `text` goes to the clipboard and `edit` removes it from the text.
    Cut { text: String, edit: TextEdit },
    /// Ctrl+C: put this text on the clipboard.
    Copy(String),
    /// Ctrl+V: request clipboard text and pass it back as [`Event::Paste`].
    PasteRequested,
    /// A pointer press landed in the area; give it keyboard focus.
    FocusRequested,
}

#[derive(Debug, Clone)]
pub struct TextArea {
    pub id: u64,
    pub bounds: Rect,
    pub enabled: bool,
    pub focused: bool,
    /// Edits are refused; selection, copy and caret keys still work.
    pub read_only: bool,
    /// Accessible name, e.g. "Document".
    pub label: String,
    /// The largest text, in bytes, that an edit may produce.
    pub max_len: usize,
    pub font: Font,
    caret: usize,
    anchor: Option<usize>,
    /// The column Up and Down try to keep, in logical pixels.
    goal_x: Option<f32>,
    /// Horizontal scroll, in logical pixels.
    hoffset: f32,
    scroll: ScrollState,
    dragging: bool,
}

impl TextArea {
    pub fn new(id: u64, label: &str) -> Self {
        Self {
            id,
            bounds: Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 200,
            },
            enabled: true,
            focused: false,
            read_only: false,
            label: label.to_owned(),
            max_len: usize::MAX,
            font: Font::monospace(12.0),
            caret: 0,
            anchor: None,
            goal_x: None,
            hoffset: 0.0,
            scroll: ScrollState::default(),
            dragging: false,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = bounds;
        self
    }

    /// The caret's byte offset into the text.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected byte range, ordered, if there is a selection.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        (anchor != self.caret).then(|| (anchor.min(self.caret), anchor.max(self.caret)))
    }

    pub fn selected_text<'a>(&self, text: &'a str) -> Option<&'a str> {
        self.selection()
            .and_then(|(start, end)| text.get(start..end))
    }

    /// Move the caret without selecting. Used after the caller undoes or
    /// redoes an edit. The index is clamped to a char boundary.
    pub fn place_caret(&mut self, text: &str, index: usize) {
        self.caret = floor_index(text, index);
        self.anchor = None;
        self.goal_x = None;
        self.reveal(text);
    }

    /// Select `start..end` and put the caret at `end`.
    pub fn select(&mut self, text: &str, start: usize, end: usize) {
        self.anchor = Some(floor_index(text, start));
        self.caret = floor_index(text, end);
        self.goal_x = None;
        self.reveal(text);
    }

    pub fn select_all(&mut self, text: &str) {
        self.select(text, 0, text.len());
    }

    /// The vertical scroll offset in logical pixels.
    pub fn scroll_offset(&self) -> u64 {
        self.scroll.offset()
    }

    /// Handle one event. `text` is the caller's current text.
    pub fn handle(&mut self, text: &str, event: &Event) -> TextAreaOutcome {
        self.sync(text);
        if !self.enabled {
            return TextAreaOutcome::Ignored;
        }
        if self.scroll.handle(event) == ScrollOutcome::Scrolled {
            return TextAreaOutcome::Changed;
        }
        match event {
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            } if self.text_rect().contains(*x, *y) => {
                let line = self.line_for_y(text, *y);
                let index = self.index_at(text, line, *x as f32 - self.text_left());
                self.caret = index;
                self.anchor = Some(index);
                self.goal_x = None;
                self.dragging = true;
                if self.focused {
                    TextAreaOutcome::Changed
                } else {
                    TextAreaOutcome::FocusRequested
                }
            }
            Event::PointerMoved { x, y } if self.dragging => {
                let line = self.line_for_y(text, *y);
                let index = self.index_at(text, line, *x as f32 - self.text_left());
                if index != self.caret {
                    self.caret = index;
                    self.goal_x = None;
                    TextAreaOutcome::Changed
                } else {
                    TextAreaOutcome::Ignored
                }
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } if self.dragging => {
                self.dragging = false;
                TextAreaOutcome::Ignored
            }
            Event::KeyPressed { key, modifiers } if self.focused => self.key(text, key, *modifiers),
            Event::Paste(pasted) if self.focused => self.insert(text, pasted),
            _ => TextAreaOutcome::Ignored,
        }
    }

    fn key(&mut self, text: &str, key: &Key, modifiers: Modifiers) -> TextAreaOutcome {
        let Modifiers {
            shift,
            control,
            alt,
            logo,
        } = modifiers;
        let vertical = matches!(key, Key::Up | Key::Down | Key::PageUp | Key::PageDown);
        if !vertical {
            self.goal_x = None;
        }
        if control && !alt && !logo {
            if let Key::Character(c) = key {
                return match c.to_ascii_lowercase() {
                    'a' => {
                        self.select_all(text);
                        TextAreaOutcome::Changed
                    }
                    'c' => self.copy(text),
                    'x' => self.cut(text),
                    'v' if !self.read_only => TextAreaOutcome::PasteRequested,
                    _ => TextAreaOutcome::Ignored,
                };
            }
        }
        if alt || logo {
            return TextAreaOutcome::Ignored;
        }
        let caret = self.caret;
        match key {
            Key::Character(c) if !control => self.insert(text, c.encode_utf8(&mut [0; 4])),
            Key::Space if !control => self.insert(text, " "),
            Key::Enter if !control => self.insert(text, "\n"),
            Key::Tab if !control => self.insert(text, "\t"),
            Key::Backspace | Key::Delete => {
                let (start, end) = match self.selection() {
                    Some(range) => range,
                    None => {
                        let forward = *key == Key::Delete;
                        let target = match (forward, control) {
                            (false, false) => prev_char(text, caret),
                            (false, true) => prev_word(text, caret),
                            (true, false) => next_char(text, caret),
                            (true, true) => next_word(text, caret),
                        };
                        (caret.min(target), caret.max(target))
                    }
                };
                if start == end {
                    return TextAreaOutcome::Ignored;
                }
                self.replace(text, start, end, "")
            }
            Key::Left | Key::Right => {
                let target = match (key, control, self.selection()) {
                    (Key::Left, false, Some((start, _))) if !shift => start,
                    (Key::Right, false, Some((_, end))) if !shift => end,
                    (Key::Left, false, _) => prev_char(text, caret),
                    (Key::Right, false, _) => next_char(text, caret),
                    (Key::Left, true, _) => prev_word(text, caret),
                    _ => next_word(text, caret),
                };
                self.move_to(target, shift);
                self.reveal(text);
                TextAreaOutcome::Changed
            }
            Key::Home | Key::End => {
                let target = if control {
                    if *key == Key::Home {
                        0
                    } else {
                        text.len()
                    }
                } else {
                    let (_, start) = line_of(text, caret);
                    if *key == Key::Home {
                        start
                    } else {
                        line_end(text, start)
                    }
                };
                self.move_to(target, shift);
                self.reveal(text);
                TextAreaOutcome::Changed
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown => {
                let (line, x) = self.caret_position(text);
                let last = line_count(text) - 1;
                let page = (self.text_rect().height as i64 / self.line_height() as i64).max(1);
                let step = match key {
                    Key::Up => -1,
                    Key::Down => 1,
                    Key::PageUp => -page,
                    _ => page,
                };
                // Up on the first line goes to the start, Down on the last to the end.
                let target = if step < 0 && line == 0 {
                    0
                } else if step > 0 && line == last {
                    text.len()
                } else {
                    let target_line = (line as i64 + step).clamp(0, last as i64) as usize;
                    self.index_at(text, target_line, self.goal_x.unwrap_or(x))
                };
                self.goal_x = Some(self.goal_x.unwrap_or(x));
                self.move_to(target, shift);
                self.reveal(text);
                TextAreaOutcome::Changed
            }
            _ => TextAreaOutcome::Ignored,
        }
    }

    fn move_to(&mut self, target: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = target;
    }

    fn copy(&self, text: &str) -> TextAreaOutcome {
        match self.selected_text(text) {
            Some(selected) => TextAreaOutcome::Copy(selected.to_owned()),
            None => TextAreaOutcome::Ignored,
        }
    }

    fn cut(&mut self, text: &str) -> TextAreaOutcome {
        let Some((start, end)) = self.selection() else {
            return TextAreaOutcome::Ignored;
        };
        let selected = text[start..end].to_owned();
        if self.read_only {
            return TextAreaOutcome::Copy(selected);
        }
        match self.replace(text, start, end, "") {
            TextAreaOutcome::Edit(edit) => TextAreaOutcome::Cut {
                text: selected,
                edit,
            },
            other => other,
        }
    }

    /// Type or paste `input` in place of the selection.
    fn insert(&mut self, text: &str, input: &str) -> TextAreaOutcome {
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let clean = clean(input);
        self.replace(text, start, end, &clean)
    }

    /// Replace `start..end` with `inserted`, if the area may change.
    fn replace(&mut self, text: &str, start: usize, end: usize, inserted: &str) -> TextAreaOutcome {
        if !self.enabled || self.read_only || (start == end && inserted.is_empty()) {
            return TextAreaOutcome::Ignored;
        }
        let size = text.len() - (end - start) + inserted.len();
        if size > self.max_len {
            return TextAreaOutcome::Ignored;
        }
        let edit = TextEdit {
            start,
            end,
            inserted: inserted.to_owned(),
        };
        self.caret = edit.caret_after();
        self.anchor = None;
        self.goal_x = None;
        self.reveal(text);
        TextAreaOutcome::Edit(edit)
    }

    /// Keep the caret inside the field's stored state (after the caller changed
    /// the text) and refresh the scroll range.
    fn sync(&mut self, text: &str) {
        self.scroll.area = self.bounds;
        let lh = self.line_height() as u64;
        let content = line_count(text) as u64 * lh + 2 * PADDING as u64;
        self.scroll.set_content(content);
        self.caret = floor_index(text, self.caret);
        self.anchor = self.anchor.map(|anchor| floor_index(text, anchor));
    }

    /// The text rectangle: the bounds without the scrollbar.
    fn text_rect(&self) -> Rect {
        self.scroll.content_area()
    }

    fn line_height(&self) -> i32 {
        (measure("Ag", &self.font).height as i32).max(1)
    }

    /// The x of the text's left edge in logical pixels (after the scroll).
    fn text_left(&self) -> f32 {
        (self.text_rect().x + PADDING) as f32 - self.hoffset
    }

    fn line_for_y(&self, text: &str, y: i32) -> usize {
        let relative = y - (self.text_rect().y + PADDING) + self.scroll.offset() as i32;
        let line = relative.max(0) / self.line_height();
        (line as usize).min(line_count(text) - 1)
    }

    /// The x of byte `offset` within one line's text.
    fn x_in_line(&self, line: &str, offset: usize) -> f32 {
        let stops = caret_stops(line, &self.font);
        stops
            .iter()
            .find(|stop| stop.0 == offset)
            .or(stops.last())
            .map_or(0.0, |stop| stop.1)
    }

    /// The caret's line number and its x within the line.
    fn caret_position(&self, text: &str) -> (usize, f32) {
        let (line, start) = line_of(text, self.caret);
        let end = line_end(text, start);
        (line, self.x_in_line(&text[start..end], self.caret - start))
    }

    /// The byte index nearest to column `x` on `line`.
    fn index_at(&self, text: &str, line: usize, x: f32) -> usize {
        let (start, end) = line_range(text, line);
        start + hit_test(&text[start..end], &self.font, x)
    }

    /// Scroll so that the caret's line and column are visible.
    fn reveal(&mut self, text: &str) {
        let lh = self.line_height();
        let (line, x) = self.caret_position(text);
        self.scroll.reveal(line as u64 * lh as u64, lh as u64);
        let inner = (self.text_rect().width as f32 - 2.0 * PADDING as f32).max(1.0);
        if x - self.hoffset > inner {
            self.hoffset = x - inner;
        } else if x < self.hoffset {
            self.hoffset = x;
        }
        self.hoffset = self.hoffset.max(0.0);
    }

    /// Paint the area. `text` is the caller's current text.
    pub fn paint(&mut self, surface: &RasterSurface, text: &str) -> Result<(), RenderError> {
        self.sync(text);
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
        let area = self.text_rect();
        let lh = self.line_height();
        let (caret_line, caret_x) = self.caret_position(text);
        let first = ((self.scroll.offset() / lh as u64) as usize).min(line_count(text) - 1);
        let visible = area.height as usize / lh as usize + 2;
        let left = self.text_left();
        let top = area.y + PADDING - self.scroll.offset() as i32;
        let selection = self.selection();
        let ink = if self.enabled {
            colors.ink
        } else {
            colors.ink_disabled
        };
        let space = measure(" ", &self.font).width;
        surface.with_clip(inset(area, 2), || {
            let skipped: usize = text.split('\n').take(first).map(|l| l.len() + 1).sum();
            let mut start = skipped;
            for (n, line) in text[skipped..].split('\n').take(visible).enumerate() {
                let line_no = first + n;
                let y = top + line_no as i32 * lh;
                let end = start + line.len();
                if let Some((s, e)) = selection {
                    if s <= end && e > start {
                        let x0 = if s > start {
                            self.x_in_line(line, s - start)
                        } else {
                            0.0
                        };
                        let x1 = if e <= end {
                            self.x_in_line(line, e - start)
                        } else {
                            // The selection includes this line's newline.
                            self.x_in_line(line, line.len()) + space
                        };
                        if x1 > x0 {
                            surface.fill_rect(
                                Rect {
                                    x: (left + x0).round() as i32,
                                    y,
                                    width: (x1 - x0).round().max(1.0) as u32,
                                    height: lh as u32,
                                },
                                if self.focused {
                                    colors.selection
                                } else {
                                    colors.selection_inactive
                                },
                            )?;
                        }
                    }
                }
                if !line.is_empty() {
                    surface.draw_text(line, left.round() as i32, y, &self.font, ink)?;
                }
                if self.focused && self.enabled && selection.is_none() && line_no == caret_line {
                    surface.fill_rect(
                        Rect {
                            x: (left + caret_x).round() as i32,
                            y,
                            width: 1,
                            height: lh as u32,
                        },
                        ink,
                    )?;
                }
                start = end + 1;
            }
            Ok(())
        })?;
        self.scroll.paint(surface)
    }

    /// The id of the `TextRun` child that carries the text's units. The top
    /// bit is set so it cannot collide with any control id.
    fn run_node_id(&self) -> u64 {
        self.id | (1 << 63)
    }

    /// The area's accessibility nodes: the text input, whose value is the
    /// whole text and whose text selection is the caret or selected range,
    /// and one `TextRun` child that gives the selection its positions. The
    /// caller adds both to its tree; the run is nested, so it is not a root.
    pub fn accessibility_nodes(&self, text: &str) -> Vec<(u64, accesskit::Node)> {
        let run_id = self.run_node_id();
        let lengths = accessibility_unit_lengths(text);
        let position = |byte: usize| accesskit::TextPosition {
            node: accesskit::NodeId(run_id),
            character_index: accessibility_unit_index(&lengths, byte),
        };
        let focus = position(self.caret);
        let anchor = self.anchor.map_or(focus, position);

        let mut node = accesskit::Node::new(accesskit::Role::MultilineTextInput);
        node.set_label(self.label.as_str());
        node.set_value(text);
        node.set_bounds(super::bounds_of(self.bounds));
        node.set_children(vec![accesskit::NodeId(run_id)]);
        node.set_text_selection(accesskit::TextSelection { anchor, focus });
        if self.enabled {
            node.add_action(accesskit::Action::Focus);
            if !self.read_only {
                node.add_action(accesskit::Action::SetValue);
            }
        } else {
            node.set_disabled();
        }

        let mut run = accesskit::Node::new(accesskit::Role::TextRun);
        run.set_value(text);
        run.set_character_lengths(lengths);
        vec![(self.id, node), (run_id, run)]
    }

    pub fn focus_node(&self) -> u64 {
        self.id
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

/// Normalise pasted or typed text: CR and CRLF become LF, and control
/// characters other than newline and tab are dropped.
pub fn clean(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The number of lines: one more than the number of newlines.
pub fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

/// The byte range of `line` without its newline. A line past the end gives
/// the last line.
pub fn line_range(text: &str, line: usize) -> (usize, usize) {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(newline) => start += newline + 1,
            None => break,
        }
    }
    (start, line_end(text, start))
}

/// The line number and start offset of the line holding byte `index`.
fn line_of(text: &str, index: usize) -> (usize, usize) {
    let before = &text[..floor_index(text, index)];
    let line = before.matches('\n').count();
    let start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (line, start)
}

/// The offset of the newline that ends the line starting at `start`, or the
/// text's length.
fn line_end(text: &str, start: usize) -> usize {
    text[start..]
        .find('\n')
        .map_or(text.len(), |newline| start + newline)
}

/// Move `index` back to a char boundary within the text.
pub fn floor_index(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// The start of the grapheme cluster before `from`. A base letter and its
/// combining marks are one unit, so one Backspace removes them together.
fn prev_char(text: &str, from: usize) -> usize {
    let from = floor_index(text, from);
    text[..from]
        .graphemes(true)
        .next_back()
        .map_or(0, |cluster| from - cluster.len())
}

/// The end of the grapheme cluster after `from`.
fn next_char(text: &str, from: usize) -> usize {
    text[from..]
        .graphemes(true)
        .next()
        .map_or(from, |cluster| from + cluster.len())
}

/// The lengths, in UTF-8 bytes, of the units that assistive technology sees:
/// the grapheme clusters the caret moves over. AccessKit stores each length
/// in a `u8`, so a cluster longer than 255 bytes is split into its scalar
/// values for that view only.
fn accessibility_unit_lengths(text: &str) -> Vec<u8> {
    let mut lengths = Vec::new();
    for cluster in text.graphemes(true) {
        if cluster.len() <= usize::from(u8::MAX) {
            lengths.push(cluster.len() as u8);
        } else {
            lengths.extend(cluster.chars().map(|c| c.len_utf8() as u8));
        }
    }
    lengths
}

/// The number of units that start before byte `byte`: the unit index of a
/// caret there. A caret inside a unit counts that unit as passed.
fn accessibility_unit_index(lengths: &[u8], byte: usize) -> usize {
    let mut start = 0;
    let mut index = 0;
    for &length in lengths {
        if start >= byte {
            break;
        }
        start += usize::from(length);
        index += 1;
    }
    index
}

fn prev_word(text: &str, from: usize) -> usize {
    let trimmed = text[..from].trim_end_matches(|c: char| !c.is_alphanumeric());
    trimmed
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(0, |(index, c)| index + c.len_utf8())
}

fn next_word(text: &str, from: usize) -> usize {
    let after = &text[from..];
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;
    use crate::foundation::geometry::Scale;

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn ctrl(c: char) -> Event {
        Event::KeyPressed {
            key: Key::Character(c),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        }
    }

    fn with_shift(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        }
    }

    fn with_control(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers {
                control: true,
                ..Modifiers::default()
            },
        }
    }

    fn focused(text: &str, caret: usize) -> TextArea {
        let mut area = TextArea::new(7, "Document").with_bounds(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 120,
        });
        area.focused = true;
        area.place_caret(text, caret);
        area
    }

    fn edit(outcome: TextAreaOutcome) -> TextEdit {
        match outcome {
            TextAreaOutcome::Edit(edit) => edit,
            other => panic!("expected an edit, got {other:?}"),
        }
    }

    #[test]
    fn typing_inserts_at_the_caret_and_moves_it() {
        let mut area = focused("ab", 1);
        let result = edit(area.handle("ab", &key(Key::Character('X'))));
        assert_eq!(
            result,
            TextEdit {
                start: 1,
                end: 1,
                inserted: "X".into()
            }
        );
        assert_eq!(area.caret(), 2);
    }

    #[test]
    fn enter_splits_a_line_and_tab_inserts_a_tab() {
        let mut area = focused("abc", 1);
        let split = edit(area.handle("abc", &key(Key::Enter)));
        assert_eq!(split.inserted, "\n");
        assert_eq!(area.caret(), 2);
        let mut area = focused("abc", 0);
        assert_eq!(edit(area.handle("abc", &key(Key::Tab))).inserted, "\t");
    }

    #[test]
    fn backspace_at_a_line_start_removes_the_newline() {
        let mut area = focused("ab\ncd", 3);
        let result = edit(area.handle("ab\ncd", &key(Key::Backspace)));
        assert_eq!(
            result,
            TextEdit {
                start: 2,
                end: 3,
                inserted: String::new()
            }
        );
        assert_eq!(area.caret(), 2);
    }

    #[test]
    fn delete_and_backspace_at_the_ends_do_nothing() {
        let mut area = focused("ab", 0);
        assert_eq!(
            area.handle("ab", &key(Key::Backspace)),
            TextAreaOutcome::Ignored
        );
        let mut area = focused("ab", 2);
        assert_eq!(
            area.handle("ab", &key(Key::Delete)),
            TextAreaOutcome::Ignored
        );
    }

    #[test]
    fn up_and_down_keep_the_column_across_short_lines() {
        let text = "abcd\nxy\nqwer";
        let mut area = focused(text, 3);
        area.handle(text, &key(Key::Down));
        // "xy" is too short for column 3: the caret goes to its end.
        assert_eq!(area.caret(), 7);
        area.handle(text, &key(Key::Down));
        assert_eq!(
            area.caret(),
            11,
            "the goal column comes back on the long line"
        );
        area.handle(text, &key(Key::Up));
        area.handle(text, &key(Key::Up));
        assert_eq!(area.caret(), 3);
    }

    #[test]
    fn up_on_the_first_line_goes_to_the_start_and_down_on_the_last_to_the_end() {
        let text = "one\ntwo";
        let mut area = focused(text, 5);
        area.handle(text, &key(Key::Up));
        assert_eq!(area.caret(), 1);
        area.handle(text, &key(Key::Up));
        assert_eq!(area.caret(), 0);
        area.handle(text, &key(Key::Down));
        area.handle(text, &key(Key::Down));
        assert_eq!(area.caret(), text.len());
    }

    #[test]
    fn home_end_and_document_ends() {
        let text = "one\ntwo";
        let mut area = focused(text, 5);
        area.handle(text, &key(Key::End));
        assert_eq!(area.caret(), 7);
        area.handle(text, &key(Key::Home));
        assert_eq!(area.caret(), 4);
        area.handle(text, &with_control(Key::Home));
        assert_eq!(area.caret(), 0);
        area.handle(text, &with_control(Key::End));
        assert_eq!(area.caret(), 7);
    }

    #[test]
    fn shift_arrows_select_and_copy_returns_the_selection() {
        let text = "hello";
        let mut area = focused(text, 0);
        for _ in 0..3 {
            area.handle(text, &with_shift(Key::Right));
        }
        assert_eq!(area.selection(), Some((0, 3)));
        assert_eq!(
            area.handle(text, &ctrl('c')),
            TextAreaOutcome::Copy("hel".into())
        );
        // A plain arrow collapses the selection to its end.
        area.handle(text, &key(Key::Right));
        assert_eq!(area.selection(), None);
        assert_eq!(area.caret(), 3);
    }

    #[test]
    fn cut_removes_the_selection_and_returns_its_text() {
        let text = "hello";
        let mut area = focused(text, 0);
        area.select(text, 0, 3);
        match area.handle(text, &ctrl('x')) {
            TextAreaOutcome::Cut { text: cut, edit } => {
                assert_eq!(cut, "hel");
                assert_eq!(
                    edit,
                    TextEdit {
                        start: 0,
                        end: 3,
                        inserted: String::new()
                    }
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(area.caret(), 0);
    }

    #[test]
    fn typing_replaces_the_selection() {
        let text = "hello";
        let mut area = focused(text, 0);
        area.select(text, 1, 4);
        let result = edit(area.handle(text, &key(Key::Character('i'))));
        assert_eq!((result.start, result.end), (1, 4));
        assert_eq!(result.caret_after(), 2);
    }

    #[test]
    fn paste_requests_clipboard_then_inserts_normalised_text() {
        let mut area = focused("", 0);
        assert_eq!(area.handle("", &ctrl('v')), TextAreaOutcome::PasteRequested);
        let result = edit(area.handle("", &Event::Paste("a\r\nb\rc".into())));
        assert_eq!(result.inserted, "a\nb\nc");
    }

    #[test]
    fn read_only_refuses_edits_but_allows_copy_and_movement() {
        let text = "abc";
        let mut area = focused(text, 0);
        area.read_only = true;
        assert_eq!(
            area.handle(text, &key(Key::Character('x'))),
            TextAreaOutcome::Ignored
        );
        assert_eq!(area.handle(text, &ctrl('v')), TextAreaOutcome::Ignored);
        area.handle(text, &key(Key::End));
        assert_eq!(area.caret(), 3);
        area.select_all(text);
        assert_eq!(
            area.handle(text, &ctrl('c')),
            TextAreaOutcome::Copy("abc".into())
        );
    }

    #[test]
    fn edits_that_would_exceed_max_len_are_refused() {
        let mut area = focused("abc", 3);
        area.max_len = 3;
        assert_eq!(
            area.handle("abc", &key(Key::Character('d'))),
            TextAreaOutcome::Ignored
        );
        // Replacing a selection does not grow the text.
        area.select("abc", 0, 1);
        let result = edit(area.handle("abc", &key(Key::Character('z'))));
        assert_eq!(result.inserted, "z");
    }

    #[test]
    fn unfocused_areas_ignore_keys() {
        let mut area = TextArea::new(1, "Document");
        assert_eq!(
            area.handle("abc", &key(Key::Character('x'))),
            TextAreaOutcome::Ignored
        );
    }

    #[test]
    fn pointer_places_the_caret_on_the_clicked_line() {
        let text = "alpha\nbeta";
        let mut area = focused(text, 0);
        let lh = area.line_height();
        let y = area.text_rect().y + PADDING + lh + lh / 2;
        let x = area.text_rect().x + PADDING;
        let outcome = area.handle(
            text,
            &Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
            },
        );
        assert_eq!(outcome, TextAreaOutcome::Changed);
        assert_eq!(area.caret(), 6, "start of 'beta'");
        area.handle(
            text,
            &Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
            },
        );
    }

    #[test]
    fn a_press_in_an_unfocused_area_asks_for_focus() {
        let text = "abc";
        let mut area = TextArea::new(1, "Document").with_bounds(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 120,
        });
        let outcome = area.handle(
            text,
            &Event::PointerPressed {
                x: 10,
                y: 10,
                button: PointerButton::Primary,
            },
        );
        assert_eq!(outcome, TextAreaOutcome::FocusRequested);
    }

    #[test]
    fn caret_movement_scrolls_the_view_to_the_caret() {
        let text: String = (0..40).map(|n| format!("line {n}\n")).collect();
        let mut area = focused(&text, 0);
        area.bounds.height = 60;
        area.handle(&text, &with_control(Key::End));
        let lh = area.line_height() as u64;
        let line = line_count(&text) as u64 - 1;
        let offset = area.scroll_offset();
        assert!(offset > 0, "the view scrolled down");
        assert!(offset <= line * lh);
        assert!(line * lh + lh <= offset + area.scroll.viewport());
    }

    #[test]
    fn page_down_moves_by_a_viewport_of_lines() {
        let text: String = (0..60).map(|n| format!("{n}\n")).collect();
        let mut area = focused(&text, 0);
        area.bounds.height = 60;
        let page = area.bounds.height as usize / area.line_height() as usize;
        area.handle(&text, &key(Key::PageDown));
        let (line, _) = area.caret_position(&text);
        assert_eq!(line, page, "moved one viewport of lines");
        area.handle(&text, &key(Key::PageUp));
        assert_eq!(area.caret_position(&text).0, 0);
    }

    #[test]
    fn scroll_wheel_scrolls_without_moving_the_caret() {
        let text: String = (0..60).map(|n| format!("{n}\n")).collect();
        let mut area = focused(&text, 0);
        area.bounds.height = 60;
        area.hover_for_test(&text);
        let outcome = area.handle(&text, &Event::Scroll { dx: 0, dy: 3 });
        assert_eq!(outcome, TextAreaOutcome::Changed);
        assert!(area.scroll_offset() > 0);
        assert_eq!(area.caret(), 0);
    }

    #[test]
    fn multi_byte_text_moves_by_characters() {
        let text = "é€x";
        let mut area = focused(text, 0);
        area.handle(text, &key(Key::Right));
        assert_eq!(area.caret(), 2);
        area.handle(text, &key(Key::Right));
        assert_eq!(area.caret(), 5);
        area.handle(text, &key(Key::Backspace));
        assert_eq!(area.caret(), 2);
    }

    #[test]
    fn accessibility_node_exposes_the_whole_text_as_the_value() {
        let area = TextArea::new(9, "Document");
        let nodes = area.accessibility_nodes("one\ntwo");
        let (id, node) = &nodes[0];
        assert_eq!(*id, 9);
        assert_eq!(node.role(), accesskit::Role::MultilineTextInput);
        assert_eq!(node.value(), Some("one\ntwo"));
        assert_eq!(node.label(), Some("Document"));
        assert_eq!(area.focus_node(), 9);
    }

    #[test]
    fn caller_edits_are_reflected_after_sync() {
        // The caller replaced the text out of band; the caret stays on a
        // char boundary inside the new text.
        let mut area = focused("héllo", 2);
        let outcome = area.handle("h", &key(Key::Character('!')));
        let result = edit(outcome);
        assert!(result.start <= 1);
        assert!(result.end <= 1);
    }

    #[test]
    fn clean_normalises_newlines_and_drops_control_characters() {
        assert_eq!(clean("a\r\nb\rc\u{7}d\te"), "a\nb\ncd\te");
    }

    #[test]
    fn line_helpers_handle_empty_and_past_the_end_lines() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a\n\nb"), 3);
        assert_eq!(line_range("a\n\nb", 1), (2, 2));
        assert_eq!(line_range("a\n\nb", 9), (3, 4));
        assert_eq!(line_of("ab\ncd", 4), (1, 3));
    }

    #[test]
    fn paint_shows_focus_and_caret_and_survives_malformed_geometry() {
        let text = "hi\nthere";
        let scale = Scale::new(1).unwrap();
        let surface = RasterSurface::new(200, 120, scale).unwrap();
        let mut unfocused = TextArea::new(1, "Document").with_bounds(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 120,
        });
        unfocused.paint(&surface, text).unwrap();
        let plain = surface.data().to_vec();

        let surface = RasterSurface::new(200, 120, scale).unwrap();
        let mut focused_area = focused(text, 1);
        focused_area.paint(&surface, text).unwrap();
        assert_ne!(
            surface.data().to_vec(),
            plain,
            "focus ring and caret are drawn"
        );

        // Zero-sized and off-surface bounds must not panic.
        for bounds in [
            Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            Rect {
                x: -500,
                y: -500,
                width: 4,
                height: 2,
            },
        ] {
            let mut area = focused(text, 3).with_bounds(bounds);
            let _ = area.paint(&surface, text);
        }
    }

    #[test]
    fn a_base_letter_and_its_combining_marks_move_and_delete_as_one() {
        // "e" + U+0301 COMBINING ACUTE ACCENT is one character, three bytes.
        let text = "e\u{301}x";
        let mut area = focused(text, 0);
        area.handle(text, &key(Key::Right));
        assert_eq!(area.caret(), 3, "one Right passes the whole cluster");
        let removed = edit(area.handle(text, &key(Key::Backspace)));
        assert_eq!(
            (removed.start, removed.end, removed.inserted.as_str()),
            (0, 3, "")
        );
        assert_eq!(area.caret(), 0);
    }

    #[test]
    fn a_multi_scalar_emoji_is_one_character_for_selection_and_delete() {
        // Thumbs up with a skin-tone modifier: two scalars, eight bytes, one cluster.
        // The caret sits after the emoji, before the "!".
        let text = "\u{1F44D}\u{1F3FD}!";
        let mut area = focused(text, 8);
        area.handle(text, &with_shift(Key::Left));
        assert_eq!(
            area.selection(),
            Some((0, 8)),
            "Shift+Left over the emoji takes the whole cluster"
        );
        assert_eq!(area.selected_text(text), Some("\u{1F44D}\u{1F3FD}"));
        match area.handle(text, &ctrl('x')) {
            TextAreaOutcome::Cut { text: cut, edit } => {
                assert_eq!(cut, "\u{1F44D}\u{1F3FD}");
                assert_eq!(edit.caret_after(), 0);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn selection_over_accented_and_cjk_text_stays_on_char_boundaries() {
        let text = "日本é";
        let mut area = focused(text, 0);
        for _ in 0..2 {
            area.handle(text, &with_shift(Key::Right));
        }
        assert_eq!(area.selection(), Some((0, 6)));
        assert_eq!(area.selected_text(text), Some("日本"));
        let result = edit(area.handle(text, &key(Key::Character('ö'))));
        assert_eq!((result.start, result.end), (0, 6));
        assert_eq!(result.inserted, "ö");
        assert_eq!(area.caret(), 2);
    }

    #[test]
    fn forward_delete_removes_a_whole_cluster() {
        let text = "a\u{301}b";
        let mut area = focused(text, 0);
        let removed = edit(area.handle(text, &key(Key::Delete)));
        assert_eq!((removed.start, removed.end), (0, 3));
    }

    #[test]
    fn accessibility_nodes_expose_the_caret_and_selection_as_unit_positions() {
        // "a" (1 byte), "日" (3 bytes), "e" + U+0301 (3 bytes): three units.
        let text = "a日e\u{301}";
        let mut area = focused(text, 0);
        area.select(text, 1, 4);
        let nodes = area.accessibility_nodes(text);
        let (_, input) = &nodes[0];
        let (run_id, run) = &nodes[1];
        assert_eq!(input.role(), accesskit::Role::MultilineTextInput);
        assert_eq!(input.children(), &[accesskit::NodeId(*run_id)]);
        assert_eq!(run.role(), accesskit::Role::TextRun);
        assert_eq!(run.value(), Some(text));
        assert_eq!(run.character_lengths(), &[1, 3, 3]);
        let selection = input.text_selection().expect("a text selection");
        assert_eq!(selection.anchor.node, accesskit::NodeId(*run_id));
        assert_eq!(selection.anchor.character_index, 1);
        assert_eq!(selection.focus.character_index, 2);

        // With no selection, the caret is both ends, and at the end of the text it is the unit count.
        area.place_caret(text, text.len());
        let nodes = area.accessibility_nodes(text);
        let selection = nodes[0].1.text_selection().unwrap();
        assert_eq!(selection.anchor, selection.focus);
        assert_eq!(selection.focus.character_index, 3);
    }

    #[test]
    fn accessibility_units_sum_to_the_text_length_even_for_huge_clusters() {
        // One base letter with 130 combining marks: a 261-byte cluster that
        // a u8 length cannot hold, so it is split into scalar values for AT.
        let text = format!("a{}", "\u{301}".repeat(130));
        let lengths = accessibility_unit_lengths(&text);
        assert_eq!(lengths.len(), 131);
        assert_eq!(
            lengths.iter().map(|&n| usize::from(n)).sum::<usize>(),
            text.len()
        );
        assert!(lengths.iter().all(|&n| n == 1 || n == 2));
        let mut area = focused(&text, text.len());
        area.place_caret(&text, text.len());
        let node = &area.accessibility_nodes(&text)[0].1;
        assert_eq!(node.text_selection().unwrap().focus.character_index, 131);
    }

    /// The ARGB value of an opaque palette colour, as `RasterSurface::pixel` reports it.
    fn argb(color: crate::foundation::color::Color) -> u32 {
        0xff00_0000
            | u32::from(color.red) << 16
            | u32::from(color.green) << 8
            | u32::from(color.blue)
    }

    #[test]
    fn classic_frame_focus_ring_and_caret_are_painted_at_1x_and_2x() {
        let colors = palette();
        for factor in [1u32, 2] {
            let scale = Scale::new(factor).unwrap();
            // Logical 200 x 120 at this scale. The caret sits at the text's left edge.
            let surface = RasterSurface::new(200, 120, scale).unwrap();
            surface.clear(colors.content).unwrap();
            let mut area = focused("", 0).with_bounds(Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 120,
            });
            area.paint(&surface, "").unwrap();
            let f = factor;
            // The outer border is the shadow colour, the inner ring is focus black.
            assert_eq!(
                surface.pixel(0, 0),
                Some(argb(colors.bevel_shadow)),
                "{f}x border"
            );
            assert_eq!(
                surface.pixel(f, f),
                Some(argb(colors.focus)),
                "{f}x focus ring"
            );
            // The caret: a one-logical-pixel column at the text's left edge, centred on the line.
            let left = 4;
            let middle = 4 + area.line_height() as u32 / 2;
            assert_eq!(
                surface.pixel(left * f, middle * f),
                Some(argb(colors.ink)),
                "{f}x caret"
            );
            // Away from the caret the content is the content colour.
            assert_eq!(
                surface.pixel(100 * f, 60 * f),
                Some(argb(colors.content)),
                "{f}x content"
            );
        }
    }

    #[test]
    fn the_caret_is_absent_when_unfocused_and_the_selection_paints_at_1x_and_2x() {
        let colors = palette();
        let text = "abc";
        for factor in [1u32, 2] {
            let scale = Scale::new(factor).unwrap();
            let surface = RasterSurface::new(200, 120, scale).unwrap();
            surface.clear(colors.content).unwrap();
            let mut area = TextArea::new(1, "Document").with_bounds(Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 120,
            });
            area.paint(&surface, text).unwrap();
            let f = factor;
            let left = 4;
            let middle = 4 + area.line_height() as u32 / 2;
            assert_eq!(
                surface.pixel(left * f, middle * f),
                Some(argb(colors.content)),
                "{f}x no caret when unfocused"
            );

            // Select all three letters, then sample a point inside the box
            // above the glyphs and one just past the selection's right edge.
            let surface = RasterSurface::new(200, 120, scale).unwrap();
            surface.clear(colors.content).unwrap();
            let mut area = focused(text, 0).with_bounds(Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 120,
            });
            area.select_all(text);
            area.paint(&surface, text).unwrap();
            let width = measure(text, &area.font).width;
            let inside = (left as u32 + 1) * f;
            let top = 4 + 1;
            assert_eq!(
                surface.pixel(inside, top * f),
                Some(argb(colors.selection)),
                "{f}x selection highlight"
            );
            let past = left as u32 + width + 2;
            assert_eq!(
                surface.pixel(past * f, top * f),
                Some(argb(colors.content)),
                "{f}x past the selection"
            );
        }
    }

    impl TextArea {
        /// Test helper: mark the area hovered so wheel events reach it.
        fn hover_for_test(&mut self, text: &str) {
            let x = self.text_rect().x + 10;
            let y = self.text_rect().y + 10;
            self.handle(text, &Event::PointerMoved { x, y });
        }
    }
}
