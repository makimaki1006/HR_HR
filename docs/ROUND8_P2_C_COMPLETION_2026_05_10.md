# Round 8 P2-C 完了報告

**日付**: 2026-05-10
**性質**: 機能完了 (実装 + ローカル PDF 検証 PASS)
**前提**: ユーザー方針確定 (P2-C は (a)+(b) のみ、(c) 最賃比指標は P2-D 別ラウンド)

---

## P2-C 概要

`src/handlers/survey/report_html/helpers.rs:936-987` の **47 県最低賃金ハードコード**を解除し、`v2_external_minimum_wage` テーブル (Local 47 行 / Turso 同期済) から SELECT で取得する経路を追加。**DB 優先 + ハードコード fallback** 設計で、DB 不在時は従来通り動作。

### スコープ確定 (ユーザー判断)

| ID | 内容 | 本ラウンド |
|---|---|---|
| (a) helpers.rs ハードコード解除 | DB SELECT に置換 | ✅ |
| (b) wage.rs 利用箇所を ctx 統一 | render_section_min_wage に db/turso 引数追加 | ✅ |
| (c) P2-A 推奨アクションへの「最賃比 X 倍」追加 | 月給→時給換算の所定労働時間設計が必要 | ❌ P2-D 別ラウンド |

(c) を分離した理由 (ユーザー指摘):
- 月給中央値 ÷ (最低賃金 × 月間所定労働時間) の前提注記が必要
- 雑に入れると説明不能な指標になる
- まず DB 接続自体の正確性を確認すべき

---

## 実装変更

### `src/handlers/survey/report_html/wage.rs`

| 変更 | 内容 |
|---|---|
| use 追加 | `crate::db::local_sqlite::LocalDb`, `crate::db::turso_http::TursoDb`, `std::collections::HashMap` |
| `render_section_min_wage` シグネチャ拡張 | `(html, agg)` → `(html, agg, db: Option<&LocalDb>, turso: Option<&TursoDb>)` |
| 関数内 SQL | `SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage` 1 回実行 |
| HashMap 構築 | DB 結果から `HashMap<String, i64>` (47 県分) |
| lookup ロジック | `wage_map.get(&p.name).copied().or_else(\|\| min_wage_for_prefecture(&p.name))` (DB 優先 + ハードコード fallback) |

### `src/handlers/survey/report_html/mod.rs`

| 変更 | 内容 |
|---|---|
| 呼び出し更新 | `render_section_min_wage(&mut html, agg)` → `render_section_min_wage(&mut html, agg, db, turso)` |

### `src/handlers/survey/report_html/helpers.rs`

| 変更 | 内容 |
|---|---|
| `min_wage_for_prefecture` ハードコード版 | **不変** (fallback として残置、既存テスト不変) |

---

## DB 値 vs ハードコード値 差分監査

事前検証 (read-only) で 47 県中 **3 県不一致** を確認:

| 県 | hardcode | DB | delta |
|---|---:|---:|---:|
| 栃木県 | 1,058 | 1,068 | +10 |
| 福島県 | 1,038 | 1,033 | -5 |
| 香川県 | 1,038 | 1,036 | -2 |

→ DB 接続により「最低賃金改定遅延 / 補正値の反映」がレポートに自動反映される。

### スキーマ (`v2_external_minimum_wage`)

```
prefecture        TEXT  NOT NULL  PRIMARY KEY
hourly_min_wage   INTEGER NOT NULL
effective_date    TEXT  NOT NULL  DEFAULT '2025-10-01'
fiscal_year       INTEGER NOT NULL  DEFAULT 2025
```

---

## ローカル PDF 検証結果

| 指標 | 結果 |
|---|---|
| pages | 27 (P2-B と同じ) |
| 福島県 (DB 1,033 vs ハードコード 1,038) | ✅ **「1,033」が 2 件出現、「1,038」は 0 件** = DB 値表示確認 |
| 「最低賃金」セクション出現 | 23 件 (既存表示崩壊なし) |
| 既存セクション (P0-1 / P0-2 / P2-A / P2-B) | ✅ regression なし |

栃木県 / 香川県は本ラウンドの fixture (indeed_test_50.csv) に求人が無いため出現 0、本番 CSV に該当県が含まれれば DB 値が反映される。

### 完了条件 (ユーザー方針)

| 条件 | 結果 |
|---|---|
| (a) helpers.rs ハードコード解除 | ✅ 関数本体は残しつつ、優先順位を DB → fallback に変更 |
| (b) wage.rs 利用箇所を ctx 統一 | ✅ db/turso 引数経由 |
| DB 既存表示と DB 値の一致確認 | ✅ 福島県で 1,033 (DB 値) 出力確認、ハードコード旧値 1,038 は 0 件 |
| 既存テスト不変 | ✅ `min_wage_for_prefecture` ハードコード版残置、mod.rs:1417 のテストは継続動作 |

---

## 残課題

| ID | 内容 | 優先 |
|---|---|---|
| **P2-D** | 最賃比指標を設計して P2-A 推奨文に追加するか判断 (月給→時給換算の所定労働時間定義 + 前提注記) | 中 |
| P2-E | 4 象限図の点数不足・サンプル説明調整 | 中 |
| Model F2 監査 | recruiting_scores 全職業 score 同値 (104 自治体) | 低-中 |
| 別 | helpers.rs:989 `_MIN_WAGE_NATIONAL_AVG: i64 = 1121` も DB 集計値に置換するか判断 | 低 |

---

## 監査メタデータ

- 着手: 2026-05-10
- ローカル PDF PASS: 2026-05-10 (固定手順遵守: PDF 削除→再生成、福島県 DB/ハードコード差分で実証)
- DB 書込: ゼロ (READ-ONLY SELECT のみ)
- 既存テスト破壊: ゼロ (cargo check 24 warnings = 既存と同レベル)

**Round 8 P2-C はローカル PDF 実物で PASS 判定。本番 push 後の Render PDF 検証で最終完了。**
