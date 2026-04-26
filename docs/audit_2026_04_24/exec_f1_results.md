# P2 ドメインロジック残課題実装結果 (F1 チーム実装)

**実装日**: 2026-04-26
**担当**: F1 (Domain Logic 仕上げチーム)
**前提**: E2 (Domain Logic 強化チーム) が 670 passed (lib全体 667) 達成済
**対象**: E2 で延期された 3 件 + SW-F04/F10 最終判断

---

## 0. エグゼクティブサマリ

| 項目 | 値 |
|------|-----|
| 修正前テスト数 (lib) | 667 件 (E2 完了時) |
| 修正後テスト数 (lib) | **687 件 全 pass** (+20、failed 0) |
| 新規逆証明テスト | 17 件 (HS-4: 4, 月給換算: 6, Panel 1: 7) |
| 既存テスト破壊 | 0 件 |
| 既存テスト更新 | 2 件 (alpha_salary_min_values_* で 160h→167h 値更新) + contract_test +4 keys |
| ビルド | ✅ pass (warnings 4 件、F1 関連の新規 warning なし) |
| 実装課題 | 4/4 (HS-4 / 月給換算 / Panel 1 / SW-F04・F10) |

---

## 1. F1 #1: HS-4 TEMP_LOW_THRESHOLD 相対閾値化

### 1-1. 修正内容
- **ファイル**: `src/handlers/insight/helpers.rs`
- **修正前**: `TEMP_LOW_THRESHOLD: f64 = 0.0`
- **修正後**: `TEMP_LOW_THRESHOLD: f64 = -0.15`

### 1-2. 根拠 (実データ確認)
hellowork.db を直接照会 (2026-04-26) し v2_text_temperature の分布を確認。

| レベル | n | min | P05 | P10 | P25 | P50 | P75 | max | 負値割合 |
|--------|---|-----|-----|-----|-----|-----|-----|-----|---------|
| 都道府県 (正社員) | 47 | -0.4063 | -0.1520 | -0.1410 | **-0.0377** | 0.1331 | 0.2481 | 0.6063 | 27.7% |
| 市区町村 (正社員) | 1004 | -2.7286 | -0.6016 | -0.3950 | **-0.1417** | 0.1020 | 0.4515 | 3.3639 | 37.8% |

旧閾値 0.0 は中立点であり「負値全部を低温」と判定 → 過剰検出傾向。
新閾値 -0.15 は市区町村レベル P25 (-0.1417) を保守側に丸めた値。
**真に下位四分位の 25% のみ発火** に変更。

### 1-3. 単位の明示
コメントに以下を追加:
- `temperature = (urgency_words - selectivity_words) / total_chars * 1000` パーミル
- 出典: `scripts/compute_v2_phase2.py:104`

### 1-4. 逆証明テスト (4 件、全 pass)
| テスト名 | 検証内容 |
|----------|----------|
| `f1_hs4_threshold_negative_015_no_fire_for_temp_minus_010` | temp=-0.10 で発火しない (旧閾値0.0なら発火、新閾値-0.15なら不発火) |
| `f1_hs4_threshold_negative_015_fires_for_temp_minus_020` | temp=-0.20 で Warning 発火 |
| `f1_hs4_threshold_boundary_at_minus_015` | 境界値 temp=-0.15 で不発火 (>= 比較) |
| `f1_hs4_threshold_constant_value_is_negative_015` | 定数値そのものを assert |

### 1-5. 既存テスト整合性
| テスト | 結果 | 備考 |
|--------|------|------|
| `hs4_no_fire_when_vacancy_low` (temp=-0.5) | ✅ pass | vac<0.30 で発火しないこと |
| `hs4_no_fire_when_temp_zero` (temp=0.0) | ✅ pass | 0.0 >= -0.15 で発火しない |
| `hs4_warning_high_vacancy_negative_temp` (temp=-0.2) | ✅ pass | -0.2 < -0.15 で Warning |
| `anomaly_hs4_null_temperature_is_treated_as_zero` | ✅ pass | null=0.0 ≥ -0.15 で発火しない |
| `p2_all_patterns_pass_phrase_validator` | ✅ pass | temp=-0.2 で発火継続 |

---

## 2. F1 #2: 月給換算定数 160h → 167h

