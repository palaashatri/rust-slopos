//! Find Applications (Super+Space): type to search installed applications,
//! Up/Down to choose, Enter to launch, Escape to close. The search is a
//! plain ranking over desktop entries, tested without a display; the UI is
//! a toolkit text field and list drawn in a popup.

use slopos_core::desktop_entry::DesktopApp;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{Event, Key, Modifiers};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{measure, Font};
use slopos_ui::theme::palette;

pub const WIDTH: u32 = 520;
pub const HEIGHT: u32 = 360;
const FIELD_ID: u64 = 500;
const LIST_ID: u64 = 510;
const ROW_HEIGHT: u32 = 36;
/// Results shown at most; the rest are a narrower search away.
const MAX_RESULTS: usize = 50;

/// How well `query` matches `app`: higher is better, `None` is no match.
pub fn score(app: &DesktopApp, query: &str) -> Option<u32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(1);
    }
    let name = app.name.to_lowercase();
    let words_start = name
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(&query));
    if name == query {
        Some(1000)
    } else if name.starts_with(&query) {
        Some(800)
    } else if words_start {
        Some(600)
    } else if name.contains(&query) {
        Some(400)
    } else if app.comment.to_lowercase().contains(&query) || app.id.to_lowercase().contains(&query)
    {
        Some(200)
    } else if is_subsequence(&query, &name) {
        Some(100)
    } else {
        None
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|c| chars.any(|h| h == c))
}

/// Matching apps, best first, then alphabetically.
pub fn search(apps: &[DesktopApp], query: &str) -> Vec<usize> {
    let mut ranked: Vec<(u32, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(index, app)| score(app, query).map(|score| (score, index)))
        .collect();
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            apps[a.1]
                .name
                .to_lowercase()
                .cmp(&apps[b.1].name.to_lowercase())
        })
    });
    ranked.truncate(MAX_RESULTS);
    ranked.into_iter().map(|(_, index)| index).collect()
}

/// What the launcher wants the bar to do after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    Repaint,
    Close,
    Launch(usize),
}

pub struct Launcher {
    pub apps: Vec<DesktopApp>,
    pub field: TextField,
    pub list: ListView,
    pub results: Vec<usize>,
    font: Font,
}

impl Launcher {
    pub fn new(apps: Vec<DesktopApp>) -> Self {
        let mut field = TextField::new(FIELD_ID, "Search applications");
        field.bounds = Rect {
            x: 12,
            y: 34,
            width: WIDTH - 24,
            height: 22,
        };
        field.focused = true;
        let mut list = ListView::new(LIST_ID, "Applications", ROW_HEIGHT);
        list.set_bounds(Rect {
            x: 12,
            y: 64,
            width: WIDTH - 24,
            height: HEIGHT - 64 - 30,
        });
        let mut launcher = Self {
            apps,
            field,
            list,
            results: Vec::new(),
            font: Font::system(12.0),
        };
        launcher.refresh();
        launcher
    }

    fn refresh(&mut self) {
        self.results = search(&self.apps, self.field.text());
        self.list.reset(self.results.len());
        if !self.results.is_empty() {
            self.list.select_only(0);
        }
    }

    /// Handle input in launcher coordinates.
    pub fn handle(&mut self, event: &Event, modifiers: Modifiers) -> Outcome {
        if let Event::KeyPressed { key, .. } = event {
            match key {
                Key::Escape => return Outcome::Close,
                Key::Enter => {
                    return self
                        .list
                        .cursor()
                        .and_then(|row| self.results.get(row).copied())
                        .map_or(Outcome::None, Outcome::Launch)
                }
                // Arrows move through results while typing.
                Key::Up | Key::Down | Key::PageUp | Key::PageDown => {
                    let names = |_: usize| String::new();
                    self.list.focused = true;
                    self.list.handle(event, modifiers, &names);
                    self.list.focused = false;
                    return Outcome::Repaint;
                }
                _ => {}
            }
        }
        // A press outside the launcher closes it.
        if let Event::PointerPressed { x, y, .. } = event {
            if *x < 0 || *y < 0 || *x >= WIDTH as i32 || *y >= HEIGHT as i32 {
                return Outcome::Close;
            }
        }
        match self.field.handle(event) {
            TextFieldOutcome::Edited => {
                self.refresh();
                return Outcome::Repaint;
            }
            TextFieldOutcome::Ignored => {}
            _ => return Outcome::Repaint,
        }
        let names: Vec<String> = self
            .results
            .iter()
            .map(|&index| self.apps[index].name.clone())
            .collect();
        let label = |row: usize| names.get(row).cloned().unwrap_or_default();
        match self.list.handle(event, modifiers, &label) {
            ListOutcome::Activated(row) => self
                .results
                .get(row)
                .copied()
                .map_or(Outcome::None, Outcome::Launch),
            ListOutcome::Ignored => Outcome::None,
            _ => Outcome::Repaint,
        }
    }

