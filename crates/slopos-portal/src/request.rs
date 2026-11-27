//! Maps the portal's `a{sv}` options onto a [`ChooserSpec`].
//!
//! The option names and types follow the xdg-desktop-portal FileChooser
//! specification: `multiple` (b), `directory` (b), `filters` (a(sa(us))),
//! `current_filter` ((sa(us))), `current_folder` (ay, NUL-terminated),
//! `current_name` (s) and `accept_label` (s). `handle_token` is not read: the
//! frontend uses it to build the handle, and the backend is given the handle
//! itself. Unknown options are ignored, as the specification allows.

use std::collections::HashMap;
use std::path::PathBuf;

use zbus::zvariant::Value;

use crate::spec::{ChooserSpec, FilterSpec, Kind, Pattern};
use slopos_ui::controls::file_chooser::validate_file_name;

pub type OptionMap<'a> = HashMap<&'a str, Value<'a>>;

/// Build the chooser for `kind` from the portal options. Invalid values are
/// errors; invalid hints (a folder that does not exist, an unusable name)
/// are dropped so the dialog falls back to its defaults.
pub fn parse_options(
    kind: Kind,
    title: &str,
    options: &OptionMap<'_>,
) -> Result<ChooserSpec, String> {
    let mut spec = ChooserSpec::new(kind, title);
    spec.multiple = flag(options, "multiple")?;
    if kind == Kind::Open && flag(options, "directory")? {
        spec.kind = Kind::OpenFolder;
    }
    spec.filters = filters(options)?;
    spec.current_filter = current_filter(options)?;
    if let Some(folder) = folder(options)? {
        spec.folder = Some(folder);
    }
    if let Some(name) = text(options, "current_name")? {
        match validate_file_name(&name) {
            Ok(()) => spec.name = Some(name),
            Err(error) => log::info!("ignoring current_name: {}", error.message()),
        }
    }
    if let Some(label) = text(options, "accept_label")? {
        spec.accept_label = (!label.is_empty()).then_some(label);
    }
    Ok(spec)
}

fn flag(options: &OptionMap<'_>, key: &str) -> Result<bool, String> {
    match options.get(key) {
        None => Ok(false),
        Some(value) => bool::try_from(value.try_clone().map_err(|e| format!("{key}: {e}"))?)
            .map_err(|_| format!("{key} must be a boolean")),
    }
}

fn text(options: &OptionMap<'_>, key: &str) -> Result<Option<String>, String> {
    match options.get(key) {
        None => Ok(None),
        Some(value) => String::try_from(value.try_clone().map_err(|e| format!("{key}: {e}"))?)
            .map(Some)
            .map_err(|_| format!("{key} must be a string")),
    }
}

type FilterWire = Vec<(String, Vec<(u32, String)>)>;

