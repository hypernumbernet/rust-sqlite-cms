use serde::{Deserialize, Serialize};

/// `widget_types` テーブルの行に対応する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WidgetType {
    pub id: i64,
    pub type_key: String,
    pub config: String,
    pub updated_at: String,
}

/// ウィジェットタイプ更新時にリポジトリへ渡す入力値。
#[derive(Debug, Clone)]
pub struct WidgetTypeInput {
    pub config: String,
}

/// news ウィジェットタイプの設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsWidgetConfig {
    #[serde(default = "default_news_limit")]
    pub limit: i64,
}

impl Default for NewsWidgetConfig {
    fn default() -> Self {
        Self {
            limit: default_news_limit(),
        }
    }
}

fn default_news_limit() -> i64 {
    5
}

/// news ウィジェットタイプの表示件数を検証する。
pub fn validate_news_limit(limit: i64) -> Result<(), String> {
    if !(1..=50).contains(&limit) {
        return Err("表示件数は 1 から 50 の範囲で指定してください".to_string());
    }
    Ok(())
}
