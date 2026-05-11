use crate::generated::v1::{
    auth_service_server::AuthService, credential_service_server::CredentialService,
    session_service_server::SessionService, AuthEvent, AuthResponse, AuthStatus as ProtoAuthStatus,
    AuthStatusResponse, CancelRequest, ConnectionStatusMap, Cookie as ProtoCookie, CookieList,
    CredentialInfo, CredentialResponse, DeleteCredentialRequest, Empty, ExportRequest,
    GetCookiesRequest, GetCredentialRequest, LoginRequest, NetscapeExport, SaveCredentialRequest,
    StatusRequest, Submit2FaRequest, SubmitCaptchaRequest, UpdateCookiesRequest, ValidateRequest,
    ValidationResult,
};
use futures::Stream;
use imauth_core::{
    platform::{instagram, Platform},
    session::state::{Cookie, Session, SessionState},
    ImauthCore,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

pub struct AuthGrpcService {
    core: Arc<ImauthCore>,
}

impl AuthGrpcService {
    pub fn new(core: Arc<ImauthCore>) -> Self {
        Self { core }
    }
}

fn platform_from_proto(p: i32) -> Option<Platform> {
    match p {
        1 => Some(Platform::Instagram),
        2 => Some(Platform::Threads),
        _ => None,
    }
}

fn auth_event_from(session: &Session, cookies: Vec<ProtoCookie>) -> AuthEvent {
    AuthEvent {
        session_id: session.id.clone(),
        status: session_state_to_proto(&session.state) as i32,
        message: session.message.clone().unwrap_or_default(),
        requires_input: session.requires_input,
        input_type: session.input_type.clone().unwrap_or_default(),
        cookies,
        screenshot: vec![],
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
        let platform_str = platform.as_str().to_string();

        let core = self.core.clone();
        let (tx, rx) = mpsc::channel(10);
        let snapshot_dir = core.config.snapshot_dir();

        tokio::spawn(async move {
            let mut session = match core.create_session(platform_str.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(Err(Status::internal(format!(
                            "Failed to create session: {e}"
                        ))))
                        .await;
                    return;
                }
            };

            let _ = tx.send(Ok(auth_event_from(&session, vec![]))).await;

            let handle = match core.browser_manager.acquire().await {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx
                        .send(Err(Status::internal(format!("Browser error: {e}"))))
                        .await;
                    return;
                }
            };

            let page = match handle.browser().new_page().await {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx
                        .send(Err(Status::internal(format!("Failed to create page: {e}"))))
                        .await;
                    return;
                }
            };

            if let Err(e) = instagram::login(
                &page,
                platform,
                &req.username,
                &req.password,
                &mut session,
                Some(snapshot_dir.as_path()),
            )
            .await
            {
                session.transition(SessionState::Failed, Some(e.to_string()));
            }

            let _ = core.update_session(&session).await;

            if session.state == SessionState::Connected {
                let _ = core.cookie_jar.save(&platform_str, &session.cookies).await;
            }

            let cookies = session.cookies.iter().map(cookie_to_proto).collect();
            let _ = tx.send(Ok(auth_event_from(&session, cookies))).await;
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::LoginStream))
    }

    async fn submit2_fa(
        &self,
        request: Request<Submit2FaRequest>,
    ) -> Result<Response<AuthResponse>, Status> {
        let req = request.into_inner();
        let core = self.core.clone();

        let mut session = core
            .get_session(&req.session_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Session not found"))?;

        let platform = Platform::from_str(&session.platform)
            .ok_or_else(|| Status::internal("Unknown platform"))?;

        let handle = core
            .browser_manager
            .acquire()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let pages = handle
            .pages()
            .await
            .map_err(|e| Status::internal(format!("Failed to list pages: {e}")))?;
        let page = pages.into_iter().next().ok_or_else(|| {
            Status::internal("No browser page found — the 2FA page may have been closed")
        })?;

        let snapshot_dir = core.config.snapshot_dir();

        instagram::submit_2fa(
            &page,
            platform,
            &req.code,
            &mut session,
            Some(snapshot_dir.as_path()),
        )
        .await
        .map_err(|e| Status::internal(e.to_string()))?;

        core.update_session(&session)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        if session.state == SessionState::Connected {
            core.cookie_jar
                .save(&session.platform, &session.cookies)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        Ok(Response::new(AuthResponse {
            success: session.state == SessionState::Connected,
            session_id: session.id,
            message: session.message.clone().unwrap_or_default(),
            cookies: session.cookies.iter().map(cookie_to_proto).collect(),
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
            .core
            .get_session(&req.session_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Session not found"))?;

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
        self.core
            .delete_session(&req.session_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(AuthResponse {
            success: true,
            session_id: req.session_id,
            message: "Session cancelled".to_string(),
            cookies: vec![],
        }))
    }
}

pub struct SessionGrpcService {
    core: Arc<ImauthCore>,
}

impl SessionGrpcService {
    pub fn new(core: Arc<ImauthCore>) -> Self {
        Self { core }
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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();
        let domains: Option<Vec<&str>> = if req.domains.is_empty() {
            None
        } else {
            Some(req.domains.iter().map(|s| s.as_str()).collect())
        };

        let cookies = self
            .core
            .cookie_jar
            .get(platform, domains.as_deref())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();
        let cookies: Vec<Cookie> = req.cookies.iter().map(proto_cookie_from).collect();

        self.core
            .cookie_jar
            .save(platform, &cookies)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();

        let content = self
            .core
            .cookie_jar
            .export_netscape(platform)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(NetscapeExport { content }))
    }

    async fn validate_session(
        &self,
        request: Request<ValidateRequest>,
    ) -> Result<Response<ValidationResult>, Status> {
        let req = request.into_inner();
        let platform = platform_from_proto(req.platform)
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?;

        let cookies = self
            .core
            .cookie_jar
            .get(platform.as_str(), None)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let session_cookie_name = platform.session_cookie_name();
        let session_cookie = cookies.iter().find(|c| c.name == session_cookie_name);

        Ok(Response::new(ValidationResult {
            valid: session_cookie.is_some(),
            expires_at: session_cookie
                .and_then(|c| c.expires)
                .map(|dt| dt.timestamp())
                .unwrap_or(0),
            session_cookie_name: session_cookie_name.to_string(),
        }))
    }

    async fn get_connection_status(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ConnectionStatusMap>, Status> {
        let jar = &self.core.cookie_jar;
        let fetches = Platform::ALL
            .iter()
            .map(|p| async move { (*p, jar.get(p.as_str(), None).await.unwrap_or_default()) });
        let results = futures::future::join_all(fetches).await;

        let platforms = results
            .into_iter()
            .map(|(p, cookies)| (p.as_str().to_string(), p.has_session_cookie(&cookies)))
            .collect();

        Ok(Response::new(ConnectionStatusMap { platforms }))
    }
}

pub struct CredentialGrpcService {
    core: Arc<ImauthCore>,
}

impl CredentialGrpcService {
    pub fn new(core: Arc<ImauthCore>) -> Self {
        Self { core }
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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();

        self.core
            .credential_store
            .save(
                platform,
                &req.username,
                &req.password,
                if req.twofa_method.is_empty() {
                    None
                } else {
                    Some(&req.twofa_method)
                },
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();

        let cred = self
            .core
            .credential_store
            .get(platform)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

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
            .ok_or_else(|| Status::invalid_argument("Unknown platform"))?
            .as_str();

        self.core
            .credential_store
            .delete(platform)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CredentialResponse {
            success: true,
            platform: req.platform,
            username: String::new(),
        }))
    }
}
