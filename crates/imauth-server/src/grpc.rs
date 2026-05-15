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
    GetCookiesRequest, GetCredentialRequest, LoginRequest, NetscapeExport, SaveCredentialRequest,
    StatusRequest, Submit2FaRequest, SubmitCaptchaRequest, UpdateCookiesRequest, ValidateRequest,
    ValidationResult,
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
    match p {
        1 => Some(Platform::Instagram),
        2 => Some(Platform::Threads),
        _ => None,
    }
}

fn session_state_to_proto(state: &SessionState) -> ProtoAuthStatus {
    match state {
        SessionState::Idle => ProtoAuthStatus::Idle,
        SessionState::Loading => ProtoAuthStatus::Loading,
        SessionState::Authenticating => ProtoAuthStatus::Authenticating,
        SessionState::NeedsCreds => ProtoAuthStatus::NeedsCreds,
        SessionState::Needs2Fa => ProtoAuthStatus::Needs2fa,
        SessionState::NeedsCaptcha => ProtoAuthStatus::NeedsCaptcha,
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
        expires: chrono::DateTime::from_timestamp(c.expires, 0),
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
        screenshot: vec![],
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
            container
                .login
                .execute(platform, req.username, req.password, tx)
                .await;
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|event| {
            let (session, cookies) = match event {
                LoginEvent::Started(s) => (s, vec![]),
                LoginEvent::Final(s, c) => (s, c),
            };
            let mut evt = auth_event_from(&session);
            evt.cookies = cookies.iter().map(cookie_to_proto).collect();
            Ok::<AuthEvent, Status>(evt)
        });

        Ok(Response::new(Box::pin(stream) as Self::LoginStream))
    }

    async fn submit2_fa(
        &self,
        request: Request<Submit2FaRequest>,
    ) -> Result<Response<AuthResponse>, Status> {
        let req = request.into_inner();
        let (session, cookies) = self
            .container
            .submit_2fa
            .execute(&req.session_id, &req.code)
            .await
            .map_err(map_auth_err)?;

        Ok(Response::new(AuthResponse {
            success: session.state == SessionState::Connected,
            session_id: session.id,
            message: session.message.clone().unwrap_or_default(),
            cookies: cookies.iter().map(cookie_to_proto).collect(),
        }))
    }

    async fn submit_captcha(
        &self,
        _request: Request<SubmitCaptchaRequest>,
    ) -> Result<Response<AuthResponse>, Status> {
        Ok(Response::new(AuthResponse {
            success: false,
            session_id: String::new(),
            message: "Captcha solving not implemented".to_string(),
            cookies: vec![],
        }))
    }

    async fn get_status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<AuthStatusResponse>, Status> {
        let req = request.into_inner();
        let session = self
            .container
            .get_status
            .execute(&req.session_id)
            .await
            .map_err(map_auth_err)?;

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
        self.container
            .cancel_session
            .execute(&req.session_id)
            .await
            .map_err(map_auth_err)?;

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

        let cookies = self
            .container
            .get_cookies
            .execute(platform, domains)
            .await
            .map_err(map_auth_err)?;

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

        Ok(Response::new(CookieList {
            cookies: req.cookies,
        }))
    }

    async fn export_netscape(
        &self,
        request: Request<ExportRequest>,
    ) -> Result<Response<NetscapeExport>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let content = self
            .container
            .export_netscape
            .execute(platform)
            .await
            .map_err(map_auth_err)?;

        Ok(Response::new(NetscapeExport { content }))
    }

    async fn validate_session(
        &self,
        request: Request<ValidateRequest>,
    ) -> Result<Response<ValidationResult>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let outcome = self
            .container
            .validate_session
            .execute(platform)
            .await
            .map_err(map_auth_err)?;

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
        let platforms = self
            .container
            .get_connection_status
            .execute()
            .await
            .map_err(map_auth_err)?;

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

        self.container
            .save_credential
            .execute(platform, &req.username, &req.password, twofa)
            .await
            .map_err(map_auth_err)?;

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

        let cred = self
            .container
            .get_credential
            .execute(platform)
            .await
            .map_err(map_auth_err)?;

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

        self.container
            .delete_credential
            .execute(platform)
            .await
            .map_err(map_auth_err)?;

        Ok(Response::new(CredentialResponse {
            success: true,
            platform: req.platform,
            username: String::new(),
        }))
    }
}
