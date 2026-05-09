# 採用マーケット診断レポート オープンデータ活用総監査

**日付**: 2026-05-09
**対象 repo**: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy`
**性質**: read-only 監査 (実装ゼロ、DB 書込ゼロ、push なし)
**監査体制**: 5 並列専門 agent + 親統合
**前提**: PDF 実物に出ているものだけが成果。E2E PASS / DOM 存在 / unit test だけでは完了扱いしない。CSV にない業界別給与・職種別給与は作らない。

---

## 0. エグゼクティブ・サマリ (3 行)

1. **30 分析中、PDF に "採用示唆" として使える品質で出ているのは 10 項目**。残り 20 項目は不在 / 品質低 / fetch 済みだが render 未接続。
2. **DB に取り込まれているのに PDF に到達していない領域が 13 テーブル分ある** (事業所 / 医療介護 / 移動 / 昼間人口 / 外国人 / 地価 / 自動車保有 / 気候 / 労働統計 等)。死蔵 + デッドコード。
3. **最大の品質問題は実装漏れではなく "経路の取り違え" 3 件**: ① 最低賃金ハードコード ② 産業構造の空配列既定 ③ MI variant で職業×地域マトリクスが対象地でない地域 (北海道伊達市) を大量列挙し読めない。

---

## 1. 現在 PDF に実際に出ている分析

**対象 PDF**: `out/round3_cprime_pdf_review/mi_via_action_bar.pdf` (26 ページ / 6.4 MB / 2026-05-09 17:38 / variant=MarketIntelligence / 対象=東京都新宿区 + サンプル CSV 54 件)。

| # | 分析 | PDF 表示 | ページ | データソース | 採用示唆に使えるか | 備考 |
|---|------|---------|------|--------------|------------------|------|
| 1 | CSV 給与分布 | ✅ | 4-6,13 | CSV | 使える | ヒストグラム + 散布図 R²=0.969 |
| 2 | CSV 求人数 | ✅ | 1-3 | CSV | 使える | 表紙 + 全国 KPI |
| 3 | 地域別求人件数 | ✅ | 14-15 | CSV | 使える | 都道府県/市区町村 |
| 4 | 産業構成 (e-Stat) | ✅ | 8-9 | `v2_external_industry_structure` | 使える | CSV ミスマッチも併載 |
| 5 | 職業人口 (国勢調査) | △ | 22-24 | `municipality_occupation_population` | **読めない** | 対象地 (新宿区) でなく **「北海道伊達市」が 100 行近く羅列**、ヘッダ崩壊 |
| 6 | 性別構成 | △ | 10 | pyramid | 図のみ | 構成比表は不在 |
| 7 | 年齢構成 | ✅ | 10-12,15 | pyramid | 使える | |
| 8 | 地域 × 産業 | ✅ | 8-9 | industry_structure | 使える | 都道府県粒度のみ |
| 9 | 地域 × 職業分類 | △ | 22-24 | occupation_pop | 限定 | #5 と同じ品質問題 |
| 10 | 地域 × 性別 × 年齢 | ❌ | - | - | - | 地域全体ピラミッドのみ、市区町村クロスなし |
| 11 | 地域 × 産業 × 性別 | ❌ | - | - | - | 未実装 |
| 12 | 地域 × 産業 × 年齢 | ❌ | - | - | - | 未実装 |
| 13 | 地域 × 産業 × 性別 × 年齢 | ❌ | - | - | - | DB スキーマ自体が無い |
| 14 | 地域 × 職業 × 性別 × 年齢 | ❌ | - | - | - | 同上 (occupation_pop は粒度不足) |
| 15 | 採用示唆ランキング | △ | 19-20 | recruiting_scores | 弱い | 冒頭で **「S/A 該当なし」**、訴求として空打ち |
| 16 | 配信優先地域 | ✅ | 20 | thickness + scores | 使える | 4 自治体のみ表示 |
| 17 | 給与妥当性 | ✅ | 16 | 最賃 + benchmark | 使える | ハードコード問題あり (§3) |
| 18 | 生活コスト比較 | △ | 21 | living_cost_proxy | 限定 | 「都道府県値を市区町村に流用」と注記 |
| 19 | 通勤流入 / 昼間人口 | △ | 19,22 | commute_od + daytime_pop | 部分 | 流入 Top 10 のみ。**昼間人口は ctx 取得済だが survey PDF で参照ゼロ** |
| 20 | 最低賃金 | ✅ | 16,21 | (ハードコード) | 使える | DB の `v2_external_minimum_wage` 47 行は **読まれていない** |
| 21 | 事業所数 | △ | 8 | - | - | 文中言及のみ、絶対値なし |
| 22 | 開業/廃業率 | ✅ | 8 | business_dynamics | 使える | |
| 23 | 入職/離職率 | ✅ | 7-8 | turnover | 使える | 都道府県粒度 |
| 24 | 医療介護需要 | ❌ | - | - | - | fetch 済みだが render なし |
| 25 | 教育施設 / 学歴 | ✅ | 10-11 | education | 中 | |
| 26 | 外国人住民 | ❌ | - | - | - | InsightContext に **`ext_foreign: 未実装のため省略`** |
| 27 | 地価 | ❌ | - | - | - | fetch 済み、Context フィールド自体なし |
| 28 | 自動車保有 / 交通 | ❌ | - | - | - | 同上 |
| 29 | 気候 / 地理 | △ | 15 | geography | 低 | 可住地密度のみ |
| 30 | インターネット利用 / 社会生活 | ✅ | 16-17 | internet_usage + social_life | 中 | lifestyle セクション |

**集計**: ✅ 完全表示 13 / △ 部分 8 / ❌ 不在 9。

---

## 2. 本来必要だった分析 (MECE)

### A. CSV 由来 (条件側) — すべて表示済み
| 必要分析 | 必須度 | PDF |
|---|---|---|
| 求人数 | 必須 | ✅ |
| 勤務地 / 自治体 | 必須 | ✅ |
| 給与中央値 | 必須 | ✅ |
| 給与レンジ | 必須 | ✅ |
| 地域別求人件数 | 必須 | ✅ |

### B. オープンデータ由来 (市場側、母集団)
| 必要分析 | 必須度 | DB | PDF |
|---|---|---|---|
| 自治体別 人口 | 必須 | ✅ | ✅ |
| 自治体別 年齢 × 性別 (ピラミッド) | 必須 | ✅ | ✅ |
| 自治体別 産業 × 性別 | 必須 | ❌ DB 粒度なし | ❌ |
| 自治体別 産業 × 年齢 × 性別 | 必須 | ❌ | ❌ |
| 自治体別 職業分類 × 年齢 × 性別 | 必須 | ✅ (729,949 行) | △ 品質崩壊 (§1#5) |
| 通勤流入 (OD) | 必須 | ✅ (86,762 ペア) | △ |
| 昼間人口 | 重要 | ✅ (1,740) | ❌ ctx 取得済だが render 未参照 |
| 生活コスト | 必須 | 🟡 物価指数のみ | △ 注記あり |
| 最低賃金 | 必須 | ✅ (47) | ⚠️ ハードコード (DB 無視) |

### C. クロス示唆
| 必要分析 | 必須度 | PDF |
|---|---|---|
| CSV 求人数 × 地域母集団 (4 象限) | 必須 | ❌ 未実装 |
| CSV 給与 × 生活コスト | 必須 | △ 部分 |
| 地域 × 職業 × 年齢 × 性別 | 必須 | ❌ |
| 地域 × 産業 × 年齢 × 性別 | 必須 | ❌ DB 粒度なし |
| 採用ターゲットランキング | 必須 | △ 弱 |

---

## 3. 既存 DB / データ資産の棚卸し

`data/hellowork.db` (1.96 GB / 50 テーブル / WAL 有り)。

### A. 採用示唆クロスのコア (✅ 揃っている)
| テーブル | 行数 | 粒度 |
|---|---|---|
| `municipality_occupation_population` | **729,949** | muni × 職業 × 年齢階級 × 性別 × basis (resident/workplace) |
| `v2_external_population` | 1,917 | pref × muni |
| `v2_external_population_pyramid` | 17,235 | pref × muni × age_group × 性別 |
| `v2_external_foreign_residents` | 1,742 (Local) / **282 (Turso) ⚠ 不一致** | pref × muni |
| `v2_external_commute_od_with_codes` | 86,762 | OD ペア (muni_code 付き) |
| `v2_external_daytime_population` | 1,740 | pref × muni |

### B. 集計済みアセット (Phase 3 / Round 4 で投入済)
| テーブル | 行数 | 用途 |
|---|---|---|
| `v2_municipality_target_thickness` | 20,845 | muni × 職業 × thickness_index (Model F2) |
| `municipality_recruiting_scores` | 20,845 | muni × 職業 × 配信優先度スコア (Round 4 完了済) |
| `municipality_living_cost_proxy` | 1,917 | muni × 物価指数 (家計支出は無し) |

### C. ❌ ローカル DB 未投入 (Turso のみ、または死蔵)
- `v2_external_industry_structure` (Local MISSING / Turso のみ)
- `v2_external_household_spending`, `v2_external_labor_stats`
- `v2_external_land_price`, `v2_external_establishments`
- `v2_external_business_dynamics`, `v2_external_turnover` (Turso のみ)
- `v2_external_care_demand`, `v2_external_medical_welfare`
- `v2_external_education`, `v2_external_education_facilities`
- `v2_external_climate`, `v2_external_geography`
- `v2_external_car_ownership`, `v2_external_internet_usage`, `v2_external_social_life`
- `v2_external_minimum_wage_history` (Local 単年のみ、履歴は Turso)

### D. ❌ どこにも無い (DB スキーマ自体がない)
- 産業 × 性別、産業 × 年齢、産業 × 性別 × 年齢 (4 軸クロス)
- 事業所数の絶対値 (経済センサス)
- 雇用入離職率の自治体粒度

### Turso 同期警告 (`docs/turso_v2_sync_report_2026-05-03.md`)
- 29 テーブル LOCAL_MISSING / 2 テーブル REMOTE_MISSING (`v2_external_minimum_wage`, `v2_external_commute_od`)
- 5 テーブル SAMPLE_MISMATCH (population / pyramid / daytime_pop / migration / prefecture_stats)
- foreign_residents は **COUNT_MISMATCH** (Local 1742 vs Remote 282)

---

## 4. fetch / render / PDF 接続状況

### 4-1. 接続マップ (重要 30 分析、状態判定)

凡例: ✅ フル接続 / ⚠ variant 制限 / ❌ 未接続 / —不在

| # | 分析 | DB | fetch 関数 (file:line) | DTO ctx フィールド | render 関数 | mod.rs | 状態 |
|---|------|----|----|----|----|---|---|
| 1-3 | CSV 系 | CSV | aggregator | `agg.*` | salary_stats / region | ✅ | フル |
| 4,8 | 産業構造 | 🟡 Turso | `subtab5_phase4_7.rs:284` | `ext_industry_employees` (**`fetch.rs:179` で `vec![]` 空初期化**) | `mod.rs:850 industry_mismatch` / `region.rs:269` | ⚠ MI/integrate のみ上書き | **空配列バグ** |
| 5,9 | 職業人口 | ✅ | `market_intelligence.rs:226` | `occupation_populations` | `render_mi_talent_supply` `render_mi_occupation_cells` | ✅ MI のみ | 品質崩壊 |
| 6,7,10 | デモグラ | ✅ pyramid | `subtab5_phase4.rs:198` | `ext_pyramid` | `demographics.rs:413` | ✅ Full のみ | MI 非表示 |
| 11-14 | 4 軸クロス | ❌ | — | — | — | — | DB 不在 |
| 15,16 | 採用ランキング | ✅ | `market_intelligence.rs:52,362,447,475` | `recruiting_scores` 等 | `render_mi_distribution_ranking` `render_mi_parent_ward_ranking` | ✅ MI のみ | 弱い |
| 17,20 | 最賃/給与妥当性 | ✅ (47) | `subtab5_phase4.rs:33` | (helpers ハードコード) | `wage.rs:169` `helpers.rs:936-958` | ✅ | **ハードコードで DB 無視** |
| 18 | 生活コスト | ✅ | `market_intelligence.rs:120` | `living_cost_proxies` | `render_mi_living_cost_panel` | ✅ MI のみ | |
| 19 | 通勤/昼間 | ✅ | `subtab5_phase4.rs:267,276` | `ext_daytime_pop` `commute_inflow_top3` | `render_mi_commute_inflow_supplement` | **survey PDF (mod.rs) は ctx.ext_daytime_pop を参照ゼロ** | 半接続 |
| 21 | 事業所 | 🟡 Turso | `subtab5_phase4.rs:354` | `ext_establishments` | (**ゼロ参照**) | ❌ | **fetch 済み未 render** |
| 22,23 | 開廃業/入離職 | 🟡 Turso | `subtab5_phase4.rs:431,383` | `ext_business_dynamics` `ext_turnover` | `market_tightness.rs:469,456` | ✅ | フル |
| 24 | 医療介護 | 🟡 Turso | `subtab5_phase4.rs:488` `subtab7_phase_a.rs:174` | `ext_care_demand` `ext_medical_welfare` | (**ゼロ参照**) | ❌ | fetch 済み未 render |
| 25 | 教育/学歴 | 🟡 Turso | `subtab5_phase4_7.rs:47` | `ext_education` | `demographics.rs:128,465` | ✅ Full / Public | |
| 26 | 外国人 | ✅ Local (但し不一致) | `subtab5_phase4_7.rs:19` | (**`fetch.rs:44` `// ext_foreign: 未実装のため省略`**) | — | ❌ | DTO フィールド自体なし |
| 27 | 地価 | 🟡 Turso | `subtab5_phase4_7.rs:136` | (Context フィールドなし) | — | ❌ | 同上 |
| 28 | 自動車 | 🟡 Turso | `subtab5_phase4_7.rs:162` | (Context フィールドなし) | — | ❌ | 同上 |
| 29 | 気候/地理 | 🟡 Turso | `subtab5_phase4.rs:459` `subtab7_phase_a.rs:263` | `ext_climate` `ext_geography` | `regional_compare.rs:307` (geography のみ Public) | ⚠ | climate 未 render |
| 30 | ネット/社会生活 | 🟡 Turso | `subtab5_phase4_7.rs:183,112` | `ext_internet_usage` `ext_social_life` | `lifestyle.rs:39` | ✅ | フル |

