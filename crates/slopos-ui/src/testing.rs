use std::{cell::RefCell, error::Error, io, rc::Rc};

use gtk::prelude::*;

use crate::{
    accessibility,
    foundation::{
        color::Color,
        geometry::{Rect, Scale},
    },
    primitives::button_plate::{paint_button_plate, ButtonPlateState, ButtonPlateVariant},
    render::RasterSurface,
};

pub fn show_surface_fixture(scale: Scale) -> Result<(), Box<dyn Error>> {
    let surface = RasterSurface::new(160, 90, scale)?;
    surface.clear(Color::WHITE)?;
    surface.fill_rect(
        Rect {
            x: 20,
            y: 20,
            width: 80,
            height: 20,
        },
        Color::BLACK,
    )?;
    show_raster_fixture(
        surface,
        scale,
        160,
        90,
        "Surface",
        "Surface geometry fixture",
        "Black rectangle on a white surface; not a button fixture.",
    )
}

pub fn show_button_plate_fixture(scale: Scale) -> Result<(), Box<dyn Error>> {
    let surface = RasterSurface::new(320, 136, scale)?;
    surface.clear(Color::WHITE)?;
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
    for (row_index, variant) in variants.iter().copied().enumerate() {
        for (column_index, state) in states.iter().copied().enumerate() {
            let origin_x = 16 + i32::try_from(column_index)? * 104;
            let origin_y = 16 + i32::try_from(row_index)? * 40;
            paint_button_plate(&surface, origin_x, origin_y, variant, state)?;
        }
    }
    show_raster_fixture(
        surface,
        scale,
        320,
        136,
        "Button Plates",
        "Button plate state matrix",
        "Geometry-only matrix. Rows: regular, default, secondary. Columns: rest, pressed, disabled. The plates are not interactive controls.",
    )
}

fn show_raster_fixture(
    surface: RasterSurface,
    scale: Scale,
    width: i32,
    height: i32,
    title: &str,
    accessible_name: &str,
    description: &str,
) -> Result<(), Box<dyn Error>> {
    gtk::gdk::set_allowed_backends("x11");
    gtk::init()?;
    if gtk::gdk::Display::default()
        .map(|display| display.type_().name().to_owned())
        .as_deref()
        != Some("GdkX11Display")
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SLOPOS UI Gallery requires an X11 display",
        )
        .into());
    }
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    let result = (|| -> Result<(), Box<dyn Error>> {
        window.set_title(&format!("SLOPOS UI Gallery — {title}"));
        window.set_default_size(width, height);
        window.set_resizable(false);
        window.realize();
        if u32::try_from(window.scale_factor()).ok() != Some(scale.factor()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "gallery scale does not match GTK; launch with GDK_SCALE={}",
                    scale.factor()
                ),
            )
            .into());
        }
        let canvas = gtk::DrawingArea::new();
        canvas.set_size_request(width, height);
        accessibility::configure(&canvas, gtk::atk::Role::Panel, accessible_name, description)?;
        let draw_error = Rc::new(RefCell::new(None));
        let callback_error = Rc::clone(&draw_error);
        canvas.connect_draw(move |_, context| {
            if let Err(error) = surface.paint_to(context) {
                *callback_error.borrow_mut() = Some(error);
                gtk::main_quit();
            }
            gtk::glib::Propagation::Stop
        });
        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            gtk::glib::Propagation::Stop
        });
        window.connect_key_press_event(|_, event| {
            if event.keyval() == gtk::gdk::keys::constants::Escape {
                gtk::main_quit();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        window.add(&canvas);
        window.show_all();
        gtk::main();
        if let Some(error) = draw_error.borrow_mut().take() {
            return Err(error.into());
        }
        Ok(())
    })();
    unsafe {
        window.destroy();
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a graphical VM with GDK_SCALE=1"]
    fn gallery_windows_are_released_after_errors_and_main_loop_exit() {
        use gtk::prelude::*;
        gtk::init().unwrap();
        let assert_no_owned_windows = || {
            let windows = gtk::Window::list_toplevels();
            assert!(windows.len() <= 1);
            for window in windows {
                assert_eq!(window.type_().name(), "GtkTooltipWindow");
                assert!(!window.is_visible());
            }
        };
        assert_no_owned_windows();
        for exit_path in 0..3 {
            let error = super::show_surface_fixture(super::Scale::new(2).unwrap()).unwrap_err();
            assert_eq!(
                error.downcast_ref::<std::io::Error>().unwrap().kind(),
                std::io::ErrorKind::InvalidInput
            );
            assert!(error.to_string().contains("GDK_SCALE=2"));
            assert_no_owned_windows();
            let timed_out = std::rc::Rc::new(std::cell::Cell::new(false));
            let callback_timeout = std::rc::Rc::clone(&timed_out);
            let timeout =
                gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
                    callback_timeout.set(true);
                    gtk::main_quit();
                });
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                match exit_path {
                    0 => gtk::main_quit(),
                    _ => {
                        let window = gtk::Window::list_toplevels()
                            .into_iter()
                            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
                            .find(|window| {
                                window.title().as_deref() == Some("SLOPOS UI Gallery — Surface")
                            })
                            .unwrap();
                        if exit_path == 1 {
                            window.close();
                        } else {
                            window.window().unwrap().focus(0);
                            window.display().sync();
                            assert!(std::process::Command::new("/usr/bin/xdotool")
                                .args(["key", "--clearmodifiers", "Escape"])
                                .status()
                                .unwrap()
                                .success());
                        }
                    }
                }
            });
            super::show_surface_fixture(super::Scale::new(1).unwrap()).unwrap();
            if !timed_out.get() {
                timeout.remove();
            }
            assert!(
                !timed_out.get(),
                "gallery exit path {exit_path} did not finish"
            );
            assert_no_owned_windows();
        }
    }

    #[test]
    #[ignore = "requires a real graphical VM client capture and explicit capture scale"]
    fn graphical_surface_capture_matches_independent_pixels() {
        let path =
            std::env::var_os("SLOPOS_GALLERY_CAPTURE").expect("SLOPOS_GALLERY_CAPTURE is required");
        let scale: i32 = std::env::var("SLOPOS_GALLERY_CAPTURE_SCALE")
            .expect("capture scale is required")
            .parse()
            .unwrap();
        assert!(matches!(scale, 1 | 2));
        let image = gtk::gdk_pixbuf::Pixbuf::from_file(path).unwrap();
        assert_eq!((image.width(), image.height()), (160 * scale, 90 * scale));
        assert_eq!(image.bits_per_sample(), 8);
        let channels = usize::try_from(image.n_channels()).unwrap();
        assert!(matches!(channels, 3 | 4));
        let stride = usize::try_from(image.rowstride()).unwrap();
        let bytes = image.read_pixel_bytes();
        let pixels = bytes.as_ref();
        for point_y in 0..image.height() {
            for point_x in 0..image.width() {
                let expected = if (20 * scale..100 * scale).contains(&point_x)
                    && (20 * scale..40 * scale).contains(&point_y)
                {
                    0
                } else {
                    255
                };
                let offset = point_y as usize * stride + point_x as usize * channels;
                assert_eq!(
                    &pixels[offset..offset + 3],
                    &[expected; 3],
                    "pixel at ({point_x}, {point_y})"
                );
                if channels == 4 {
                    assert_eq!(pixels[offset + 3], 255);
                }
            }
        }
    }
}
