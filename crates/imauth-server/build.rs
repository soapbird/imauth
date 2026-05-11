fn main() {
    let proto_dir = std::path::PathBuf::from("../../proto");
    let out_dir = std::path::PathBuf::from("src/generated");
    std::fs::create_dir_all(&out_dir).unwrap();

    let protos = [
        "../../proto/imauth/v1/common.proto",
        "../../proto/imauth/v1/auth.proto",
        "../../proto/imauth/v1/session.proto",
        "../../proto/imauth/v1/credential.proto",
    ];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .compile_protos(&protos, &[&proto_dir])
        .expect("Failed to compile protos");

    println!("cargo:rerun-if-changed=../../proto/");
}
