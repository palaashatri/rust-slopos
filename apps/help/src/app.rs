//! The Help window: a search field, the topic list, and the topic's text in
//! a scrolling panel. Everything shown comes from [`crate::topics`].

use accesskit::TreeUpdate;
use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::dialog::wrap;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::scroll::{ScrollOutcome, ScrollState, WHEEL_STEP};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::Font;
use slopos_ui::theme::palette;

use crate::topics::{self, Block, Topic};

pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 480;

const MARGIN: i32 = 12;
/// Width of the content panel's scroll bar.
const BAR_WIDTH: i32 = 12;
const SEARCH_HEIGHT: u32 = 24;
const LIST_WIDTH: u32 = 196;
const ROW_HEIGHT: u32 = 18;
const PADDING: i32 = 10;
const BODY_LINE: i32 = 16;
const HEADING_LINE: i32 = 22;
const CODE_LINE: i32 = 14;
const BLOCK_GAP: i32 = 6;
/// Most text lines exposed to assistive technology at once.
const MAX_LINE_NODES: usize = 200;

const SEARCH_NODE: u64 = 2;
const LIST_NODE: u64 = 3;
const CONTENT_NODE: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Topics,
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Body,
    Heading,
    Code,
}

/// One drawn line of a topic, positioned in the panel's scrolled content.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    y: i32,
    text: String,
    style: Style,
}

pub struct HelpApp {
    topics: Vec<Topic>,
    search: TextField,
    list: ListView,
    /// Indices into `topics` of the rows the list shows.
    shown: Vec<usize>,
    current: Option<usize>,
    lines: Vec<Line>,
    content_height: i32,
    scroll: ScrollState,
    panel: Rect,
    focus: Focus,
    body: Font,
    heading: Font,
    code: Font,
}

impl HelpApp {
    /// Opens the window with the bundled topics. `topic` is a topic id to show
    /// first (a deep link); an unknown id shows the first topic.
    pub fn new(topic: Option<&str>) -> Self {
        Self::with_topics(topics::bundled(), topic)
    }

    pub fn with_topics(topics: Vec<Topic>, topic: Option<&str>) -> Self {
        let mut search = TextField::new(SEARCH_NODE, "Search help").with_bounds(Rect {
            x: MARGIN,
            y: MARGIN,
            width: LIST_WIDTH,
            height: SEARCH_HEIGHT,
        });
        search.focused = true;
        let mut list = ListView::new(LIST_NODE, "Topics", ROW_HEIGHT);
        list.set_bounds(Rect {
            x: MARGIN,
            y: MARGIN + SEARCH_HEIGHT as i32 + 8,
            width: LIST_WIDTH,
            height: HEIGHT - (2 * MARGIN as u32 + SEARCH_HEIGHT + 8),
        });
        let panel = Rect {
            x: MARGIN + LIST_WIDTH as i32 + 10,
            y: MARGIN,
            width: WIDTH - (2 * MARGIN as u32 + LIST_WIDTH + 10),
            height: HEIGHT - 2 * MARGIN as u32,
        };
        let mut app = Self {
            topics,
            search,
            list,
            shown: Vec::new(),
            current: None,
            lines: Vec::new(),
            content_height: 0,
            scroll: ScrollState::new(panel),
            panel,
            focus: Focus::Search,
            body: Font::system(12.0),
            heading: Font::system(13.0).bold(),
            code: Font::monospace(11.0),
        };
        app.refilter();
        if let Some(index) = topic.and_then(|id| topics::find(&app.topics, id)) {
            app.show_topic(index);
        }
        app
    }

