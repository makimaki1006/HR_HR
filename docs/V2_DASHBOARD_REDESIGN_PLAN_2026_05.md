# V2 ダッシュボード再設計プラン (2026-05-15 策定)

## 背景

MEMORY `project_dashboard_redesign_2026_05.md` (2026-05-14) の方針:
- レポート (PDF) と同じデータを **アプリ側 (画面/タブ) でも個別確認可能** にする
- HW (ハローワーク求人) データは **抑えめ** に表示
- **SalesNow + 公的統計 + CSV** をメインに据える
- **未着手** (方針記録のみ → 本ドキュメントで具体プラン化)

2026-05-15 Section 7.5 振り分け作業で、レポート側で 12 表が Section 02/04/06/07 に統合済み。本プランはそれと対応する画面タブ機能を設計する。

---

## 1. 現状把握

### 1.1 タブ構成 (`templates/dashboard_inline.html` L79-102)

| # | タブ名 | route | ハンドラ | データ性質 |
|---|--------|-------|---------|-----------|
| 1 | 市場概況 | `/tab/market` | `handlers::market::tab_market` | HW中心 |
| 2 | 地図 | `/tab/jobmap` | `jobmap::tab_jobmap` | HW中心 |
| 3 | 地域カルテ | `/tab/region_karte` | `region::tab_region_karte` | HW + 公的統計 |
| 4 | 詳細分析 | `/tab/analysis` | `analysis::tab_analysis` | HW + 公的統計 (6 subtab) |
| 5 | 総合診断 | `/tab/insight` | `insight::tab_insight` | HW中心 (22 insight pattern) |
| 6 | トレンド | `/tab/trend` | `trend::tab_trend` | HW時系列 |
| 7 | 都道府県比較 | `/tab/comparison` | `comparison::tab_comparison` | HW中心 |
| 8 | 求人検索 | `/tab/competitive` | `competitive::tab_competitive` | HW中心 |
| 9 | 条件診断 | `/tab/diagnostic` | `diagnostic::tab_diagnostic` | 入力フォーム |
| 10 | 採用診断 | `/tab/recruitment_diag` | `recruitment_diag::tab_recruitment_diag` | HW + SalesNow + 公的統計 |
| 11 | 企業検索 | `/tab/company` | `company::tab_company` | **SalesNow中心** |
| 12 | 媒体分析 | `/tab/survey` | `survey::tab_survey` | CSV |

### 1.2 ext_* データの画面表示状況 (棚卸し)

| データ | レポート (PDF) | 画面 (Dashboard) | 棚卸し |
|--------|---------------|-----------------|-------|
| `commute_inflow_top3` | ✅ Section 02 表 2-C | ❌ なし | 新規追加 |
| `ext_geography` | ✅ Section 02 表 2-B | ❌ なし | 新規追加 |
| 県平均比較 (マクロ指標) | ✅ Section 02 表 2-D | △ (region_karte 一部) | 統合 |
| `ext_job_ratio` | ✅ Section 04 表 4-A | ✅ subtab5 | OK |
| `ext_establishments` | ✅ Section 04 表 4-C | ✅ subtab5 | OK |
| `ext_business_dynamics` | ✅ Section 04 表 4-D | ✅ subtab5 | OK |
| `ext_population/pyramid` | ✅ Section 06 表 6-B | ✅ subtab5 | OK |
| `ext_migration` | ✅ Section 06 表 6-C | ✅ subtab5 | OK |
| `ext_vital` (自然増減) | ✅ Section 06 表 6-D | ❌ なし | 新規追加 |
| `ext_education_facilities` | ✅ Section 06 表 6-A | ❌ navy 専用 | 新規追加 |
| `ext_education` (進学率) | ✅ Section 06 表 6-F | ✅ subtab5 | OK |
| `ext_labor_force` | ✅ Section 06 表 6-E | ✅ subtab5 | OK |
| `ext_min_wage` | ✅ Section 07 | ✅ subtab5 | OK |
| `ext_household_spending` | ✅ Section 07 表 7-A | ✅ subtab5 | OK |
| 通勤圏 (subtab7) | ✅ Section 07 表 7-B | ✅ subtab7 | OK |
| `ext_daytime_pop` (昼夜間) | ✅ Section 07 表 7-C | ✅ subtab5 | OK |
| `ext_households` (世帯) | ✅ Section 07 表 7-D | ✅ subtab5 | OK |

