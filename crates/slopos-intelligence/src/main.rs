//! Text-first, opt-in, local-only demonstration of SLOPOS Intelligence.
//! Not a daemon and not loaded at login. No filesystem indexing, microphones,
//! screenshots, remote inference, or process execution without confirmation.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use slopos_core::{
    desktop_entry::scan_desktop_apps,
    process::{resolve_program, spawn_detached},
};
use slopos_intelligence::{
    authorize, build_prompt, disclosed_apps, launch_plan, parse_proposal, Decision,
};
use std::{
    env,
    io::{self, IsTerminal, Read, Write},
    time::Duration,
};

const MAX_OLLAMA_RESPONSE_BYTES: usize = 1024 * 1024;

/// Escape terminal control characters originating from installed-app metadata.
fn printable(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        if ch.is_control() {
            escaped.extend(ch.escape_default());
        } else {
            escaped.push(ch);
        }
    }
    escaped
}

fn print_apps(apps: &[slopos_core::desktop_entry::DesktopApp]) {
    for app in apps {
        println!("{}\t{}", printable(&app.id), printable(&app.name));
    }
}

fn run() -> Result<(), String> {
    let apps = disclosed_apps(scan_desktop_apps());
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--list" {
        print_apps(&apps);
        return Ok(());
    }
    let request = args.join(" ");
    if request.is_empty() {
        return Err("usage: slopos-intelligence --list | <natural language request>".into());
    }
    let prompt = build_prompt(&request, &apps)?;
    // Loopback only: no cloud endpoint or user-controlled remote URL.
    let response = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?
        .post("http://127.0.0.1:11434/api/generate")
        .json(&json!({
            "model": "lfm3:1.7b",
            "stream": false,
            "format": "json",
            "prompt": prompt,
            "options": { "num_predict": 128, "temperature": 0 }
        }))
        .send()
        .map_err(|e| format!("local Ollama unavailable: {e}"))?;
    // Reject redirects and errors. A malicious loopback listener must not
    // redirect model context to a network destination.
    if !response.status().is_success() {
        return Err(format!("local Ollama returned HTTP {}", response.status()));
    }
    let mut body = Vec::new();
    response
        .take((MAX_OLLAMA_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|e| format!("cannot read local Ollama response: {e}"))?;
    if body.len() > MAX_OLLAMA_RESPONSE_BYTES {
        return Err("Ollama response exceeds 1 MiB".into());
    }
    let response: Value =
        serde_json::from_slice(&body).map_err(|e| format!("invalid Ollama response: {e}"))?;
    let output = response
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| "Ollama returned no text proposal".to_string())?;
    match authorize(parse_proposal(output)?, &apps)? {
        Decision::ListApps => print_apps(&apps),
        Decision::Help => {
            println!("I can list installed applications or open one with your approval.");
        }
        Decision::ConfirmOpen(app) => {
            println!(
                "SLOPOS proposes opening: {} ({})",
                printable(&app.name),
                printable(&app.id)
            );
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return Err("launch confirmation requires an interactive terminal".into());
            }
            print!("Confirm launch? Type yes: ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut response = String::new();
            io::stdin()
                .read_line(&mut response)
                .map_err(|e| e.to_string())?;
            if response.trim() != "yes" {
                println!("Cancelled.");
                return Ok(());
            }
            // Only trusted desktop-entry argv is executed, with no shell.
            // Terminal=true programs must run inside a supported terminal.
            let terminal = if app.terminal {
                ["xfce4-terminal", "xterm"]
                    .iter()
                    .find_map(|name| resolve_program(name))
            } else {
                None
            };
            let (program, arguments) = launch_plan(app, terminal.as_deref())?;
            spawn_detached(&program, &arguments)
                .map_err(|e| format!("launch failed: {e}"))?;
            println!("Launch requested.");
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("SLOPOS Intelligence: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_metadata_cannot_inject_escape_sequences() {
        let escaped = printable("Name\u{1b}[31m\u{7}test\n");
        assert_eq!(escaped, r"Name\u{1b}[31m\u{7}test\n");
        assert!(!escaped.contains(char::from(27)));
        assert!(!escaped.contains(char::from(7)));
    }
}
