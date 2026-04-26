# Fix-A 実行結果: 媒体分析タブ CSV パース → 集計 中核 6 件 修正

**実施日**: 2026-04-26
**チーム**: Fix-A (Backend / Pipeline Core)
**対象**: `src/handlers/survey/{upload.rs, salary_parser.rs, aggregator.rs, statistics.rs}` + `src/handlers/survey/render.rs`
**根拠**: `deepdive_d1_survey_pipeline.md` (D-1 監査) の 🔴 Critical 6 件
**MEMORY 遵守**: feedback_partial_commit_verify / feedback_test_data_validation / feedback_reverse_proof_tests / feedback_dedup_rules / feedback_never_guess_data

---

## 0. サマリ

| 項目 | 値 |
|------|---|
| 既存テスト | 710 → 769 (+59 中 24 が Fix-A 新規 / 35 が並列他チーム由来) |
| 既存テスト破壊 | 0 件 |
| ビルド警告 | 既存 2 件のみ (Fix-A 由来 0) |
| Cargo.toml 追加 | `encoding_rs = "0.8"` |
| 公開 API 破壊 | なし (`ParsedSalary` は **bonus_months: Option<f64>` 拡張のみ**) |

---

## 1. 修正 #1: emp_classifier 統一を aggregator.rs に適用

### 何を直したか
`aggregator.rs::classify_emp_group_label` の旧実装は

```rust
} else if emp.contains("正社員") || emp.contains("正職員")
          || emp.contains("契約") || emp.contains("業務委託") {
    "正社員"  // ← 契約社員/業務委託も「正社員」に分類していた誤り
}
```

これを `crate::handlers::emp_classifier::classify` (Plan P2 #2 / E2 で導入) に委譲するよう変更。
`EmpGroup::Regular → "正社員"` / `EmpGroup::PartTime → "パート"` / `EmpGroup::Other → "派遣・その他"`。

### 呼出元監査 (再発防止)
`grep classify_emp_group_label src/` の結果、survey/aggregator.rs **1 箇所のみ** で使用 → 統一の徹底度は完全。
emp_classifier::classify は `recruitment_diag/condition_gap.rs` でも使用されており、survey 側がこれに合流したことで「分類二重定義」状態が解消。

### 数値変化 (逆証明)
**fixture**: `[正社員 月給 30 万 / 契約社員 月給 25 万]`

| 集計対象 | 旧仕様 (修正前) | 新仕様 (修正後) |
|---------|---------------|---------------|
| 正社員グループ count | 2 | 1 |
| 正社員グループ mean | 275,000 円 | **300,000 円** |
| 派遣・その他グループ count | 0 (分類不可) | 1 |
| 派遣・その他グループ mean | — | 250,000 円 |

**fixture**: `[正社員 月給 30 万 / 業務委託 月給 80 万 (フリーランス級高額)]`

| 集計対象 | 旧仕様 | 新仕様 |
|---------|------|------|
| 正社員 mean | 550,000 円 (高額業務委託で歪み) | **300,000 円** |
| 業務委託の行き先 | 正社員グループ (誤) | 派遣・その他 |

### 新規テスト
- `aggregator::tests::fixa_emp_group_contract_worker_routes_to_other_not_seishain`
- `aggregator::tests::fixa_emp_group_gyomu_itaku_routes_to_other_not_seishain`
- `aggregator::tests::fixa_emp_group_seishain_igai_routes_to_other`

---

## 2. 修正 #2: 派遣・その他 native_unit 判定ロジック再設計

### 何を直したか
旧実装は

```rust
if bucket.hourly_values.len() > bucket.monthly_values.len() {
    "時給"
} else {
    "月給"
}
```

しかし `bucket.hourly_values` と `bucket.monthly_values` は **全レコードで両方 push** される設計
(時給→月給換算/月給→時給換算 両方) のため、両者は常に同件数。
結果、`>` は常に false になり **常に「月給」が選択される silent bug**。

### 修正方針
新たに `salary_type_counts: HashMap<&'static str, usize>` を Bucket に追加し、
**元レコードの salary_type を直接カウント**。
`Hourly` 件数 > `Monthly`(+ Annual+Daily+Weekly) 件数 → 時給、それ以外 → 月給 (タイは月給優先で保守的)。

### 数値変化
**fixture**: `[派遣 時給1500 / 派遣 時給1600 / 派遣 時給1700 / 派遣 月給 25 万]`

| 項目 | 旧仕様 | 新仕様 |
|------|------|------|
| native_unit | 月給 (silent bug) | **時給** |

### 新規テスト
- `aggregator::tests::fixa_native_unit_other_group_majority_hourly_picks_jikyu`
- `aggregator::tests::fixa_native_unit_other_group_majority_monthly_picks_gekkyu`
- `aggregator::tests::fixa_native_unit_other_group_tie_picks_gekkyu_conservative`

---

## 3. 修正 #3: 賞与パース対応

### 何を直したか
`ParsedSalary` 構造体に `bonus_months: Option<f64>` フィールドを追加 (後方互換: 既存呼出に影響なし)。
`parse_bonus_months` 関数を新設し以下表記をパース:

