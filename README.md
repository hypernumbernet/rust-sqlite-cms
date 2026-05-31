# rust-sqlite-cms

Rust と SQLite で動作する軽量 CMS です。テンプレートの中で「ここを更新可能にしたい」という領域を自由に定義すると、管理画面からその内容をウィジェットとして直接更新できるようになります。お知らせや特集、ヒーロー、画像カルーセルなど、ホームページの動的ブロックを柔軟に設計・運用できるのが特徴です。将来的には商品管理や注文処理など EC サイト構築機能への拡張も目指しています。

- 単一バイナリで配布・運用しやすい構成
- 組み込み SQLite によるシンプルなデータ永続化
- サーバーサイドレンダリング（管理画面は [Askama](https://github.com/askama-rs/askama) でコンパイル時検証、公開サイトは [MiniJinja](https://github.com/mitsuhiko/minijinja) でランタイム差し替え可能）

## 機能

現時点で利用できる機能:

- 企業ホームページのサンプル表示（`/`） — お知らせ欄は動的に生成されます
- 管理ダッシュボード（`/admin`）でサイト名・説明の表示
- 管理画面（`/admin/posts`）からのお知らせ（投稿）の作成・編集・公開 — プレースホルダー（テンプレート用スロット名）を定義し、その配下に複数の投稿エントリを管理（`news` などがデフォルトで利用可能）
- 管理画面（`/admin/pages`）からページ（トップ・MiniJinja テンプレート・静的 HTML）の作成・編集・削除と URL 公開（プリセット選択対応）
- 管理画面（`/admin/widgets`）からウィジェットの設定（例: お知らせの表示件数）
- 管理画面（`/admin/settings`）からサイト名・説明・サイト URL の編集
- `work/config.toml` および環境変数（`CMS_*`）による設定
- 初回起動時の SQLite データベース自動生成とスキーマ適用 + `work/` ディレクトリの初期化

現時点では未対応（実装予定）:

- ユーザー認証・ログイン
- 画像カルーセル（画像・リンクアップロード対応、Phase 2前倒し予定）
- メディアライブラリ、ユーザー管理などの管理画面（ナビゲーション上はリンクがありますが未実装）

実装の進捗とロードマップは [doc/PLAN.md](doc/PLAN.md) を参照してください。

## はじめに

### 前提

- Rust 1.85 以降（edition 2024）
- Cargo

### ビルド・実行

```bash
git clone <repository-url>
cd rust-sqlite-cms
cargo run
```

`cargo run` で次の起動シーケンスが実行されます: `work/config.toml` の確認（無ければ `config.example.toml` から生成）→ 設定読み込み → `data/cms.db`（無ければ自動生成）への接続 → マイグレーション適用 → 既定 `options` の確認 → `work/` ディレクトリの初期化（テンプレート seed + pages ディレクトリ作成 + トップページ行の確保） → `127.0.0.1:3000` で待受。

`work/config.toml` が無くてもデフォルト値で起動します（初回起動時に自動生成されます）。設定は環境変数でも上書きできます（例: `CMS_BIND_ADDR=0.0.0.0:3000 cargo run`）。

## 管理画面

ブラウザで `http://127.0.0.1:3000/` にアクセスすると企業ホームページのサンプルが表示されます。お知らせ欄には、管理画面で公開状態にした投稿が新しい順に表示されます（`news` プレースホルダー + MiniJinja テンプレートで動的生成）。

`http://127.0.0.1:3000/admin` にアクセスすると管理ダッシュボードが表示されます。そこから以下の管理が可能です:

- `/admin/posts` — お知らせ（投稿）の管理。プレースホルダー（テンプレート内で参照する名前、例: `news`）の追加と、その配下に表示する個別の投稿エントリ（タイトル・本文・抜粋・下書き/公開）の CRUD
- `/admin/pages` — ページ管理（トップ `index.html` を含む）。プリセットデザインからの新規作成、MiniJinja テンプレート vs 静的 HTML の選択、URL 割り当て、公開/非公開切り替え
- `/admin/widgets` — ウィジェット設定（現在は `news` ウィジェットの表示件数を変更可能）
- `/admin/settings` — サイト設定（サイト名・説明・サイト URL）

- 認証は未実装のため、ログインなしで開けます
- 画像カルーセル（Phase 2前倒し予定）、ユーザー管理などは未実装です（Phase 2 以降予定）

## 設定ファイル

設定は **`work/config.toml`** に保存します。初回起動時にリポジトリ直下の `config.example.toml` から自動生成されます（ルートに旧 `config.toml` がある場合はそちらを優先して移行します）。サイト名・説明は `/admin/settings` から編集でき、`options` テーブルと `[site]` セクションの両方に同期されます。

設定の優先順位は **デフォルト値 → `work/config.toml` → 環境変数（`CMS_*`）** です。

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

## ページ

公開サイトの HTML は **本文をファイル、メタ情報を DB**（`pages` テーブル）で管理します。`is_static` フラグで種別を区別します。

| `is_static` | 保存先 | 公開時の扱い |
|-------------|--------|--------------|
| `0` | `work/templates/` | [MiniJinja](https://github.com/mitsuhiko/minijinja) で評価（`blogname` / `blogdescription` のほか、`news` などのプレースホルダー変数も自動で渡されます） |
| `1` | `work/pages/` | 生 HTML をそのまま返す |

お知らせなどの動的コンテンツは、管理画面の **投稿** で定義したプレースホルダー（例: `news`）経由でテンプレートに注入されます。テンプレート側では `{% if has_news %}` / `{% for item in news %}` のように使えます。

```
work/templates/
├── index.html      # 公開トップ（/）。DB にトップ行を seed、無ければ presets から生成
├── page-3.html     # MiniJinja ページ（page-{id}.html）
└── static/         # CSS / JS。/static で配信

work/pages/
└── page-4.html     # 静的 HTML ページ
```

- **管理画面**: `/admin/pages` で一覧・編集。初回起動後は「トップページ」行から `index.html` を編集できます。`/admin/posts` でお知らせ（プレースホルダー配下の投稿）を管理し、テンプレートから参照できます。
- **任意 URL**: デザイン（`presets/`）を選んで作成し URL を割り当てると、フォールバックで公開されます。
- **静的アセット**: `work/templates/static/` を `/static` で配信します。

MiniJinja ページはランタイム差し替え可能（再起動不要）。管理画面 UI は `src/templates/admin/` の Askama に固定し、公開ページの影響を受けません。

## 開発者向け

設計思想、アーキテクチャ、データモデル、ロードマップ、実装進捗は [doc/PLAN.md](doc/PLAN.md) にまとめています。

## ライセンス

TBD（リポジトリオーナーが決定するまで未設定）
