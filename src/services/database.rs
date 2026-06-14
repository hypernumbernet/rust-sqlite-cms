//! データベーススキーマのイントロスペクションとユーザー定義オブジェクトの管理。

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDateTime, TimeZone};
use futures_util::future::try_join_all;
use rand::rngs::OsRng;
use rand::Rng;
use serde::Deserialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::error::{AppError, AppResult, DomainError, DomainResult};

/// テーブルデータ一覧の1回あたりの取得行数。
pub const TABLE_DATA_CHUNK_SIZE: i64 = 1000;

/// テストデータ生成の1回あたりの最大件数。
pub const MAX_TEST_DATA_ROWS: u32 = 10_000_000;

/// テストデータ生成フォームの既定件数。
pub const DEFAULT_SEED_COUNT: u32 = 100;

const NULL_SEED_PROBABILITY: f64 = 0.1;

const ASCII_ALNUM: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ASCII_PRINTABLE: &[u8] = b" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
const HIRAGANA: &str = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをん";
const KATAKANA: &str = "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン";
const KANJI: &str = "日月火水木金土山川田人口手目耳足心文字本学校会社国都市町村春夏秋冬";

/// 文字列テストデータの文字種。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringCharset {
    AsciiAlnum,
    AsciiPrintable,
    Japanese,
}

/// カラムごとのテストデータ生成ルール。
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnSeedRule {
    Integer {
        min: i64,
        max: i64,
        include_null: bool,
    },
    Real {
        min: f64,
        max: f64,
        include_null: bool,
    },
    Text {
        min_len: u32,
        max_len: u32,
        charset: StringCharset,
        include_null: bool,
    },
    Blob {
        min_len: u32,
        max_len: u32,
        include_null: bool,
    },
    Timestamp {
        from: String,
        to: String,
        include_null: bool,
    },
    Boolean {
        include_null: bool,
    },
}

/// テストデータ生成フォームの1カラム分の表示用データ。
#[derive(Debug, Clone)]
pub struct SeedFormRow {
    pub name: String,
    pub type_key: String,
    pub type_label: String,
    pub nullable: bool,
    pub int_min: String,
    pub int_max: String,
    pub real_min: String,
    pub real_max: String,
    pub text_min: String,
    pub text_max: String,
    pub charset: String,
    pub blob_min: String,
    pub blob_max: String,
    pub timestamp_from: String,
    pub timestamp_to: String,
    pub include_null: bool,
}

/// テストデータ生成フォームの POST データ。
#[derive(Debug, Deserialize, Default)]
pub struct TestDataSeedForm {
    #[serde(default)]
    pub count: String,
    #[serde(default)]
    pub col_name: Vec<String>,
    #[serde(default)]
    pub col_type: Vec<String>,
    #[serde(default)]
    pub col_int_min: Vec<String>,
    #[serde(default)]
    pub col_int_max: Vec<String>,
    #[serde(default)]
    pub col_real_min: Vec<String>,
    #[serde(default)]
    pub col_real_max: Vec<String>,
    #[serde(default)]
    pub col_text_min: Vec<String>,
    #[serde(default)]
    pub col_text_max: Vec<String>,
    #[serde(default)]
    pub col_charset: Vec<String>,
    #[serde(default)]
    pub col_blob_min: Vec<String>,
    #[serde(default)]
    pub col_blob_max: Vec<String>,
    #[serde(default)]
    pub col_timestamp_from: Vec<String>,
    #[serde(default)]
    pub col_timestamp_to: Vec<String>,
    #[serde(default)]
    pub col_include_null: Vec<String>,
}

/// テーブル作成フォームから受け取るカラム定義。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableColumnInput {
    pub name: String,
    pub type_key: String,
    pub nullable: bool,
    /// 列編集時の元カラム名。新規列は `None`。
    pub orig_name: Option<String>,
}

/// 列編集の差分マイグレーション計画。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMigrationPlan {
    pub renames: Vec<(String, String)>,
    pub drops: Vec<String>,
    pub adds: Vec<TableColumnInput>,
    /// NOT NULL から NULL 可へ緩和する列の元カラム名。
    pub nullable_relaxations: Vec<String>,
}

/// 列ソートの方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TableSortDirection {
    Asc,
    Desc,
}

/// 列ソートの1エントリ（配列の先頭が最優先）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableSortEntry {
    pub column: String,
    pub direction: TableSortDirection,
}

/// 列フィルターの1エントリ（複数列は AND 条件）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableFilterEntry {
    pub column: String,
    pub text: String,
}

/// データ一覧 API 向けの列メタデータ。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableDataColumnMeta {
    pub name: String,
    pub pk: bool,
    pub type_key: String,
    pub nullable: bool,
}

/// ビュー UI ビルダー向けの列メタデータ。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableColumnUiInfo {
    pub name: String,
    pub type_key: String,
    pub pk: bool,
    pub nullable: bool,
}

/// ビュー UI ビルダーで表示する列の状態。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimpleViewUiColumn {
    pub name: String,
    pub type_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_condition: Option<String>,
}

/// SELECT 列に含まれない列への WHERE 条件。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtraWhereCondition {
    pub column: String,
    pub suffix: String,
}

/// ビュー UI ビルダーの完全な状態。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimpleViewUiSpec {
    pub base_table: String,
    #[serde(default)]
    pub distinct: bool,
    pub columns: Vec<SimpleViewUiColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_where: Vec<ExtraWhereCondition>,
}

/// ビュー定義の UI 展開結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ViewUiSpecResolveResult {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<SimpleViewUiSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_columns: Option<Vec<TableColumnUiInfo>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedViewColumn {
    pub(crate) name: String,
    pub(crate) alias: Option<String>,
    pub(crate) expression: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum ParsedViewColumnList {
    All,
    Columns(Vec<ParsedViewColumn>),
}

/// 単純な WHERE 条件（列名 + サフィックス）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedWhereCondition {
    pub(crate) column: String,
    pub(crate) suffix: String,
}

/// 単純な `SELECT ... FROM table` 定義の解析結果。
#[derive(Debug, Clone)]
pub(crate) struct ParsedSimpleViewSelect {
    pub(crate) base_table: String,
    pub(crate) distinct: bool,
    pub(crate) columns: ParsedViewColumnList,
    pub(crate) where_conditions: Vec<ParsedWhereCondition>,
}

/// セル更新リクエスト。
#[derive(Debug, Clone, Deserialize)]
pub struct TableCellUpdateRequest {
    pub column: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub null: bool,
    pub keys: std::collections::HashMap<String, String>,
}

/// セル更新結果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableCellUpdateResult {
    pub value: Option<String>,
}

/// ユーザーテーブルのデータ一覧（1チャンク分）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableDataView {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub total_count: i64,
    pub offset: i64,
    pub has_more: bool,
    /// offset=0 のときのみ。保存済み列幅（カラム名 → ピクセル幅）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_widths: Option<std::collections::HashMap<String, i32>>,
    /// offset=0 のときのみ。適用中のソート（空ならデフォルト順）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<TableSortEntry>>,
    /// offset=0 のときのみ。適用中の列フィルター。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<TableFilterEntry>>,
    /// offset=0 のときのみ。列メタデータ（PK・型・NULL 可）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_meta: Option<Vec<TableDataColumnMeta>>,
}

/// 一覧画面の複製ダイアログ向けカラム JSON。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableColumnDuplicateJson {
    pub name: String,
    pub type_key: String,
    pub nullable: bool,
}

/// 一覧画面の複製ダイアログ向けペイロード。
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct DatabaseDuplicatePayloads {
    pub tables: HashMap<String, Vec<TableColumnDuplicateJson>>,
    pub views: HashMap<String, String>,
}

/// 一覧取得済みの SQL から複製ダイアログ用ペイロードを組み立てる。
pub fn build_duplicate_payloads(
    tables: &[DbObjectItem],
    views: &[DbObjectItem],
) -> DatabaseDuplicatePayloads {
    let mut payloads = DatabaseDuplicatePayloads::default();

    for item in tables {
        if !is_db_admin_editable(&item.name) {
            continue;
        }
        let Ok(columns) = parse_user_columns_from_create_sql(&item.sql) else {
            continue;
        };
        payloads.tables.insert(
            item.name.clone(),
            columns
                .into_iter()
                .map(|column| TableColumnDuplicateJson {
                    name: column.name,
                    type_key: column.type_key,
                    nullable: column.nullable,
                })
                .collect(),
        );
    }

    for item in views {
        if !is_db_admin_editable(&item.name) {
            continue;
        }
        let Ok(definition) = extract_view_select_from_create_sql(&item.sql) else {
            continue;
        };
        payloads.views.insert(item.name.clone(), definition);
    }

    payloads
}

