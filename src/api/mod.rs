//! JSON API ルーティング（/api/v1 配下）。
//!
//! すべてのエンドポイントはサービス層を呼び、JSON で応答する。
//! 将来的な認証・レート制限などのミドルウェアをここに layer しやすい構造。

use axum::Router;

use crate::state::AppState;

pub mod v1;

pub fn router() -> Router<AppState> {
    Router::new().nest("/v1", v1::router())
}
