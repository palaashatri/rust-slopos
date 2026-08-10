//! Multi-client window rules (KWin/Mutter-class pure policy).
//!
//! Match app_id / title patterns → workspace, floating, maximize, skip-taskbar.
//! No Wayland I/O — pure evaluation used by shell Force Quit / compositor bridge.

use serde::{Deserialize, Serialize};

/// How a rule field matches a window property.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Exact equality (case-sensitive).
    Exact,
    /// Case-insensitive substring.
    Contains,
    /// Glob-ish: `*` matches any substring (simple, not full glob).
    Glob,
}

#[allow(clippy::derivable_impls)]
impl Default for MatchKind {
    fn default() -> Self {
        Self::Contains
    }
}

/// One match criterion on a window property.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WindowMatch {
    pub field: MatchField,
    pub kind: MatchKind,
    pub pattern: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchField {
    AppId,
    Title,
    Class,
}

/// Actions applied when a rule matches.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WindowRuleActions {
    /// Assign to workspace index (0-based). `None` = no change.
    pub workspace: Option<u8>,
    pub maximize: bool,
    pub floating: bool,
    pub skip_taskbar: bool,
    pub fullscreen: bool,
    /// Optional human label for UI / logs.
    pub tag: Option<String>,
}

/// A named window rule with priority (higher wins on first match order).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WindowRule {
    pub id: String,
    pub enabled: bool,
    /// Higher priority evaluated first.
    pub priority: i32,
    pub matches: Vec<WindowMatch>,
    /// If true, all matchers must succeed; else any matcher succeeds.
    pub require_all: bool,
    pub actions: WindowRuleActions,
}

/// Snapshot of a client window used for rule evaluation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WindowInfo {
    pub app_id: String,
    pub title: String,
    pub class: String,
}

/// Pure: does `pattern` match `value` under `kind`?
pub fn field_matches(kind: &MatchKind, pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    match kind {
        MatchKind::Exact => value == pattern,
        MatchKind::Contains => value
            .to_ascii_lowercase()
            .contains(&pattern.to_ascii_lowercase()),
        MatchKind::Glob => glob_match(pattern, value),
    }
}

/// Minimal glob: only `*` wildcards, case-insensitive.
fn glob_match(pattern: &str, value: &str) -> bool {
    let pat = pattern.to_ascii_lowercase();
    let val = value.to_ascii_lowercase();
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return val == pat;
    }
    if !val.starts_with(parts[0]) {
        return false;
    }
    if !val.ends_with(parts[parts.len() - 1]) {
        return false;
    }
    let mut rest = &val[parts[0].len()..];
    if !parts[parts.len() - 1].is_empty() {
        rest = &rest[..rest.len() - parts[parts.len() - 1].len()];
    }
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = rest.find(part) {
            rest = &rest[idx + part.len()..];
        } else {
            return false;
        }
    }
    true
}

fn match_one(m: &WindowMatch, info: &WindowInfo) -> bool {
    let value = match m.field {
        MatchField::AppId => info.app_id.as_str(),
        MatchField::Title => info.title.as_str(),
        MatchField::Class => info.class.as_str(),
    };
    field_matches(&m.kind, &m.pattern, value)
}

/// Pure: evaluate whether a rule matches window info.
pub fn rule_matches(rule: &WindowRule, info: &WindowInfo) -> bool {
    if !rule.enabled || rule.matches.is_empty() {
        return false;
    }
    if rule.require_all {
        rule.matches.iter().all(|m| match_one(m, info))
    } else {
        rule.matches.iter().any(|m| match_one(m, info))
    }
}

/// Sorted by priority descending; first match wins.
pub fn evaluate_rules(rules: &[WindowRule], info: &WindowInfo) -> Option<WindowRuleActions> {
    let mut ordered: Vec<&WindowRule> = rules.iter().filter(|r| r.enabled).collect();
    ordered.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    for rule in ordered {
        if rule_matches(rule, info) {
            return Some(rule.actions.clone());
        }
    }
    None
}

