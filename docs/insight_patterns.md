# insight 38 patterns カタログ

**最終更新**: 2026-04-26
**対象範囲**: `src/handlers/insight/` の全 38 patterns (発火可能 36 + 未実装プレースホルダ 2)
**根拠**: `docs/audit_2026_04_24/team_gamma_domain.md`、`src/handlers/insight/{engine.rs, engine_flow.rs, helpers.rs, phrase_validator.rs}`、`pattern_audit_test.rs` (1,767 行)
**マスター**: ルート [`CLAUDE.md`](../CLAUDE.md) §3.1 〜 §7、`src/handlers/CLAUDE.md` §1.4

---

## 0. サマリ統計

| カテゴリ | patterns 数 | phrase_validator 適用済 |
|---------|-----------|----------------------|
| HS (採用構造) | 6 | ❌ 0/6 |
| FC (将来予測) | 4 | ❌ 0/4 |
| RC (地域比較) | 3 | ❌ 0/3 |
| AP (アクション提案) | 3 | ❌ 0/3 |
| CZ (通勤圏 距離) | 3 | ❌ 0/3 |
| CF (通勤フロー) | 3 | ❌ 0/3 |
| LS / HH / MF / IN / GE (構造分析) | 6 | ✅ 6/6 |
| SW-F (Agoop 人流) | 10 | ✅ 8/8 (実装分のみ) |

- **発火可能**: 36 patterns (実装済)
- **未実装プレースホルダ**: 2 patterns (SW-F04, SW-F10)
- **phrase_validator 適用済**: 14 patterns (LS/HH/MF/IN/GE 6 + SW-F 8)
- **未適用**: 22 patterns (HS/FC/RC/AP/CZ/CF) → P2 改善対象
- **重大バグ疑い**: 3 件 (MF-1 単位、IN-1 反転、SW-F02 vs SW-F05 同時発火)

---

## 1. HS (採用構造分析) 6 patterns

| ID | 名称 | カテゴリ | severity | 閾値 (`helpers.rs` 定数) | 発火条件 | data source | phrase_validator | ファイル:行 | pattern_audit_test ID |
|----|------|---------|---------|------------------------|---------|------------|:----------------:|------------|----------------------|
| HS-1 | 慢性的人材不足 | HiringStructure | Critical/Warning | `VACANCY_CRITICAL=0.30` `_WARNING=0.20` `_TREND=0.25` | vacancy_rate ≥ 0.20 | v2_vacancy_rate, ts_turso_vacancy | ❌ 未適用 (P2) | `engine.rs:73-144` | `test_hs1_*` |
| HS-2 | 給与競争力不足 | HiringStructure | Critical/Warning | `SALARY_COMP_CRITICAL=0.80` `_WARNING=0.90` | local_mean / national_mean ≤ 0.90 | v2_salary_competitiveness | ❌ 未適用 | `engine.rs:147-206` | `test_hs2_*` |
| HS-3 | 情報開示不足 | HiringStructure | Critical/Warning | `TRANSPARENCY_CRITICAL=0.40` `_WARNING=0.50` | transparency_score ≤ 0.50 | v2_transparency_score | ❌ 未適用 | `engine.rs:209-271` | `test_hs3_*` |
| HS-4 | 温度と採用難の乖離 | HiringStructure | Warning | `TEMP_LOW_THRESHOLD=0.0` (⚠ 根拠不明) | vacancy_rate ≥ Critical かつ temperature < 0 | v2_text_temperature + v2_vacancy_rate | ❌ 未適用 | `engine.rs:274-321` | `test_hs4_*` |
| HS-5 | 雇用者集中 | HiringStructure | Warning | `HHI_CRITICAL=0.25` `TOP1_SHARE_CRITICAL=0.30` | HHI > 0.25 OR top1 > 0.30 | v2_monopsony_index | ❌ 未適用 | `engine.rs:324-369` | `test_hs5_*` |
| HS-6 | 空間ミスマッチ | HiringStructure | Warning | `ISOLATION_WARNING=0.50` `DAYTIME_POP_RATIO_LOW=0.90` | isolation_score > 0.50 | v2_spatial_mismatch | ❌ 未適用 | `engine.rs:372-422` | `test_hs6_*` |

