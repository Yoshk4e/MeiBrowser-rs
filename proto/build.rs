use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from("out");
    std::fs::create_dir_all(&out_dir)?;

    let mut config = prost_build::Config::new();
    config.out_dir(&out_dir);

    config.compile_protos(
        &[
            "proto/SophonPatchProto.proto",
            "proto/SophonManifestProto.proto",
        ],
        &["proto"],
    )?;
    println!("cargo:rerun-if-changed=proto/SophonPatchProto.proto");
    println!("cargo:rerun-if-changed=proto/SophonManifestProto.proto");

    Ok(())
}
