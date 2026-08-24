// tonic::Status is intentionally large; this is a known clippy lint we accept.
#![allow(clippy::result_large_err)]

use futures::Stream;
use imauth_core::application::login::LoginEvent;
use imauth_core::domain::session::{Cookie, Session, SessionState};
use imauth_core::domain::Platform;
use imauth_core::AppContainer;
use imauth_proto::generated::v1::{
    auth_service_server::AuthService, credential_service_server::CredentialService,
    session_service_server::SessionService, AuthEvent, AuthResponse, AuthStatus as ProtoAuthStatus,
    AuthStatusResponse, CancelRequest, ConnectionStatusMap, Cookie as ProtoCookie, CookieList,
    CredentialInfo, CredentialResponse, DeleteCredentialRequest, Empty, ExportRequest,
    GetCookiesRequest, GetCredentialRequest, LoginRequest, NetscapeExport,
    Platform as ProtoPlatform, SaveCredentialRequest, StatusRequest, UpdateCookiesRequest,
    ValidateRequest, ValidationResult,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

// ---- proto ↔ domain converters (server-crate concerns) ------------------------

fn map_auth_err(err: imauth_core::ImauthError) -> tonic::Status {
    use imauth_core::ImauthError;
    match err {
        ImauthError::NotFound(m) => tonic::Status::not_found(m),
        ImauthError::Platform(m) => tonic::Status::invalid_argument(m),
        other => {
            tracing::error!(error = %other, "Internal error mapped to gRPC status");
            tonic::Status::internal("Internal server error")
        }
    }
}

fn platform_from_proto(p: i32) -> Option<Platform> {
    match ProtoPlatform::try_from(p).ok()? {
        ProtoPlatform::Instagram => Some(Platform::Instagram),
        ProtoPlatform::Threads => Some(Platform::Threads),
        ProtoPlatform::Naver => Some(Platform::Naver),
        ProtoPlatform::Novelpia => Some(Platform::Novelpia),
        ProtoPlatform::Unspecified => None,
    }
}

fn session_state_to_proto(state: &SessionState) -> ProtoAuthStatus {
    match state {
        SessionState::Idle => ProtoAuthStatus::Idle,
        SessionState::Loading => ProtoAuthStatus::Loading,
        SessionState::Authenticating => ProtoAuthStatus::Authenticating,
        SessionState::WaitingForUser => ProtoAuthStatus::WaitingForUser,
        SessionState::Connected => ProtoAuthStatus::Connected,
        SessionState::Failed => ProtoAuthStatus::Failed,
    }
}

fn cookie_to_proto(c: &Cookie) -> ProtoCookie {
    ProtoCookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: c.domain.clone(),
        path: c.path.clone(),
        expires: c.expires.map(|dt| dt.timestamp()).unwrap_or(0),
        http_only: c.http_only,
        secure: c.secure,
    }
}

fn proto_cookie_from(c: &ProtoCookie) -> Cookie {
    Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: c.domain.clone(),
        path: c.path.clone(),
        expires: if c.expires == 0 {
            None
        } else {
            chrono::DateTime::from_timestamp(c.expires, 0)
        },
        http_only: c.http_only,
        secure: c.secure,
    }
}

fn auth_event_from(session: &Session) -> AuthEvent {
    AuthEvent {
        session_id: session.id.clone(),
        status: session_state_to_proto(&session.state) as i32,
        message: session.message.clone().unwrap_or_default(),
        requires_input: session.requires_input,
        input_type: session.input_type.clone().unwrap_or_default(),
        cookies: vec![],
        viewer_url: String::new(),
    }
}

// ---- AuthService -------------------------------------------------------------

pub struct AuthGrpcService {
    container: Arc<AppContainer>,
}

impl AuthGrpcService {
    pub fn new(container: Arc<AppContainer>) -> Self {
        Self { container }
    }
}

#[tonic::async_trait]
impl AuthService for AuthGrpcService {
    type LoginStream = Pin<Box<dyn Stream<Item = Result<AuthEvent, Status>> + Send>>;

    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<Self::LoginStream>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let container = self.container.clone();
        let (tx, rx) = mpsc::channel::<LoginEvent>(10);

        tokio::spawn(async move {
            container.login.execute(platform, tx).await;
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|event| {
            let (session, cookies, viewer_url) = match event {
                LoginEvent::Started(s) => (s, vec![], String::new()),
                LoginEvent::WaitingForUser(s, url) => (s, vec![], url),
                LoginEvent::Final(s, c) => (s, c, String::new()),
            };
            let mut evt = auth_event_from(&session);
            evt.cookies = cookies.iter().map(cookie_to_proto).collect();
            evt.viewer_url = viewer_url;
            Ok::<AuthEvent, Status>(evt)
        });

        Ok(Response::new(Box::pin(stream) as Self::LoginStream))
    }

    async fn get_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<AuthStatusResponse>, Status> {
        let req = request.into_inner();
        let session = self.container.get_status.execute(&req.session_id).await;
        let session = session.map_err(map_auth_err)?;

        Ok(Response::new(AuthStatusResponse {
            session_id: session.id,
            status: session_state_to_proto(&session.state) as i32,
            message: session.message.clone().unwrap_or_default(),
            requires_input: session.requires_input,
            input_type: session.input_type.clone().unwrap_or_default(),
        }))
    }

