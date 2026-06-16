# sqlparser クレート導入検討 — ビュー・ビジュアル編集のパース精度

このドキュメントは、ビュー定義のビジュアル編集機能における SQL パース処理に `sqlparser` クレート（[apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs)）を導入できるかを検討した結果をまとめたものです。

## 結論

**導入を推奨する。**

- **Phase 1（パーサー置き換え）**: 現行の手書きパーサー（約 600 行）を `sqlparser` + `SQLiteDialect` に置き換えることで、対応サブセット内の誤判定を減らし、保守性を向上できる。
- **Phase 2（JOIN 等の UI 拡張）**: ビジュアル編集を JOIN に対応させる場合、`sqlparser` の AST（`TableWithJoins` 等）が事実上の前提となる。手書きパーサーの拡張は現実的でない。

ただし、`sqlparser` は「あらゆる SQL をビジュアル化する」万能手段ではない。UI が表現できるデータモデル（`SimpleViewUiSpec`）の範囲内での精度向上と、将来拡張の基盤獲得が主な効果となる。

---

## 現状のアーキテクチャ

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
| フロント | ビルダー UI・タブ切替 | `assets/admin.js`, `src/templates/admin/database/view_form.html` |

### 対応している SQL（意図的なサブセット）

- 単一テーブル: `SELECT [DISTINCT] cols FROM table [alias] [WHERE ...]`
- 列: 単純列 / 式（例: `LENGTH("body")`）/ 別名（`AS` または暗黙別名）
- WHERE: トップレベル `AND` のみ（`OR`・括弧付き複合条件は非対応）

### 明示的に非対応（`supported: false`）

- `JOIN`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `UNION` 等
- テンプレートの注意書き: `view_form.html` — 「JOIN・集計・複雑な WHERE などを含む SQL は SQL 編集タブで編集」

### テスト基盤

- `database.rs` 内の unit テスト 30 ケース以上（`parse_simple_view_select_*`）
- `tests/admin_database.rs` の API 統合テスト（`/admin/database/views/ui-spec`）

移行時はこれらを回帰テストとしてそのまま利用できる。

---

## 手書きパーサーの弱点

現行実装はコメント除去・キーワード探索・カンマ/`AND` 分割を自前で行っており、構文木ベースではない。

### 1. 文字列リテラル内キーワードの誤検出（実害あり）

`find_sql_keyword()` は文字列リテラルを考慮しない。

```sql
SELECT id FROM notes WHERE body = 'copied FROM archive'
```

`'FROM archive'` 内の `FROM` が句境界と誤認され、パース失敗（`supported: false`）になりうる。`sqlparser` はトークナイザー段階でリテラルを保護するため、この系統の誤判定を解消できる。

### 2. 式・別名の境界判定

`split_top_level_commas` / `split_select_item_alias` は括弧・引用符の手動追跡に依存する。ネストした関数呼び出しや複雑な式で失敗しうる。`sqlparser` の `Expr` / `SelectItem` AST で安定化できる。

### 3. メンテナンスコスト

`strip_sql_comments`, `find_sql_keyword`, `split_top_level_and` 等が `database.rs`（6000 行超）に散在している。パース層の分離と `sqlparser` への置き換えで、今後の拡張（JOIN 等）の土台を作れる。

### 4. sqlparser だけでは解決しない点

| 課題 | 説明 |
|---|---|
| UI モデルの制約 | `SimpleViewUiSpec` は `base_table: String` のみ。JOIN UI 化にはデータモデル拡張が必要 |
| WHERE の表現 | 「列名 + サフィックス」への変換ロジックは引き続き自前実装が必要 |
| ラウンドトリップ | 式・WHERE サフィックスは原文保持が望ましい。`Spanned` トレイトの `Span` からスライスを取る方針がよい（`Display` 再生成は引用符・空白が変わるリスクあり）。一部ノード（`IS NULL` 等）は span が不完全 |
| SQLite 互換 | `SQLiteDialect` は主要構文をカバーするが、SQLite 固有の端数にギャップがありうる。POC で実データ検証が必要 |

---

## sqlparser の適合性

| 観点 | 評価 | 補足 |
|---|---|---|
| ライセンス・依存 | 推奨 | Pure Rust / Apache-2.0。crates.io 最新 0.62 系、`SQLiteDialect` あり |
| 現行サブセットの精度向上 | **有効** | 誤検出削減・式パース安定化 |
| JOIN 等の UI 拡張 | **必須に近い** | `TableWithJoins`, `JoinOperator`, `JoinConstraint` が AST として提供される |
| バイナリ・ビルド | 許容範囲 | 管理画面専用パス。`default-features = false` で影響を抑制可能 |
| 代替 crate | 次点 | `sqlite3-parser` は SQLite 特化だが、JOIN UI への汎用マッピングは `sqlparser` の方が容易 |

---

## 推奨アーキテクチャ

### Phase 1: パーサー置き換え（API / UI インターフェース不変）

```
SELECT 定義文字列
  → sqlparser (SQLiteDialect)
  → view_select_parse モジュール（新規）
  → ParsedSimpleViewSelect（既存型を維持）
  → build_simple_view_ui_spec() / build_simple_view_select()（既存）
```

