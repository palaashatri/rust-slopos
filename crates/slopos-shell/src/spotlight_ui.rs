//! Spotlight search UI — rendering and input handling for the search overlay.
//!
//! Pixels are produced by the SDK canvas path (`draw_widget`) walking
//! [`SpotlightUI`] widgets exposed via [`ShellDesktop::children`]. Kit
//! `Widget::draw` stubs are intentionally unused.

use crate::spotlight::{SearchResult, Spotlight};
use slopos_kit::event::{KeyCode, Modifiers};
use slopos_kit::list_view::ListView;
use slopos_kit::panel::Panel;
use slopos_kit::text_field::TextField;
use slopos_kit::{EventResult, Rect, Widget};

/// Spotlight search UI state — owns the spotlight logic + drawable widgets.
pub struct SpotlightUI {
    /// The search backend and state machine.
    pub spotlight: Spotlight,
    /// Full-screen dimming scrim (behind the card).
    pub(crate) scrim: Panel,
    /// Raised card behind the search field + results.
    pub(crate) card: Panel,
    /// Text input field for the search query.
    pub(crate) search_field: TextField,
    /// List view for displaying search results.
    pub(crate) results_list: ListView,
    /// Current search results.
    current_results: Vec<SearchResult>,
    /// Index of the currently selected result (keyboard navigation).
    selected_index: usize,
}

impl SpotlightUI {
    /// Create a new Spotlight UI.
    pub fn new() -> Self {
        Self {
            spotlight: Spotlight::new(),
            scrim: Panel::scrim(),
            card: Panel::card(),
            search_field: TextField::new().with_placeholder("Search apps, files, settings..."),
            results_list: ListView::new(),
            current_results: Vec::new(),
            selected_index: 0,
        }
    }

    /// Check if the overlay is visible.
    pub fn is_visible(&self) -> bool {
        self.spotlight.is_visible()
    }

    /// Show the overlay (invoked on Super+Space).
    pub fn show(&mut self) {
        self.spotlight.show();
        self.search_field.set_text("");
        self.search_field.widget_state_mut().focused = true;
        self.current_results.clear();
        self.selected_index = 0;
        self.sync_widgets();
    }

    /// Hide the overlay.
    pub fn hide(&mut self) {
        self.spotlight.hide();
        self.search_field.set_text("");
        self.search_field.widget_state_mut().focused = false;
        self.current_results.clear();
        self.selected_index = 0;
        self.sync_widgets();
    }

    /// Update the search results based on current query and available apps.
    pub fn update_results(&mut self, apps: &[crate::launch_services::AppBundle]) {
        self.current_results = self.spotlight.search_results(apps);
        self.selected_index = 0;
        self.sync_widgets();
    }

    /// Keep drawable widgets in sync with search state.
    fn sync_widgets(&mut self) {
        self.search_field.set_text(self.spotlight.query());
        self.results_list.items = self
            .current_results
            .iter()
            .map(|r| {
                if let Some(desc) = r.description() {
                    format!("{} — {}", r.display_name(), desc)
                } else {
                    r.display_name()
                }
            })
            .collect();
        self.results_list.selected_index = if self.current_results.is_empty() {
            None
        } else {
            Some(self.selected_index)
        };
    }