### 2-1. 修正内容
- **ファイル**: `src/handlers/survey/aggregator.rs`, `src/handlers/survey/report_html.rs`

新規定数 (aggregator.rs):
```rust
pub(crate) const HOURLY_TO_MONTHLY_HOURS: i64 = 167;        // 旧: 160 (8h × 20日)
pub(crate) const DAILY_TO_MONTHLY_DAYS: i64 = 21;           // 旧: 20 (8h × 20日想定)
pub(crate) const DAILY_HOURS: i64 = 8;
pub(crate) const WEEKLY_TO_MONTHLY_NUM: i64 = 433;          // 旧: × 4
pub(crate) const WEEKLY_TO_MONTHLY_DEN: i64 = 100;
pub(crate) const WEEKLY_HOURS: i64 = 40;
```

### 2-2. 修正前/修正後の具体値
| 項目 | 修正前 | 修正後 | 差 |
|------|--------|--------|-----|
| 時給 1,500円 → 月給 | 240,000 円 | 250,500 円 | +10,500 円 (+4.4%) |
| 月給 200,000円 → 時給 | 1,250 円/h | 1,197 円/h | -53 円 (-4.2%) |
| 日給 12,000円 → 月給 | 240,000 円 | 252,000 円 | +12,000 円 (+5.0%) |
| 週給 50,000円 → 月給 | 200,000 円 | 216,500 円 | +16,500 円 (+8.25%) |

### 2-3. 根拠
タスク仕様: 厚労省「就業条件総合調査 2024」の月平均所定労働時間 167h
(= 8h × 20.875日 ≈ 8h × 21日近似) を採用。
- Daily: 21日 (= 20.875 切り上げ)
- Weekly: 4.33 = 52週 / 12月 (年間平均週数)

### 2-4. salary_parser との不整合 (既知の問題、F1 範囲外)
`src/handlers/survey/salary_parser.rs:33` には既に
`HOURLY_TO_MONTHLY: f64 = 173.8` (8h × 21.7日、GAS 互換) が存在。

| 経路 | 換算係数 | 月給 200,000円 の時給 |
|------|---------|---------------------|
| salary_parser (parse_salary 経由) | 173.8h | 1,150 円/h |
| aggregator (direct conversion) | 167h | 1,197 円/h |

不整合は **F1 #2 では aggregator 側のみ修正** (タスク指示通り)。
salary_parser の統一は P3 release notes と合わせて別タスクで実施推奨。

### 2-5. 逆証明テスト (6 件、全 pass)
| テスト名 | 検証内容 |
|----------|----------|
| `f1_monthly_to_hourly_conversion_200k_specific_value` | 200_000 / 167 = 1_197 |
| `f1_hourly_to_monthly_conversion_1500yen_specific_value` | 1_500 * 167 = 250_500 |
| `f1_daily_to_monthly_conversion_specific_value` | 12_000 * 21 = 252_000 |
| `f1_weekly_to_monthly_conversion_specific_value` | 50_000 * 433/100 = 216_500 (salary_parser と一致) |
| `f1_aggregate_by_emp_group_native_hourly_uses_167` | パートグループの sample 取得確認 |
| `f1_constant_inconsistency_between_parser_and_aggregator` | 不整合 (47円差) を明示記録 |

### 2-6. 既存テスト更新 (2 件)
| テスト名 | 旧期待値 | 新期待値 |
|----------|---------|---------|
| `alpha_salary_min_values_type_conversion_exact` | mins=[240k,240k,300k,500k] / maxs=[300k,320k,400k,700k] | mins=[250.5k,252k,300k,500k] / maxs=[315k,334k,400k,700k] |
| `alpha_salary_min_values_filter_below_50k` | Hourly 300円 → 48,000 < 50,000 除外 | Hourly 200円 → 33,400 除外、400円 → 66,800 含む (300円*167=50,100 ≥ 50,000 で含まれてしまうためテストケース変更) |

### 2-7. report_html.rs 表示文言更新
- 「最低賃金比較（160h換算）」→「最低賃金比較（167h換算）」
- 「月給を160h（8h×20日）で割り...」→「月給を167h（8h×20.875日、厚労省基準）で割り...」
- 「160h=所定労働時間（8h×20日）で換算」→「167h=所定労働時間（8h×20.875日、厚労省「就業条件総合調査 2024」基準）で換算」
- 文言「（時給は ×160 で月給換算）...（月給は /160 で時給換算）」→「×167 / /167」