**ギャップ**: `commute_inflow_top3` / `ext_geography` / `ext_vital` / `ext_education_facilities` の **4 件のみ未画面化**。他は subtab5 集約済み。

---

## 2. 再設計後の構成案 (推奨: 詳細分析タブ内 subtab 再編)

「**詳細分析タブの subtab5 (異常値・外部) を分解して PDF Section 02/04/06/07 構成に合わせる**」を中核とする。新規タブ追加ではなく、既存 `/api/analysis/subtab/{id}` のサブタブ拡張で実現。

### 2.1 新サブタブ構成 (`ANALYSIS_SUBTABS` 改訂)

```
詳細分析タブ /tab/analysis
  ├─ subtab 1: 求人動向         [既存 HW] そのまま
  ├─ subtab 2: 給与分析         [既存 HW] そのまま
  ├─ subtab 4: 市場構造         [既存 HW] そのまま
  ├─ subtab 6: 予測・推定       [既存 HW] そのまま
  ├─ subtab 7: 通勤圏           [既存 公的統計] そのまま
  ├─ subtab 10: 地域診断 (NEW)  [Section 02 相当・公的統計中心]
  │    地理基礎 / 通勤流入元 / 県平均比較
  ├─ subtab 11: 採用市場 (NEW)  [Section 04 相当・公的統計中心]
  │    有効求人倍率 / 産業密度 / 事業所統計 / 開廃業動態 / 離職率
  ├─ subtab 12: デモグラフィック (NEW) [Section 06 相当]
  │    ピラミッド / 人口統計 / 人口移動 / 自然増減 (NEW) / 労働力 / 進学率 / 教育施設 (NEW)
  ├─ subtab 13: ライフスタイル (NEW) [Section 07 相当]
  │    最低賃金 / 家計支出 / 昼夜間人口 / 世帯構成 / インターネット / 自動車保有
  └─ subtab 5: 異常値のみ       [縮退: anomaly + region_benchmark のみ]
```

---

## 3. 流用 vs 新規実装

### 3.1 流用可能 (subtab5_anomaly.rs から振り分け)

| 流用元関数 | 振り分け先 |
|-----------|----------|
| `render_prefecture_stats_section` (L851) | subtab 10 地域診断 |
| `render_job_openings_ratio_section` (L423) | subtab 11 採用市場 |
| `render_labor_stats_section` (L629) | subtab 11 + 12 |
| `render_establishment_section` (L1132) | subtab 11 |
| `render_business_dynamics_section` (L1513) | subtab 11 |
| `render_turnover_section` (L1194) | subtab 11 |
| `render_care_demand_section` (L1632) | subtab 11 |
| `render_population_section` (L957) | subtab 12 |
| `render_demographics_section` (L1068) | subtab 12 + 13 |
| `render_education_section` (L1747) | subtab 12 |
| `render_household_type_section` (L1808) | subtab 13 |
| `render_foreign_residents_section` (L1856) | subtab 12 |
| `render_minimum_wage_section` (L239) | subtab 13 |
| `render_wage_compliance_section` (L338) | subtab 13 |
| `render_household_spending_section` (L1428) | subtab 13 |
| `render_land_price_section` (L1886) | subtab 13 |
| `render_regional_infra_section` (L1944) | subtab 13 |
| `render_social_life_section` (L2012) | subtab 13 |
| `render_boj_tankan_section` (L2083) | subtab 11 |
| `render_climate_section` (L1578) | subtab 13 (任意) |
| `render_region_benchmark_section` (L2207) | subtab 5 残置 |

→ **20 関数を振り分けるだけ**

### 3.2 新規実装が必要

#### A. データ render 関数 (4 件)

navy_report.rs から HTML 構造をポート:

1. `render_geography_section` (Section 02 表 2-B 相当)
2. `render_commute_inflow_section` (Section 02 表 2-C 相当)
3. `render_vital_section` (Section 06 表 6-D 相当、自然増減)
4. `render_education_facilities_section` (Section 06 表 6-A 相当)

#### B. ハンドラ / ルーティング (5 件)

- `analysis::helpers::ANALYSIS_SUBTABS` に id=10/11/12/13 追加
- `analysis::handlers::analysis_subtab` の match 拡張
- `analysis/render/mod.rs` で 4 module 宣言・公開

---

## 4. Phase 分け

### Phase 1: 基盤・流用配置 (Day 1-2)

