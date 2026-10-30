//! SLOPOS Software. This first slice is the trust and transaction core
//! (STORE-01) with a command-line front end for catalogue publishers; the
//! native window arrives with STORE-02.
//!
//! slopos-software keygen KEY-FILE
//! slopos-software sign KEY-FILE CATALOGUE.json        (signs in place)
//! slopos-software verify TRUST.json CATALOGUE.json

// The installer is driven by the window added in STORE-02.
#[allow(dead_code)]
mod install;
mod trust;

use std::path::Path;
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use trust::{signing, SignedCatalogue, TrustStore};

fn read_key(path: &Path) -> Result<SigningKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes: [u8; 32] = hex::decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("the key file must hold 64 hex digits")?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn keygen(path: &Path) -> Result<(), String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let mut seed = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut seed))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    let key = SigningKey::from_bytes(&seed);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::Write::write_all(&mut file, hex::encode(seed).as_bytes())
        .map_err(|e| e.to_string())?;
    println!("public key: {}", signing::public_key_hex(&key));
    Ok(())
}

fn sign(key_path: &Path, catalogue_path: &Path) -> Result<(), String> {
    let key = read_key(key_path)?;
    let bytes = std::fs::read(catalogue_path).map_err(|e| e.to_string())?;
    let mut catalogue = SignedCatalogue::from_json(&bytes).map_err(|e| e.to_string())?;
    for entry in &catalogue.entries {
        entry.validate().map_err(|e| e.to_string())?;
    }
    signing::sign_catalogue(&mut catalogue, &key);
    let json = serde_json::to_string_pretty(&catalogue).map_err(|e| e.to_string())?;
    std::fs::write(catalogue_path, json + "\n").map_err(|e| e.to_string())?;
    println!(
        "signed {} entries with key {}",
        catalogue.entries.len(),
        catalogue.key_id
    );
    Ok(())
}

fn verify(trust_path: &Path, catalogue_path: &Path) -> Result<(), String> {
    let trust = TrustStore::load(trust_path).map_err(|e| e.to_string())?;
    let bytes = trust::read_protected(catalogue_path, "catalogue").map_err(|e| e.to_string())?;
    let catalogue = SignedCatalogue::from_json(&bytes).map_err(|e| e.to_string())?;
    catalogue.verify(&trust).map_err(|e| e.to_string())?;
    println!(
        "verified {} entries from {}",
        catalogue.entries.len(),
        catalogue.publisher
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", key] => keygen(Path::new(key)),
        ["sign", key, catalogue] => sign(Path::new(key), Path::new(catalogue)),
        ["verify", trust, catalogue] => verify(Path::new(trust), Path::new(catalogue)),
        _ => Err(
            "usage: slopos-software keygen KEY | sign KEY CATALOGUE | verify TRUST CATALOGUE"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopos-software: {error}");
            ExitCode::FAILURE
        }
    }
}
