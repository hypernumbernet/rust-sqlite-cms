//! お問い合わせフォームウィジェットの送信処理。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult, DomainError, DomainResult};
use crate::models::post::PostInput;
use crate::models::widget::ContactFormWidgetConfig;
use crate::repos::{placeholders, postmeta, posts, widget_types};

type HmacSha256 = Hmac<Sha256>;

const TOKEN_TTL_SECS: u64 = 3600;

/// フォーム POST で受け取る入力。
#[derive(Debug, Clone)]
pub struct ContactFormSubmission {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub message: String,
    pub token: String,
}

/// 署名付きフォームトークンを発行する。
pub fn issue_token(placeholder_id: i64, secret: &str) -> AppResult<String> {
    let secret = resolve_secret(secret)?;
    let expiry = current_unix_secs().saturating_add(TOKEN_TTL_SECS);
    let payload = format!("{placeholder_id}:{expiry}");
    let sig = sign_payload(&payload, &secret)?;
    Ok(format!("{payload}:{sig}"))
}

/// トークンを検証する（placeholder_id が一致し、有効期限内であること）。
pub fn verify_token(token: &str, placeholder_id: i64, secret: &str) -> DomainResult<()> {
    let secret = resolve_secret(secret).map_err(|e| DomainError::Internal(e.into()))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(DomainError::Validation("フォームの有効期限が切れています。ページを再読み込みしてください。".to_string()));
    }

    let parts: Vec<&str> = token.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(DomainError::Validation("無効なフォームトークンです。".to_string()));
    }
    let sig = parts[0];
    let payload = parts[1];

    let payload_parts: Vec<&str> = payload.splitn(2, ':').collect();
    if payload_parts.len() != 2 {
        return Err(DomainError::Validation("無効なフォームトークンです。".to_string()));
    }

    let token_placeholder_id: i64 = payload_parts[0]
        .parse()
        .map_err(|_| DomainError::Validation("無効なフォームトークンです。".to_string()))?;
    if token_placeholder_id != placeholder_id {
        return Err(DomainError::Validation("無効なフォームトークンです。".to_string()));
    }

    let expiry: u64 = payload_parts[1]
        .parse()
        .map_err(|_| DomainError::Validation("無効なフォームトークンです。".to_string()))?;
    if current_unix_secs() > expiry {
        return Err(DomainError::Validation(
            "フォームの有効期限が切れています。ページを再読み込みしてください。".to_string(),
        ));
    }

    let expected = sign_payload(payload, &secret)?;
    if !constant_time_eq(sig.as_bytes(), expected.as_bytes()) {
        return Err(DomainError::Validation("無効なフォームトークンです。".to_string()));
    }

    Ok(())
}

/// お問い合わせを検証して posts + postmeta に保存する。
pub async fn submit(
    pool: &SqlitePool,
    placeholder_id: i64,
    secret: &str,
    input: &ContactFormSubmission,
) -> DomainResult<()> {
    verify_token(&input.token, placeholder_id, secret)?;

    let placeholder = placeholders::find(pool, placeholder_id)
        .await
        .map_err(|e| DomainError::Internal(e.into()))?;
    let widget_type = widget_types::find(pool, placeholder.widget_type_id)
        .await
        .map_err(|e| DomainError::Internal(e.into()))?;
    if widget_type.type_key != "contact_form" {
        return Err(DomainError::BadRequest(
            "このプレースホルダーはお問い合わせフォームではありません。".to_string(),
        ));
    }

    let mut cfg: ContactFormWidgetConfig =
        serde_json::from_str(&widget_type.config).unwrap_or_default();
    if let Ok(instance) = serde_json::from_str::<serde_json::Value>(&placeholder.config) {
        merge_contact_config(&mut cfg, &instance);
    }

    validate_submission(input, &cfg)?;

    let slug = format!(
        "contact-{}-{}",
        current_unix_secs(),
        rand::random::<u32>()
    );
    let excerpt: String = input.message.chars().take(120).collect();
    let title = format!("お問い合わせ: {}", input.name.trim());

    let post = PostInput {
        placeholder_id,
        title,
        content: input.message.trim().to_string(),
        excerpt,
        post_status: "publish".to_string(),
        post_name: slug,
    };

    let post_id = posts::insert(pool, &post)
        .await
        .map_err(|e| DomainError::Internal(e.into()))?;

    let mut meta = HashMap::new();
    meta.insert("contact_name".to_string(), input.name.trim().to_string());
    meta.insert("contact_email".to_string(), input.email.trim().to_string());
    if let Some(phone) = input.phone.as_ref().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        meta.insert("contact_phone".to_string(), phone.to_string());
    }
    postmeta::set_many(pool, post_id, &meta)
        .await
        .map_err(|e| DomainError::Internal(e.into()))?;

    Ok(())
}

fn merge_contact_config(cfg: &mut ContactFormWidgetConfig, instance: &serde_json::Value) {
    if let Some(heading) = instance.get("heading").and_then(|v| v.as_str()) {
        if !heading.trim().is_empty() {
            cfg.heading = heading.trim().to_string();
        }
    }
    if let Some(submit_label) = instance.get("submit_label").and_then(|v| v.as_str()) {
        if !submit_label.trim().is_empty() {
            cfg.submit_label = submit_label.trim().to_string();
        }
    }
    if let Some(success_message) = instance.get("success_message").and_then(|v| v.as_str()) {
        if !success_message.trim().is_empty() {
            cfg.success_message = success_message.trim().to_string();
        }
    }
    if let Some(show_phone) = instance.get("show_phone").and_then(|v| v.as_bool()) {
        cfg.show_phone = show_phone;
    }
}

fn validate_submission(input: &ContactFormSubmission, cfg: &ContactFormWidgetConfig) -> DomainResult<()> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(DomainError::Validation("お名前を正しく入力してください。".to_string()));
    }

    let email = input.email.trim();
    if email.is_empty() || email.len() > 254 || !email.contains('@') {
        return Err(DomainError::Validation(
            "メールアドレスを正しく入力してください。".to_string(),
        ));
    }

    let message = input.message.trim();
    if message.is_empty() || message.len() > 5000 {
        return Err(DomainError::Validation(
            "お問い合わせ内容を正しく入力してください。".to_string(),
        ));
    }

    if let Some(phone) = input.phone.as_ref().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if phone.len() > 30 {
            return Err(DomainError::Validation(
                "電話番号は 30 文字以内で入力してください。".to_string(),
            ));
        }
    } else if cfg.show_phone {
        // 電話番号は任意（show_phone 時も必須ではない）
    }

    Ok(())
}

fn resolve_secret(secret: &str) -> AppResult<String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(AppError::Other(anyhow::anyhow!(
            "session secret is required for contact form tokens"
        )));
    }
    Ok(secret.to_string())
}

fn sign_payload(payload: &str, secret: &str) -> AppResult<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Other(e.into()))?;
    mac.update(payload.as_bytes());
    let result = mac.finalize().into_bytes();
    Ok(URL_SAFE_NO_PAD.encode(result))
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify_token_roundtrip() {
        let token = issue_token(42, "test-secret").expect("issue token");
        verify_token(&token, 42, "test-secret").expect("verify token");
    }

    #[test]
    fn verify_token_rejects_wrong_placeholder() {
        let token = issue_token(42, "test-secret").expect("issue token");
        assert!(verify_token(&token, 99, "test-secret").is_err());
    }

    #[test]
    fn verify_token_rejects_tampered_token() {
        let token = issue_token(42, "test-secret").expect("issue token");
        let tampered = format!("{token}x");
        assert!(verify_token(&tampered, 42, "test-secret").is_err());
    }
}