- `ANALYSIS_SUBTABS` に id=10/11/12/13 追加
- `analysis/render/` 配下に `subtab10_region.rs` / `subtab11_market.rs` / `subtab12_demographics.rs` / `subtab13_lifestyle.rs` を新規追加 (subtab5_anomaly.rs から該当関数を pub(super) use で呼ぶだけ)
- `analysis/render/mod.rs` で module 宣言・再 export
- `analysis/handlers.rs` の subtab dispatcher 拡張
- `subtab5_anomaly.rs:18 render_subtab_5` を縮退
- 既存 subtab5 統合テストの修正

**成果**: 既存テスト全 pass のまま PDF Section 02-07 構成を画面化。

### Phase 2: ギャップ埋め (Day 3-4)

- `navy_report.rs:1554-` (表 2-B), `1570-` (表 2-C), `3267-` (表 6-D), `3287-` (表 6-A) のロジックを参照
- 新 render 関数 4 つを実装
- 対応 fetch 関数を追加 (`fetch_vital` / `fetch_education_facilities` / `fetch_geography` / `fetch_commute_inflow_top3`)
- ドメイン不変条件テスト追加

**成果**: PDF レポート Section 02/04/06/07 と画面の表が 1:1 で対応。

### Phase 3: UI 整備 (Day 5-7)

- subtab10-13 の navy 配色 (PDF) を web 配色に統一
- ECharts ベース インタラクティブ化
- フィルタ連動 (年範囲スライダ / 市区町村切替)
- 市場概況タブ上部「公的統計サマリ KPI ストリップ」追加
- 企業検索タブのタブ順序を 2 番目に移動
- HW 色を控えめに、SalesNow + 公的統計を強調

**成果**: 「公的統計と SalesNow がメイン」と認識できる UI。

---

## 5. 見積もり

| Phase | Day | 成果物 | リスク |
|-------|-----|-------|--------|
| Phase 1 | 1-2 | subtab10-13 の枠 + 既存関数の振り分け | 依存解決 |
| Phase 2 | 3-4 | 4 つの新 render 関数 + fetch | navy ロジック移植時の細部差異 |
| Phase 3 | 5-7 | CSS + インタラクティブ化 + 並び替え | ECharts ブラウザ差異 |
| **合計** | **7 営業日** | | |

**最小 MVP** (Day 1-2 のみ) でも「PDF と画面構成が一致」状態には到達可能。

---

## 6. 議論ポイント (ユーザー判断要)

1. **タブ追加 vs subtab 内分割**: 推奨は subtab 内分割。代案で上位タブを 12→16 個に増やすのは過密。
2. **HW 抑制の程度**:
   - 案A: CSS 配色のみ抑える
   - 案B: HW タブを「HW データ」グループに折り畳む
   - 案C: 画面でも Full/Public variant 切替
3. **subtab5 の今後**:
   - 案A: anomaly + region_benchmark のみに縮退 (推奨)
   - 案B: subtab5 自体を廃止し新 subtab に統合
4. **SalesNow centerpiece 化の方法**:
   - 案A: タブ順序入れ替えのみ
   - 案B: 市場概況に「注目企業 TOP10」ウィジェット
   - 案C: SalesNow 専用タブ群 (急成長/大手/中堅/採用活発)
5. **Round 1-3 投入予定の SSDSE-A / Agoop OD** の取扱い: Turso 投入完了後 Phase 2 へ追加組込 (1-2 Day 追加)

---

## 7. 重要ファイル位置

- 流用元 (PDF): `src/handlers/survey/report_html/navy_report.rs` — Section 02 (L1460), 04 (L1843), 06 (L3062), 07 (L3502)
- 流用元 (画面): `src/handlers/analysis/render/subtab5_anomaly.rs` (2272 行、20 関数集約済み)
- subtab 一覧: `src/handlers/analysis/helpers.rs:9`
- ルート: `src/lib.rs:87-91`
- InsightContext: `src/handlers/insight/fetch.rs:25-89`
- タブ HTML: `templates/dashboard_inline.html:79-102`
- SalesNow: `src/handlers/survey/report_html/salesnow.rs`

---

## 8. 着手前のチェックリスト

- [ ] ユーザーが議論ポイント 1-4 を選択
- [ ] Round 1-3 SSDSE-A / Agoop OD の Turso 投入状況確認
- [ ] 既存 subtab5 テスト一覧抽出 (Phase 1 で移動が必要なため)
- [ ] navy_report.rs の Section 02/04/06/07 で追加された insight ロジックの移植戦略 (CSS スコープ + 関数粒度)