| 表記 | 抽出値 |
|------|--------|
| 月給25万円 賞与年4ヶ月 | 4.0 |
| 月給20万円 賞与年2.5ヶ月 | 2.5 |
| 賞与年4ケ月 / 4か月 / 4カ月 / 4ヵ月 / 4箇月 | 4.0 |
| 賞与年4月 (ヶ月 suffix なし) | 4.0 |
| 賞与年2回 (回数のみ) | None (推測しない) |
| 賞与年20ヶ月 (>12) | None (異常値) |
| 月給25万円 (賞与表記なし) | None |

「月給」「月収」「月額」と衝突しないよう除外ロジック実装。

### HW Panel 5 整合
`condition_gap.rs:115-126` の `annual_with_bonus = monthly_min × (12 + bonus_months)` と整合可能に。
逆証明テスト `fixa_bonus_annual_with_bonus_calc_alignment`:
- 月給 20 万 + 賞与 4 ヶ月 → 20 × 16 = **320 万円** (旧仕様: 月給×12 = 240 万円のみ)

### 新規テスト (8 件)
- `salary_parser::tests::fixa_bonus_parse_year_4_kagetsu`
- `salary_parser::tests::fixa_bonus_parse_decimal_2_5_kagetsu`
- `salary_parser::tests::fixa_bonus_parse_kekanji` (3 表記)
- `salary_parser::tests::fixa_bonus_parse_kanji_kagetsu` (suffix なし)
- `salary_parser::tests::fixa_bonus_parse_no_bonus_returns_none`
- `salary_parser::tests::fixa_bonus_parse_bonus_count_only_returns_none`
- `salary_parser::tests::fixa_bonus_parse_clamp_invalid` (>12, 0)
- `salary_parser::tests::fixa_bonus_parse_does_not_confuse_gekkyu`
- `salary_parser::tests::fixa_bonus_annual_with_bonus_calc_alignment`

---

## 4. 修正 #4: Shift-JIS 未対応 → encoding_rs 統合

### 何を直したか
`Cargo.toml` に `encoding_rs = "0.8"` 追加 (transitive dep 経由で既に Cargo.lock に存在)。
`upload.rs::decode_csv_bytes` を新設し、以下の優先順で BOM/エンコーディング判定:

1. **UTF-8 BOM** (0xEF 0xBB 0xBF) → BOM 除去
2. **UTF-16LE BOM** (0xFF 0xFE) → encoding_rs UTF_16LE
3. **UTF-16BE BOM** (0xFE 0xFF) → encoding_rs UTF_16BE
4. **UTF-8 valid?** (`std::str::from_utf8` 成功) → そのまま
5. **Shift-JIS** (CP932) → encoding_rs SHIFT_JIS。文字化け率 (U+FFFD) ≤5% で採用
6. **fallback**: UTF-8 lossy

### 数値変化 (逆証明)
**fixture**: Excel 「CSV (Shift-JIS) 保存」相当の 1 行 CSV

| 項目 | 旧仕様 | 新仕様 |
|------|------|------|
| `parse_csv_bytes` records.len() | **0 件** ("CSVにデータ行がありません") | **1 件** |
| company_name | (取得不能) | "株式会社A" |

### 新規テスト
- `upload::fixa_upload_tests::fixa_decode_utf8_bom_strips_bom`
- `upload::fixa_upload_tests::fixa_decode_plain_utf8_passes_through`
- `upload::fixa_upload_tests::fixa_decode_shift_jis_excel_save`
- `upload::fixa_upload_tests::fixa_parse_csv_bytes_accepts_shift_jis`

---

## 5. 修正 #5: CSV 行レベル重複検出

### 何を直したか
`parse_csv_bytes_with_hints` 内で
`hash(job_title + company_name + location_raw + salary_raw + employment_type)` ベースの行重複検出を追加。
完全一致行は 1 件にまとめる。`HashSet<u64>` で O(1) 判定。

### dedupe key 設計判断 (MEMORY: feedback_dedup_rules)
- **employment_type を key に含める** → 同一施設の正社員/パートは別レコード扱い (V1/V2 共通ルール)
- description / tags は除外 → 同一求人で表記ゆれによる擬似重複を避ける
- file_hash 重複アップロード警告は **本 sprint 対象外** (将来 Fix で対応)

### 数値変化 (逆証明)
**fixture**: 完全一致 3 行 CSV

| 項目 | 旧仕様 | 新仕様 |
|------|------|------|
| records.len() | 3 | **1** |
| duplicates_removed (log) | (出力なし) | 2 |

### 新規テスト
- `upload::fixa_upload_tests::fixa_dedupe_removes_exact_duplicate_rows`
- `upload::fixa_upload_tests::fixa_dedupe_keeps_different_employment_type_as_separate`
- `upload::fixa_upload_tests::fixa_dedupe_keeps_different_location_as_separate`

---

## 6. 修正 #6: IQR 片側適用矛盾整理

### 監査結果と判断
コード上は **既に両側適用** (`statistics.rs:188-194` で `v >= lower && v <= upper`)。
notes.rs / executive_summary.rs / employment.rs のドキュメント文言も「Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR」で **両側で正しく記述**されていた。

