use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "omega/value.proto",
        "omega/storage.proto",
        "omega/host.proto",
        "omega/state/display.proto",
        "omega/state/media.proto",
        "omega/state/network.proto",
        "omega/state/power.proto",
        "omega/state/session.proto",
        "omega/state/system.proto",
        "omega/state/time.proto",
        "omega/state/plugins.proto",
        "omega/state.proto",
        "omega/action.proto",
        "omega/event.proto",
        "omega/plugin.proto",
        "omega/ui.proto",
        "omega/instance.proto",
        "omega/document.proto",
        "omega/wire.proto",
        "omega/preview.proto",
    ];

    // Use bundled protoc unless PROTOC selects a distributor-provided compiler.
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

    // Generate protobuf JSON serde implementations only when the json feature is enabled.
    if std::env::var_os("CARGO_FEATURE_JSON").is_some() {
        let descriptor_set = std::fs::read(descriptor_path)?;
        pbjson_build::Builder::new()
            .register_descriptors(&descriptor_set)?
            .build(&[".omega"])?;
    }

    println!("cargo:rerun-if-changed=schema");
    Ok(())
}
