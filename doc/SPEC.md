# 設計仕様 (SPEC)

開発者向けの設計思想・アーキテクチャ・データモデル・機能仕様・ロードマップ。  
バージョン **0.2.3** 時点の実装に基づく。

## 概要

Rust + SQLite を基盤とした軽量 CMS。公開サイトの HTML テンプレート（MiniJinja）にプレースホルダーを置き、管理画面からウィジェットとして編集する。カルーセル・お知らせ・お問い合わせフォームなどの動的ブロックに加え、ユーザー定義テーブル・ビューの管理 UI も提供する。

- 単一バイナリ配布、永続化は SQLite のみ
- 公開サイト: MiniJinja（`work/layouts/`）、管理画面: Askama
- ビジネスロジックはサービス層に集約し、HTML ルートと REST API の両方から利用
- セッションは署名付き Cookie（`tower-sessions`）

## 技術スタック

| 領域 | 採用技術 |
|------|----------|
| HTTP | Axum 0.8 |
| 公開テンプレート | MiniJinja 2.x + minijinja-autoreload |
| 管理画面 | Askama 0.16 |
| DB | SQLite（sqlx 0.9、マイグレーション） |
| 設定 | figment（TOML + `CMS_` 環境変数） |
| 認証 | tower-sessions、Argon2 |
| ビュー SQL パース | syntaqlite 0.5 |
| バックアップ | zip（deflate） |
| ログ | tracing |

## アーキテクチャ

### レイヤ構成

```
routes/          HTTP（管理画面 / 公開サイト / REST API）
services/        ビジネスロジック・バリデーション・ファイル I/O
repos/           DB アクセス
models/          ドメイン型・入力検証
widgets/         ウィジェット描画・レンダリングコンテキスト
theme/           公開テンプレートエンジン（MiniJinja）
page_render.rs   公開ページ描画
```

`repos` は DB 操作のみ。`services` がドメインルールと複数リポジトリの調整を担う。

### 起動フロー

1. `work/config.toml` を用意（無ければ `config.example.toml` からコピー）
2. SQLite 接続、`migrations/` 適用
3. 既定 options・管理ユーザー（`admin`）の確認
4. `work/layouts/` の初期シード、ブートストラップレイアウトの確認
5. `work/uploads/` の確認、トップページ（`url_path = '/'`）の確認
6. Axum サーバー起動

`cargo run -- --test` では admin パスワードが常に `testpass` になる。

### ルーティング

| パス | 用途 |
|------|------|
| `/` | 公開トップ・公開済みページ（フォールバック） |
| `/static/{layout_key}/*` | レイアウト配下の静的ファイル |
| `/uploads/*` | メディア配信 |
| `/contact/{placeholder_id}` | お問い合わせフォーム POST |
| `/admin/*` | 管理画面（`/admin/login` を除き認証必須） |
| `/api/v1/*` | REST API（セッション認証） |

ページ URL として使えない予約パス: `/admin`, `/static`, `/uploads` およびその配下。

## 設定

`work/config.toml`（初回起動時に自動生成）

| セクション | 項目 | 環境変数 |
|------------|------|----------|
| `[server]` | `bind_addr` | `CMS_BIND_ADDR` |
| `[database]` | `path` | `CMS_DATABASE_PATH` |
| `[paths]` | `work_dir`, `uploads_dir` | `CMS_PATHS__*` |
| `[site]` | `title`, `tagline` | — |
| `[session]` | `cookie_name`, `max_age_secs` | — |
| `[security]` | `session_secret` | `CMS_SESSION_SECRET` |

`session_secret` はセッション Cookie とお問い合わせフォームトークンの署名に使う。未設定時は開発用固定値（本番では必ず上書き）。

サイト名・説明は `options.blogname` / `options.blogdescription` にも保存され、`/admin/settings` から編集できる。

## ディレクトリ

```
work/
  config.toml
  layouts/{layout_key}/
    shell.html          共通枠
    pages/              ページ本文
    static/             CSS・画像など
  uploads/              メディア
data/cms.db             SQLite（パスは設定可）
migrations/             マイグレーション
presets/                初回シード・サンプルセット
assets/                 管理画面 CSS/JS（バイナリ埋め込み）
src/templates/          管理画面 Askama テンプレート
tests/                  統合テスト
```

