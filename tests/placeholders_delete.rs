//! プレースホルダー削除（ゴミ箱投稿と FK）の回帰テスト。

mod common;

use rust_sqlite_cms::{
    models::placeholder::PlaceholderInput,
    models::post::PostInput,
    repos::{placeholders, postmeta, posts, widget_types},
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

    let trashed = posts::list_trashed(&pool).await.expect("list trashed");
    let post = trashed
        .iter()
        .find(|p| p.id == post_id)
        .expect("trashed post should remain");
    assert!(post.placeholder_id.is_none());
    assert_eq!(
        postmeta::get(&pool, post_id, "_deleted_placeholder_name")
            .await
            .expect("get meta"),
        Some("delete_after_trash".to_string())
    );
}

#[tokio::test]
async fn delete_placeholder_trashes_active_posts() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "delete_with_active".to_string(),
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
            title: "残す".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "active".to_string(),
        },
    )
    .await
    .expect("insert post");

    placeholders::delete(&pool, placeholder_id)
        .await
        .expect("active posts should be trashed with placeholder delete");

    assert!(placeholders::find(&pool, placeholder_id).await.is_err());

    let active = posts::list_all_for_placeholder(&pool, placeholder_id)
        .await
        .expect("list active");
    assert!(active.is_empty());

    let trashed = posts::list_trashed(&pool).await.expect("list trashed");
    let post = trashed
        .iter()
        .find(|p| p.id == post_id)
        .expect("post should be in trash");
    assert_eq!(post.post_status, "trash");
    assert!(post.placeholder_id.is_none());
    assert_eq!(
        postmeta::get(&pool, post_id, "_deleted_placeholder_name")
            .await
            .expect("get meta"),
        Some("delete_with_active".to_string())
    );
}