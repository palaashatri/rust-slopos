//! End-to-end OCR over an image using the installed PP-OCRv4 models.
//!
//! Usage: `cargo run -p slopos-vision --example extract_text -- <image> [models_dir]`
//!
//! Prints the recognized lines with per-line confidence.

use slopos_vision::engine::{VisionEngine, VisionEngineConfig};
use slopos_vision::types::OcrOptions;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let image_path = args
        .next()
        .unwrap_or_else(|| "models/vision/ocr_test.png".into());
    let models_dir = args.next().unwrap_or_else(|| "models/vision".into());

    let engine = VisionEngine::load(VisionEngineConfig {
        models_dir: models_dir.into(),
        ..Default::default()
    })?;
    let image = engine.decode_image(Path::new(&image_path))?;
    let result = engine.extract_text(&image, OcrOptions::default())?;

    println!("image: {}x{}", result.image_width, result.image_height);
    println!("--- recognized text ---");
    for line in &result.lines {
        let conf = line
            .confidence
            .map(|c| format!("{c:.3}"))
            .unwrap_or_else(|| "-".into());
        println!("[{conf}] {}", line.text);
    }
    println!(
        "({} lines, {} words)",
        result.lines.len(),
        result.all_words().count()
    );
    Ok(())
}