## データモデル

日時は UTC の ISO8601 文字列（TEXT）。

### CMS コアテーブル

| テーブル | 役割 |
|----------|------|
| `widget_types` | ウィジェット種別（HTML・設定スキーマ・config） |
| `placeholders` | テンプレート参照名とウィジェットの紐付け |
| `posts` | 投稿・メディア添付（`post_type`: post / attachment） |
| `postmeta` | 投稿メタ（画像の float/margin 等） |
| `options` | サイト設定 |
| `layouts` | レイアウト（`key` ↔ `work/layouts/{key}/`） |
| `pages` | ページメタ（URL・公開状態。本文はファイル） |
| `users` | 管理ユーザー |
| `user_table_meta` | DB 管理 UI の列幅・ソート・フィルター・ビュー編集モード |

投稿ステータス: `draft` / `publish` / `future` / `trash`。メディアは `post_type = 'attachment'`, `post_status = 'inherit'`。

### 組み込みウィジェット

| type_key | 説明 |
|----------|------|
| `news` | 公開済み投稿を新しい順に表示 |
| `image` | メディア画像 1 枚 |
| `carousel` | 画像スライドショー |
| `contact_form` | お問い合わせ（HMAC トークン・PRG・二重送信防止） |

カスタムタイプの追加とエクスポート/インポートに対応。`type_key` は英小文字始まり、英小文字・数字・アンダースコアのみ。

### ページ・レイアウト

- 本文: `work/layouts/{layout_key}/pages/{file_name}`
- **公開差し替え**: 指定レイアウトのページを一括公開し、既存の公開ページは非公開化・URL 解除（`swap_live_layout`）

### テンプレート変数

プレースホルダー名 `{name}` に対し注入される:

- `{name}_html` — レンダリング済み HTML（`| safe` 必須）
- `has_{name}` — データ有無
- `{name}` — 構造化データ（種別による）

プレビュー時は `cms-widget-target` ラッパーと `data-cms-placeholder-id` を付与。

## 機能仕様

### 公開サイト

- `/` は `url_path = '/'` かつ公開中のページ
- その他 URL はフォールバックで `pages` を参照
- カスタム `public_url` のメディアとファビコン（`/favicon.ico`）を配信

### 管理画面

| パス | 機能 |
|------|------|
| `/admin` | ダッシュボード |
| `/admin/posts` | プレースホルダー・投稿・ゴミ箱 |
| `/admin/pages` | ページ作成・編集・プレビュー |
| `/admin/layouts` | レイアウト・公開差し替え・静的ファイル・ZIP 入出力 |
| `/admin/widgets` | ウィジェットタイプ（HTML・設定スキーマ） |
| `/admin/media` | メディアライブラリ |
| `/admin/settings` | サイト名・説明 |
| `/admin/users` | 管理ユーザー |
| `/admin/samples` | サンプルセットのインストール |
| `/admin/backup` | バックアップ ZIP |
| `/admin/database` | テーブル・ビュー |

管理画面 CSS/JS/favicon はバイナリ埋め込み。

### 投稿・プレースホルダー

- 投稿はプレースホルダー配下で管理。ゴミ箱 → 復元 → 完全削除（purge）
- プレースホルダー削除時、アクティブな投稿は自動でゴミ箱へ移動してから削除
- 参照中のプレースホルダーがあるウィジェットタイプは削除不可

### レイアウト

ZIP のエクスポート/インポート（新規・上書き・スキップ・リネーム）、複製、静的ファイル管理。インポート時は URL 衝突を検出し接頭辞で回避。リネーム時はテンプレート内のレイアウトキー参照を書き換える。

### メディア

画像のアップロード・削除、`public_url` 設定（ページ URL や重複との衝突を拒否）。ファビコンは画像のみ。

### DB 管理

**テーブル種別と操作権限**

| 種別 | 例 | 列編集 | 閲覧 | セル編集 | シード |
|------|-----|--------|------|----------|--------|
| インフラ | `_sqlx_migrations`, `sqlite_*` | × | × | × | × |
| CMS コア | `posts`, `users` 等 | × | ○（読み取り専用） | × | × |
| ユーザー定義 | 任意 | ○ | ○ | ○ | ○ |

システムテーブル行は初期非表示（トグルで表示）。