/// `sqlite_master` から取得したテーブルまたはビューの一覧項目。
#[derive(Debug, Clone)]
pub struct DbObjectItem {
    pub name: String,
    pub sql: String,
    pub sql_preview: String,
    pub is_system: bool,
    pub row_count: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MasterRow {
    name: String,
    sql: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PragmaTableInfoRow {
    name: String,
    #[sqlx(rename = "type")]
    type_name: String,
    notnull: i32,
    pk: i32,
}

/// CMS コアテーブル（マイグレーション定義・リードオンリー扱い）。
const CMS_TABLES: &[&str] = &[
    "widget_types",
    "placeholders",
    "posts",
    "postmeta",
    "options",
    "layouts",
    "pages",
    "users",
    "user_table_meta",
];

/// 管理画面 DB 一覧に表示しないインフラ用テーブル。_sqlx_migrationsのカラムタイプが表示が難しいため
const HIDDEN_ADMIN_TABLES: &[&str] = &["_sqlx_migrations"];

/// DB 管理 UI メタデータ（列幅など）を保持するシステムテーブル名。
pub const USER_TABLE_META_TABLE: &str = "user_table_meta";

/// 列幅の最小・最大ピクセル値。
const COLUMN_WIDTH_MIN_PX: i32 = 40;
const COLUMN_WIDTH_MAX_PX: i32 = 2000;

/// 管理画面 DB 一覧に表示しないインフラ用テーブルかどうか。
pub fn is_hidden_admin_table(name: &str) -> bool {
    HIDDEN_ADMIN_TABLES.contains(&name)
}

/// インフラ用テーブル（`_sqlx_migrations` や `sqlite_*`）かどうか。
pub fn is_infra_table(name: &str) -> bool {
    is_hidden_admin_table(name) || name.starts_with("sqlite_")
}

/// CMS コアテーブル（`users` 含む 8 表）かどうか。
pub fn is_cms_core_table(name: &str) -> bool {
    CMS_TABLES.contains(&name)
}

/// DB 管理でデータ閲覧のみ可能な CMS コアテーブルかどうか。
pub fn is_cms_readonly_table(name: &str) -> bool {
    is_cms_core_table(name)
}

/// DB 管理で列編集・テストデータ生成が可能かどうか（ユーザー定義テーブルのみ）。
pub fn is_db_admin_editable(name: &str) -> bool {
    !is_infra_table(name) && !is_cms_core_table(name)
}

/// DB 管理でデータ閲覧が可能かどうか。
pub fn is_db_admin_data_viewable(name: &str) -> bool {
    !is_infra_table(name)
}

/// インフラ用テーブルへの操作拒否メッセージ。
pub fn infra_table_denied_message(name: &str) -> String {
    format!("`{name}` はインフラ用のシステムテーブルです。編集・データ閲覧はできません。")
}

/// CMS コアテーブルへの列編集・シード拒否メッセージ。
pub fn cms_table_edit_denied_message(name: &str) -> String {
    format!("`{name}` はCMSのシステムテーブルです。列編集・テストデータ生成はできません。")
}

/// システムテーブルへの編集・データ閲覧拒否メッセージ（後方互換）。
pub fn system_table_denied_message(name: &str) -> String {
    if is_infra_table(name) {
        infra_table_denied_message(name)
    } else {
        cms_table_edit_denied_message(name)
    }
}

/// システム系オブジェクト（一覧の種別バッジ・予約名チェック用）かどうか。
pub fn is_system_table(name: &str) -> bool {
    is_infra_table(name) || is_cms_core_table(name)
}

pub fn truncate_sql(sql: &str, max_len: usize) -> String {
    if sql.chars().count() <= max_len {
        sql.to_string()
    } else {
        let truncated: String = sql.chars().take(max_len).collect();
        format!("{truncated}…")
    }
}

pub async fn list_tables(pool: &SqlitePool) -> AppResult<Vec<DbObjectItem>> {
    let rows = sqlx::query_as::<_, MasterRow>(
        r#"
        SELECT name, sql
        FROM sqlite_master
        WHERE type = 'table'
          AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?;

    let visible_rows: Vec<_> = rows
        .into_iter()
        .filter(|row| !is_hidden_admin_table(&row.name))
        .collect();
    let counts = try_join_all(
        visible_rows
            .iter()
            .map(|row| table_row_count(pool, &row.name, "", &[])),
    )
    .await?;

    Ok(visible_rows
        .into_iter()
        .zip(counts)
        .map(|(row, row_count)| {
            let sql = row.sql.unwrap_or_default();
            DbObjectItem {
                name: row.name.clone(),
                sql_preview: truncate_sql(&sql, 80),
                is_system: is_system_table(&row.name),
                row_count: Some(row_count),
                sql,
            }
        })
        .collect())
}

pub async fn list_views(pool: &SqlitePool) -> AppResult<Vec<DbObjectItem>> {
    let rows = sqlx::query_as::<_, MasterRow>(
        r#"
        SELECT name, sql
        FROM sqlite_master
        WHERE type = 'view'
          AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| {
            let sql = row.sql.unwrap_or_default();
            DbObjectItem {
                name: row.name.clone(),
                sql_preview: truncate_sql(&sql, 80),
                is_system: is_system_table(&row.name),
                row_count: None,
                sql,
            }
        })
        .collect();

    Ok(items)
}

const FORBIDDEN_DDL_KEYWORDS: &[&str] = &[
    "ATTACH", "ALTER", "CREATE", "DELETE", "DETACH", "DROP", "INSERT", "PRAGMA", "UPDATE",
];

const MAX_SQL_IDENTIFIER_LEN: usize = 120;
const MAX_BLOB_SEED_BYTES: u32 = 4096;

fn sanitize_identifier_input(name: &str) -> &str {
    name.trim()
}

fn validate_identifier_chars(name: &str, label: &str) -> DomainResult<()> {
    if name.is_empty() {
        return Err(DomainError::Validation(format!("{label}は必須です")));
    }
    if name.chars().count() > MAX_SQL_IDENTIFIER_LEN {
        return Err(DomainError::Validation(format!(
            "{label}は {MAX_SQL_IDENTIFIER_LEN} 文字以内で指定してください"
        )));
    }
    for ch in name.chars() {
        if ch.is_control() || ch == '"' || ch == ';' {
            return Err(DomainError::Validation(format!(
                "{label}に使用できない文字が含まれています"
            )));
        }
    }
    Ok(())
}

/// SQLite 識別子をダブルクォートで囲み、内部の `"` をエスケープする。
pub fn quote_sql_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// ユーザーテーブル・ビュー名を検証する。
pub fn validate_table_name(name: &str) -> DomainResult<String> {
    let name = sanitize_identifier_input(name);
    validate_identifier_chars(name, "名前")?;
    if name.contains('/') || name.contains('\\') {
        return Err(DomainError::Validation(
            "名前に / または \\ は使用できません".into(),
        ));
    }
    if name.starts_with("sqlite_") || is_system_table(name) {
        return Err(DomainError::Validation(
            "この名前はシステム用に予約されています".into(),
        ));
    }
    Ok(name.to_string())
}

/// ユーザーカラム名を検証する。
pub fn validate_column_name(name: &str) -> DomainResult<String> {
    let name = sanitize_identifier_input(name);
    validate_identifier_chars(name, "カラム名")?;
    if name.eq_ignore_ascii_case("id") {
        return Err(DomainError::Validation(
            "カラム名 `id` は自動追加されるため使用できません".into(),
        ));
    }
    Ok(name.to_string())
}

/// ユーザーが作成できるオブジェクト名か検証する。
pub fn validate_user_object_name(name: &str) -> DomainResult<String> {
    validate_table_name(name)
}

pub fn build_table_definition(columns: &[TableColumnInput]) -> DomainResult<String> {
    let mut parts = vec!["id INTEGER PRIMARY KEY".to_string()];
    let mut seen = std::collections::HashSet::from(["id".to_string()]);

    for column in columns {
        if column.name.trim().is_empty() {
            continue;
        }
        let name = validate_column_name(&column.name)?;
        if !seen.insert(name.clone()) {
            return Err(DomainError::Validation(format!(
                "カラム名 `{name}` が重複しています"
            )));
        }

        parts.push(column_definition_sql(&name, column)?);
    }

    Ok(parts.join(", "))
}

pub async fn load_user_table_columns(
    pool: &SqlitePool,
    name: &str,
) -> DomainResult<Vec<TableColumnInput>> {
    let name = ensure_editable_user_table(name)?;
    if !object_name_exists(pool, name, "table").await? {
        return Err(DomainError::NotFound);
    }

    let sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;

    let sql = sql.ok_or(DomainError::NotFound)?;
    parse_user_columns_from_create_sql(&sql)
}

/// データ一覧画面向けに、テーブルが閲覧可能か検証する。
pub async fn ensure_user_table_viewable(pool: &SqlitePool, name: &str) -> DomainResult<()> {
    ensure_named_object_viewable(pool, name, "table").await
}

/// データ一覧画面向けに、ビューが閲覧可能か検証する。
pub async fn ensure_user_view_viewable(pool: &SqlitePool, name: &str) -> DomainResult<()> {
    ensure_named_object_viewable(pool, name, "view").await
}

pub async fn update_user_table_from_columns(
    pool: &SqlitePool,
    name: &str,
    columns: &[TableColumnInput],
) -> DomainResult<()> {
    let name = ensure_editable_user_table(name)?;
    if !object_name_exists(pool, name, "table").await? {
        return Err(DomainError::NotFound);
    }

    let current = load_user_table_columns(pool, name).await?;
    let plan = plan_column_migration(&current, columns)?;
    validate_column_migration_adds(pool, name, &plan).await?;
    apply_column_migration(pool, name, &plan, columns).await?;
    Ok(())
}

/// 既存列定義と編集後の定義から差分マイグレーション計画を組み立てる。
pub fn plan_column_migration(
    current: &[TableColumnInput],
    desired: &[TableColumnInput],
) -> DomainResult<ColumnMigrationPlan> {
    let mut normalized = Vec::new();
    for column in desired {
        if column.name.trim().is_empty() {
            continue;
        }
        let name = validate_column_name(&column.name)?;
        normalized.push(TableColumnInput {
            name,
            type_key: column.type_key.clone(),
            nullable: column.nullable,
            orig_name: column.orig_name.clone(),
        });
    }

    build_table_definition(&normalized)?;

    let current_by_name: std::collections::HashMap<&str, &TableColumnInput> = current
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect();

    let mut renames = Vec::new();
    let mut nullable_relaxations = Vec::new();
    for column in &normalized {
        let Some(orig_name) = column.orig_name.as_deref() else {
            continue;
        };
        let Some(existing) = current_by_name.get(orig_name) else {
            return Err(DomainError::Validation(format!(
                "列 `{orig_name}` は存在しません"
            )));
        };
        if existing.type_key != column.type_key {
            return Err(DomainError::Validation(format!(
                "列 `{orig_name}` の型は変更できません"
            )));
        }
        if existing.nullable && !column.nullable {
            return Err(DomainError::Validation(format!(
                "列 `{orig_name}` を NOT NULL に変更することはできません"
            )));
        }
        if !existing.nullable && column.nullable {
            nullable_relaxations.push(orig_name.to_string());
        }
        if orig_name != column.name {
            renames.push((orig_name.to_string(), column.name.clone()));
        }
    }

    let retained_orig: std::collections::HashSet<String> = normalized
        .iter()
        .filter_map(|column| column.orig_name.clone())
        .collect();
    let drops = current
        .iter()
        .map(|column| column.name.clone())
        .filter(|name| !retained_orig.contains(name))
        .collect();

    let adds = normalized
        .iter()
        .filter(|column| column.orig_name.is_none())
        .cloned()
        .collect();

    Ok(ColumnMigrationPlan {
        renames,
        drops,
        adds,
        nullable_relaxations,
    })
}

async fn validate_column_migration_adds(
    pool: &SqlitePool,
    name: &str,
    plan: &ColumnMigrationPlan,
) -> DomainResult<()> {
    let has_not_null_add = plan.adds.iter().any(|column| !column.nullable);
    if !has_not_null_add {
        return Ok(());
    }

    let quoted = quote_sql_identifier(name);
    let count_sql = format!("SELECT COUNT(*) FROM {quoted}");
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Err(DomainError::Validation(
            "既存データがあるテーブルには NOT NULL な列を追加できません".into(),
        ));
    }
    Ok(())
}

async fn apply_column_migration(
    pool: &SqlitePool,
    name: &str,
    plan: &ColumnMigrationPlan,
    desired: &[TableColumnInput],
) -> DomainResult<()> {
    if !plan.nullable_relaxations.is_empty() {
        return rebuild_user_table(pool, name, desired).await;
    }

    if plan.renames.is_empty() && plan.drops.is_empty() && plan.adds.is_empty() {
        return Ok(());
    }

    let quoted_table = quote_sql_identifier(name);
    let mut tx = pool.begin().await?;

    for (old_name, new_name) in &plan.renames {
        let ddl = format!(
            "ALTER TABLE {quoted_table} RENAME COLUMN {} TO {}",
            quote_sql_identifier(old_name),
            quote_sql_identifier(new_name)
        );
        sqlx::query(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *tx)
            .await?;
    }

    for column_name in &plan.drops {
        let ddl = format!(
            "ALTER TABLE {quoted_table} DROP COLUMN {}",
            quote_sql_identifier(column_name)
        );
        sqlx::query(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *tx)
            .await?;
    }

    for column in &plan.adds {
        let definition = column_definition_sql(&column.name, column)?;
        let ddl = format!("ALTER TABLE {quoted_table} ADD COLUMN {definition}");
        sqlx::query(sqlx::AssertSqlSafe(ddl))
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    sanitize_data_ui_preferences(
        pool,
        name,
        SanitizeUiPreferencesOptions {
            clear_sort_filter: false,
            column_renames: &plan.renames,
        },
    )
    .await?;
    Ok(())
}

/// NOT NULL 緩和など、ALTER TABLE だけでは反映できない変更をテーブル再構築で適用する。
async fn rebuild_user_table(
    pool: &SqlitePool,
    table_name: &str,
    desired: &[TableColumnInput],
) -> DomainResult<()> {
    let mut normalized = Vec::new();
    for column in desired {
        if column.name.trim().is_empty() {
            continue;
        }
        let name = validate_column_name(&column.name)?;
        normalized.push(TableColumnInput {
            name,
            type_key: column.type_key.clone(),
            nullable: column.nullable,
            orig_name: column.orig_name.clone(),
        });
    }

    let definition = build_table_definition(&normalized)?;
    let quoted_table = quote_sql_identifier(table_name);
    let temp_name = format!("{table_name}__cms_rebuild");
    let quoted_temp = quote_sql_identifier(&temp_name);

    let mut dest_columns = vec![quote_sql_identifier("id")];
    let mut source_exprs = vec!["id".to_string()];
    for column in &normalized {
        dest_columns.push(quote_sql_identifier(&column.name));
        let source = if let Some(orig_name) = column.orig_name.as_deref() {
            quote_sql_identifier(orig_name)
        } else {
            "NULL".to_string()
        };
        source_exprs.push(source);
    }

    let create_ddl = format!("CREATE TABLE {quoted_temp} ({definition})");
    let insert_ddl = format!(
        "INSERT INTO {quoted_temp} ({}) SELECT {} FROM {quoted_table}",
        dest_columns.join(", "),
        source_exprs.join(", ")
    );
    let drop_ddl = format!("DROP TABLE {quoted_table}");
    let rename_ddl = format!(
        "ALTER TABLE {quoted_temp} RENAME TO {}",
        quote_sql_identifier(table_name)
    );

    let mut tx = pool.begin().await?;
    sqlx::query(sqlx::AssertSqlSafe(create_ddl))
        .execute(&mut *tx)
        .await?;
    sqlx::query(sqlx::AssertSqlSafe(insert_ddl))
        .execute(&mut *tx)
        .await?;
    sqlx::query(sqlx::AssertSqlSafe(drop_ddl))
        .execute(&mut *tx)
        .await?;
    sqlx::query(sqlx::AssertSqlSafe(rename_ddl))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let renames: Vec<(String, String)> = normalized
        .iter()
        .filter_map(|column| {
            column.orig_name.as_ref().and_then(|orig_name| {
                if orig_name == &column.name {
                    None
                } else {
                    Some((orig_name.clone(), column.name.clone()))
                }
            })
        })
        .collect();
    sanitize_data_ui_preferences(
        pool,
        table_name,
        SanitizeUiPreferencesOptions {
            clear_sort_filter: false,
            column_renames: &renames,
        },
    )
    .await?;
    Ok(())
}

/// ユーザー定義テーブルを別名で複製する。
pub async fn duplicate_user_table(
    pool: &SqlitePool,
    source: &str,
    target: &str,
    columns: &[TableColumnInput],
    include_data: bool,
) -> DomainResult<()> {
    let source = ensure_editable_user_table(source)?;
    let target = validate_user_object_name(target)?;

    if source == target {
        return Err(DomainError::Validation(
            "複製先のテーブル名は複製元と異なる必要があります".to_string(),
        ));
    }

    if !object_name_exists(pool, source, "table").await? {
        return Err(DomainError::NotFound);
    }

    if object_name_exists(pool, &target, "table").await? {
        return Err(DomainError::Conflict(format!(
            "テーブル `{target}` は既に存在します"
        )));
    }

    let definition = build_table_definition(columns)?;
    let quoted_target = quote_sql_identifier(&target);
    let quoted_source = quote_sql_identifier(source);
    let create_ddl = format!("CREATE TABLE {quoted_target} ({definition})");

    let mut tx = pool.begin().await?;
    sqlx::query(sqlx::AssertSqlSafe(create_ddl))
        .execute(&mut *tx)
        .await?;

    if include_data {
        let dest_names: HashSet<&str> = columns
            .iter()
            .filter(|column| !column.name.trim().is_empty())
            .map(|column| column.name.as_str())
            .collect();
        let copy_cols: Vec<String> = table_column_names(pool, source)
            .await?
            .into_iter()
            .filter(|name| name == "id" || dest_names.contains(name.as_str()))
            .collect();
        let quoted_cols: Vec<String> = copy_cols
            .iter()
            .map(|name| quote_sql_identifier(name))
            .collect();
        let insert_ddl = format!(
            "INSERT INTO {quoted_target} ({}) SELECT {} FROM {quoted_source}",
            quoted_cols.join(", "),
            quoted_cols.join(", ")
        );
        sqlx::query(sqlx::AssertSqlSafe(insert_ddl))
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn create_user_table_from_columns(
    pool: &SqlitePool,
    name: &str,
    columns: &[TableColumnInput],
) -> DomainResult<()> {
    let name = validate_user_object_name(name)?;
    let definition = build_table_definition(columns)?;

    if object_name_exists(pool, &name, "table").await? {
        return Err(DomainError::Conflict(format!(
            "テーブル `{name}` は既に存在します"
        )));
    }

    let ddl = format!(
        "CREATE TABLE {} ({definition})",
        quote_sql_identifier(&name)
    );
    execute_ddl(pool, &ddl).await?;
    Ok(())
}

fn column_sql_type(column: &TableColumnInput) -> DomainResult<&'static str> {
    match column.type_key.as_str() {
        "integer" => Ok("INTEGER"),
        "real" => Ok("REAL"),
        "text" => Ok("TEXT"),
        "blob" => Ok("BLOB"),
        "timestamp" => Ok("TIMESTAMP"),
        "boolean" => Ok("BOOLEAN"),
        _ => Err(DomainError::Validation(format!(
            "カラム `{}` の型が不正です",
            column.name
        ))),
    }
}

fn column_definition_sql(name: &str, column: &TableColumnInput) -> DomainResult<String> {
    let null_clause = if column.nullable {
        String::new()
    } else {
        " NOT NULL".to_string()
    };

    let sql_type = column_sql_type(column)?;
    Ok(format!(
        "{} {sql_type}{null_clause}",
        quote_sql_identifier(name)
    ))
}

/// ユーザー定義ビューを別名で複製する。
pub async fn duplicate_user_view(
    pool: &SqlitePool,
    source: &str,
    target: &str,
    definition: &str,
) -> DomainResult<()> {
    let source = ensure_editable_user_table(source)?;

    if source == target {
        return Err(DomainError::Validation(
            "複製先のビュー名は複製元と異なる必要があります".to_string(),
        ));
    }

    if !object_name_exists(pool, source, "view").await? {
        return Err(DomainError::NotFound);
    }

    create_user_view(pool, target, definition).await
}

pub async fn create_user_view(
    pool: &SqlitePool,
    name: &str,
    definition: &str,
) -> DomainResult<()> {
    let name = validate_user_object_name(name)?;
    let definition = validate_view_definition(definition)?;

    if object_name_exists(pool, &name, "view").await? {
        return Err(DomainError::Conflict(format!("ビュー `{name}` は既に存在します")));
    }
    if object_name_exists(pool, &name, "table").await? {
        return Err(DomainError::Conflict(format!(
            "同名のテーブル `{name}` が既に存在します"
        )));
    }

    execute_ddl(pool, &create_view_ddl(&name, &definition)).await?;
    Ok(())
}

pub async fn load_user_view_definition(pool: &SqlitePool, name: &str) -> DomainResult<String> {
    let name = ensure_editable_user_table(name)?;
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'view' AND name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    let sql = sql.ok_or(DomainError::NotFound)?;
    extract_view_select_from_create_sql(&sql)
}

pub async fn update_user_view(
    pool: &SqlitePool,
    old_name: &str,
    new_name: &str,
    definition: &str,
) -> DomainResult<()> {
    let old_name = ensure_editable_user_table(old_name)?;
    let new_name = validate_user_object_name(new_name)?;
    if !object_name_exists(pool, old_name, "view").await? {
        return Err(DomainError::NotFound);
    }

    let definition = validate_view_definition(definition)?;
    let old_definition = load_user_view_definition(pool, old_name).await?;
    let definition_changed = old_definition.trim() != definition.trim();
    let renaming = old_name != new_name;

    if renaming {
        if object_name_exists(pool, &new_name, "view").await? {
            return Err(DomainError::Conflict(format!(
                "ビュー `{new_name}` は既に存在します"
            )));
        }
        if object_name_exists(pool, &new_name, "table").await? {
            return Err(DomainError::Conflict(format!(
                "同名のテーブル `{new_name}` が既に存在します"
            )));
        }
    }

    let quoted_old = quote_sql_identifier(old_name);
    let mut tx = pool.begin().await?;

    if renaming {
        sqlx::query(sqlx::AssertSqlSafe(create_view_ddl(&new_name, &definition)))
            .execute(&mut *tx)
            .await?;
        migrate_user_table_meta_key_in_tx(&mut tx, old_name, &new_name).await?;
        let drop_ddl = format!("DROP VIEW IF EXISTS {quoted_old}");
        sqlx::query(sqlx::AssertSqlSafe(drop_ddl))
            .execute(&mut *tx)
            .await?;
        if definition_changed {
            sanitize_data_ui_preferences_in_tx(
                &mut tx,
                &new_name,
                SanitizeUiPreferencesOptions {
                    clear_sort_filter: true,
                    column_renames: &[],
                },
            )
            .await?;
        }
    } else {
        let drop_ddl = format!("DROP VIEW IF EXISTS {quoted_old}");
        sqlx::query(sqlx::AssertSqlSafe(drop_ddl))
            .execute(&mut *tx)
            .await?;
        sqlx::query(sqlx::AssertSqlSafe(create_view_ddl(old_name, &definition)))
            .execute(&mut *tx)
            .await?;
        sanitize_data_ui_preferences_in_tx(
            &mut tx,
            old_name,
            SanitizeUiPreferencesOptions {
                clear_sort_filter: true,
                column_renames: &[],
            },
        )
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

fn create_view_ddl(name: &str, definition: &str) -> String {
    format!(
        "CREATE VIEW {} AS {definition}",
        quote_sql_identifier(name)
    )
}

fn extract_view_select_from_create_sql(sql: &str) -> DomainResult<String> {
    let upper = sql.to_ascii_uppercase();
    let marker = " AS ";
    let pos = upper
        .find(marker)
        .ok_or(DomainError::NotFound)?;
    let definition = sql[pos + marker.len()..].trim();
    if definition.is_empty() {
        return Err(DomainError::NotFound);
    }
    Ok(definition.to_string())
}

fn validate_view_definition(definition: &str) -> DomainResult<String> {
    let definition = definition.trim();
    if definition.is_empty() {
        return Err(DomainError::Validation("SELECT 文は必須です".into()));
    }
    let stripped = strip_sql_comments(definition);
    let without_comments = stripped.trim();
    if !without_comments.to_ascii_uppercase().starts_with("SELECT") {
        return Err(DomainError::Validation(
            "ビュー定義は SELECT で始めてください".into(),
        ));
    }
    validate_ddl_fragment(definition, "SELECT 文")?;
    Ok(definition.to_string())
}

/// ビュー UI ビルダーで選択可能な元テーブル名の一覧。
pub async fn list_view_source_tables(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let tables = list_tables(pool).await?;
    Ok(tables
        .into_iter()
        .filter(|item| is_db_admin_data_viewable(&item.name))
        .map(|item| item.name)
        .collect())
}

/// ビュー UI ビルダー向けにテーブルのカラム一覧を返す。
pub async fn table_columns_for_ui(
    pool: &SqlitePool,
    table_name: &str,
) -> DomainResult<Vec<TableColumnUiInfo>> {
    if !is_db_admin_data_viewable(table_name) {
        return Err(DomainError::SystemTable(infra_table_denied_message(table_name)));
    }
    if !object_name_exists(pool, table_name, "table").await? {
        return Err(DomainError::NotFound);
    }

    let rows = table_pragma_info(pool, table_name).await?;
    rows.iter()
        .map(|row| {
            let type_key = pragma_type_to_type_key(&row.type_name).ok_or_else(|| {
                DomainError::Validation(format!(
                    "カラム `{}` の型を判別できません",
                    row.name
                ))
            })?;
            Ok(TableColumnUiInfo {
                name: row.name.clone(),
                type_key: type_key.to_string(),
                pk: row.pk > 0,
                nullable: row.notnull == 0,
            })
        })
        .collect()
}

/// ビュー定義 SQL を UI ビルダー状態へ展開する。
pub async fn resolve_view_ui_spec(
    pool: &SqlitePool,
    definition: &str,
) -> ViewUiSpecResolveResult {
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return ViewUiSpecResolveResult {
            supported: true,
            spec: None,
            table_columns: None,
        };
    }

    let Some(parsed) = parse_simple_view_select(trimmed) else {
        return ViewUiSpecResolveResult {
            supported: false,
            spec: None,
            table_columns: None,
        };
    };

    let Ok(table_columns) = table_columns_for_ui(pool, &parsed.base_table).await else {
        return ViewUiSpecResolveResult {
            supported: false,
            spec: None,
            table_columns: None,
        };
    };

    view_ui_spec_resolve_success(build_simple_view_ui_spec(&parsed, &table_columns), table_columns)
}

/// 単純な `SELECT ... FROM table` 形式のビュー定義を UI 状態へ復元する。
pub(crate) fn parse_simple_view_select(definition: &str) -> Option<ParsedSimpleViewSelect> {
    let stripped = strip_sql_comments(definition.trim());
    let trimmed = stripped.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("SELECT") {
        return None;
    }

    let from_pos = find_sql_keyword(trimmed, "FROM", "SELECT".len())?;
    let mut select_part = trimmed["SELECT".len()..from_pos].trim();
    let after_from = trimmed[from_pos + "FROM".len()..].trim();
    if after_from.is_empty() {
        return None;
    }

    let distinct = if find_sql_keyword(select_part, "DISTINCT", 0) == Some(0) {
        select_part = select_part["DISTINCT".len()..].trim();
        true
    } else if find_sql_keyword(select_part, "ALL", 0) == Some(0) {
        select_part = select_part["ALL".len()..].trim();
        false
    } else {
        false
    };

    let from_clause = parse_from_clause(after_from)?;
    let where_conditions = parse_optional_where_conditions(&from_clause.remainder, &from_clause.base_table, from_clause.table_alias.as_deref())?;
    let columns = parse_select_column_list(
        select_part,
        &from_clause.base_table,
        from_clause.table_alias.as_deref(),
    )?;

    Some(ParsedSimpleViewSelect {
        base_table: from_clause.base_table,
        distinct,
        columns,
        where_conditions,
    })
}

/// 解析結果とテーブル定義からビュー UI ビルダーの初期状態を組み立てる。
pub(crate) fn build_simple_view_ui_spec(
    parsed: &ParsedSimpleViewSelect,
    table_columns: &[TableColumnUiInfo],
) -> SimpleViewUiSpec {
    let type_by_name: std::collections::HashMap<_, _> = table_columns
        .iter()
        .map(|column| (column.name.as_str(), column.type_key.as_str()))
        .collect();

    let ordered_columns: Vec<ParsedViewColumn> = match &parsed.columns {
        ParsedViewColumnList::All => table_columns
            .iter()
            .map(|column| ParsedViewColumn {
                name: column.name.clone(),
                alias: None,
                expression: None,
            })
            .collect(),
        ParsedViewColumnList::Columns(selected) => selected.clone(),
    };

    let (column_where, extra_where) =
        partition_where_conditions(&ordered_columns, &parsed.where_conditions);

    let columns = ordered_columns
        .into_iter()
        .zip(column_where)
        .map(|(column, where_condition)| SimpleViewUiColumn {
            type_key: type_by_name
                .get(column.name.as_str())
                .map(|value| (*value).to_string())
                .unwrap_or_else(|| "text".to_string()),
            where_condition,
            alias: column.alias,
            expression: column.expression,
            name: column.name,
        })
        .collect();

    SimpleViewUiSpec {
        base_table: parsed.base_table.clone(),
        distinct: parsed.distinct,
        columns,
        extra_where: extra_where
            .into_iter()
            .map(|condition| ExtraWhereCondition {
                column: condition.column,
                suffix: condition.suffix,
            })
            .collect(),
    }
}

fn view_ui_spec_resolve_success(
    spec: SimpleViewUiSpec,
    table_columns: Vec<TableColumnUiInfo>,
) -> ViewUiSpecResolveResult {
    ViewUiSpecResolveResult {
        supported: true,
        spec: Some(spec),
        table_columns: Some(table_columns),
    }
}

/// ビュー UI ビルダーの状態から単純な SELECT 文を生成する。
pub fn build_simple_view_select(spec: &SimpleViewUiSpec) -> DomainResult<String> {
    if spec.columns.is_empty() {
        return Err(DomainError::Validation(
            "ビューに含めるカラムを1つ以上選択してください".into(),
        ));
    }

    validate_view_output_column_names(&spec.columns)?;

    let column_sql = spec
        .columns
        .iter()
        .map(format_simple_view_select_column)
        .collect::<Vec<_>>()
        .join(", ");

    let distinct_kw = if spec.distinct { "DISTINCT " } else { "" };
    let where_sql = build_simple_view_where_clause(&spec.columns, &spec.extra_where);
    Ok(format!(
        "SELECT {distinct_kw}{column_sql} FROM {}{where_sql}",
        quote_sql_identifier(&spec.base_table)
    ))
}

fn effective_view_column_name(column: &SimpleViewUiColumn) -> String {
    column
        .alias
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            column
                .expression
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| column.name.clone())
}

fn format_simple_view_select_column(column: &SimpleViewUiColumn) -> String {
    let select_part = column
        .expression
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| quote_sql_identifier(&column.name));
    let alias = column
        .alias
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    match alias {
        Some(alias) => format!("{select_part} AS {}", quote_sql_identifier(alias)),
        None => select_part,
    }
}

fn validate_view_column_alias(alias: &str) -> DomainResult<()> {
    validate_identifier_chars(alias.trim(), "別名")
}

fn validate_view_output_column_names(columns: &[SimpleViewUiColumn]) -> DomainResult<()> {
    let mut seen = std::collections::HashSet::new();
    for column in columns {
        if let Some(alias) = column
            .alias
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            validate_view_column_alias(alias)?;
        }
        if let Some(expression) = column
            .expression
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            validate_ddl_fragment(expression, "式")?;
        }
        let output_name = effective_view_column_name(column);
        if !seen.insert(output_name.clone()) {
            return Err(DomainError::Validation(
                "出力列名が重複しています。別名で区別してください".into(),
            ));
        }
    }
    Ok(())
}

fn select_output_name(column: &ParsedViewColumn) -> String {
    column
        .alias
        .as_ref()
        .cloned()
        .or_else(|| column.expression.clone())
        .unwrap_or_else(|| column.name.clone())
}

fn select_output_names_unique(columns: &[ParsedViewColumn]) -> bool {
    let mut seen = std::collections::HashSet::new();
    columns
        .iter()
        .map(select_output_name)
        .all(|name| seen.insert(name))
}

fn partition_where_conditions(
    columns: &[ParsedViewColumn],
    where_conditions: &[ParsedWhereCondition],
) -> (Vec<Option<String>>, Vec<ParsedWhereCondition>) {
    let mut used = vec![false; where_conditions.len()];
    let column_where = columns
        .iter()
        .map(|column| {
            for (index, condition) in where_conditions.iter().enumerate() {
                if !used[index] && condition.column == column.name {
                    used[index] = true;
                    return Some(condition.suffix.clone());
                }
            }
            None
        })
        .collect();

    let extra_where = where_conditions
        .iter()
        .enumerate()
        .filter_map(|(index, condition)| {
            if used[index] {
                None
            } else {
                Some(condition.clone())
            }
        })
        .collect();

    (column_where, extra_where)
}

fn build_simple_view_where_clause(
    columns: &[SimpleViewUiColumn],
    extra_where: &[ExtraWhereCondition],
) -> String {
    let mut conditions: Vec<String> = columns
        .iter()
        .filter_map(|column| {
            if column.expression.is_some() {
                return None;
            }
            column
                .where_condition
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|suffix| format!("{} {suffix}", quote_sql_identifier(&column.name)))
        })
        .collect();

