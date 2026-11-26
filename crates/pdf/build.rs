//! Embed the PDFium shared library when it is available at build time.
//!
//! PDFium is not distributed as a static library, so the practical way to ship
//! one executable is to carry the shared library inside it and write it to the
//! user's cache before the first load. The runtime search still lets an explicit
//! `LIDE_PDFIUM_PATH` or a library beside the binary take precedence.

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rustc-check-cfg=cfg(embed_pdfium)");
    println!("cargo:rerun-if-env-changed=LIDE_EMBED_PDFIUM");

    if env::var_os("CARGO_FEATURE_PDFIUM").is_none() {
        return;
    }

    let target = env::var("TARGET").unwrap_or_default();
    let host = env::var("HOST").unwrap_or_default();
    let name = library_name(&target);

    // The checkout's library is the default for a native build. Watch it even
    // when it is missing so that fetching it later rebuilds this crate.
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/pdfium/lib")
        .join(name);
    println!("cargo:rerun-if-changed={}", checkout.display());

    let requested = env::var_os("LIDE_EMBED_PDFIUM").map(PathBuf::from);
    if requested.as_deref() == Some(Path::new("0")) {
        return;
    }
    let explicit = requested.as_deref().filter(|path| *path != Path::new("1"));
    if let Some(path) = explicit {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // Only use the checkout's library when it is for the machine we are
    // compiling for; a cross build must name the target library explicitly.
    let source = match explicit {
        Some(path) if path.is_file() => Some(path.to_path_buf()),
        Some(path) => {
            println!(
                "cargo:warning=LIDE_EMBED_PDFIUM points at {}, which is not a file; PDFium will not be embedded",
                path.display()
            );
            return;
        }
        None => (host == target && checkout.is_file()).then_some(checkout),
    };
    let Some(source) = source else {
        return;
    };

    let bytes = match std::fs::read(&source) {
        Ok(bytes) => bytes,
        Err(error) => {
            println!(
                "cargo:warning=could not read {} for embedding: {error}",
                source.display()
            );
            return;
        }
    };
    if bytes.is_empty() {
        return;
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let destination = out.join(name);
    if let Err(error) = std::fs::write(&destination, &bytes) {
        println!(
            "cargo:warning=could not copy {} into the binary: {error}",
            source.display()
        );
        return;
    }

    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());

    println!("cargo:rustc-cfg=embed_pdfium");
    println!("cargo:rustc-env=LIDE_EMBEDDED_PDFIUM_NAME={name}");
    println!("cargo:rustc-env=LIDE_EMBEDDED_PDFIUM_HASH={hash}");
    println!("cargo:rustc-env=LIDE_EMBEDDED_PDFIUM_BYTES={}", bytes.len());
    println!("cargo:rerun-if-changed={}", source.display());
}

fn library_name(target: &str) -> &'static str {
    if target.contains("windows") {
        "pdfium.dll"
    } else if target.contains("apple") || target.contains("darwin") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}
