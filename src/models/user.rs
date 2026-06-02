use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub const PROTECTED_LOGIN: &str = "admin";
pub const ROLE_ADMINISTRATOR: &str = "administrator";
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_LOGIN_LEN: usize = 60;
pub const MAX_DISPLAY_NAME_LEN: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i64,
    pub login: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    pub fn is_protected(&self) -> bool {
        self.login.eq_ignore_ascii_case(PROTECTED_LOGIN)
    }

    pub fn role_label(&self) -> &'static str {
        match self.role.as_str() {
            ROLE_ADMINISTRATOR => "管理者",
            _ => "不明",
        }
    }
}

#[derive(Debug, Clone)]
pub struct UserInput {
    pub login: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: String,
}

pub fn validate_login(login: &str) -> Result<(), String> {
    let login = login.trim();
    if login.is_empty() {
        return Err("ログイン名を入力してください".to_string());
    }
    if login.len() > MAX_LOGIN_LEN {
        return Err(format!("ログイン名は {MAX_LOGIN_LEN} 文字以内にしてください"));
    }
    if login.eq_ignore_ascii_case(PROTECTED_LOGIN) {
        return Err("ログイン名 admin は既定ユーザー用のため使用できません".to_string());
    }
    if !login
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("ログイン名は英数字・ハイフン・アンダースコアのみ使用できます".to_string());
    }
    Ok(())
}

pub fn validate_display_name(display_name: &str) -> Result<(), String> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err("表示名を入力してください".to_string());
    }
    if display_name.len() > MAX_DISPLAY_NAME_LEN {
        return Err(format!(
            "表示名は {MAX_DISPLAY_NAME_LEN} 文字以内にしてください"
        ));
    }
    Ok(())
}

pub fn validate_password(password: &str, required: bool) -> Result<(), String> {
    if password.is_empty() {
        if required {
            return Err("パスワードを入力してください".to_string());
        }
        return Ok(());
    }
    if password.len() < MIN_PASSWORD_LEN {
        return Err(format!(
            "パスワードは {MIN_PASSWORD_LEN} 文字以上にしてください"
        ));
    }
    Ok(())
}
