//! Debug helper: print input shapes of the installed ONNX models.
//! Not part of the public API; used during development.

use rten::Model;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "models/vision".into());
    for name in [
        "ch_PP-OCRv4_det_infer.onnx",
        "ch_PP-OCRv4_rec_infer.onnx",
        "u2netp.onnx",
    ] {
        let path = std::path::Path::new(&dir).join(name);
        match Model::load_file(&path) {
            Ok(model) => {
                println!("== {name}");
                println!("  inputs: {}", model.input_ids().len());
                for i in 0..model.input_ids().len() {
                    println!("    input[{i}] shape = {:?}", model.input_shape(i));
                }
                println!("  outputs: {}", model.output_ids().len());
                for i in 0..model.output_ids().len() {
                    let name = model
                        .node_info(model.output_ids()[i])
                        .and_then(|n| n.name());
                    let shape = model
                        .node_info(model.output_ids()[i])
                        .and_then(|n| n.shape())
                        .map(|s| format!("{s:?}"))
                        .unwrap_or_default();
                    println!("    output[{i}] name = {name:?} shape = {shape}");
                }
                println!("  params: {}", model.total_params());
            }
            Err(err) => println!("== {name}: ERROR {err}"),
        }
    }
}
