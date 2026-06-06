//! ゴミ箱一覧・復元・完全削除の回帰テスト。

mod common;

use rust_sqlite_cms::{
    models::placeholder::PlaceholderInput,
    models::post::PostInput,
    repos::{placeholders, posts, widget_types},
};

#[tokio::test]
async fn trashed_post_appears_in_list_trashed() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "trash_list_test".to_string(),
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
            title: "ゴミ箱テスト".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "trash-list".to_string(),
        },
    )
    .await
    .expect("insert post");

    posts::delete_in_placeholder(&pool, placeholder_id, post_id)
        .await
        .expect("soft delete");

    let trashed = posts::list_trashed(&pool).await.expect("list trashed");
    assert!(trashed.iter().any(|p| p.id == post_id));
}

#[tokio::test]
async fn restore_returns_post_to_placeholder_list() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "restore_test".to_string(),
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
            title: "復元テスト".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "restore-me".to_string(),
        },
    )
    .await
    .expect("insert post");

    posts::delete_in_placeholder(&pool, placeholder_id, post_id)
        .await
        .expect("soft delete");

    posts::restore(&pool, post_id).await.expect("restore");

    let active = posts::list_all_for_placeholder(&pool, placeholder_id)
        .await
        .expect("list active");
    assert!(active.iter().any(|p| p.id == post_id && p.post_status == "draft"));

    let trashed = posts::list_trashed(&pool).await.expect("list trashed");
    assert!(!trashed.iter().any(|p| p.id == post_id));
}

#[tokio::test]
async fn restore_published_post_as_publish() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "restore_publish_test".to_string(),
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
            title: "公開復元".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "publish".to_string(),
            post_name: "pub-restore".to_string(),
        },
    )
    .await
    .expect("insert post");

    posts::delete_in_placeholder(&pool, placeholder_id, post_id)
        .await
        .expect("soft delete");

    posts::restore(&pool, post_id).await.expect("restore");

    let post = posts::find(&pool, post_id).await.expect("find post");
    assert_eq!(post.post_status, "publish");
    assert!(post.published_at.is_some());
}

#[tokio::test]
async fn purge_removes_post_permanently() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "purge_test".to_string(),
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
            title: "完全削除".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "purge-me".to_string(),
        },
    )
    .await
    .expect("insert post");

    posts::delete_in_placeholder(&pool, placeholder_id, post_id)
        .await
        .expect("soft delete");

    posts::purge(&pool, post_id).await.expect("purge");

    assert!(posts::find(&pool, post_id).await.is_err());
}

#[tokio::test]
async fn restore_and_purge_fail_for_non_trash_post() {
    let app = common::TestApp::new().await;
    let pool = app.state.pool();

    let news_type = widget_types::find_by_key(&pool, "news")
        .await
        .expect("news widget type");

    let placeholder_id = placeholders::insert(
        &pool,
        &PlaceholderInput {
            name: "not_trash_test".to_string(),
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
            title: "通常投稿".to_string(),
            content: "".to_string(),
            excerpt: "".to_string(),
            post_status: "draft".to_string(),
            post_name: "active".to_string(),
        },
    )
    .await
    .expect("insert post");

    assert!(posts::restore(&pool, post_id).await.is_err());
    assert!(posts::purge(&pool, post_id).await.is_err());
}
