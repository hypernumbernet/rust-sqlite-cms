use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// アプリ全体で扱うエラー型。`?` で各レイヤーのエラーを集約し、
/// HTTP レスポンスへ変換する。
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

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "見つかりませんでした").into_response(),
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