    pub fn paint(
        &mut self,
        surface: &RasterSurface,
        icon: &mut dyn FnMut(&str) -> Option<slopos_ui::image::Image>,
    ) -> Result<(), RenderError> {
        let colors = palette();
        let bounds = Rect {
            x: 0,
            y: 0,
            width: WIDTH,
            height: HEIGHT,
        };
        surface.fill_rect(bounds, colors.surface)?;
        surface.stroke_rect(bounds, colors.border)?;
        let bold = self.font.clone().bold();
        surface.draw_text("Find Applications", 12, 10, &bold, colors.ink)?;
        self.field.paint(surface)?;
        let icons: Vec<Option<slopos_ui::image::Image>> = self
            .list
            .visible_rows()
            .map(|row| {
                self.results
                    .get(row)
                    .and_then(|&index| icon(&self.apps[index].icon))
            })
            .collect();
        let first = self.list.visible_rows().start;
        let apps = &self.apps;
        let results = &self.results;
        let font = &self.font;
        let line = measure("Ag", font).height as i32;
        self.list.paint(surface, |surface, row, rect, selected| {
            let app = &apps[results[row]];
            if let Some(Some(image)) = icons.get(row - first) {
                surface.draw_image_scaled(image, rect.x + 6, rect.y + 4, 28, 28)?;
            }
            let (ink, secondary) = if selected {
                (colors.selection_ink, colors.selection_ink)
            } else {
                (colors.ink, colors.ink_secondary)
            };
            surface.draw_text(&app.name, rect.x + 42, rect.y + 3, &bold, ink)?;
            surface.draw_text(
                &app.comment,
                rect.x + 42,
                rect.y + 5 + line,
                font,
                secondary,
            )
        })?;
        let count = match self.results.len() {
            0 => "No matching applications".to_owned(),
            1 => "1 application".to_owned(),
            n => format!("{n} applications"),
        };
        surface.draw_text(&count, 12, HEIGHT as i32 - 22, font, colors.ink_secondary)
    }

    pub fn accessibility_nodes(&self) -> Vec<(u64, accesskit::Node)> {
        let mut nodes = vec![(FIELD_ID, self.field.accessibility_node())];
        let label = |row: usize| {
            self.results
                .get(row)
                .map(|&index| {
                    let app = &self.apps[index];
                    format!("{}, {}", app.name, app.comment)
                })
                .unwrap_or_default()
        };
        nodes.extend(self.list.accessibility_nodes(&label));
        nodes
    }

    pub fn focus_node(&self) -> u64 {
        FIELD_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, comment: &str) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: name.into(),
            argv: vec![id.into()],
            icon: String::new(),
            comment: comment.into(),
            terminal: false,
            startup_wm_class: String::new(),
            mime_types: Vec::new(),
            exec: id.into(),
        }
    }

    fn key(key: Key) -> Event {
        Event::KeyPressed {
            key,
            modifiers: Modifiers::default(),
        }
    }

    #[test]
    fn ranking_prefers_names_then_words_then_comments() {
        let apps = vec![
            app("slopos-files", "Files", "Browse files"),
            app("profiles", "Profile Editor", "Edit profiles"),
            app("slopos-calculator", "Calculator", "Do sums"),
            app("viewer", "Image Viewer", "Look at pictures"),
        ];
        assert_eq!(search(&apps, "files"), vec![0, 1]);
        assert_eq!(search(&apps, "view"), vec![3]);
        assert_eq!(search(&apps, "pictures"), vec![3]);
        assert_eq!(search(&apps, "clc"), vec![2]);
        assert!(search(&apps, "zzz").is_empty());
        // An empty query lists everything alphabetically.
        assert_eq!(search(&apps, ""), vec![2, 0, 3, 1]);
    }

    #[test]
    fn typing_narrows_arrows_move_and_enter_launches() {
        let apps = vec![
            app("a", "Alpha", ""),
            app("b", "Beta", ""),
            app("c", "Alphabet Soup", ""),
        ];
        let mut launcher = Launcher::new(apps);
        let none = Modifiers::default();
        for c in "alp".chars() {
            launcher.handle(&key(Key::Character(c)), none);
        }
        assert_eq!(launcher.results, vec![0, 2]);
        assert_eq!(launcher.handle(&key(Key::Down), none), Outcome::Repaint);
        assert_eq!(launcher.handle(&key(Key::Enter), none), Outcome::Launch(2));
        assert_eq!(launcher.handle(&key(Key::Escape), none), Outcome::Close);
        let outside = Event::PointerPressed {
            x: -5,
            y: 10,
            button: slopos_ui::event::PointerButton::Primary,
        };
        assert_eq!(launcher.handle(&outside, none), Outcome::Close);
    }
}
