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

    let kcompat = "c_libs/kcompat.c";
    println!("cargo:rerun-if-changed={}", kcompat);

    // Zstd decompressor
    let zstd_src = "c_libs/zstd/zstd_decompress.c";
    println!("cargo:rerun-if-changed={}", zstd_src);
    println!("cargo:rerun-if-changed=c_libs/zstd/zstd_decompress.h");
    cc::Build::new()
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

    // Bzip2 decompressor
    let bzip2_src = "c_libs/bzip2/bzip2_decompress.c";
    println!("cargo:rerun-if-changed={}", bzip2_src);
    println!("cargo:rerun-if-changed=c_libs/bzip2/bzip2_decompress.h");
    cc::Build::new()
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

    // XZ/LZMA2 decompressor
    let xz_src = "c_libs/xz/xz_decompress.c";
    println!("cargo:rerun-if-changed={}", xz_src);
    println!("cargo:rerun-if-changed=c_libs/xz/xz_decompress.h");
    cc::Build::new()
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
}
