use std::sync::{Arc, RwLock};

use arc_swap::ArcSwap;
use sqlx::SqlitePool;

use crate::config::AppConfig;
use crate::db;
use crate::error::AppResult;
use crate::theme::{self, Templates};

/// 各ハンドラへ共有されるアプリケーション状態。
/// `axum` の `State` で取り回すため `Clone` 可能（中身は共有参照）。
#[derive(Clone)]
pub struct AppState {
    pool: Arc<ArcSwap<SqlitePool>>,
    pub config: Arc<AppConfig>,
    templates: Arc<RwLock<Arc<Templates>>>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Arc<AppConfig>, templates: Arc<Templates>) -> Self {
        Self {
            pool: Arc::new(ArcSwap::from_pointee(pool)),
            config,
            templates: Arc::new(RwLock::new(templates)),
        }
    }

    /// 現在の DB 接続プールを取得する。
    pub fn pool(&self) -> SqlitePool {
        (*self.pool.load_full()).clone()
    }

    /// 現在のテンプレートエンジンを取得する。
    pub fn templates(&self) -> Arc<Templates> {
        self.templates.read().expect("templates lock").clone()
    }

    /// SQLite ファイルを差し替える前に、既存接続を閉じる。
    pub async fn release_pool(&self) {
        self.pool.load_full().close().await;
    }

    /// ディスク上の DB / レイアウトから接続とテンプレートを再読み込みする。
    pub async fn reload_storage(&self) -> AppResult<()> {
        let config = self.config.as_ref();
        let pool = db::connect(&config.database.path).await?;
        self.pool.store(Arc::new(pool));
        self.reload_templates();
        Ok(())
    }

    /// 新しい DB 接続とテンプレートをインストールする（サンプルリセット等）。
    pub async fn install_storage(&self, pool: SqlitePool) -> AppResult<()> {
        self.pool.store(Arc::new(pool));
        self.reload_templates();
        Ok(())
    }

    fn reload_templates(&self) {
        let templates = Arc::new(Templates::new(theme::layouts_dir(
            &self.config.paths.work_dir,
        )));
        *self.templates.write().expect("templates lock") = templates;
    }
}
