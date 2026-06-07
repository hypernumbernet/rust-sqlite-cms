//! データベーススキーマのイントロスペクションとユーザー定義オブジェクトの管理。

use chrono::{Duration, Local, NaiveDateTime};
use rand::rngs::OsRng;
use rand::Rng;
use serde::Deserialize;
use sqlx::{Row, SqlitePool};

use crate::error::{AppError, AppResult, DomainError, DomainResult};

/// テーブルデータ一覧の1回あたりの取得行数。
pub const TABLE_DATA_CHUNK_SIZE: i64 = 1000;

/// テストデータ生成の1回あたりの最大件数。
pub const MAX_TEST_DATA_ROWS: u32 = 100_000;

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
}

/// ユーザーテーブルのデータ一覧（1チャンク分）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableDataView {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_count: i64,
    pub offset: i64,
    pub has_more: bool,
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
];

/// 管理画面 DB 一覧に表示しないインフラ用テーブル。
const HIDDEN_ADMIN_TABLES: &[&str] = &["_sqlx_migrations"];

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

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        if is_hidden_admin_table(&row.name) {
            continue;
        }
        let sql = row.sql.unwrap_or_default();
        let row_count = table_row_count(pool, &row.name).await?;
        items.push(DbObjectItem {
            name: row.name.clone(),
            sql_preview: truncate_sql(&sql, 80),
            is_system: is_system_table(&row.name),
            row_count: Some(row_count),
            sql,
        });
    }
    Ok(items)
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
    let name = ensure_viewable_table(name)?;
    if !object_name_exists(pool, name, "table").await? {
        return Err(DomainError::NotFound);
    }
    Ok(())
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
    apply_column_migration(pool, name, &plan).await?;
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
        if existing.nullable != column.nullable {
            return Err(DomainError::Validation(format!(
                "列 `{orig_name}` の NULL 設定は変更できません"
            )));
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
) -> DomainResult<()> {
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

    let ddl = format!(
        "CREATE VIEW {} AS {definition}",
        quote_sql_identifier(&name)
    );
    execute_ddl(pool, &ddl).await?;
    Ok(())
}

fn validate_view_definition(definition: &str) -> DomainResult<String> {
    let definition = definition.trim();
    if definition.is_empty() {
        return Err(DomainError::Validation("SELECT 文は必須です".into()));
    }
    if !definition.to_ascii_uppercase().starts_with("SELECT") {
        return Err(DomainError::Validation(
            "ビュー定義は SELECT で始めてください".into(),
        ));
    }
    validate_ddl_fragment(definition, "SELECT 文")?;
    Ok(definition.to_string())
}

