# syntaqlite クレート導入検討 — ビュー・ビジュアル編集のパース精度

このドキュメントは、ビュー定義のビジュアル編集機能における SQL パース処理に [syntaqlite](https://github.com/LalitMaganti/syntaqlite) クレートを導入できるかを検討した結果をまとめたものです。

関連: [sqlparser 導入検討](./sqlparser-view-visual-editing.md)

## 結論

**本プロジェクト（SQLite 専用 CMS）では sqlparser より syntaqlite を優先検討する価値が高い。**

- **パース精度**: SQLite 本体の Lemon 文法・トークナイザーをベースにしており、「SQLite が受理する SQL」との一致度が最も高い。
- **Phase 1（パーサー置き換え）**: 手書きパーサー（約 600 行）の置き換え先として、誤判定削減・原文保持・保守性向上の効果が期待できる。
- **Phase 2（JOIN 等の UI 拡張）**: `SelectStmt` / `JoinClause` / `JoinType` 等の SQLite ネイティブ AST があり、JOIN ビジュアル編集の基盤として適している。

一方で、**0.x 系 API の安定性**と**ビルド依存（C コンポーネント同梱）**は sqlparser より重い。導入前に POC でビルド・API 調査が必須。

---

## syntaqlite とは

| 項目 | 内容 |
|---|---|
| 目的 | SQLite SQL のパース・フォーマット・静的解析・LSP |
| 文法 | SQLite 本体の [Lemon 文法](https://www.sqlite.org/lemon.html) を C からラップ |
| 方針 | 「SQLite が受理すれば syntaqlite もパース、SQLite が拒否すれば syntaqlite も拒否」 |
| ライセンス | Apache-2.0（SQLite 部分は public domain） |
| 現行版 | crates.io `0.5.10`（2026 年時点。README は 0.x で API 変更ありうると明記） |

公式ベンチマーク（[Comparison to other tools](https://docs.syntaqlite.com/latest/reference/comparison/)）では、SQLite 固有構文 40 件のパース正答率が **syntaqlite 100%** に対し **sqlparser-rs 65%**。SQLite 上流テストスイート約 39.6 万文との比較で **約 99.7% の一致**と報告されている。

本 CMS は sqlx + SQLite のみを使うため、この「SQLite 特化」の強みが直接活きる。

---

## 現状のアーキテクチャ（共通）

ビューのビジュアル編集は次の流れで動作する。

```
SQL textarea
  → POST /admin/database/views/ui-spec
  → parse_simple_view_select()
  → SimpleViewUiSpec
  → admin.js ビルダー
  → build_simple_view_select()
  → SQL textarea
```

| 層 | 実装 | ファイル |
|---|---|---|
| API エントリ | `resolve_view_ui_spec()` | `src/services/database.rs` |
| パース | `parse_simple_view_select()` + 補助関数群（L1286–L2290 付近） | 同上 |
| UI 状態 | `SimpleViewUiSpec` | 同上 |
| SQL 生成 | `build_simple_view_select()` | 同上 |

### 対応している SQL（意図的なサブセット）

- 単一テーブル: `SELECT [DISTINCT] cols FROM table [alias] [WHERE ...]`
- 列: 単純列 / 式 / 別名
- WHERE: トップレベル `AND` のみ

### 明示的に非対応（`supported: false`）

- `JOIN`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `UNION` 等（UI モデル `base_table: String` の制約）

---

## 手書きパーサーの弱点（syntaqlite で改善できる点）

### 1. 文字列リテラル内キーワードの誤検出

現行 `find_sql_keyword()` はリテラル内の `FROM` / `WHERE` / `AND` を句境界と誤認しうる。

```sql
SELECT id FROM notes WHERE body = 'copied FROM archive'
```

syntaqlite は SQLite トークナイザーでリテラルを保護するため、この系統の誤判定を根本的に防げる。

### 2. SQLite 固有構文の取りこぼし

手書きパーサーは `NOTNULL`（`IS NOT NULL` の別表記）、`IS NOT DISTINCT FROM`、数値リテラルの `_` 区切り等の SQLite 固有表現を個別実装していない。syntaqlite は文法レベルで追随する。

### 3. 式・コメント・別名の境界

ネスト関数・インラインコメント・暗黙別名など、手書きの括弧/引用符追跡より `ResultColumn` / `Expr` AST の走査の方が安定する。

### 4. syntaqlite だけでは解決しない点

| 課題 | 説明 |
|---|---|
| UI モデルの制約 | `SimpleViewUiSpec` は単一 `base_table`。JOIN UI 化にはモデル拡張が別途必要 |
| WHERE の表現 | UI は「列名 + サフィックス」形式。`Expr` からの変換ロジックは自前実装が必要 |
| 対応サブセットの境界 | パースできても `GROUP BY` 付き SELECT をビジュアル化するわけではない（意図的に `supported: false` とする判定は残る） |

---

## syntaqlite の適合性

### ビュー・パースに使える API

| API / 型 | 用途 |
|---|---|
| `Parser` + `ParseOutcome` | SELECT 文のパース |
| `nodes::SelectStmt` | `columns()`, `from_clause()`, `where_clause()`, `flags()`（DISTINCT 等） |
| `nodes::ResultColumn` | `expr()`, `alias()` — SELECT 列の復元 |
| `nodes::JoinClause` | `join_type()`, `left()`, `right()`, `on_expr()`, `using_columns()` — JOIN 拡張用 |
| `nodes::TableSource` / `TableRef` | FROM 句のテーブル名・エイリアス |
| `nodes::Expr` / `BinaryExpr` / `IsExpr` | WHERE 条件の分解 |
| `source::DocRange` / `StmtRange` | バイトオフセットによる原文スライス（ラウンドトリップ向け） |
| `Analyzer` + `Catalog`（`validation` feature） | スキーマを渡した静的解析（列名・関数の検証） |
| `Formatter`（`fmt` feature） | SQL の正規化（オプション） |

`SelectStmt` はライフタイム付き参照（`'a`）で元 SQL 文字列に紐づく設計。パース結果と原文の対応が取りやすい。

### sqlparser との比較（本プロジェクト視点）

| 観点 | syntaqlite | sqlparser |
|---|---|---|
| SQLite パース精度 | **高い**（SQLite 文法ベース、ベンチ 100%） | 中程度（汎用 + `SQLiteDialect`、ベンチ 65%） |
| 実機との一致 | SQLite が受理するかとほぼ一致 | 方言ギャップがありうる |
| 原文保持 | バイトオフセット型（`DocRange` 等）が充実 | `Spanned` は一部ノードで不完全 |
| スキーマ検証 | `Analyzer` + `Catalog` で DB 接続なしに列解決 | パースのみ（検証は別途） |
| 依存の軽さ | C コンポーネント同梱、ビルドやや重い | Pure Rust、軽量 |
| API 安定性 | 0.x、変更ありうる | 比較的成熟 |
| JOIN AST | `JoinClause`（SQLite ネイティブ） | `TableWithJoins`（汎用） |
| エコシステム | 新しめ（2025–2026 活発） | DataFusion 等で広く利用 |

**SQLite 専用 CMS では syntaqlite の方がパース精度面で有利。** 汎用性・依存の軽さを最優先するなら sqlparser が妥当。

### 追加メリット: Analyzer によるスキーマ連携

`resolve_view_ui_spec()` は既に `table_columns_for_ui()` で PRAGMA から列情報を取得している。これを `Catalog` に流し込めば、パースと同時に次が可能になる。

- 存在しない列参照の検出
- 関数 typo の did-you-mean 提案
- ビジュアル編集タブ切替前の早期警告

これはパース精度というより**ユーザー体験の向上**だが、syntaqlite 固有の差別化ポイントになる。

---

## 推奨アーキテクチャ

### Phase 1: パーサー置き換え（API / UI 不変）

```
SELECT 定義文字列
  → syntaqlite::Parser
  → view_select_parse モジュール（新規）
  → ParsedSimpleViewSelect（既存型を維持）
  → build_simple_view_ui_spec() / build_simple_view_select()（既存）
```

**変更ファイル（最小）**

| ファイル | 変更内容 |
|---|---|
| `Cargo.toml` | `syntaqlite = { version = "0.5", default-features = false, features = ["sqlite"] }`（必要なら `validation` を追加） |
| `src/services/view_select_parse.rs`（新規） | syntaqlite アダプター |
| `src/services/database.rs` | `parse_simple_view_select()` を委譲。手書き lexer 群を削除 |

**アダプターの責務**

1. `Parser::new().parse(sql)` で文を取得し、`SelectStmt` に到達
2. `CompoundSelect`（`UNION` 等）や `WithClause`（CTE）があれば `None`（Phase 1 は現行仕様維持）
3. `from_clause()` が単一テーブルで `JoinClause` がないことを確認
4. `columns()` の `ResultColumn` から `ParsedViewColumnList` へ変換
   - 式列: `StmtRange` で原文スライスを保持（`Display` 再生成はフォールバック）
5. `where_clause()` の `Expr` をフラットな `AND` 列に分解できれば `ParsedWhereCondition` へ
6. `SelectStmtFlags` から `distinct` を取得

**Cargo 依存の目安**

```toml
# パースのみ（最小）
syntaqlite = { version = "0.5", default-features = false, features = ["sqlite"] }

# スキーマ検証も使う場合
syntaqlite = { version = "0.5", default-features = false, features = ["sqlite", "validation"] }
```

`fmt` / `lsp` はビュー・パースには不要。ビルド時間・バイナリサイズ削減のため default-features は無効化を推奨。

### Phase 2: JOIN 等の UI 拡張

Phase 1 完了後。ユーザー要望として JOIN 対応も視野に入れる場合の設計。

| 領域 | syntaqlite との対応 |
|---|---|
| データモデル | `SimpleViewUiSpec` に `joins: Vec<ViewUiJoin>` を追加 |
| パース | `TableSource` ツリーを走査し `JoinClause` を抽出。`JoinType` → UI の結合種別 |
| ON 条件 | `on_expr()` を文字列または構造化で保持。`using_columns()` は Phase 2 後半 |
| 列修飾 | `ColumnRef` の qualifier とテーブルエイリアスを照合 |
| 生成 | 既存 `build_simple_view_select()` を multi-table 対応に拡張 |
| 制約（初期） | `INNER JOIN` のみ、2 テーブルから段階的に |

syntaqlite の `JoinClause` は `left` / `right` / `on_expr` を直接持つため、JOIN UI へのマッピングは sqlparser と同等以上に自然。

---

## POC（導入判断用スパイク）

### 手順

1. `syntaqlite` を追加し、`cargo build` が CI 環境で通るか確認（C ツールチェーン要否）
2. `view_select_parse.rs` に薄いアダプターを実装
3. 既存 `parse_simple_view_select_*` テストをすべて実行
4. sqlparser / 手書きパーサーとの三者比較用ケースを追加
5. `StmtRange` による原文スライスでラウンドトリップが壊れないか検証

### 新規回帰ケース

| カテゴリ | 例 |
|---|---|
| リテラル内キーワード | `WHERE body = 'copied FROM archive'` |
| SQLite 固有 | `WHERE body NOTNULL`, `IS NOT DISTINCT FROM` |
| 複雑な式 | `COALESCE(LOWER("body"), '')`, `LENGTH("body")` |
| コメント混在 | `SELECT /* x */ DISTINCT "id" FROM t` |
| JOIN（Phase 2 準備） | `SELECT a.id FROM t1 a JOIN t2 b ON a.id = b.ref` がパース可能か |

### Go / No-Go 基準

| 判定 | 条件 |
|---|---|
| **Go** | 既存テスト 100% 通過 + 手書きパーサーより新規ケース成功率が高い + CI ビルド安定 |
| **条件付き Go** | ビルドは通るが API が複雑 → アダプター層で隠蔽し 0.x 更新は adapter のみ追従 |
| **No-Go** | クロスコンパイル/CI で C ビルドが不安定 → sqlparser にフォールバック（[別 doc](./sqlparser-view-visual-editing.md) 参照） |

---

## リスクと対策

| リスク | 影響 | 対策 |
|---|---|---|
| 0.x API 変更 | アダプター修正コスト | `view_select_parse.rs` に隔離。バージョンを Cargo.lock で固定 |
| C コンポーネントのビルド | CI・クロスコンパイル | POC で Linux/macOS/Windows を確認。問題あれば sqlparser へ切替 |
| デフォルト feature の肥大 | ビルド時間増 | `default-features = false`, `sqlite` のみ有効化 |
| AST 走査の学習コスト | 実装遅延 | `syntaqlite parse` CLI で AST を確認しながら開発 |
| パース成功 ≠ UI 対応 | ユーザー混乱 | 現行と同様、UI モデル外は `supported: false` を明示 |
| Formatter による正規化 | ユーザー SQL の書き味が変わる | Phase 1 では Formatter を使わない。生成は既存 `build_simple_view_select()` を維持 |

---

## sqlparser との選定指針

```
SQLite パース精度を最優先？
  ├─ Yes → syntaqlite を POC（推奨）
  └─ No（Pure Rust・軽量・汎用性優先）→ sqlparser

JOIN UI 拡張を見据える？
  ├─ Yes → どちらも AST 基盤として有効。syntaqlite は SQLite 実機一致で有利
  └─ No → Phase 1 の誤判定削減だけならどちらでも可。syntaqlite の方が上限は高い

CI ビルドがシンプルであることを最優先？
  └─ sqlparser（Pure Rust）
```

本プロジェクトは **SQLite のみ**・**ビュー定義は SQLite が実際に実行する**ため、syntaqlite の「実機と同じ文法」はパース精度の観点で最も説得力がある。

---

## 推奨ロードマップ

| フェーズ | 内容 | 目安 |
|---|---|---|
| Phase 0 | syntaqlite POC（ビルド確認 + 既存テスト通過） | 2 日 |
| Phase 1a | アダプター実装（`StmtRange` 原文保持） | 3 日 |
| Phase 1b | 手書きパーサー削除 + 回帰テスト追加 | 1 日 |
| Phase 1c（任意） | `Catalog` 連携で列参照の事前検証 | 2 日 |
| Phase 2a | JOIN 対応 `ViewUiSpec` v2 設計 | 2 日 |
| Phase 2b | `JoinClause` マッピング + バックエンド | 5 日 |
| Phase 2c | フロントエンド（JOIN UI） | 5 日 |

Phase 0 でビルドが問題なければ Phase 1 へ進む。問題があれば [sqlparser 検討 doc](./sqlparser-view-visual-editing.md) の Phase 1 を代替とする。

---

## 参考

- [syntaqlite (crates.io)](https://crates.io/crates/syntaqlite)
- [syntaqlite Rust API (docs.rs)](https://docs.rs/syntaqlite/latest/syntaqlite/)
- [Comparison to other tools](https://docs.syntaqlite.com/latest/reference/comparison/)
- [SelectStmt](https://docs.rs/syntaqlite/latest/syntaqlite/nodes/struct.SelectStmt.html)
- [JoinClause](https://docs.rs/syntaqlite/latest/syntaqlite/nodes/struct.JoinClause.html)
- [source モジュール（バイトオフセット）](https://docs.rs/syntaqlite/latest/syntaqlite/source/index.html)
- 関連ソース: `src/services/database.rs`, `assets/admin.js`, `src/templates/admin/database/view_form.html`, `tests/admin_database.rs`
- 比較対象: [sqlparser 導入検討](./sqlparser-view-visual-editing.md)