### 4-2. variant 別 render の差 (`mod.rs`)

| variant | demographics | regional_compare | industry_structure (印刷) | industry_salary (Round 3-A) | occupation_salary (Round 3-C) | salesnow |
|---|---|---|---|---|---|---|
| Full | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Public | ❌ | ✅ | ❌ | ❌ | ❌ | ✅ |
| **MarketIntelligence** | ❌ | ✅ | ✅ | ✅ | ✅ (**0/16 hit**) | ❌ |

→ MI variant では人口ピラミッド / 市区町村別デモグラが **出ない設計**。Round 3-A/3-C を MI に追加した代わりに失われている。

### 4-3. Round 3-C (occupation_salary) 0/16 hit の根本原因

`occupation_salary.rs:426 if rows.is_empty() { return; }` が silent に発動。原因連鎖:

1. `aggregate_occupation_salary` が `agg.by_tag_salary` / `agg.by_company` を CSV 由来辞書 (`map_keyword_to_occupation_group` `occupation_salary.rs:128-296`) で照合。
2. テスト CSV のタグ列は `"16万","18万",...,"50万"` (給与レンジ軸) しかなく、職種語彙ゼロ。
3. 全件 `continue` で `rows` 空 → セクション完全スキップ。

**結論**: Round 3-C/3-C' の実装は正しいが、**入力データに職種語彙がそもそも無い**。CSV 列設計の問題であり、辞書拡張では解決しない (Round 3-C-2 監査で確認済)。**MI variant では職種推定セクションを諦めるべき**。

