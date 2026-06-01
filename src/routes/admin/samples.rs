use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;

use crate::dev::reset::perform_basic_reset;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/samples/index.html")]
struct SamplesTemplate {
    message: String,
    error_message: String,
    last_result: Option<ResetSummary>,
}

#[derive(Debug, Clone)]
pub struct ResetSummary {
    pub message: String,
    pub placeholders_count: i64,
    pub posts_count: i64,
    pub media_count: i64,
}

#[derive(Debug, Deserialize)]
struct ResetForm {
    /// 将来的にどのサンプルを適用するかを選択できるようにする
    #[serde(default)]
    sample: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/samples", get(show).post(apply))
}

async fn show(State(_state): State<AppState>) -> AppResult<impl IntoResponse> {
    let html = SamplesTemplate {
        message: String::new(),
        error_message: String::new(),
        last_result: None,
    }
    .render()?;
    Ok(Html(html))
}

async fn apply(
    State(state): State<AppState>,
    Form(form): Form<ResetForm>,
) -> AppResult<Response> {
    // 現在は "basic" のみ対応。将来的に form.sample で分岐
    let _sample_key = if form.sample.is_empty() {
        "basic"
    } else {
        &form.sample
    };

    match perform_basic_reset(&state).await {
        Ok(result) => {
            let summary = ResetSummary {
                message: result.message,
                placeholders_count: result.placeholders_count,
                posts_count: result.posts_count,
                media_count: result.media_count,
            };

            let html = SamplesTemplate {
                message: "リセットが完了しました。".to_string(),
                error_message: String::new(),
                last_result: Some(summary),
            }
            .render()?;

            Ok(Html(html).into_response())
        }
        Err(e) => {
            let html = SamplesTemplate {
                message: String::new(),
                error_message: format!("リセット中にエラーが発生しました: {}", e),
                last_result: None,
            }
            .render()?;
            Ok(Html(html).into_response())
        }
    }
}
