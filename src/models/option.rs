/// `options` テーブルの 1 行に対応する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OptionRow {
    pub option_id: i64,
    pub option_name: String,
    pub option_value: String,
    pub autoload: i64,
}
