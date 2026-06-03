use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use crate::dev::reset::{perform_basic_append, perform_basic_reset};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

use super::{auth::AuthUser, layout};

#[derive(Template)]
#[template(path = "admin/samples/index.html")]
struct SamplesTemplate {
    layout: layout::AdminLayoutCtx,
    message: String,
    error_message: String,
    last_result: Option<ResetSummary>,
    show_restart_notice: bool,
    result_heading: String,
}

#[derive(Debug, Clone)]
pub struct ResetSummary {
    pub message: String,
    pub placeholders_count: i64,
    pub posts_count: i64,
    pub media_count: i64,
}

#[derive(Debug, Deserialize)]
struct SampleForm {
    /// 将来的にどのサンプルを適用するかを選択できるようにする
    #[serde(default)]
    sample: String,
    /// reset: 全体リセット / append: 既存データに追加
    #[serde(default = "default_action_reset")]
    action: String,
}

fn default_action_reset() -> String {
    "reset".to_string()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/samples", get(show).post(apply))
}

async fn show(auth: AuthUser, State(_state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = SamplesTemplate {
        layout: layout::AdminLayoutCtx::new(&auth),
        message: String::new(),
        error_message: String::new(),
        last_result: None,
        show_restart_notice: false,
        result_heading: String::new(),
    }
    .render()?;
    Ok(Html(html))
}

async fn apply(
    auth: AuthUser,
    State(state): State<AppState>,
    Form(form): Form<SampleForm>,
) -> AppResult<Response> {
    let _sample_key = if form.sample.is_empty() {
        "basic"
    } else {
        form.sample.as_str()
    };

    let is_append = form.action == "append";

    let result = if is_append {
        perform_basic_append(&state).await
    } else {
        perform_basic_reset(&state).await
    };

    match result {
        Ok(result) => {
            let summary = ResetSummary {
                message: result.message,
                placeholders_count: result.placeholders_count,
                posts_count: result.posts_count,
                media_count: result.media_count,
            };

            let (message, result_heading, show_restart_notice) = if is_append {
                (
                    "サンプルデータの追加が完了しました。".to_string(),
                    "追加完了".to_string(),
                    false,
                )
            } else {
                (
                    "リセットが完了しました。".to_string(),
                    "リセット完了".to_string(),
                    true,
                )
            };

            let html = SamplesTemplate {
                layout: layout::AdminLayoutCtx::new(&auth),
                message,
                error_message: String::new(),
                last_result: Some(summary),
                show_restart_notice,
                result_heading,
            }
            .render()?;

            Ok(Html(html).into_response())
        }
        Err(AppError::Conflict(msg)) => {
            let error_message = msg.strip_prefix("conflict: ").unwrap_or(&msg).to_string();

            let html = SamplesTemplate {
                layout: layout::AdminLayoutCtx::new(&auth),
                message: String::new(),
                error_message,
                last_result: None,
                show_restart_notice: false,
                result_heading: String::new(),
            }
            .render()?;
            Ok(Html(html).into_response())
        }
        Err(e) => {
            let label = if is_append { "追加" } else { "リセット" };
            let html = SamplesTemplate {
                layout: layout::AdminLayoutCtx::new(&auth),
                message: String::new(),
                error_message: format!("{}中にエラーが発生しました: {}", label, e),
                last_result: None,
                show_restart_notice: false,
                result_heading: String::new(),
            }
            .render()?;
            Ok(Html(html).into_response())
        }
    }
}