### 4-4. 「fetch 済みだが render から ctx を参照していない」ものリスト
`ext_establishments` / `ext_care_demand` / `ext_medical_welfare` / `ext_climate` / `ext_vital` / `ext_migration` / `ext_daytime_pop` / `ext_labor_stats` の 8 ctx フィールドが、`report_html/*.rs` から参照ゼロ (Agent C 実検)。`integration.rs` (Tab UI) のみで利用されており、PDF には届かない。

### 4-5. legacy 残置
- `render_section_hw_comparison` (hw_enrichment.rs:303) — mod.rs 未接続、コメントで legacy 明示
- `render_section_company_segments` (salesnow.rs:417) — `_with_industry` 版に置換済

---

## 5. 重要クロスの実現可否

| クロス | DB 粒度 | fetch | render | PDF | 結論 |
|---|---|---|---|---|---|
| 1. 地域 × 職業 × 性別 × 年齢 | ✅ `municipality_occupation_population` (729K行) | ✅ | ⚠ MI のみ品質崩壊 | △ | **再設計で復活可能** (P0 候補) |
| 2. 地域 × 産業 × 性別 | ❌ DB なし | ❌ | ❌ | ❌ | 不可。新規 fetch 必要 |
| 3. 地域 × 産業 × 年齢 | ❌ | ❌ | ❌ | ❌ | 同上 |
| 4. 地域 × 産業 × 性別 × 年齢 | ❌ | ❌ | ❌ | ❌ | 同上 |
| 5. CSV 求人数 × 地域母集団 (4 象限) | ✅ (求人 + thickness) | ✅ | ❌ render なし | ❌ | **既存 DB で実装可能** (P0 候補) |
| 6. CSV 給与 × 生活コスト | ✅ | ✅ | △ MI のみ | △ | 既に出ているが品質改善余地 |
| 7. 採用示唆ランキング | ✅ recruiting_scores (20,845 行) | ✅ | △ S/A 該当なし表示 | △ | スコア閾値の調整が必要 |

