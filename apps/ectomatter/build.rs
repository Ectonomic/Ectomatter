// The window's Slint, compiled with the design tokens it draws with: the build
// writes design/ectomatter.tokens.json out as tokens.slint (the globals every
// .slint file imports) and tokens.rs (src/tokens.rs takes it in, for the pages
// laid out in Rust), so the two always hold the same values.
fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let file = manifest.join("../../design/ectomatter.tokens.json");
    println!("cargo:rerun-if-changed={}", file.display());
    let json = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("can't read {}: {e}", file.display()));
    let tokens = ectomatter_tokens::generate(&json).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    std::fs::write(out.join("tokens.slint"), tokens.slint).unwrap();
    std::fs::write(out.join("tokens.rs"), tokens.rust).unwrap();
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_include_paths(vec![out]);
    slint_build::compile_with_config("ui/app.slint", config).unwrap();
}
