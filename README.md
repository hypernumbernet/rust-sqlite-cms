# rust-sqlite-cms

Rust と SQLite で動作する軽量 CMS です。テンプレートの中で「ここを更新可能にしたい」という領域を自由に定義すると、管理画面からその内容をウィジェットとして直接更新できるようになります。お知らせや特集、ヒーロー、画像カルーセルなど、ホームページの動的ブロックを柔軟に設計・運用できるのが特徴です。将来的には商品管理や注文処理など EC サイト構築機能への拡張も目指しています。

- 単一バイナリで配布・運用しやすい構成
- 組み込み SQLite によるシンプルなデータ永続化
- サーバーサイドレンダリング（管理画面は [Askama](https://github.com/askama-rs/askama) でコンパイル時検証、公開サイトは [MiniJinja](https://github.com/mitsuhiko/minijinja) でランタイム差し替え可能）

## 機能

現時点で利用できる機能:

- 企業ホームページのサンプル表示（`/`） — カルーセル・お知らせ欄などはウィジェットにより動的に生成
- 管理ダッシュボード（`/admin`）でサイト名・説明の表示と各管理画面へのカードリンク
- 管理画面（`/admin/layouts`）からレイアウト（shell.html・静的アセット・ページ本文テンプレート）の管理。公開サイトは常に MiniJinja で評価され、`/static/{layout_key}/*` で静的配信
- 管理画面（`/admin/pages`）からページ（トップ・任意 URL）の作成・編集・削除・公開。レイアウト選択、プリセット（ランディング/シンプル/お知らせ一覧）対応、プレビュー（編集モード注釈付き）
- 管理画面（`/admin/posts`）からのお知らせ（投稿）の作成・編集・公開・ゴミ箱（trash）。プレースホルダー（テンプレート用スロット名）を定義し、その配下に複数の投稿エントリを管理（`news` / `main_carousel` などが利用可能）
- 管理画面（`/admin/widgets`）からウィジェットタイプの HTML 構成（`html_template`）・インスタンス設定スキーマ（`config_schema`）の編集。news / image / carousel などがプリセット済み。各行/編集画面からの JSON エクスポート、一覧からの JSON インポート（カスタム type_key も可）
- 管理画面（`/admin/media`）から画像・ファイルのアップロード・一覧・削除。image ウィジェットや carousel ウィジェットのスライドで添付・利用
- 管理画面（`/admin/samples`）から開発用サンプルデータの投入（リセット/追記）。カルーセル画像入りデモなどを簡単に再現
- 管理画面（`/admin/settings`）からサイト名・説明・サイト URL の編集（`options` と `work/config.toml` の `[site]` に同期）
- 管理画面（`/admin/users`）から管理ユーザーの作成・編集・削除（既定の `admin` は削除不可）
- 管理画面（`/admin/database`）から SQLite テーブル・ビューの一覧、ユーザー定義テーブルの作成・列編集、データ閲覧、テストデータ生成
- REST API（`/api/v1`）— レイアウト・プレースホルダー・投稿・ページ・ウィジェット・メディア・設定の JSON 操作（Cookie セッション認証）。セッション API は認証不要
- `work/config.toml` および環境変数（`CMS_*`）による設定
- 初回起動時の SQLite データベース自動生成とスキーマ適用（`migrations/` 配下） + `work/layouts/default/` のシード + 既定レイアウト/トップページ/アップロードディレクトリの確保

未対応の主な機能（ロードマップ参照）:

- ロール/権限の細かい capability 制御（現在は全管理者が全操作可能）
- タクソノミー（カテゴリ・タグ）
- ナビゲーションメニュー・RSS・予約公開
- 商品・注文など EC 機能（Phase 3 以降）
- レイアウト単位のエクスポート/インポート（Phase C）

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

開発・動作確認用に、管理ユーザー `admin` のパスワードを常に固定値 **`testpass`** にするテストモードもあります:

```bash
cargo run -- --test
```

`--test` 指定時は起動のたびに `admin` のパスワードが `testpass` に設定されます（既存 DB に `admin` がいても上書き）。本番環境では使用しないでください。

`cargo run` で次の起動シーケンスが実行されます: `work/config.toml` の確認（無ければ `config.example.toml` から生成）→ 設定読み込み → `data/cms.db`（親 dir 含め自動生成）への接続 → マイグレーション適用（`migrations/0001_init.sql`） → 既定 `options` の確認 → 既定ユーザー `admin` の確認（通常起動: 初回のみランダムパスワードを起動ログに一度出力 / `--test`: パスワードを `testpass` に設定）→ `work/` ディレクトリの初期化（`work/layouts/default/` の shell/pages/static seed、既定レイアウト行の確保、uploads ディレクトリ確保、トップページ行の確保） → `127.0.0.1:3000` で待受。

`work/config.toml` が無くてもデフォルト値で起動します（初回起動時に自動生成されます）。設定は環境変数でも上書きできます（例: `CMS_BIND_ADDR=0.0.0.0:3000 cargo run`）。

`migrations/0001_init.sql` を変更したあとに起動エラー（マイグレーションのチェックサム不一致）が出る場合は、既存 DB を削除して再生成してください。

```bash
rm -f data/cms.db
cargo run
```

## 管理画面

ブラウザで `http://127.0.0.1:3000/` にアクセスすると企業ホームページのサンプルが表示されます。カルーセル（`main_carousel`）やお知らせ欄（`news`）は、管理画面で公開状態にした投稿/メディアをウィジェットとして動的生成します（MiniJinja テンプレート + `*_html` 変数）。

`http://127.0.0.1:3000/admin` にアクセスすると、未ログイン時は `/admin/login` へリダイレクトされます。ログイン後、ダッシュボードから以下の管理が可能です:

- `/admin/posts` — 投稿（お知らせ等）の管理。プレースホルダー（テンプレート内で参照する名前、例: `news` / `main_carousel`）の追加と、その配下に表示する個別の投稿エントリ（タイトル・本文・抜粋・下書き/公開/ゴミ箱）の CRUD
- `/admin/pages` — ページ管理（トップ `/` を含む）。レイアウト選択 + プリセット（ランディング/シンプルページ/お知らせ一覧）からの新規作成、MiniJinja テンプレート編集、URL 割り当て、公開/非公開切り替え、プレビュー（ウィジェット編集注釈付き）
- `/admin/layouts` — レイアウト管理（shell.html 編集、static ファイルのアップロード/削除、favicon メディア選択、所属ページの確認）
- `/admin/widgets` — ウィジェットタイプの HTML 構成（`html_template`）・インスタンス設定スキーマ（`config_schema`）の編集。news/image/carousel プリセット済み。各行または編集画面から JSON エクスポート、一覧から JSON インポート（カスタム `type_key` の新規登録可）
- `/admin/media` — メディアライブラリ。画像/ファイルのアップロード、一覧表示（プレビュー付き）、削除。ウィジェット（image/carousel）での利用時に参照
- `/admin/samples` — サンプルデータ投入（基本リセット / 追記）。カルーセル付きデモページなどをワンクリックで再現可能
- `/admin/settings` — サイト設定（サイト名・説明・サイト URL）
- `/admin/users` — 管理ユーザー（アカウントの追加・編集・削除。既定の `admin` は削除不可）
- `/admin/database` — DB 管理（テーブル・ビュー一覧、ユーザー定義テーブルの作成・列編集、データ閲覧、テストデータ生成）

### DB管理のアクセス制御

DB 管理画面では、テーブル種別ごとに操作可能な範囲が異なります。

| テーブル種別 | 一覧 | データ閲覧 | 列編集・テストデータ生成 | 種別表示 |
|---|---|---|---|---|
| インフラ用（`_sqlx_migrations`） | 非表示 | 不可 | 不可 | — |
| CMS コア 8 表 | 表示 | 可（閲覧専用） | 不可 | システム |
| ユーザー定義 | 表示 | 可 | 可 | ユーザー |

CMS コア 8 表: `widget_types`, `placeholders`, `posts`, `postmeta`, `options`, `layouts`, `pages`, `users`

- **インフラ用テーブル** — `_sqlx_migrations` はマイグレーション管理用のため、DB 管理画面の一覧にも表示されず、直接 URL でアクセスしても閲覧・編集はできません。
- **CMS コアテーブル** — マイグレーションで定義された CMS 本体のテーブルです。一覧では種別「システム」と表示され、**データボタン**から行データの閲覧が可能です。データ画面は**閲覧専用モード**（列編集・テストデータ生成のツールバーは非表示）です。`users` も CMS コアテーブルとして同様の扱いです。
- **ユーザー定義テーブル** — 管理画面から新規作成したテーブルです。列編集・データ閲覧・テストデータ生成がすべて可能です。

アカウントの作成・編集・削除は `/admin/users` が担当します。DB 管理画面では `users` テーブルの生データ閲覧のみ可能で、列定義の変更はできません。

### ログイン

- 管理画面（`/admin/*`）はログイン必須です（`/admin/login`・`/admin/logout` を除く）
- **テストモード**（`cargo run -- --test`）: ログイン名 `admin`、パスワード **`testpass`**（起動のたびにこの値へ設定されます）
- **通常起動**（`cargo run`）: 初回起動時、DB に `admin` が無い場合はランダムな初期パスワードが **起動ログに一度だけ** 出力されます（`tracing` の `warn` レベル）。`/admin/login` で `admin` とそのパスワードを入力してログインしてください
- 本番環境ではセッション署名鍵を必ず設定してください（詳細は [セッション署名鍵](#セッション署名鍵securitysession_secret)）。未設定時は起動ごとにランダムな鍵が使われ、再起動で全セッションが無効になります
- REST API（`/api/v1/*`、セッション API を除く）はログイン必須です。CLI 等からは `POST /api/v1/session` に `{ "login", "password" }` を送り、返却される `Set-Cookie` を以降のリクエストに付与してください。管理画面（`/admin/login`）で取得した Cookie も共用できます

## 設定ファイル

設定は **`work/config.toml`** に保存します。初回起動時にリポジトリ直下の `config.example.toml` から自動生成されます（ルートに旧 `config.toml` がある場合はそちらを優先して移行します）。サイト名・説明は `/admin/settings` から編集でき、`options` テーブルと `[site]` セクションの両方に同期されます。

設定の優先順位は **デフォルト値 → `work/config.toml` → 環境変数（`CMS_*`）** です。

```toml
# config.example.toml

[server]
# リッスンアドレス（例: "127.0.0.1:3000"）。環境変数: CMS_BIND_ADDR
bind_addr = "127.0.0.1:3000"

[database]
# SQLite データベースファイルのパス。環境変数: CMS_DATABASE_PATH
path = "data/cms.db"

[paths]
# メディアのアップロード先（work 配下を推奨）。環境変数: CMS_PATHS__UPLOADS_DIR など
uploads_dir = "work/uploads"
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
| `CMS_SESSION_SECRET` | セッション署名用シークレット（`[security].session_secret` に対応） |
| `CMS_PATHS__UPLOADS_DIR` | アップロード先ディレクトリ（`paths.uploads_dir`） |
| `CMS_PATHS__WORK_DIR` | work ディレクトリ（`paths.work_dir`） |

一般的な `CMS_<SECTION>__<KEY>` 形式（`__` でネスト）でも上書き可能です。

### セッション署名鍵（`security.session_secret`）

管理画面（`/admin/*`）および REST API（`/api/v1/*`）のログイン状態は、署名付き Cookie セッションで保持されます。`security.session_secret` はその Cookie を HMAC 署名するための秘密鍵です。

#### なぜ設定が必要か

| 状態 | 挙動 |
|------|------|
| **未設定** | 起動のたびにランダムな鍵が生成される。再起動で既存セッションがすべて無効になる |
| **固定値を設定** | プロセス再起動後も同じ鍵で検証されるため、Cookie の有効期限内はログイン状態が維持される |

本番環境では必ず固定の秘密鍵を設定してください。未設定のまま運用すると、デプロイや再起動のたびに全ユーザーが再ログインを求められます。秘密鍵が推測可能だとセッション Cookie の改ざんリスクも高まるため、十分な長さのランダム文字列を使ってください。

#### 設定方法

優先順位は **環境変数 `CMS_SESSION_SECRET` → `work/config.toml` の `[security].session_secret` → 未設定（ランダム鍵）** です。

**方法 1: 環境変数（本番推奨）**

秘密鍵をファイルに書かず、プロセス起動時だけ注入する方法です。systemd・Docker・PaaS などのシークレット管理と相性が良いです。

```bash
# 例: 32 バイトのランダム値を Base64 エンコードして設定
export CMS_SESSION_SECRET="$(openssl rand -base64 32)"
cargo run
```

```bash
# 1 回だけ実行して起動
CMS_SESSION_SECRET="$(openssl rand -base64 32)" cargo run
```

**方法 2: 設定ファイル**

`work/config.toml` に直接書き込みます。ローカル開発や、設定ファイルのアクセス権限を厳しく管理できる環境向けです。

```toml
[security]
session_secret = "ここに十分な長さのランダム文字列を設定"
```

初回起動後に `work/config.toml` が自動生成されている場合は、`[security]` セクションのコメントを外して値を追記してください。`config.example.toml` にも同じ項目があります。

#### 秘密鍵の生成例

次のいずれかで、推測困難な文字列を生成できます。

```bash
openssl rand -base64 32
```

```bash
openssl rand -hex 32
```

生成した文字列をそのまま `session_secret` または `CMS_SESSION_SECRET` に設定します（引用符で囲む場合は TOML の文字列リテラルとして記述）。

#### 環境別の目安

| 環境 | 推奨 |
|------|------|
| **ローカル開発**（`cargo run -- --test` など） | 未設定でも可。再起動でセッションが切れるだけ |
| **ステージング・本番** | `CMS_SESSION_SECRET` などで固定値を必ず設定。値の変更・ローテーション時は全セッションが無効になる点に注意 |
| **複数インスタンス** | 同一の `session_secret` を全インスタンスで共有しないと、別インスタンスへ振られたリクエストでセッション検証に失敗します |

#### 関連設定

セッション Cookie の名前と有効期限は `[session]` セクションで変更できます（`cookie_name`・`max_age_secs`）。署名鍵とは独立した項目です。

## ページとレイアウト

公開サイトは **レイアウト**（共通 shell・CSS）と **ページ**（URL・本文テンプレート）の 2 層で構成します。メタ情報は DB、本文と shell は `work/layouts/{key}/` に保持します。公開時は常に [MiniJinja](https://github.com/mitsuhiko/minijinja) で評価します（`blogname` / `blogdescription` / `favicon_url` およびプレースホルダー変数（`news_html` / `main_carousel_html` など）を自動注入）。

```
work/layouts/default/
├── shell.html           # 共通枠（head / nav / footer）。{% extends "default/shell.html" %} + {% block content %} が標準
├── static/
│   └── site.css         # /static/default/site.css で配信（レイアウトごとに分離）
└── pages/
    ├── index.html       # トップ（url_path = /）
    └── page-3.html      # その他ページ（file_name はレイアウト内相対）
```

- **管理画面**: `/admin/layouts` で shell 編集・static アップロード・favicon（メディアから）選択、`/admin/pages` でページ CRUD（レイアウト所属必須）。`/admin/posts` でプレースホルダー配下の投稿を管理します。
- **静的アセット**: `/static/{layout_key}/*`（例: `/static/default/site.css`）。`/favicon.ico` は既定レイアウトの favicon_media_id から配信。
- **任意 URL**: プリセットからページを作成し URL を割り当てるとフォールバックで公開されます。
- **詳細設計**: [doc/LAYOUT_SPEC.md](doc/LAYOUT_SPEC.md)

テンプレートはランタイム差し替え可能（再起動不要、minijinja-autoreload）。管理画面 UI は `src/templates/admin/` の Askama に固定し、公開ページの影響を受けません。

## 開発者向け

設計思想、アーキテクチャ、データモデル、ロードマップ、実装進捗は [doc/PLAN.md](doc/PLAN.md) にまとめています。

## ライセンス

TBD（リポジトリオーナーが決定するまで未設定）
