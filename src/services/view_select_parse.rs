//! syntaqlite ベースのビュー SELECT パーサーアダプター。
//!
//! `parse_simple_view_select()` の実装。既存 `ParsedSimpleViewSelect` 型へ変換する。

use syntaqlite::any::AnyNodeId;
use syntaqlite::nodes::{
    BinaryOp, ColumnRef, Expr, Name, ResultColumn, SelectStmt, Stmt, TableSource,
};
use syntaqlite::parse::{ParsedStatement, ParserConfig};
use syntaqlite::{ParseOutcome, Parser};

use crate::services::database::{
    ParsedSimpleViewSelect, ParsedViewColumn, ParsedViewColumnList, ParsedWhereCondition,
};

/// 単純な `SELECT ... FROM table` 形式のビュー定義を UI 状態へ復元する。
pub(crate) fn parse_simple_view_select(definition: &str) -> Option<ParsedSimpleViewSelect> {
    let trimmed = definition.trim();
    if trimmed.is_empty() {
        return None;
    }

    let config = ParserConfig::default().with_collect_node_extents(true);
    let parser = Parser::with_config(&config);
    let mut session = parser.parse(trimmed);

    let parsed = {
        let stmt = match session.next() {
            ParseOutcome::Ok(stmt) => stmt,
            ParseOutcome::Err(_) | ParseOutcome::Done => return None,
        };
        parse_from_stmt(&stmt)?
    };

    if !matches!(session.next(), ParseOutcome::Done) {
        return None;
    }

    Some(parsed)
}

fn parse_from_stmt(stmt: &ParsedStatement<'_>) -> Option<ParsedSimpleViewSelect> {
    let Stmt::SelectStmt(select) = stmt.root()? else {
        return None;
    };

    if !select_supported_shape(&select) {
        return None;
    }

    let from_clause = select.from_clause()?;
    let (base_table, table_alias) = parse_table_source(from_clause)?;

    let columns = parse_column_list(stmt, select.columns()?, &base_table, table_alias.as_deref())?;
    let where_conditions =
        parse_where_clause(stmt, select.where_clause(), &base_table, table_alias.as_deref())?;

    Some(ParsedSimpleViewSelect {
        base_table,
        distinct: select.flags().distinct(),
        columns,
        where_conditions,
    })
}

fn select_supported_shape(select: &SelectStmt<'_>) -> bool {
    select.groupby().is_none()
        && select.having().is_none()
        && select.orderby().is_none()
        && select.limit_clause().is_none()
        && select.window_clause().is_none()
}

fn parse_table_source(source: TableSource<'_>) -> Option<(String, Option<String>)> {
    match source {
        TableSource::TableRef(table_ref) => {
            let base_table = normalize_identifier(table_ref.table_name());
            let alias = table_ref.alias().and_then(name_text);
            Some((base_table, alias))
        }
        TableSource::JoinClause(_) | TableSource::JoinPrefix(_) | TableSource::SubqueryTableSource(_) => {
            None
        }
    }
}

fn parse_column_list(
    stmt: &ParsedStatement<'_>,
    list: syntaqlite::nodes::ResultColumnList<'_>,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedViewColumnList> {
    let items: Vec<ResultColumn<'_>> = list.iter().collect();
    if items.is_empty() {
        return None;
    }

    if items.len() == 1 {
        let only = &items[0];
        if only.flags().star() {
            return Some(ParsedViewColumnList::All);
        }
        if let Some(expr) = only.expr() {
            if is_matching_qualified_star(expr, base_table, table_alias) {
                return Some(ParsedViewColumnList::All);
            }
        }
    }

    let columns: Vec<ParsedViewColumn> = items
        .iter()
        .filter_map(|item| parse_result_column(stmt, item, base_table, table_alias))
        .collect();

    if columns.len() != items.len() {
        return None;
    }
    if !select_output_names_unique(&columns) {
        return None;
    }

    Some(ParsedViewColumnList::Columns(columns))
}

fn parse_result_column(
    stmt: &ParsedStatement<'_>,
    column: &ResultColumn<'_>,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedViewColumn> {
    if column.flags().star() {
        return None;
    }

    let alias = column.alias().and_then(name_text);
    let expr = column.expr()?;

    if is_matching_qualified_star(expr, base_table, table_alias) {
        return None;
    }

    if let Expr::ColumnRef(column_ref) = expr {
        if column_ref.table().is_empty() {
            return Some(ParsedViewColumn {
                name: normalize_identifier(column_ref.column()),
                alias,
                expression: None,
            });
        }
        let qualifier = normalize_identifier(column_ref.table());
        if qualifier_matches(&qualifier, base_table, table_alias) {
            return Some(ParsedViewColumn {
                name: normalize_identifier(column_ref.column()),
                alias,
                expression: None,
            });
        }
        return None;
    }

    let expression = node_sql_text(stmt, expr_node_id(expr))?;
    Some(ParsedViewColumn {
        name: String::new(),
        alias,
        expression: Some(expression),
    })
}

