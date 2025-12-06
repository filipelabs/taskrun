use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the project root (workspace root)
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let proto_dir = workspace_root.join("proto");

    // Proto files to compile
    let proto_files = [
        proto_dir.join("taskrun/v1/common.proto"),
        proto_dir.join("taskrun/v1/run_service.proto"),
        proto_dir.join("taskrun/v1/task_service.proto"),
        proto_dir.join("taskrun/v1/worker_service.proto"),
    ];

    // Tell Cargo to rerun if proto files change
    for proto in &proto_files {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    // Configure and run tonic-build
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/gen")
        .compile_protos(&proto_files, &[proto_dir])?;

    Ok(())
}
