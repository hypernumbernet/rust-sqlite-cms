use askama::Template;
use axum::{
    Router,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate;

async fn admin_dashboard() -> impl IntoResponse {
    match DashboardTemplate.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            eprintln!("template render error: {err}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "テンプレートの描画に失敗しました",
            )
                .into_response()
        }
    }
}

async fn index() -> impl IntoResponse {
    Redirect::temporary("/admin")
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/admin", get(admin_dashboard));

    let bind_addr = "127.0.0.1:8080";
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .expect("failed to bind address");

    println!("管理画面: http://{bind_addr}/admin");

    axum::serve(listener, app)
        .await
        .expect("server error");
}