    /// Get the currently selected result, if any.
    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.current_results.get(self.selected_index)
    }

    /// Append a character to the search query.
    pub fn append_char(&mut self, c: char) {
        self.spotlight.append_char(c);
        self.sync_widgets();
    }

    /// Handle a keyboard event (for the search UI overlay).
    /// Returns `EventResult::Handled` if the event was processed.
    pub fn handle_overlay_key(&mut self, key: KeyCode, _modifiers: &Modifiers) -> EventResult {
        match key {
            KeyCode::Escape => {
                self.hide();
                EventResult::Handled
            }
            KeyCode::Enter => {
                // Activation: user selected a result
                // (actual launch/open would be done by caller)
                EventResult::Handled
            }
            KeyCode::ArrowUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.sync_widgets();
                }
                EventResult::Handled
            }
            KeyCode::ArrowDown => {
                if self.selected_index < self.current_results.len().saturating_sub(1) {
                    self.selected_index += 1;
                    self.sync_widgets();
                }
                EventResult::Handled
            }
            KeyCode::Backspace => {
                self.spotlight.backspace();
                self.sync_widgets();
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }

    /// Position scrim/card/field/list for the given screen size.
    pub fn layout_for_screen(&mut self, screen_w: f32, screen_h: f32) {
        self.scrim.set_rect(Rect::new(0.0, 0.0, screen_w, screen_h));

        let width = (screen_w * 0.55).clamp(420.0, 720.0);
        let height = (screen_h * 0.45).clamp(260.0, 420.0);
        let x = ((screen_w - width) / 2.0).max(0.0);
        let y = (screen_h * 0.18).min(screen_h - height).max(40.0);
        self.card.set_rect(Rect::new(x, y, width, height));

        let padding = 16.0;
        let field_height = 36.0;
        self.search_field.set_rect(Rect::new(
            x + padding,
            y + padding,
            width - padding * 2.0,
            field_height,
        ));

        let list_y = y + padding + field_height + 12.0;
        let list_height = (height - (padding * 2.0 + field_height + 12.0)).max(40.0);
        self.results_list.set_rect(Rect::new(
            x + padding,
            list_y,
            width - padding * 2.0,
            list_height,
        ));
    }

    /// Drawable widgets in paint order (scrim → card → field → list).
    pub fn paint_widgets(&self) -> [&dyn Widget; 4] {
        [
            &self.scrim as &dyn Widget,
            &self.card as &dyn Widget,
            &self.search_field as &dyn Widget,
            &self.results_list as &dyn Widget,
        ]
    }

    /// Mutable drawable widgets (for update walks).
    pub fn paint_widgets_mut(&mut self) -> [&mut dyn Widget; 4] {
        [
            &mut self.scrim as &mut dyn Widget,
            &mut self.card as &mut dyn Widget,
            &mut self.search_field as &mut dyn Widget,
            &mut self.results_list as &mut dyn Widget,
        ]
    }

    /// Get the current search query string.
    pub fn query(&self) -> &str {
        self.spotlight.query()
    }

    /// Get the current search results.
    pub fn results(&self) -> &[SearchResult] {
        &self.current_results
    }

    /// Get the index of the selected result.
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

impl Default for SpotlightUI {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spotlight_ui_visibility() {
        let mut ui = SpotlightUI::new();
        assert!(!ui.is_visible());

        ui.show();
        assert!(ui.is_visible());

        ui.hide();
        assert!(!ui.is_visible());
    }

    #[test]
    fn keyboard_navigation() {
        let mut ui = SpotlightUI::new();
        ui.show();

        let results = vec![
            SearchResult::Setting {
                category: "Display".to_string(),
                title: "Brightness".to_string(),
            },
            SearchResult::Setting {
                category: "Sound".to_string(),
                title: "Volume".to_string(),
            },
        ];
        ui.current_results = results;
        ui.selected_index = 0;
        ui.sync_widgets();

        let modifiers = Modifiers {
            shift: false,
            control: false,
            alt: false,
            meta: false,
        };

        ui.handle_overlay_key(KeyCode::ArrowDown, &modifiers);
        assert_eq!(ui.selected_index, 1);
        assert_eq!(ui.results_list.selected_index, Some(1));

        ui.handle_overlay_key(KeyCode::ArrowUp, &modifiers);
        assert_eq!(ui.selected_index, 0);

        let result = ui.handle_overlay_key(KeyCode::Escape, &modifiers);
        match result {
            EventResult::Handled => {}
            _ => panic!("Expected Handled"),
        }
        assert!(!ui.is_visible());
    }

    #[test]
    fn layout_places_card_on_screen() {
        let mut ui = SpotlightUI::new();
        ui.show();
        ui.layout_for_screen(1280.0, 800.0);
        assert!(ui.scrim.rect().width >= 1280.0);
        assert!(ui.card.rect().width >= 420.0);
        assert!(ui.search_field.rect().width > 0.0);
        assert!(ui.results_list.rect().height > 0.0);
    }
}