---

## 3. F1 #3: Panel 1 採用難度 観光地補正

### 3-1. 修正内容
- **ファイル**: `src/handlers/recruitment_diag/handlers.rs`

新規定数:
```rust
pub(crate) const TOURIST_AREA_DAYNIGHT_RATIO: f64 = 1.5;
```

新規純粋関数:
```rust
pub(crate) fn compute_difficulty_score_with_tourist_correction(
    hw_count: i64,
    day_population: f64,
    night_population: f64,
) -> (f64, f64, f64, bool)  // (score, population_used, day_night_ratio, is_tourist_area)
```

### 3-2. 補正ロジック
1. day_population (平日昼滞在) と night_population (平日深夜滞在 ≒ 居住人口代理) を両方取得
2. day_night_ratio = day / night
3. is_tourist_area = (night > 0) AND (ratio > 1.5)
4. 観光地のとき: 分母を **night** に切り替え (タスク仕様 max(day, night) は観光地で day 優位のため意図と矛盾。**居住人口側 night を採用** が正しい解釈)
5. score = hw_count / population * 10,000

### 3-3. 銀座 (citycode=13102) / 京都四条河原町 (citycode=26100) 想定の試算
| シナリオ | hw | day | night | ratio | 修正前 score | 修正後 score | rank 変化 |
|---------|----|----|------|-------|------------|------------|----------|
| 銀座的 (昼夜比 3.0) | 20 | 30,000 | 10,000 | 3.0 | 6.67 (rank 3 平均的) | 20.00 (rank 5 超激戦) | 平均→超激戦 |
| 京都四条河原町的 (2.0) | 10 | 20,000 | 10,000 | 2.0 | 5.00 (rank 3 平均的) | 10.00 (rank 4 激戦) | 平均→激戦 |
| 通常エリア (1.2) | 5 | 12,000 | 10,000 | 1.2 | 4.17 (rank 3) | 4.17 (rank 3) | 補正なし |
| 境界 (1.5) | 5 | 15,000 | 10,000 | 1.5 | 3.33 (rank 3) | 3.33 (rank 3) | 補正なし (>1.5 で発動) |

### 3-4. UI 上の注記
metrics に追加:
- `day_population`, `night_population`, `day_night_ratio`, `is_tourist_area`