    async fn cancel(
        &self,
        request: Request<CancelRequest>,
    ) -> Result<Response<AuthResponse>, Status> {
        let req = request.into_inner();
        let cancel_result = self.container.cancel_session.execute(&req.session_id).await;
        cancel_result.map_err(map_auth_err)?;

        Ok(Response::new(AuthResponse {
            success: true,
            session_id: req.session_id,
            message: "Session cancelled".to_string(),
            cookies: vec![],
        }))
    }
}

// ---- SessionService ----------------------------------------------------------

pub struct SessionGrpcService {
    container: Arc<AppContainer>,
}

impl SessionGrpcService {
    pub fn new(container: Arc<AppContainer>) -> Self {
        Self { container }
    }
}

#[tonic::async_trait]
impl SessionService for SessionGrpcService {
    async fn get_cookies(
        &self,
        request: Request<GetCookiesRequest>,
    ) -> Result<Response<CookieList>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;
        let domains = if req.domains.is_empty() {
            None
        } else {
            Some(req.domains)
        };

        let cookies = self.container.get_cookies.execute(platform, domains).await;
        let cookies = cookies.map_err(map_auth_err)?;

        Ok(Response::new(CookieList {
            cookies: cookies.iter().map(cookie_to_proto).collect(),
        }))
    }

    async fn update_cookies(
        &self,
        request: Request<UpdateCookiesRequest>,
    ) -> Result<Response<CookieList>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;
        let cookies: Vec<Cookie> = req.cookies.iter().map(proto_cookie_from).collect();

        self.container
            .update_cookies
            .execute(platform, cookies)
            .await
            .map_err(map_auth_err)?;
        let stored = self
            .container
            .get_cookies
            .execute(platform, None)
            .await
            .map_err(map_auth_err)?;

        Ok(Response::new(CookieList {
            cookies: stored.iter().map(cookie_to_proto).collect(),
        }))
    }

    async fn export_netscape(
        &self,
        request: Request<ExportRequest>,
    ) -> Result<Response<NetscapeExport>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let content = self.container.export_netscape.execute(platform).await;
        let content = content.map_err(map_auth_err)?;

        Ok(Response::new(NetscapeExport { content }))
    }

    async fn validate_session(
        &self,
        request: Request<ValidateRequest>,
    ) -> Result<Response<ValidationResult>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let outcome = self.container.validate_session.execute(platform).await;
        let outcome = outcome.map_err(map_auth_err)?;

        Ok(Response::new(ValidationResult {
            valid: outcome.valid,
            expires_at: outcome.expires_at,
            session_cookie_name: platform.session_cookie_name().to_string(),
        }))
    }

    async fn get_connection_status(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ConnectionStatusMap>, Status> {
        let platforms = self.container.get_connection_status.execute().await;
        let platforms = platforms.map_err(map_auth_err)?;

        Ok(Response::new(ConnectionStatusMap { platforms }))
    }
}

// ---- CredentialService -------------------------------------------------------

pub struct CredentialGrpcService {
    container: Arc<AppContainer>,
}

impl CredentialGrpcService {
    pub fn new(container: Arc<AppContainer>) -> Self {
        Self { container }
    }
}

#[tonic::async_trait]
impl CredentialService for CredentialGrpcService {
    async fn save(
        &self,
        request: Request<SaveCredentialRequest>,
    ) -> Result<Response<CredentialResponse>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let twofa = if req.twofa_method.is_empty() {
            None
        } else {
            Some(req.twofa_method.as_str())
        };

        let save_result = self
            .container
            .save_credential
            .execute(platform, &req.username, &req.password, twofa)
            .await;
        save_result.map_err(map_auth_err)?;

        Ok(Response::new(CredentialResponse {
            success: true,
            platform: req.platform,
            username: req.username,
        }))
    }

    async fn get(
        &self,
        request: Request<GetCredentialRequest>,
    ) -> Result<Response<CredentialInfo>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let cred = self.container.get_credential.execute(platform).await;
        let cred = cred.map_err(map_auth_err)?;

        match cred {
            Some(c) => Ok(Response::new(CredentialInfo {
                platform: req.platform,
                username: c.username,
                has_password: !c.password_encrypted.is_empty(),
                twofa_method: c.twofa_method.unwrap_or_default(),
            })),
            None => Err(Status::not_found("Credential not found")),
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteCredentialRequest>,
    ) -> Result<Response<CredentialResponse>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let delete_result = self.container.delete_credential.execute(platform).await;
        delete_result.map_err(map_auth_err)?;

        Ok(Response::new(CredentialResponse {
            success: true,
            platform: req.platform,
            username: String::new(),
        }))
    }
}

