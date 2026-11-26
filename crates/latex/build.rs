//! Builds the SyncTeX reference parser that is vendored in `third_party`.

fn main() {
    let dir = std::path::Path::new("../../third_party/synctex");
    let files = ["synctex_parser.c", "synctex_parser_utils.c"];
    let mut build = cc::Build::new();
    build.include(dir).warnings(false).flag_if_supported("-w");
    for file in files {
        build.file(dir.join(file));
        println!("cargo:rerun-if-changed={}", dir.join(file).display());
    }
    build.compile("synctex");
    // The parser reads `.synctex.gz` through zlib.
    println!("cargo:rustc-link-lib=z");
}
