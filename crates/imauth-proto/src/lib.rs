pub mod generated {
    pub mod v1 {
        // tonic-generated stubs return `Result<_, tonic::Status>`, which trips
        // clippy::result_large_err on rustc 1.98+. The file is generated; the
        // lint is allowed here instead of editing generated code.
        #![allow(clippy::result_large_err)]

        include!("generated/imauth.v1.rs");
    }
}
