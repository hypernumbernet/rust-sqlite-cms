use std::sync::Arc;

use askama::Template;
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use tracing_subscriber::{EnvFilter, fmt};

use rust_sqlite_cms::{
    config::AppConfig,
    db,
    error::AppResult,
    repos::options,
    state::AppState,
};

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    blogname: String,
    blogdescription: String,
}

async fn admin_dashboard(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    let blogname = options::get(&state.pool, "blogname")
        .await?
        .unwrap_or_else(|| state.config.site.title.clone());
    let blogdescription = options::get(&state.pool, "blogdescription")
        .await?
        .unwrap_or_else(|| state.config.site.tagline.clone());

    let html = DashboardTemplate {
        blogname,
        blogdescription,
    }
    .render()?;
    Ok(Html(html))
}

async fn index() -> impl IntoResponse {
    Redirect::temporary("/admin")
}

#[tokio::main]
async fn main() -> AppResult<()> {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rust_sqlite_cms=debug")),
        )
        .init();

    let config = AppConfig::load()?;
    tracing::info!(bind_addr = %config.server.bind_addr, db = %config.database.path, "起動設定を読み込みました");

    let pool = db::connect(&config.database.path).await?;
    db::migrate(&pool).await?;
    tracing::info!("マイグレーションを適用しました");

    options::ensure_defaults(&pool, &config).await?;
    tracing::info!("既定の options を確認しました");

    let bind_addr = config.server.bind_addr.clone();
    let state = AppState {
        pool,
        config: Arc::new(config),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/admin", get(admin_dashboard))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("管理画面: http://{bind_addr}/admin");

    axum::serve(listener, app).await?;
    Ok(())
}
