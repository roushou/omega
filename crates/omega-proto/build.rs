use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "omega/value.proto",
        "omega/state/display.proto",
        "omega/state/media.proto",
        "omega/state/network.proto",
        "omega/state/power.proto",
        "omega/state/units.proto",
        "omega/state.proto",
        "omega/action.proto",
        "omega/event.proto",
        "omega/unit.proto",
        "omega/ui.proto",
        "omega/document.proto",
        "omega/wire.proto",
    ];

    // Where protoc comes from.
    //
    // A published crate is built on machines that have never heard of
    // protobuf, and `cargo install omega-cli` failing with "could not find
    // protoc" is not an answer — the schema is omega's business, not the
    // reader's. The bundled compiler is used unless the environment names
    // another, so a distributor with its own toolchain still wins.
    if std::env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        // SAFETY: a build script is single-threaded at this point, and this
        // runs before anything reads the environment.
        unsafe { std::env::set_var("PROTOC", protoc) };
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let descriptor_path = out_dir.join("omega_descriptor.bin");

    // 1. Generate the prost types, and save the descriptor set pbjson needs.
    prost_build::Config::new()
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&protos, &[PathBuf::from("schema")])?;

    // 2. Generate the serde impls (canonical protobuf JSON mapping).
    let descriptor_set = std::fs::read(descriptor_path)?;
    pbjson_build::Builder::new()
        .register_descriptors(&descriptor_set)?
        .build(&[".omega"])?;

    println!("cargo:rerun-if-changed=schema");
    Ok(())
}
