# rust-sqlite-cms

Rust と SQLite で動作する軽量 CMS です。一般的なホームページのお知らせ欄などを、ブラウザからお手軽に更新できる体験を中心に、商品管理や注文処理など EC サイト構築へと拡張していくことを目指しています。

- 単一バイナリで配布・運用しやすい構成
- 組み込み SQLite によるシンプルなデータ永続化
- サーバーサイドレンダリング（管理画面は [Askama](https://github.com/askama-rs/askama) でコンパイル時検証、公開サイトは [MiniJinja](https://github.com/mitsuhiko/minijinja) でランタイム差し替え可能）

## 機能

現時点で利用できる機能:

- 企業ホームページのサンプル表示（`/`）
- 管理ダッシュボード（`/admin`）でサイト名・説明の表示
- 管理画面（`/admin/posts`）からのお知らせ作成・編集・公開
- 管理画面（`/admin/templates`）から HTML テンプレートの作成・編集・削除と URL 公開
- `config.toml` および環境変数（`CMS_*`）による設定
- 初回起動時の SQLite データベース自動生成とスキーマ適用

現時点では未対応（実装予定）:

- ユーザー認証・ログイン
- 固定ページの作成・編集

実装の進捗とロードマップは [doc/PLAN.md](doc/PLAN.md) を参照してください。

## はじめに

### 前提

- Rust 1.85 以降（edition 2024）
- Cargo

### ビルド・実行

```bash
git clone <repository-url>
cd rust-sqlite-cms
# 任意: 設定を変更する場合のみ
cp config.example.toml config.toml
cargo run
```

`cargo run` で次の起動シーケンスが実行されます: 設定読み込み → `data/cms.db`（無ければ自動生成）への接続 → `migrations/` の適用 → 既定 `options` の確認 → `127.0.0.1:3000` で待受。

`config.toml` が無くてもデフォルト値で起動します。設定は環境変数でも上書きできます（例: `CMS_BIND_ADDR=0.0.0.0:3000 cargo run`）。

## 管理画面

ブラウザで `http://127.0.0.1:3000/` にアクセスすると企業ホームページのサンプルが表示されます。お知らせ欄には、管理画面で公開状態にした投稿が新しい順に表示されます。

`http://127.0.0.1:3000/admin` にアクセスすると管理ダッシュボードが表示され、`http://127.0.0.1:3000/admin/posts` からお知らせを作成・編集できます。

- 認証は未実装のため、ログインなしで開けます
- 固定ページ・メディア・ユーザー・設定・コメントなどは未実装です

## 設定ファイル

`config.example.toml` を `config.toml` にコピーして編集できます。設定の優先順位は **デフォルト値 → `config.toml` → 環境変数（`CMS_*`）** です。

```toml
# config.example.toml

[server]
# リッスンアドレス（例: "127.0.0.1:3000"）
bind_addr = "127.0.0.1:3000"

[database]
# SQLite データベースファイルのパス
path = "data/cms.db"

[paths]
# メディアのアップロード先
uploads_dir = "uploads"
# テンプレート・静的アセットのステートフルな作業ディレクトリ
work_dir = "work"

[site]
# 表示名・説明（options テーブルの既定値）
title = "My Site"
tagline = "Just another rust-sqlite-cms site"

[session]
# セッション Cookie 名・有効期限（秒）
cookie_name = "cms_session"
max_age_secs = 604800

[security]
# 本番では必ず環境変数 CMS_SESSION_SECRET 等で上書きすること
# session_secret = "change-me-in-production"
```

環境変数での上書き:

| 変数 | 説明 |
|------|------|
| `CMS_BIND_ADDR` | リッスンアドレス |
| `CMS_DATABASE_PATH` | DB ファイルパス |
| `CMS_SESSION_SECRET` | セッション署名用シークレット |

## テンプレート

公開サイトは `work/templates/` 以下の [MiniJinja](https://github.com/mitsuhiko/minijinja)（Jinja2 系）テンプレート（ファイル）で描画します。MiniJinja はランタイムでテンプレートを評価するため、**サーバーを再起動せずに差し替え**できます。`work/` は実行時に生成・編集される**ステートフルなディレクトリ**（git 管理外）で、初期テンプレートは `presets/` を seed とします。

```
work/templates/
├── index.html      # 公開トップ（/）。無ければ presets/index.html から自動生成
├── page-3.html     # 管理画面で作成した各ページ（page-{id}.html）
└── static/         # CSS / JS。/static で配信
```

- **本文はファイル、メタ情報は DB**: HTML 本文は `work/templates/` のファイルに保持し、URL・公開フラグなどのメタ情報は `templates` テーブルに保存します（`file_name` 列が物理ファイルと対応）。管理画面はこの両者を同期して読み書きします。
- **公開トップ（`/`）**: `work/templates/index.html` を描画します。起動時に存在しなければ `presets/index.html` から自動生成され、編集すると `minijinja-autoreload` の監視で再起動なしに反映されます。
- **任意 URL のページ**: 管理画面（`/admin/templates`）でデザイン（`presets/`）を選んで作成し、URL を割り当てて公開すると、未一致パスのフォールバックで `work/templates/{file_name}` を描画します。
- **静的アセット**: `work/templates/static/` 配下の CSS / JS を `/static` で配信します。各 HTML からは `static/xxx.css` で参照できます。

テンプレートには Rust 側で組み立てたコンテキスト（`blogname` / `blogdescription` / 公開済みお知らせ `news`）を渡します。

管理画面は `src/templates/admin/` の Askama テンプレートに固定し、公開テンプレートの影響を受けません。

## 開発者向け

設計思想、アーキテクチャ、データモデル、ロードマップ、実装進捗は [doc/PLAN.md](doc/PLAN.md) にまとめています。

## ライセンス

TBD（リポジトリオーナーが決定するまで未設定）
