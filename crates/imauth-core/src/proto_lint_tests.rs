#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    #[test]
    fn session_proto_uses_correct_korean_export_comment() {
        let proto_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../proto/imauth/v1/session.proto");
        let proto = fs::read_to_string(&proto_path).expect("session.proto should be readable");

        assert!(
            proto.contains("Netscape 포맷으로 쿠키 내보내기"),
            "expected corrected Korean export comment in {}",
            proto_path.display()
        );
        assert!(
            !proto.contains("Netscape 포맷으로 쿠키 내보기"),
            "expected old Korean typo to be removed from {}",
            proto_path.display()
        );
    }
}
