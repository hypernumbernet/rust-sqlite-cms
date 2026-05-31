use serde::{Deserialize, Serialize};

/// `widget_types` テーブルの行に対応する。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
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

/// image ウィジェットタイプの設定（共通設定なし）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageWidgetConfig {}

/// image エントリの float 値を検証する。
pub fn validate_image_float(float: &str) -> Result<(), String> {
    match float {
        "none" | "left" | "right" => Ok(()),
        _ => Err("回り込みは「なし」「左」「右」から選択してください".to_string()),
    }
}

/// image エントリの margin 値を検証する。空文字列は許可。
pub fn validate_image_margin(margin: &str) -> Result<(), String> {
    let margin = margin.trim();
    if margin.is_empty() {
        return Ok(());
    }

    for token in margin.split_whitespace() {
        if !is_margin_token(token) {
            return Err(
                "マージンは CSS の margin 形式（例: 16px、8px 16px）で指定してください".to_string(),
            );
        }
    }
    Ok(())
}

fn is_margin_token(token: &str) -> bool {
    let token = token.trim();
    if token.is_empty() {
        return false;
    }
    for unit in ["px", "em", "rem", "%"] {
        if let Some(num) = token.strip_suffix(unit) {
            return !num.is_empty() && num.parse::<f64>().is_ok();
        }
    }
    false
}

/// image エントリのリンク URL を検証する。空文字列は許可。
pub fn validate_image_link_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(());
    }
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with('/') {
        Ok(())
    } else {
        Err("リンク URL は http://、https://、または / で始めてください".to_string())
    }
}

/// image エントリ用の postmeta からインライン style を組み立てる。
pub fn build_image_style(float: &str, margin: &str) -> String {
    let mut parts = Vec::new();
    if float == "left" || float == "right" {
        parts.push(format!("float:{float}"));
    }
    let margin = margin.trim();
    if !margin.is_empty() {
        parts.push(format!("margin:{margin}"));
    }
    parts.join(";")
}

/// カルーセルウィジェットタイプの設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarouselWidgetConfig {
    #[serde(default = "default_carousel_interval")]
    pub interval: u32,
    #[serde(default = "default_carousel_width")]
    pub width: String,
    #[serde(default = "default_carousel_height")]
    pub height: String,
}

impl Default for CarouselWidgetConfig {
    fn default() -> Self {
        Self {
            interval: default_carousel_interval(),
            width: default_carousel_width(),
            height: default_carousel_height(),
        }
    }
}

fn default_carousel_interval() -> u32 {
    5
}

fn default_carousel_width() -> String {
    "100%".to_string()
}

fn default_carousel_height() -> String {
    "400px".to_string()
}

/// カルーセルのスライド間隔（秒）を検証する。
pub fn validate_carousel_interval(interval: u32) -> Result<(), String> {
    if !(1..=30).contains(&interval) {
        return Err("スライド間隔は 1 から 30 秒の範囲で指定してください".to_string());
    }
    Ok(())
}

/// カルーセルの幅・高さ（CSS サイズ値）を簡易検証。
pub fn validate_carousel_size(value: &str, field: &str) -> Result<(), String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(format!("{}を入力してください", field));
    }
    // 簡易チェック: 数値 + 単位 または % または auto
    if v == "auto" || v.ends_with('%') {
        return Ok(());
    }
    for unit in ["px", "em", "rem", "vh", "vw", "dvh"] {
        if let Some(num) = v.strip_suffix(unit) {
            if !num.is_empty() && num.parse::<f64>().is_ok() {
                return Ok(());
            }
        }
    }
    // 生の数値も px 扱いで許可（例: "400"）
    if v.parse::<f64>().is_ok() {
        return Ok(());
    }
    Err(format!(
        "{}は CSS サイズ形式（例: 100%、400px、50vh）で指定してください",
        field
    ))
}
