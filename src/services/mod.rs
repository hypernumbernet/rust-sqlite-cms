//! アプリケーションサービス層。
//!
//! ビジネスロジック、ドメインルール、バリデーション、
//! DB + ファイルI/O のオーケストレーションを担当。
//! すべての HTTP アダプタ（HTML ルート / JSON API）から利用される。

pub mod backup;
pub mod contact_form;
pub mod database;
pub mod layouts;
pub mod media;
pub mod users;
pub mod options;
pub mod pages;
pub mod placeholders;
pub mod posts;
pub mod widgets;
