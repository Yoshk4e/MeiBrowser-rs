use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from("out");
    std::fs::create_dir_all(&out_dir)?;

    let protos = ["SophonPatchProto.proto", "SophonManifestProto.proto"];
    if protos.iter().all(|p| PathBuf::from(p).exists()) {
        let mut config = prost_build::Config::new();
        config.out_dir(&out_dir);

        if let Err(e) = config.compile_protos(&protos, &["."]) {
            println!("cargo:warning=skipping proto codegen: {e}");
        }
    } else {
        println!("cargo:warning=.proto sources not found, skipping codegen");
    }

    println!("cargo:rerun-if-changed=SophonPatchProto.proto");
    println!("cargo:rerun-if-changed=SophonManifestProto.proto");
    Ok(())
}
