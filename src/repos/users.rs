use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use getrandom::fill;
use rand::rngs::OsRng;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::user::{User, UserInput, PROTECTED_LOGIN, ROLE_ADMINISTRATOR};

/// 全ユーザーをログイン名順で取得する。
pub async fn list_all(pool: &SqlitePool) -> AppResult<Vec<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, login, display_name, password_hash, role, created_at, updated_at \
         FROM users ORDER BY login COLLATE NOCASE ASC, id ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// ID でユーザーを取得する。存在しなければ `NotFound`。
pub async fn find(pool: &SqlitePool, id: i64) -> AppResult<User> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, login, display_name, password_hash, role, created_at, updated_at \
         FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?)
}

/// ログイン名でユーザーを取得する。存在しなければ `None`。
pub async fn find_by_login(pool: &SqlitePool, login: &str) -> AppResult<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, login, display_name, password_hash, role, created_at, updated_at \
         FROM users WHERE login = ? COLLATE NOCASE",
    )
    .bind(login)
    .fetch_optional(pool)
    .await?)
}

/// ログイン名が既に使われているか（大文字小文字を区別しない）。
pub async fn exists_by_login(pool: &SqlitePool, login: &str) -> AppResult<bool> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE login = ? COLLATE NOCASE LIMIT 1")
            .bind(login)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

/// ユーザーを作成する。
pub async fn insert(pool: &SqlitePool, input: &UserInput) -> AppResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO users (login, display_name, password_hash, role)
         VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.login)
    .bind(&input.display_name)
    .bind(&input.password_hash)
    .bind(&input.role)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// 表示名・パスワードハッシュを更新する。
pub async fn update(
    pool: &SqlitePool,
    id: i64,
    display_name: &str,
    password_hash: Option<&str>,
) -> AppResult<()> {
    match password_hash {
        Some(hash) => {
            sqlx::query(
                "UPDATE users SET display_name = ?, password_hash = ?,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
            )
            .bind(display_name)
            .bind(hash)
            .bind(id)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "UPDATE users SET display_name = ?,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
            )
            .bind(display_name)
            .bind(id)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// ユーザーを削除する。
pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

fn hash_password(plain: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| AppError::Other(anyhow::anyhow!("password hash failed: {e}")))?;
    Ok(hash.to_string())
}

fn generate_random_password() -> AppResult<String> {
    let mut bytes = [0u8; 24];
    fill(&mut bytes).map_err(|e| AppError::Other(anyhow::anyhow!("random generation failed: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// 既定の `admin` ユーザーが無ければ作成し、初回パスワードをログに出力する。
pub async fn ensure_default_admin(pool: &SqlitePool) -> AppResult<()> {
    if find_by_login(pool, PROTECTED_LOGIN).await?.is_some() {
        return Ok(());
    }

    let plain_password = generate_random_password()?;
    let password_hash = hash_password(&plain_password)?;

    let input = UserInput {
        login: PROTECTED_LOGIN.to_string(),
        display_name: "管理者".to_string(),
        password_hash,
        role: ROLE_ADMINISTRATOR.to_string(),
    };
    insert(pool, &input).await?;

    tracing::warn!(
        login = PROTECTED_LOGIN,
        password = %plain_password,
        "既定の管理ユーザー admin を作成しました。初回パスワードはこのログに一度だけ記録されます。\
         /admin/login からログインしてください。本番環境ではログの取り扱いに注意してください。"
    );

    Ok(())
}
