fn main() {
    let proto_dir = "../../proto";
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
