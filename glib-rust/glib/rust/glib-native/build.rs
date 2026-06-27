//! Generate `include/glib_native.h` from `src/ffi.rs` via cbindgen, falling back
//! to the committed template when cbindgen is unavailable or fails.

use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ffi_rs = manifest_dir.join("src/ffi.rs");
    let header_path = manifest_dir.join("include/glib_native.h");
    let template_path = manifest_dir.join("include/glib_native.h.template");

    println!("cargo:rerun-if-changed={}", ffi_rs.display());
    println!("cargo:rerun-if-changed={}", template_path.display());

    let mut config = cbindgen::Config::default();
    config.language = cbindgen::Language::C;
    config.header =
        Some("glib_native.h — C ABI for glib-native (auto-generated or template fallback)".into());
    config.include_guard = Some("GLIB_NATIVE_H".into());
    config.cpp_compat = true;
    config.documentation = false;
    config.usize_is_size_t = true;

    let generated = std::panic::catch_unwind(|| {
        cbindgen::Builder::new()
            .with_crate(&manifest_dir)
            .with_config(config)
            .generate()
    });

    match generated {
        Ok(Ok(bindings)) => {
            if !bindings.write_to_file(&header_path) {
                panic!("failed to write include/glib_native.h");
            }
        }
        Ok(Err(err)) => {
            eprintln!(
                "cbindgen: warning: {err}; using template {}",
                template_path.display()
            );
            if template_path.exists() {
                std::fs::copy(&template_path, &header_path)
                    .expect("failed to copy glib_native.h template");
            } else if !header_path.exists() {
                panic!(
                    "cbindgen failed and no template at {}",
                    template_path.display()
                );
            }
        }
        Err(_) => {
            eprintln!(
                "cbindgen: panicked during generation; using template {}",
                template_path.display()
            );
            if template_path.exists() {
                std::fs::copy(&template_path, &header_path)
                    .expect("failed to copy glib_native.h template");
            } else if !header_path.exists() {
                panic!(
                    "cbindgen panicked and no template at {}",
                    template_path.display()
                );
            }
        }
    }
}
