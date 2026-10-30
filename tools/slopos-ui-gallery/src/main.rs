//! Component gallery for the SLOPOS toolkit: one window per fixture, and a
//! headless `--snapshot FILE` mode for review and QA captures.

mod widgets;

use slopos_ui::controls::button::{Button, ButtonOutcome};
use slopos_ui::controls::focus::FocusChain;
use slopos_ui::controls::text_field::{TextField, TextFieldOutcome};
use slopos_ui::event::{Event, Key};
use slopos_ui::foundation::color::Color;
use slopos_ui::foundation::geometry::{Rect, Scale};
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::primitives::button_plate::{
    paint_button_plate, ButtonPlateState, ButtonPlateVariant,
};
use slopos_ui::render::RasterSurface;
use slopos_ui::scene::{Command, Scene};
use slopos_ui::scene_raster::replay_to_raster;
use slopos_ui::text::Font;

const USAGE: &str = "usage: slopos-ui-gallery [--component surface|button-plate|buttons|text-field|widgets] [--scale INTEGER] [--snapshot FILE]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GalleryFixture {
    Surface,
    ButtonPlate,
    Buttons,
    TextField,
    Widgets,
}

impl GalleryFixture {
    fn size(self) -> (u32, u32) {
        match self {
            Self::Surface => (160, 90),
            Self::ButtonPlate => (320, 136),
            Self::Buttons => (320, 120),
            Self::TextField => (320, 130),
            Self::Widgets => widgets::SIZE,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Surface => "Surface",
            Self::ButtonPlate => "Button Plates",
            Self::Buttons => "Buttons",
            Self::TextField => "Text Fields",
            Self::Widgets => "Widgets",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GalleryOptions {
    scale: Scale,
    fixture: GalleryFixture,
    snapshot: Option<std::path::PathBuf>,
}

fn parse_arguments(arguments: &[String]) -> Result<GalleryOptions, &'static str> {
    let mut scale = None;
    let mut fixture = None;
    let mut snapshot = None;
    let mut index = 0;
    while index < arguments.len() {
        let value = || arguments.get(index + 1).ok_or(USAGE);
        match arguments[index].as_str() {
            "--scale" if scale.is_none() => {
                let factor = value()?
                    .parse::<u32>()
                    .map_err(|_| "scale must be a positive integer")?;
                if !(1..=4).contains(&factor) {
                    return Err("supported gallery scales are 1 to 4");
                }
                scale = Scale::new(factor);
            }
            "--component" if fixture.is_none() => {
                fixture = Some(match value()?.as_str() {
                    "surface" => GalleryFixture::Surface,
                    "button-plate" => GalleryFixture::ButtonPlate,
                    "buttons" => GalleryFixture::Buttons,
                    "text-field" => GalleryFixture::TextField,
                    "widgets" => GalleryFixture::Widgets,
                    _ => return Err(
                        "supported components are surface, button-plate, buttons, text-field and widgets",
                    ),
                });
            }
            "--snapshot" if snapshot.is_none() => snapshot = Some(value()?.into()),
            "--scale" | "--component" | "--snapshot" => {
                return Err("each option may only be specified once")
            }
            _ => return Err(USAGE),
        }
        index += 2;
    }
    Ok(GalleryOptions {
        scale: scale.unwrap_or_else(platform::scale_from_environment),
        fixture: fixture.unwrap_or(GalleryFixture::Surface),
        snapshot,
    })
}

struct Gallery {
    fixture: GalleryFixture,
    buttons: Vec<Button>,
    fields: Vec<TextField>,
    focus: FocusChain,
    font: Font,
}

impl Gallery {
    fn new(fixture: GalleryFixture) -> Self {
        let mut buttons = Vec::new();
        if fixture == GalleryFixture::Buttons {
            // Row 1: the three variants; row 2: focused and disabled.
            let variants = [
                ("Cancel", ButtonPlateVariant::Regular),
                ("OK", ButtonPlateVariant::Default),
                ("Options", ButtonPlateVariant::Secondary),
            ];
            for (index, (label, variant)) in variants.into_iter().enumerate() {
                buttons.push(
                    Button::new(index as u64 + 1, label, variant).at(16 + index as i32 * 104, 20),
                );
            }
            let mut focused = Button::new(4, "Focused", ButtonPlateVariant::Regular).at(16, 72);
            focused.focused = true;
            let mut disabled = Button::new(5, "Disabled", ButtonPlateVariant::Regular).at(120, 72);
            disabled.enabled = false;
            buttons.extend([focused, disabled]);
        }
        let mut fields = Vec::new();
        if fixture == GalleryFixture::TextField {
            let bounds = |y| Rect {
                x: 16,
                y,
                width: 288,
                height: 22,
            };
            let mut name = TextField::new(10, "Name").with_bounds(bounds(16));
            name.set_text("Résumé – 日本語 draft.txt");
            name.select(0, "Résumé".len());
            name.focused = true;
            let mut search = TextField::new(11, "Search").with_bounds(bounds(52));
            search.placeholder = "Search".into();
            let mut disabled = TextField::new(12, "Disabled").with_bounds(bounds(88));
            disabled.set_text("Read-only value");
            disabled.enabled = false;
            fields.extend([name, search, disabled]);
        }
        let mut focus = FocusChain::new(
            fields
                .iter()
                .filter(|field| field.enabled)
                .map(|field| field.id)
                .collect(),
        );
        if let Some(first) = fields.first() {
            focus.focus(first.id);
        }
        Self {
            fixture,
            buttons,
            fields,
            focus,
            font: Font::system(12.0),
        }
    }

    /// Move focus to the next or previous enabled button, wrapping around.
    fn move_focus(&mut self, forward: bool, control: &mut Control) {
        let enabled: Vec<usize> = self
            .buttons
            .iter()
            .enumerate()
            .filter_map(|(index, button)| button.enabled.then_some(index))
            .collect();
        let Some(first) = enabled.first().copied() else {
            return;
        };
        let current = self.buttons.iter().position(|button| button.focused);
        let next = match (current, forward) {
            (Some(current), true) => enabled
                .iter()
                .copied()
                .find(|index| *index > current)
                .unwrap_or(first),
            (Some(current), false) => enabled
                .iter()
                .rev()
                .copied()
                .find(|index| *index < current)
                .unwrap_or_else(|| *enabled.last().unwrap()),
            (None, true) => first,
            (None, false) => *enabled.last().unwrap(),
        };
        let mut changed = false;
        for (index, button) in self.buttons.iter_mut().enumerate() {
            let focused = index == next;
            if button.focused != focused {
                button.focused = focused;
                changed = true;
            }
        }
        if changed {
            control.request_redraw();
        }
    }
}

impl Gallery {
    fn field_event(&mut self, event: &Event, control: &mut Control) {
        if self.focus.handle(event).is_some() {
            self.sync_focus();
            control.request_redraw();
            return;
        }
        let mut focus_to = None;
        for field in &mut self.fields {
            match field.handle(event) {
                TextFieldOutcome::Ignored => continue,
                TextFieldOutcome::FocusRequested => focus_to = Some(field.id),
                TextFieldOutcome::Copy(text) => control.set_clipboard(&text),
                TextFieldOutcome::PasteRequested => control.request_paste(),
                TextFieldOutcome::Submitted => log::info!("submitted {:?}", field.text()),
                _ => {}
            }
            control.request_redraw();
        }
        if let Some(id) = focus_to {
            self.focus.focus(id);
            self.sync_focus();
        }
    }

    fn sync_focus(&mut self) {
        for field in &mut self.fields {
            field.focused = self.focus.is_focused(field.id);
        }
    }
}

impl App for Gallery {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested
            | Event::KeyPressed {
                key: Key::Escape, ..
            } => control.quit(),
            Event::KeyPressed {
                key: Key::Tab,
                modifiers,
            } if self.fields.is_empty() => {
                self.move_focus(!modifiers.shift, control);
            }
            _ if !self.fields.is_empty() => self.field_event(event, control),
            _ => {
                for button in &mut self.buttons {
                    match button.handle(event) {
                        ButtonOutcome::Activated => {
                            log::info!("activated {}", button.label);
                            control.request_redraw();
                        }
                        ButtonOutcome::Changed => control.request_redraw(),
                        ButtonOutcome::Ignored => {}
                    }
                }
            }
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        if self.fields.is_empty() {
            return None;
        }
        let nodes = self
            .fields
            .iter()
            .map(|field| (field.id, field.accessibility_node()))
            .collect();
        Some(slopos_ui::accessibility::window_tree(
            self.fixture.title(),
            nodes,
            self.focus.focused(),
        ))
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = paint_fixture(self, surface) {
            eprintln!("slopos-ui-gallery: {error}");
        }
    }
}

fn paint_fixture(
    gallery: &mut Gallery,
    surface: &RasterSurface,
) -> Result<(), Box<dyn std::error::Error>> {
    surface.clear(Color::WHITE)?;
    match gallery.fixture {
        GalleryFixture::Surface => {
            let mut scene = Scene::new(1);
            scene.push(Command::FillRect {
                rect: Rect {
                    x: 20,
                    y: 20,
                    width: 80,
                    height: 20,
                },
                color: Color::BLACK,
            })?;
            replay_to_raster(&scene, surface)?;
        }
        GalleryFixture::ButtonPlate => {
            // Rows: Regular, Default, Secondary. Columns: Rest, Pressed, Disabled.
            let variants = [
                ButtonPlateVariant::Regular,
                ButtonPlateVariant::Default,
                ButtonPlateVariant::Secondary,
            ];
            let states = [
                ButtonPlateState::Rest,
                ButtonPlateState::Pressed,
                ButtonPlateState::Disabled,
            ];
            for (row, variant) in variants.into_iter().enumerate() {
                for (column, state) in states.into_iter().enumerate() {
                    let x = 16 + column as i32 * 104;
                    let y = 16 + row as i32 * 40;
                    paint_button_plate(surface, x, y, variant, state)?;
                }
            }
        }
        GalleryFixture::Buttons => {
            for button in &gallery.buttons {
                button.paint(surface, &gallery.font)?;
            }
        }
        // Painted by `widgets::Widgets`, never through `Gallery`.
        GalleryFixture::Widgets => {}
        GalleryFixture::TextField => {
            surface.clear(slopos_ui::theme::palette().surface)?;
            for field in &mut gallery.fields {
                field.paint(surface)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let options = parse_arguments(&arguments)?;
    let (width, height) = options.fixture.size();
    if options.fixture == GalleryFixture::Widgets {
        let mut app = widgets::Widgets::new();
        if let Some(path) = options.snapshot {
            let surface = RasterSurface::new(width, height, options.scale)?;
            app.paint(&surface);
            surface.save_png(&path)?;
            return Ok(());
        }
        let mut window = WindowOptions::new("Widgets", "slopos-ui-gallery", width, height);
        window.scale = Some(options.scale);
        platform::run(window, app, |_| {})?;
        return Ok(());
    }
    let mut gallery = Gallery::new(options.fixture);
    if let Some(path) = options.snapshot {
        let surface = RasterSurface::new(width, height, options.scale)?;
        gallery.paint(&surface);
        surface.save_png(&path)?;
        return Ok(());
    }
    let mut window =
        WindowOptions::new(options.fixture.title(), "slopos-ui-gallery", width, height);
    window.scale = Some(options.scale);
    window.resizable = false;
    platform::run(window, gallery, |_| {})?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_argument_selects_the_button_plate_matrix() {
        let options = parse_arguments(&[
            "--component".into(),
            "button-plate".into(),
            "--scale".into(),
            "2".into(),
        ])
        .unwrap();
        assert_eq!(options.fixture, GalleryFixture::ButtonPlate);
        assert_eq!(options.scale.factor(), 2);
    }

    #[test]
    fn scale_defaults_to_one_and_accepts_positive_integers() {
        std::env::remove_var("SLOPOS_SCALE");
        std::env::remove_var("GDK_SCALE");
        assert_eq!(parse_arguments(&[]).unwrap().scale.factor(), 1);
        assert_eq!(
            parse_arguments(&["--scale".into(), "2".into()])
                .unwrap()
                .scale
                .factor(),
            2
        );
    }

    #[test]
    fn malformed_arguments_cannot_launch_a_fixture() {
        for arguments in [
            vec!["--scale"],
            vec!["--scale", "0"],
            vec!["--scale", "-1"],
            vec!["--scale", "1.5"],
            vec!["--scale", "5"],
            vec!["--scale", "4294967295"],
            vec!["--scale", "4294967296"],
            vec!["--unknown"],
            vec!["--scale", "1", "extra"],
        ] {
            assert!(
                parse_arguments(&arguments.into_iter().map(String::from).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }

    #[test]
    fn duplicate_options_are_rejected() {
        for arguments in [
            vec!["--scale", "1", "--scale", "2"],
            vec!["--component", "surface", "--component", "button-plate"],
        ] {
            assert!(
                parse_arguments(&arguments.into_iter().map(String::from).collect::<Vec<_>>())
                    .is_err()
            );
        }
    }
    #[test]
    fn tab_moves_focus_through_the_enabled_buttons() {
        let mut gallery = Gallery::new(GalleryFixture::Buttons);
        let mut control = Control::default();
        // The example "Focused" button starts focused; Tab wraps to Cancel
        // because the disabled button is skipped.
        gallery.event(
            &Event::KeyPressed {
                key: Key::Tab,
                modifiers: Default::default(),
            },
            &mut control,
        );
        assert!(gallery.buttons[0].focused);
        assert!(!gallery.buttons[3].focused);
        gallery.event(
            &Event::KeyPressed {
                key: Key::Tab,
                modifiers: slopos_ui::event::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
            &mut control,
        );
        assert!(gallery.buttons[3].focused);
        assert!(!gallery.buttons[0].focused);
    }

    #[test]
    fn text_fields_take_focus_typing_and_clipboard_requests() {
        use slopos_ui::event::Modifiers;
        let mut gallery = Gallery::new(GalleryFixture::TextField);
        let key = |key, control| Event::KeyPressed {
            key,
            modifiers: Modifiers {
                control,
                ..Modifiers::default()
            },
        };
        let mut control = Control::default();
        gallery.event(&key(Key::Tab, false), &mut control);
        assert_eq!(gallery.focus.focused(), Some(11));
        assert!(gallery.fields[1].focused && !gallery.fields[0].focused);
        for c in "hi".chars() {
            gallery.event(&key(Key::Character(c), false), &mut control);
        }
        assert_eq!(gallery.fields[1].text(), "hi");
        gallery.event(&key(Key::Character('a'), true), &mut control);
        gallery.event(&key(Key::Character('c'), true), &mut control);
        assert_eq!(control.pending_clipboard(), Some("hi"));
        gallery.event(&key(Key::Character('v'), true), &mut control);
        assert!(control.paste_requested());
        // The disabled field is skipped by Tab.
        gallery.event(&key(Key::Tab, false), &mut control);
        assert_eq!(gallery.focus.focused(), Some(10));
        let tree = gallery.accessibility().unwrap();
        assert_eq!(tree.focus, accesskit::NodeId(10));
        assert_eq!(tree.nodes.len(), 4);
    }

    #[test]
    fn every_fixture_renders_something_dark() {
        for fixture in [
            GalleryFixture::Surface,
            GalleryFixture::ButtonPlate,
            GalleryFixture::Buttons,
        ] {
            let (width, height) = fixture.size();
            let surface = RasterSurface::new(width, height, Scale::new(1).unwrap()).unwrap();
            let mut gallery = Gallery::new(fixture);
            gallery.paint(&surface);
            let dark = (0..width)
                .flat_map(|x| (0..height).map(move |y| (x, y)))
                .filter(|(x, y)| surface.pixel(*x, *y) == Some(0xff000000))
                .count();
            assert!(dark > 50, "{fixture:?} rendered nothing");
        }
    }
}
