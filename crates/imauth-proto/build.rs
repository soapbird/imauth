use std::env;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let proto_dir = format!("{manifest_dir}/../../proto");

    let protos = [
        "imauth/v1/common.proto",
        "imauth/v1/auth.proto",
        "imauth/v1/session.proto",
        "imauth/v1/credential.proto",
    ];

    // Verify all proto files exist before attempting compilation
    for proto in &protos {
        let full_path = std::path::Path::new(&proto_dir).join(proto);
        if !full_path.exists() {
            panic!("Proto file not found: {:?}", full_path);
        }
    }

    tonic_build::configure()
        .out_dir("src/generated")
        .compile_protos(&protos, &[&proto_dir])
        .unwrap_or_else(|e| {
            panic!(
                "Failed to compile protos.\n  proto_dir: {proto_dir}\n  protos: {protos:?}\n  error: {e}"
            );
        });
}
