# Round 9 P2-H: ratio_160 → ratio_min_wage 命名 cleanup

**日付**: 2026-05-10
**性質**: 命名 rename のみ (機能変更なし)

---

## 概要

Round 8 Agent A 監査で発見:
- `wage.rs:211-213` で `hourly_160` / `diff_160` / `ratio_160` という名前のまま、実体は `HOURLY_TO_MONTHLY_HOURS = 167` (`aggregator.rs:25`) で計算
- 旧 173.8 → 167 への移行が完了 (`salary_parser.rs:40-41`) したが変数名 _160 が残存

---

## 採用命名 (Agent H 推奨)

意味ベース、定数変更耐性、簡潔:

| 旧 | 新 | 意味 |
|---|---|---|
| `hourly_160` | `hourly_equiv` | 時給換算値 (equivalent) |
| `diff_160` | `diff_min_wage` | 最賃との差額 |
| `ratio_160` | `ratio_min_wage` | 最賃比 (倍率) |

---

## 実装変更

### `src/handlers/survey/report_html/wage.rs` (単一ファイル内 23 箇所)

| カテゴリ | 件数 | 内容 |
|---|---|---|
| 構造体フィールド | 3 | `MinWageEntry { hourly_equiv, diff_min_wage, ratio_min_wage }` (旧 `_160` 系) |
| ローカル変数 (let) | 3 | 232-234 行 |
| 構造体初期化 (shorthand) | 3 | 239-241 行 |
| フィールド参照 | 14 | sort/severity/format! 等 |
| コメント訂正 | 1 | `// 月給÷160h` → `// 月給÷167h (HOURLY_TO_MONTHLY_HOURS 経由)` |

### 影響範囲

| 種別 | 件数 |
|---|---|
| 他ファイルへの API 波及 | **0** (`MinWageEntry` は関数内 private struct) |
| テスト直接参照 | **0** (`tests/`, `*.py`, `*.ts` ヒットゼロ) |
| docs 内参照 | 過去ラウンド docs (Round 8) のみ、新規 docs で言及で十分 |

### 検証

- `grep -c "_160" wage.rs` = **0** (置換完了)
- `cargo check` 通過、24 warnings (既存と同レベル)
- `cargo test` 既存テスト破壊なし

---

## スコープ外 (将来 cleanup 候補)

Agent H 報告で「コメント残存 7 箇所 + guide.rs:153 UI 文言」も指摘されたが、本 PR では `wage.rs` の rename のみに限定。理由:
- 機能無影響な箇所 (コメント / docstring) を P2-H に含めるとスコープが膨れる
- guide.rs:153 のユーザー可視「160h」は別 PR で対応する方が安全 (UI レビュー要)

| 残 cleanup 対象 | 場所 | 優先 |
|---|---|---|
| `aggregator.rs:193,194,342` コメント `// ... ×160` | docstring | 低 |
| `upload.rs:74,76` 関数コメント `... x160` / `/160` | docstring | 低 |
| `hw_enrichment.rs:520,522` コメント `// ...160h 換算` | コメント | 低 |
| `guide.rs:153` UI 文言「最低賃金×160h」 | ユーザー可視 | 中 (別 PR 推奨) |

**保持必須** (絶対変更不可):
- `aggregator.rs:12, 21` 移行履歴・差分率説明
- `notes.rs:97` 「160h ではなく 167h」対比説明 (読者教育目的)
- `parser_aggregator_audit_test.rs` 全箇所 (旧 160h 値での逆証明テスト、`feedback_reverse_proof_tests.md` 準拠)

---

## 監査メタデータ

- 修正ファイル: 1 件 (wage.rs)
- 機械置換: replace_all で 23 箇所一括
- 個別判断: コメント 1 箇所のみ
- 既存テスト破壊: ゼロ
- API 波及: ゼロ (private struct)

**Round 9 P2-H 完了。PDF 表示文言・計算ロジックは不変、命名のみ更新。**