---

## 6. 現状の失敗点 (成果物基準)

| 失敗 | 証拠 |
|------|------|
| **PDF 見切れ修正に時間を使いすぎた** | Round 2.7-2.11 で viewport / margin / chart bbox に集中。docs `PDF_*_2026_05_08.md` 11 件、`PDF_*_2026_05_09.md` 5 件 = 16 件が見切れ調査 |
| **E2E / unit test を過信した** | Round 3-C 完了報告時、E2E PASS だが PDF に 0/16 hit。fixture テストは tag_samples を確認していなかった |
| **本来の採用示唆クロスが PDF に出ていない** | §1 #10-14 (4軸系) すべて ❌。§5 #1 (職業×地域×性別×年齢) は DB あるのに品質崩壊 |
| **CSV で作れない方向に寄った** | Round 3-A/3-B/3-B'/3-C/3-C' で業界推定・職種推定に注力。CSV 由来辞書が CSV と合わず空打ち |
| **既存データ接続状況を最初に棚卸ししなかった** | 13 テーブル fetch 済み未 render が放置。今回 (Round 7) 初めて全領域横断 |
| **PDF に出る前に完了扱いした** | Round 3-C `d8156e7` push → "完了" 報告 → 後日 0/16 hit 発覚 |
| **variant 設計が分断** | MI で demographics/regional_compare 非表示。Full に出る人口ピラミッドが MI に無い |

