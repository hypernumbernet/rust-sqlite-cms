use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt};

use rust_sqlite_cms::{
    config::AppConfig, db, error::AppResult, repos::options, routes, state::AppState,
    theme::Templates,
};

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
    let templates = Arc::new(Templates::new(config.paths.themes_dir.clone().into()));
    let state = AppState {
        pool,
        config: Arc::new(config),
        templates,
    };

    let app = routes::router().with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("公開サイト: http://{bind_addr}/");
    tracing::info!("管理画面: http://{bind_addr}/admin");

    axum::serve(listener, app).await?;
    Ok(())
}
