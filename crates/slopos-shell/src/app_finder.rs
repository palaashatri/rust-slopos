//! Desktop application discovery and safe `.desktop` Exec parsing.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub argv: Vec<String>,
    pub icon: String,
    pub comment: String,
    pub terminal: bool,
}

pub fn scan_desktop_apps() -> Vec<DesktopApp> {
    // Later directories deliberately override earlier ones so a user's desktop
    // entry wins over the system copy with the same desktop-file id.
    let dirs = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        dirs_home_applications(),
    ];
    let mut apps_by_id = HashMap::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir)
            .max_depth(2)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(path) {
                apps_by_id.insert(app.id.clone(), app);
            }
        }
    }

    let mut apps: Vec<_> = apps_by_id.into_values().collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

fn parse_desktop_file(path: &Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let id = path.file_stem()?.to_string_lossy();
    parse_desktop_entry(&content, id.as_ref())
}

fn parse_desktop_entry(content: &str, id: &str) -> Option<DesktopApp> {
    let mut in_desktop_entry = false;
    let mut app_type = String::new();
    let mut name = String::new();
    let mut exec = String::new();
    let mut icon = String::new();
    let mut comment = String::new();
    let mut try_exec = String::new();
    let mut only_show_in = String::new();
    let mut not_show_in = String::new();
    let mut no_display = false;
    let mut hidden = false;
    let mut terminal = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = false;
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Type" if app_type.is_empty() => app_type = value.to_string(),
            "Name" if name.is_empty() => name = value.to_string(),
            "Exec" if exec.is_empty() => exec = value.to_string(),
            "Icon" if icon.is_empty() => icon = value.to_string(),
            "Comment" if comment.is_empty() => comment = value.to_string(),
            "TryExec" if try_exec.is_empty() => try_exec = value.to_string(),
            "OnlyShowIn" if only_show_in.is_empty() => only_show_in = value.to_string(),
            "NotShowIn" if not_show_in.is_empty() => not_show_in = value.to_string(),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "Terminal" => terminal = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if hidden || no_display || app_type != "Application" || name.is_empty() || exec.is_empty() {
        return None;
    }
    if !desktop_visibility_allows(&only_show_in, &not_show_in) {
        return None;
    }
    if !try_exec.is_empty() && !command_exists(&try_exec) {
        return None;
    }

    let argv = parse_exec_argv(&exec)?;
    if argv.is_empty() || !command_exists(&argv[0]) {
        return None;
    }

    Some(DesktopApp {
        id: id.to_string(),
        name,
        argv,
        icon,
        comment,
        terminal,
    })
}

fn desktop_visibility_allows(only_show_in: &str, not_show_in: &str) -> bool {
    const DESKTOP: &str = "SLOPOS";
    let contains = |value: &str| {
        value
            .split(';')
            .any(|desktop| desktop.eq_ignore_ascii_case(DESKTOP))
    };

    (only_show_in.is_empty() || contains(only_show_in)) && !contains(not_show_in)
}

/// Parse the freedesktop Exec token grammar without invoking a shell.
/// Field codes requiring a selected file/URL are omitted because Search starts
/// applications without an associated document.
fn parse_exec_argv(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = exec.chars().peekable();
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => quoted = !quoted,
            '\\' => current.push(chars.next()?),
            ' ' | '\t' if !quoted => {
                push_exec_token(&mut args, &mut current)?;
            }
            _ => current.push(ch),
        }
    }

    if quoted {
        return None;
    }
    push_exec_token(&mut args, &mut current)?;
    Some(args)
}

fn push_exec_token(args: &mut Vec<String>, current: &mut String) -> Option<()> {
    if current.is_empty() {
        return Some(());
    }
    let expanded = expand_field_codes(current)?;
    current.clear();
    if !expanded.is_empty() {
        args.push(expanded);
    }
    Some(())
}

fn expand_field_codes(token: &str) -> Option<String> {
    let mut output = String::new();
    let mut chars = token.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }

        match chars.next()? {
            '%' => output.push('%'),
            // No file/URL, icon, display name, or desktop-entry path is
            // supplied by the application palette, so these expand to empty.
            'f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' => {}
            _ => return None,
        }
    }
    Some(output)
}

fn command_exists(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
}

fn dirs_home_applications() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local/share/applications")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_parser_preserves_quoted_arguments_and_drops_file_codes() {
        assert_eq!(
            parse_exec_argv(r#"/bin/echo --title "Hello world" %U %%"#),
            Some(vec![
                "/bin/echo".to_string(),
                "--title".to_string(),
                "Hello world".to_string(),
                "%".to_string(),
            ])
        );
    }

    #[test]
    fn exec_parser_rejects_unterminated_quotes() {
        assert_eq!(parse_exec_argv(r#"/bin/echo "broken"#), None);
    }

    #[test]
    fn hidden_non_application_and_foreign_desktop_entries_are_rejected() {
        let hidden =
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=/bin/true\nHidden=true\n";
        assert!(parse_desktop_entry(hidden, "hidden").is_none());

        let link = "[Desktop Entry]\nType=Link\nName=Docs\nExec=/bin/true\n";
        assert!(parse_desktop_entry(link, "link").is_none());

        let gnome_only = "[Desktop Entry]\nType=Application\nName=GNOME only\nExec=/bin/true\nOnlyShowIn=GNOME;\n";
        assert!(parse_desktop_entry(gnome_only, "gnome-only").is_none());
    }

    #[test]
    fn valid_entry_is_parsed_without_shell_expansion() {
        let entry = r#"[Desktop Entry]
Type=Application
Name=Echo
Comment=Test
Exec=/bin/echo "hello world" %F
Icon=utilities-terminal
Terminal=false
"#;
        let app = parse_desktop_entry(entry, "echo").expect("valid entry");
        assert_eq!(app.name, "Echo");
        assert_eq!(app.argv, vec!["/bin/echo", "hello world"]);
        assert!(!app.terminal);
    }
}