/// Seed rules useful for a SLOPOS-I session (terminals → workspace 1, etc.).
pub fn default_session_rules() -> Vec<WindowRule> {
    vec![
        WindowRule {
            id: "terminals".into(),
            enabled: true,
            priority: 50,
            matches: vec![
                WindowMatch {
                    field: MatchField::AppId,
                    kind: MatchKind::Contains,
                    pattern: "term".into(),
                },
                WindowMatch {
                    field: MatchField::AppId,
                    kind: MatchKind::Contains,
                    pattern: "kitty".into(),
                },
                WindowMatch {
                    field: MatchField::AppId,
                    kind: MatchKind::Contains,
                    pattern: "alacritty".into(),
                },
            ],
            require_all: false,
            actions: WindowRuleActions {
                workspace: Some(1),
                tag: Some("terminal".into()),
                ..Default::default()
            },
        },
        WindowRule {
            id: "browser-fullscreen-hint".into(),
            enabled: true,
            priority: 40,
            matches: vec![WindowMatch {
                field: MatchField::AppId,
                kind: MatchKind::Glob,
                pattern: "*firefox*".into(),
            }],
            require_all: true,
            actions: WindowRuleActions {
                maximize: true,
                tag: Some("browser".into()),
                ..Default::default()
            },
        },
        WindowRule {
            id: "force-quit-skip".into(),
            enabled: true,
            priority: 100,
            matches: vec![WindowMatch {
                field: MatchField::Title,
                kind: MatchKind::Exact,
                pattern: "Force Quit".into(),
            }],
            require_all: true,
            actions: WindowRuleActions {
                skip_taskbar: true,
                floating: true,
                tag: Some("shell-transient".into()),
                ..Default::default()
            },
        },
    ]
}

/// Parse a minimal rules file: one rule per non-empty line  
/// `id|priority|field:kind:pattern|workspace=N,maximize,floating`
pub fn parse_rules_simple(text: &str) -> Vec<WindowRule> {
    let mut rules = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 3 {
            continue;
        }
        let id = parts[0].trim().to_string();
        let priority: i32 = parts[1].trim().parse().unwrap_or(0);
        let mut matches = Vec::new();
        for m in parts[2].split(';') {
            let m = m.trim();
            if m.is_empty() {
                continue;
            }
            let bits: Vec<&str> = m.splitn(3, ':').collect();
            if bits.len() != 3 {
                continue;
            }
            let field = match bits[0] {
                "title" => MatchField::Title,
                "class" => MatchField::Class,
                _ => MatchField::AppId,
            };
            let kind = match bits[1] {
                "exact" => MatchKind::Exact,
                "glob" => MatchKind::Glob,
                _ => MatchKind::Contains,
            };
            matches.push(WindowMatch {
                field,
                kind,
                pattern: bits[2].to_string(),
            });
        }
        let mut actions = WindowRuleActions::default();
        if let Some(act) = parts.get(3) {
            for token in act.split(',') {
                let token = token.trim();
                if let Some(ws) = token.strip_prefix("workspace=") {
                    actions.workspace = ws.parse().ok();
                } else if token == "maximize" {
                    actions.maximize = true;
                } else if token == "floating" {
                    actions.floating = true;
                } else if token == "skip_taskbar" {
                    actions.skip_taskbar = true;
                } else if token == "fullscreen" {
                    actions.fullscreen = true;
                }
            }
        }
        rules.push(WindowRule {
            id: if id.is_empty() {
                format!("rule-{i}")
            } else {
                id
            },
            enabled: true,
            priority,
            matches,
            require_all: false,
            actions,
        });
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_rule_assigns_workspace() {
        let rules = default_session_rules();
        let info = WindowInfo {
            app_id: "org.slopos-i.Terminal".into(),
            title: "bash".into(),
            class: String::new(),
        };
        let a = evaluate_rules(&rules, &info).expect("match");
        assert_eq!(a.workspace, Some(1));
        assert_eq!(a.tag.as_deref(), Some("terminal"));
    }

    #[test]
    fn force_quit_skips_taskbar() {
        let rules = default_session_rules();
        let info = WindowInfo {
            app_id: "slopos-i".into(),
            title: "Force Quit".into(),
            class: String::new(),
        };
        let a = evaluate_rules(&rules, &info).unwrap();
        assert!(a.skip_taskbar && a.floating);
    }

    #[test]
    fn glob_and_priority() {
        assert!(field_matches(&MatchKind::Glob, "*fox*", "firefox"));
        assert!(!field_matches(&MatchKind::Exact, "a", "A"));
        let rules = parse_rules_simple("r1|10|app_id:contains:vim|workspace=2,maximize\n");
        let info = WindowInfo {
            app_id: "gvim".into(),
            title: String::new(),
            class: String::new(),
        };
        let a = evaluate_rules(&rules, &info).unwrap();
        assert_eq!(a.workspace, Some(2));
        assert!(a.maximize);
    }

    #[test]
    fn require_all_fails_partial() {
        let rule = WindowRule {
            id: "both".into(),
            enabled: true,
            priority: 1,
            matches: vec![
                WindowMatch {
                    field: MatchField::AppId,
                    kind: MatchKind::Exact,
                    pattern: "a".into(),
                },
                WindowMatch {
                    field: MatchField::Title,
                    kind: MatchKind::Exact,
                    pattern: "b".into(),
                },
            ],
            require_all: true,
            actions: WindowRuleActions {
                maximize: true,
                ..Default::default()
            },
        };
        let info = WindowInfo {
            app_id: "a".into(),
            title: "nope".into(),
            class: String::new(),
        };
        assert!(!rule_matches(&rule, &info));
    }
}