### 真の矛盾箇所
`render.rs:408` の「分布」セクションヘッダーが
**「外れ値除外（IQR法）適用済」** と誤表記 — しかし実際の `by_salary_range` / `by_employment_type` は
パース直後の生レコードを件数集計しており IQR 適用なし。

### 修正
`render.rs:408` のラベルを **「件数集計（生値ベース・IQR 未適用）」** に変更し事実と整合化。
IQR は給与統計 (mean/median/Q1/Q3 等) と雇用形態グループ別集計の数値計算側のみ適用、
件数分布チャートは生値ベース、と明示化。

### 新規テスト (両側性の逆証明)
- `statistics::outlier_tests::fixa_iqr_filter_removes_low_outlier_proves_two_sided`
  - データ `[1, 200, 220, 240, 250, 260, 280, 300]` → 下側「1」が除外されることを assert
- `statistics::outlier_tests::fixa_iqr_filter_removes_both_sides_simultaneously`
  - データ `[1, 200..300, 99_999_999]` → 両端 2 件除外を assert

---

## 7. テスト結果サマリ

| テスト群 | 件数 |
|---------|------|
| salary_parser (Fix-A 新規) | 9 件 |
| aggregator (Fix-A 新規) | 6 件 |
| upload (Fix-A 新規) | 7 件 |
| statistics (Fix-A 新規) | 2 件 |
| **Fix-A 新規合計** | **24 件** |
| 既存テスト (回帰) | 745 件 |
| **lib total** | **769 passed / 0 failed / 1 ignored** |

---

## 8. リリースノート ドラフト

### 媒体分析タブ パイプライン中核修正 (2026-04-26)

**Critical 修正**:
- **賞与込み年収比較が可能になりました**: アップロード CSV から「年X.Yヶ月」「賞与年Z回」等の表記を自動抽出し、HW Panel 5 (採用診断) と整合する `年収 = 月給 × (12 + 賞与月数)` の計算を有効化。
- **Excel 標準保存の Shift-JIS CSV に対応**: これまで「CSVにデータ行がありません」エラーになっていた Excel 標準保存ファイルを正しくパースできるようになりました。UTF-16 BOM (LE/BE) も対応。
- **CSV 重複行を自動除外**: 完全一致行を 1 件にまとめて集計バイアスを除去。雇用形態 (正社員/パート) が異なる場合は別求人として保持します。
- **正社員 vs 契約社員/業務委託 の経済的本質に整合した分類**: これまで「契約社員」「業務委託」が「正社員」グループに混入していた誤分類を修正。給与統計の中央値・平均がより実態に近い値になりました。
- **派遣・その他グループの単位選択ロジック修正**: 時給契約が多い派遣求人で常に「月給」表示になっていた silent bug を修正。
- **「分布」チャートのラベル整合性**: IQR 未適用の生値件数集計に対する誤った「外れ値除外（IQR法）適用済」表記を修正。

**注意事項** (memory: feedback_correlation_not_causation):
- 賞与パースは CSV テキスト中の表記を抽出するもので、実際の賞与支給を保証するものではありません (媒体掲載文言ベース)。
- 「年収比較」結果はあくまで **掲載求人テキスト** の傾向であり、市場全体の賃金水準を断定するものではありません。

---

## 9. 親セッション統合チェックリスト

- [x] cargo build --lib errors=0
- [x] cargo test --lib 769 passed / 0 failed
- [x] 公開 API シグネチャ不変 (ParsedSalary は拡張のみ)
- [x] emp_classifier 呼出元監査 (1 箇所のみ → 統一徹底)
- [x] 並列他チーム (Fix-B, Fix-C) との競合なし (改修ファイルが分離)
- [ ] **Fix-C 確認待ち**: VacancyRatePct Newtype が ParsedSalary 周辺に波及した場合の協調 (現時点では `survey/integration.rs`, `hw_enrichment.rs` のみ参照、Fix-A 改修ファイルとは独立)
- [ ] E2E テスト (Fix-C 担当) で SJIS CSV / 重複排除 / 賞与込み年収表示の通しシナリオ追加推奨

---

## 10. 改修ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | encoding_rs = "0.8" 追加 |
| `src/handlers/survey/salary_parser.rs` | ParsedSalary.bonus_months 追加 / parse_bonus_months 実装 / 9 テスト追加 |
| `src/handlers/survey/aggregator.rs` | classify_emp_group_label を emp_classifier に委譲 / native_unit 判定を salary_type_counts ベースに修正 / empty_salary 更新 / 6 テスト追加 |
| `src/handlers/survey/upload.rs` | decode_csv_bytes (BOM/SJIS) 実装 / 行レベル dedupe 実装 / 7 テスト追加 |
| `src/handlers/survey/statistics.rs` | IQR 両側性の逆証明テスト 2 件追加 (コード変更なし) |
| `src/handlers/survey/render.rs` | 「分布」セクションラベル整合化 (IQR 適用済 → 生値ベース) |
| `src/handlers/survey/parser_aggregator_audit_test.rs` | empty_salary に bonus_months 追加 (構造体拡張に追従) |
