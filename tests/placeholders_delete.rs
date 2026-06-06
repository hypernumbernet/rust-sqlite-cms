//! プレースホルダー削除（ゴミ箱投稿と FK）の回帰テスト。

mod common;

use rust_sqlite_cms::{
    models::placeholder::PlaceholderInput,
    models::post::PostInput,
    repos::{placeholders, posts, widget_types},
};

#[tokio::test]
async fn delete_placeholder_after_entries_are_trashed() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "delete_after_trash".to_string(),
            widget_type_id: news_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    let post_id = posts::insert(
        &pool,
        &PostInput {
            placeholder_id,
            title: "削除予定".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "to-trash".to_string(),
        },
    )
    .await
    .expect("insert post");

    posts::delete_in_placeholder(&pool, placeholder_id, post_id)
        .await
        .expect("trash post");

    placeholders::delete(&pool, placeholder_id)
        .await
        .expect("placeholder delete should succeed after trash cleanup");

    assert!(placeholders::find(&pool, placeholder_id).await.is_err());
}

#[tokio::test]
async fn delete_placeholder_blocked_when_active_post_exists() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "delete_blocked".to_string(),
            widget_type_id: news_type.id,
            config: "{}".to_string(),
        },
    )
    .await
    .expect("insert placeholder");

    posts::insert(
        &pool,
        &PostInput {
            placeholder_id,
            title: "残す".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "active".to_string(),
        },
    )
    .await
    .expect("insert post");

    let err = placeholders::delete(&pool, placeholder_id)
        .await
        .expect_err("active post should block delete");
    assert!(err.to_string().contains("紐付いている"));
}
