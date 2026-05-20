fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let proto_dir = format!("{manifest_dir}/../../proto");

    // Debug: verify proto dir exists
    println!("cargo:warning=proto_dir = {proto_dir}");
    let proto_path = std::path::Path::new(&proto_dir);
    if !proto_path.exists() {
        panic!("proto dir does not exist: {proto_dir}");
    }
    for entry in std::fs::read_dir(proto_path.join("imauth/v1")).unwrap() {
        let e = entry.unwrap();
        println!("cargo:warning=  proto file: {:?}", e.path());
    }

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
