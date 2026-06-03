use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// ドメイン／サービス層で使う中立的なエラー型。
/// HTTP 表現（AppError / ApiError）とは分離して定義する。
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type DomainResult<T> = Result<T, DomainError>;

/// アプリ全体で扱うエラー型（主にHTML/SSRルート用）。
/// 段階的に DomainError へ移行中。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("configuration error: {0}")]
    Config(#[from] figment::Error),

    #[error("template error: {0}")]
    Template(#[from] askama::Error),

    #[error("render error: {0}")]
    Render(#[from] minijinja::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error("session error: {0}")]
    Session(#[from] tower_sessions::session::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound => AppError::NotFound,
            DomainError::Conflict(msg) => AppError::Conflict(msg),
            DomainError::Validation(msg) | DomainError::BadRequest(msg) => {
                AppError::Conflict(msg) // HTML側では Conflict として扱う（既存挙動維持）
            }
            DomainError::Internal(inner) => AppError::Other(inner),
        }
    }
}

impl From<AppError> for DomainError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::NotFound => DomainError::NotFound,
            AppError::Conflict(msg) => DomainError::Conflict(msg),
            other => DomainError::Internal(anyhow::anyhow!("{other}")),
        }
    }
}

impl From<std::io::Error> for DomainError {
    fn from(err: std::io::Error) -> Self {
        DomainError::Internal(err.into())
    }
}

impl From<sqlx::Error> for DomainError {
    fn from(err: sqlx::Error) -> Self {
        DomainError::Internal(err.into())
    }
}

/// API（JSON）レスポンス用のエラー型。
/// サービス層のエラーをここにマッピングし、一貫した JSON エラーを返す。
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("internal server error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use serde_json::json;

        let (status, code, message) = match &self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg.clone()),
            ApiError::Internal(_) => {
                tracing::error!(error = %self, "API internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "サーバーエラーが発生しました".to_string(),
                )
            }
        };

        let body = json!({
            "error": {
                "code": code,
                "message": message,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound => ApiError::NotFound,
            DomainError::Conflict(msg) => ApiError::Conflict(msg),
            DomainError::Validation(msg) => ApiError::Validation(msg),
            DomainError::BadRequest(msg) => ApiError::BadRequest(msg),
            DomainError::Internal(inner) => ApiError::Internal(inner),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::NotFound => ApiError::NotFound,
            AppError::Conflict(msg) => ApiError::Conflict(msg),
            other => ApiError::Internal(anyhow::anyhow!("{other}")),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "見つかりませんでした").into_response(),
            AppError::Conflict(message) => (StatusCode::CONFLICT, message).into_response(),
            other => {
                tracing::error!(error = %other, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "サーバーエラーが発生しました",
                )
                    .into_response()
            }
        }
    }
}