fn parse_where_clause(
    stmt: &ParsedStatement<'_>,
    where_expr: Option<Expr<'_>>,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<Vec<ParsedWhereCondition>> {
    let Some(where_expr) = where_expr else {
        return Some(Vec::new());
    };

    if expr_contains_or(where_expr) {
        return None;
    }

    let parts = flatten_and(where_expr);
    let mut conditions = Vec::with_capacity(parts.len());
    for part in parts {
        conditions.push(parse_where_condition(stmt, part, base_table, table_alias)?);
    }
    Some(conditions)
}

fn flatten_and(expr: Expr<'_>) -> Vec<Expr<'_>> {
    match expr {
        Expr::BinaryExpr(binary) if binary.op() == BinaryOp::And => {
            let mut parts = Vec::new();
            if let Some(left) = binary.left() {
                parts.extend(flatten_and(left));
            }
            if let Some(right) = binary.right() {
                parts.extend(flatten_and(right));
            }
            parts
        }
        other => vec![other],
    }
}

fn expr_contains_or(expr: Expr<'_>) -> bool {
    match expr {
        Expr::BinaryExpr(binary) if binary.op() == BinaryOp::Or => true,
        Expr::BinaryExpr(binary) => {
            binary.left().is_some_and(expr_contains_or) || binary.right().is_some_and(expr_contains_or)
        }
        Expr::ParenExpr(paren) => paren.expr().is_some_and(expr_contains_or),
        _ => false,
    }
}

fn parse_where_condition(
    stmt: &ParsedStatement<'_>,
    expr: Expr<'_>,
    base_table: &str,
    table_alias: Option<&str>,
) -> Option<ParsedWhereCondition> {
    let Some(column_ref) = root_column_ref(expr) else {
        let full_text = node_sql_text(stmt, expr_node_id(expr))?;
        return Some(ParsedWhereCondition::expression_only(full_text));
    };
    let full_text = node_sql_text(stmt, expr_node_id(expr))?;
    let column = normalize_identifier(column_ref.column());

    if !column_ref.table().is_empty() {
        let qualifier = normalize_identifier(column_ref.table());
        if !qualifier_matches(&qualifier, base_table, table_alias) {
            return None;
        }
    }

    let left_text = node_sql_text(stmt, column_ref.node_id().into())?;
    let left_trimmed = left_text.trim();
    let start = full_text.find(left_trimmed)?;
    let suffix = full_text[start + left_trimmed.len()..].trim();
    if suffix.is_empty() {
        return None;
    }

    Some(ParsedWhereCondition::column_suffix(column, suffix))
}

fn root_column_ref(expr: Expr<'_>) -> Option<ColumnRef<'_>> {
    match expr {
        Expr::ColumnRef(column_ref) => Some(column_ref),
        Expr::BinaryExpr(binary) => binary.left().and_then(root_column_ref),
        Expr::IsExpr(is_expr) => is_expr.left().and_then(root_column_ref),
        Expr::LikeExpr(like_expr) => like_expr.operand().and_then(root_column_ref),
        Expr::BetweenExpr(between_expr) => between_expr.operand().and_then(root_column_ref),
        Expr::InExpr(in_expr) => in_expr.operand().and_then(root_column_ref),
        Expr::CollateExpr(collate_expr) => collate_expr.expr().and_then(root_column_ref),
        Expr::UnaryExpr(unary_expr) => unary_expr.operand().and_then(root_column_ref),
        _ => None,
    }
}

fn is_matching_qualified_star(
    expr: Expr<'_>,
    base_table: &str,
    table_alias: Option<&str>,
) -> bool {
    let Expr::ColumnRef(column_ref) = expr else {
        return false;
    };
    if column_ref.column() != "*" {
        return false;
    }
    let qualifier = normalize_identifier(column_ref.table());
    qualifier_matches(&qualifier, base_table, table_alias)
}

fn qualifier_matches(qualifier: &str, base_table: &str, table_alias: Option<&str>) -> bool {
    qualifier == base_table || table_alias.is_some_and(|alias| qualifier == alias)
}

fn name_text(name: Name<'_>) -> Option<String> {
    match name {
        Name::IdentName(ident) => Some(normalize_identifier(ident.source())),
        Name::Error(_) => None,
    }
}

fn normalize_identifier(raw: &str) -> String {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        raw[1..raw.len() - 1].replace("\"\"", "\"")
    } else {
        raw.to_string()
    }
}

fn expr_node_id(expr: Expr<'_>) -> AnyNodeId {
    expr.node_id().into()
}

fn node_sql_text(stmt: &ParsedStatement<'_>, id: AnyNodeId) -> Option<String> {
    stmt.node_text(id)
        .map(|(text, _)| text.to_string())
        .or_else(|| stmt.node_expanded_text(id).map(|text| text.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::{
        build_simple_view_select, build_simple_view_ui_spec, parse_simple_view_select,
        ExtraWhereCondition, SimpleViewUiSpec, TableColumnUiInfo,
    };

    fn sorted_where(conditions: &[ParsedWhereCondition]) -> Vec<ParsedWhereCondition> {
        let mut sorted = conditions.to_vec();
        sorted.sort_by(|left, right| {
            left.column
                .cmp(&right.column)
                .then(left.suffix.cmp(&right.suffix))
        });
        sorted
    }

    #[test]
    fn parse_where_with_literal_keyword_in_string() {
        let sql = r#"SELECT "id" FROM "custom_notes" WHERE "body" = 'copied FROM archive'"#;
        let parsed = super::parse_simple_view_select(sql).expect("syntaqlite should parse");
        assert_eq!(parsed.base_table, "custom_notes");
        assert_eq!(parsed.where_conditions.len(), 1);
        assert_eq!(parsed.where_conditions[0].column, "body");
        assert!(parsed.where_conditions[0].suffix.contains("'copied FROM archive'"));
    }

    #[test]
    fn parse_select_item_literal_containing_from_keyword() {
        let sql = r#"SELECT 'copied FROM archive' AS "source", "id" FROM "custom_notes""#;
        let parsed = super::parse_simple_view_select(sql).expect("syntaqlite should parse");
        assert_eq!(parsed.base_table, "custom_notes");
        let ParsedViewColumnList::Columns(columns) = parsed.columns else {
            panic!("expected columns");
        };
        assert_eq!(columns[0].expression.as_deref(), Some("'copied FROM archive'"));
    }

    #[test]
    fn parse_where_is_not_distinct_from_sqlite_specific() {
        let sql = r#"SELECT "id" FROM "custom_notes" WHERE "body" IS NOT DISTINCT FROM "title""#;
        let parsed = super::parse_simple_view_select(sql).expect("syntaqlite should parse");
        assert_eq!(parsed.where_conditions.len(), 1);
        assert_eq!(parsed.where_conditions[0].column, "body");
        assert_eq!(
            parsed.where_conditions[0].suffix,
            "IS NOT DISTINCT FROM \"title\""
        );
    }

    #[test]
    fn parse_where_numeric_literal_with_underscore_separator() {
        let sql = r#"SELECT "id" FROM "custom_notes" WHERE "id" > 1_000"#;
        let parsed = super::parse_simple_view_select(sql).expect("syntaqlite should parse");
        assert_eq!(parsed.where_conditions.len(), 1);
        assert_eq!(parsed.where_conditions[0].column, "id");
        assert_eq!(parsed.where_conditions[0].suffix, "> 1_000");
    }

    #[test]
    fn parse_where_notnull_sqlite_specific() {
        let sql = r#"SELECT "id" FROM "custom_notes" WHERE "body" NOTNULL"#;
        let parsed = super::parse_simple_view_select(sql).expect("syntaqlite should parse NOTNULL");
        assert_eq!(
            parsed.where_conditions,
            vec![ParsedWhereCondition {
                column: "body".to_string(),
                suffix: "NOTNULL".to_string(),
                expression: None,
            }]
        );
    }

    #[test]
    fn parse_expression_column_coalesce() {
        let sql = r#"SELECT COALESCE(LOWER("body"), '') AS "body_norm" FROM "custom_notes""#;
        let parsed = super::parse_simple_view_select(sql).expect("parsed");
        let ParsedViewColumnList::Columns(columns) = parsed.columns else {
            panic!("expected columns");
        };
        assert_eq!(columns.len(), 1);
        assert_eq!(
            columns[0].expression.as_deref(),
            Some(r#"COALESCE(LOWER("body"), '')"#)
        );
        assert_eq!(columns[0].alias.as_deref(), Some("body_norm"));
    }

    #[test]
    fn parse_distinct_with_inline_comment() {
        let sql = r#"SELECT /* x */ DISTINCT "id" FROM "custom_notes""#;
        let parsed = super::parse_simple_view_select(sql).expect("parsed");
        assert!(parsed.distinct);
        let ParsedViewColumnList::Columns(columns) = parsed.columns else {
            panic!("expected columns");
        };
        assert_eq!(columns[0].name, "id");
    }

    #[test]
    fn parse_rejects_join_for_phase2_prep() {
        let sql = "SELECT a.id FROM t1 a JOIN t2 b ON a.id = b.ref";
        assert!(super::parse_simple_view_select(sql).is_none());
    }

    #[test]
    fn roundtrip_parse_build_parse_preserves_meaning() {
        let sql = r#"SELECT LENGTH("body") AS "body_len", "id" FROM "custom_notes" WHERE "body" IS NOT NULL AND "id" > 0"#;
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

        let parsed = parse_simple_view_select(sql).expect("initial parse");
        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        let rebuilt = build_simple_view_select(&spec).expect("rebuild");
        let reparsed = parse_simple_view_select(&rebuilt).expect("re-parse");

        assert_eq!(reparsed.base_table, parsed.base_table);
        assert_eq!(reparsed.distinct, parsed.distinct);
        assert_eq!(
            sorted_where(&reparsed.where_conditions),
            sorted_where(&parsed.where_conditions)
        );

        let normalize_columns = |spec: &SimpleViewUiSpec| {
            spec.columns
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        c.alias.clone(),
                        c.expression.clone(),
                        c.where_condition.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            normalize_columns(&spec),
            normalize_columns(&build_simple_view_ui_spec(&reparsed, &table_columns))
        );
    }

    #[test]
    fn roundtrip_distinct_and_extra_where() {
        let sql = r#"SELECT DISTINCT "id" FROM "custom_notes" WHERE "body" IS NOT NULL"#;
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

        let parsed = parse_simple_view_select(sql).expect("parsed");
        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert!(spec.distinct);
        assert_eq!(
            spec.extra_where,
            vec![ExtraWhereCondition {
                column: "body".to_string(),
                suffix: "IS NOT NULL".to_string(),
                expression: None,
            }]
        );

        let rebuilt = build_simple_view_select(&spec).expect("rebuild");
        let reparsed = parse_simple_view_select(&rebuilt).expect("re-parse");
        assert!(reparsed.distinct);
        assert_eq!(reparsed.where_conditions, parsed.where_conditions);
    }

    #[test]
    fn parse_corporate_services_mixed_where_conditions() {
        let sql = r#"SELECT "id", "is_featured", "name" FROM "corporate_services" WHERE "id" >=2 AND LENGTH("name")>0"#;
        let parsed = super::parse_simple_view_select(sql).expect("parsed");
        assert_eq!(parsed.base_table, "corporate_services");
        assert_eq!(parsed.where_conditions.len(), 2);
        assert_eq!(
            parsed.where_conditions[0],
            ParsedWhereCondition::column_suffix("id", ">=2")
        );
        assert_eq!(
            parsed.where_conditions[1].expression.as_deref(),
            Some(r#"LENGTH("name")>0"#)
        );

        let table_columns = vec![
            TableColumnUiInfo {
                name: "id".to_string(),
                type_key: "integer".to_string(),
                pk: true,
                nullable: false,
            },
            TableColumnUiInfo {
                name: "is_featured".to_string(),
                type_key: "integer".to_string(),
                pk: false,
                nullable: true,
            },
            TableColumnUiInfo {
                name: "name".to_string(),
                type_key: "text".to_string(),
                pk: false,
                nullable: false,
            },
        ];
        let spec = build_simple_view_ui_spec(&parsed, &table_columns);
        assert_eq!(spec.columns[0].where_condition.as_deref(), Some(">=2"));
        assert_eq!(
            spec.extra_where,
            vec![ExtraWhereCondition {
                column: String::new(),
                suffix: String::new(),
                expression: Some(r#"LENGTH("name")>0"#.to_string()),
            }]
        );

        let rebuilt = build_simple_view_select(&spec).expect("rebuild");
        assert!(
            rebuilt.contains(r#"WHERE "id" >=2 AND LENGTH("name")>0"#),
            "rebuilt SQL: {rebuilt}"
        );
        let reparsed = parse_simple_view_select(&rebuilt).expect("re-parse");
        assert_eq!(reparsed.where_conditions.len(), 2);
        assert_eq!(reparsed.where_conditions[0].column, "id");
        assert_eq!(reparsed.where_conditions[0].suffix, ">=2");
        assert_eq!(
            reparsed.where_conditions[1].expression.as_deref(),
            Some(r#"LENGTH("name")>0"#)
        );
    }

    #[test]
    fn parse_quoted_drop_function_expression_for_validation_recovery() {
        let sql = r#"SELECT "DROP"("body") AS "body_len" FROM "expr_recover_notes""#;
        let parsed = super::parse_simple_view_select(sql).expect("parsed");
        let ParsedViewColumnList::Columns(columns) = parsed.columns else {
            panic!("expected columns");
        };
        assert_eq!(columns[0].expression.as_deref(), Some(r#""DROP"("body")"#));
        assert_eq!(columns[0].alias.as_deref(), Some("body_len"));
    }
}
