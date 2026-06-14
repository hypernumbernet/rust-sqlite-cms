//! 「業務データ（コーポレート向け）」DBテーブルサンプルセット。

use sqlx::SqlitePool;

use crate::error::AppResult;
use crate::samples::conflict;
use crate::services::database::{self, TableColumnInput};
use crate::state::AppState;

const TABLE_SERVICES: &str = "corporate_services";
const TABLE_TEAM: &str = "corporate_team";
const VIEW_FEATURED_SERVICES: &str = "corporate_featured_services";

const TABLE_NAMES: &[&str] = &[TABLE_SERVICES, TABLE_TEAM];
const VIEW_NAMES: &[&str] = &[VIEW_FEATURED_SERVICES];
const SAMPLE_ROW_COUNT: i64 = 11;

/// 業務データサンプルをインストールする。
pub async fn install(state: &AppState) -> AppResult<super::super::InstallResult> {
    let pool = state.pool();

    if let Err(conflicts) = check_conflicts(&pool).await {
        return Err(conflict::abort(conflicts));
    }

    database::create_user_table_from_columns(&pool, TABLE_SERVICES, &services_columns())
        .await?;
    database::create_user_table_from_columns(&pool, TABLE_TEAM, &team_columns())
        .await?;

    insert_services_rows(&pool).await?;
    insert_team_rows(&pool).await?;

    database::create_user_view(
        &pool,
        VIEW_FEATURED_SERVICES,
        r#"
SELECT id, name, category, description, price_from
FROM corporate_services
WHERE is_featured = 1
        "#,
    )
    .await?;

    Ok(super::super::InstallResult::Tables {
        message: "業務データ（コーポレート向け）のサンプルをインストールしました。".to_string(),
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

fn services_columns() -> Vec<TableColumnInput> {
    vec![
        col("name", "text", false),
        col("category", "text", false),
        col("description", "text", true),
        col("price_from", "integer", false),
        col("is_featured", "boolean", false),
    ]
}

fn team_columns() -> Vec<TableColumnInput> {
    vec![
        col("name", "text", false),
        col("department", "text", false),
        col("role", "text", false),
        col("joined_at", "timestamp", true),
    ]
}

async fn insert_services_rows(pool: &SqlitePool) -> AppResult<()> {
    let rows = [
        (
            "Web サイト制作",
            "制作",
            "企業サイト・採用サイトの企画から公開まで一貫支援",
            30,
            1,
        ),
        (
            "業務システム開発",
            "開発",
            "受注管理・在庫管理など業務に合わせたアプリ開発",
            50,
            1,
        ),
        (
            "運用サポート",
            "支援",
            "公開後の更新代行・改善提案・保守対応",
            10,
            1,
        ),
        (
            "コンテンツ制作",
            "制作",
            "お知らせ記事・事例紹介などのライティング支援",
            5,
            0,
        ),
        (
            "セキュリティ診断",
            "支援",
            "公開サイトの脆弱性チェックと改善アドバイス",
            20,
            0,
        ),
    ];

    for (name, category, description, price_from, is_featured) in rows {
        sqlx::query(
            r#"
            INSERT INTO corporate_services (name, category, description, price_from, is_featured)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(name)
        .bind(category)
        .bind(description)
        .bind(price_from)
        .bind(is_featured)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn insert_team_rows(pool: &SqlitePool) -> AppResult<()> {
    let rows = [
        (
            "田中 美咲",
            "営業部",
            "アカウントマネージャー",
            "2019-04-01T00:00:00",
        ),
        (
            "佐藤 健太",
            "開発部",
            "リードエンジニア",
            "2018-10-15T00:00:00",
        ),
        (
            "鈴木 彩",
            "デザイン部",
            "UI デザイナー",
            "2021-03-01T00:00:00",
        ),
        (
            "高橋 亮",
            "開発部",
            "バックエンドエンジニア",
            "2022-07-01T00:00:00",
        ),
        (
            "伊藤 真由",
            "カスタマーサクセス部",
            "サポートリード",
            "2020-01-20T00:00:00",
        ),
        (
            "渡辺 翔",
            "営業部",
            "インサイドセールス",
            "2023-04-10T00:00:00",
        ),
    ];

    for (name, department, role, joined_at) in rows {
        sqlx::query(
            r#"
            INSERT INTO corporate_team (name, department, role, joined_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(name)
        .bind(department)
        .bind(role)
        .bind(joined_at)
        .execute(pool)
        .await?;
    }

    Ok(())
}