    for condition in extra_where {
        let suffix = condition.suffix.trim();
        if suffix.is_empty() {
            continue;
        }
        conditions.push(format!(
            "{} {suffix}",
            quote_sql_identifier(&condition.column)
        ));
    }

    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

#[derive(Debug, Clone)]
struct ParsedFromClause<'a> {
    base_table: String,
    table_alias: Option<String>,
    remainder: &'a str,
}

fn parse_from_clause(input: &str) -> Option<ParsedFromClause<'_>> {
    let (base_table, mut rest) = parse_table_reference(input)?;
    rest = rest.trim();

    let table_alias = if find_sql_keyword(rest, "AS", 0) == Some(0) {
        let after_as = rest["AS".len()..].trim();
        let (alias, after_alias) = parse_sql_identifier_prefix(after_as)?;
        rest = after_alias.trim();
        Some(alias)
    } else if let Some((candidate, after_candidate)) = parse_sql_identifier_prefix(rest) {
        if !is_sql_clause_keyword(&candidate) {
            let after_trimmed = after_candidate.trim();
            if after_trimmed.is_empty() || find_sql_keyword(after_trimmed, "WHERE", 0) == Some(0) {
                rest = after_trimmed;
                Some(candidate)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if contains_unsupported_trailing_clause(rest) {
        return None;
    }

    Some(ParsedFromClause {
        base_table,
        table_alias,
        remainder: rest,
    })
}

fn parse_table_reference(input: &str) -> Option<(String, &str)> {
    let (first, rest) = parse_sql_identifier_prefix(input)?;
    let rest = rest.trim();
    if rest.starts_with('.') {
        let (table, after_table) = parse_sql_identifier_prefix(rest[1..].trim())?;
        return Some((table, after_table));
    }
    Some((first, rest))
}

fn is_sql_clause_keyword(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "WHERE" | "JOIN" | "GROUP" | "HAVING" | "ORDER" | "LIMIT" | "UNION" | "EXCEPT" | "INTERSECT" | "ON"
    )
}

fn qualifier_matches(qualifier: &str, base_table: &str, table_alias: Option<&str>) -> bool {
    qualifier == base_table
        || table_alias.is_some_and(|alias| qualifier == alias)
}

fn parse_select_column_list(
    select_part: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedViewColumnList> {
    if select_part == "*" || is_matching_qualified_star(select_part, base_table, table_alias) {
        return Some(ParsedViewColumnList::All);
    }

    let items = parse_comma_separated_select_items(select_part, base_table, table_alias)?;
    if items.is_empty() {
        return None;
    }
    if !select_output_names_unique(&items) {
        return None;
    }
    Some(ParsedViewColumnList::Columns(items))
}

fn is_matching_qualified_star(
    expr: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> bool {
    let expr = expr.trim();
    if !expr.ends_with(".*") {
        return false;
    }
    let qualifier = expr[..expr.len() - 2].trim();
    if qualifier.is_empty() {
        return false;
    }
    match parse_sql_identifier_prefix(qualifier) {
        Some((name, rest)) if rest.trim().is_empty() => {
            qualifier_matches(&name, base_table, table_alias)
        }
        _ => false,
    }
}

fn parse_dotted_identifier(input: &str) -> Option<(String, String)> {
    let (qualifier, rest) = parse_sql_identifier_prefix(input)?;
    let rest = rest.trim();
    if !rest.starts_with('.') {
        return None;
    }
    let (name, after_name) = parse_sql_identifier_prefix(rest[1..].trim())?;
    if !after_name.trim().is_empty() {
        return None;
    }
    Some((qualifier, name))
}

fn parse_qualified_column_name(
    expr: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<String> {
    let (qualifier, column) = parse_dotted_identifier(expr)?;
    if qualifier_matches(&qualifier, base_table, table_alias) {
        Some(column)
    } else {
        None
    }
}

/// SQL の `--` / `/* */` コメントを除去する。文字列・識別子リテラル内は保護する。
fn strip_sql_comments(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let bytes = sql.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        let ch = sql[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if in_single_quote {
            result.push(ch);
            if ch == '\'' {
                if bytes.get(i + ch_len) == Some(&b'\'') {
                    result.push('\'');
                    i += ch_len * 2;
                    continue;
                }
                in_single_quote = false;
            }
            i += ch_len;
            continue;
        }

        if in_double_quote {
            result.push(ch);
            if ch == '"' {
                if bytes.get(i + ch_len) == Some(&b'"') {
                    result.push('"');
                    i += ch_len * 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += ch_len;
            continue;
        }

        if ch == '\'' {
            in_single_quote = true;
            result.push(ch);
            i += ch_len;
            continue;
        }

        if ch == '"' {
            in_double_quote = true;
            result.push(ch);
            i += ch_len;
            continue;
        }

        if ch == '-' && bytes.get(i + ch_len) == Some(&b'-') {
            i += ch_len * 2;
            while i < bytes.len() {
                let c = sql[i..].chars().next().unwrap();
                if c == '\n' || c == '\r' {
                    result.push(' ');
                    i += c.len_utf8();
                    break;
                }
                i += c.len_utf8();
            }
            continue;
        }

        if ch == '/' && bytes.get(i + ch_len) == Some(&b'*') {
            i += ch_len * 2;
            while i < bytes.len() {
                let c = sql[i..].chars().next().unwrap();
                let c_len = c.len_utf8();
                if c == '*' && bytes.get(i + c_len) == Some(&b'/') {
                    i += c_len + 1;
                    result.push(' ');
                    break;
                }
                if c == '\n' || c == '\r' {
                    result.push(' ');
                }
                i += c_len;
            }
            continue;
        }

        result.push(ch);
        i += ch_len;
    }

    result
}

fn find_sql_keyword(sql: &str, keyword: &str, start: usize) -> Option<usize> {
    let keyword_bytes = keyword.as_bytes();
    let keyword_len = keyword_bytes.len();
    if keyword_len == 0 {
        return None;
    }

    let sql_bytes = sql.as_bytes();
    if start >= sql_bytes.len() {
        return None;
    }

    let mut pos = start;
    while pos + keyword_len <= sql_bytes.len() {
        if sql_bytes[pos..pos + keyword_len]
            .iter()
            .zip(keyword_bytes.iter())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
        {
            let before_ok = pos == 0 || !is_identifier_char(sql_bytes[pos - 1]);
            let after_pos = pos + keyword_len;
            let after_ok =
                after_pos >= sql_bytes.len() || !is_identifier_char(sql_bytes[after_pos]);
            if before_ok && after_ok {
                return Some(pos);
            }
        }
        pos += 1;
    }

    None
}

fn is_identifier_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn contains_unsupported_trailing_clause(remainder: &str) -> bool {
    const CLAUSES: &[&str] = &[
        "JOIN", "GROUP", "HAVING", "ORDER", "LIMIT", "UNION", "EXCEPT", "INTERSECT",
    ];

    CLAUSES.iter().any(|clause| find_sql_keyword(remainder, clause, 0).is_some())
}

fn parse_optional_where_conditions(
    after_table: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<Vec<ParsedWhereCondition>> {
    if after_table.is_empty() {
        return Some(Vec::new());
    }

    let where_pos = find_sql_keyword(after_table, "WHERE", 0)?;
    if where_pos != 0 {
        return None;
    }

    let where_part = after_table[where_pos + "WHERE".len()..].trim();
    if where_part.is_empty() {
        return None;
    }

    parse_simple_where_conditions(where_part, base_table, table_alias)
}

fn parse_simple_where_conditions(
    where_part: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<Vec<ParsedWhereCondition>> {
    if contains_top_level_keyword(where_part, "OR") {
        return None;
    }

    let parts = split_top_level_and(where_part)?;
    let mut conditions = Vec::with_capacity(parts.len());

    for part in parts {
        let condition = parse_column_where_condition(&part, base_table, table_alias)?;
        conditions.push(condition);
    }

    Some(conditions)
}

fn find_top_level_keyword(input: &str, keyword: &str) -> Option<usize> {
    let mut in_single_quote = false;
    let mut i = 0usize;

    while i < input.len() {
        let ch = input[i..].chars().next()?;
        let ch_len = ch.len_utf8();
        if ch == '\'' {
            in_single_quote = !in_single_quote;
            i += ch_len;
            continue;
        }

        if !in_single_quote && find_sql_keyword(&input[i..], keyword, 0) == Some(0) {
            return Some(i);
        }

        i += ch_len;
    }

    None
}

fn split_top_level_and(input: &str) -> Option<Vec<String>> {
    if has_unclosed_string_literal(input) {
        return None;
    }

    let mut parts = Vec::new();
    let mut segment_start = 0usize;
    let mut i = 0usize;

    while let Some(and_pos) = find_top_level_keyword(&input[i..], "AND") {
        let split_at = i + and_pos;
        let segment = input[segment_start..split_at].trim();
        if segment.is_empty() {
            return None;
        }
        parts.push(segment.to_string());
        i = split_at + "AND".len();
        while let Some(next) = input[i..].chars().next() {
            if !next.is_whitespace() {
                break;
            }
            i += next.len_utf8();
        }
        segment_start = i;
    }

    let last = input[segment_start..].trim();
    if last.is_empty() {
        return if parts.is_empty() { None } else { Some(parts) };
    }
    parts.push(last.to_string());
    Some(parts)
}

fn parse_column_where_condition(
    part: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedWhereCondition> {
    let trimmed = part.trim();
    if let Some((qualifier, rest)) = parse_sql_identifier_prefix(trimmed) {
        let rest = rest.trim();
        if rest.starts_with('.') {
            let after_dot = rest[1..].trim();
            if let Some((column, suffix_rest)) = parse_sql_identifier_prefix(after_dot) {
                if qualifier_matches(&qualifier, base_table, table_alias) {
                    let suffix = suffix_rest.trim();
                    if suffix.is_empty() {
                        return None;
                    }
                    return Some(ParsedWhereCondition {
                        column,
                        suffix: suffix.to_string(),
                    });
                }
                return None;
            }
        }
    }

    let (column, rest) = parse_sql_identifier_prefix(trimmed)?;
    let suffix = rest.trim();
    if suffix.is_empty() {
        return None;
    }
    Some(ParsedWhereCondition {
        column,
        suffix: suffix.to_string(),
    })
}

fn contains_top_level_keyword(input: &str, keyword: &str) -> bool {
    find_top_level_keyword(input, keyword).is_some()
}

fn has_unclosed_string_literal(input: &str) -> bool {
    let mut in_single_quote = false;
    for ch in input.chars() {
        if ch == '\'' {
            in_single_quote = !in_single_quote;
        }
    }
    in_single_quote
}

fn parse_comma_separated_select_items(
    input: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<Vec<ParsedViewColumn>> {
    let parts = split_top_level_commas(input)?;
    parts
        .iter()
        .map(|part| parse_single_select_item(part, base_table, table_alias))
        .collect()
}

fn parse_single_select_item(
    item: &str,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedViewColumn> {
    let trimmed = item.trim();
    if trimmed == "*" || is_matching_qualified_star(trimmed, base_table, table_alias) {
        return None;
    }

    let (expr_part, alias) = split_select_item_alias(trimmed)?;
    if expr_part.is_empty() {
        return None;
    }

    if let Some(column_name) = parse_qualified_column_name(&expr_part, base_table, table_alias) {
        return Some(ParsedViewColumn {
            name: column_name,
            alias,
            expression: None,
        });
    }

    if let Some((name, rest)) = parse_sql_identifier_prefix(&expr_part) {
        if rest.trim().is_empty() {
            return Some(ParsedViewColumn {
                name,
                alias,
                expression: None,
            });
        }
    }

    Some(ParsedViewColumn {
        name: String::new(),
        alias,
        expression: Some(expr_part),
    })
}

fn split_select_item_alias(item: &str) -> Option<(String, Option<String>)> {
    if let Some(as_pos) = find_top_level_keyword_with_parens(item, "AS") {
        let expr_part = item[..as_pos].trim();
        let after_as = item[as_pos + "AS".len()..].trim();
        let (alias_name, rest) = parse_sql_identifier_prefix(after_as)?;
        if !rest.trim().is_empty() {
            return None;
        }
        return Some((expr_part.to_string(), Some(alias_name)));
    }

    if let Some((expr_part, alias)) = split_trailing_implicit_alias(item) {
        return Some((expr_part, Some(alias)));
    }

    Some((item.to_string(), None))
}

fn split_trailing_implicit_alias(item: &str) -> Option<(String, String)> {
    let mut paren_depth = 0i32;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut last_space: Option<usize> = None;
    let mut i = 0usize;

    while i < item.len() {
        let ch = item[i..].chars().next()?;
        let ch_len = ch.len_utf8();

        if in_single_quote {
            if ch == '\'' {
                in_single_quote = false;
            }
            i += ch_len;
            continue;
        }

        if in_double_quote {
            if ch == '"' {
                let next_byte = item.as_bytes().get(i + ch_len).copied();
                if next_byte == Some(b'"') {
                    i += ch_len * 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += ch_len;
            continue;
        }

        match ch {
            '\'' => in_single_quote = true,
            '"' => in_double_quote = true,
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            ch if ch.is_whitespace() && paren_depth == 0 => last_space = Some(i),
            _ => {}
        }

        i += ch_len;
    }

    let split_at = last_space?;
    let expr_part = item[..split_at].trim();
    let alias_part = item[split_at..].trim();
    if expr_part.is_empty() || alias_part.is_empty() {
        return None;
    }
    let (alias, rest) = parse_sql_identifier_prefix(alias_part)?;
    if !rest.trim().is_empty() || is_sql_clause_keyword(&alias) {
        return None;
    }
    Some((expr_part.to_string(), alias))
}

fn split_top_level_commas(input: &str) -> Option<Vec<String>> {
    if has_unclosed_string_literal(input) {
        return None;
    }

    let mut parts = Vec::new();
    let mut segment_start = 0usize;
    let mut paren_depth = 0i32;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut i = 0usize;

    while i < input.len() {
        let ch = input[i..].chars().next()?;
        let ch_len = ch.len_utf8();

        if in_single_quote {
            if ch == '\'' {
                in_single_quote = false;
            }
            i += ch_len;
            continue;
        }

        if in_double_quote {
            if ch == '"' {
                let next_byte = input.as_bytes().get(i + ch_len).copied();
                if next_byte == Some(b'"') {
                    i += ch_len * 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += ch_len;
            continue;
        }

        match ch {
            '\'' => in_single_quote = true,
            '"' => in_double_quote = true,
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            ',' if paren_depth == 0 => {
                let segment = input[segment_start..i].trim();
                if segment.is_empty() {
                    return None;
                }
                parts.push(segment.to_string());
                segment_start = i + ch_len;
            }
            _ => {}
        }

        i += ch_len;
    }

    let last = input[segment_start..].trim();
    if last.is_empty() {
        return if parts.is_empty() { None } else { Some(parts) };
    }
    parts.push(last.to_string());
    Some(parts)
}

fn find_top_level_keyword_with_parens(input: &str, keyword: &str) -> Option<usize> {
    let mut paren_depth = 0i32;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut i = 0usize;

    while i < input.len() {
        let ch = input[i..].chars().next()?;
        let ch_len = ch.len_utf8();

        if in_single_quote {
            if ch == '\'' {
                in_single_quote = false;
            }
            i += ch_len;
            continue;
        }

        if in_double_quote {
            if ch == '"' {
                let next_byte = input.as_bytes().get(i + ch_len).copied();
                if next_byte == Some(b'"') {
                    i += ch_len * 2;
                    continue;
                }
                in_double_quote = false;
            }
            i += ch_len;
            continue;
        }

        match ch {
            '\'' => in_single_quote = true,
            '"' => in_double_quote = true,
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            _ => {
                if paren_depth == 0 && find_sql_keyword(&input[i..], keyword, 0) == Some(0) {
                    return Some(i);
                }
            }
        }

        i += ch_len;
    }

    None
}

fn parse_sql_identifier_prefix(input: &str) -> Option<(String, &str)> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if input.starts_with('"') {
        let mut escaped = false;
        let mut end = 1usize;
        let chars: Vec<char> = input.chars().collect();
        while end < chars.len() {
            let ch = chars[end];
            if escaped {
                escaped = false;
            } else if ch == '"' {
                if end + 1 < chars.len() && chars[end + 1] == '"' {
                    escaped = true;
                } else {
                    let byte_len: usize = chars[..=end].iter().map(|c| c.len_utf8()).sum();
                    let name = unquote_sql_identifier(&input[..byte_len])?;
                    return Some((name, &input[byte_len..]));
                }
            }
            end += 1;
        }
        return None;
    }

    let end = input
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    if end == 0 {
        return None;
    }

    Some((input[..end].to_string(), &input[end..]))
}

fn unquote_sql_identifier(input: &str) -> Option<String> {
    if !input.starts_with('"') || !input.ends_with('"') || input.len() < 2 {
        return None;
    }
    Some(input[1..input.len() - 1].replace("\"\"", "\""))
}

fn validate_ddl_fragment(fragment: &str, label: &str) -> DomainResult<()> {
    if fragment.contains(';') {
        return Err(DomainError::Validation(format!(
            "{label}にセミコロンは使用できません"
        )));
    }
    if contains_forbidden_keyword(&strip_sql_comments(fragment)) {
        return Err(DomainError::Validation(format!(
            "{label}に使用できない SQL キーワードが含まれています"
        )));
    }
    Ok(())
}

fn contains_forbidden_keyword(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    upper
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|word| !word.is_empty())
        .any(|word| FORBIDDEN_DDL_KEYWORDS.contains(&word))
}

async fn object_name_exists(
    pool: &SqlitePool,
    name: &str,
    object_type: &str,
) -> DomainResult<bool> {
    let exists = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM sqlite_master WHERE type = ? AND name = ? LIMIT 1",
    )
    .bind(object_type)
    .bind(name)
    .fetch_optional(pool)
    .await?
    .is_some();
    Ok(exists)
}

async fn ensure_named_object_viewable(
    pool: &SqlitePool,
    name: &str,
    object_type: &str,
) -> DomainResult<()> {
    let name = ensure_viewable_table(name)?;
    if !object_name_exists(pool, name, object_type).await? {
        return Err(DomainError::NotFound);
    }
    Ok(())
}

async fn object_exists_as_table_or_view(pool: &SqlitePool, name: &str) -> DomainResult<bool> {
    let exists = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM sqlite_master WHERE name = ? AND type IN ('table', 'view') LIMIT 1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .is_some();
    Ok(exists)
}

/// 保存済みの列幅を取得する。未保存の場合は `None`。
pub async fn get_table_column_widths(
    pool: &SqlitePool,
    name: &str,
) -> DomainResult<Option<std::collections::HashMap<String, i32>>> {
    Ok(get_table_ui_meta(pool, name).await?.column_widths)
}

/// 列幅を保存する（UPSERT）。
pub async fn save_table_column_widths(
    pool: &SqlitePool,
    name: &str,
    widths: &std::collections::HashMap<String, i32>,
) -> DomainResult<()> {
    let name = ensure_viewable_table(name)?;
    if !object_exists_as_table_or_view(pool, name).await? {
        return Err(DomainError::NotFound);
    }

    let column_names = table_column_names(pool, name).await?;
    let column_set: std::collections::HashSet<&str> =
        column_names.iter().map(String::as_str).collect();

    let pruned_widths: std::collections::HashMap<String, i32> = widths
        .iter()
        .filter(|(col, _)| column_set.contains(col.as_str()))
        .filter_map(|(col, width)| {
            if *width < COLUMN_WIDTH_MIN_PX || *width > COLUMN_WIDTH_MAX_PX {
                return None;
            }
            Some(((*col).clone(), *width))
        })
        .collect();

    let json = serde_json::to_string(&pruned_widths).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("列幅 JSON のシリアライズに失敗: {e}"))
    })?;

    sqlx::query(
        r#"
        INSERT INTO user_table_meta (table_name, column_widths_json, updated_at)
        VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            column_widths_json = excluded.column_widths_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(name)
    .bind(json)
    .execute(pool)
    .await?;

    Ok(())
}

/// `col:value,col2:value2` 形式のクエリ断片を列名と値に分割する。
fn split_column_value_query_parts(
    raw: &str,
    kind_label: &str,
) -> DomainResult<Vec<(String, String)>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let mut parts = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (column, value) = part.split_once(':').ok_or_else(|| {
            DomainError::Validation(format!("{kind_label}指定の形式が不正です: `{part}`"))
        })?;
        let column = column.trim();
        if column.is_empty() {
            return Err(DomainError::Validation(format!(
                "{kind_label}指定に列名が含まれていません"
            )));
        }
        parts.push((column.to_string(), value.to_string()));
    }

    Ok(parts)
}

/// クエリ文字列 `name:asc,age:desc` をパースする。
pub fn parse_sort_query_param(raw: &str) -> DomainResult<Vec<TableSortEntry>> {
    split_column_value_query_parts(raw, "ソート")?
        .into_iter()
        .map(|(column, direction)| {
            let direction = match direction.trim().to_ascii_lowercase().as_str() {
                "asc" => TableSortDirection::Asc,
                "desc" => TableSortDirection::Desc,
                other => {
                    return Err(DomainError::Validation(format!(
                        "ソート方向は asc または desc で指定してください: `{other}`"
                    )));
                }
            };
            Ok(TableSortEntry { column, direction })
        })
        .collect()
}

/// クエリ文字列 `name:hello,age:42` をパースする（値は URL エンコード可）。
pub fn parse_filter_query_param(raw: &str) -> DomainResult<Vec<TableFilterEntry>> {
    Ok(split_column_value_query_parts(raw, "フィルター")?
        .into_iter()
        .map(|(column, text)| TableFilterEntry {
            column: percent_decode_query_component(&column),
            text: percent_decode_query_component(&text),
        })
        .collect())
}

fn compact_filter_entries(entries: &[TableFilterEntry]) -> Vec<TableFilterEntry> {
    entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty())
        .cloned()
        .collect()
}

fn percent_decode_query_component(raw: &str) -> String {
    urlencoding::decode(raw)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| raw.to_string())
}

fn escape_like_pattern(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            other => escaped.push(other),
        }
    }
    escaped
}

fn is_missing_user_table_meta_error(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db_err)
            if db_err.message().contains("no such table: user_table_meta")
    )
}

/// 保存済みの列ソートを取得する。未保存の場合は `None`。
pub async fn get_table_sort(
    pool: &SqlitePool,
    name: &str,
) -> DomainResult<Option<Vec<TableSortEntry>>> {
    Ok(get_table_ui_meta(pool, name).await?.sort)
}

/// 列ソートを保存する（UPSERT）。
pub async fn save_table_sort(
    pool: &SqlitePool,
    name: &str,
    sort: &[TableSortEntry],
) -> DomainResult<()> {
    let name = ensure_viewable_table(name)?;
    if !object_exists_as_table_or_view(pool, name).await? {
        return Err(DomainError::NotFound);
    }

    let column_names = table_column_names(pool, name).await?;
    let sort = filter_sort_entries_to_columns(&column_names, sort);

    let json = serde_json::to_string(&sort).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("ソート JSON のシリアライズに失敗: {e}"))
    })?;

    sqlx::query(
        r#"
        INSERT INTO user_table_meta (table_name, sort_json, updated_at)
        VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            sort_json = excluded.sort_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(name)
    .bind(json)
    .execute(pool)
    .await?;

    Ok(())
}

