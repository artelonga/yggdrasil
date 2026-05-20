//! Build script: generates minimal WASM stub files in `embedded/` for each
//! universo if they are absent. Real artifacts are produced by
//! `scripts/build-universes.sh` (YG-61) and overwrite these stubs. Without
//! this, `include_bytes!()` would fail at compile time before
//! `scripts/build-universes.sh` has been run.

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let embedded_dir = std::path::Path::new(&manifest_dir).join("embedded");
    std::fs::create_dir_all(&embedded_dir).expect("failed to create yggdrasil-web/embedded/");

    // Minimal valid WebAssembly: magic (4 bytes) + version (4 bytes), no sections.
    let stub: &[u8] = &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];

    for name in ["snake", "tetris", "invaders", "pointset", "poker", "vim"] {
        let path = embedded_dir.join(format!("{name}.wasm"));
        if !path.exists() {
            std::fs::write(&path, stub).unwrap_or_else(|e| {
                panic!(
                    "cannot write stub {name}.wasm: {e}\n\
                    Tip: run `scripts/build-universes.sh` to build real WASM artifacts."
                )
            });
        }
        // Recompile if the real artifact is replaced by build-universes.sh
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
