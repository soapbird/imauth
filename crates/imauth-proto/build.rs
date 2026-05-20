fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let proto_dir = format!("{manifest_dir}/../../proto");
    tonic_build::configure()
        .out_dir("src/generated")
        .compile_protos(
            &[
                "imauth/v1/common.proto",
                "imauth/v1/auth.proto",
                "imauth/v1/session.proto",
                "imauth/v1/credential.proto",
            ],
            &[proto_dir],
        )
        .expect("Failed to compile protos");
}
