fn main() {
    // Note: boot.s assembly is not used for bootimage builds
    // The bootloader crate handles boot setup automatically
    // Custom boot assembly is only needed for manual multiboot builds

    // Rerun if these files change
    println!("cargo:rerun-if-changed=src/boot.s");
    println!("cargo:rerun-if-changed=link.ld");

    // ── Compile C compression libraries ──────────────────────────
    // These provide zstd, bzip2, and xz/lzma2 decompression for the
    // kernel package manager. They use a kernel compat layer (kcompat.h)
    // that maps malloc/free/memset to the RustOS kernel allocator.

    // On macOS hosts, the system `ar`/`ranlib` produce Mach-O archives
    // that rust-lld cannot link. Use llvm-ar / llvm-ranlib from the Rust toolchain.
    let llvm_tools = find_llvm_tools();
    if let Some((ar, ranlib)) = llvm_tools.as_ref() {
        let has_ranlib = ranlib.ends_with("llvm-ranlib");
        std::env::set_var("AR", &ar);
        std::env::set_var("AR_x86_64_rustos", &ar);
        std::env::set_var("AR_x86_64-rustos", &ar);
        std::env::set_var("AR_x86_64_unknown_none", &ar);
        std::env::set_var("AR_x86_64-unknown-none", &ar);
        if has_ranlib {
            std::env::set_var("RANLIB", &ranlib);
            std::env::set_var("RANLIB_x86_64_rustos", &ranlib);
            std::env::set_var("RANLIB_x86_64-rustos", &ranlib);
            std::env::set_var("RANLIB_x86_64_unknown_none", &ranlib);
            std::env::set_var("RANLIB_x86_64-unknown-none", &ranlib);
        }
    }

    let kcompat = "c_libs/kcompat.c";
    println!("cargo:rerun-if-changed={}", kcompat);

    // Zstd decompressor
    let zstd_src = "c_libs/zstd/zstd_decompress.c";
    println!("cargo:rerun-if-changed={}", zstd_src);
    println!("cargo:rerun-if-changed=c_libs/zstd/zstd_decompress.h");
    let mut zstd_build = cc::Build::new();
    configure_cc_tools(&mut zstd_build, llvm_tools.as_ref());
    zstd_build
        .file(kcompat)
        .file(zstd_src)
        .include("c_libs")
        .include("c_libs/zstd")
        .flag("-ffreestanding")
        .flag("-fno-stack-protector")
        .flag("-fno-exceptions")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-unused-variable")
        .flag("-Wno-unused-function")
        .flag("-Wno-sign-compare")
        .compile("zstd_decompress");
    rebuild_archive(
        "libzstd_decompress.a",
        &["kcompat.o", "zstd_decompress.o"],
        llvm_tools.as_ref(),
    );

    // Bzip2 decompressor
    let bzip2_src = "c_libs/bzip2/bzip2_decompress.c";
    println!("cargo:rerun-if-changed={}", bzip2_src);
    println!("cargo:rerun-if-changed=c_libs/bzip2/bzip2_decompress.h");
    let mut bzip2_build = cc::Build::new();
    configure_cc_tools(&mut bzip2_build, llvm_tools.as_ref());
    bzip2_build
        .file(bzip2_src)
        .include("c_libs")
        .include("c_libs/bzip2")
        .flag("-ffreestanding")
        .flag("-fno-stack-protector")
        .flag("-fno-exceptions")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-unused-variable")
        .flag("-Wno-unused-function")
        .flag("-Wno-sign-compare")
        .compile("bzip2_decompress");
    rebuild_archive(
        "libbzip2_decompress.a",
        &["bzip2_decompress.o"],
        llvm_tools.as_ref(),
    );

    // XZ/LZMA2 decompressor
    let xz_src = "c_libs/xz/xz_decompress.c";
    println!("cargo:rerun-if-changed={}", xz_src);
    println!("cargo:rerun-if-changed=c_libs/xz/xz_decompress.h");
    let mut xz_build = cc::Build::new();
    configure_cc_tools(&mut xz_build, llvm_tools.as_ref());
    xz_build
        .file(xz_src)
        .include("c_libs")
        .include("c_libs/xz")
        .flag("-ffreestanding")
        .flag("-fno-stack-protector")
        .flag("-fno-exceptions")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-unused-variable")
        .flag("-Wno-unused-function")
        .flag("-Wno-sign-compare")
        .compile("xz_decompress");
    rebuild_archive(
        "libxz_decompress.a",
        &["xz_decompress.o"],
        llvm_tools.as_ref(),
    );
}

fn configure_cc_tools(build: &mut cc::Build, tools: Option<&(String, String)>) {
    if let Some((ar, ranlib)) = tools {
        build.archiver(ar);
        if ranlib.ends_with("llvm-ranlib") {
            build.ranlib(ranlib);
        }
    }
}

fn rebuild_archive(archive_name: &str, object_suffixes: &[&str], tools: Option<&(String, String)>) {
    let Some((ar, ranlib)) = tools else {
        return;
    };
    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let out_dir = std::path::PathBuf::from(out_dir);
    let mut objects = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&out_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if object_suffixes.iter().any(|suffix| name.ends_with(suffix)) {
                objects.push(path);
            }
        }
    }
    if objects.is_empty() {
        return;
    }

    objects.sort();
    let archive = out_dir.join(archive_name);
    let _ = std::fs::remove_file(&archive);
    let mut cmd = std::process::Command::new(ar);
    cmd.arg("crs").arg(&archive);
    for object in &objects {
        cmd.arg(object);
    }
    if !cmd.status().map(|status| status.success()).unwrap_or(false) {
        return;
    }
    if ranlib.ends_with("llvm-ranlib") {
        let _ = std::process::Command::new(ranlib).arg(&archive).status();
    } else {
        let _ = std::process::Command::new(ar)
            .arg("s")
            .arg(&archive)
            .status();
    }
}

/// Locate `llvm-ar` and `llvm-ranlib` shipped with the active Rust toolchain.
fn find_llvm_tools() -> Option<(String, String)> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = std::process::Command::new(&rustc)
        .arg("-vV")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let host: &str = stdout.lines().find_map(|l| l.strip_prefix("host: "))?;
    let sysroot = std::process::Command::new(&rustc)
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())?;
    let bin_dir = format!("{sysroot}/lib/rustlib/{host}/bin");
    let ar = format!("{bin_dir}/llvm-ar");
    let ranlib = format!("{bin_dir}/llvm-ranlib");
    if !std::path::Path::new(&ar).exists() {
        return None;
    }
    let ranlib = if std::path::Path::new(&ranlib).exists() {
        ranlib
    } else {
        ar.clone()
    };
    Some((ar, ranlib))
}
