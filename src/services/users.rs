//! ユーザー管理サービス（CRUD・パスワードハッシュ・admin 保護）。

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use sqlx::SqlitePool;

use crate::error::{DomainError, DomainResult};
use crate::models::user::{
    validate_display_name, validate_login, validate_password, User, PROTECTED_LOGIN,
    ROLE_ADMINISTRATOR,
};
use crate::repos::users as users_repo;

pub fn hash_password(plain: &str) -> DomainResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;
    Ok(hash.to_string())
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

const AUTH_FAILED_MSG: &str = "ログイン名またはパスワードが正しくありません";

/// ログイン名とパスワードでユーザーを認証する。
pub async fn authenticate(
    pool: &SqlitePool,
    login: &str,
    password: &str,
) -> DomainResult<User> {
    let login = login.trim();
    if login.is_empty() || password.is_empty() {
        return Err(DomainError::Validation(AUTH_FAILED_MSG.to_string()));
    }

    let Some(user) = users_repo::find_by_login(pool, login).await? else {
        return Err(DomainError::Validation(AUTH_FAILED_MSG.to_string()));
    };

    if !verify_password(password, &user.password_hash) {
        return Err(DomainError::Validation(AUTH_FAILED_MSG.to_string()));
    }

    Ok(user)
}

pub struct CreateUserParams<'a> {
    pub login: &'a str,
    pub display_name: &'a str,
    pub password: &'a str,
}

pub struct UpdateUserParams<'a> {
    pub login: &'a str,
    pub display_name: &'a str,
    pub password: &'a str,
}

/// 新規ユーザーを作成する。
pub async fn create(pool: &SqlitePool, params: CreateUserParams<'_>) -> DomainResult<i64> {
    validate_login(params.login).map_err(DomainError::Validation)?;
    validate_display_name(params.display_name).map_err(DomainError::Validation)?;
    validate_password(params.password, true).map_err(DomainError::Validation)?;

    if users_repo::exists_by_login(pool, params.login.trim()).await? {
        return Err(DomainError::Conflict(
            "このログイン名は既に使用されています".to_string(),
        ));
    }

    let password_hash = hash_password(params.password)?;
    let input = crate::models::user::UserInput {
        login: params.login.trim().to_string(),
        display_name: params.display_name.trim().to_string(),
        password_hash,
        role: ROLE_ADMINISTRATOR.to_string(),
    };

    users_repo::insert(pool, &input).await.map_err(Into::into)
}

/// ユーザーを更新する（login 変更は admin 以外も POST 値が異なれば拒否）。
pub async fn update(
    pool: &SqlitePool,
    user: &User,
    params: UpdateUserParams<'_>,
) -> DomainResult<()> {
    validate_display_name(params.display_name).map_err(DomainError::Validation)?;

    let submitted_login = params.login.trim();
    if !submitted_login.eq_ignore_ascii_case(&user.login) {
        return Err(DomainError::Conflict(
            "ログイン名は変更できません".to_string(),
        ));
    }

    validate_password(params.password, false).map_err(DomainError::Validation)?;

    let password_hash = if params.password.is_empty() {
        None
    } else {
        Some(hash_password(params.password)?)
    };

    users_repo::update(
        pool,
        user.id,
        params.display_name.trim(),
        password_hash.as_deref(),
    )
    .await
    .map_err(Into::into)
}

/// ユーザーを削除する（`admin` は不可）。
pub async fn delete(pool: &SqlitePool, user: &User) -> DomainResult<()> {
    if user.login.eq_ignore_ascii_case(PROTECTED_LOGIN) {
        return Err(DomainError::Conflict(
            "既定の admin ユーザーは削除できません".to_string(),
        ));
    }

    users_repo::delete(pool, user.id).await.map_err(Into::into)
}
