use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::AppConfig;

/// 各ハンドラへ共有されるアプリケーション状態。
/// `axum` の `State` で取り回すため `Clone` 可能（中身は共有参照）。
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<AppConfig>,
}