⚠ **HS-1 vacancy_rate の意味**: 「recruitment_reason_code=1 (欠員補充) を理由とする求人の割合」。労働経済学の欠員率 (未充足求人/常用労働者数) **ではない**。UI ラベル統一は P0。

---

## 2. FC (将来予測) 4 patterns

| ID | 名称 | severity | 閾値 | 発火条件 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|---------|------------|:----------------:|------------|
| FC-1 | 求人量トレンド | Warning/Positive | `TREND_INCREASE=0.05` `_DECREASE=-0.05` | 線形外挿 forecast_6m = latest × (1 + slope×6) | ts_turso_counts | ❌ 未適用 | `engine.rs:444-486` |
| FC-2 | 給与上昇圧力 | Warning | (slope 比較) | salary_slope < wage_slope (賃金 < 最低賃金) | v2_external_minimum_wage_history + ts_turso_salary | ❌ 未適用 | `engine.rs:489-540` |
| FC-3 | 人口動態 | Critical/Warning | 0.30 + net_migration<0 で Critical, 0.25 で Warning | 55歳以上 / 生産年齢 ≥ 0.25 | v2_external_population_pyramid + v2_external_migration | ❌ 未適用 | `engine.rs:543-637` |
| FC-4 | 充足困難度悪化 | Warning | days_slope > 0.03 かつ churn_slope > 0.02 | 月次 3% / 2% 同時悪化 | ts_turso_fulfillment | ❌ 未適用 | `engine.rs:640-700` |

---

## 3. RC (地域比較) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| RC-1 | ベンチマーク順位 | Warning/Positive | composite < 30 / > 70 | v2_region_benchmark | ❌ 未適用 | `engine.rs:719-763` |
| RC-2 | 給与・休日地域差 | (各severity) | ±10000円 / -20000円 (固定、職種無視) | v2_salary_structure + holidays | ❌ 未適用 | `engine.rs:766-829` |
| RC-3 | 人口×求人密度 | Warning/Positive | density > 50/千人 (Warning) / < 5/千人 (Positive)、GE-1 と cross-ref | postings + v2_external_population | ❌ 未適用 ✅ caveat あり | `engine.rs:832-898` |

---

## 4. AP (アクション提案) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| AP-1 | 給与改善 | Info | (HS-2 trigger 後) | v2_salary_competitiveness + 全国中央値 | ❌ 未適用 (「到達できます」断定) | `engine.rs:928-971` |
| AP-2 | 求人原稿改善 | Info | 開示率 < 0.30 | v2_transparency_score | ❌ 未適用 | `engine.rs:974-1017` |
| AP-3 | 採用エリア拡大 | Info | daytime_ratio < 1.0 | v2_external_daytime_population | ❌ 未適用 (「可能性」あり) | `engine.rs:1020-1047` |

---

## 5. CZ (通勤圏 距離) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| CZ-1 | 通勤圏人口ポテンシャル | Positive | local_share < 0.05 | v2_external_population (30km 圏) | ❌ 未適用 | `engine.rs:1084-1128` |
| CZ-2 | 通勤圏給与格差 | Warning | ±5%/-10% | v2_salary_structure | ❌ 未適用 | `engine.rs:1131-1180` |
| CZ-3 | 通勤圏高齢化 | Info/Warning | 0.20/0.30 | v2_external_population_pyramid | ❌ 未適用 | `engine.rs:1183-1219` |

---