// ---- unit tests for converters ----------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use imauth_core::domain::session::Session as DomainSession;

    #[test]
    fn map_auth_err_translates_not_found_to_grpc_not_found() {
        let status = map_auth_err(imauth_core::ImauthError::NotFound("nope".into()));
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert_eq!(status.message(), "nope");
    }

    #[test]
    fn map_auth_err_translates_platform_to_invalid_argument() {
        let status = map_auth_err(imauth_core::ImauthError::Platform("bad".into()));
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert_eq!(status.message(), "bad");
    }

    #[test]
    fn map_auth_err_redacts_other_errors_to_internal() {
        let status = map_auth_err(imauth_core::ImauthError::Database(
            "select foo from bar: detail".into(),
        ));
        assert_eq!(status.code(), tonic::Code::Internal);
        assert_eq!(status.message(), "Internal server error");
    }

    #[test]
    fn map_auth_err_redacts_encryption_errors() {
        let status = map_auth_err(imauth_core::ImauthError::Encryption(
            "decrypt failed: aad".into(),
        ));
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn platform_from_proto_known_values() {
        assert!(matches!(platform_from_proto(1), Some(Platform::Instagram)));
        assert!(matches!(platform_from_proto(2), Some(Platform::Threads)));
        assert!(matches!(platform_from_proto(3), Some(Platform::Naver)));
        assert!(matches!(platform_from_proto(4), Some(Platform::Novelpia)));
    }

    #[test]
    fn platform_from_proto_unknown_returns_none() {
        assert!(platform_from_proto(0).is_none());
        assert!(platform_from_proto(99).is_none());
    }

    #[test]
    fn session_state_to_proto_covers_every_variant() {
        let pairs = [
            (SessionState::Idle, ProtoAuthStatus::Idle),
            (SessionState::Loading, ProtoAuthStatus::Loading),
            (
                SessionState::Authenticating,
                ProtoAuthStatus::Authenticating,
            ),
            (
                SessionState::WaitingForUser,
                ProtoAuthStatus::WaitingForUser,
            ),
            (SessionState::Connected, ProtoAuthStatus::Connected),
            (SessionState::Failed, ProtoAuthStatus::Failed),
        ];
        for (input, expected) in pairs {
            assert_eq!(session_state_to_proto(&input) as i32, expected as i32);
        }
    }

    fn sample_cookie() -> Cookie {
        Cookie {
            name: "sessionid".to_string(),
            value: "abc123".to_string(),
            domain: ".instagram.com".to_string(),
            path: "/".to_string(),
            expires: Some(chrono::Utc.timestamp_opt(1700000000, 0).unwrap()),
            http_only: true,
            secure: true,
        }
    }

    #[test]
    fn cookie_to_proto_preserves_all_fields() {
        let c = sample_cookie();
        let p = cookie_to_proto(&c);
        assert_eq!(p.name, "sessionid");
        assert_eq!(p.value, "abc123");
        assert_eq!(p.domain, ".instagram.com");
        assert_eq!(p.path, "/");
        assert_eq!(p.expires, 1700000000);
        assert!(p.http_only);
        assert!(p.secure);
    }

    #[test]
    fn cookie_to_proto_handles_missing_expires_as_zero() {
        let mut c = sample_cookie();
        c.expires = None;
        let p = cookie_to_proto(&c);
        assert_eq!(p.expires, 0);
    }

    #[test]
    fn proto_cookie_from_treats_zero_expires_as_session_cookie() {
        let mut proto = cookie_to_proto(&sample_cookie());
        proto.expires = 0;

        let cookie = proto_cookie_from(&proto);

        assert!(cookie.expires.is_none());
    }

    #[test]
    fn proto_cookie_from_round_trips() {
        let original = sample_cookie();
        let p = cookie_to_proto(&original);
        let back = proto_cookie_from(&p);
        assert_eq!(back.name, original.name);
        assert_eq!(back.value, original.value);
        assert_eq!(back.domain, original.domain);
        assert_eq!(back.path, original.path);
        assert_eq!(back.http_only, original.http_only);
        assert_eq!(back.secure, original.secure);
        assert_eq!(
            back.expires.map(|t| t.timestamp()),
            original.expires.map(|t| t.timestamp())
        );
    }

    #[test]
    fn auth_event_from_includes_session_metadata() {
        let mut sess = DomainSession::new("sess-1".into(), "instagram".into());
        sess.state = SessionState::WaitingForUser;
        sess.message = Some("Log in via browser".into());
        sess.requires_input = true;
        sess.input_type = Some("viewer_url".into());

        let evt = auth_event_from(&sess);
        assert_eq!(evt.session_id, "sess-1");
        assert_eq!(evt.status, ProtoAuthStatus::WaitingForUser as i32);
        assert_eq!(evt.message, "Log in via browser");
        assert!(evt.requires_input);
        assert_eq!(evt.input_type, "viewer_url");
        assert!(evt.cookies.is_empty());
        assert!(evt.viewer_url.is_empty());
    }

    #[test]
    fn auth_event_from_defaults_missing_optional_fields() {
        let sess = DomainSession::new("sess-2".into(), "naver".into());
        let evt = auth_event_from(&sess);
        assert_eq!(evt.session_id, "sess-2");
        assert_eq!(evt.message, "");
        assert_eq!(evt.input_type, "");
        assert!(!evt.requires_input);
    }
}
