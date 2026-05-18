#![allow(clippy::result_large_err)]

//! Bearer/x-api-key authentication interceptor shared by the binary and the
//! integration tests. Keeping it in a single place means the constant-time
//! comparison + whitespace-trim logic can't drift between the production code
//! path and what the tests exercise.

use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Returns the configured API key, or `None` when it is missing, empty, or
/// whitespace-only. An empty `IMAUTH_API_KEY` must NOT enable auth — otherwise
/// callers could authenticate with `Authorization: Bearer ` (empty token).
pub fn normalize_api_key(raw: Option<String>) -> Option<String> {
    raw.map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// Build the tonic interceptor closure. When `api_key` is `None` the server
/// runs without authentication; when `Some`, every request must carry either
/// `Authorization: Bearer <key>` or `x-api-key: <key>` matching in constant
/// time.
pub fn auth_interceptor(
    api_key: Option<Arc<String>>,
) -> impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone {
    move |req: tonic::Request<()>| {
        if let Some(ref key) = api_key {
            let provided = req
                .metadata()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .or_else(|| {
                    req.metadata()
                        .get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                });

            match provided {
                Some(k) if bool::from(k.as_bytes().ct_eq(key.as_bytes())) => Ok(req),
                _ => Err(tonic::Status::unauthenticated("Invalid or missing API key")),
            }
        } else {
            Ok(req)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_key_is_none() {
        assert_eq!(normalize_api_key(None), None);
    }

    #[test]
    fn empty_key_is_treated_as_none() {
        assert_eq!(normalize_api_key(Some(String::new())), None);
    }

    #[test]
    fn whitespace_only_key_is_treated_as_none() {
        assert_eq!(normalize_api_key(Some("   \t\n".to_string())), None);
    }

    #[test]
    fn real_key_is_preserved_after_trim() {
        assert_eq!(
            normalize_api_key(Some("  abc123  ".to_string())),
            Some("abc123".to_string())
        );
    }

    fn req_with(header: &'static str, value: &str) -> tonic::Request<()> {
        let mut req = tonic::Request::new(());
        req.metadata_mut()
            .insert(header, value.parse().unwrap());
        req
    }

    #[test]
    fn no_auth_required_when_key_is_none() {
        let interceptor = auth_interceptor(None);
        assert!(interceptor(tonic::Request::new(())).is_ok());
        assert!(interceptor(req_with("authorization", "Bearer anything")).is_ok());
    }

    #[test]
    fn rejects_when_key_required_and_missing() {
        let interceptor = auth_interceptor(Some(Arc::new("expected".to_string())));
        let err = interceptor(tonic::Request::new(())).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn accepts_bearer_match() {
        let interceptor = auth_interceptor(Some(Arc::new("expected".to_string())));
        assert!(interceptor(req_with("authorization", "Bearer expected")).is_ok());
    }

    #[test]
    fn accepts_x_api_key_match() {
        let interceptor = auth_interceptor(Some(Arc::new("expected".to_string())));
        assert!(interceptor(req_with("x-api-key", "expected")).is_ok());
    }

    #[test]
    fn rejects_wrong_key_with_unauthenticated() {
        let interceptor = auth_interceptor(Some(Arc::new("expected".to_string())));
        let err = interceptor(req_with("authorization", "Bearer wrong")).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}
