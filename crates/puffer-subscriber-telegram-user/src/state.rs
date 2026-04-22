//! Process-wide mutable state for the Telegram subscriber.
//!
//! The subscriber holds at most one active login attempt at a time. While
//! waiting for a code or 2FA password, the in-flight [`LoginToken`] or
//! [`PasswordToken`] must be retained in memory so the corresponding
//! "submit" command can complete the flow.

use std::path::PathBuf;

use grammers_client::types::{LoginToken, PasswordToken};

/// Ambient configuration resolved once at startup from environment variables.
#[derive(Debug, Clone)]
pub struct SkillEnv {
    /// Absolute path to the session file that persists MTProto auth keys.
    pub session_path: PathBuf,
    /// Event topic to stamp on outbound events. Defaults to `"telegram-user"`.
    pub topic: String,
}

impl SkillEnv {
    /// Resolves ambient configuration from `PUFFER_SKILL_STATE_DIR` and
    /// `PUFFER_SKILL_TOPIC`, falling back to sensible defaults when unset.
    pub fn from_env() -> Self {
        let session_path = match std::env::var("PUFFER_SKILL_STATE_DIR") {
            Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("telegram.session"),
            _ => PathBuf::from("./telegram.session"),
        };
        let topic = std::env::var("PUFFER_SKILL_TOPIC")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "telegram-user".to_string());
        Self { session_path, topic }
    }
}

/// Transient state carried between login-flow commands.
///
/// Once a login has completed successfully both fields are cleared. While a
/// code request is pending, [`Self::login_token`] is populated. While a 2FA
/// password is pending, [`Self::password_token`] is populated.
#[derive(Default)]
pub struct LoginState {
    /// Token returned by `request_login_code`, consumed by `sign_in`.
    pub login_token: Option<LoginToken>,
    /// Token returned by `sign_in` when 2FA is required, consumed by
    /// `check_password`.
    pub password_token: Option<PasswordToken>,
    /// Phone number currently being signed in with, retained so outbound
    /// events can echo it back to the operator.
    pub phone: Option<String>,
    /// Telegram API id used for the current attempt. Needed because sign-in
    /// happens after `request_login_code` on a previously-connected client.
    pub api_id: Option<i32>,
    /// Telegram API hash used for the current attempt.
    pub api_hash: Option<String>,
}

impl LoginState {
    /// Constructs an empty [`LoginState`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the login-token / password-token fields after a successful or
    /// terminally-failed login attempt. Credentials (api id/hash/phone) are
    /// preserved so a subsequent retry can reuse them without re-sending
    /// `TelegramLoginStart`.
    pub fn clear_tokens(&mut self) {
        self.login_token = None;
        self.password_token = None;
    }
}
