//! Text-first, opt-in, local-only demonstration of SLOPOS Intelligence.
//! Not a daemon and not loaded at login. No filesystem indexing, microphones,
//! screenshots, remote inference, or process execution without confirmation.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use slopos_core::{desktop_entry::scan_desktop_apps, process::spawn_detached};
use slopos_intelligence::{authorize, build_prompt, disclosed_apps, parse_proposal, Decision};
use std::{
    env,
    io::{self, Write},
    path::Path,
    time::Duration,
};

fn run() -> Result<(), String> {
    let apps = disclosed_apps(scan_desktop_apps());
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--list" {
        for app in &apps {
            println!("{}\t{}", app.id, app.name);
        }
        return Ok(());
    }
    let request = args.join(" ");
    if request.is_empty() {
        return Err("usage: slopos-intelligence --list | <natural language request>".into());
    }
    let prompt = build_prompt(&request, &apps)?;
    // Loopback only: no cloud endpoint or user-controlled remote URL.
    let response: Value = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
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
        .map_err(|e| format!("local Ollama unavailable: {e}"))?
        .error_for_status()
        .map_err(|e| format!("local Ollama request failed: {e}"))?
        .json()
        .map_err(|e| format!("invalid Ollama response: {e}"))?;
    let output = response
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| "Ollama returned no text proposal".to_string())?;
    match authorize(parse_proposal(output)?, &apps)? {
        Decision::ListApps => {
            for app in &apps {
                println!("{}\t{}", app.id, app.name);
            }
        }
        Decision::Help => {
            println!("I can list installed applications or open one with your approval.");
        }
        Decision::ConfirmOpen(app) => {
            println!("SLOPOS proposes opening: {} ({})", app.name, app.id);
            print!("Confirm launch? Type yes: ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut response = String::new();
            io::stdin().read_line(&mut response).map_err(|e| e.to_string())?;
            if response.trim() != "yes" {
                println!("Cancelled.");
                return Ok(());
            }
            // A trusted local .desktop entry supplied this argv. The model
            // supplies only an exact registry id; no shell interpretation.
            let (program, args) = app.argv.split_first()
                .ok_or_else(|| "application has no executable".to_string())?;
            spawn_detached(Path::new(program), args)
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