/// 保存済みの列フィルターを取得する。未保存の場合は `None`。
pub async fn get_table_filter(
    pool: &SqlitePool,
    name: &str,
) -> DomainResult<Option<Vec<TableFilterEntry>>> {
    Ok(get_table_ui_meta(pool, name).await?.filter)
}

/// 列フィルターを保存する（UPSERT）。
pub async fn save_table_filter(
    pool: &SqlitePool,
    name: &str,
    filter: &[TableFilterEntry],
) -> DomainResult<()> {
    let name = ensure_viewable_table(name)?;
    if !object_exists_as_table_or_view(pool, name).await? {
        return Err(DomainError::NotFound);
    }

    let column_names = table_column_names(pool, name).await?;
    let filter = filter_filter_entries_to_columns(
        &column_names,
        &compact_filter_entries(filter),
    );

    let json = serde_json::to_string(&filter).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("フィルター JSON のシリアライズに失敗: {e}"))
    })?;

    sqlx::query(
        r#"
        INSERT INTO user_table_meta (table_name, filter_json, updated_at)
        VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            filter_json = excluded.filter_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(name)
    .bind(json)
    .execute(pool)
    .await?;

    Ok(())
}

fn filter_sort_entries_to_columns(
    column_names: &[String],
    entries: &[TableSortEntry],
) -> Vec<TableSortEntry> {
    if entries.is_empty() {
        return Vec::new();
    }

    let column_set: std::collections::HashSet<&str> =
        column_names.iter().map(String::as_str).collect();

    entries
        .iter()
        .filter(|entry| column_set.contains(entry.column.as_str()))
        .cloned()
        .collect()
}

fn filter_filter_entries_to_columns(
    column_names: &[String],
    entries: &[TableFilterEntry],
) -> Vec<TableFilterEntry> {
    if entries.is_empty() {
        return Vec::new();
    }

    let column_set: std::collections::HashSet<&str> =
        column_names.iter().map(String::as_str).collect();

    entries
        .iter()
        .filter(|entry| column_set.contains(entry.column.as_str()))
        .cloned()
        .collect()
}

fn prune_column_widths(
    column_names: &[String],
    widths: Option<std::collections::HashMap<String, i32>>,
) -> Option<std::collections::HashMap<String, i32>> {
    let widths = widths?;
    let column_set: std::collections::HashSet<&str> =
        column_names.iter().map(String::as_str).collect();
    let pruned: std::collections::HashMap<String, i32> = widths
        .into_iter()
        .filter(|(name, _)| column_set.contains(name.as_str()))
        .collect();
    if pruned.is_empty() {
        None
    } else {
        Some(pruned)
    }
}

#[derive(Debug, Default)]
struct SanitizeUiPreferencesOptions<'a> {
    clear_sort_filter: bool,
    column_renames: &'a [(String, String)],
}

#[derive(Debug)]
struct SanitizedUiPreferencesValues {
    sort: Vec<TableSortEntry>,
    filter: Vec<TableFilterEntry>,
    column_widths: Option<std::collections::HashMap<String, i32>>,
}

#[derive(Debug)]
struct SerializedTableUiMeta {
    sort_json: String,
    filter_json: String,
    widths_json: String,
}

fn decode_table_ui_meta_row(row: Option<(String, String, String)>) -> DomainResult<TableUiMeta> {
    let Some((widths_json, sort_json, filter_json)) = row else {
        return Ok(TableUiMeta::default());
    };

    let column_widths = if widths_json.is_empty() || widths_json == "{}" {
        None
    } else {
        let parsed: std::collections::HashMap<String, i32> =
            serde_json::from_str(&widths_json).map_err(|e| {
                DomainError::Internal(anyhow::anyhow!("列幅 JSON の解析に失敗: {e}"))
            })?;
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    };

    let sort = if sort_json.is_empty() || sort_json == "[]" {
        None
    } else {
        let parsed: Vec<TableSortEntry> = serde_json::from_str(&sort_json).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("ソート JSON の解析に失敗: {e}"))
        })?;
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    };

    let filter = if filter_json.is_empty() || filter_json == "[]" {
        None
    } else {
        let parsed: Vec<TableFilterEntry> = serde_json::from_str(&filter_json).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("フィルター JSON の解析に失敗: {e}"))
        })?;
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    };

    Ok(TableUiMeta {
        sort,
        filter,
        column_widths,
    })
}

fn compute_sanitized_ui_preferences(
    column_names: &[String],
    meta: &TableUiMeta,
    options: SanitizeUiPreferencesOptions<'_>,
) -> SanitizedUiPreferencesValues {
    let mut sort = if options.clear_sort_filter {
        Vec::new()
    } else {
        meta.sort.clone().unwrap_or_default()
    };
    let mut filter = if options.clear_sort_filter {
        Vec::new()
    } else {
        meta.filter.clone().unwrap_or_default()
    };
    let mut column_widths = meta.column_widths.clone();

    if !options.clear_sort_filter && !options.column_renames.is_empty() {
        apply_column_renames_to_sort(&mut sort, options.column_renames);
        apply_column_renames_to_filter(&mut filter, options.column_renames);
        apply_column_renames_to_widths(&mut column_widths, options.column_renames);
    }

    SanitizedUiPreferencesValues {
        sort: filter_sort_entries_to_columns(column_names, &sort),
        filter: filter_filter_entries_to_columns(column_names, &filter),
        column_widths: prune_column_widths(column_names, column_widths),
    }
}

fn serialize_table_ui_meta(
    sort: &[TableSortEntry],
    filter: &[TableFilterEntry],
    column_widths: Option<&std::collections::HashMap<String, i32>>,
) -> DomainResult<SerializedTableUiMeta> {
    let sort_json = serde_json::to_string(sort).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("ソート JSON のシリアライズに失敗: {e}"))
    })?;
    let filter_json = serde_json::to_string(filter).map_err(|e| {
        DomainError::Internal(anyhow::anyhow!("フィルター JSON のシリアライズに失敗: {e}"))
    })?;
    let widths_json = match column_widths {
        Some(widths) if !widths.is_empty() => serde_json::to_string(widths).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("列幅 JSON のシリアライズに失敗: {e}"))
        })?,
        _ => "{}".to_string(),
    };

    Ok(SerializedTableUiMeta {
        sort_json,
        filter_json,
        widths_json,
    })
}

fn should_skip_ui_meta_persist(
    has_meta_row: bool,
    sort: &[TableSortEntry],
    filter: &[TableFilterEntry],
    column_widths: Option<&std::collections::HashMap<String, i32>>,
) -> bool {
    !has_meta_row && sort.is_empty() && filter.is_empty() && column_widths.is_none()
}

fn pragma_table_info_sql(table_name: &str) -> String {
    format!(
        "PRAGMA table_info({})",
        quote_sql_identifier(table_name)
    )
}

fn apply_column_renames_to_sort(
    entries: &mut [TableSortEntry],
    renames: &[(String, String)],
) {
    let rename_map: std::collections::HashMap<&str, &str> = renames
        .iter()
        .map(|(old_name, new_name)| (old_name.as_str(), new_name.as_str()))
        .collect();
    for entry in entries.iter_mut() {
        if let Some(new_name) = rename_map.get(entry.column.as_str()) {
            entry.column = (*new_name).to_string();
        }
    }
}

fn apply_column_renames_to_filter(
    entries: &mut [TableFilterEntry],
    renames: &[(String, String)],
) {
    let rename_map: std::collections::HashMap<&str, &str> = renames
        .iter()
        .map(|(old_name, new_name)| (old_name.as_str(), new_name.as_str()))
        .collect();
    for entry in entries.iter_mut() {
        if let Some(new_name) = rename_map.get(entry.column.as_str()) {
            entry.column = (*new_name).to_string();
        }
    }
}

fn apply_column_renames_to_widths(
    widths: &mut Option<std::collections::HashMap<String, i32>>,
    renames: &[(String, String)],
) {
    let Some(width_map) = widths.as_mut() else {
        return;
    };
    let rename_map: std::collections::HashMap<&str, &str> = renames
        .iter()
        .map(|(old_name, new_name)| (old_name.as_str(), new_name.as_str()))
        .collect();
    let mut renamed = std::collections::HashMap::new();
    for (key, value) in width_map.drain() {
        let new_key = rename_map
            .get(key.as_str())
            .map(|name| (*name).to_string())
            .unwrap_or(key);
        renamed.insert(new_key, value);
    }
    *width_map = renamed;
}

async fn persist_sanitized_table_ui_meta(
    pool: &SqlitePool,
    name: &str,
    sort: &[TableSortEntry],
    filter: &[TableFilterEntry],
    column_widths: Option<std::collections::HashMap<String, i32>>,
) -> DomainResult<()> {
    let has_meta_row: bool = match sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM user_table_meta WHERE table_name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row.is_some(),
        Err(err) if is_missing_user_table_meta_error(&err) => false,
        Err(err) => return Err(err.into()),
    };

    if should_skip_ui_meta_persist(has_meta_row, sort, filter, column_widths.as_ref()) {
        return Ok(());
    }

    let serialized = serialize_table_ui_meta(sort, filter, column_widths.as_ref())?;
    sqlx::query(
        r#"
        INSERT INTO user_table_meta (table_name, column_widths_json, sort_json, filter_json, updated_at)
        VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            column_widths_json = excluded.column_widths_json,
            sort_json = excluded.sort_json,
            filter_json = excluded.filter_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(name)
    .bind(serialized.widths_json)
    .bind(serialized.sort_json)
    .bind(serialized.filter_json)
    .execute(pool)
    .await?;

    Ok(())
}

async fn sanitize_data_ui_preferences(
    pool: &SqlitePool,
    name: &str,
    options: SanitizeUiPreferencesOptions<'_>,
) -> DomainResult<()> {
    let column_names = table_column_names(pool, name).await?;
    let meta = get_table_ui_meta(pool, name).await?;
    let sanitized = compute_sanitized_ui_preferences(&column_names, &meta, options);
    persist_sanitized_table_ui_meta(
        pool,
        name,
        &sanitized.sort,
        &sanitized.filter,
        sanitized.column_widths,
    )
    .await
}

async fn migrate_user_table_meta_key_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    old_name: &str,
    new_name: &str,
) -> DomainResult<()> {
    if old_name == new_name {
        return Ok(());
    }

    let has_old: bool = match sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM user_table_meta WHERE table_name = ?",
    )
    .bind(old_name)
    .fetch_optional(&mut **tx)
    .await
    {
        Ok(row) => row.is_some(),
        Err(err) if is_missing_user_table_meta_error(&err) => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    if !has_old {
        return Ok(());
    }

    sqlx::query("DELETE FROM user_table_meta WHERE table_name = ?")
        .bind(new_name)
        .execute(&mut **tx)
        .await?;

    sqlx::query("UPDATE user_table_meta SET table_name = ? WHERE table_name = ?")
        .bind(new_name)
        .bind(old_name)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

async fn sanitize_data_ui_preferences_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    options: SanitizeUiPreferencesOptions<'_>,
) -> DomainResult<()> {
    let column_names = table_column_names_in_tx(tx, name).await?;
    let meta = get_table_ui_meta_in_tx(tx, name).await?;
    let sanitized = compute_sanitized_ui_preferences(&column_names, &meta, options);
    persist_sanitized_table_ui_meta_in_tx(
        tx,
        name,
        &sanitized.sort,
        &sanitized.filter,
        sanitized.column_widths,
    )
    .await
}

async fn table_column_names_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
) -> DomainResult<Vec<String>> {
    let rows = table_pragma_info_in_tx(tx, table_name).await?;
    Ok(rows.into_iter().map(|row| row.name).collect())
}

async fn table_pragma_info_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    table_name: &str,
) -> DomainResult<Vec<PragmaTableInfoRow>> {
    let rows = sqlx::query_as::<_, PragmaTableInfoRow>(sqlx::AssertSqlSafe(pragma_table_info_sql(
        table_name,
    )))
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

async fn get_table_ui_meta_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
) -> DomainResult<TableUiMeta> {
    let name = ensure_viewable_table(name)?;
    let row = match sqlx::query_as::<_, (String, String, String)>(
        "SELECT column_widths_json, sort_json, filter_json FROM user_table_meta WHERE table_name = ?",
    )
    .bind(name)
    .fetch_optional(&mut **tx)
    .await
    {
        Ok(row) => row,
        Err(err) if is_missing_user_table_meta_error(&err) => return Ok(TableUiMeta::default()),
        Err(err) => return Err(err.into()),
    };
    decode_table_ui_meta_row(row)
}