JSON 返り値に追加:
- `tourist_correction_note`: 観光地時のみ「※観光地・繁華街判定（昼夜比 X.XX > 1.5）。昼間滞在膨張による『穴場』誤判定を避けるため、居住人口側で再算出した値です。」
- `notes.calculation`: 観光地時は「[F1 #3: 観光地補正適用、平日昼滞在は外来流入で膨張するため不採用]」を追記
- `notes.tourist_threshold`: 1.5

### 3-5. 逆証明テスト (7 件、全 pass)
| テスト名 | 検証内容 |
|----------|----------|
| `f1_panel1_tourist_threshold_constant_is_1_5` | 定数値の確認 |
| `f1_panel1_tourist_correction_ginza_like_increases_score` | 銀座型 (3.0) で score 6.67→20.00、rank 3→5 |
| `f1_panel1_tourist_correction_kyoto_like_changes_rank` | 京都型 (2.0) で score 5.00→10.00、rank 3→4 |
| `f1_panel1_no_tourist_correction_for_normal_area` | 通常 (1.2) で補正なし |
| `f1_panel1_tourist_correction_boundary_at_1_5` | 境界 1.5 ちょうどで不発動 (> 比較) |
| `f1_panel1_no_tourist_correction_when_night_zero` | night=0 で補正不能 |
| `f1_panel1_correction_score_delta_matches_population_swap` | score 変化率 = day/night の比 |

### 3-6. 既存テスト更新
| テスト | 修正内容 |
|--------|----------|
| `panel1_difficulty_shape_contains_required_keys` | day_population, night_population, day_night_ratio, is_tourist_area の 4 keys 検証追加 |

### 3-7. メモリルール遵守
- `feedback_correlation_not_causation.md`: 「傾向」「うかがえ」表現使用
- `feedback_reverse_proof_tests.md`: 修正前/修正後の具体値を assert
- `feedback_test_data_validation.md`: classify_difficulty 連携で rank 変化まで検証

---

## 4. F1 #4: SW-F04 / SW-F10 最終判断

### 4-1. 判断
**選択肢 B (現状維持) を確定**。`engine_flow.rs:131-148, 329-336` のコメントを格上げ。

### 4-2. コメント変更内容
旧:
```rust
// v2_posting_mesh1km 投入後にメッシュ単位の Z-score に拡張予定
None
```

新:
```rust
// **F1 #4 (2026-04-26) 最終判断**: 選択肢 B 採用（プレースホルダ維持、Phase C で本実装）。
// **Phase C 仕様未確定**: SSDSE-A 業種マッピング (e-Stat 産業分類 14 業種) と
// v2_posting_mesh1km (Agoop メッシュ単位求人密度) の両方が必要。
None
```

SW-F10 も同様 (3 データソース統合後に拡張)。

### 4-3. 既存テスト
- `swf04_always_none_placeholder` ✅ pass
- `swf10_always_none_phase_c_pending` ✅ pass

---

## 5. テスト実行結果

### 5-1. ビルド
```
cargo build --lib
warning: function `fetch_industry_structure` is never used (P3 領域、F1 無関係)
warning: function `render_survey_report_page` is never used
warning: function `render_comparison_card` is never used
warning: function `render_section_hw_comparison` is never used
warning: `rust_dashboard` (lib) generated 4 warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
```
F1 関連の新規 warning なし。

### 5-2. テスト実行
```
cargo test --lib
test result: ok. 687 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.18s
```

差分: 667 (E2 完了時) → **687 (F1 完了時、+20、failed 0)**

### 5-3. F1 新規テスト一覧 (17 件、全 pass)
```
handlers::insight::pattern_audit_test::f1_hs4_threshold_constant_value_is_negative_015 ... ok
handlers::insight::pattern_audit_test::f1_hs4_threshold_boundary_at_minus_015 ... ok
handlers::insight::pattern_audit_test::f1_hs4_threshold_negative_015_fires_for_temp_minus_020 ... ok
handlers::insight::pattern_audit_test::f1_hs4_threshold_negative_015_no_fire_for_temp_minus_010 ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_no_tourist_correction_for_normal_area ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_tourist_correction_boundary_at_1_5 ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_tourist_correction_ginza_like_increases_score ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_tourist_correction_kyoto_like_changes_rank ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_tourist_threshold_constant_is_1_5 ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_correction_score_delta_matches_population_swap ... ok
handlers::recruitment_diag::handlers::tests::f1_panel1_no_tourist_correction_when_night_zero ... ok
handlers::survey::parser_aggregator_audit_test::f1_constant_inconsistency_between_parser_and_aggregator ... ok
handlers::survey::parser_aggregator_audit_test::f1_aggregate_by_emp_group_native_hourly_uses_167 ... ok
handlers::survey::parser_aggregator_audit_test::f1_daily_to_monthly_conversion_specific_value ... ok
handlers::survey::parser_aggregator_audit_test::f1_hourly_to_monthly_conversion_1500yen_specific_value ... ok
handlers::survey::parser_aggregator_audit_test::f1_monthly_to_hourly_conversion_200k_specific_value ... ok
handlers::survey::parser_aggregator_audit_test::f1_weekly_to_monthly_conversion_specific_value ... ok
```

---

## 6. リリースノート draft (給与換算変更影響)

### 6-1. 変更概要
求人レポート (Survey タブ) の給与表示で使用していた月給換算定数を、
**月160h (= 8h × 20日) → 月167h (= 8h × 20.875日、厚労省「就業条件総合調査 2024」基準)** に変更しました。

### 6-2. 影響を受ける表示
| 項目 | 旧値 | 新値 | 変動率 |
|------|-----|------|-------|
| 時給→月給換算（パート系の月給相当値） | 時給 × 160 | 時給 × 167 | **+4.4%** |
| 月給→時給換算（最低賃金比較カード等） | 月給 / 160 | 月給 / 167 | **-4.2%** |
| 日給→月給換算 | 日給 × 20 | 日給 × 21 | +5.0% |
| 週給→月給換算 | 週給 × 4 | 週給 × 4.33 | +8.25% |

### 6-3. ユーザーへの影響
- パート求人 (時給ベース) の月給相当表示が **約 4.4% 上昇** する傾向。
- 最低賃金比較カードの「160h換算」表示が「167h換算」に切り替わり、時給が **約 53 円 (200,000円月給の場合) 低下** する傾向 → 最低賃金との差は数値的に縮小して見える可能性。

### 6-4. 既知の限界
1. **salary_parser (求人テキストの自然言語解析) は別系統**: 173.8h (= 8h × 21.7日、GAS 互換) を維持。両者で月給 200,000円の時給値が 1,150 vs 1,197 円と 47 円差が出る。完全統一は次フェーズ (P3) で release notes と合わせて実施予定。
2. **時給/日給/週給の境界整数丸め**: Daily は 20.875 → 21 (切り上げ)、Weekly は 4.33 (= 52/12) を 433/100 で整数化。

---

## 7. 修正/新規ファイル一覧 (絶対パス)

### 修正対象 (実装関連)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\helpers.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\engine_flow.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\aggregator.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\handlers.rs`

### 修正対象 (テスト関連)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\pattern_audit_test.rs` (HS-4 4件追加)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\parser_aggregator_audit_test.rs` (月給換算 6件追加 + 既存 2件更新)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\recruitment_diag\contract_tests.rs` (Panel 1 keys 4件追加)

### 新規作成
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_f1_results.md` (本ファイル)

---

## 8. 親セッションへの統合チェックリスト

- [x] `cargo build --lib` errors 0、F1 関連の新規 warning なし
- [x] `cargo test --lib` 全 687 件 pass (E2 後 667 → +20、failed 0)
- [x] HS-4 TEMP_LOW_THRESHOLD: 0.0 → -0.15 (実データ P25 基準)
- [x] 月給換算: 160h → 167h、20日 → 21日、4週 → 4.33週
- [x] Panel 1 観光地補正: day_night_ratio > 1.5 で night を分母に
- [x] SW-F04 / SW-F10: コメント格上げ (Phase C 仕様未確定明記)
- [x] 全修正に修正前/修正後の具体値を assert 形式で記録 (17 件)
- [x] memory ルール遵守:
  - [x] `feedback_reverse_proof_tests.md`: 17 件の逆証明テスト
  - [x] `feedback_correlation_not_causation.md`: 「傾向」「うかがえ」表現
  - [x] `feedback_test_data_validation.md`: classify_difficulty 経由で rank 変化検証
  - [x] `feedback_never_guess_data.md`: hellowork.db 直接照会で実分布確認
- [x] リリースノート draft 記載 (給与換算 4.4% 変動)
- [ ] **未対応 (タスク範囲外)**:
  - salary_parser (173.8h) と aggregator (167h) の整合性統一 → P3 release notes と合わせて
  - HS-4 の動的 (県別 P25) 閾値化 → 現実装は固定 -0.15、より厳密な相対閾値は P4

---

## 9. 重要な保守メモ

1. **HS-4 閾値の定期再評価**: temperature 分布は ETL 投入データに依存。新規データ投入時は分布を再確認し閾値を調整する余地あり。`scripts/compute_v2_phase2.py` 実行後に再計測推奨。

2. **月給換算の 167h と 173.8h 不整合**: `f1_constant_inconsistency_between_parser_and_aggregator` テストが両定数の差を明示記録。次フェーズで以下のいずれかに統一:
   - 案A: aggregator も 173.8 に合わせる (GAS 互換維持、表示値が +3.5% 上昇)
   - 案B: salary_parser を 167 に合わせる (厚労省整合、CSV取込互換に注意)

3. **Panel 1 観光地補正の解釈**: タスク仕様の `max(day, night)` は観光地 (day優位) で意味なし → **night 採用** で実装。タスク後半「居住人口」記載と整合。報告書に経緯記載。

4. **night_population が居住人口の代理**: 厳密な居住人口は e-Stat 国勢調査要参照だが、Agoop 平日深夜滞在 (timezone=1) で代理可能と判断。一致度は別途検証推奨。
