use gtk::prelude::*;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("widget has no accessible object")]
pub struct AccessibilityError;

pub fn configure<W: IsA<gtk::Widget>>(
    widget: &W,
    role: gtk::atk::Role,
    name: &str,
    description: &str,
) -> Result<(), AccessibilityError> {
    let accessible = widget.accessible().ok_or(AccessibilityError)?;
    accessible.set_role(role);
    accessible.set_name(name);
    accessible.set_description(description);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a graphical VM and ATK runtime"]
    fn configuration_sets_accessible_role_name_and_description() {
        use gtk::prelude::*;

        gtk::init().unwrap();
        let widget = gtk::DrawingArea::new();
        super::configure(
            &widget,
            gtk::atk::Role::PushButton,
            "Apply",
            "Apply the pending settings",
        )
        .unwrap();

        let accessible = widget.accessible().unwrap();
        assert_eq!(accessible.role(), gtk::atk::Role::PushButton);
        assert_eq!(accessible.name().as_deref(), Some("Apply"));
        assert_eq!(
            accessible.description().as_deref(),
            Some("Apply the pending settings")
        );
    }
}