async fn persist_sanitized_table_ui_meta_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    sort: &[TableSortEntry],
    filter: &[TableFilterEntry],
    column_widths: Option<std::collections::HashMap<String, i32>>,
) -> DomainResult<()> {
    let has_meta_row: bool = match sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM user_table_meta WHERE table_name = ?",
    )
    .bind(name)
    .fetch_optional(&mut **tx)
    .await
    {
        Ok(row) => row.is_some(),
        Err(err) if is_missing_user_table_meta_error(&err) => false,
        Err(err) => return Err(err.into()),
    };

    if should_skip_ui_meta_persist(has_meta_row, sort, filter, column_widths.as_ref()) {
        return Ok(());
    }

    let serialized = serialize_table_ui_meta(sort, filter, column_widths.as_ref())?;
    sqlx::query(
        r#"
        INSERT INTO user_table_meta (table_name, column_widths_json, sort_json, filter_json, updated_at)
        VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(table_name) DO UPDATE SET
            column_widths_json = excluded.column_widths_json,
            sort_json = excluded.sort_json,
            filter_json = excluded.filter_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(name)
    .bind(serialized.widths_json)
    .bind(serialized.sort_json)
    .bind(serialized.filter_json)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn build_where_from_filter(
    _column_names: &[String],
    _table_name: &str,
    entries: &[TableFilterEntry],
) -> DomainResult<(String, Vec<String>)> {
    let active: Vec<&TableFilterEntry> = entries
        .iter()
        .filter(|entry| !entry.text.is_empty())
        .collect();
    if active.is_empty() {
        return Ok((String::new(), Vec::new()));
    }

    let mut binds = Vec::with_capacity(active.len());
    let parts: Vec<String> = active
        .iter()
        .map(|entry| {
            let pattern = format!(
                "%{}%",
                escape_like_pattern(&entry.text.to_ascii_lowercase())
            );
            binds.push(pattern);
            format!(
                "LOWER(CAST({} AS TEXT)) LIKE ? ESCAPE '\\'",
                quote_sql_identifier(&entry.column)
            )
        })
        .collect();

    Ok((format!("WHERE {}", parts.join(" AND ")), binds))
}

fn build_order_by_from_sort(
    _column_names: &[String],
    _table_name: &str,
    sort_entries: &[TableSortEntry],
) -> DomainResult<String> {
    let parts: Vec<String> = sort_entries
        .iter()
        .map(|entry| {
            let direction = match entry.direction {
                TableSortDirection::Asc => "ASC",
                TableSortDirection::Desc => "DESC",
            };
            format!("{} {direction}", quote_sql_identifier(&entry.column))
        })
        .collect();

    Ok(parts.join(", "))
}

fn order_columns_from_pragma(rows: &[PragmaTableInfoRow]) -> String {
    let mut pk_columns: Vec<(i32, String)> = rows
        .iter()
        .filter(|row| row.pk > 0)
        .map(|row| (row.pk, row.name.clone()))
        .collect();
    if !pk_columns.is_empty() {
        pk_columns.sort_by_key(|(pk, _)| *pk);
        return pk_columns
            .into_iter()
            .map(|(_, name)| quote_sql_identifier(&name))
            .collect::<Vec<_>>()
            .join(", ");
    }

    if rows
        .iter()
        .any(|row| row.name.eq_ignore_ascii_case("id"))
    {
        return quote_sql_identifier("id");
    }

    if let Some(first) = rows.first() {
        return quote_sql_identifier(&first.name);
    }

    "rowid".to_string()
}

/// ソート指定から ORDER BY 句を組み立てる。空のときはデフォルト順。
pub async fn build_order_by_clause(
    pool: &SqlitePool,
    table_name: &str,
    sort_entries: &[TableSortEntry],
) -> DomainResult<String> {
    let pragma_rows = table_pragma_info(pool, table_name).await?;
    if sort_entries.is_empty() {
        return Ok(order_columns_from_pragma(&pragma_rows));
    }

    let column_names: Vec<String> = pragma_rows.into_iter().map(|row| row.name).collect();
    build_order_by_from_sort(&column_names, table_name, sort_entries)
}

#[derive(Debug, Default)]
struct TableUiMeta {
    sort: Option<Vec<TableSortEntry>>,
    filter: Option<Vec<TableFilterEntry>>,
    column_widths: Option<std::collections::HashMap<String, i32>>,
}

async fn get_table_ui_meta(pool: &SqlitePool, name: &str) -> DomainResult<TableUiMeta> {
    let name = ensure_viewable_table(name)?;
    let row = match sqlx::query_as::<_, (String, String, String)>(
        "SELECT column_widths_json, sort_json, filter_json FROM user_table_meta WHERE table_name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(err) if is_missing_user_table_meta_error(&err) => return Ok(TableUiMeta::default()),
        Err(err) => return Err(err.into()),
    };
    decode_table_ui_meta_row(row)
}

pub async fn list_user_table_rows(
    pool: &SqlitePool,
    name: &str,
    offset: i64,
    sort_override: Option<&[TableSortEntry]>,
    filter_override: Option<&[TableFilterEntry]>,
) -> DomainResult<TableDataView> {
    let name = ensure_viewable_table(name)?;
    if !object_exists_as_table_or_view(pool, name).await? {
        return Err(DomainError::NotFound);
    }
    if offset < 0 {
        return Err(DomainError::Validation(
            "offset は 0 以上で指定してください".into(),
        ));
    }

    let pragma_rows = table_pragma_info(pool, name).await?;
    let columns: Vec<String> = pragma_rows.iter().map(|row| row.name.clone()).collect();

    let ui_meta = if sort_override.is_none() || filter_override.is_none() {
        Some(get_table_ui_meta(pool, name).await?)
    } else {
        None
    };

    let mut effective_sort: Vec<TableSortEntry> = match sort_override {
        Some(entries) => entries.to_vec(),
        None => ui_meta
            .as_ref()
            .and_then(|meta| meta.sort.clone())
            .unwrap_or_default(),
    };
    effective_sort = filter_sort_entries_to_columns(&columns, &effective_sort);

    let mut effective_filter = compact_filter_entries(match filter_override {
        Some(entries) => entries,
        None => ui_meta
            .as_ref()
            .and_then(|meta| meta.filter.as_deref())
            .unwrap_or_default(),
    });
    effective_filter = filter_filter_entries_to_columns(&columns, &effective_filter);

    let (where_clause, where_binds) =
        build_where_from_filter(&columns, name, &effective_filter)?;
    let total_count =
        table_row_count(pool, name, &where_clause, &where_binds).await.map_err(DomainError::from)?;

    let order_by = if effective_sort.is_empty() {
        order_columns_from_pragma(&pragma_rows)
    } else {
        build_order_by_from_sort(&columns, name, &effective_sort)?
    };

    let rows = fetch_table_rows_chunk(
        pool,
        name,
        offset,
        TABLE_DATA_CHUNK_SIZE,
        columns.len(),
        &order_by,
        &where_clause,
        &where_binds,
    )
    .await?;
    let fetched = rows.len() as i64;
    let has_more = offset + fetched < total_count;

    let column_widths = if offset == 0 {
        let widths = match &ui_meta {
            Some(meta) => meta.column_widths.clone(),
            None => get_table_column_widths(pool, name).await?,
        };
        prune_column_widths(&columns, widths)
    } else {
        None
    };

    let sort = if offset == 0 {
        Some(effective_sort)
    } else {
        None
    };

    let filter = if offset == 0 {
        Some(effective_filter)
    } else {
        None
    };

    let column_meta = if offset == 0 {
        Some(column_meta_from_pragma(&pragma_rows)?)
    } else {
        None
    };

    Ok(TableDataView {
        columns,
        rows,
        total_count,
        offset,
        has_more,
        column_widths,
        sort,
        filter,
        column_meta,
    })
}

pub async fn update_table_cell(
    pool: &SqlitePool,
    name: &str,
    request: &TableCellUpdateRequest,
) -> DomainResult<TableCellUpdateResult> {
    let name = ensure_editable_user_table(name)?;
    if !object_name_exists(pool, name, "table").await? {
        return Err(DomainError::NotFound);
    }

    let pragma_rows = table_pragma_info(pool, name).await?;
    let column_meta = column_meta_from_pragma(&pragma_rows)?;
    let target = column_meta
        .iter()
        .find(|col| col.name == request.column)
        .ok_or_else(|| {
            DomainError::Validation(format!(
                "カラム `{}` はテーブルに存在しません",
                request.column
            ))
        })?;

    if target.pk {
        return Err(DomainError::Validation(
            "主キー列は編集できません".into(),
        ));
    }

    let pk_columns: Vec<&TableDataColumnMeta> =
        column_meta.iter().filter(|col| col.pk).collect();
    if pk_columns.is_empty() {
        return Err(DomainError::Validation(
            "主キー列が定義されていないテーブルは編集できません".into(),
        ));
    }

    for pk in &pk_columns {
        if !request.keys.contains_key(&pk.name) {
            return Err(DomainError::Validation(format!(
                "主キー列 `{}` の値が指定されていません",
                pk.name
            )));
        }
    }

    let new_value = parse_cell_update_value(
        &target.type_key,
        target.nullable,
        request.null,
        &request.value,
        &target.name,
    )?;

    let quoted_table = quote_sql_identifier(name);
    let quoted_column = quote_sql_identifier(&target.name);
    let where_clause = pk_columns
        .iter()
        .map(|pk| format!("{} = ?", quote_sql_identifier(&pk.name)))
        .collect::<Vec<_>>()
        .join(" AND ");
    let query = format!("UPDATE {quoted_table} SET {quoted_column} = ? WHERE {where_clause}");

    let mut sql_query = sqlx::query(sqlx::AssertSqlSafe(query));
    sql_query = bind_cell_value(sql_query, &new_value);
    for pk in &pk_columns {
        let raw = request
            .keys
            .get(&pk.name)
            .map(String::as_str)
            .unwrap_or("");
        let pk_value = parse_cell_key_value(&pk.type_key, raw, &pk.name)?;
        sql_query = bind_cell_value(sql_query, &pk_value);
    }

    let result = sql_query.execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(DomainError::NotFound);
    }

    Ok(TableCellUpdateResult {
        value: cell_value_to_display(&target.type_key, &new_value),
    })
}

pub async fn fetch_table_rows_chunk(
    pool: &SqlitePool,
    name: &str,
    offset: i64,
    limit: i64,
    column_count: usize,
    order_by: &str,
    where_clause: &str,
    where_binds: &[String],
) -> DomainResult<Vec<Vec<Option<String>>>> {
    let name = ensure_viewable_table(name)?;
    if offset < 0 || limit < 0 {
        return Err(DomainError::Validation(
            "offset と limit は 0 以上で指定してください".into(),
        ));
    }

    let quoted = quote_sql_identifier(name);
    let query = if where_clause.is_empty() {
        format!("SELECT * FROM {quoted} ORDER BY {order_by} LIMIT {limit} OFFSET {offset}")
    } else {
        format!(
            "SELECT * FROM {quoted} {where_clause} ORDER BY {order_by} LIMIT {limit} OFFSET {offset}"
        )
    };
    let mut sql_query = sqlx::query(sqlx::AssertSqlSafe(query));
    for bind in where_binds {
        sql_query = sql_query.bind(bind);
    }
    let sql_rows = sql_query.fetch_all(pool).await?;

    Ok(sql_rows
        .iter()
        .map(|row| row_to_cells(row, column_count))
        .collect())
}

async fn table_pragma_info(pool: &SqlitePool, table_name: &str) -> DomainResult<Vec<PragmaTableInfoRow>> {
    let rows = sqlx::query_as::<_, PragmaTableInfoRow>(sqlx::AssertSqlSafe(pragma_table_info_sql(
        table_name,
    )))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn table_column_names(pool: &SqlitePool, table_name: &str) -> DomainResult<Vec<String>> {
    let rows = table_pragma_info(pool, table_name).await?;
    Ok(rows.into_iter().map(|row| row.name).collect())
}

fn column_meta_from_pragma(rows: &[PragmaTableInfoRow]) -> DomainResult<Vec<TableDataColumnMeta>> {
    rows.iter()
        .map(|row| {
            let type_key = pragma_type_to_type_key(&row.type_name).ok_or_else(|| {
                DomainError::Validation(format!(
                    "カラム `{}` の型を判別できません",
                    row.name
                ))
            })?;
            Ok(TableDataColumnMeta {
                name: row.name.clone(),
                pk: row.pk > 0,
                type_key: type_key.to_string(),
                nullable: row.notnull == 0,
            })
        })
        .collect()
}

fn pragma_type_to_type_key(type_name: &str) -> Option<&'static str> {
    let upper = type_name.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return Some("text");
    }
    if upper.contains("INT") {
        return Some("integer");
    }
    if upper.contains("REAL") || upper.contains("FLOA") || upper.contains("DOUB") {
        return Some("real");
    }
    if upper.contains("BLOB") {
        return Some("blob");
    }
    if upper.contains("BOOL") {
        return Some("boolean");
    }
    if upper.contains("TIMESTAMP")
        || upper.contains("DATETIME")
        || upper.contains("DATE")
        || upper.contains("TIME")
    {
        return Some("timestamp");
    }
    if upper.contains("TEXT") || upper.contains("CHAR") || upper.contains("CLOB") {
        return Some("text");
    }
    None
}

#[derive(Debug, Clone)]
enum CellBindValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

fn parse_cell_update_value(
    type_key: &str,
    nullable: bool,
    is_null: bool,
    raw: &str,
    column_name: &str,
) -> DomainResult<CellBindValue> {
    if is_null {
        if !nullable {
            return Err(DomainError::Validation(format!(
                "NOT NULL 列 `{column_name}` には NULL を設定できません"
            )));
        }
        return Ok(CellBindValue::Null);
    }

    let raw = raw.trim();
    match type_key {
        "integer" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            let value = raw
                .parse::<i64>()
                .map_err(|_| DomainError::Validation(format!("{column_name}は整数で指定してください")))?;
            Ok(CellBindValue::Integer(value))
        }
        "boolean" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            let value = match raw {
                "0" | "false" | "FALSE" | "False" => 0,
                "1" | "true" | "TRUE" | "True" => 1,
                _ => {
                    return Err(DomainError::Validation(format!(
                        "{column_name}は 0/1 または true/false で指定してください"
                    )));
                }
            };
            Ok(CellBindValue::Integer(value))
        }
        "real" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            let value = raw
                .parse::<f64>()
                .map_err(|_| DomainError::Validation(format!("{column_name}は数値で指定してください")))?;
            Ok(CellBindValue::Real(value))
        }
        "text" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            Ok(CellBindValue::Text(raw.to_string()))
        }
        "timestamp" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            let canonical =
                parse_and_canonicalize_cell_timestamp(raw, &format!("{column_name}"))?;
            Ok(CellBindValue::Text(canonical))
        }
        "blob" => {
            if raw.is_empty() && nullable {
                return Ok(CellBindValue::Null);
            }
            Ok(CellBindValue::Blob(parse_hex_blob(raw, column_name)?))
        }
        _ => Err(DomainError::Validation(format!(
            "カラム `{column_name}` の型が不正です"
        ))),
    }
}

fn parse_cell_key_value(
    type_key: &str,
    raw: &str,
    column_name: &str,
) -> DomainResult<CellBindValue> {
    parse_cell_update_value(type_key, false, false, raw, column_name)
}

fn parse_hex_blob(raw: &str, column_name: &str) -> DomainResult<Vec<u8>> {
    let raw = raw.trim();
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    if !hex.len().is_multiple_of(2) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(DomainError::Validation(format!(
            "{column_name}は16進数（偶数桁）で指定してください"
        )));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars: Vec<char> = hex.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let pair: String = chars[index..index + 2].iter().collect();
        let byte = u8::from_str_radix(&pair, 16).map_err(|_| {
            DomainError::Validation(format!("{column_name}は16進数で指定してください"))
        })?;
        bytes.push(byte);
        index += 2;
    }
    Ok(bytes)
}

fn bind_cell_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    value: &CellBindValue,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match value {
        CellBindValue::Null => query.bind(None::<String>),
        CellBindValue::Integer(value) => query.bind(value),
        CellBindValue::Real(value) => query.bind(value),
        CellBindValue::Text(value) => query.bind(value),
        CellBindValue::Blob(value) => query.bind(value),
    }
}

fn cell_value_to_display(_type_key: &str, value: &CellBindValue) -> Option<String> {
    match value {
        CellBindValue::Null => None,
        CellBindValue::Integer(value) => Some(value.to_string()),
        CellBindValue::Real(value) => Some(value.to_string()),
        CellBindValue::Text(value) => Some(value.clone()),
        CellBindValue::Blob(bytes) => Some(format!("0x{}", bytes_to_hex(bytes))),
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn row_to_cells(row: &sqlx::sqlite::SqliteRow, column_count: usize) -> Vec<Option<String>> {
    (0..column_count)
        .map(|index| format_sqlite_cell(row, index))
        .collect()
}

fn format_sqlite_cell(row: &sqlx::sqlite::SqliteRow, index: usize) -> Option<String> {
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(|v| v.to_string());
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value.map(|v| v.to_string());
    }
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value;
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value.map(|bytes| format!("0x{}", bytes_to_hex(&bytes)));
    }
    None
}

pub fn build_seed_form_rows(columns: &[TableColumnInput]) -> Vec<SeedFormRow> {
    let now = Local::now().naive_local();
    let year_ago = now - Duration::days(365);
    let timestamp_from = year_ago.format("%Y-%m-%dT%H:%M").to_string();
    let timestamp_to = now.format("%Y-%m-%dT%H:%M").to_string();

    columns
        .iter()
        .map(|column| SeedFormRow {
            name: column.name.clone(),
            type_key: column.type_key.clone(),
            type_label: column_type_label(&column.type_key).to_string(),
            nullable: column.nullable,
            int_min: "0".to_string(),
            int_max: "1000".to_string(),
            real_min: "0".to_string(),
            real_max: "100".to_string(),
            text_min: "8".to_string(),
            text_max: "64".to_string(),
            charset: "ascii_alnum".to_string(),
            blob_min: "1".to_string(),
            blob_max: "32".to_string(),
            timestamp_from: timestamp_from.clone(),
            timestamp_to: timestamp_to.clone(),
            include_null: false,
        })
        .collect()
}

