//! Cut the main subject out of an image using the installed u2netp model.
//!
//! Usage: `cargo run -p slopos-vision --example lift_subject -- <image> [out.png] [models_dir]`
//!
//! Writes a transparent-background PNG cutout and reports the mask
//! foreground coverage, which verifies the segmentation output selection.

use slopos_vision::engine::{VisionEngine, VisionEngineConfig};
use slopos_vision::types::SegmentationOptions;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let image_path = args
        .next()
        .unwrap_or_else(|| "models/vision/subject_test.png".into());
    let out_path = args.next().unwrap_or_else(|| "target/lifted.png".into());
    let models_dir = args.next().unwrap_or_else(|| "models/vision".into());

    let engine = VisionEngine::load(VisionEngineConfig {
        models_dir: models_dir.into(),
        ..Default::default()
    })?;
    let image = engine.decode_image(Path::new(&image_path))?;
    let lifted = engine.lift_subject(&image, SegmentationOptions::default())?;

    let alpha = &lifted.mask.alpha;
    let fg = alpha.iter().filter(|&&a| a > 128).count();
    let pct = 100.0 * fg as f64 / alpha.len() as f64;
    println!(
        "mask: {}x{}, {:.1}% foreground ({} px alpha > 128)",
        lifted.mask.width, lifted.mask.height, pct, fg
    );
    lifted.image.save(&out_path)?;
    println!("saved cutout to {out_path}");
    Ok(())
}