**変更ファイル（最小）**

| ファイル | 変更内容 |
|---|---|
| `Cargo.toml` | `sqlparser = "0.62"`（`default-features = false` 推奨） |
| `src/services/view_select_parse.rs`（新規） | sqlparser アダプター |
| `src/services/database.rs` | `parse_simple_view_select()` を委譲。手書き lexer 群を削除 |

**アダプターの責務**

1. `Parser::parse_sql()` で `Statement::Query` を取得
2. `SetExpr::Select` のみ受理（`UNION` 等は `None`）
3. `from` が単一 `TableWithJoins` かつ `joins` が空であることを確認
4. `projection` から `ParsedViewColumnList` へ変換（`Span` で原文保持）
5. `selection`（WHERE）を `Expr::BinaryOp(And)` のフラット列に分解できれば `ParsedWhereCondition` へ（`OR`・サブクエリは拒否）
6. 既存の `build_simple_view_ui_spec()` / `build_simple_view_select()` はそのまま利用

### Phase 2: JOIN 等の UI 拡張（別タスク）

Phase 1 完了後に実施。`sqlparser` 導入は Phase 2 の前提条件。

| 領域 | 変更内容 |
|---|---|
| データモデル | `SimpleViewUiSpec` を拡張（例: `joins: Vec<ViewUiJoin>`） |
| パース | `TableWithJoins.joins` を `INNER JOIN` / `LEFT JOIN` 等にマッピング |
| 生成 | `build_simple_view_select()` を multi-table JOIN 対応に |
| UI | `view_form.html` に JOIN 行 UI（テーブル・結合種別・ON 条件） |
| フロント | `admin.js` の SQL 生成・カラム修飾を拡張 |
| 制約（初期） | `INNER JOIN` のみ、段階的に自己結合・3 テーブル以上を検討 |

---

## POC（導入判断用スパイク）

実装前に 1–2 日相当のスパイクで Go/No-Go を判断する。

### 手順

1. `sqlparser` を追加し、`view_select_parse.rs` に薄いアダプターを実装
2. 既存 `parse_simple_view_select_*` テストをすべて実行し、差分を記録
3. 新規回帰ケースを追加して改善効果を定量比較
4. ラウンドトリップ検証: `parse → build_simple_view_select → parse` で意味等価性を確認

### 新規回帰ケース（手書きパーサーとの比較用）

- 文字列内 `FROM` / `WHERE` / `AND`
- ネスト関数: `COALESCE(LOWER("body"), '')`
- `WHERE` 括弧: `(id > 0) AND (body IS NOT NULL)` — Phase 1 で対応するか方針決定
- SQLite 固有: `IS NOT NULL` / `NOTNULL`（`SQLiteDialect::supports_notnull_operator`）

### Go / No-Go 基準

| 判定 | 条件 |
|---|---|
| **Go** | 既存テスト 100% 通過 + 新規回帰で手書きより成功率が高い、または同等でコード量が削減 |
| **No-Go** | SQLite 固有構文で sqlparser が頻繁に失敗 → `sqlite3-parser` 併用またはハイブリッド（sqlparser 失敗時のみ手書き）を検討 |

---

## リスクと対策

| リスク | 対策 |
|---|---|
| `Span` 不完全で式原文が取れない | `Span` 優先。取れない場合は `Expr` の `Display` + テストで許容差分を定義 |
| sqlparser と SQLite 実機の差異 | `validate_view_definition()` で SQLite に `CREATE VIEW` を試行（現行フロー維持） |
| Phase 2 の UI 肥大化 | INNER JOIN 1 追加から段階リリース。複雑 SQL は SQL タブ推奨を維持 |
| `database.rs` の肥大化 | パース層は `view_select_parse.rs` へ分離（Phase 1 で実施） |

---

## 推奨ロードマップ

| フェーズ | 内容 | 目安 |
|---|---|---|
| Phase 1a | POC スパイク・Go/No-Go 判断 | 2 日 |
| Phase 1b | 本番アダプター実装（Span ベース原文保持） | 3 日 |
| Phase 1c | 手書きパーサー削除・回帰テスト追加 | 1 日 |
| Phase 2a | JOIN 対応 `ViewUiSpec` v2 設計 | 2 日 |
| Phase 2b | バックエンド（モデル・API・SQL 生成） | 5 日 |
| Phase 2c | フロントエンド（UI・admin.js） | 5 日 |

Phase 1 単体でもパース精度向上と保守性改善のメリットがあり、JOIN 拡張を見据えた場合も基盤として妥当である。

---

## 参考

- [sqlparser crate (crates.io)](https://crates.io/crates/sqlparser)
- [SQLiteDialect (docs.rs)](https://docs.rs/sqlparser/latest/sqlparser/dialect/struct.SQLiteDialect.html)
- [Spanned trait (docs.rs)](https://docs.rs/sqlparser/latest/sqlparser/ast/trait.Spanned.html)
- 関連ソース: `src/services/database.rs`, `assets/admin.js`, `src/templates/admin/database/view_form.html`, `tests/admin_database.rs`