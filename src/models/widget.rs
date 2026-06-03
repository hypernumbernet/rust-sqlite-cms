use serde::{Deserialize, Serialize};

/// ウィジェットタイプのエクスポート/インポート用パッケージ（format_version 1）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetPackage {
    pub format_version: u32,
    pub type_key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub config: String,
    pub html_template: String,
    pub config_schema: String,
}

/// インポート時の衝突解決モード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetImportMode {
    /// 既存行があれば上書き、なければ新規作成。
    Overwrite,
    /// 既存行があれば更新しない。
    Skip,
}

/// インポート結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetImportAction {
    Created,
    Updated,
    Skipped,
}

/// `widget_types` テーブルの行に対応する。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct WidgetType {
    pub id: i64,
    pub type_key: String,
    pub label: String,
    pub description: String,
    pub config: String,
    /// ウィジェットを構成するHTML/MiniJinjaフラグメント（ウィジェット画面で編集）。
    pub html_template: String,
    /// このウィジェットタイプのインスタンス（プレースホルダー）が持つべき設定項目のスキーマ定義（JSON）。
    /// プレースホルダー編集画面で、このスキーマに基づいて入力フォームを自動生成する。
    /// 例: { "fields": [ { "key": "limit", "label": "表示件数", "type": "number", "default": 5, "min": 1, "max": 50 } ] }
    pub config_schema: String,
    pub updated_at: String,
}

/// ウィジェットタイプ更新時にリポジトリへ渡す入力値（HTML構成編集用に拡張）。
#[derive(Debug, Clone)]
pub struct WidgetTypeInput {
    pub config: String,
    pub html_template: String,
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

/// image ウィジェットタイプのプレースホルダー単位設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageWidgetConfig {
    #[serde(default = "default_image_width")]
    pub width: String,
    #[serde(default = "default_image_height")]
    pub height: String,
    #[serde(default = "default_image_object_fit")]
    pub object_fit: String,
    #[serde(default = "default_image_border_radius")]
    pub border_radius: String,
}

impl Default for ImageWidgetConfig {
    fn default() -> Self {
        Self {
            width: default_image_width(),
            height: default_image_height(),
            object_fit: default_image_object_fit(),
            border_radius: default_image_border_radius(),
        }
    }
}

fn default_image_width() -> String {
    "100%".to_string()
}

fn default_image_height() -> String {
    "auto".to_string()
}

fn default_image_object_fit() -> String {
    "cover".to_string()
}

fn default_image_border_radius() -> String {
    "0".to_string()
}

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

/// image エントリ用の figure インライン style（float / margin）を組み立てる。
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

/// image の img 要素用インライン style（プレースホルダー単位のサイズ・収め方・角丸）。
pub fn build_image_img_style(
    width: &str,
    height: &str,
    object_fit: &str,
    border_radius: &str,
) -> String {
    let mut parts = vec![
        format!("width:{}", width.trim()),
        format!("height:{}", height.trim()),
        "display:block".to_string(),
    ];
    let height = height.trim();
    if height != "auto" {
        parts.push(format!("object-fit:{}", object_fit.trim()));
    }
    let radius = border_radius.trim();
    if !radius.is_empty() && radius != "0" {
        parts.push(format!("border-radius:{radius}"));
    }
    parts.join(";")
}

/// image の object-fit 値を検証する。
pub fn validate_image_object_fit(value: &str) -> Result<(), String> {
    match value.trim() {
        "cover" | "contain" | "fill" | "none" | "scale-down" => Ok(()),
        _ => Err(
            "画像の収め方は cover、contain、fill、none、scale-down から選択してください"
                .to_string(),
        ),
    }
}

/// image の角丸（CSS サイズ値）を検証する。空は 0 扱い。
pub fn validate_image_border_radius(value: &str) -> Result<(), String> {
    let v = value.trim();
    if v.is_empty() || v == "0" {
        return Ok(());
    }
    validate_css_size(v, "角丸")
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

/// CSS サイズ値を簡易検証（幅・高さ・角丸など共通）。
pub fn validate_css_size(value: &str, field: &str) -> Result<(), String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(format!("{}を入力してください", field));
    }
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
    if v.parse::<f64>().is_ok() {
        return Ok(());
    }
    Err(format!(
        "{}は CSS サイズ形式（例: 100%、400px、50vh）で指定してください",
        field
    ))
}

/// カルーセルの幅・高さ（CSS サイズ値）を簡易検証。
pub fn validate_carousel_size(value: &str, field: &str) -> Result<(), String> {
    validate_css_size(value, field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_widget_config_defaults() {
        let cfg = ImageWidgetConfig::default();
        assert_eq!(cfg.width, "100%");
        assert_eq!(cfg.height, "auto");
        assert_eq!(cfg.object_fit, "cover");
        assert_eq!(cfg.border_radius, "0");

        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: ImageWidgetConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.width, "100%");
    }

    #[test]
    fn validate_image_object_fit_accepts_known_values() {
        for v in ["cover", "contain", "fill", "none", "scale-down"] {
            assert!(validate_image_object_fit(v).is_ok(), "{}", v);
        }
        assert!(validate_image_object_fit("stretch").is_err());
    }

    #[test]
    fn image_border_radius_validation() {
        assert!(validate_image_border_radius("").is_ok());
        assert!(validate_image_border_radius("0").is_ok());
        assert!(validate_image_border_radius("8px").is_ok());
        assert!(validate_image_border_radius("50%").is_ok());
        assert!(validate_image_border_radius("bad").is_err());
    }

    #[test]
    fn build_image_img_style_fixed_height_includes_object_fit() {
        let style = build_image_img_style("100%", "280px", "cover", "12px");
        assert!(style.contains("width:100%"));
        assert!(style.contains("height:280px"));
        assert!(style.contains("object-fit:cover"));
        assert!(style.contains("border-radius:12px"));
    }

    #[test]
    fn build_image_img_style_auto_height_omits_object_fit() {
        let style = build_image_img_style("100%", "auto", "cover", "0");
        assert!(style.contains("height:auto"));
        assert!(!style.contains("object-fit"));
        assert!(!style.contains("border-radius"));
    }
}