pub fn parse_seed_form(
    columns: &[TableColumnInput],
    form: &TestDataSeedForm,
) -> DomainResult<(u32, Vec<(String, ColumnSeedRule)>)> {
    let count = form
        .count
        .trim()
        .parse::<u32>()
        .map_err(|_| DomainError::Validation("生成件数は数値で指定してください".into()))?;
    if count == 0 || count > MAX_TEST_DATA_ROWS {
        return Err(DomainError::Validation(format!(
            "生成件数は 1〜{MAX_TEST_DATA_ROWS} で指定してください"
        )));
    }

    if form.col_name.len() != columns.len() || form.col_type.len() != columns.len() {
        return Err(DomainError::Validation(
            "カラム定義とフォームの内容が一致しません".into(),
        ));
    }

    let mut rules = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        let form_name = form.col_name.get(index).map(String::as_str).unwrap_or("");
        let form_type = form.col_type.get(index).map(String::as_str).unwrap_or("");
        if form_name != column.name || form_type != column.type_key {
            return Err(DomainError::Validation(format!(
                "カラム `{}` のフォーム内容が不正です",
                column.name
            )));
        }

        let include_null = form
            .col_include_null
            .get(index)
            .map(|value| value == "1")
            .unwrap_or(false);
        if !column.nullable && include_null {
            return Err(DomainError::Validation(format!(
                "NOT NULL 列 `{}` では NULL を含められません",
                column.name
            )));
        }

        let rule = match column.type_key.as_str() {
            "integer" => {
                let min = parse_form_i64(
                    form.col_int_min.get(index),
                    &format!("カラム `{}` の最小値", column.name),
                )?;
                let max = parse_form_i64(
                    form.col_int_max.get(index),
                    &format!("カラム `{}` の最大値", column.name),
                )?;
                if min > max {
                    return Err(DomainError::Validation(format!(
                        "カラム `{}` の最小値は最大値以下にしてください",
                        column.name
                    )));
                }
                ColumnSeedRule::Integer {
                    min,
                    max,
                    include_null,
                }
            }
            "real" => {
                let min = parse_form_f64(
                    form.col_real_min.get(index),
                    &format!("カラム `{}` の最小値", column.name),
                )?;
                let max = parse_form_f64(
                    form.col_real_max.get(index),
                    &format!("カラム `{}` の最大値", column.name),
                )?;
                if min > max {
                    return Err(DomainError::Validation(format!(
                        "カラム `{}` の最小値は最大値以下にしてください",
                        column.name
                    )));
                }
                ColumnSeedRule::Real {
                    min,
                    max,
                    include_null,
                }
            }
            "text" => {
                let min_len = parse_form_u32(
                    form.col_text_min.get(index),
                    &format!("カラム `{}` の最小文字数", column.name),
                )?;
                let max_len = parse_form_u32(
                    form.col_text_max.get(index),
                    &format!("カラム `{}` の最大文字数", column.name),
                )?;
                if min_len < 1 || max_len < 1 || min_len > max_len {
                    return Err(DomainError::Validation(format!(
                        "カラム `{}` の文字数範囲が不正です",
                        column.name
                    )));
                }
                ColumnSeedRule::Text {
                    min_len,
                    max_len,
                    charset: parse_charset(form.col_charset.get(index), &column.name)?,
                    include_null,
                }
            }
            "blob" => {
                let min_len = parse_form_u32(
                    form.col_blob_min.get(index),
                    &format!("カラム `{}` の最小バイト数", column.name),
                )?;
                let max_len = parse_form_u32(
                    form.col_blob_max.get(index),
                    &format!("カラム `{}` の最大バイト数", column.name),
                )?;
                if min_len < 1 || max_len < 1 || min_len > max_len || max_len > MAX_BLOB_SEED_BYTES
                {
                    return Err(DomainError::Validation(format!(
                        "カラム `{}` のバイト数範囲は 1〜{MAX_BLOB_SEED_BYTES} で指定してください",
                        column.name
                    )));
                }
                ColumnSeedRule::Blob {
                    min_len,
                    max_len,
                    include_null,
                }
            }
            "timestamp" => {
                let from = form
                    .col_timestamp_from
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let to = form
                    .col_timestamp_to
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let from_dt = parse_form_datetime(
                    &from,
                    &format!("カラム `{}` の開始日時", column.name),
                )?;
                let to_dt = parse_form_datetime(
                    &to,
                    &format!("カラム `{}` の終了日時", column.name),
                )?;
                if from_dt > to_dt {
                    return Err(DomainError::Validation(format!(
                        "カラム `{}` の開始日時は終了日時以前にしてください",
                        column.name
                    )));
                }
                ColumnSeedRule::Timestamp {
                    from,
                    to,
                    include_null,
                }
            }
            "boolean" => ColumnSeedRule::Boolean { include_null },
            _ => {
                return Err(DomainError::Validation(format!(
                    "カラム `{}` の型はテストデータ生成に対応していません",
                    column.name
                )));
            }
        };
        rules.push((column.name.clone(), rule));
    }

    Ok((count, rules))
}

fn seed_progress_interval(count: u32) -> u32 {
    (count / 100).max(1)
}

fn maybe_report_seed_progress<F: FnMut(u32, u32)>(
    done: u32,
    total: u32,
    interval: u32,
    on_progress: &mut Option<F>,
) {
    if done == 1 || done == total || done % interval == 0 {
        if let Some(callback) = on_progress {
            callback(done, total);
        }
    }
}

pub async fn generate_test_data<F, C>(
    pool: &SqlitePool,
    table_name: &str,
    count: u32,
    rules: &[(String, ColumnSeedRule)],
    mut on_progress: Option<F>,
    mut should_cancel: Option<C>,
) -> DomainResult<u32>
where
    F: FnMut(u32, u32),
    C: FnMut() -> bool,
{
    let table_name = ensure_editable_user_table(table_name)?;
    if !object_name_exists(pool, table_name, "table").await? {
        return Err(DomainError::NotFound);
    }
    if count == 0 || count > MAX_TEST_DATA_ROWS {
        return Err(DomainError::Validation(format!(
            "生成件数は 1〜{MAX_TEST_DATA_ROWS} で指定してください"
        )));
    }

    let mut tx = pool.begin().await?;
    let mut rng = OsRng;
    let progress_interval = seed_progress_interval(count);

    if rules.is_empty() {
        let quoted_table = quote_sql_identifier(table_name);
        let query = format!("INSERT INTO {quoted_table} DEFAULT VALUES");
        for done in 1..=count {
            if should_cancel.as_mut().map(|check| check()).unwrap_or(false) {
                return Err(DomainError::Validation("生成がキャンセルされました".into()));
            }
            sqlx::query(sqlx::AssertSqlSafe(query.clone()))
                .execute(&mut *tx)
                .await?;
            maybe_report_seed_progress(done, count, progress_interval, &mut on_progress);
        }
    } else {
        let quoted_cols = rules
            .iter()
            .map(|(name, _)| quote_sql_identifier(name))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = std::iter::repeat("?")
            .take(rules.len())
            .collect::<Vec<_>>()
            .join(", ");
        let quoted_table = quote_sql_identifier(table_name);
        let query = format!("INSERT INTO {quoted_table} ({quoted_cols}) VALUES ({placeholders})");

        for done in 1..=count {
            if should_cancel.as_mut().map(|check| check()).unwrap_or(false) {
                return Err(DomainError::Validation("生成がキャンセルされました".into()));
            }
            let mut sql_query = sqlx::query(sqlx::AssertSqlSafe(query.clone()));
            for (_, rule) in rules {
                sql_query = match generate_cell_value(rule, &mut rng)? {
                    SeedCellValue::Null => sql_query.bind(None::<String>),
                    SeedCellValue::Integer(value) => sql_query.bind(value),
                    SeedCellValue::Real(value) => sql_query.bind(value),
                    SeedCellValue::Text(value) => sql_query.bind(value),
                    SeedCellValue::Blob(value) => sql_query.bind(value),
                };
            }
            sql_query.execute(&mut *tx).await?;
            maybe_report_seed_progress(done, count, progress_interval, &mut on_progress);
        }
    }

    tx.commit().await?;
    Ok(count)
}

enum SeedCellValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

fn generate_cell_value(rule: &ColumnSeedRule, rng: &mut impl Rng) -> DomainResult<SeedCellValue> {
    if should_generate_null(rule, rng) {
        return Ok(SeedCellValue::Null);
    }

    Ok(match rule {
        ColumnSeedRule::Integer { min, max, .. } => {
            SeedCellValue::Integer(rng.gen_range(*min..=*max))
        }
        ColumnSeedRule::Real { min, max, .. } => SeedCellValue::Real(rng.gen_range(*min..*max)),
        ColumnSeedRule::Text {
            min_len,
            max_len,
            charset,
            ..
        } => {
            let len = rng.gen_range(*min_len..=*max_len);
            SeedCellValue::Text(random_string(*charset, len, rng))
        }
        ColumnSeedRule::Blob { min_len, max_len, .. } => {
            let len = rng.gen_range(*min_len..=*max_len) as usize;
            SeedCellValue::Blob(random_bytes(len, rng))
        }
        ColumnSeedRule::Timestamp { from, to, .. } => {
            let from_dt = parse_form_datetime(from, "開始日時")?;
            let to_dt = parse_form_datetime(to, "終了日時")?;
            SeedCellValue::Text(random_timestamp_between(from_dt, to_dt, rng))
        }
        ColumnSeedRule::Boolean { .. } => SeedCellValue::Integer(if rng.gen_bool(0.5) { 1 } else { 0 }),
    })
}

fn should_generate_null(rule: &ColumnSeedRule, rng: &mut impl Rng) -> bool {
    let include_null = match rule {
        ColumnSeedRule::Integer { include_null, .. }
        | ColumnSeedRule::Real { include_null, .. }
        | ColumnSeedRule::Text { include_null, .. }
        | ColumnSeedRule::Blob { include_null, .. }
        | ColumnSeedRule::Timestamp { include_null, .. }
        | ColumnSeedRule::Boolean { include_null } => *include_null,
    };
    include_null && rng.gen_bool(NULL_SEED_PROBABILITY)
}

fn random_string(charset: StringCharset, len: u32, rng: &mut impl Rng) -> String {
    let len = len as usize;
    match charset {
        StringCharset::AsciiAlnum => {
            (0..len)
                .map(|_| ASCII_ALNUM[rng.gen_range(0..ASCII_ALNUM.len())] as char)
                .collect()
        }
        StringCharset::AsciiPrintable => {
            (0..len)
                .map(|_| ASCII_PRINTABLE[rng.gen_range(0..ASCII_PRINTABLE.len())] as char)
                .collect()
        }
        StringCharset::Japanese => {
            let pool: Vec<char> = HIRAGANA
                .chars()
                .chain(KATAKANA.chars())
                .chain(KANJI.chars())
                .chain(ASCII_ALNUM.iter().map(|&b| b as char))
                .collect();
            (0..len)
                .map(|_| pool[rng.gen_range(0..pool.len())])
                .collect()
        }
    }
}

fn random_bytes(len: usize, rng: &mut impl Rng) -> Vec<u8> {
    (0..len).map(|_| rng.gen_range(0..=255)).collect()
}

fn random_timestamp_between(
    from: NaiveDateTime,
    to: NaiveDateTime,
    rng: &mut impl Rng,
) -> String {
    let diff_secs = to.signed_duration_since(from).num_seconds();
    let offset = if diff_secs <= 0 {
        0
    } else {
        rng.gen_range(0..=diff_secs)
    };
    (from + Duration::seconds(offset))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

fn column_type_label(type_key: &str) -> &'static str {
    match type_key {
        "integer" => "整数",
        "real" => "実数",
        "text" => "文字列",
        "blob" => "バイナリ",
        "timestamp" => "日時",
        "boolean" => "真偽値",
        _ => "不明",
    }
}

fn parse_charset(value: Option<&String>, column_name: &str) -> DomainResult<StringCharset> {
    match value.map(String::as_str).unwrap_or("ascii_alnum") {
        "ascii_alnum" => Ok(StringCharset::AsciiAlnum),
        "ascii_printable" => Ok(StringCharset::AsciiPrintable),
        "japanese" => Ok(StringCharset::Japanese),
        _ => Err(DomainError::Validation(format!(
            "カラム `{column_name}` の文字種が不正です"
        ))),
    }
}

fn parse_form_i64(value: Option<&String>, label: &str) -> DomainResult<i64> {
    let raw = value.map(String::as_str).unwrap_or("").trim();
    raw.parse::<i64>()
        .map_err(|_| DomainError::Validation(format!("{label}は整数で指定してください")))
}

fn parse_form_f64(value: Option<&String>, label: &str) -> DomainResult<f64> {
    let raw = value.map(String::as_str).unwrap_or("").trim();
    raw.parse::<f64>()
        .map_err(|_| DomainError::Validation(format!("{label}は数値で指定してください")))
}

fn parse_form_u32(value: Option<&String>, label: &str) -> DomainResult<u32> {
    let raw = value.map(String::as_str).unwrap_or("").trim();
    raw.parse::<u32>()
        .map_err(|_| DomainError::Validation(format!("{label}は正の整数で指定してください")))
}

fn parse_form_datetime(value: &str, label: &str) -> DomainResult<NaiveDateTime> {
    let normalized = value.trim().replace(' ', "T");
    let normalized = normalized
        .strip_suffix('Z')
        .or_else(|| normalized.strip_suffix('z'))
        .unwrap_or(normalized.as_str());

    const FORMATS: &[&str] = &["%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S"];
    for format in FORMATS {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(normalized, format) {
            return Ok(parsed);
        }
    }

    Err(DomainError::Validation(format!(
        "{label}は YYYY-MM-DDTHH:MM 形式で指定してください"
    )))
}

fn parse_and_canonicalize_cell_timestamp(value: &str, label: &str) -> DomainResult<String> {
    let value = value.trim().replace(' ', "T");
    if value.is_empty() {
        return Err(DomainError::Validation(format!("{label}を指定してください")));
    }

    const OFFSET_FORMATS: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%dT%H:%M%z",
    ];
    for format in OFFSET_FORMATS {
        if let Ok(parsed) = DateTime::parse_from_str(&value, format) {
            return Ok(parsed.format("%Y-%m-%dT%H:%M:%S%:z").to_string());
        }
    }

    if let Some(base) = value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) {
        if let Some(parsed) = parse_naive_datetime(base) {
            let offset = FixedOffset::east_opt(0).ok_or_else(|| {
                DomainError::Internal(anyhow::anyhow!("UTC オフセットの構築に失敗"))
            })?;
            return Ok(format_timestamp_with_offset(
                parsed,
                offset,
                &format!("{label}の日時"),
            )?);
        }
    }

    if let Some(parsed) = parse_naive_datetime(&value) {
        let offset = default_jst_offset()?;
        return Ok(format_timestamp_with_offset(
            parsed,
            offset,
            &format!("{label}の日時"),
        )?);
    }

    Err(DomainError::Validation(format!(
        "{label}は YYYY-MM-DDTHH:MM:SS+HH:MM 形式で指定してください"
    )))
}

fn parse_naive_datetime(value: &str) -> Option<NaiveDateTime> {
    const FORMATS: &[&str] = &["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"];
    FORMATS
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
}

fn default_jst_offset() -> DomainResult<FixedOffset> {
    FixedOffset::east_opt(9 * 3600).ok_or_else(|| {
        DomainError::Internal(anyhow::anyhow!("日本標準時オフセットの構築に失敗"))
    })
}

fn format_timestamp_with_offset(
    naive: NaiveDateTime,
    offset: FixedOffset,
    label: &str,
) -> DomainResult<String> {
    let parsed = offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| DomainError::Validation(format!("{label}が不正です")))?;
    Ok(parsed.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
}

fn sanitize_table_name_input(name: &str) -> DomainResult<&str> {
    let name = sanitize_identifier_input(name);
    if name.is_empty() {
        return Err(DomainError::NotFound);
    }
    if validate_identifier_chars(name, "テーブル名").is_err()
        || name.contains('/')
        || name.contains('\\')
    {
        return Err(DomainError::NotFound);
    }
    Ok(name)
}

fn ensure_viewable_table(name: &str) -> DomainResult<&str> {
    let name = sanitize_table_name_input(name)?;
    if is_infra_table(name) {
        return Err(DomainError::SystemTable(infra_table_denied_message(name)));
    }
    Ok(name)
}

fn ensure_editable_user_table(name: &str) -> DomainResult<&str> {
    let name = sanitize_table_name_input(name)?;
    if is_infra_table(name) {
        return Err(DomainError::SystemTable(infra_table_denied_message(name)));
    }
    if is_cms_core_table(name) {
        return Err(DomainError::SystemTable(cms_table_edit_denied_message(name)));
    }
    Ok(name)
}

fn parse_user_columns_from_create_sql(sql: &str) -> DomainResult<Vec<TableColumnInput>> {
    let body = extract_create_table_body(sql)?;
    let mut columns = Vec::new();

    for part in split_column_definitions(&body) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_uppercase()
            .starts_with("IDINTEGERPRIMARYKEY")
        {
            continue;
        }
        columns.push(parse_column_definition(part)?);
    }

    Ok(columns)
}

fn extract_create_table_body(sql: &str) -> DomainResult<&str> {
    let open = sql
        .find('(')
        .ok_or_else(|| DomainError::Validation("テーブル定義の解析に失敗しました".into()))?;
    let close = sql
        .rfind(')')
        .ok_or_else(|| DomainError::Validation("テーブル定義の解析に失敗しました".into()))?;
    if close <= open {
        return Err(DomainError::Validation(
            "テーブル定義の解析に失敗しました".into(),
        ));
    }
    Ok(&sql[open + 1..close])
}

