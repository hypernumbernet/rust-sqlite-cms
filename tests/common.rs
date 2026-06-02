//! テスト用共通ヘルパー。
//!
//! - テンポラリ作業ディレクトリ + SQLite DB の作成
//! - テスト用 AppState / Router の構築
//! - マイグレーション + 初期データ投入

// テストヘルパーは意図的に一部未使用のフィールド/メソッドを公開しているため警告を抑制
#![allow(dead_code)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use rust_sqlite_cms::{
    config::AppConfig,
    db,
    media,
    repos::{options, pages},
    routes,
    routes::admin::auth,
    session,
    state::AppState,
    theme::{self, Templates},
};
use tempfile::TempDir;
use tower::ServiceExt;

/// テスト用 admin パスワード（固定値）。
pub const TEST_ADMIN_PASSWORD: &str = "test-admin-password";

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
        config.security.session_secret = Some("test-session-secret-for-integration-tests".to_string());

        options::ensure_defaults(&pool, &config)
            .await
            .expect("failed to ensure defaults");

        auth::ensure_test_admin(&pool, TEST_ADMIN_PASSWORD)
            .await
            .expect("failed to ensure test admin");

        pages::ensure_index_page(&pool)
            .await
            .expect("failed to ensure index page");

        let templates = Arc::new(Templates::new(theme::templates_dir(&work_dir)));
        let static_dir = theme::static_dir(&work_dir);
        let uploads_dir_path = media::uploads_dir(&uploads_dir.to_string_lossy());

        let session_layer = session::session_layer(&config);
        let state = AppState {
            pool,
            config: Arc::new(config),
            templates,
        };

        let app = routes::router(static_dir, uploads_dir_path)
            .layer(session_layer)
            .with_state(state.clone());

        Self {
            app,
            state,
            _temp_dir: temp_dir,
        }
    }

    /// POST /admin/login して Set-Cookie を返す。
    pub async fn login_admin_cookie(&self) -> String {
        let body = format!("login=admin&password={TEST_ADMIN_PASSWORD}");
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("http://localhost/admin/login")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .expect("login request failed");

        assert_eq!(
            response.status(),
            StatusCode::SEE_OTHER,
            "admin login should redirect on success"
        );

        extract_session_cookie(&response)
    }

    /// ログイン済み Cookie 付きで管理画面へリクエストする。
    pub async fn admin_request(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        content_type: Option<&str>,
    ) -> axum::http::Response<axum::body::Body> {
        let cookie = self.login_admin_cookie().await;
        let method = method.parse::<Method>().unwrap();
        let uri = format!("http://localhost{path}");

        let mut builder = Request::builder().method(method).uri(uri).header("cookie", cookie);

        let req = if let Some(body) = body {
            if let Some(ct) = content_type {
                builder = builder.header("content-type", ct);
            }
            builder.body(Body::from(body.to_string())).unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };

        self.app.clone().oneshot(req).await.expect("request failed")
    }

    /// APIリクエストを送信する便利メソッド
    pub async fn api_request(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> axum::http::Response<axum::body::Body> {
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

fn extract_session_cookie(response: &axum::http::Response<axum::body::Body>) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(|cookie| cookie.split(';').next().unwrap_or(cookie))
        .collect::<Vec<_>>()
        .join("; ")
}
