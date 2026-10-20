use slopos_ui::foundation::geometry::Scale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GalleryFixture {
    Surface,
    ButtonPlate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GalleryOptions {
    scale: Scale,
    fixture: GalleryFixture,
}

fn parse_arguments(arguments: &[String]) -> Result<GalleryOptions, &'static str> {
    let mut scale_factor = 1;
    let mut fixture = GalleryFixture::Surface;
    let mut scale_seen = false;
    let mut component_seen = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--scale" => {
                if scale_seen {
                    return Err("--scale may only be specified once");
                }
                let value = arguments.get(index + 1).ok_or(
                    "usage: slopos-ui-gallery [--component surface|button-plate] [--scale INTEGER]",
                )?;
                scale_factor = value
                    .parse::<u32>()
                    .map_err(|_| "scale must be a positive integer")?;
                scale_seen = true;
                index += 2;
            }
            "--component" => {
                if component_seen {
                    return Err("--component may only be specified once");
                }
                let value = arguments.get(index + 1).ok_or(
                    "usage: slopos-ui-gallery [--component surface|button-plate] [--scale INTEGER]",
                )?;
                fixture = match value.as_str() {
                    "surface" => GalleryFixture::Surface,
                    "button-plate" => GalleryFixture::ButtonPlate,
                    _ => return Err("supported components are surface and button-plate"),
                };
                component_seen = true;
                index += 2;
            }
            _ => {
                return Err(
                    "usage: slopos-ui-gallery [--component surface|button-plate] [--scale INTEGER]",
                )
            }
        }
    }
    if !matches!(scale_factor, 1 | 2) {
        return Err("supported gallery scales are 1 and 2");
    }
    Ok(GalleryOptions {
        scale: Scale::new(scale_factor).ok_or("scale must be a positive integer")?,
        fixture,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let options = parse_arguments(&arguments)?;
    match options.fixture {
        GalleryFixture::Surface => slopos_ui::testing::show_surface_fixture(options.scale),
        GalleryFixture::ButtonPlate => slopos_ui::testing::show_button_plate_fixture(options.scale),
    }
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
            vec!["--scale", "3"],
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
}
