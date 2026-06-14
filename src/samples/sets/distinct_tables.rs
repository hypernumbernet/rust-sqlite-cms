//! 「DISTINCT デモ（部署・オフィス）」DBテーブルサンプルセット。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::samples::conflict;
use crate::services::database::{self, TableColumnInput};
use crate::state::AppState;

const TABLE_EMPLOYEES: &str = "distinct_employees";
const VIEW_DEPARTMENTS: &str = "distinct_departments";
const VIEW_OFFICES: &str = "distinct_offices";
const VIEW_DEPARTMENT_OFFICES: &str = "distinct_department_offices";

const TABLE_NAMES: &[&str] = &[TABLE_EMPLOYEES];
const VIEW_NAMES: &[&str] = &[VIEW_DEPARTMENTS, VIEW_OFFICES, VIEW_DEPARTMENT_OFFICES];
const SAMPLE_ROW_COUNT: i64 = 14;

/// DISTINCT デモサンプルをインストールする。
pub async fn install(state: &AppState) -> AppResult<super::super::InstallResult> {
    let pool = state.pool();

    if let Err(conflicts) = check_conflicts(&pool).await {
        return Err(conflict::abort(conflicts));
    }

    database::create_user_table_from_columns(&pool, TABLE_EMPLOYEES, &employee_columns())
        .await?;

    insert_employee_rows(&pool).await?;

    database::create_user_view(
        &pool,
        VIEW_DEPARTMENTS,
        r#"
SELECT DISTINCT department
FROM distinct_employees
ORDER BY department
        "#,
    )
    .await?;

    database::create_user_view(
        &pool,
        VIEW_OFFICES,
        r#"
SELECT DISTINCT office
FROM distinct_employees
ORDER BY office
        "#,
    )
    .await?;

    database::create_user_view(
        &pool,
        VIEW_DEPARTMENT_OFFICES,
        r#"
SELECT DISTINCT department, office
FROM distinct_employees
ORDER BY department, office
        "#,
    )
    .await?;

    Ok(super::super::InstallResult::Tables {
        message: "DISTINCT デモ（部署・オフィス）のサンプルをインストールしました。".to_string(),
        tables_count: TABLE_NAMES.len() as i64,
        views_count: VIEW_NAMES.len() as i64,
        rows_count: SAMPLE_ROW_COUNT,
    })
}

async fn check_conflicts(pool: &SqlitePool) -> Result<(), Vec<String>> {
    let mut conflicts = Vec::new();

    for name in TABLE_NAMES {
        if conflict::sqlite_object_exists(pool, "table", name)
            .await
            .map_err(|e| vec![e])?
        {
            conflicts.push(format!(r#"テーブル "{}""#, name));
        }
    }

    for name in VIEW_NAMES {
        if conflict::sqlite_object_exists(pool, "view", name)
            .await
            .map_err(|e| vec![e])?
        {
            conflicts.push(format!(r#"ビュー "{}""#, name));
        }
    }

    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(conflicts)
    }
}

fn col(name: &str, type_key: &str, nullable: bool) -> TableColumnInput {
    TableColumnInput {
        name: name.to_string(),
        type_key: type_key.to_string(),
        nullable,
        orig_name: None,
    }
}

fn employee_columns() -> Vec<TableColumnInput> {
    vec![
        col("name", "text", false),
        col("department", "text", true),
        col("office", "text", true),
    ]
}

async fn insert_employee_rows(pool: &SqlitePool) -> AppResult<()> {
    let rows = [
        ("田中太郎", "営業部", "東京本社"),
        ("鈴木花子", "開発部", "大阪支社"),
        ("佐藤次郎", "開発部", "東京本社"),
        ("高橋三郎", "営業部", "東京本社"),
        ("渡辺四郎", "人事部", "大阪支社"),
        ("伊藤五郎", "開発部", "東京本社"),
        ("山田六子", "営業部", "大阪支社"),
        ("中村七郎", "開発部", "大阪支社"),
        ("小林八子", "人事部", "東京本社"),
        ("加藤九郎", "営業部", "東京本社"),
        ("木村十郎", "開発部", "東京本社"),
        ("林十一子", "人事部", "大阪支社"),
        ("清水十二郎", "営業部", "大阪支社"),
        ("山本十三子", "開発部", "大阪支社"),
    ];

    for (name, department, office) in rows {
        sqlx::query(
            r#"
            INSERT INTO distinct_employees (name, department, office)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(name)
        .bind(department)
        .bind(office)
        .execute(pool)
        .await?;
    }

    Ok(())
}