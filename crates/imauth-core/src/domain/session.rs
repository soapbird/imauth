use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Idle,
    Loading,
    Authenticating,
    WaitingForUser,
    Connected,
    Failed,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Loading => "loading",
            SessionState::Authenticating => "authenticating",
            SessionState::WaitingForUser => "waiting_for_user",
            SessionState::Connected => "connected",
            SessionState::Failed => "failed",
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SessionState {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "loading" => SessionState::Loading,
            "authenticating" => SessionState::Authenticating,
            "waiting_for_user" => SessionState::WaitingForUser,
            "connected" => SessionState::Connected,
            "failed" => SessionState::Failed,
            _ => SessionState::Idle,
        })
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
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition(&mut self, state: SessionState, message: Option<String>) {
        self.state = state;
        self.message = message;
        self.updated_at = Utc::now();
        self.requires_input = matches!(state, SessionState::WaitingForUser);
        self.input_type = match state {
            SessionState::WaitingForUser => Some("viewer_url".to_string()),
            _ => None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new("s1".into(), "instagram".into())
    }

    #[test]
    fn new_starts_idle_with_no_input_required() {
        let s = session();
        assert_eq!(s.state, SessionState::Idle);
        assert!(!s.requires_input);
        assert!(s.input_type.is_none());
        assert_eq!(s.created_at, s.updated_at);
    }

    #[test]
    fn transition_to_waiting_for_user_sets_viewer_url_input_type() {
        let mut s = session();
        s.transition(SessionState::WaitingForUser, Some("Log in via browser".into()));
        assert_eq!(s.state, SessionState::WaitingForUser);
        assert!(s.requires_input);
        assert_eq!(s.input_type.as_deref(), Some("viewer_url"));
        assert_eq!(s.message.as_deref(), Some("Log in via browser"));
    }

    #[test]
    fn transition_to_connected_clears_requires_input_and_input_type() {
        let mut s = session();
        s.transition(SessionState::WaitingForUser, None);
        s.transition(SessionState::Connected, Some("ok".into()));
        assert_eq!(s.state, SessionState::Connected);
        assert!(!s.requires_input);
        assert!(s.input_type.is_none());
    }

    #[test]
    fn transition_to_failed_clears_input_state() {
        let mut s = session();
        s.transition(SessionState::Failed, Some("nope".into()));
        assert!(!s.requires_input);
        assert!(s.input_type.is_none());
    }

    #[test]
    fn display_renders_snake_case() {
        assert_eq!(SessionState::WaitingForUser.to_string(), "waiting_for_user");
    }
}