**ユーザー定義テーブル**: 作成・列編集（追加/削除/リネーム、NOT NULL の緩和のみ。型変更不可）。型は integer / real / text / blob / timestamp / boolean。複製（スキーマのみ or データ込み）。データ一覧は 1000 行/チャンク、列幅・ソート・フィルターを `user_table_meta` に保存。PK 列以外はインライン編集可。テストデータ生成は SSE で進捗通知（最大 1,000 万行）。

**ユーザー定義ビュー**: SQL モードで SELECT 句を直接編集。UI モードは単一テーブルの単純 SELECT を GUI 化（列選択・エイリアス・式、`DISTINCT`、WHERE）。JOIN / GROUP BY / ORDER BY / LIMIT / サブクエリは非対応で SQL モードを使う。パースは syntaqlite（`view_select_parse.rs`）。編集モード（`sql` / `ui`）は `user_table_meta.view_edit_mode` に保存。データ閲覧は読み取り専用だが、ソート・フィルター・列幅はテーブルと同様。

### サンプルセット

| 種別 | key | 内容 |
|------|-----|------|
| レイアウト | `corporate` | コーポレート（4 ページ・6 ウィジェット） |
| レイアウト | `bicycle` | 自転車屋（同構成） |
| DB | `corporate-tables` | サービス・チームのテーブルとビュー |
| DB | `distinct-tables` | 社員テーブル 1 件と DISTINCT デモ用ビュー 3 件 |

再インストール時は上書き/スキップ等の衝突解決を経る。

### バックアップ

エクスポート: マニフェスト + DB + `work/layouts/` + `work/uploads/`（フォーマット v1）。インポート時はマニフェスト・パス・バージョンを検証。

### REST API（`/api/v1`）

| パス | 操作 |
|------|------|
| `/session` | ログイン・確認・ログアウト |
| `/layouts`, `/pages` | CRUD |
| `/placeholders` | CRUD + 配下投稿 |
| `/posts/{id}` | 取得・更新・削除 |
| `/widgets` | 一覧・インポート・更新・削除 |
| `/widgets/{type_key}/export` | エクスポート |
| `/media` | 一覧 |
| `/settings` | サイト設定 |

未認証は 401。管理画面のセッション Cookie でも認証できる。

### 認証

パスワードは Argon2。`admin` は削除不可。ロールは `administrator` のみ（細かい権限分離は未実装）。

## ロードマップ

| 項目 | 状態 |
|------|------|
| 権限分離（編集者・閲覧者等） | 未対応 |
| カテゴリ・タグ | 未対応 |
| メニュー | 未対応 |
| RSS | 未対応 |
| 予約公開（`future` の自動切替） | スキーマのみ |
| EC（商品・注文） | 未対応 |
| 外部 DB | 非方針 |
| API レート制限 | 未対応 |

## テスト

`tests/` に統合テスト約 160 件。`TestApp`（`common.rs`）が一時ディレクトリ上に独立した DB と work ディレクトリを構築する。機能仕様の回帰とエッジケースをカバーし、DB 管理（`admin_database.rs`、81 件）が最も網羅的。

| ファイル | 主な検証内容 |
|----------|--------------|
| `admin_auth.rs` | 認証・リダイレクト |
| `admin_users.rs` | ユーザー CRUD |
| `admin_database.rs` | テーブル・ビュー・UI ビルダー・ソート/フィルター・シード |
| `api_auth.rs`, `api_smoke.rs` | API 認証・CRUD |
| `backup_export_import.rs` | バックアップ往復 |
| `contact_form.rs` | フォーム送信・二重送信防止 |
| `layouts_export_import.rs` | レイアウト ZIP |
| `layout_publish_swap.rs` | 公開差し替え |
| `layout_static_files.rs` | 静的ファイル |
| `media_public_url.rs` | メディア URL・ファビコン |
| `placeholders_delete.rs` | プレースホルダー削除と投稿のゴミ箱移動 |
| `posts_trash.rs` | ゴミ箱・復元・purge |
| `preview_edit_mode.rs` | プレビュー編集 UI |
| `samples_install.rs` | サンプルインストール |
| `services_pages.rs` | ページファイル永続化 |
| `widgets_export_import.rs`, `widgets_html.rs` | ウィジェット |

実装の詳細は `src/services/` と上記テストを参照。