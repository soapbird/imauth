fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let proto_dir = format!("{manifest_dir}/../../proto");

    // Debug: verify proto dir exists
    println!("cargo:warning=proto_dir = {proto_dir}");
    let which_protoc = std::process::Command::new("which").arg("protoc").output();
    match which_protoc {
        Ok(out) => println!("cargo:warning=which protoc = {}", String::from_utf8_lossy(&out.stdout).trim()),
        Err(e) => println!("cargo:warning=which protoc FAILED: {e}"),
    }
    if let Ok(protoc_ver) = std::process::Command::new("protoc").arg("--version").output() {
        println!("cargo:warning=protoc --version = {}", String::from_utf8_lossy(&protoc_ver.stdout).trim());
    } else {
        println!("cargo:warning=protoc --version FAILED");
    }
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