    /// The topic on screen, by id.
    pub fn current_id(&self) -> Option<&'static str> {
        self.current.map(|index| self.topics[index].id)
    }

    /// Rebuilds the list for the search text and shows the first match.
    fn refilter(&mut self) {
        self.shown = topics::search(&self.topics, self.search.text());
        self.list.reset(self.shown.len());
        if let Some(first) = self.shown.first().copied() {
            self.list.select_only(0);
            self.show_topic(first);
        } else {
            self.current = None;
            self.lines.clear();
            self.content_height = 0;
            self.scroll.set_content(0);
        }
    }

    fn show_topic(&mut self, index: usize) {
        self.current = Some(index);
        if let Some(row) = self.shown.iter().position(|shown| *shown == index) {
            self.list.select_without_scrolling(row);
        }
        self.layout();
        self.scroll.scroll_to(0);
    }

    /// Wraps the current topic into lines for the panel's width.
    fn layout(&mut self) {
        self.lines.clear();
        let mut y = PADDING;
        let Some(index) = self.current else {
            self.content_height = 0;
            self.scroll.set_content(0);
            return;
        };
        let width = self.text_width();
        let blocks = self.topics[index].blocks.clone();
        for block in &blocks {
            let (text, style, line_height) = match block {
                Block::Heading(text) => (text.clone(), Style::Heading, HEADING_LINE),
                Block::Text(text) => (text.clone(), Style::Body, BODY_LINE),
                Block::Item(text) => (format!("\u{2022} {text}"), Style::Body, BODY_LINE),
                Block::Code(text) => (text.clone(), Style::Code, CODE_LINE),
            };
            let font = match style {
                Style::Body => &self.body,
                Style::Heading => &self.heading,
                Style::Code => &self.code,
            };
            for line in wrap(&text, font, width) {
                self.lines.push(Line {
                    y,
                    text: line,
                    style,
                });
                y += line_height;
            }
            y += BLOCK_GAP;
        }
        self.content_height = y + PADDING;
        self.scroll.set_content(self.content_height as u64);
    }

    /// The area the topic's text is drawn in: the panel less its bar.
    fn inner(&self) -> Rect {
        Rect {
            x: self.panel.x,
            y: self.panel.y,
            width: (self.panel.width as i32 - BAR_WIDTH).max(1) as u32,
            height: self.panel.height,
        }
    }

    /// The width text may take: the inner area less the padding on each side.
    fn text_width(&self) -> u32 {
        (self.inner().width as i32 - 2 * PADDING).max(1) as u32
    }

    fn set_focus(&mut self, focus: Focus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        self.search.focused = focus == Focus::Search;
        self.list.focused = focus == Focus::Topics;
        true
    }

    fn cycle_focus(&mut self, forward: bool) {
        let next = match (self.focus, forward) {
            (Focus::Search, true) | (Focus::Content, false) => Focus::Topics,
            (Focus::Topics, true) | (Focus::Search, false) => Focus::Content,
            (Focus::Content, true) | (Focus::Topics, false) => Focus::Search,
        };
        self.set_focus(next);
    }

    /// Keys that scroll the topic when the content has focus.
    fn scroll_key(&mut self, key: &Key) -> bool {
        let page = (self.scroll.viewport() as i64).max(i64::from(BODY_LINE));
        match key {
            Key::Up => self.scroll.scroll_by(-WHEEL_STEP),
            Key::Down => self.scroll.scroll_by(WHEEL_STEP),
            Key::PageUp => self.scroll.scroll_by(-page),
            Key::PageDown => self.scroll.scroll_by(page),
            Key::Home => self.scroll.scroll_to(0),
            Key::End => {
                let last = self.scroll.content().saturating_sub(self.scroll.viewport());
                self.scroll.scroll_to(last)
            }
            _ => return false,
        };
        true
    }

    fn visible_text(&self) -> String {
        let top = self.scroll.offset() as i32;
        let bottom = top + self.scroll.viewport() as i32;
        self.lines
            .iter()
            .filter(|line| line.y + BODY_LINE >= top && line.y <= bottom)
            .take(MAX_LINE_NODES)
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn paint_content(&self, surface: &RasterSurface) -> Result<(), RenderError> {
        let colors = palette();
        let inner = self.inner();
        let top = self.scroll.offset() as i32;
        for line in &self.lines {
            let y = inner.y + line.y - top;
            let height = match line.style {
                Style::Heading => HEADING_LINE,
                Style::Body => BODY_LINE,
                Style::Code => CODE_LINE,
            };
            if y + height < inner.y || y > inner.y + inner.height as i32 {
                continue;
            }
            let (font, ink) = match line.style {
                Style::Heading => (&self.heading, colors.ink),
                Style::Body => (&self.body, colors.ink),
                Style::Code => (&self.code, colors.ink_secondary),
            };
            surface.draw_text(&line.text, inner.x + PADDING, y, font, ink)?;
        }
        Ok(())
    }
}

