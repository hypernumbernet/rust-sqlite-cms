//! テスト用共通ヘルパー。
//!
//! - テンポラリ作業ディレクトリ + SQLite DB の作成
//! - テスト用 AppState / Router の構築
//! - マイグレーション + 初期データ投入

// テストヘルパーは意図的に一部未使用のフィールド/メソッドを公開しているため警告を抑制
#![allow(dead_code)]

use std::sync::Arc;

use rust_sqlite_cms::{
    config::AppConfig,
    db,
    media,
    repos::{options, pages, users},
    routes,
    state::AppState,
    theme::{self, Templates},
};
use tempfile::TempDir;
use tower::ServiceExt;

/// テスト用アプリケーション一式を返す。
///
/// 各テストで独立したテンポラリ環境が作られる。
pub struct TestApp {
    pub app: axum::Router,
    pub state: AppState,
    /// テスト終了時に自動削除される
    pub _temp_dir: TempDir,
}

impl TestApp {
    /// フル機能のテストアプリを作成（静的ファイル配信なども含む）
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let work_dir = temp_dir.path().to_str().unwrap().to_string();
        let uploads_dir = temp_dir.path().join("uploads");
        let db_path = temp_dir.path().join("test.db").to_string_lossy().to_string();

        // ディレクトリ準備
        std::fs::create_dir_all(&uploads_dir).unwrap();
        theme::ensure_seeded(&work_dir).unwrap();
        theme::ensure_pages_dir(&work_dir).unwrap();
        media::ensure_uploads_dir(uploads_dir.to_str().unwrap()).unwrap();

        // DB
        let pool = db::connect(&db_path).await.expect("failed to connect test db");
        db::migrate(&pool).await.expect("failed to migrate test db");

        // 最小設定
        let mut config = AppConfig::default();
        config.database.path = db_path.clone();
        config.paths.work_dir = work_dir.clone();
        config.paths.uploads_dir = uploads_dir.to_string_lossy().to_string();

        options::ensure_defaults(&pool, &config)
            .await
            .expect("failed to ensure defaults");

        users::ensure_default_admin(&pool)
            .await
            .expect("failed to ensure default admin");

        pages::ensure_index_page(&pool)
            .await
            .expect("failed to ensure index page");

        let templates = Arc::new(Templates::new(theme::templates_dir(&work_dir)));
        let static_dir = theme::static_dir(&work_dir);
        let uploads_dir_path = media::uploads_dir(&uploads_dir.to_string_lossy());

        let state = AppState {
            pool,
            config: Arc::new(config),
            templates,
        };

        let app = routes::router(static_dir, uploads_dir_path).with_state(state.clone());

        Self {
            app,
            state,
            _temp_dir: temp_dir,
        }
    }

    /// APIリクエストを送信する便利メソッド
    pub async fn api_request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> axum::http::Response<axum::body::Body> {
        use axum::http::{Method, Request};

        let uri = format!("http://localhost{}", path);
        let method = method.parse::<Method>().unwrap();

        let req = if let Some(json_body) = body {
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(json_body.to_string())
                .unwrap()
        } else {
            Request::builder()
                .method(method)
                .uri(uri)
                .body(String::new())
                .unwrap()
        };

        self.app
            .clone()
            .oneshot(req)
            .await
            .expect("request failed")
    }
}
