//! The Fonts window: the installed faces with their owner, a preview in the
//! selected face, and the user's own fonts: install from a path, remove
//! (confirmed by a second press), and refresh the cache.
//!
//! The catalogue is read once when the window opens and after each change.

use std::path::{Path, PathBuf};

use accesskit::TreeUpdate;
use slopos_ui::accessibility::{text_label, window_tree};
use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::dialog::wrap;
use slopos_ui::controls::list::{ListOutcome, ListView};
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{AccessibilityRequest, Event, Key};
use slopos_ui::foundation::geometry::Rect;
use slopos_ui::platform::{App, Control};
use slopos_ui::primitives::button_plate::ButtonPlateVariant;
use slopos_ui::render::{RasterSurface, RenderError};
use slopos_ui::text::{Font, FontFamily};
use slopos_ui::theme::palette;

use crate::catalog::{self, Face, FontSource, Owner};

pub const WIDTH: u32 = 680;
pub const HEIGHT: u32 = 440;

const MARGIN: i32 = 12;
const LIST_WIDTH: u32 = 280;
const TOP: i32 = 12;
const FIELD_HEIGHT: u32 = 24;
const ROW_HEIGHT: u32 = 18;
const BUTTON_HEIGHT: u32 = 26;
const BODY_LINE: i32 = 16;
const SAMPLE: &str = "The quick brown fox jumps over the lazy dog 0123456789";

const SEARCH_NODE: u64 = 2;
const LIST_NODE: u64 = 3;
const PATH_NODE: u64 = 4;
const INSTALL_NODE: u64 = 5;
const REFRESH_NODE: u64 = 6;
const REMOVE_NODE: u64 = 7;
const DETAILS_NODE: u64 = 8;
const STATUS_NODE: u64 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    List,
    Path,
    Install,
    Refresh,
    Remove,
}

const FOCUS_ORDER: [Focus; 6] = [
    Focus::Search,
    Focus::List,
    Focus::Path,
    Focus::Install,
    Focus::Refresh,
    Focus::Remove,
];

pub struct FontsApp {
    source: Box<dyn FontSource>,
    user_dir: PathBuf,
    faces: Vec<Face>,
    /// Indices into `faces` that match the search text.
    shown: Vec<usize>,
    search: TextField,
    list: ListView,
    path: TextField,
    install: Button,
    refresh: Button,
    remove: Button,
    focus: Focus,
    /// A user font whose removal waits for the second press.
    pending_remove: Option<PathBuf>,
    status: String,
    status_is_error: bool,
    body: Font,
    title: Font,
}

impl FontsApp {
    pub fn new(source: Box<dyn FontSource>, user_dir: PathBuf) -> Self {
        let right = MARGIN + LIST_WIDTH as i32 + 12;
        let list_top = TOP + BUTTON_HEIGHT as i32 + 8;
        let bottom_top = HEIGHT as i32 - MARGIN - BUTTON_HEIGHT as i32;

        let mut search = TextField::new(SEARCH_NODE, "Search fonts").with_bounds(Rect {
            x: MARGIN,
            y: TOP,
            width: LIST_WIDTH,
            height: FIELD_HEIGHT,
        });
        search.focused = true;
        let mut list = ListView::new(LIST_NODE, "Installed fonts", ROW_HEIGHT);
        list.set_bounds(Rect {
            x: MARGIN,
            y: list_top,
            width: LIST_WIDTH,
            height: (bottom_top - 8 - list_top) as u32,
        });
        let path = TextField::new(PATH_NODE, "Font file to install").with_bounds(Rect {
            x: MARGIN,
            y: bottom_top,
            width: LIST_WIDTH,
            height: BUTTON_HEIGHT,
        });
        let button = |id: u64, label: &str, bounds: Rect| {
            Button::new(id, label, ButtonPlateVariant::Regular).with_bounds(bounds)
        };
        let mut app = Self {
            source,
            user_dir,
            faces: Vec::new(),
            shown: Vec::new(),
            search,
            list,
            path,
            install: button(
                INSTALL_NODE,
                "Install",
                Rect {
                    x: MARGIN + LIST_WIDTH as i32 + 12,
                    y: bottom_top,
                    width: 96,
                    height: BUTTON_HEIGHT,
                },
            ),
            refresh: button(
                REFRESH_NODE,
                "Refresh cache",
                Rect {
                    x: right,
                    y: TOP,
                    width: 120,
                    height: BUTTON_HEIGHT,
                },
            ),
            remove: button(
                REMOVE_NODE,
                "Remove",
                Rect {
                    x: right + 128,
                    y: TOP,
                    width: 96,
                    height: BUTTON_HEIGHT,
                },
            ),
            focus: Focus::Search,
            pending_remove: None,
            status: String::new(),
            status_is_error: false,
            body: Font::system(12.0),
            title: Font::system(15.0).bold(),
        };
        app.reload();
        app
    }

