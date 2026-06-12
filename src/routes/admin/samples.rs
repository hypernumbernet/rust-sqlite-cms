use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::samples::{self, InstallResult, SampleLayoutSetMeta, SampleTableSetMeta};
use crate::state::AppState;

use super::{auth::AuthUser, layout};

#[derive(Template)]
#[template(path = "admin/samples/index.html")]
struct SamplesTemplate {
    layout: layout::AdminLayoutCtx,
    layout_sets: Vec<SampleLayoutSetMeta>,
    table_sets: Vec<SampleTableSetMeta>,
    message: String,
    error_message: String,
    last_result: Option<InstallSummary>,
}

#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub result_kind: String,
    pub message: String,
    pub layout_key: String,
    pub preview_path: String,
    pub placeholders_count: i64,
    pub posts_count: i64,
    pub media_count: i64,
    pub pages_count: i64,
    pub tables_count: i64,
    pub views_count: i64,
    pub rows_count: i64,
}

#[derive(Debug, Deserialize)]
struct SampleForm {
    #[serde(default)]
    sample: String,
    #[serde(default = "default_action_install")]
    action: String,
}

fn default_action_install() -> String {
    "install".to_string()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/samples", get(show).post(apply))
}

fn render_template(
    auth: &AuthUser,
    message: String,
    error_message: String,
    last_result: Option<InstallSummary>,
) -> AppResult<String> {
    SamplesTemplate {
        layout: layout::AdminLayoutCtx::new(auth),
        layout_sets: samples::SAMPLE_LAYOUT_SETS.to_vec(),
        table_sets: samples::SAMPLE_TABLE_SETS.to_vec(),
        message,
        error_message,
        last_result,
    }
    .render()
    .map_err(Into::into)
}

async fn show(auth: AuthUser, State(_state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = render_template(&auth, String::new(), String::new(), None)?;
    Ok(Html(html))
}

async fn apply(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<SampleForm>,
) -> AppResult<Response> {
    if form.action != "install" {
        let html = render_template(
            &auth,
            String::new(),
            "不明な操作です。".to_string(),
            None,
        )?;
        return Ok(Html(html).into_response());
    }

    if form.sample.is_empty() {
        let html = render_template(
            &auth,
            String::new(),
            "サンプルセットが指定されていません。".to_string(),
            None,
        )?;
        return Ok(Html(html).into_response());
    }

    match samples::install_sample_set(&state, &form.sample).await {
        Ok(result) => {
            let (success_message, summary) = summary_from_result(&result);
            let html = render_template(&auth, success_message, String::new(), Some(summary))?;
            Ok(Html(html).into_response())
        }
        Err(AppError::Conflict(msg)) => {
            let error_message = msg.strip_prefix("conflict: ").unwrap_or(&msg).to_string();
            let html = render_template(&auth, String::new(), error_message, None)?;
            Ok(Html(html).into_response())
        }
        Err(e) => {
            let html = render_template(
                &auth,
                String::new(),
                format!("インストール中にエラーが発生しました: {e}"),
                None,
            )?;
            Ok(Html(html).into_response())
        }
    }
}

fn summary_from_result(result: &InstallResult) -> (String, InstallSummary) {
    match result {
        InstallResult::Layout {
            message,
            layout_key,
            preview_path,
            placeholders_count,
            posts_count,
            media_count,
            pages_count,
        } => (
            "サンプルレイアウトセットのインストールが完了しました。".to_string(),
            InstallSummary {
                result_kind: "layout".to_string(),
                message: message.clone(),
                layout_key: layout_key.clone(),
                preview_path: preview_path.clone(),
                placeholders_count: *placeholders_count,
                posts_count: *posts_count,
                media_count: *media_count,
                pages_count: *pages_count,
                tables_count: 0,
                views_count: 0,
                rows_count: 0,
            },
        ),
        InstallResult::Tables {
            message,
            tables_count,
            views_count,
            rows_count,
        } => (
            "DBテーブルセットのインストールが完了しました。".to_string(),
            InstallSummary {
                result_kind: "tables".to_string(),
                message: message.clone(),
                layout_key: String::new(),
                preview_path: String::new(),
                placeholders_count: 0,
                posts_count: 0,
                media_count: 0,
                pages_count: 0,
                tables_count: *tables_count,
                views_count: *views_count,
                rows_count: *rows_count,
            },
        ),
    }
}