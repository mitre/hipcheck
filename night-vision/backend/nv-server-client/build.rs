use std::{env, fs, path::PathBuf};

fn main() {
    let spec_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("../openapi/nv-server-openapi.json");
    println!("cargo:rerun-if-changed={}", spec_path.display());

    let spec_file = fs::File::open(&spec_path).expect("open OpenAPI description");
    let spec = serde_json::from_reader(spec_file).expect("parse OpenAPI description");
    let mut generation_settings = progenitor::GenerationSettings::default();
    let settings = generation_settings.with_interface(progenitor::InterfaceStyle::Builder);
    let mut generator = progenitor::Generator::new(settings);
    let tokens = generator
        .generate_tokens(&spec)
        .expect("generate OpenAPI client");
    let syntax = syn::parse2(tokens).expect("parse generated OpenAPI client");
    let source = prettyplease::unparse(&syntax);
    let output = PathBuf::from(env::var("OUT_DIR").expect("output directory")).join("client.rs");
    fs::write(output, source).expect("write generated OpenAPI client");
}