fn split_column_definitions(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in body.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

fn parse_column_definition(definition: &str) -> DomainResult<TableColumnInput> {
    let (name, rest) = parse_column_name_and_rest(definition)?;
    if name.eq_ignore_ascii_case("id") {
        return Err(DomainError::Validation(
            "主キー列 id は編集対象に含められません".into(),
        ));
    }

    let upper = rest.to_ascii_uppercase();
    let nullable = !upper.contains("NOT NULL");

    let type_key = if upper.contains(" INTEGER") || upper.starts_with("INTEGER") {
        "integer"
    } else if upper.contains(" REAL") || upper.starts_with("REAL") {
        "real"
    } else if upper.contains(" TEXT") || upper.starts_with("TEXT") {
        "text"
    } else if upper.contains(" BLOB") || upper.starts_with("BLOB") {
        "blob"
    } else if upper.contains(" TIMESTAMP") || upper.starts_with("TIMESTAMP") {
        "timestamp"
    } else if upper.contains(" BOOLEAN") || upper.starts_with("BOOLEAN") {
        "boolean"
    } else {
        return Err(DomainError::Validation(format!(
            "カラム `{name}` の型を判別できません"
        )));
    };

    Ok(TableColumnInput {
        name: validate_column_name(&name)?,
        type_key: type_key.to_string(),
        nullable,
        orig_name: None,
    })
}

fn parse_column_name_and_rest(definition: &str) -> DomainResult<(String, &str)> {
    let definition = definition.trim();
    if definition.is_empty() {
        return Err(DomainError::Validation(
            "空のカラム定義は解析できません".into(),
        ));
    }

    if definition.starts_with('"') {
        let (name, rest) = parse_quoted_sql_identifier(definition)?;
        return Ok((name, rest));
    }

    let split_at = definition
        .find(char::is_whitespace)
        .ok_or_else(|| DomainError::Validation("カラム名の解析に失敗しました".into()))?;
    let name = definition[..split_at].to_string();
    let rest = definition[split_at..].trim();
    Ok((name, rest))
}

fn parse_quoted_sql_identifier(definition: &str) -> DomainResult<(String, &str)> {
    if !definition.starts_with('"') {
        return Err(DomainError::Validation(
            "クォートされた識別子の解析に失敗しました".into(),
        ));
    }

    let mut name = String::new();
    let mut chars = definition[1..].chars();
    loop {
        match chars.next() {
            None => {
                return Err(DomainError::Validation(
                    "カラム名の解析に失敗しました".into(),
                ));
            }
            Some('"') => {
                if chars.as_str().starts_with('"') {
                    name.push('"');
                    chars.next();
                } else {
                    return Ok((name, chars.as_str().trim_start()));
                }
            }
            Some(ch) => name.push(ch),
        }
    }
}

async fn execute_ddl(pool: &SqlitePool, ddl: &str) -> DomainResult<()> {
    sqlx::query(sqlx::AssertSqlSafe(ddl.to_string()))
        .execute(pool)
        .await?;
    Ok(())
}

async fn table_row_count(
    pool: &SqlitePool,
    name: &str,
    where_clause: &str,
    where_binds: &[String],
) -> AppResult<i64> {
    let quoted = quote_sql_identifier(name);
    let query = if where_clause.is_empty() {
        format!("SELECT COUNT(*) FROM {quoted}")
    } else {
        format!("SELECT COUNT(*) FROM {quoted} {where_clause}")
    };
    let mut sql_query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query));
    for bind in where_binds {
        sql_query = sql_query.bind(bind);
    }
    let count = sql_query
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_system_table_detects_cms_and_infra_tables() {
        assert!(is_system_table("_sqlx_migrations"));
        assert!(is_system_table("sqlite_sequence"));
        assert!(is_system_table("posts"));
        assert!(is_system_table("users"));
        assert!(is_system_table("user_table_meta"));
        assert!(!is_system_table("my_custom_table"));
    }

    #[test]
    fn is_cms_core_table_includes_users() {
        assert!(is_cms_core_table("posts"));
        assert!(is_cms_core_table("users"));
        assert!(is_cms_core_table("user_table_meta"));
        assert!(!is_cms_core_table("my_custom_table"));
    }

    #[test]
    fn is_cms_readonly_table_covers_all_cms_core_tables() {
        assert!(is_cms_readonly_table("posts"));
        assert!(is_cms_readonly_table("users"));
        assert!(is_cms_readonly_table("user_table_meta"));
        assert!(!is_cms_readonly_table("my_custom_table"));
    }

    #[test]
    fn is_db_admin_editable_allows_only_user_defined_tables() {
        assert!(!is_db_admin_editable("posts"));
        assert!(!is_db_admin_editable("users"));
        assert!(!is_db_admin_editable("user_table_meta"));
        assert!(!is_db_admin_editable("_sqlx_migrations"));
        assert!(is_db_admin_editable("my_custom_table"));
    }

    #[test]
    fn build_duplicate_payloads_extracts_editable_objects_only() {
        let payloads = build_duplicate_payloads(
            &[DbObjectItem {
                name: "custom_notes".to_string(),
                sql: r#"CREATE TABLE "custom_notes" (id INTEGER PRIMARY KEY, "body" TEXT NOT NULL)"#
                    .to_string(),
                sql_preview: String::new(),
                is_system: false,
                row_count: Some(0),
            }],
            &[DbObjectItem {
                name: "custom_notes_view".to_string(),
                sql: r#"CREATE VIEW "custom_notes_view" AS SELECT id, body FROM custom_notes"#
                    .to_string(),
                sql_preview: String::new(),
                is_system: false,
                row_count: None,
            }],
        );

        assert_eq!(payloads.tables["custom_notes"].len(), 1);
        assert_eq!(payloads.tables["custom_notes"][0].name, "body");
        assert_eq!(
            payloads.views["custom_notes_view"],
            "SELECT id, body FROM custom_notes"
        );
        assert!(!payloads.tables.contains_key("posts"));
    }

    #[test]
    fn is_db_admin_data_viewable_blocks_infra_only() {
        assert!(!is_db_admin_data_viewable("_sqlx_migrations"));
        assert!(!is_db_admin_data_viewable("sqlite_sequence"));
        assert!(is_db_admin_data_viewable("posts"));
        assert!(is_db_admin_data_viewable("users"));
        assert!(is_db_admin_data_viewable("user_table_meta"));
        assert!(is_db_admin_data_viewable("my_custom_table"));
    }

    #[test]
    fn is_hidden_admin_table_detects_infra_tables() {
        assert!(is_hidden_admin_table("_sqlx_migrations"));
        assert!(!is_hidden_admin_table("posts"));
        assert!(!is_hidden_admin_table("user_table_meta"));
    }

    #[test]
    fn system_table_denied_message_describes_table_kind() {
        assert!(cms_table_edit_denied_message("posts").contains("列編集・テストデータ生成はできません"));
        assert!(infra_table_denied_message("_sqlx_migrations").contains("インフラ用"));
        assert!(system_table_denied_message("posts").contains("CMSのシステムテーブル"));
        assert!(system_table_denied_message("_sqlx_migrations").contains("インフラ用"));
    }

    #[test]
    fn validate_user_object_name_rejects_system_names() {
        assert!(validate_user_object_name("posts").is_err());
        assert!(validate_user_object_name("user_table_meta").is_err());
        assert!(validate_user_object_name("my_table").is_ok());
        assert!(validate_user_object_name("記事").is_ok());
        assert!(validate_user_object_name("a/b").is_err());
    }

    #[test]
    fn quote_sql_identifier_escapes_embedded_quotes() {
        assert_eq!(quote_sql_identifier("title"), "\"title\"");
        assert_eq!(quote_sql_identifier(r#"a"b"#), r#""a""b""#);
    }

    #[test]
    fn validate_column_name_allows_unicode_and_rejects_id() {
        assert!(validate_column_name("タイトル").is_ok());
        assert!(validate_column_name("id").is_err());
        assert!(validate_column_name("ID").is_err());
    }

    #[test]
    fn build_table_definition_quotes_multilingual_names() {
        let definition = build_table_definition(&[TableColumnInput {
            name: "タイトル".to_string(),
            type_key: "text".to_string(),
            nullable: false,
            orig_name: None,
        }])
        .unwrap();

        assert!(definition.contains(r#""タイトル" TEXT NOT NULL"#));
    }

    #[test]
    fn validate_view_definition_requires_select() {
        assert!(validate_view_definition("SELECT id FROM posts").is_ok());
        assert!(validate_view_definition("DELETE FROM posts").is_err());
    }

    #[test]
    fn validate_view_definition_allows_leading_comment() {
        assert!(validate_view_definition("-- notes view\nSELECT id FROM posts").is_ok());
        assert!(validate_view_definition("/* notes */ SELECT id FROM posts").is_ok());
        assert!(validate_view_definition(
            "-- DROP TABLE posts\nSELECT id FROM posts"
        )
        .is_ok());
    }

    #[test]
    fn build_table_definition_adds_id_and_columns() {
        let definition = build_table_definition(&[
            TableColumnInput {
                name: "title".to_string(),
                type_key: "text".to_string(),
                nullable: false,
                orig_name: None,
            },
            TableColumnInput {
                name: "score".to_string(),
                type_key: "real".to_string(),
                nullable: true,
                orig_name: None,
            },
        ])
        .unwrap();

        assert!(definition.starts_with("id INTEGER PRIMARY KEY"));
        assert!(definition.contains(r#""title" TEXT NOT NULL"#));
        assert!(definition.contains(r#""score" REAL"#));
        assert!(!definition.contains(r#""score" REAL NOT NULL"#));
    }

    #[test]
    fn build_table_definition_supports_all_sqlite_types() {
        let definition = build_table_definition(&[
            TableColumnInput {
                name: "payload".to_string(),
                type_key: "blob".to_string(),
                nullable: true,
                orig_name: None,
            },
            TableColumnInput {
                name: "created_at".to_string(),
                type_key: "timestamp".to_string(),
                nullable: false,
                orig_name: None,
            },
            TableColumnInput {
                name: "active".to_string(),
                type_key: "boolean".to_string(),
                nullable: false,
                orig_name: None,
            },
        ])
        .unwrap();

        assert!(definition.contains(r#""payload" BLOB"#));
        assert!(definition.contains(r#""created_at" TIMESTAMP NOT NULL"#));
        assert!(definition.contains(r#""active" BOOLEAN NOT NULL"#));

        let err = build_table_definition(&[TableColumnInput {
            name: "id".to_string(),
            type_key: "integer".to_string(),
            nullable: false,
            orig_name: None,
        }])
        .unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    fn sample_column(name: &str, type_key: &str, nullable: bool) -> TableColumnInput {
        TableColumnInput {
            name: name.to_string(),
            type_key: type_key.to_string(),
            nullable,
            orig_name: None,
        }
    }

    fn sample_column_with_orig(
        orig_name: &str,
        name: &str,
        type_key: &str,
        nullable: bool,
    ) -> TableColumnInput {
        TableColumnInput {
            name: name.to_string(),
            type_key: type_key.to_string(),
            nullable,
            orig_name: Some(orig_name.to_string()),
        }
    }

    #[test]
    fn plan_column_migration_detects_rename_add_and_drop() {
        let current = vec![
            sample_column("body", "text", false),
            sample_column("score", "real", true),
        ];
        let desired = vec![
            sample_column_with_orig("body", "title", "text", false),
            sample_column("memo", "text", true),
        ];

        let plan = plan_column_migration(&current, &desired).unwrap();
        assert_eq!(plan.renames, vec![("body".to_string(), "title".to_string())]);
        assert_eq!(plan.drops, vec!["score".to_string()]);
        assert_eq!(plan.adds.len(), 1);
        assert_eq!(plan.adds[0].name, "memo");
    }

    #[test]
    fn plan_column_migration_rejects_type_change() {
        let current = vec![sample_column("body", "text", false)];
        let desired = vec![sample_column_with_orig("body", "body", "integer", false)];

        let err = plan_column_migration(&current, &desired).unwrap_err();
        assert!(err.to_string().contains("型は変更できません"));
    }

    #[test]
    fn plan_column_migration_allows_not_null_to_nullable() {
        let current = vec![sample_column("body", "text", false)];
        let desired = vec![sample_column_with_orig("body", "body", "text", true)];

        let plan = plan_column_migration(&current, &desired).unwrap();
        assert_eq!(plan.nullable_relaxations, vec!["body".to_string()]);
        assert!(plan.renames.is_empty());
        assert!(plan.drops.is_empty());
        assert!(plan.adds.is_empty());
    }

    #[test]
    fn plan_column_migration_rejects_nullable_to_not_null() {
        let current = vec![sample_column("body", "text", true)];
        let desired = vec![sample_column_with_orig("body", "body", "text", false)];

        let err = plan_column_migration(&current, &desired).unwrap_err();
        assert!(err.to_string().contains("NOT NULL に変更することはできません"));
    }

    #[test]
    fn parse_user_columns_from_create_sql_extracts_columns() {
        let sql = r#"CREATE TABLE "notes" (id INTEGER PRIMARY KEY, "body" TEXT NOT NULL, "score" REAL, "payload" BLOB, "created_at" TIMESTAMP, "active" BOOLEAN)"#;
        let columns = parse_user_columns_from_create_sql(sql).unwrap();
        assert_eq!(columns.len(), 5);
        assert_eq!(columns[0].name, "body");
        assert_eq!(columns[0].type_key, "text");
        assert!(!columns[0].nullable);
        assert_eq!(columns[1].name, "score");
        assert_eq!(columns[1].type_key, "real");
        assert!(columns[1].nullable);
        assert_eq!(columns[2].type_key, "blob");
        assert_eq!(columns[3].type_key, "timestamp");
        assert_eq!(columns[4].type_key, "boolean");
    }

    #[tokio::test]
    async fn list_user_table_rows_supports_non_id_primary_key() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "_sqlx_test" (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");

        sqlx::query(
            r#"INSERT INTO "_sqlx_test"
               (version, description, success, checksum, execution_time)
               VALUES (1, 'init', 1, X'0102', 42)"#,
        )
        .execute(&pool)
        .await
        .expect("insert row");

        let view = list_user_table_rows(&pool, "_sqlx_test", 0, None, None)
            .await
            .expect("list rows");
        assert_eq!(
            view.columns,
            vec![
                "version".to_string(),
                "description".to_string(),
                "installed_on".to_string(),
                "success".to_string(),
                "checksum".to_string(),
                "execution_time".to_string(),
            ]
        );
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0][0], Some("1".to_string()));
        assert_eq!(view.rows[0][1], Some("init".to_string()));
        assert_eq!(view.rows[0][3], Some("1".to_string()));
        assert_eq!(view.rows[0][5], Some("42".to_string()));
        let meta = view.column_meta.expect("column meta");
        assert!(meta[0].pk);
        assert!(!meta[1].pk);
        assert_eq!(meta[0].type_key, "integer");
        assert_eq!(meta[1].type_key, "text");
    }

    #[tokio::test]
    async fn update_table_cell_updates_value() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "items" (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                score REAL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(r#"INSERT INTO "items" ("title", "score") VALUES ('old', 1.5)"#)
            .execute(&pool)
            .await
            .expect("insert row");

        let result = update_table_cell(
            &pool,
            "items",
            &TableCellUpdateRequest {
                column: "title".to_string(),
                value: "new".to_string(),
                null: false,
                keys: [("id".to_string(), "1".to_string())]
                    .into_iter()
                    .collect(),
            },
        )
        .await
        .expect("update cell");
        assert_eq!(result.value, Some("new".to_string()));

        let title: String = sqlx::query_scalar(r#"SELECT title FROM "items" WHERE id = 1"#)
            .fetch_one(&pool)
            .await
            .expect("fetch title");
        assert_eq!(title, "new");
    }

    #[tokio::test]
    async fn update_table_cell_accepts_datetime_local_format() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "events" (
                id INTEGER PRIMARY KEY,
                created_at TIMESTAMP NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(r#"INSERT INTO "events" ("created_at") VALUES ('2024-01-01T00:00:00')"#)
            .execute(&pool)
            .await
            .expect("insert row");

        let result = update_table_cell(
            &pool,
            "events",
            &TableCellUpdateRequest {
                column: "created_at".to_string(),
                value: "2024-06-15T14:30:00+09:00".to_string(),
                null: false,
                keys: [("id".to_string(), "1".to_string())]
                    .into_iter()
                    .collect(),
            },
        )
        .await
        .expect("update timestamp cell");
        assert_eq!(result.value, Some("2024-06-15T14:30:00+09:00".to_string()));

        let stored: String =
            sqlx::query_scalar(r#"SELECT created_at FROM "events" WHERE id = 1"#)
                .fetch_one(&pool)
                .await
                .expect("fetch timestamp");
        assert_eq!(stored, "2024-06-15T14:30:00+09:00");
    }

    #[tokio::test]
    async fn update_table_cell_normalizes_naive_timestamp_to_jst_offset() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "events_naive" (
                id INTEGER PRIMARY KEY,
                created_at TIMESTAMP NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(r#"INSERT INTO "events_naive" ("created_at") VALUES ('2024-01-01T00:00:00')"#)
            .execute(&pool)
            .await
            .expect("insert row");

        let result = update_table_cell(
            &pool,
            "events_naive",
            &TableCellUpdateRequest {
                column: "created_at".to_string(),
                value: "2024-06-15T14:30".to_string(),
                null: false,
                keys: [("id".to_string(), "1".to_string())]
                    .into_iter()
                    .collect(),
            },
        )
        .await
        .expect("update timestamp cell");
        assert_eq!(result.value, Some("2024-06-15T14:30:00+09:00".to_string()));
    }

    #[tokio::test]
    async fn list_user_table_rows_serializes_null_as_none() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "nullable_rows" (
                id INTEGER PRIMARY KEY,
                body TEXT,
                score INTEGER
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");
        sqlx::query(
            r#"INSERT INTO "nullable_rows" ("body", "score") VALUES ('', NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("insert row");

        let view = list_user_table_rows(&pool, "nullable_rows", 0, None, None)
            .await
            .expect("list rows");
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0][1], Some(String::new()));
        assert_eq!(view.rows[0][2], None);
    }

    #[tokio::test]
    async fn list_user_table_rows_limits_to_chunk_size() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(r#"CREATE TABLE "big" (id INTEGER PRIMARY KEY, "n" INTEGER NOT NULL)"#)
            .execute(&pool)
            .await
            .expect("create table");

        for index in 1..=1001 {
            sqlx::query(r#"INSERT INTO "big" ("n") VALUES (?)"#)
                .bind(index)
                .execute(&pool)
                .await
                .expect("insert row");
        }

        let view = list_user_table_rows(&pool, "big", 0, None, None)
            .await
            .expect("list rows");
        assert_eq!(view.columns, vec!["id".to_string(), "n".to_string()]);
        assert_eq!(view.rows.len(), 1000);
        assert_eq!(view.total_count, 1001);
        assert!(view.has_more);
    }

    #[test]
    fn parse_sort_query_param_parses_entries() {
        let entries = parse_sort_query_param("name:asc,age:desc").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].column, "name");
        assert_eq!(entries[0].direction, TableSortDirection::Asc);
        assert_eq!(entries[1].column, "age");
        assert_eq!(entries[1].direction, TableSortDirection::Desc);
    }

    #[test]
    fn parse_sort_query_param_empty_returns_empty_vec() {
        assert!(parse_sort_query_param("").unwrap().is_empty());
    }

    #[test]
    fn parse_sort_query_param_rejects_invalid_direction() {
        let err = parse_sort_query_param("name:up").unwrap_err();
        assert!(err.to_string().contains("asc または desc"));
    }

    #[tokio::test]
    async fn build_order_by_clause_uses_custom_sort() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(r#"CREATE TABLE "items" (id INTEGER PRIMARY KEY, "title" TEXT NOT NULL)"#)
            .execute(&pool)
            .await
            .expect("create table");

        let entries = vec![TableSortEntry {
            column: "title".to_string(),
            direction: TableSortDirection::Desc,
        }];
        let order_by = build_order_by_clause(&pool, "items", &entries)
            .await
            .expect("order by");
        assert_eq!(order_by, r#""title" DESC"#);
    }

    #[tokio::test]
    async fn list_user_table_rows_sorts_rows() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(r#"CREATE TABLE "sorted" (id INTEGER PRIMARY KEY, "label" TEXT NOT NULL)"#)
            .execute(&pool)
            .await
            .expect("create table");

        for label in ["c", "a", "b"] {
            sqlx::query(r#"INSERT INTO "sorted" ("label") VALUES (?)"#)
                .bind(label)
                .execute(&pool)
                .await
                .expect("insert row");
        }

        let sort = vec![TableSortEntry {
            column: "label".to_string(),
            direction: TableSortDirection::Asc,
        }];
        let view = list_user_table_rows(&pool, "sorted", 0, Some(&sort), None)
            .await
            .expect("list rows");
        assert_eq!(
            view.rows
                .iter()
                .map(|row| row[1].as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a"), Some("b"), Some("c")]
        );
        assert_eq!(view.sort, Some(sort));
    }

    #[test]
    fn parse_filter_query_param_parses_entries() {
        let entries = parse_filter_query_param("name:hello,age:42").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].column, "name");
        assert_eq!(entries[0].text, "hello");
        assert_eq!(entries[1].column, "age");
        assert_eq!(entries[1].text, "42");
    }

    #[test]
    fn parse_filter_query_param_decodes_percent_encoding() {
        let entries = parse_filter_query_param("body:hello%2Cworld").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].column, "body");
        assert_eq!(entries[0].text, "hello,world");
    }

    #[test]
    fn parse_filter_query_param_empty_returns_empty_vec() {
        assert!(parse_filter_query_param("").unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_user_table_rows_filters_rows() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "filtered" (
                id INTEGER PRIMARY KEY,
                "label" TEXT NOT NULL,
                "score" INTEGER NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");

        let rows = [("alpha", 10), ("beta", 20), ("alphabet", 30)];
        for (label, score) in rows {
            sqlx::query(r#"INSERT INTO "filtered" ("label", "score") VALUES (?, ?)"#)
                .bind(label)
                .bind(score)
                .execute(&pool)
                .await
                .expect("insert row");
        }

        let filter = vec![TableFilterEntry {
            column: "label".to_string(),
            text: "ta".to_string(),
        }];
        let view = list_user_table_rows(&pool, "filtered", 0, None, Some(&filter))
            .await
            .expect("list rows");
        assert_eq!(view.total_count, 1);
        assert_eq!(view.rows[0][1].as_deref(), Some("beta"));

        let multi_filter = vec![
            TableFilterEntry {
                column: "label".to_string(),
                text: "a".to_string(),
            },
            TableFilterEntry {
                column: "score".to_string(),
                text: "2".to_string(),
            },
        ];
        let view = list_user_table_rows(&pool, "filtered", 0, None, Some(&multi_filter))
            .await
            .expect("list rows");
        assert_eq!(view.total_count, 1);
        assert_eq!(view.rows[0][1].as_deref(), Some("beta"));
    }

    #[tokio::test]
    async fn list_user_table_rows_filter_excludes_null_cells() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "nullable_filter" (
                id INTEGER PRIMARY KEY,
                body TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create table");

        sqlx::query(r#"INSERT INTO "nullable_filter" ("body") VALUES ('match'), (NULL)"#)
            .execute(&pool)
            .await
            .expect("insert rows");

        let filter = vec![TableFilterEntry {
            column: "body".to_string(),
            text: "match".to_string(),
        }];
        let view = list_user_table_rows(&pool, "nullable_filter", 0, None, Some(&filter))
            .await
            .expect("list rows");
        assert_eq!(view.total_count, 1);
    }

    #[test]
    fn parse_seed_form_validates_integer_range() {
        let columns = vec![TableColumnInput {
            name: "score".to_string(),
            type_key: "integer".to_string(),
            nullable: true,
            orig_name: None,
        }];
        let form = TestDataSeedForm {
            count: "10".to_string(),
            col_name: vec!["score".to_string()],
            col_type: vec!["integer".to_string()],
            col_int_min: vec!["100".to_string()],
            col_int_max: vec!["10".to_string()],
            ..Default::default()
        };
        let err = parse_seed_form(&columns, &form).unwrap_err();
        assert!(err.to_string().contains("最小値"));

        let form = TestDataSeedForm {
            col_int_min: vec!["0".to_string()],
            col_int_max: vec!["100".to_string()],
            ..form
        };
        let (count, rules) = parse_seed_form(&columns, &form).unwrap();
        assert_eq!(count, 10);
        assert_eq!(
            rules[0].1,
            ColumnSeedRule::Integer {
                min: 0,
                max: 100,
                include_null: false,
            }
        );
    }

    #[tokio::test]
    async fn generate_test_data_reports_progress() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(r#"CREATE TABLE "progress" (id INTEGER PRIMARY KEY, "body" TEXT NOT NULL)"#)
            .execute(&pool)
            .await
            .expect("create table");

        let rules = vec![(
            "body".to_string(),
            ColumnSeedRule::Text {
                min_len: 4,
                max_len: 4,
                charset: StringCharset::AsciiAlnum,
                include_null: false,
            },
        )];
        let mut progress = Vec::new();
        generate_test_data(
            &pool,
            "progress",
            250,
            &rules,
            Some(|done, total| {
                progress.push((done, total));
            }),
            None::<fn() -> bool>,
        )
        .await
        .expect("generate");

        assert!(!progress.is_empty());
        assert_eq!(progress.last().copied(), Some((250, 250)));
        assert!(progress.iter().all(|(_, total)| *total == 250));
        assert!(progress.windows(2).all(|window| window[0].0 < window[1].0));

        let count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM "progress""#)
            .fetch_one(&pool)
            .await
            .expect("count rows");
        assert_eq!(count.0, 250);
    }

    #[tokio::test]
    async fn generate_test_data_can_be_cancelled() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(r#"CREATE TABLE "cancel" (id INTEGER PRIMARY KEY, "body" TEXT NOT NULL)"#)
            .execute(&pool)
            .await
            .expect("create table");

        let rules = vec![(
            "body".to_string(),
            ColumnSeedRule::Text {
                min_len: 4,
                max_len: 4,
                charset: StringCharset::AsciiAlnum,
                include_null: false,
            },
        )];
        let cancel_after = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_after_worker = std::sync::Arc::clone(&cancel_after);
        let cancel_flag_worker = std::sync::Arc::clone(&cancel_flag);

        let err = generate_test_data(
            &pool,
            "cancel",
            500,
            &rules,
            Some(move |done, _| {
                if done >= 50 {
                    cancel_flag_worker.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                cancel_after_worker.store(done, std::sync::atomic::Ordering::Relaxed);
            }),
            Some(move || cancel_flag.load(std::sync::atomic::Ordering::Relaxed)),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("キャンセル"));

        let count: (i64,) = sqlx::query_as(r#"SELECT COUNT(*) FROM "cancel""#)
            .fetch_one(&pool)
            .await
            .expect("count rows");
        assert_eq!(count.0, 0);
        assert!(cancel_after.load(std::sync::atomic::Ordering::Relaxed) >= 50);
    }

    #[tokio::test]
    async fn generate_test_data_supports_blob_and_boolean() {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("connect memory db");
        sqlx::query(
            r#"CREATE TABLE "mixed" (id INTEGER PRIMARY KEY, "payload" BLOB NOT NULL, "active" BOOLEAN NOT NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("create table");

        let rules = vec![
            (
                "payload".to_string(),
                ColumnSeedRule::Blob {
                    min_len: 4,
                    max_len: 8,
                    include_null: false,
                },
            ),
            (
                "active".to_string(),
                ColumnSeedRule::Boolean { include_null: false },
            ),
        ];
        generate_test_data(
            &pool,
            "mixed",
            10,
            &rules,
            None::<fn(u32, u32)>,
            None::<fn() -> bool>,
        )
        .await
        .expect("generate");

        let rows: Vec<(Vec<u8>, i64)> = sqlx::query_as(r#"SELECT "payload", "active" FROM "mixed""#)
            .fetch_all(&pool)
            .await
            .expect("fetch rows");
        assert_eq!(rows.len(), 10);
        for (payload, active) in rows {
            assert!((4..=8).contains(&payload.len()));
            assert!(active == 0 || active == 1);
        }
    }

    #[test]
    fn truncate_sql_shortens_long_definitions() {
        let sql = "CREATE VIEW example AS SELECT id, title FROM posts WHERE status = 'publish'";
        assert_eq!(truncate_sql(sql, 80), sql);
        assert!(truncate_sql(sql, 20).ends_with('…'));
        assert!(truncate_sql(sql, 20).chars().count() <= 21);
    }

    #[test]
    fn extract_view_select_from_create_sql_parses_quoted_view_name() {
        let sql = r#"CREATE VIEW "custom_notes_view" AS SELECT id, body FROM custom_notes"#;
        let definition = extract_view_select_from_create_sql(sql).expect("extract definition");
        assert_eq!(definition, "SELECT id, body FROM custom_notes");
    }

    #[test]
    fn extract_view_select_from_create_sql_rejects_missing_as_clause() {
        let err = extract_view_select_from_create_sql("CREATE VIEW broken").unwrap_err();
        assert!(matches!(err, DomainError::NotFound));
    }

    #[test]
    fn parse_simple_view_select_parses_column_list() {
        let parsed = parse_simple_view_select("SELECT id, body FROM custom_notes").expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[0].alias.is_none()
                    && columns[1].name == "body"
                    && columns[1].alias.is_none()
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_quoted_identifiers() {
        let parsed =
            parse_simple_view_select(r#"SELECT "id", "body" FROM "custom_notes""#).expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[1].name == "body"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_column_aliases() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id" AS "user_id", "body" FROM "custom_notes""#,
        )
        .expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[0].alias.as_deref() == Some("user_id")
                    && columns[1].name == "body"
                    && columns[1].alias.is_none()
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_duplicate_columns_with_alias() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id", "id" AS "id2" FROM "custom_notes""#,
        )
        .expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[0].alias.is_none()
                    && columns[1].name == "id"
                    && columns[1].alias.as_deref() == Some("id2")
        ));
    }

    #[test]
    fn parse_simple_view_select_rejects_duplicate_output_names() {
        assert!(parse_simple_view_select(r#"SELECT "id", "id" FROM "custom_notes""#).is_none());
    }

    #[test]
    fn parse_simple_view_select_parses_star() {
        let parsed = parse_simple_view_select("SELECT * FROM posts").expect("parsed");
        assert_eq!(parsed.base_table, "posts");
        assert!(matches!(parsed.columns, ParsedViewColumnList::All));
    }

    #[test]
    fn parse_simple_view_select_rejects_join() {
        assert!(parse_simple_view_select("SELECT id FROM posts JOIN users ON 1 = 1").is_none());
    }

    #[test]
    fn parse_simple_view_select_parses_leading_line_comment() {
        let parsed = parse_simple_view_select(
            "-- notes view\nSELECT \"id\", \"body\" FROM \"custom_notes\"",
        )
        .expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[1].name == "body"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_leading_block_comment() {
        let parsed =
            parse_simple_view_select("/* notes */ SELECT \"id\" FROM \"custom_notes\"")
                .expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
    }

    #[test]
    fn parse_simple_view_select_parses_inline_select_comment() {
        let parsed = parse_simple_view_select(
            "SELECT \"id\" -- id column\n, \"body\" FROM \"custom_notes\"",
        )
        .expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name == "id"
                    && columns[1].name == "body"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_distinct_with_block_comment() {
        let parsed = parse_simple_view_select(
            "SELECT /* cols */ DISTINCT \"id\" FROM \"custom_notes\" WHERE \"body\" IS NOT NULL",
        )
        .expect("parsed");
        assert!(parsed.distinct);
        assert_eq!(
            parsed.where_conditions,
            vec![ParsedWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
            }]
        );
    }

    #[test]
    fn parse_simple_view_select_preserves_dash_in_quoted_identifier() {
        let parsed =
            parse_simple_view_select(r#"SELECT "col--name" FROM "custom_notes""#).expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 1 && columns[0].name == "col--name"
        ));
    }

    #[test]
    fn parse_simple_view_select_rejects_join_with_comment() {
        assert!(parse_simple_view_select(
            "-- join view\nSELECT id FROM posts JOIN users ON 1 = 1"
        )
        .is_none());
    }

    #[test]
    fn parse_simple_view_select_parses_simple_where() {
        let parsed =
            parse_simple_view_select(r#"SELECT "id" FROM "join_src" WHERE "body" IS NOT NULL"#)
                .expect("parsed");
        assert_eq!(parsed.base_table, "join_src");
        assert_eq!(
            parsed.where_conditions,
            vec![ParsedWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
            }]
        );
    }

    #[test]
    fn parse_simple_view_select_parses_multiple_where_conditions() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id", "body" FROM "custom_notes" WHERE "body" IS NOT NULL AND "id" > 0"#,
        )
        .expect("parsed");
        assert_eq!(
            parsed.where_conditions,
            vec![
                ParsedWhereCondition {
                    column: "body".to_string(),
                    suffix: "IS NOT NULL".to_string(),
                },
                ParsedWhereCondition {
                    column: "id".to_string(),
                    suffix: "> 0".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_simple_view_select_parses_duplicate_column_where_conditions() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id", "id" AS "id2" FROM "custom_notes" WHERE "id" > 0 AND "id" IS NOT NULL"#,
        )
        .expect("parsed");
        assert_eq!(
            parsed.where_conditions,
            vec![
                ParsedWhereCondition {
                    column: "id".to_string(),
                    suffix: "> 0".to_string(),
                },
                ParsedWhereCondition {
                    column: "id".to_string(),
                    suffix: "IS NOT NULL".to_string(),
                },
            ]
        );
    }

    #[test]
    fn build_simple_view_ui_spec_assigns_where_per_select_row() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id", "id" AS "id2", "body" FROM "custom_notes" WHERE "id" > 0 AND "id" IS NOT NULL"#,
        )
        .expect("parsed");
        let table_columns = vec![
            TableColumnUiInfo {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                pk: true,
                nullable: false,
            },
            TableColumnUiInfo {
                name: "body".to_string(),
                type_key: "text".to_string(),
                pk: false,
                nullable: false,
            },
        ];

        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert_eq!(spec.columns.len(), 3);
        assert_eq!(
            spec.columns[0].where_condition.as_deref(),
            Some("> 0")
        );
        assert_eq!(
            spec.columns[1].where_condition.as_deref(),
            Some("IS NOT NULL")
        );
        assert!(spec.columns[2].where_condition.is_none());
    }

    #[test]
    fn parse_simple_view_select_rejects_complex_where() {
        assert!(parse_simple_view_select(
            "SELECT id FROM posts WHERE id = 1 OR body IS NOT NULL"
        )
        .is_none());
    }

    #[test]
    fn build_simple_view_select_generates_quoted_sql() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![SimpleViewUiColumn {
                name: "body".to_string(),
                type_key: "text".to_string(),
                alias: None,
                expression: None,
                where_condition: None,
            }],
        };

        let sql = build_simple_view_select(&spec).expect("sql");
        assert_eq!(sql, r#"SELECT "body" FROM "custom_notes""#);
    }

    #[test]
    fn build_simple_view_select_generates_column_alias() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![SimpleViewUiColumn {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                alias: Some("user_id".to_string()),
                expression: None,
                where_condition: None,
            }],
        };

        let sql = build_simple_view_select(&spec).expect("sql");
        assert_eq!(sql, r#"SELECT "id" AS "user_id" FROM "custom_notes""#);
    }

    #[test]
    fn build_simple_view_select_rejects_duplicate_output_names() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![
                SimpleViewUiColumn {
                    name: "id".to_string(),
                    type_key: "integer".to_string(),
                    alias: None,
                    expression: None,
                    where_condition: None,
                },
                SimpleViewUiColumn {
                    name: "id".to_string(),
                    type_key: "integer".to_string(),
                    alias: None,
                    expression: None,
                    where_condition: None,
                },
            ],
        };

        let err = build_simple_view_select(&spec).unwrap_err();
        assert!(matches!(err, DomainError::Validation(ref msg) if msg.contains("出力列名が重複")));
    }

    #[test]
    fn build_simple_view_select_generates_where_clause() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![
                SimpleViewUiColumn {
                    name: "id".to_string(),
                    type_key: "integer".to_string(),
                    alias: None,
                    expression: None,
                    where_condition: Some("> 0".to_string()),
                },
                SimpleViewUiColumn {
                    name: "body".to_string(),
                    type_key: "text".to_string(),
                    alias: None,
                    expression: None,
                    where_condition: Some("IS NOT NULL".to_string()),
                },
            ],
        };

        let sql = build_simple_view_select(&spec).expect("sql");
        assert_eq!(
            sql,
            r#"SELECT "id", "body" FROM "custom_notes" WHERE "id" > 0 AND "body" IS NOT NULL"#
        );
    }

    #[test]
    fn build_simple_view_select_requires_column() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![],
        };
        assert!(build_simple_view_select(&spec).is_err());
    }

    #[test]
    fn parse_simple_view_select_parses_expression_with_alias() {
        let parsed = parse_simple_view_select(
            r#"SELECT LENGTH("body") AS "body_len", "id" FROM "custom_notes""#,
        )
        .expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 2
                    && columns[0].name.is_empty()
                    && columns[0].expression.as_deref() == Some(r#"LENGTH("body")"#)
                    && columns[0].alias.as_deref() == Some("body_len")
                    && columns[1].name == "id"
                    && columns[1].expression.is_none()
        ));
    }

    #[test]
    fn build_simple_view_select_generates_expression_column() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![
                SimpleViewUiColumn {
                    name: String::new(),
                    type_key: "text".to_string(),
                    alias: Some("body_len".to_string()),
                    expression: Some(r#"LENGTH("body")"#.to_string()),
                    where_condition: None,
                },
                SimpleViewUiColumn {
                    name: "id".to_string(),
                    type_key: "integer".to_string(),
                    alias: None,
                    expression: None,
                    where_condition: None,
                },
            ],
        };

        let sql = build_simple_view_select(&spec).expect("sql");
        assert_eq!(
            sql,
            r#"SELECT LENGTH("body") AS "body_len", "id" FROM "custom_notes""#
        );
    }

    #[test]
    fn build_simple_view_ui_spec_restores_expression_column() {
        let parsed = parse_simple_view_select(
            r#"SELECT LENGTH("body") AS "body_len" FROM "custom_notes""#,
        )
        .expect("parsed");
        let table_columns = vec![TableColumnUiInfo {
            name: "body".to_string(),
            type_key: "text".to_string(),
            pk: false,
            nullable: false,
        }];

        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert_eq!(spec.columns.len(), 1);
        assert_eq!(
            spec.columns[0].expression.as_deref(),
            Some(r#"LENGTH("body")"#)
        );
        assert_eq!(spec.columns[0].alias.as_deref(), Some("body_len"));
        assert!(spec.columns[0].where_condition.is_none());
    }

    #[test]
    fn build_simple_view_select_rejects_duplicate_expression_output_names() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![
                SimpleViewUiColumn {
                    name: String::new(),
                    type_key: "text".to_string(),
                    alias: None,
                    expression: Some(r#"LENGTH("body")"#.to_string()),
                    where_condition: None,
                },
                SimpleViewUiColumn {
                    name: String::new(),
                    type_key: "text".to_string(),
                    alias: None,
                    expression: Some(r#"LENGTH("body")"#.to_string()),
                    where_condition: None,
                },
            ],
        };

        let err = build_simple_view_select(&spec).unwrap_err();
        assert!(matches!(err, DomainError::Validation(ref msg) if msg.contains("出力列名が重複")));
    }

    #[test]
    fn build_simple_view_select_rejects_forbidden_keyword_in_expression() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: false,
            extra_where: vec![],
            columns: vec![SimpleViewUiColumn {
                name: String::new(),
                type_key: "text".to_string(),
                alias: None,
                expression: Some("DELETE FROM custom_notes".to_string()),
                where_condition: None,
            }],
        };

        let err = build_simple_view_select(&spec).unwrap_err();
        assert!(matches!(err, DomainError::Validation(ref msg) if msg.contains("式")));
    }

    #[test]
    fn parse_simple_view_select_parses_distinct() {
        let parsed =
            parse_simple_view_select(r#"SELECT DISTINCT "id" FROM "custom_notes""#).expect("parsed");
        assert!(parsed.distinct);
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns) if columns.len() == 1 && columns[0].name == "id"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_table_alias() {
        let parsed =
            parse_simple_view_select(r#"SELECT "id" FROM "custom_notes" AS "n""#).expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns) if columns.len() == 1 && columns[0].name == "id"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_implicit_table_alias() {
        let parsed = parse_simple_view_select(r#"SELECT n."id" FROM "custom_notes" n"#).expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns) if columns.len() == 1 && columns[0].name == "id"
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_schema_qualified_table() {
        let parsed =
            parse_simple_view_select(r#"SELECT "id" FROM main."custom_notes""#).expect("parsed");
        assert_eq!(parsed.base_table, "custom_notes");
    }

    #[test]
    fn parse_simple_view_select_parses_implicit_column_alias() {
        let parsed =
            parse_simple_view_select(r#"SELECT "id" user_id FROM "custom_notes""#).expect("parsed");
        assert!(matches!(
            parsed.columns,
            ParsedViewColumnList::Columns(ref columns)
                if columns.len() == 1
                    && columns[0].name == "id"
                    && columns[0].alias.as_deref() == Some("user_id")
        ));
    }

    #[test]
    fn parse_simple_view_select_parses_qualified_where() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id" FROM "custom_notes" n WHERE n."body" IS NOT NULL"#,
        )
        .expect("parsed");
        assert_eq!(
            parsed.where_conditions,
            vec![ParsedWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
            }]
        );
    }

    #[test]
    fn build_simple_view_ui_spec_splits_extra_where() {
        let parsed = parse_simple_view_select(
            r#"SELECT "id" FROM "custom_notes" WHERE "body" IS NOT NULL"#,
        )
        .expect("parsed");
        let table_columns = vec![
            TableColumnUiInfo {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                pk: true,
                nullable: false,
            },
            TableColumnUiInfo {
                name: "body".to_string(),
                type_key: "text".to_string(),
                pk: false,
                nullable: false,
            },
        ];

        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert_eq!(spec.columns.len(), 1);
        assert!(spec.columns[0].where_condition.is_none());
        assert_eq!(
            spec.extra_where,
            vec![ExtraWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
            }]
        );
    }

    #[test]
    fn build_simple_view_select_generates_distinct_and_extra_where() {
        let spec = SimpleViewUiSpec {
            base_table: "custom_notes".to_string(),
            distinct: true,
            extra_where: vec![ExtraWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
            }],
            columns: vec![SimpleViewUiColumn {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                alias: None,
                expression: None,
                where_condition: None,
            }],
        };

        let sql = build_simple_view_select(&spec).expect("sql");
        assert_eq!(
            sql,
            r#"SELECT DISTINCT "id" FROM "custom_notes" WHERE "body" IS NOT NULL"#
        );
    }

    #[test]
    fn filter_sort_entries_to_columns_drops_missing_columns() {
        let filtered = filter_sort_entries_to_columns(
            &["user_id".to_string()],
            &[TableSortEntry {
                column: "body".to_string(),
                direction: TableSortDirection::Asc,
            }],
        );
        assert!(filtered.is_empty());
    }

    #[test]
    fn build_simple_view_ui_spec_returns_only_selected_columns() {
        let parsed = parse_simple_view_select("SELECT body FROM custom_notes").expect("parsed");
        let table_columns = vec![
            TableColumnUiInfo {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                pk: true,
                nullable: false,
            },
            TableColumnUiInfo {
                name: "body".to_string(),
                type_key: "text".to_string(),
                pk: false,
                nullable: false,
            },
        ];

        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert_eq!(spec.base_table, "custom_notes");
        assert_eq!(spec.columns.len(), 1);
        assert_eq!(spec.columns[0].name, "body");
        assert!(spec.columns[0].where_condition.is_none());
    }
}
