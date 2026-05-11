pub struct Selectors {
    pub username_input: &'static str,
    pub password_input: &'static str,
    pub submit_button: &'static str,
    pub twofa_input: &'static [&'static str],
    pub twofa_submit: &'static [&'static str],
}

pub const INSTAGRAM_SELECTORS: Selectors = Selectors {
    // Instagram login page uses name="email" and name="pass" (not username/password)
    username_input: "input[name='email']",
    password_input: "input[name='pass']",
    submit_button: "div[role='button'][aria-label='Log In']",
    twofa_input: &[
        "input[name=\"verificationCode\"]",
        "input[aria-label*=\"code\" i]",
        "input[placeholder*=\"code\" i]",
        "input[type=\"tel\"]",
        "input[type=\"text\"][autocomplete=\"off\"]",
    ],
    twofa_submit: &["button[type='submit']", "input[type='submit']"],
};