fn validate_ddl_fragment(fragment: &str, label: &str) -> DomainResult<()> {
    if fragment.contains(';') {
        return Err(DomainError::Validation(format!(
            "{label}にセミコロンは使用できません"
        )));
    }
    if contains_forbidden_keyword(fragment) {
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

pub async fn list_user_table_rows(
    pool: &SqlitePool,
    name: &str,
    offset: i64,
) -> DomainResult<TableDataView> {
    let name = ensure_viewable_table(name)?;
    if !object_name_exists(pool, name, "table").await? {
        return Err(DomainError::NotFound);
    }
    if offset < 0 {
        return Err(DomainError::Validation(
            "offset は 0 以上で指定してください".into(),
        ));
    }

    let columns = table_column_names(pool, name).await?;
    let total_count = table_row_count(pool, name).await.map_err(DomainError::from)?;
    let rows =
        fetch_table_rows_chunk(pool, name, offset, TABLE_DATA_CHUNK_SIZE, columns.len()).await?;
    let fetched = rows.len() as i64;
    let has_more = offset + fetched < total_count;

    Ok(TableDataView {
        columns,
        rows,
        total_count,
        offset,
        has_more,
    })
}

pub async fn fetch_table_rows_chunk(
    pool: &SqlitePool,
    name: &str,
    offset: i64,
    limit: i64,
    column_count: usize,
) -> DomainResult<Vec<Vec<String>>> {
    let name = ensure_viewable_table(name)?;
    if offset < 0 || limit < 0 {
        return Err(DomainError::Validation(
            "offset と limit は 0 以上で指定してください".into(),
        ));
    }

    let quoted = quote_sql_identifier(name);
    let order_by = table_order_columns(pool, name).await?;
    let query = format!("SELECT * FROM {quoted} ORDER BY {order_by} LIMIT {limit} OFFSET {offset}");
    let sql_rows = sqlx::query(sqlx::AssertSqlSafe(query))
        .fetch_all(pool)
        .await?;

    Ok(sql_rows
        .iter()
        .map(|row| row_to_cells(row, column_count))
        .collect())
}

async fn table_pragma_info(pool: &SqlitePool, table_name: &str) -> DomainResult<Vec<PragmaTableInfoRow>> {
    let quoted = quote_sql_identifier(table_name);
    let query = format!("PRAGMA table_info({quoted})");
    let rows = sqlx::query_as::<_, PragmaTableInfoRow>(sqlx::AssertSqlSafe(query))
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

async fn table_column_names(pool: &SqlitePool, table_name: &str) -> DomainResult<Vec<String>> {
    let rows = table_pragma_info(pool, table_name).await?;
    Ok(rows.into_iter().map(|row| row.name).collect())
}

async fn table_order_columns(pool: &SqlitePool, table_name: &str) -> DomainResult<String> {
    let rows = table_pragma_info(pool, table_name).await?;

    let mut pk_columns: Vec<(i32, String)> = rows
        .iter()
        .filter(|row| row.pk > 0)
        .map(|row| (row.pk, row.name.clone()))
        .collect();
    if !pk_columns.is_empty() {
        pk_columns.sort_by_key(|(pk, _)| *pk);
        return Ok(pk_columns
            .into_iter()
            .map(|(_, name)| quote_sql_identifier(&name))
            .collect::<Vec<_>>()
            .join(", "));
    }

    if rows
        .iter()
        .any(|row| row.name.eq_ignore_ascii_case("id"))
    {
        return Ok(quote_sql_identifier("id"));
    }

    if let Some(first) = rows.first() {
        return Ok(quote_sql_identifier(&first.name));
    }

    Ok("rowid".to_string())
}

fn row_to_cells(row: &sqlx::sqlite::SqliteRow, column_count: usize) -> Vec<String> {
    (0..column_count)
        .map(|index| format_sqlite_cell(row, index))
        .collect()
}

fn format_sqlite_cell(row: &sqlx::sqlite::SqliteRow, index: usize) -> String {
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(|v| v.to_string()).unwrap_or_default();
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value.map(|v| v.to_string()).unwrap_or_default();
    }
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.unwrap_or_default();
    }
    String::new()
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

pub async fn generate_test_data(
    pool: &SqlitePool,
    table_name: &str,
    count: u32,
    rules: &[(String, ColumnSeedRule)],
) -> DomainResult<u32> {
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

    if rules.is_empty() {
        let quoted_table = quote_sql_identifier(table_name);
        let query = format!("INSERT INTO {quoted_table} DEFAULT VALUES");
        for _ in 0..count {
            sqlx::query(sqlx::AssertSqlSafe(query.clone()))
                .execute(&mut *tx)
                .await?;
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

        for _ in 0..count {
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
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .map_err(|_| DomainError::Validation(format!("{label}は YYYY-MM-DDTHH:MM 形式で指定してください")))
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

async fn table_row_count(pool: &SqlitePool, name: &str) -> AppResult<i64> {
    let quoted = quote_sql_identifier(name);
    let query = format!("SELECT COUNT(*) FROM {quoted}");
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
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
        assert!(!is_system_table("my_custom_table"));
    }

    #[test]
    fn is_cms_core_table_includes_users() {
        assert!(is_cms_core_table("posts"));
        assert!(is_cms_core_table("users"));
        assert!(!is_cms_core_table("my_custom_table"));
    }

    #[test]
    fn is_cms_readonly_table_covers_all_cms_core_tables() {
        assert!(is_cms_readonly_table("posts"));
        assert!(is_cms_readonly_table("users"));
        assert!(!is_cms_readonly_table("my_custom_table"));
    }

    #[test]
    fn is_db_admin_editable_allows_only_user_defined_tables() {
        assert!(!is_db_admin_editable("posts"));
        assert!(!is_db_admin_editable("users"));
        assert!(!is_db_admin_editable("_sqlx_migrations"));
        assert!(is_db_admin_editable("my_custom_table"));
    }

    #[test]
    fn is_db_admin_data_viewable_blocks_infra_only() {
        assert!(!is_db_admin_data_viewable("_sqlx_migrations"));
        assert!(!is_db_admin_data_viewable("sqlite_sequence"));
        assert!(is_db_admin_data_viewable("posts"));
        assert!(is_db_admin_data_viewable("users"));
        assert!(is_db_admin_data_viewable("my_custom_table"));
    }

    #[test]
    fn is_hidden_admin_table_detects_infra_tables() {
        assert!(is_hidden_admin_table("_sqlx_migrations"));
        assert!(!is_hidden_admin_table("posts"));
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
    fn plan_column_migration_rejects_nullable_change() {
        let current = vec![sample_column("body", "text", false)];
        let desired = vec![sample_column_with_orig("body", "body", "text", true)];

        let err = plan_column_migration(&current, &desired).unwrap_err();
        assert!(err.to_string().contains("NULL 設定は変更できません"));
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

        let view = list_user_table_rows(&pool, "_sqlx_test", 0)
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
        assert_eq!(view.rows[0][0], "1");
        assert_eq!(view.rows[0][1], "init");
        assert_eq!(view.rows[0][3], "1");
        assert_eq!(view.rows[0][5], "42");
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

        let view = list_user_table_rows(&pool, "big", 0)
            .await
            .expect("list rows");
        assert_eq!(view.columns, vec!["id".to_string(), "n".to_string()]);
        assert_eq!(view.rows.len(), 1000);
        assert_eq!(view.total_count, 1001);
        assert!(view.has_more);
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
        generate_test_data(&pool, "mixed", 10, &rules)
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
}