    /// Reads the faces again and keeps the search and selection sensible.
    fn reload(&mut self) {
        match self.source.faces() {
            Ok(faces) => {
                self.faces = faces;
                self.set_status(format!("{} faces found.", self.faces.len()), false);
            }
            Err(error) => {
                self.faces = Vec::new();
                self.set_status(error, true);
            }
        }
        self.refilter();
    }

    fn refilter(&mut self) {
        let query = self.search.text().trim().to_lowercase();
        self.shown = self
            .faces
            .iter()
            .enumerate()
            .filter(|(_, face)| {
                query.is_empty()
                    || face.family.to_lowercase().contains(&query)
                    || face.style.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        self.list.reset(self.shown.len());
        if !self.shown.is_empty() {
            self.list.select_only(0);
        }
        self.pending_remove = None;
        self.sync_buttons();
    }

    fn selected(&self) -> Option<&Face> {
        let row = self.list.cursor()?;
        self.faces.get(*self.shown.get(row)?)
    }

    fn sync_buttons(&mut self) {
        self.remove.enabled = self
            .selected()
            .is_some_and(|face| face.owner == Owner::User);
    }

    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = text.into();
        self.status_is_error = is_error;
    }

    fn set_focus(&mut self, focus: Focus) -> bool {
        if self.focus == focus {
            return false;
        }
        self.focus = focus;
        self.search.focused = focus == Focus::Search;
        self.list.focused = focus == Focus::List;
        self.path.focused = focus == Focus::Path;
        self.install.focused = focus == Focus::Install;
        self.refresh.focused = focus == Focus::Refresh;
        self.remove.focused = focus == Focus::Remove;
        true
    }

    fn cycle_focus(&mut self, forward: bool) {
        let index = FOCUS_ORDER
            .iter()
            .position(|focus| *focus == self.focus)
            .unwrap_or(0);
        let count = FOCUS_ORDER.len();
        let next = if forward {
            (index + 1) % count
        } else {
            (index + count - 1) % count
        };
        self.set_focus(FOCUS_ORDER[next]);
    }

    fn install_from_path(&mut self) {
        let typed = self.path.text().trim().to_owned();
        if typed.is_empty() {
            self.set_status("Type the path of a font file, then press Install.", true);
            return;
        }
        match catalog::install(Path::new(&typed), &self.user_dir) {
            Ok(target) => {
                let refreshed = self.source.refresh();
                self.path.set_text("");
                self.reload();
                match refreshed {
                    Ok(()) => self.set_status(format!("Installed {}.", target.display()), false),
                    Err(error) => self.set_status(
                        format!(
                            "Installed {}, but the cache was not refreshed: {error}",
                            target.display()
                        ),
                        true,
                    ),
                }
            }
            Err(error) => self.set_status(error, true),
        }
    }

    fn remove_selected(&mut self) {
        let Some(face) = self.selected().cloned() else {
            return;
        };
        if face.owner != Owner::User {
            self.set_status("System fonts cannot be removed here.", true);
            return;
        }
        if self.pending_remove.as_ref() != Some(&face.file) {
            self.set_status(
                format!("Press Remove again to delete {}.", face.file.display()),
                true,
            );
            self.pending_remove = Some(face.file);
            return;
        }
        self.pending_remove = None;
        match catalog::remove(&face.file, &self.user_dir) {
            Ok(()) => {
                let refreshed = self.source.refresh();
                self.reload();
                if let Err(error) = refreshed {
                    self.set_status(
                        format!("Removed, but the cache was not refreshed: {error}"),
                        true,
                    );
                } else {
                    self.set_status(format!("Removed {}.", face.file.display()), false);
                }
            }
            Err(error) => self.set_status(error, true),
        }
    }

    fn refresh_cache(&mut self) {
        match self.source.refresh() {
            Ok(()) => {
                self.reload();
                self.set_status("The font cache was refreshed.", false);
            }
            Err(error) => self.set_status(error, true),
        }
    }

    fn row_label(&self, row: usize) -> String {
        self.shown
            .get(row)
            .and_then(|index| self.faces.get(*index))
            .map(row_text)
            .unwrap_or_default()
    }

    fn details(&self) -> Vec<String> {
        match self.selected() {
            Some(face) => vec![
                format!("Family: {}", face.family),
                format!("Style: {}", face.style),
                format!("File: {}", face.file.display()),
                format!(
                    "Owner: {}",
                    match face.owner {
                        Owner::System => "system (not removable here)",
                        Owner::User => "you (installed in your font folder)",
                    }
                ),
            ],
            None => vec!["No font selected.".to_owned()],
        }
    }

    fn paint_preview(&self, surface: &RasterSurface, area: Rect) -> Result<(), RenderError> {
        let colors = palette();
        let Some(face) = self.selected() else {
            return Ok(());
        };
        let preview = Font {
            family: FontFamily::Named(face.family.clone()),
            size: 22.0,
            bold: false,
        };
        let width = area.width.saturating_sub(16).max(1);
        let mut y = area.y + 8;
        for line in wrap(SAMPLE, &preview, width) {
            surface.draw_text(&line, area.x + 8, y, &preview, colors.ink)?;
            y += 30;
        }
        Ok(())
    }
}

fn row_text(face: &Face) -> String {
    let owner = match face.owner {
        Owner::System => "system",
        Owner::User => "yours",
    };
    format!("{}, {}    {}", face.family, face.style, owner)
}

impl App for FontsApp {
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
            Event::Accessibility(AccessibilityRequest::Focus(id)) => {
                let focus = match *id {
                    SEARCH_NODE => Focus::Search,
                    LIST_NODE => Focus::List,
                    PATH_NODE => Focus::Path,
                    INSTALL_NODE => Focus::Install,
                    REFRESH_NODE => Focus::Refresh,
                    REMOVE_NODE => Focus::Remove,
                    _ => return,
                };
                if self.set_focus(focus) {
                    control.request_redraw();
                }
            }
            Event::Accessibility(AccessibilityRequest::Click(id)) => match *id {
                INSTALL_NODE => self.install_from_path(),
                REFRESH_NODE => self.refresh_cache(),
                REMOVE_NODE => self.remove_selected(),
                _ => {}
            },
            _ => {
                match self.search.handle(event) {
                    TextFieldOutcome::Edited => self.refilter(),
                    TextFieldOutcome::FocusRequested => {
                        self.set_focus(Focus::Search);
                    }
                    _ => {}
                }
                match self.path.handle(event) {
                    TextFieldOutcome::Submitted => self.install_from_path(),
                    TextFieldOutcome::FocusRequested => {
                        self.set_focus(Focus::Path);
                    }
                    _ => {}
                }
                let modifiers = control.modifiers();
                let shown = &self.shown;
                let faces = &self.faces;
                match self.list.handle(event, modifiers, &|row| {
                    shown
                        .get(row)
                        .and_then(|index| faces.get(*index))
                        .map(row_text)
                        .unwrap_or_default()
                }) {
                    ListOutcome::SelectionChanged | ListOutcome::Activated(_) => {
                        self.pending_remove = None;
                        self.sync_buttons();
                    }
                    ListOutcome::FocusRequested => {
                        self.set_focus(Focus::List);
                    }
                    _ => {}
                }
                if self.install.handle(event) == ButtonOutcome::Activated {
                    self.set_focus(Focus::Install);
                    self.install_from_path();
                }
                if self.refresh.handle(event) == ButtonOutcome::Activated {
                    self.set_focus(Focus::Refresh);
                    self.refresh_cache();
                }
                if self.remove.handle(event) == ButtonOutcome::Activated {
                    self.set_focus(Focus::Remove);
                    self.remove_selected();
                }
            }
        }
        // Every event here comes from the user; repaint after handling it.
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        let colors = palette();
        let _ = surface.clear(colors.surface);
        let _ = self.search.paint(surface);
        let _ = self.path.paint(surface);
        let window = &*self;
        let _ = window.list.paint(surface, |surface, row, rect, selected| {
            let ink = if selected {
                palette().selection_ink
            } else {
                palette().ink
            };
            let text = window.row_label(row);
            surface.draw_text(&text, rect.x + 6, rect.y + 1, &window.body, ink)
        });
        let _ = self.install.paint(surface, &self.body);
        let _ = self.refresh.paint(surface, &self.body);
        let _ = self.remove.paint(surface, &self.body);

