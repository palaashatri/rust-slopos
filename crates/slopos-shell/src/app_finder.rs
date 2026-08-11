//! Desktop Application Finder
//! Scans system and user directories for `.desktop` application files.

use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub comment: String,
    pub path: PathBuf,
}

pub fn scan_desktop_apps() -> Vec<DesktopApp> {
    let mut apps = Vec::new();
    let dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        dirs_home_applications(),
    ];

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).max_depth(2).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                if let Some(app) = parse_desktop_file(path) {
                    apps.push(app);
                }
            }
        }
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

fn parse_desktop_file(path: &std::path::Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut name = String::new();
    let mut exec = String::new();
    let mut icon = String::new();
    let mut comment = String::new();
    let mut no_display = false;

    for line in content.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = false;
        }
        if !in_desktop_entry {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "Name" if name.is_empty() => name = val.to_string(),
                "Exec" if exec.is_empty() => exec = sanitize_exec(val),
                "Icon" if icon.is_empty() => icon = val.to_string(),
                "Comment" if comment.is_empty() => comment = val.to_string(),
                "NoDisplay" if val.eq_ignore_ascii_case("true") => no_display = true,
                _ => {}
            }
        }
    }

    if no_display || name.is_empty() || exec.is_empty() {
        return None;
    }

    let id = path.file_stem()?.to_string_lossy().to_string();

    Some(DesktopApp {
        id,
        name,
        exec,
        icon,
        comment,
        path: path.to_path_buf(),
    })
}

fn sanitize_exec(exec: &str) -> String {
    // Remove %f, %F, %u, %U, etc. field codes
    exec.split_whitespace()
        .filter(|arg| !arg.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

fn dirs_home_applications() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local/share/applications")
    } else {
        PathBuf::from("/tmp")
    }
}
