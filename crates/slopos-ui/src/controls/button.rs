use gtk::prelude::*;

use crate::{
    accessibility,
    foundation::{color::Color, geometry::Scale},
    primitives::button_plate::{paint_button_plate, ButtonPlateState, ButtonPlateVariant},
    render::RasterSurface,
};

#[derive(Debug, thiserror::Error)]
pub enum ButtonControlError {
    #[error(transparent)]
    Render(#[from] crate::render::RenderError),
    #[error(transparent)]
    Accessibility(#[from] accessibility::AccessibilityError),
    #[error(transparent)]
    Style(#[from] gtk::glib::Error),
}

pub struct ButtonControl {
    button: gtk::Button,
}

impl ButtonControl {
    pub fn new(
        label: &str,
        description: &str,
        variant: ButtonPlateVariant,
        font: &gtk::pango::FontDescription,
    ) -> Result<Self, ButtonControlError> {
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        button.set_can_focus(true);
        let dimensions = variant.dimensions();
        button.set_size_request(dimensions.width as i32, dimensions.height as i32);
        button.style_context().add_class("slopos-button-control");
        let style = gtk::CssProvider::new();
        style.load_from_data(
            b"button.slopos-button-control { padding: 0; border: 0; margin: 0; min-width: 0; min-height: 0; box-shadow: none; }",
        )?;
        button
            .style_context()
            .add_provider(&style, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        let area = gtk::DrawingArea::new();
        area.set_size_request(dimensions.width as i32, dimensions.height as i32);
        button.add(&area);
        accessibility::configure(&button, gtk::atk::Role::PushButton, label, description)?;

        let state = std::rc::Rc::new(std::cell::Cell::new(ButtonPlateState::Rest));
        let state_for_draw = std::rc::Rc::clone(&state);
        let label = label.to_owned();
        let font = font.clone();
        area.connect_draw(move |area, context| {
            let surface = match RasterSurface::new(
                dimensions.width,
                dimensions.height,
                Scale::new(u32::try_from(area.scale_factor()).unwrap_or(1)).unwrap(),
            ) {
                Ok(surface) => surface,
                Err(_) => return gtk::glib::Propagation::Proceed,
            };
            let plate_state = state_for_draw.get();
            if paint_button_plate(&surface, 0, 0, variant, plate_state).is_err() {
                return gtk::glib::Propagation::Proceed;
            }
            if surface.paint_to(context).is_err() {
                return gtk::glib::Propagation::Proceed;
            }
            let layout = pangocairo::functions::create_layout(context);
            layout.set_font_description(Some(&font));
            layout.set_text(&label);
            let (text_width, text_height) = layout.pixel_size();
            let x = (f64::from(dimensions.width) - f64::from(text_width)) / 2.0;
            let y = (f64::from(dimensions.height) - f64::from(text_height)) / 2.0;
            context.move_to(x, y);
            let color = if plate_state == ButtonPlateState::Pressed {
                Color::WHITE
            } else if plate_state == ButtonPlateState::Disabled {
                Color {
                    red: 0xc0,
                    green: 0xc0,
                    blue: 0xc0,
                    alpha: 0xff,
                }
            } else {
                Color::BLACK
            };
            context.set_source_rgba(
                f64::from(color.red) / 255.0,
                f64::from(color.green) / 255.0,
                f64::from(color.blue) / 255.0,
                1.0,
            );
            pangocairo::functions::show_layout(context, &layout);
            gtk::glib::Propagation::Proceed
        });
        let state_for_flags = std::rc::Rc::clone(&state);
        let area_weak = area.downgrade();
        button.connect_state_flags_changed(move |button, _| {
            state_for_flags.set(if !button.is_sensitive() {
                ButtonPlateState::Disabled
            } else if button.state_flags().contains(gtk::StateFlags::ACTIVE) {
                ButtonPlateState::Pressed
            } else {
                ButtonPlateState::Rest
            });
            if let Some(area) = area_weak.upgrade() {
                area.queue_draw();
            }
        });
        Ok(Self { button })
    }

    pub fn widget(&self) -> &gtk::Button {
        &self.button
    }

    pub fn connect_activate<F: Fn() + 'static>(&self, callback: F) {
        self.button.connect_clicked(move |button| {
            if button.is_sensitive() {
                callback();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    #[ignore = "requires an X11 graphical VM"]
    fn disabled_button_does_not_activate_and_enabled_button_activates() {
        use gtk::prelude::*;

        gtk::init().unwrap();
        let font = gtk::pango::FontDescription::from_string("Sans 12");
        let control = ButtonControl::new(
            "Apply",
            "Apply the pending changes",
            ButtonPlateVariant::Regular,
            &font,
        )
        .unwrap();
        let activations = Rc::new(Cell::new(0));
        let activations_for_handler = Rc::clone(&activations);
        control.connect_activate(move || {
            activations_for_handler.set(activations_for_handler.get() + 1);
        });

        control.widget().set_sensitive(false);
        control.widget().clicked();
        assert_eq!(activations.get(), 0);

        control.widget().set_sensitive(true);
        control.widget().clicked();
        assert_eq!(activations.get(), 1);
        let accessible = control.widget().accessible().unwrap();
        assert_eq!(accessible.name().as_deref(), Some("Apply"));
        assert_eq!(accessible.role(), gtk::atk::Role::PushButton);

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.add(control.widget());
        window.show_all();
        while gtk::glib::MainContext::default().pending() {
            gtk::glib::MainContext::default().iteration(false);
        }
        let allocation = control.widget().allocation();
        assert_eq!(allocation.width(), 80);
        assert_eq!(allocation.height(), 20);
        unsafe {
            window.destroy();
        }

        for variant in [ButtonPlateVariant::Default, ButtonPlateVariant::Secondary] {
            let control = ButtonControl::new("OK", "Confirm", variant, &font).unwrap();
            let window = gtk::Window::new(gtk::WindowType::Toplevel);
            window.add(control.widget());
            window.show_all();
            while gtk::glib::MainContext::default().pending() {
                gtk::glib::MainContext::default().iteration(false);
            }
            let allocation = control.widget().allocation();
            let dimensions = variant.dimensions();
            assert_eq!(allocation.width(), dimensions.width as i32);
            assert_eq!(allocation.height(), dimensions.height as i32);
            unsafe {
                window.destroy();
            }
        }
    }

    fn button_fixture() -> (gtk::Window, gtk::Fixed, ButtonControl, Rc<Cell<u32>>) {
        gtk::init().unwrap();
        let font = gtk::pango::FontDescription::from_string("Sans 12");
        let control = ButtonControl::new(
            "Apply",
            "Apply the pending changes",
            ButtonPlateVariant::Regular,
            &font,
        )
        .unwrap();
        let activations = Rc::new(Cell::new(0));
        let activations_for_handler = Rc::clone(&activations);
        control.connect_activate(move || {
            activations_for_handler.set(activations_for_handler.get() + 1);
        });

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        let fixed = gtk::Fixed::new();
        window.set_default_size(240, 120);
        fixed.put(control.widget(), 40, 40);
        window.add(&fixed);
        window.show_all();
        window.present();
        process_events();
        (window, fixed, control, activations)
    }

    fn process_events() {
        let context = gtk::glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }
    }

    fn send_xdotool(arguments: &[&str]) {
        let output = std::process::Command::new("/usr/bin/xdotool")
            .args(arguments)
            .args(["getmouselocation", "--shell"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "xdotool failed: {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn screen_position(window: &gtk::Window, button: &gtk::Button, x: i32, y: i32) -> (i32, i32) {
        let (button_x, button_y) = button.translate_coordinates(window, x, y).unwrap();
        let (window_x, window_y) = window.position();
        (window_x + button_x, window_y + button_y)
    }

    fn wait_for(description: &str, condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            process_events();
            if condition() {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {description}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    struct X11InputCleanup {
        window: gtk::Window,
        pointer_pressed: bool,
        space_pressed: bool,
    }

    impl X11InputCleanup {
        fn new(window: &gtk::Window) -> Self {
            Self {
                window: window.clone(),
                pointer_pressed: false,
                space_pressed: false,
            }
        }

        fn press_pointer(&mut self) {
            self.pointer_pressed = true;
            send_xdotool(&["mousedown", "1"]);
        }

        fn release_pointer(&mut self) {
            send_xdotool(&["mouseup", "1"]);
            self.pointer_pressed = false;
        }

        fn press_space(&mut self) {
            self.space_pressed = true;
            send_xdotool(&["keydown", "space"]);
        }

        fn release_space(&mut self) {
            send_xdotool(&["keyup", "space"]);
            self.space_pressed = false;
        }
    }

    impl Drop for X11InputCleanup {
        fn drop(&mut self) {
            if self.pointer_pressed {
                let _ = std::process::Command::new("/usr/bin/xdotool")
                    .args(["mouseup", "1", "getmouselocation", "--shell"])
                    .output();
            }
            if self.space_pressed {
                let _ = std::process::Command::new("/usr/bin/xdotool")
                    .args(["keyup", "space", "getmouselocation", "--shell"])
                    .output();
            }
            if let Some(window) = self.window.window() {
                window.display().sync();
            }
            process_events();
            self.window.close();
        }
    }

    #[test]
    #[ignore = "requires an isolated X11 display"]
    fn pointer_release_outside_cancels_button_activation() {
        let (window, _fixed, control, activations) = button_fixture();
        let mut input_cleanup = X11InputCleanup::new(&window);
        let button = control.widget();
        let target = window.window().unwrap();
        let release_events = Rc::new(Cell::new(0));
        let release_events_for_handler = Rc::clone(&release_events);
        button.connect_button_release_event(move |_, _| {
            release_events_for_handler.set(release_events_for_handler.get() + 1);
            gtk::glib::Propagation::Proceed
        });
        let allocation = button.allocation();
        let (center_x, center_y) = screen_position(
            &window,
            button,
            allocation.width() / 2,
            allocation.height() / 2,
        );
        let (outside_x, outside_y) = screen_position(
            &window,
            button,
            allocation.width() + 12,
            allocation.height() + 12,
        );
        let center_x = center_x.to_string();
        let center_y = center_y.to_string();
        let outside_x = outside_x.to_string();
        let outside_y = outside_y.to_string();

        send_xdotool(&["mousemove", &center_x, &center_y]);
        input_cleanup.press_pointer();
        target.display().sync();
        wait_for("button to enter its pressed state", || {
            button.state_flags().contains(gtk::StateFlags::ACTIVE)
        });
        assert_eq!(activations.get(), 0);
        input_cleanup.release_pointer();
        target.display().sync();
        wait_for("inside pointer release to activate", || {
            activations.get() == 1
        });

        send_xdotool(&["mousemove", &center_x, &center_y]);
        input_cleanup.press_pointer();
        target.display().sync();
        wait_for("second press to enter its pressed state", || {
            button.state_flags().contains(gtk::StateFlags::ACTIVE)
        });
        send_xdotool(&["mousemove", &outside_x, &outside_y]);
        target.display().sync();
        process_events();
        let outside_release_count = release_events.get();
        input_cleanup.release_pointer();
        target.display().sync();
        wait_for("outside release to reach the button", || {
            release_events.get() > outside_release_count
        });
        assert_eq!(activations.get(), 1);
        assert!(!button.state_flags().contains(gtk::StateFlags::ACTIVE));
    }

    #[test]
    #[ignore = "requires an isolated X11 display"]
    fn focused_button_activates_from_space() {
        let (window, _fixed, control, activations) = button_fixture();
        let mut input_cleanup = X11InputCleanup::new(&window);
        let button = control.widget();
        button.grab_focus();
        process_events();
        assert!(button.has_focus());
        let target = window.window().unwrap();
        target.focus(0);
        target.display().sync();
        input_cleanup.press_space();
        target.display().sync();
        wait_for("Space press to enter the pressed state", || {
            button.state_flags().contains(gtk::StateFlags::ACTIVE)
        });
        assert_eq!(activations.get(), 0);
        input_cleanup.release_space();
        target.display().sync();
        wait_for("Space release to activate", || activations.get() == 1);
    }
}
