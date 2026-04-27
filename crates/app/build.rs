use std::{env, fs, path::PathBuf};

fn main() {
    tauri_build::build();

    // Copy pdfium.dll next to the compiled binary so `cargo tauri dev` can find it.
    // In production bundles, tauri.conf.json `bundle.resources` handles this instead.
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dll_src = manifest_dir
        .join("..")
        .join("..")
        .join("resources")
        .join("pdfium")
        .join("pdfium.dll");

    if dll_src.exists() {
        // OUT_DIR = target/{triple}/{profile}/build/{crate}-{hash}/out
        // Three ancestors up = target/{triple}/{profile}/
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        if let Some(target_dir) = out_dir.ancestors().nth(3) {
            let dst = target_dir.join("pdfium.dll");
            if let Err(e) = fs::copy(&dll_src, &dst) {
                println!("cargo:warning=Failed to copy pdfium.dll to {}: {e}", dst.display());
            }
        }
    } else {
        println!("cargo:warning=pdfium.dll not found at {}", dll_src.display());
    }

    println!(
        "cargo:rerun-if-changed={}",
        dll_src.display()
    );
}