impl App for HelpApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } => {
                self.cycle_focus(!modifiers.shift);
                control.request_redraw();
            }
            Event::KeyPressed { key, .. } if self.focus == Focus::Content => {
                if self.scroll_key(key) {
                    control.request_redraw();
                }
            }
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                let focus = match *id {
                    SEARCH_NODE => Focus::Search,
                    LIST_NODE => Focus::Topics,
                    CONTENT_NODE => Focus::Content,
                    _ => return,
                };
                if self.set_focus(focus) {
                    control.request_redraw();
                }
            }
            _ => {
                let scrolled = self.scroll.handle(event);
                if scrolled == ScrollOutcome::Scrolled {
                    control.request_redraw();
                    return;
                }
                match self.search.handle(event) {
                    TextFieldOutcome::Edited => {
                        self.refilter();
                        control.request_redraw();
                        return;
                    }
                    TextFieldOutcome::FocusRequested => {
                        self.set_focus(Focus::Search);
                        control.request_redraw();
                        return;
                    }
                    TextFieldOutcome::Changed => {
                        control.request_redraw();
                        return;
                    }
                    _ => {}
                }
                let modifiers = control.modifiers();
                match self.list.handle(event, modifiers, &|row| {
                    self.shown
                        .get(row)
                        .map(|index| self.topics[*index].title.clone())
                        .unwrap_or_default()
                }) {
                    ListOutcome::SelectionChanged | ListOutcome::Activated(_) => {
                        if let Some(row) = self.list.cursor() {
                            if let Some(index) = self.shown.get(row).copied() {
                                self.show_topic(index);
                            }
                        }
                        control.request_redraw();
                    }
                    ListOutcome::FocusRequested => {
                        self.set_focus(Focus::Topics);
                        control.request_redraw();
                    }
                    ListOutcome::Changed => control.request_redraw(),
                    ListOutcome::ContextMenu { .. } | ListOutcome::Ignored => {}
                }
                if matches!(event, Event::PointerPressed { x, y, .. } if self.panel.contains(*x, *y))
                {
                    self.set_focus(Focus::Content);
                    control.request_redraw();
                }
            }
        }
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.surface);
        let _ = self.search.paint(surface);
        let titles: Vec<String> = self
            .shown
            .iter()
            .map(|index| self.topics[*index].title.clone())
            .collect();
        let _ = self.list.paint(surface, |surface, row, rect, selected| {
            let ink = if selected {
                colors.selection_ink
            } else {
                colors.ink
            };
            let Some(title) = titles.get(row) else {
                return Ok(());
            };
            surface.draw_text(title, rect.x + 6, rect.y + 1, &self.body, ink)
        });
        let _ = surface.fill_rect(self.panel, colors.content);
        let _ = surface.stroke_rect(self.panel, colors.border);
        let _ = surface.with_clip(self.inner(), || self.paint_content(surface));
        let _ = self.scroll.paint(surface);
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let titles = |row: usize| {
            self.shown
                .get(row)
                .map(|index| self.topics[*index].title.clone())
                .unwrap_or_default()
        };
        let mut nodes = vec![(SEARCH_NODE, self.search.accessibility_node())];
        nodes.extend(self.list.accessibility_nodes(&titles));
        let mut content = text_label(self.visible_text());
        content.set_label(match self.current_id() {
            Some(id) => format!("Help topic {id}"),
            None => "No topic".to_owned(),
        });
        nodes.push((CONTENT_NODE, content));
        let focus = match self.focus {
            Focus::Search => SEARCH_NODE,
            Focus::Topics => self.list.focus_node(),
            Focus::Content => CONTENT_NODE,
        };
        Some(window_tree("Help", nodes, Some(focus)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topics::parse;
    use slopos_ui::foundation::geometry::Scale;

    fn key(app: &mut HelpApp, key: Key) {
        app.event(
            &Event::KeyPressed {
                key,
                modifiers: Default::default(),
            },
            &mut Control::default(),
        );
    }

    fn type_text(app: &mut HelpApp, text: &str) {
        for c in text.chars() {
            key(app, Key::Character(c));
        }
    }

    fn long_topic() -> Topic {
        let mut markdown = String::from("# Long\n\n");
        for number in 0..200 {
            markdown.push_str(&format!(
                "Paragraph {number} with enough words to wrap across the panel width.\n\n"
            ));
        }
        parse("long", &markdown)
    }

    #[test]
    fn a_deep_link_opens_that_topic() {
        let app = HelpApp::new(Some("architecture"));
        assert_eq!(app.current_id(), Some("architecture"));
    }

    #[test]
    fn an_unknown_deep_link_opens_the_first_topic() {
        let app = HelpApp::new(Some("no-such-topic"));
        assert_eq!(app.current_id(), Some("readme"));
    }

    #[test]
    fn searching_narrows_the_list_and_an_empty_match_clears_the_text() {
        let mut app = HelpApp::with_topics(
            vec![
                parse("a", "# Alpha\n\nApples.\n"),
                parse("b", "# Beta\n\nBananas.\n"),
            ],
            None,
        );
        assert_eq!(app.shown, vec![0, 1]);
        type_text(&mut app, "banana");
        assert_eq!(app.shown, vec![1]);
        assert_eq!(app.current_id(), Some("b"));
        type_text(&mut app, "zzz");
        assert!(app.shown.is_empty());
        assert_eq!(app.current_id(), None);
        assert!(app.lines.is_empty());
    }

    #[test]
    fn tab_then_down_moves_to_the_next_topic() {
        let mut app = HelpApp::with_topics(
            vec![parse("a", "# A\n\nfirst\n"), parse("b", "# B\n\nsecond\n")],
            None,
        );
        assert_eq!(app.current_id(), Some("a"));
        key(&mut app, Key::Tab);
        assert_eq!(app.focus, Focus::Topics);
        key(&mut app, Key::Down);
        assert_eq!(app.current_id(), Some("b"));
        key(&mut app, Key::Up);
        assert_eq!(app.current_id(), Some("a"));
    }

    #[test]
    fn the_content_scrolls_with_keys_when_it_has_focus() {
        let mut app = HelpApp::with_topics(vec![long_topic()], None);
        key(&mut app, Key::Tab);
        key(&mut app, Key::Tab);
        assert_eq!(app.focus, Focus::Content);
        assert!(app.scroll.scrollable(), "a long topic must scroll");
        key(&mut app, Key::PageDown);
        assert!(app.scroll.offset() > 0);
        key(&mut app, Key::End);
        assert_eq!(
            app.scroll.offset(),
            app.scroll.content() - app.scroll.viewport()
        );
        key(&mut app, Key::Home);
        assert_eq!(app.scroll.offset(), 0);
    }

    #[test]
    fn lines_are_wrapped_inside_the_panel_and_go_down_the_page() {
        let app = HelpApp::with_topics(vec![long_topic()], None);
        assert!(app.lines.len() > 200, "{} lines", app.lines.len());
        let width = app.text_width();
        assert!(width > 100 && width < WIDTH, "{width}");
        assert!(app.lines.windows(2).all(|pair| pair[0].y < pair[1].y));
        assert!(app.content_height > app.panel.height as i32);
    }

    #[test]
    fn the_topic_text_is_named_for_assistive_technology() {
        let app = HelpApp::new(Some("development"));
        let update = app.accessibility().expect("a tree");
        let labelled = update
            .nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Help topic development"));
        assert!(labelled);
    }

    #[test]
    fn paints_at_1x_and_2x_and_the_topic_changes_the_picture() {
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let shown = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let mut app = HelpApp::new(Some("readme"));
            app.paint(&shown);
            assert_eq!(shown.device_width(), WIDTH * factor);
            assert_eq!(shown.device_height(), HEIGHT * factor);

            let empty = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            HelpApp::with_topics(Vec::new(), None).paint(&empty);
            assert_ne!(shown.data().to_vec(), empty.data().to_vec(), "{factor}x");
        }
    }
}
