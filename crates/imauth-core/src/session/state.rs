use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Loading,
    Authenticating,
    NeedsCreds,
    Needs2Fa,
    NeedsCaptcha,
    Connected,
    Failed,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Idle => write!(f, "idle"),
            SessionState::Loading => write!(f, "loading"),
            SessionState::Authenticating => write!(f, "authenticating"),
            SessionState::NeedsCreds => write!(f, "needs_creds"),
            SessionState::Needs2Fa => write!(f, "needs_2fa"),
            SessionState::NeedsCaptcha => write!(f, "needs_captcha"),
            SessionState::Connected => write!(f, "connected"),
            SessionState::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub platform: String,
    pub state: SessionState,
    pub message: Option<String>,
    pub requires_input: bool,
    pub input_type: Option<String>,
    pub cookies: Vec<Cookie>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<DateTime<Utc>>,
    pub http_only: bool,
    pub secure: bool,
}

impl Session {
    pub fn new(id: String, platform: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            platform,
            state: SessionState::Idle,
            message: None,
            requires_input: false,
            input_type: None,
            cookies: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition(&mut self, state: SessionState, message: Option<String>) {
        self.state = state;
        self.message = message;
        self.updated_at = Utc::now();
        self.requires_input = matches!(
            state,
            SessionState::NeedsCreds | SessionState::Needs2Fa | SessionState::NeedsCaptcha
        );
        self.input_type = match state {
            SessionState::Needs2Fa => Some("2fa_code".to_string()),
            SessionState::NeedsCaptcha => Some("captcha".to_string()),
            SessionState::NeedsCreds => Some("creds".to_string()),
            _ => None,
        };
    }
}