## 6. CF (通勤フロー) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| CF-1 | 実通勤フロー | Warning | actual_ratio < 0.01 | v2_external_commute_od | ❌ 未適用 | `engine.rs:1224-1277` |
| CF-2 | 流入元ターゲティング | Info | (流入top抽出) | v2_external_commute_od | ❌ 未適用 | `engine.rs:1280-1306` |
| CF-3 | 地元就業率 | Positive/Warning | 0.7/0.3 | v2_external_commute_od | ❌ 未適用 | `engine.rs:1309-1355` |

---

## 7. 構造分析 6 patterns (LS / HH / MF / IN / GE)

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| LS-1 | 採用余力シグナル | Warning/Critical | unemployment > 県平均 × 1.2/1.5 | v2_external_labor_force + pref avg | ✅ 適用 | `engine.rs:1399-1445` (⚠「未マッチ層」用語問題) |
| LS-2 | 産業偏在 | Warning | 第3次 ≥ 85% OR 第1次 ≥ 20% | v2_external_industry_structure | ✅ 適用 | `engine.rs:1451-1501` |
| HH-1 | 単独世帯 | Info | 単独世帯率 ≥ 40% (全国 38%) | v2_external_household | ✅ 適用 | `engine.rs:1506-1543` |
| MF-1 | 医療福祉供給密度 | Warning/Critical | local/national < 0.8/0.6 | v2_external_medical_welfare + v2_external_population | ✅ 適用 | ⚠ `engine.rs:1565` 単位 10× バグ疑い (P0) |
| IN-1 | 産業構造ミスマッチ | Warning | `!(0.05..=0.3).contains(&mw_share)` | v2_external_establishments (industry='850') | ✅ 適用 | ⚠ `engine.rs:1637` 発火条件反転疑い |
| GE-1 | 可住地密度 | Warning/Critical | 50-10000 / CRITICAL 20-20000 (人/km²) | v2_external_geography | ✅ 適用 ✅ RC-3 cross-ref | `engine.rs:1666-1740` |

---

## 8. Agoop 人流 SW-F01〜F10

| ID | 名称 | severity | 閾値 (`helpers.rs:185-220`) | 発火条件 | data source | phrase_validator | ファイル:行 |
|----|------|---------|---------------------------|---------|------------|:----------------:|------------|
| SW-F01 | 夜勤需要 | Warning/Critical | `MIDNIGHT_RATIO_WARNING=1.2` `_CRITICAL=1.5` | midnight/daytime ≥ 1.2 | v2_flow_mesh1km_* | ✅ 適用 | `engine_flow.rs:43-70` |
| SW-F02 | 休日商圏不足 | Warning | `HOLIDAY_CROWD_WARNING=1.3` | holiday/weekday ≥ 1.3 | 同上 | ✅ 適用 | `engine_flow.rs:73-95` (⚠ SW-F05 と同時発火) |
| SW-F03 | ベッドタウン | Info | `BEDTOWN_DIFF=0.2` (1-daynight) | daynight < 0.8 かつ outflow ≥ 0.2 | 同上 | ✅ 適用 | `engine_flow.rs:98-125` |
| SW-F04 | メッシュ人材ギャップ | (未実装) | `MESH_ZSCORE=1.5` | None 返却プレースホルダ (v2_posting_mesh1km 投入後拡張) | (将来) | (該当なし) | `engine_flow.rs:128-141` |
| SW-F05 | 観光ポテンシャル | Info | `TOURISM_RATIO=1.5` | holiday/weekday ≥ 1.5 | 同上 | ✅ 適用 | `engine_flow.rs:144-166` (⚠ SW-F02 矛盾) |
| SW-F06 | コロナ回復乖離 | Info | `COVID_FLOW_RECOVERY=0.9` `POSTING_LAG=0.8` | 仕様 AND だが実装は人流のみ | v2_flow_mesh1km_2019/2021 | ✅ 適用 | `engine_flow.rs:169-192` (⚠ 仕様乖離) |
| SW-F07 | 広域流入比率 | Info | `INFLOW_DIFF_REGION=0.15` | diff_region_inflow ≥ 15% | v2_flow_fromto_city | ✅ 適用 | `engine_flow.rs:195-217` |
| SW-F08 | 昼間労働力プール | Info | `DAYTIME_POOL=1.3` | daynight ≥ 1.3 | v2_flow_mesh1km_* | ✅ 適用 | `engine_flow.rs:220-243` (⚠ SW-F03 と中間沈黙) |
| SW-F09 | 季節雇用ミスマッチ | Info | `SEASONAL_AMPLITUDE=0.3` | 月次振幅 ≥ 0.3 | v2_flow_mesh1km_* (12 ヶ月) | ✅ 適用 | `engine_flow.rs:246-269` |
| SW-F10 | 企業立地マッチ | (未実装) | `COMPANY_TIME_DIFF=3h` | None 返却 (v2_posting_mesh1km 依存) | (将来) | (該当なし) | `engine_flow.rs:272-278` |