fn filter_from_wire((name, patterns): (String, Vec<(u32, String)>)) -> Result<FilterSpec, String> {
    let patterns = patterns
        .into_iter()
        .map(|(kind, pattern)| match kind {
            0 => Ok(Pattern::Glob(pattern)),
            1 => Ok(Pattern::Mime(pattern)),
            other => Err(format!("unknown filter pattern type {other}")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FilterSpec { name, patterns })
}

fn filters(options: &OptionMap<'_>) -> Result<Vec<FilterSpec>, String> {
    let Some(value) = options.get("filters") else {
        return Ok(Vec::new());
    };
    let wire = FilterWire::try_from(value.try_clone().map_err(|e| format!("filters: {e}"))?)
        .map_err(|_| "filters must be a(sa(us))".to_owned())?;
    wire.into_iter().map(filter_from_wire).collect()
}

/// The index of `current_filter` within `filters`, when it names one of them.
fn current_filter(options: &OptionMap<'_>) -> Result<Option<usize>, String> {
    let Some(value) = options.get("current_filter") else {
        return Ok(None);
    };
    let wire = <(String, Vec<(u32, String)>)>::try_from(
        value
            .try_clone()
            .map_err(|e| format!("current_filter: {e}"))?,
    )
    .map_err(|_| "current_filter must be (sa(us))".to_owned())?;
    let wanted = filter_from_wire(wire)?;
    let all = filters(options)?;
    Ok(all.iter().position(|filter| *filter == wanted))
}

/// The requested folder, as an absolute path. Bytes that are not UTF-8 are
/// kept as the path's raw bytes.
fn folder(options: &OptionMap<'_>) -> Result<Option<PathBuf>, String> {
    use std::os::unix::ffi::OsStringExt;
    let Some(value) = options.get("current_folder") else {
        return Ok(None);
    };
    let mut bytes = Vec::<u8>::try_from(
        value
            .try_clone()
            .map_err(|e| format!("current_folder: {e}"))?,
    )
    .map_err(|_| "current_folder must be ay".to_owned())?;
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    if bytes.is_empty() || bytes.contains(&0) {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(std::ffi::OsString::from_vec(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn options(entries: Vec<(&'static str, Value<'static>)>) -> OptionMap<'static> {
        entries.into_iter().collect()
    }

    #[test]
    fn open_options_map_onto_the_spec() {
        let spec = parse_options(
            Kind::Open,
            "Open File",
            &options(vec![
                ("multiple", Value::from(true)),
                (
                    "filters",
                    Value::from(vec![(
                        "Images".to_owned(),
                        vec![(1u32, "image/*".to_owned()), (0u32, "*.PNG".to_owned())],
                    )]),
                ),
                ("accept_label", Value::from("Pick")),
            ]),
        )
        .unwrap();
        assert_eq!(spec.kind, Kind::Open);
        assert!(spec.multiple);
        assert_eq!(spec.filters[0].name, "Images");
        assert_eq!(spec.accept_label.as_deref(), Some("Pick"));
    }

    #[test]
    fn the_frontend_handle_token_is_not_read_and_unknown_options_are_ignored() {
        let spec = parse_options(
            Kind::Open,
            "Open",
            &options(vec![
                ("handle_token", Value::from("a/b")),
                ("something_new", Value::from(1u32)),
            ]),
        )
        .expect("the backend is given the handle, so the token is not checked");
        assert_eq!(spec.kind, Kind::Open);
    }

    #[test]
    fn directory_turns_an_open_into_a_folder_choice() {
        let spec = parse_options(
            Kind::Open,
            "Folder",
            &options(vec![("directory", Value::from(true))]),
        )
        .unwrap();
        assert_eq!(spec.kind, Kind::OpenFolder);
    }

    #[test]
    fn folders_and_names_are_hints_that_fall_back_quietly() {
        let mut folder_bytes = b"/tmp".to_vec();
        folder_bytes.push(0);
        let spec = parse_options(
            Kind::Save,
            "Save",
            &options(vec![
                ("current_folder", Value::from(folder_bytes)),
                ("current_name", Value::from("../escape")),
            ]),
        )
        .unwrap();
        assert_eq!(spec.folder, Some(PathBuf::from("/tmp")));
        assert_eq!(spec.name, None, "a name with a slash is dropped");
        let spec = parse_options(
            Kind::Save,
            "Save",
            &options(vec![("current_name", Value::from("notes.txt"))]),
        )
        .unwrap();
        assert_eq!(spec.name.as_deref(), Some("notes.txt"));
    }

    #[test]
    fn current_filter_selects_a_listed_filter() {
        let filter = ("Text".to_owned(), vec![(0u32, "*.txt".to_owned())]);
        let spec = parse_options(
            Kind::Open,
            "Open",
            &options(vec![
                (
                    "filters",
                    Value::from(vec![
                        ("Images".to_owned(), vec![(1u32, "image/*".to_owned())]),
                        filter.clone(),
                    ]),
                ),
                ("current_filter", Value::from(filter)),
            ]),
        )
        .unwrap();
        assert_eq!(spec.current_filter, Some(1));
    }

    #[test]
    fn malformed_options_are_errors() {
        assert!(parse_options(
            Kind::Open,
            "x",
            &options(vec![("multiple", Value::from("yes"))])
        )
        .is_err());
        assert!(parse_options(
            Kind::Open,
            "x",
            &options(vec![(
                "filters",
                Value::from(vec![("Bad".to_owned(), vec![(9u32, "*".to_owned())])])
            )])
        )
        .is_err());
    }
}
