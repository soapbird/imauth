pub struct Credential {
    pub platform: String,
    pub username: String,
    pub password_encrypted: String,
    pub twofa_method: Option<String>,
}
