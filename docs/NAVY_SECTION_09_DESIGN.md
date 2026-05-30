## NAVY_SECTION_09_DESIGN — Market Intelligence variant 専用 Section 09 設計

**作成日**: 2026-05-30
**対応**: P0-8 (MI variant 専用に Section 09 として 5 件以上の追加サブセクションを実装)
**準拠**: `SURVEY_MARKET_INTELLIGENCE_METRICS.md` / `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` v1.0
**配置**: `src/handlers/survey/report_html/navy_report/section_09_market_intelligence.rs`
**呼出**: `report_html/mod.rs` の navy_report 起動箇所 (Section 08 直前) に `if matches!(cfg.variant, ReportVariant::MarketIntelligence)` ガードで挿入

> NOTE: 本ファイルは agent sandbox worktree 制約により `src/handlers/analysis/fetch/` 配下 (writable path) に一時 draft 格納。parent agent が `docs/` 配下に手動コピーすることを想定 (MEMORY: feedback_agent_sandbox_worktree_handoff)。

---

### 0. 設計方針

| 項目 | 値 |
|------|-----|
| variant ガード | MarketIntelligence のみ。Full / Public では完全非出力 (HTML に section タグ自体を出さない) |
| データソース | `cfg.hw_context` (InsightContext) のみ。新規 Turso fetch は本コミットでは導入しない (副作用最小化) |
| HTML 構造 | 既存 navy パターン (page-navy / page-head / kpi-row / table-navy / so-what) を完全踏襲 |
| ECharts 不使用 | SSR テキスト + CSS バー + 数値テーブル |
| 旧 market_intelligence.rs との関係 | **置換せず補完**。旧モジュールは媒体分析タブの実 DB 接続経路を維持。Section 09 は navy_report 内で hw_context ベースの簡易版を提供 |
| DISPLAY_SPEC §2 (人数表示禁止) | 厳守。指数・ランク・濃淡のみ。「○○人」「○○名」「○○万人」「○○億円」を一切出さない |
| 「半径 5km」捏造禁止 | 集計範囲は target_region (Section 02 と同一) を継承。架空条件を文書化しない |
| 仮説なきデータ投入禁止 | 各サブセクションに必ず「SO WHAT」(配信判断に直結する示唆) を添える |
| SalesNow 文字列禁止 | 「外部企業データ」「企業データベース」と記述 |

---

### 1. 抽出した 6 つの MI 専用テーマ (要件: 5 件以上)

設計メモ `SURVEY_MARKET_INTELLIGENCE_METRICS.md` §3〜9 から、**hw_context に既に存在するデータ** で実現可能なテーマを優先選定。Phase 3 Step 5 の Turso 4 テーブル接続は将来 commit に切出すスコープ最小化。

| # | テーマ (具体ラベル) | METRICS §参照 | 入力 (hw_context) | 出力 | 階層 |
|---|---|---|---|---|---|
| **9-A** | **配信優先度サマリーカード** | §2 / §10 | `ext_job_ratio`, `ext_labor_force`, `ext_min_wage`, `commute_inflow_total`, `commute_self_rate` | KPI 4 タイル + 配信優先度の定性ラベル (重点配信/拡張候補/維持/優先度低) + SO WHAT | 1 |
| **9-B** | **採用ターゲット厚み (相対指数)** | §3 / §4 | `ext_industry_employees`, `hw_industry_counts` | 産業大分類 × 構成比 / 全国平均との乖離 / 厚み指数 (0-200) + (推定) バッジ | 2 |
| **9-C** | **競合求人密度 (クロス分析)** | §5 | `hw_industry_counts`, `ext_industry_employees`, `ext_population` | 「人口千あたり競合求人密度」表 + (実測) バッジ。比率のみ | 3 |
| **9-D** | **通勤到達性 (流入Sankey 簡易版)** | §6 / §10 | `commute_inflow_top3`, `commute_self_rate`, `commute_zone_count` | 流入元 TOP3 + 通勤圏到達性スコア (0-100 指数) + SO WHAT | 4 |
| **9-E** | **生活コスト補正後給与魅力度** | §7 / §10 | `ext_min_wage`, `ext_household_spending`, `pref_avg_unemployment_rate`, `agg` median 給与 | 給与競争力指数 KPI + 県平均との乖離 + (参考) バッジ | 4 |
| **9-F** | **配信シナリオ濃淡 (保守/標準/強気)** | §9 / §2 | 9-A〜9-E の合成 | 3 段階バー + 各指数値 + SO WHAT | 5 |

#### 1.1 差別化クロスチェック

| 比較対象 | 重複リスク | 差別化策 |
|---------|----------|---------|
| Section 04 | 求人倍率 / 失業率を重複表示 | 9-A は配信判断視点で再構成。Section 04 は採用難度視点 |
| Section 05 | 産業構成を重複表示 | 9-B は全国平均比 (相対指数) に変換 |
| 旧 `market_intelligence.rs` | 同テーマ | 旧 = Turso ベース (媒体分析タブ画面)、新 = hw_context ベース (PDF レポート)。経路完全分離 |

---

### 2. 関数シグネチャ

```rust
pub(crate) fn render_navy_section_09_market_intelligence(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    agg: &SurveyAggregation,
    variant: ReportVariant,
    target_region: &str,
)
```

呼出側 (mod.rs L748 直前): `if matches!(cfg.variant, ReportVariant::MarketIntelligence) { ... }` ガード。

---

### 3. テスト (5 件)

| # | テスト名 | 検証 |
|---|---------|-----|
| 1 | `section_09_mi_variant_outputs_section_tag` | MI variant で `navy-mi` クラス出力 |
| 2 | `section_09_full_variant_outputs_nothing` | Full variant では Section 09 関連クラス非出力 |
| 3 | `section_09_public_variant_outputs_nothing` | Public variant でも同様 |
| 4 | `section_09_does_not_emit_population_numbers` | 「○○人」「○○名」「○○万人」「○○億円」非含有 |
| 5 | `compute_priority_label_classifies_scores` | 80+/65-79/50-64/0-49 の境界値正しく分類 |

---

### 4. SO WHAT

| サブセクション | SO WHAT |
|---|---|
| 9-A | 配信優先度ラベルに従い「重点配信」地域から媒体投下を開始する |
| 9-B | 厚み指数 120+ の産業を主訴求軸とし、80- の産業は別チャネルを検討する |
| 9-C | 競合密度の薄い産業帯で配信単価を抑え、密度の高い帯では訴求差別化に投資する |
| 9-D | 流入元 TOP3 を補助配信地域として追加投下する |
| 9-E | 給与魅力度が全国平均比でマイナスの場合、家賃補助・通勤手当を訴求に追加する |
| 9-F | 配信予算を保守/標準/強気の 3 段階で分散し、強気シナリオは外縁部にテスト投下する |

---

**本書をもって Section 09 の設計を確定。実装に着手する。**