---

## 7. 立て直し優先順位 (P0 / P1 / P2)

### P0 (3 件、採用コンサルレポートとして必須)

#### **P0-1**: 最低賃金ハードコード解除 (`helpers.rs:936-958` → DB 接続)
- **現状**: PDF の最低賃金値は Rust ベタ書き 47 県分。DB の `v2_external_minimum_wage` (47 行) は **読まれていない**
- **影響**: 給与妥当性 (§1 #17) は採用示唆の主軸。10 月改定時に PDF が古いまま出続ける運用リスク
- **必要作業**: `helpers.rs:936-958` を DB SELECT に置換、`wage.rs:169` の利用箇所を ctx 経由に統一
- **完了条件**: 同じ DB 値を直接 SELECT した結果と PDF の表示が一致 (E2E + 値レベル検証)

#### **P0-2**: 産業構造の空配列既定バグ修正 (`insight/fetch.rs:179`)
- **現状**: `ext_industry_employees: vec![]` で空初期化、`survey/handlers.rs:552` の MI/integrate 経路だけが上書き。**Full / Public の標準経路では永遠に空** → `render_section_industry_mismatch` が silent 空白化する
- **影響**: 産業ミスマッチ (§1 #4 / #8) は採用判断の主軸
- **必要作業**: `InsightContext` 構築時に必ず `fetch_industry_structure` を呼ぶ統一ヘルパーを作る (実装は Round 8)。本監査では fix 仕様の確定のみ
- **完了条件**: Full / Public / MI どの variant でも `ext_industry_employees` が空にならない (テスト: 各 variant の PDF に産業構成 Top 10 が必ず出る)

#### **P0-3**: 職業 × 地域 × 性別 × 年齢 の MI 表示崩壊修正 (`market_intelligence.rs:242` 周辺)
- **現状**: `render_mi_occupation_cells` が対象自治体 (新宿区) でなく、**北海道 伊達市の職業マトリクスを 100 行近く列挙** (P22-24 / 全 26 ページの 12% を浪費)。原因: フィルタが `target_municipalities` に限定されておらず、全国羅列が出ている可能性
- **影響**: 既存 DB (`municipality_occupation_population` 729,949 行) の最大の使い道。これが死ねば採用ターゲット示唆の核が死ぬ
- **必要作業**: フィルタを `target_municipalities` に明示限定、ページ数 (12%) を 1-2 ページに圧縮、表示形式を「対象地 × 上位 5 職業 × 年齢 × 性別」のサマリ表に変更
- **完了条件**: 対象地 (新宿区) の職業×年齢×性別が 1 ページ内で読める。「北海道 伊達市」が PDF 内に 5 件以下

### P1 (P0 後に着手)

| ID | タスク | 根拠 |
|----|-------|------|
| P1-1 | 昼間人口 (`ext_daytime_pop`) を survey PDF render に連結 | survey PDF mod.rs から参照ゼロ。配信地域示唆の補強 |
| P1-2 | CSV 求人数 × 地域母集団 4 象限図を新設 | DB は揃っている (求人 + thickness)、render が無いだけ |
| P1-3 | `municipality_living_cost_proxy` の家計支出列追加 (現状物価指数のみ) | §1 #18 の「都道府県値流用」注記を解消 |

### P2 (後回し)

| ID | タスク | 根拠 |
|----|-------|------|
| P2-1 | 事業所数 (`ext_establishments`) の市場規模パネル新設 | 寄与度: 中 |
| P2-2 | 自動車保有 (`v2_external_car_ownership`) を採用エリア半径に連結 | InsightContext フィールド追加から必要 |
| P2-3 | UI / 章構成 polish (Round 6 系の継続) | |
| P2-4 | legacy 削除 (`render_section_hw_comparison` 等) | |
| P2-5 | Turso 同期不一致解消 (foreign_residents COUNT_MISMATCH 等) | |

### 諦めるもの (実装不可 / 不要)

| 項目 | 理由 |
|------|------|
| 業界別給与 / 職種別給与 (Round 3-A 〜 3-C') | CSV に業界列・職種列なし。会社名・法人種別からの推定は採らない |
| 地域 × 産業 × 性別 × 年齢 | DB スキーマ自体がない。新規 e-Stat 取得が必要だが優先度低 |
| 医療介護需要 (`ext_care_demand`) を MI に出す | 全業種対象レポートで医療介護のみ突出させる必要性が薄い |
| 気候 (`ext_climate`) | 採用直接寄与が薄い |

---

## 8. 実装してはいけないもの (再確認)

- ❌ CSV に業界列がない状態での業界別給与
- ❌ CSV に職種列がない状態での職種別給与
- ❌ 会社名 → 職種推定 (例: 「○○病院」→ 看護系)
- ❌ 法人種別 → 職種推定 (例: 医療法人 → 看護系、社会福祉法人 → 介護系)
- ❌ PDF に出ない裏側実装だけを成果扱い
- ❌ E2E PASS のみで完了扱い

→ Round 3-C `aggregate_occupation_salary` は CSV 由来辞書で動く設計だが、本番 CSV に職種語彙が無いため事実上死んでいる。**MI variant から職種推定セクションを撤去するか、入力 CSV 側で職種列を必須化するかの判断が必要** (本監査の範囲外)。

---

## 9. 最終出力サマリ

| 優先 | タスク | 現状 | 必要作業 | 完了条件 |
|---|---|---|---|---|
| **P0-1** | 最低賃金 DB 接続 | `helpers.rs:936-958` ベタ書き、DB 47 行が無視されている | ハードコード削除 → `v2_external_minimum_wage` SELECT 経由 → `wage.rs:169` を ctx 統一 | DB 値と PDF 値が一致、改定時に DB 更新だけで反映 |
| **P0-2** | 産業構造 空配列バグ | `insight/fetch.rs:179 ext_industry_employees: vec![]`、Full/Public 経路で空のまま | InsightContext 構築統一、全 variant で `fetch_industry_structure` 必須化 | Full / Public / MI すべての PDF に産業構成 Top 10 が常時表示 |
| **P0-3** | 職業×地域×性別×年齢 MI 表示崩壊 | P22-24 で対象外自治体「北海道伊達市」が 100 行羅列、対象地 (新宿区) の数字が読めない | `render_mi_occupation_cells` のフィルタを `target_municipalities` 限定、表形式を 1-2 ページに圧縮 | 対象地のセグメント (職業×年齢×性別) が 1 ページ内で読める。誤地表示が 5 件以下 |

---

## 10. 監査メタデータ

- 監査体制: 5 並列 agent (PDF実物 / DB資産 / コード経路 / 18領域広域 / scripts・docs)
- 実行時刻: 2026-05-09
- 監査対象 PDF: `out/round3_cprime_pdf_review/mi_via_action_bar.pdf` (mtime 2026-05-09 17:38)
- 関連 docs: `OPEN_DATA_MEDIA_REPORT_VALUE_AUDIT_2026_05_09.md` (Round 7 媒体価値監査), `ROUND4_COMPLETION_REPORT.md` (Phase 3+ 完了), `turso_v2_sync_report_2026-05-03.md` (DB 同期状況)
- DB 書込: ゼロ
- コード変更: ゼロ
- Turso READ: ゼロ (Local DB のみ)
- 次の意思決定: P0 3 件のうち、どこから着手するかをユーザー判断。本監査が終わるまで追加実装は禁止 (指示書の規定通り遵守)
