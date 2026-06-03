use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt};

use rust_sqlite_cms::{
    config::AppConfig, db, error::AppResult, media,
    repos::{options, pages, users},
    routes, routes::admin::auth, session, state::AppState,
    theme::{self, Templates},
};

struct CliArgs {
    test_mode: bool,
}

fn parse_args() -> CliArgs {
    let mut test_mode = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--test" => test_mode = true,
            other => {
                eprintln!("不明なオプション: {other}");
                eprintln!("使用法: cargo run [-- --test]");
                std::process::exit(1);
            }
        }
    }
    CliArgs { test_mode }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rust_sqlite_cms=debug")),
        )
        .init();

    let args = parse_args();

    AppConfig::ensure_default_file()?;
    let config = AppConfig::load()?;
    tracing::info!(bind_addr = %config.server.bind_addr, db = %config.database.path, "起動設定を読み込みました");
    if args.test_mode {
        tracing::warn!("テストモードで起動します（admin パスワードは常に testpass）");
    }

    let pool = db::connect(&config.database.path).await?;
    db::migrate(&pool).await?;
    tracing::info!("マイグレーションを適用しました");

    options::ensure_defaults(&pool, &config).await?;
    tracing::info!("既定の options を確認しました");

    if args.test_mode {
        auth::ensure_test_admin(&pool, auth::TEST_MODE_ADMIN_PASSWORD).await?;
        tracing::info!(
            login = "admin",
            password = auth::TEST_MODE_ADMIN_PASSWORD,
            "テストモード: 管理ユーザー admin のパスワードを設定しました"
        );
    } else {
        users::ensure_default_admin(&pool).await?;
        tracing::info!("既定の管理ユーザーを確認しました");
    }

    theme::ensure_seeded(&config.paths.work_dir)?;
    theme::ensure_pages_dir(&config.paths.work_dir)?;
    media::ensure_uploads_dir(&config.paths.uploads_dir)?;
    pages::ensure_index_page(&pool).await?;
    tracing::info!("作業ディレクトリを初期化しました");

    let bind_addr = config.server.bind_addr.clone();
    let templates = Arc::new(Templates::new(theme::templates_dir(&config.paths.work_dir)));
    let static_dir = theme::static_dir(&config.paths.work_dir);
    let uploads_dir = media::uploads_dir(&config.paths.uploads_dir);
    let session_layer = session::session_layer(&config);
    let state = AppState {
        pool,
        config: Arc::new(config),
        templates,
    };

    let app = routes::router(static_dir, uploads_dir)
        .layer(session_layer)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("公開サイト: http://{bind_addr}/");
    tracing::info!("管理画面: http://{bind_addr}/admin");

    axum::serve(listener, app).await?;
    Ok(())
}