        let panel = Rect {
            x: MARGIN + LIST_WIDTH as i32 + 12,
            y: TOP + BUTTON_HEIGHT as i32 + 8,
            width: WIDTH - (2 * MARGIN as u32 + LIST_WIDTH + 12),
            height: (HEIGHT as i32
                - 2 * MARGIN
                - BUTTON_HEIGHT as i32
                - 8
                - BUTTON_HEIGHT as i32
                - 8) as u32,
        };
        let _ = surface.fill_rect(panel, colors.content);
        let _ = surface.stroke_rect(panel, colors.border);
        let _ = surface.draw_text("Preview", panel.x + 8, panel.y + 6, &self.title, colors.ink);
        let preview_area = Rect {
            x: panel.x,
            y: panel.y + 26,
            width: panel.width,
            height: 120,
        };
        let _ = self.paint_preview(surface, preview_area);
        let mut y = panel.y + 150;
        for line in self.details() {
            for wrapped in wrap(&line, &self.body, panel.width.saturating_sub(16).max(1)) {
                let _ = surface.draw_text(&wrapped, panel.x + 8, y, &self.body, colors.ink);
                y += BODY_LINE;
            }
            y += 4;
        }
        let status_ink = if self.status_is_error {
            colors.danger
        } else {
            colors.ink
        };
        let _ = surface.with_clip(
            Rect {
                x: MARGIN + LIST_WIDTH as i32 + 12 + 108,
                y: HEIGHT as i32 - MARGIN - BUTTON_HEIGHT as i32,
                width: WIDTH - (2 * MARGIN as u32 + LIST_WIDTH + 12 + 108),
                height: BUTTON_HEIGHT,
            },
            || {
                surface.draw_text(
                    &self.status,
                    MARGIN + LIST_WIDTH as i32 + 12 + 108,
                    HEIGHT as i32 - MARGIN - BUTTON_HEIGHT as i32 + 6,
                    &self.body,
                    status_ink,
                )
            },
        );
    }

    fn accessibility(&self) -> Option<TreeUpdate> {
        let mut nodes = vec![
            (SEARCH_NODE, self.search.accessibility_node()),
            (PATH_NODE, self.path.accessibility_node()),
            (INSTALL_NODE, self.install.accessibility_node()),
            (REFRESH_NODE, self.refresh.accessibility_node()),
            (REMOVE_NODE, self.remove.accessibility_node()),
        ];
        nodes.extend(self.list.accessibility_nodes(&|row| self.row_label(row)));
        nodes.push((DETAILS_NODE, text_label(self.details().join("\n"))));
        nodes.push((STATUS_NODE, text_label(self.status.clone())));
        let focus = match self.focus {
            Focus::Search => SEARCH_NODE,
            Focus::List => self.list.focus_node(),
            Focus::Path => PATH_NODE,
            Focus::Install => INSTALL_NODE,
            Focus::Refresh => REFRESH_NODE,
            Focus::Remove => REMOVE_NODE,
        };
        Some(window_tree("Fonts", nodes, Some(focus)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Faces from memory. A face whose file is gone is dropped on the next read,
    /// the way fontconfig stops listing a removed font.
    struct FakeFonts {
        faces: Vec<Face>,
        refreshes: Arc<AtomicUsize>,
    }

    impl FontSource for FakeFonts {
        fn faces(&self) -> Result<Vec<Face>, String> {
            Ok(self
                .faces
                .iter()
                .filter(|face| face.file.exists())
                .cloned()
                .collect())
        }

        fn refresh(&self) -> Result<(), String> {
            self.refreshes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    fn face(family: &str, file: PathBuf, owner: Owner) -> Face {
        Face {
            family: family.to_owned(),
            style: "Regular".to_owned(),
            file,
            owner,
        }
    }

    fn window(faces: Vec<Face>, user_dir: &Path) -> (FontsApp, Arc<AtomicUsize>) {
        let refreshes = Arc::new(AtomicUsize::new(0));
        let source = FakeFonts {
            faces,
            refreshes: Arc::clone(&refreshes),
        };
        (
            FontsApp::new(Box::new(source), user_dir.to_owned()),
            refreshes,
        )
    }

    fn click(app: &mut FontsApp, node: u64) {
        app.event(
            &Event::Accessibility(AccessibilityRequest::Click(node)),
            &mut Control::default(),
        );
    }

    fn texts(app: &FontsApp) -> Vec<String> {
        app.accessibility()
            .expect("a tree")
            .nodes
            .iter()
            .filter_map(|(_, node)| node.value().or(node.label()).map(str::to_owned))
            .collect()
    }

    #[test]
    fn rows_say_who_owns_each_face() {
        let dir = tempfile::tempdir().unwrap();
        let user_file = dir.path().join("Mine.otf");
        std::fs::write(&user_file, "x").unwrap();
        let system_file = dir.path().join("Sys.ttf");
        std::fs::write(&system_file, "y").unwrap();
        let (app, _) = window(
            vec![
                face("Mine", user_file, Owner::User),
                face("Sys", system_file, Owner::System),
            ],
            dir.path(),
        );
        let rows = texts(&app);
        assert!(
            rows.iter().any(|row| row == "Mine, Regular    yours"),
            "{rows:?}"
        );
        assert!(
            rows.iter().any(|row| row == "Sys, Regular    system"),
            "{rows:?}"
        );
    }

    #[test]
    fn search_narrows_the_faces() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.ttf");
        std::fs::write(&file, "x").unwrap();
        let other = dir.path().join("b.ttf");
        std::fs::write(&other, "y").unwrap();
        let (mut app, _) = window(
            vec![
                face("Alpha", file, Owner::System),
                face("Beta", other, Owner::System),
            ],
            dir.path(),
        );
        assert_eq!(app.shown.len(), 2);
        for c in "beta".chars() {
            app.event(
                &Event::KeyPressed {
                    key: Key::Character(c),
                    modifiers: Default::default(),
                },
                &mut Control::default(),
            );
        }
        assert_eq!(app.shown.len(), 1);
        assert_eq!(app.selected().map(|f| f.family.as_str()), Some("Beta"));
    }

    #[test]
    fn a_system_font_is_never_removed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Sys.ttf");
        std::fs::write(&file, "y").unwrap();
        let (mut app, _) = window(vec![face("Sys", file.clone(), Owner::System)], dir.path());
        click(&mut app, REMOVE_NODE);
        assert!(
            app.status().contains("System fonts cannot be removed"),
            "{}",
            app.status()
        );
        assert!(file.exists());
    }

    #[test]
    fn removing_a_user_font_takes_two_presses_and_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let user_dir = dir.path().join("fonts");
        std::fs::create_dir_all(&user_dir).unwrap();
        let file = user_dir.join("Mine.otf");
        std::fs::write(&file, "x").unwrap();
        let (mut app, refreshes) = window(vec![face("Mine", file.clone(), Owner::User)], &user_dir);
        assert!(app.remove.enabled);

        click(&mut app, REMOVE_NODE);
        assert!(
            app.status().starts_with("Press Remove again"),
            "{}",
            app.status()
        );
        assert!(file.exists());

        click(&mut app, REMOVE_NODE);
        assert!(!file.exists());
        assert_eq!(refreshes.load(Ordering::Relaxed), 1);
        assert!(app.faces.is_empty());
    }

    #[test]
    fn install_refuses_a_file_that_is_not_a_font() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("notes.txt");
        std::fs::write(&text, "hi").unwrap();
        let (mut app, _) = window(Vec::new(), &dir.path().join("fonts"));
        app.path.set_text(text.to_str().unwrap());
        click(&mut app, INSTALL_NODE);
        assert!(app.status_is_error);
        assert!(app.status().contains("not a font file"), "{}", app.status());
    }

    #[test]
    fn paints_at_1x_and_2x_and_the_faces_change_the_picture() {
        use slopos_ui::foundation::geometry::Scale;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.ttf");
        std::fs::write(&file, "x").unwrap();
        for factor in [1, 2] {
            let scale = Scale::new(factor).unwrap();
            let listed = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let (mut app, _) = window(vec![face("Alpha", file.clone(), Owner::System)], dir.path());
            app.paint(&listed);
            let empty = RasterSurface::new(WIDTH, HEIGHT, scale).unwrap();
            let (mut none, _) = window(Vec::new(), dir.path());
            none.paint(&empty);
            assert_eq!(listed.device_width(), WIDTH * factor);
            assert_ne!(listed.data().to_vec(), empty.data().to_vec(), "{factor}x");
        }
    }
}