---

## 9. 既知バグ・要追加検証

| 種別 | ID | 内容 | ファイル:行 | 監査参照 |
|------|----|------|-----------|---------|
| 重大バグ疑い | MF-1 | 医師密度 単位 10× ズレ疑 | `engine.rs:1565` | P0 #3、`team_gamma_domain.md §M-1` |
| 重大バグ疑い | IN-1 | `!(0.05..=0.3).contains(&mw_share)` 発火条件反転疑い | `engine.rs:1637` | `team_gamma_domain.md §IN-1` |
| 重大バグ疑い | SW-F02 vs SW-F05 | holiday/weekday 閾値 1.3 と 1.5 の同時発火 | `engine_flow.rs:73-166` | `team_gamma_domain.md §SW-F` |
| 仕様乖離 | SW-F06 | 仕様は AND だが実装は人流のみ | `engine_flow.rs:169-192` | 同上 |
| 用語問題 | LS-1 | 「未マッチ層」表記 | `engine.rs:1399-1445` | `team_alpha_userfacing.md §L1` |
| 概念混乱 | HS-1 (vacancy_rate) | 欠員補充率 vs 労働経済学的欠員率 | `engine.rs:127` | P0 #4 |
| 未実装 | SW-F04 | v2_posting_mesh1km 投入後実装 | `engine_flow.rs:128-141` | (将来) |
| 未実装 | SW-F10 | 同上 | `engine_flow.rs:272-278` | 同上 |

---

## 10. 共通 caveat

- 全 patterns で「相関 ≠ 因果」原則 (`feedback_correlation_not_causation`)。LS/HH/MF/IN/GE/SW-F は走時 phrase_validator で機械的検証
- 全 insight body に「傾向」「可能性」を含めることを推奨 (今後の HS/FC/RC/AP/CZ/CF にも展開)
- HW 限定性: insight タブヘッダ + 各レポートで明示 (`insight/render.rs:99` "HW（ハローワーク）掲載求人に基づく分析です")

---

## 11. pattern_audit_test.rs ID 対応

`src/handlers/insight/pattern_audit_test.rs` (1,767 行) で 22 patterns × 各 body を具体値検証:
- `test_hs1_*` 〜 `test_hs6_*`: HS 系
- `test_fc1_*` 〜 `test_fc4_*`: FC 系
- `test_rc1_*` 〜 `test_rc3_*`: RC 系
- `test_ap1_*` 〜 `test_ap3_*`: AP 系
- `test_cz1_*` 〜 `test_cz3_*`: CZ 系
- `test_cf1_*` 〜 `test_cf3_*`: CF 系

LS/HH/MF/IN/GE/SW-F は `engine.rs:1368-1388` 付近に走時 `assert_valid_phrase()` を埋め込み、UI 表示時に「確実に」「必ず」「100%」「絶対」等が混入しないことを保証。

---

**改訂履歴**:
- 2026-04-26: 新規作成 (P4 / audit_2026_04_24 #10 対応)。Plan P4 §11 から独立カタログ化
