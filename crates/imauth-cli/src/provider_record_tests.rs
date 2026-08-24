use crate::provider_record::{RECORDER_SOURCE, REDACTION_SOURCE};

#[test]
fn embedded_runtime_contains_playwright_and_redaction_modules() {
    assert!(RECORDER_SOURCE.contains("from \"playwright\""));
    assert!(RECORDER_SOURCE.contains("provider-record-redaction.mjs"));
    assert!(REDACTION_SOURCE.contains("scanSanitizedText"));
}
