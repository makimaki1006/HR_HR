//! Phase 3 Step 3: 採用マーケットインテリジェンス HTML セクション群
//!
//! `?variant=market_intelligence` のときだけ追加表示する 5 セクション:
//!
//! 1. 結論サマリーカード (`render_mi_summary_card`)
//! 2. 配信地域ランキング (`render_mi_distribution_ranking`)
//! 3. 人材供給ヒートマップ (`render_mi_talent_supply`)
//! 4. 給与・生活コスト比較 (`render_mi_salary_living_cost`)
//! 5. 保守/標準/強気 母集団レンジ (`render_mi_scenario_population_range`)
//!
//! ## ラベル規則 (METRICS.md §1)
//!
//! - 実測 (`MEASURED_LABEL`): 統計から直接集計可能な値
//! - 推定 (`ESTIMATED_LABEL`): 仮定を置いて算出する値
//! - 参考 (`REFERENCE_LABEL`): 粒度や定義に限界がある補助値
//!
//! ## 設計方針
//!
//! - データ欠損時はセクション内 placeholder (「データ準備中」) で代替し panic しない
//! - DTO の不変条件 (`is_priority_score_in_range` 等) が崩れたエントリは表示から除外し注意文に集計
//! - 既存 `Full` / `Public` variant の HTML 出力には一切影響しない
//!
//! Step 4 で追加した `variant.show_market_intelligence_sections()` フラグで分岐される。

use super::super::super::analysis::fetch::{
    fetch_code_master, fetch_commute_flow_summary, fetch_living_cost_proxy, fetch_occupation_cells,
    fetch_occupation_population, fetch_recruiting_scores_by_municipalities,
    fetch_ward_rankings_by_parent, fetch_ward_thickness, to_code_master, to_commute_flows,
    to_living_cost_proxies, to_occupation_cells, to_occupation_populations, to_recruiting_scores,
    to_ward_rankings, to_ward_thickness_dtos, CommuteFlowSummary, LivingCostProxy,
    MunicipalityCodeMasterDto, MunicipalityRecruitingScore, OccupationCellDto,
    OccupationPopulationCell, SurveyMarketIntelligenceData, WardRankingRowDto,
};
use super::super::super::helpers::escape_html;
use std::collections::BTreeMap;

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;

// --------------- ラベル定数 (METRICS.md §1) ---------------

/// 実測ラベル (HTML 表示用)
pub const MEASURED_LABEL: &str = "実測";
/// 推定ラベル
pub const ESTIMATED_LABEL: &str = "推定";
/// 参考ラベル
pub const REFERENCE_LABEL: &str = "参考";
/// 該当なしラベル (P1-1: 数値が存在しない / すべて NULL のとき視覚化)
/// P0 (2026-05-06): PDF 本文に「データ不足」内部 fallback 文言が出ていたため、
/// バッジ表示テキストを「該当なし」に変更 (mi-badge-insufficient class は維持)。
pub const INSUFFICIENT_LABEL: &str = "該当なし";

// --------------- Phase 3 Step 5 Phase 4: Plan B 表示ラベル定数 ---------------
//
// 表示分岐ルール (DISPLAY_SPEC_PLAN_B 必須要件):
// - workplace × measured 行: 人数 (population) 表示 OK + WORKPLACE_LABEL
// - resident × estimated_beta 行: 指数 (estimate_index) のみ + RESIDENT_LABEL
//   (人数表示は絶対 NG / 「推定人数」「想定人数」等 Hard NG 用語禁止)

/// 従業地ベース (国勢調査由来の実測値)
pub(crate) const WORKPLACE_LABEL: &str = "従業地ベース (実測)";
/// 常住地ベース (検証済み推定 β。人数ではなく指数のみ表示)
pub(crate) const RESIDENT_LABEL: &str = "常住地ベース (推定 β)";
/// 実測データの出典 (国勢調査 R2)
pub(crate) const MEASURED_DATA_SOURCE: &str = "国勢調査 R2";
/// 推定 β 注記 (Model F2 で検証済み)
pub(crate) const ESTIMATED_BETA_NOTE: &str = "検証済み推定 β (Model F2)";
/// 工業集積地アンカーバッジ (将来の WardThicknessDto.is_industrial_anchor 表示用)
#[allow(dead_code)]
pub(crate) const ANCHOR_BADGE: &str = "\u{1F3ED} 工業集積地";
/// 全国順位 / 補助情報の参考表記 (将来の national_rank 補足用)
#[allow(dead_code)]
pub(crate) const REFERENCE_NOTE: &str = "参考";

// --------------- データ構築 ---------------

/// Step 1 fetch + Step 2 DTO 変換を統合し、レポート用 `SurveyMarketIntelligenceData` を構築する。
///
/// `target_municipalities` が空のときは全 fetch をスキップして空 DTO を返す。
/// (Phase 3 後続: agg から主要市区町村 TOP N を抽出して渡す予定)
#[allow(dead_code)]
pub(crate) fn build_market_intelligence_data(
    db: &Db,
    turso: Option<&TursoDb>,
    target_municipalities: &[&str],
    occupation_group_code: &str,
    dest_pref: &str,
    dest_muni: &str,
    top_n_inflow: usize,
) -> SurveyMarketIntelligenceData {
    if target_municipalities.is_empty() && (dest_pref.is_empty() || dest_muni.is_empty()) {
        return SurveyMarketIntelligenceData::default();
    }

    let recruiting_rows = fetch_recruiting_scores_by_municipalities(
        db,
        turso,
        target_municipalities,
        occupation_group_code,
    );
    let living_cost_rows = fetch_living_cost_proxy(db, turso, target_municipalities);
    let commute_rows = fetch_commute_flow_summary(db, turso, dest_pref, dest_muni, top_n_inflow);

    // 全市区町村 × 全職業 を取ると重いため、target_municipalities の最初の 1 件のみで取得
    let occupation_rows = if let Some(first_code) = target_municipalities.first() {
        fetch_occupation_population(db, turso, first_code, "resident", &[])
    } else {
        Vec::new()
    };

    // ----- Phase 3 Step 5 Phase 4: Plan B 4 新規 Vec を populate -----
    //
    // target_municipalities が空の場合は早期 return (上の if 条件が dest_pref/dest_muni のみ
    // の経路で来る可能性に対応)。
    let (occupation_cells, ward_thickness, ward_rankings, code_master) = if target_municipalities
        .is_empty()
    {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    } else {
        // (1) 職業セル (workplace + resident 両 basis)
        let occ_cell_rows = fetch_occupation_cells(db, turso, target_municipalities, None, None);
        let occupation_cells = to_occupation_cells(&occ_cell_rows);

        // (2) 政令市区 thickness 詳細
        let thickness_rows = fetch_ward_thickness(db, turso, target_municipalities, None);
        let ward_thickness = to_ward_thickness_dtos(&thickness_rows);

        // (3) コードマスター (target に対して lookup)
        let code_master_rows = fetch_code_master(db, turso, target_municipalities);
        let code_master = to_code_master(&code_master_rows);

        // (4) parent ward ranking (商品の核心、parent_code 別に collect)
        //     designated_ward の parent_code 一覧を抽出 → 主要 occupation で fetch
        let ward_rankings =
            collect_ward_rankings_for_targets(db, turso, &code_master, occupation_group_code);

        (occupation_cells, ward_thickness, ward_rankings, code_master)
    };

    SurveyMarketIntelligenceData {
        recruiting_scores: to_recruiting_scores(&recruiting_rows),
        living_cost_proxies: to_living_cost_proxies(&living_cost_rows),
        commute_flows: to_commute_flows(&commute_rows),
        occupation_populations: to_occupation_populations(&occupation_rows),
        occupation_cells,
        ward_thickness,
        ward_rankings,
        code_master,
    }
}

/// `code_master` から designated_ward を持つ parent_code 一覧を抽出し、
/// 各 parent について `fetch_ward_rankings_by_parent` を呼んで結果を flatten する。
///
/// `occupation_code` が空の場合は代表値 "08_生産工程" を使用 (Plan B 主要職種)。
#[allow(dead_code)]
fn collect_ward_rankings_for_targets(
    db: &Db,
    turso: Option<&TursoDb>,
    code_master: &[MunicipalityCodeMasterDto],
    occupation_code: &str,
) -> Vec<WardRankingRowDto> {
    if code_master.is_empty() {
        return Vec::new();
    }
    // 代表 occupation: 引数優先、空なら "08_生産工程" にフォールバック
    let occ = if occupation_code.is_empty() {
        "08_生産工程"
    } else {
        occupation_code
    };

    // designated_ward を持つ parent_code (重複排除)
    let mut parent_codes: Vec<String> = code_master
        .iter()
        .filter(|m| m.area_type == "designated_ward")
        .filter_map(|m| m.parent_code.clone())
        .collect();
    parent_codes.sort();
    parent_codes.dedup();

    let mut all_rankings: Vec<WardRankingRowDto> = Vec::new();
    for parent_code in &parent_codes {
        let rows = fetch_ward_rankings_by_parent(db, turso, parent_code, occ);
        let mut dtos = to_ward_rankings(&rows);
        all_rankings.append(&mut dtos);
    }
    all_rankings
}

// --------------- 統合エントリ (5 セクション一括 render) ---------------

/// MarketIntelligence variant 専用セクション群を HTML に追加する。
///
/// 呼び出し側 (`render_survey_report_page_with_variant_v3_themed`) で
/// `variant.show_market_intelligence_sections()` ガード後に 1 度だけ呼ぶ。
///
/// データが完全に空の場合は親 wrapper section + placeholder 1 件のみで返す。
pub(crate) fn render_section_market_intelligence(
    html: &mut String,
    data: &SurveyMarketIntelligenceData,
) {
    // Phase 3 Step 5 / Round 2 Worker D: variant guard 内の専用 <style> ブロック。
    // mi-* prefix で完結し default / v8 / v7a の既存テーマと衝突しない。
    // Full / Public variant では本関数自体が呼ばれないため影響なし。
    html.push_str(MI_STYLE_BLOCK);

    html.push_str(
        "<section class=\"mi-root\" data-mi-section=\"market-intelligence\" \
         role=\"region\" aria-labelledby=\"mi-root-heading\" \
         style=\"margin-top:24px;padding:16px;border-top:4px solid #1e3a8a;\">\n",
    );
    html.push_str(
        "<h2 id=\"mi-root-heading\" style=\"color:#1e3a8a;\">\
         採用マーケットインテリジェンス \
         <span style=\"font-size:13px;color:#64748b;font-weight:400;\">(\u{1F4CA} 拡張版)</span></h2>\n"
    );
    html.push_str(
        "<p class=\"mi-disclaimer\" style=\"font-size:12px;color:#64748b;margin:0 0 12px;\">\
         本セクションは Phase 3 採用マーケットインテリジェンス機能 (媒体配信地域提案向け)。\
         指標には <strong>実測 / 推定 / 参考</strong> ラベルを付与する (`docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §1)。\
         相関は提示するが因果関係を断定しない (MEMORY ルール)。</p>\n"
    );

    // P1 C: 印刷向け要約ブロック (免責直後、ヒーローバーより前)
    //   画面では mi-print-only で非表示、印刷時のみ表示。
    render_mi_print_summary(html, data);

    // P0: 配信ヒーローバー (免責直下、KPI カードより前)
    render_mi_hero_bar(html, data);

    // Worker D: 主要指標サマリ KPI カード (セクション冒頭)
    render_mi_kpi_cards(html, data);

    render_mi_summary_card(html, data);
    render_mi_distribution_ranking(html, &data.recruiting_scores);
    // Plan B occupation cells are the primary path.
    // If legacy cells are empty, omit the legacy section instead of exposing fallback details.
    if !data.occupation_populations.is_empty() {
        render_mi_talent_supply(html, &data.occupation_populations);
    }
    render_mi_salary_living_cost(html, &data.recruiting_scores, &data.living_cost_proxies);

    // Worker D: 生活コスト・給与実質感パネル (参考統計、NULL は - 表示)
    render_mi_living_cost_panel(html, &data.living_cost_proxies, &data.recruiting_scores);

    render_mi_scenario_population_range(html, &data.recruiting_scores);
    render_mi_commute_inflow_supplement(html, &data.commute_flows);

    // Round 8 P0-1 (2026-05-09): 旧 render_mi_occupation_cells は raw 行を 60 件 take する
    // 設計で対象自治体・職業・年齢・性別が混在し、PDF 上で対象外自治体 (例: 北海道伊達市) が
    // 大量列挙される表示崩壊を起こしていた。`render_mi_occupation_segment_summary` は対象
    // 自治体ごとに職業 Top 5 を集計し、性別比 + 年齢構成 + 採用示唆を 1〜2 ページの密度で出す。
    // 旧 `render_mi_occupation_cells` は legacy として保持 (既存テスト互換)。
    if !data.occupation_cells.is_empty() {
        render_mi_occupation_segment_summary(html, &data.occupation_cells);
    }

    // Phase 3 Step 5 Phase 4: 政令市区別ランキング (商品の核心)
    render_mi_parent_ward_ranking(html, &data.ward_rankings, &data.code_master);

    // Worker D: レポート末尾の総合注記 (画面専用、機能重複回避のため mi-screen-only で囲う)
    render_mi_footer_notes(html);

    // P1 D: 印刷向け注釈・データ凡例 (印刷専用)
    render_mi_print_annotations(html);

    html.push_str("</section>\n");
}

// --------------- Worker D: 専用 CSS ブロック ---------------
//
// mi-* prefix のみ。既存テーマ (default / v8 / v7a) の class と衝突しない。
// print 時にも視認性を維持: バッジは色 + テキスト併記、KPI grid は block fallback。

const MI_STYLE_BLOCK: &str = r#"<style>
.mi-kpi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin: 8px 0 16px; }
.mi-kpi-card { background: #fff; border: 1px solid #cbd5e1; border-left: 4px solid #1e3a8a; border-radius: 6px; padding: 12px; }
.mi-kpi-card .mi-kpi-label { font-size: 11px; color: #64748b; margin-bottom: 4px; }
.mi-kpi-card .mi-kpi-value { font-size: 22px; font-weight: 700; color: #0f172a; line-height: 1.2; }
.mi-kpi-card .mi-kpi-unit { font-size: 11px; color: #64748b; font-weight: 400; margin-left: 3px; }
.mi-kpi-legend { font-size: 11px; color: #64748b; margin: 0 0 12px; }
.mi-badge { display: inline-block; padding: 1px 6px; border-radius: 3px; font-size: 10px; font-weight: 600; vertical-align: middle; margin: 0 2px; border: 1px solid transparent; }
.mi-badge-measured { background: #dcfce7; color: #166534; border-color: #86efac; }
.mi-badge-estimated-beta { background: #fef9c3; color: #854d0e; border-color: #fde047; }
.mi-badge-estimated-beta::after { content: "β"; font-size: 8px; vertical-align: super; margin-left: 2px; }
.mi-badge-reference { background: #e2e8f0; color: #475569; border-color: #cbd5e1; }
.mi-badge-insufficient { background: #f3f4f6; color: #6b7280; border: 1px dashed #9ca3af; }
.mi-priority-badge { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 11px; font-weight: 700; min-width: 22px; text-align: center; }
.mi-priority-s { background: #7c3aed; color: #fff; }
.mi-priority-a { background: #16a34a; color: #fff; }
.mi-priority-b { background: #eab308; color: #1f2937; }
.mi-priority-c { background: #94a3b8; color: #fff; }
.mi-priority-d { background: #e2e8f0; color: #475569; }
.mi-anchor-badge { display: inline-block; font-size: 11px; color: #b45309; margin-left: 4px; }
.mi-parent-rank { font-size: 16px; font-weight: 700; color: #1e3a8a; }
.mi-parent-rank strong { font-size: 18px; }
.mi-ref { font-size: 10px !important; color: #94a3b8 !important; }
.mi-thickness-bar-wrap { display: inline-block; width: 80px; height: 8px; background: #e2e8f0; border-radius: 2px; vertical-align: middle; margin-left: 6px; overflow: hidden; }
.mi-thickness-bar-fill { display: block; height: 100%; background: linear-gradient(90deg, #60a5fa, #1e3a8a); }
.mi-living-cost-panel { background: #f8fafc; border: 1px solid #cbd5e1; border-radius: 6px; padding: 12px; margin: 16px 0; }
.mi-living-cost-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 10px; margin: 8px 0; }
.mi-living-cost-card { background: #fff; border: 1px solid #e2e8f0; border-radius: 4px; padding: 8px 10px; }
.mi-living-cost-card .mi-lc-label { font-size: 10px; color: #64748b; }
.mi-living-cost-card .mi-lc-value { font-size: 16px; font-weight: 600; color: #0f172a; }
.mi-footer-notes { margin-top: 20px; padding: 12px; background: #f1f5f9; border-left: 4px solid #94a3b8; font-size: 11px; color: #475569; line-height: 1.7; }
.mi-footer-notes ul { margin: 6px 0; padding-left: 20px; }
/* P0: 配信ヒーローバー (mi-* prefix で variant 隔離) */
.mi-hero-bar { margin: 12px 0 16px; }
.mi-hero-grid { display: grid; grid-template-columns: repeat(3, minmax(180px, 1fr)); gap: 12px; }
.mi-hero-card { background: #fff; border: 1px solid #cbd5e1; border-radius: 6px; padding: 14px 14px 12px; box-shadow: 0 1px 2px rgba(15, 23, 42, 0.05); }
.mi-hero-card.mi-hero-primary { border-left: 6px solid #1e3a8a; background: linear-gradient(180deg, #eff6ff, #fff); }
.mi-hero-eyebrow { font-size: 11px; color: #475569; font-weight: 600; letter-spacing: 0.02em; margin-bottom: 4px; }
.mi-hero-value { font-size: 24px; font-weight: 700; color: #0f172a; line-height: 1.2; }
.mi-hero-value strong { font-size: 28px; }
.mi-hero-value span.mi-hero-unit { font-size: 12px; color: #64748b; font-weight: 400; margin-left: 3px; }
.mi-hero-context { font-size: 11px; color: #64748b; margin-top: 6px; }
.mi-visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
/* P0: parent_rank 列幅固定 (主表示 = 市内順位、従表示 = 全国順位) */
.mi-rank-table { table-layout: fixed; }
.mi-rank-table col.mi-col-prank { width: 22%; }
.mi-rank-table col.mi-col-name  { width: 28%; }
.mi-rank-table col.mi-col-thick { width: 22%; }
.mi-rank-table col.mi-col-prio  { width: 12%; }
.mi-rank-table col.mi-col-nrank { width: 16%; }
.mi-rank-table th.mi-col-nrank, .mi-rank-table td.mi-col-nrank { font-size: 10px; color: #94a3b8; font-weight: 400; }
/* P0: 統一 data-label badge (視認性強化) */
.mi-badge { padding: 2px 7px; font-weight: 700; letter-spacing: 0.02em; }
.mi-badge-measured { background: #dcfce7; color: #14532d; border-color: #4ade80; }
.mi-badge-estimated-beta { background: #fef9c3; color: #713f12; border-color: #facc15; }
.mi-badge-reference { background: #e2e8f0; color: #334155; border-color: #94a3b8; }
.mi-badge-insufficient { background: #f3f4f6; color: #4b5563; border: 1px dashed #6b7280; font-weight: 700; }
/* P1 B: 印刷専用 / 画面専用 切替 (デフォルトは画面表示) */
.mi-print-only { display: none; }
.mi-screen-only { display: block; }
/* P1 C: 印刷向け要約ブロック (画面では非表示) */
.mi-print-summary { background: #f8fafc; border: 2px solid #1e3a8a; border-radius: 6px; padding: 14px 16px; margin: 12px 0 16px; font-size: 11pt; color: #0f172a; }
.mi-print-summary h2 { margin: 0 0 8px; color: #1e3a8a; font-size: 14pt; border-bottom: 1px solid #cbd5e1; padding-bottom: 4px; }
.mi-print-summary ul { margin: 6px 0 0; padding-left: 20px; line-height: 1.7; }
/* P1 D: 印刷向け注釈ブロック */
.mi-print-annotations { background: #f1f5f9; border-left: 4px solid #1e3a8a; padding: 10px 14px; margin: 16px 0 8px; font-size: 10pt; color: #334155; }
.mi-print-annotations h3 { margin: 0 0 6px; font-size: 11pt; color: #1e3a8a; }
.mi-print-annotations ul { margin: 4px 0 0; padding-left: 20px; line-height: 1.6; }
.mi-print-annotations li { margin-bottom: 2px; }
@media print {
  /* P1 B: ページ設定 (重複定義は MI_STYLE_BLOCK 内で 1 箇所のみ)
   * 2026-05-06: 上位 style.rs の @page (margin: 10mm 8mm 12mm 8mm) と
   * cascade 競合する場合に備え、MI_STYLE_BLOCK は HTML 末尾側で出力されるため
   * 後勝ちで MI 用余白 (12mm 14mm) を強制する。 */
  @page { size: A4 portrait; margin: 12mm 14mm; }
  /* P1 B (2026-05-06): body padding (8px 16px) と @page margin の二重インデントで
   * 本文幅が縮む問題を回避するため、印刷時は html/body の余白を 0 にして
   * @page margin だけが効くようにする。背景色は全要素で保持する。 */
  html, body {
    margin: 0 !important;
    padding: 0 !important;
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }
  body { font-size: 10.5pt; }
  /* P1 B: 印刷専用 / 画面専用 切替 */
  .mi-print-only { display: block !important; }
  .mi-screen-only { display: none !important; }
  /* P1 B: 全 mi-* 要素で背景色を保持 */
  .mi-root, .mi-root * {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }
  /* P1 B: 見出し直後の改ページ防止 */
  .mi-root h2, .mi-root h3 { break-after: avoid; page-break-after: avoid; }
  /* P1 B: カード類は途中で切断しない */
  .mi-hero-card, .mi-kpi-card, .mi-living-cost-card,
  .mi-print-summary, .mi-print-annotations {
    break-inside: avoid; page-break-inside: avoid;
  }
  .mi-kpi-grid { display: block; }
  .mi-kpi-card { display: block; page-break-inside: avoid; margin-bottom: 8px; }
  .mi-living-cost-grid { display: block; }
  .mi-living-cost-card { display: block; margin-bottom: 6px; page-break-inside: avoid; }
  .mi-badge, .mi-priority-badge { border: 1px solid #475569 !important; }
  .mi-thickness-bar-wrap { border: 1px solid #94a3b8; max-width: 80%; }
  .mi-footer-notes { page-break-inside: avoid; }
  /* P0: 背景色を印刷で保持 */
  .mi-hero-card, .mi-hero-card.mi-hero-primary,
  .mi-kpi-card, .mi-living-cost-card, .mi-living-cost-panel,
  .mi-summary, .mi-priority-badge, .mi-badge,
  .mi-parent-group {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }
  /* P1 B: ランキング行・テーブル行が途中で切断されないよう */
  .mi-rank-table thead { display: table-header-group; }
  .mi-rank-table tr, table.mi-rank-table tr {
    break-inside: avoid; page-break-inside: avoid; page-break-after: auto;
  }
  .mi-rank-table tbody { page-break-inside: auto; }
  /* P1 B: hero は A4 でも 3 枚横並び維持 */
  .mi-hero-grid { display: grid !important; grid-template-columns: repeat(3, 1fr); gap: 6px; }
  /* P0 (2026-05-06): hero bar 自体もページまたぎ防止 */
  .mi-hero-bar { break-inside: avoid; page-break-inside: avoid; }
  .mi-hero-card { break-inside: avoid; page-break-inside: avoid; }
  /* P0: 厚みバーは印刷時に幅縮退 (はみ出し防止) */
  .mi-thickness-bar-fill { max-width: 80%; }
  /* P1 (2026-05-06): 注釈ブロックを単独ページ化させない
   * 監査では page 19 が注釈 5 行 + 余白 95% の単独ページになっていた。
   * break-before: avoid で「前ページに入るなら入れる」挙動に変え、
   * font-size 9.5pt + 行間圧縮で page 18 末尾に統合されやすくする。 */
  .mi-print-annotations {
    break-before: avoid !important;
    page-break-before: avoid !important;
    font-size: 9.5pt;
    margin: 8px 0 4px;
    padding: 6px 10px;
  }
  .mi-print-annotations h3 { font-size: 10pt; margin: 0 0 4px; }
  .mi-print-annotations ul { margin: 2px 0 0; padding-left: 16px; line-height: 1.4; }
  .mi-print-annotations li { margin: 0.1em 0; }
  /* P2 (2026-05-08): page 25 情報密度改善
   * 配信地域ランキング / 給与・生活コスト比較 / シナリオレンジの 3 表が同居して
   * 読みづらくなる問題を解消。
   * - mi-print-block: 表ブロック自体の途中分断を防止
   * - mi-print-break-before: 給与・生活コスト比較を独立ページ化 (page 25 の混在を回避) */
  .mi-print-block {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }
  .mi-print-break-before {
    break-before: page !important;
    page-break-before: always !important;
  }
}
</style>
"#;

// --------------- Worker D: 主要指標サマリ KPI カード ---------------
//
// セクション冒頭に 4 枚の KPI を grid 表示。workplace 実測 / resident β / 参考 が
// 混在するため凡例 (mi-kpi-legend) を必ず付与。数値型 KPI でも `target_count` 等の
// 用語は使わない (Hard NG 維持)。

#[allow(dead_code)]
pub(crate) fn render_mi_kpi_cards(html: &mut String, data: &SurveyMarketIntelligenceData) {
    html.push_str(
        "<section class=\"mi-kpi-summary\" data-mi-section=\"kpi-cards\" \
         aria-labelledby=\"mi-kpi-summary-heading\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-kpi-summary-heading\" style=\"margin:8px 0;\">\u{1F4CA} 主要指標サマリ \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">(MarketIntelligence)</span></h3>\n",
    );

    // 凡例 (3 ラベル混在)
    html.push_str(
        "<p class=\"mi-kpi-legend\">\
         <span class=\"mi-badge mi-badge-measured\">実測</span> 国勢調査 R2 / \
         <span class=\"mi-badge mi-badge-estimated-beta\">推定</span> 検証済み推定 (Model F2) / \
         <span class=\"mi-badge mi-badge-reference\">参考</span> 都道府県家計調査・最低賃金 等</p>\n",
    );

    // 集計値 (Worker E Round 3: 新フィールドへ繋ぎ替え)
    // 「配信優先度 A 件数」: recruiting_scores.distribution_priority == "A" の件数
    //   ward_rankings の priority も後方互換のため fallback で利用 (新スキーマ未投入時)
    let priority_a_from_scores = data
        .recruiting_scores
        .iter()
        .filter(|s| {
            s.distribution_priority
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("A"))
                .unwrap_or(false)
        })
        .count();
    let priority_a_count = if priority_a_from_scores > 0 {
        priority_a_from_scores
    } else {
        // fallback: ward_rankings の priority A/S を集計 (既存挙動温存)
        data.ward_rankings
            .iter()
            .filter(|w| {
                w.priority.eq_ignore_ascii_case("A") || w.priority.eq_ignore_ascii_case("S")
            })
            .count()
    };

    // 「厚み指数 平均」: recruiting_scores.target_thickness_index 平均 (NULL 除外)
    //   旧来の ward_rankings.thickness_index は fallback 用に温存
    let thickness_vals_from_scores: Vec<f64> = data
        .recruiting_scores
        .iter()
        .filter_map(|s| s.target_thickness_index)
        .collect();
    let thickness_avg: Option<f64> = if !thickness_vals_from_scores.is_empty() {
        Some(
            thickness_vals_from_scores.iter().sum::<f64>()
                / thickness_vals_from_scores.len() as f64,
        )
    } else if !data.ward_rankings.is_empty() {
        let sum: f64 = data.ward_rankings.iter().map(|w| w.thickness_index).sum();
        Some(sum / data.ward_rankings.len() as f64)
    } else {
        None
    };

    // 「政令市区 集積地」: recruiting_scores.distribution_priority == "S" 件数
    //   未投入時は code_master の designated_ward 件数で fallback
    let s_priority_count = data
        .recruiting_scores
        .iter()
        .filter(|s| {
            s.distribution_priority
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("S"))
                .unwrap_or(false)
        })
        .count();
    let designated_ward_count = if s_priority_count > 0 {
        s_priority_count
    } else {
        data.code_master
            .iter()
            .filter(|m| m.area_type == "designated_ward")
            .count()
    };

    // 「重点配信候補」: distribution_priority IN ('S','A') 件数
    //   未投入時は distribution_priority_score >= 80 で fallback
    let high_priority_from_grade = data
        .recruiting_scores
        .iter()
        .filter(|s| {
            s.distribution_priority
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("S") || p.eq_ignore_ascii_case("A"))
                .unwrap_or(false)
        })
        .count();
    let high_priority_score_count = if high_priority_from_grade > 0 {
        high_priority_from_grade
    } else {
        data.recruiting_scores
            .iter()
            .filter(|s| s.is_priority_score_in_range())
            .filter(|s| s.distribution_priority_score.unwrap_or(0.0) >= 80.0)
            .count()
    };

    html.push_str("<div class=\"mi-kpi-grid\" role=\"list\">\n");
    render_mi_kpi_card(
        html,
        "配信優先度 A 件数",
        &format!("{priority_a_count}"),
        "件",
        "mi-badge-estimated-beta",
        "推定",
    );
    // P1-1: thickness_avg が None のとき insufficient badge へ切替 (人数化はしない)
    let (thickness_badge_cls, thickness_badge_txt) = if thickness_avg.is_some() {
        ("mi-badge-estimated-beta", ESTIMATED_LABEL)
    } else {
        ("mi-badge-insufficient", INSUFFICIENT_LABEL)
    };
    render_mi_kpi_card(
        html,
        "厚み指数 平均",
        &thickness_avg
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "-".to_string()),
        "(相対)",
        thickness_badge_cls,
        thickness_badge_txt,
    );
    render_mi_kpi_card(
        html,
        "政令市区 集積地",
        &format!("{designated_ward_count}"),
        "区",
        "mi-badge-measured",
        "実測",
    );
    // P0 (2026-05-06): ヒーロー Card 1「重点配信候補 (S+A)」と同名ラベルだったが
    // 計算定義が異なる (S/A 件数 vs スコア 80+ 件数) ため、ラベルを別名にして
    // 同ページ内の数値矛盾を解消する。
    render_mi_kpi_card(
        html,
        "配信検証候補",
        &format!("{high_priority_score_count}"),
        "件 (スコア80+)",
        "mi-badge-estimated-beta",
        "推定",
    );
    html.push_str("</div>\n");
    // 2026-05-08 Round 2-2: 「重点配信候補 (S+A)」と「配信検証候補 (スコア80+)」が
    // 同じヒーロー / KPI 内に出るため、定義の違いを 1 行で明示する。
    // 数値矛盾 (Round 1-K で 0 件 vs 11 件) の根本対策として、説明文を必ず併記する。
    html.push_str(&format!(
        "<p class=\"mi-priority-distinction-note\" \
         style=\"font-size:9pt;color:#475569;margin:8px 0 0;border-left:3px solid #cbd5e1;padding-left:8px;\">\
         {note}</p>\n",
        note = escape_html(super::labels::distribution_candidates::DISTINCTION_NOTE),
    ));
    html.push_str("</section>\n");
}

fn render_mi_kpi_card(
    html: &mut String,
    label: &str,
    value: &str,
    unit: &str,
    badge_class: &str,
    badge_text: &str,
) {
    html.push_str(&format!(
        "<div class=\"mi-kpi-card\" role=\"listitem\">\
         <div class=\"mi-kpi-label\">{label} <span class=\"mi-badge {badge_cls}\">{badge_txt}</span></div>\
         <div class=\"mi-kpi-value\">{value}<span class=\"mi-kpi-unit\">{unit}</span></div>\
         </div>\n",
        label = escape_html(label),
        badge_cls = badge_class,
        badge_txt = escape_html(badge_text),
        value = escape_html(value),
        unit = escape_html(unit),
    ));
}

// --------------- P0/P1-1: 統一 data-label badge ---------------
//
// 「実測 / 推定 β / 参考 / データ不足」ラベルを共通の見た目で出す。kind:
//   - "measured"       → mi-badge-measured (実測)
//   - "estimated_beta" → mi-badge-estimated-beta (推定)
//   - "reference"      → mi-badge-reference (参考)
//   - "insufficient"   → mi-badge-insufficient (データ不足) — 数値が存在しないことの視覚化
// 不明な kind は空文字列 (バッジ非表示)。resident estimated_beta の人数化禁止は維持。
#[allow(dead_code)]
pub(crate) fn render_mi_data_label_badge(kind: &str) -> String {
    let (cls, text) = match kind {
        "measured" => ("mi-badge-measured", MEASURED_LABEL),
        "estimated_beta" => ("mi-badge-estimated-beta", ESTIMATED_LABEL),
        "reference" => ("mi-badge-reference", REFERENCE_LABEL),
        "insufficient" => ("mi-badge-insufficient", INSUFFICIENT_LABEL),
        _ => return String::new(),
    };
    format!(
        "<span class=\"mi-badge {cls}\">{text}</span>",
        cls = cls,
        text = escape_html(text),
    )
}

// --------------- P0: 配信ヒーローバー ---------------
//
// 免責直下に 3 枚のカードを横一列で表示し、ファーストビューでの判断材料を提示する。
//   1. 重点配信候補 (S+A 件数) — 推定 β
//   2. 市内 1 位市区 (先頭政令市) — 実測 (parent_rank ベース)
//   3. 職業人口 (実測 R2 合計) — workplace_measured 限定
//
// 第 3 枠の fallback ルール (Hard NG 厳守):
//   workplace_measured データが無い場合は「実測値準備中」+ 厚み指数 平均 を
//   指数として表示する。estimated_beta の人数化は絶対に行わない。
#[allow(dead_code)]
pub(crate) fn render_mi_hero_bar(html: &mut String, data: &SurveyMarketIntelligenceData) {
    // 1) 重点配信候補 (S + A 件数)
    let high_priority_count = data
        .recruiting_scores
        .iter()
        .filter(|s| {
            s.distribution_priority
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("S") || p.eq_ignore_ascii_case("A"))
                .unwrap_or(false)
        })
        .count();
    let high_priority_fallback = if high_priority_count == 0 {
        data.ward_rankings
            .iter()
            .filter(|w| {
                w.priority.eq_ignore_ascii_case("S") || w.priority.eq_ignore_ascii_case("A")
            })
            .count()
    } else {
        high_priority_count
    };

    // 2) 市内 1 位市区 (先頭政令市の parent_rank == 1)
    //    parent_code でグループ化したうち先頭の parent_rank=1 を選ぶ
    let mut first_top_ward: Option<&WardRankingRowDto> = None;
    let mut parents_seen: BTreeMap<String, usize> = BTreeMap::new();
    for r in &data.ward_rankings {
        *parents_seen.entry(r.parent_code.clone()).or_insert(0) += 1;
        if r.parent_rank == 1 && first_top_ward.is_none() {
            first_top_ward = Some(r);
        }
    }

    // 3) 職業人口 (workplace × measured の population 合計、measured 限定)
    let workplace_measured_sum: i64 = data
        .occupation_cells
        .iter()
        .filter(|c| c.basis == "workplace" && c.data_label == "measured")
        .filter_map(|c| c.population)
        .sum();
    let has_workplace_measured = data
        .occupation_cells
        .iter()
        .any(|c| c.basis == "workplace" && c.data_label == "measured" && c.population.is_some());

    html.push_str(
        "<section class=\"mi-hero-bar\" data-mi-section=\"hero-bar\" \
         role=\"region\" aria-labelledby=\"mi-hero-heading\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-hero-heading\" class=\"mi-visually-hidden\">配信判断 ヒーロー</h3>\n",
    );
    html.push_str("<div class=\"mi-hero-grid\" role=\"list\">\n");

    // Card 1: 重点配信候補
    html.push_str(&format!(
        "<div class=\"mi-hero-card mi-hero-primary\" role=\"listitem\">\
         <div class=\"mi-hero-eyebrow\">重点配信候補 (S + A)</div>\
         <div class=\"mi-hero-value\"><strong>{n}</strong><span class=\"mi-hero-unit\">件</span></div>\
         <div class=\"mi-hero-context\">{badge} Model F2</div>\
         </div>\n",
        n = high_priority_fallback,
        badge = render_mi_data_label_badge("estimated_beta"),
    ));

    // Card 2: 市内 1 位市区
    // P0 (2026-05-06): 値なし時に「データ準備中」「-」を出していたため、
    // mi-badge-insufficient (「該当なし」) で統一。値表示部も「該当なし」のみ。
    match first_top_ward {
        Some(w) => {
            html.push_str(&format!(
                "<div class=\"mi-hero-card\" role=\"listitem\">\
                 <div class=\"mi-hero-eyebrow\">市内 1 位市区 (先頭政令市)</div>\
                 <div class=\"mi-hero-value\">{label}</div>\
                 <div class=\"mi-hero-context\">{ctx}</div>\
                 </div>\n",
                label = escape_html(&w.municipality_name),
                ctx = escape_html(&format!("市内順位 1 / {} 区", w.parent_total)),
            ));
        }
        None => {
            html.push_str(
                "<div class=\"mi-hero-card\" role=\"listitem\">\
                 <div class=\"mi-hero-eyebrow\">\u{5E02}\u{5185} 1 \u{4F4D}\u{5E02}\u{533A} (\u{5148}\u{982D}\u{653F}\u{4EE4}\u{5E02})</div>\
                 <div class=\"mi-hero-value\">\u{8A72}\u{5F53}\u{306A}\u{3057}</div>\
                 <div class=\"mi-hero-context\">\u{653F}\u{4EE4}\u{5E02}\u{533A}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{5BFE}\u{8C61}\u{5916}</div>\
                 </div>\n",
            );
        }
    }

    // Card 3: 職業人口 (workplace × measured 限定) または fallback (厚み指数 平均)
    if has_workplace_measured {
        html.push_str(&format!(
            "<div class=\"mi-hero-card\" role=\"listitem\">\
             <div class=\"mi-hero-eyebrow\">職業人口 (実測 R2 合計)</div>\
             <div class=\"mi-hero-value\"><strong>{val}</strong><span class=\"mi-hero-unit\">人</span></div>\
             <div class=\"mi-hero-context\">{badge} 国勢調査</div>\
             </div>\n",
            val = format_thousands(workplace_measured_sum),
            badge = render_mi_data_label_badge("measured"),
        ));
    } else {
        // Fallback: workplace measured が無い場合は厚み指数 平均を指数表示。
        // resident estimated_beta を人数として絶対に出さない (Hard NG 厳守)。
        let thickness_vals: Vec<f64> = data
            .recruiting_scores
            .iter()
            .filter_map(|s| s.target_thickness_index)
            .chain(data.ward_rankings.iter().map(|w| w.thickness_index))
            .collect();
        let thick_avg = if thickness_vals.is_empty() {
            None
        } else {
            Some(thickness_vals.iter().sum::<f64>() / thickness_vals.len() as f64)
        };
        let val_str = thick_avg
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".to_string());
        let _ = parents_seen; // suppress unused warning if not needed elsewhere
        html.push_str(&format!(
            "<div class=\"mi-hero-card\" role=\"listitem\">\
             <div class=\"mi-hero-eyebrow\">\u{539A}\u{307F}\u{6307}\u{6570} \u{5E73}\u{5747}</div>\
             <div class=\"mi-hero-value\">{val}<span class=\"mi-hero-unit\">(指数)</span></div>\
             <div class=\"mi-hero-context\">{badge} 検証済み推定 β に基づく相対指標</div>\
             </div>\n",
            val = escape_html(&val_str),
            badge = render_mi_data_label_badge("estimated_beta"),
        ));
    }

    html.push_str("</div>\n");
    html.push_str("</section>\n");
}

// --------------- Worker D: 生活コスト・給与実質感パネル ---------------
//
// 都道府県家計調査 + 都道府県最低賃金の参考値。NULL は「-」表示 (ゼロ埋め禁止)。
// 「給与実質感 proxy」は median_salary_yen / retail_price_index_proxy の比率を相対化した
// 表示用 proxy。市区町村実態を保証しない旨をフッターで明示。

#[allow(dead_code)]
pub(crate) fn render_mi_living_cost_panel(
    html: &mut String,
    living: &[LivingCostProxy],
    scores: &[MunicipalityRecruitingScore],
) {
    html.push_str(
        "<section class=\"mi-living-cost-panel\" data-mi-section=\"living-cost-panel\" \
         aria-labelledby=\"mi-lc-panel-heading\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-lc-panel-heading\" style=\"margin:0 0 8px;\">\u{1F4B0} 生活コスト・給与実質感 \
         <span class=\"mi-badge mi-badge-reference\">参考</span></h3>\n",
    );

    // 代表値 (都道府県集約の発想)。ここではデータ平均で代表化。
    // Worker E Round 3: Worker A 投入版の cost_index フィールドを使用 (旧 retail_price_index_proxy は fetch SQL から外れて常に None)
    let cost_index_avg: Option<f64> = {
        let vals: Vec<f64> = living
            .iter()
            .filter_map(|l| l.cost_index.or(l.retail_price_index_proxy))
            .collect();
        if vals.is_empty() {
            None
        } else {
            Some(vals.iter().sum::<f64>() / vals.len() as f64)
        }
    };
    // Worker E Round 3: 最低賃金は LivingCostProxy.min_wage (Worker A 投入版) から平均算出
    let min_wage_yen: Option<i64> = {
        let vals: Vec<i64> = living.iter().filter_map(|l| l.min_wage).collect();
        if vals.is_empty() {
            None
        } else {
            Some(vals.iter().sum::<i64>() / vals.len() as i64)
        }
    };
    // Worker E Round 3: 給与実質感 proxy は LivingCostProxy.salary_real_terms_proxy (Worker A 版) を優先
    //   未収録時は recruiting_scores.salary_living_score を 100 基準で正規化 (新フィールド)
    //   それも無ければ NULL ("-")
    let salary_real_proxy: Option<f64> = {
        let direct: Vec<f64> = living
            .iter()
            .filter_map(|l| l.salary_real_terms_proxy)
            .collect();
        if !direct.is_empty() {
            Some(direct.iter().sum::<f64>() / direct.len() as f64)
        } else {
            // fallback: salary_living_score (0-100 指数) を 1.0 基準に正規化
            let salary_scores: Vec<f64> = scores
                .iter()
                .filter_map(|s| s.salary_living_score)
                .collect();
            if !salary_scores.is_empty() {
                let avg = salary_scores.iter().sum::<f64>() / salary_scores.len() as f64;
                Some(avg / 100.0)
            } else {
                None
            }
        }
    };

    html.push_str("<div class=\"mi-living-cost-grid\">\n");

    // P1-1: 値が None のカードには mi-badge-insufficient を付与
    render_mi_lc_card_with_badge(
        html,
        "都道府県 cost_index",
        &cost_index_avg
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".to_string()),
        if cost_index_avg.is_none() {
            "insufficient"
        } else {
            ""
        },
    );
    render_mi_lc_card_with_badge(
        html,
        "最低賃金 (時給)",
        &min_wage_yen
            .map(|v| format!("{} 円", format_thousands(v)))
            .unwrap_or_else(|| "-".to_string()),
        if min_wage_yen.is_none() {
            "insufficient"
        } else {
            ""
        },
    );
    render_mi_lc_card_with_badge(
        html,
        "給与実質感 proxy",
        &salary_real_proxy
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string()),
        if salary_real_proxy.is_none() {
            "insufficient"
        } else {
            ""
        },
    );
    // 市区町村差分は現状未算出のため常に insufficient
    render_mi_lc_card_with_badge(html, "市区町村差分", "-", "insufficient");

    html.push_str("</div>\n");
    html.push_str(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         \u{203B} 都道府県家計調査 + 都道府県最低賃金の参考値です。\
         市区町村ごとの実態を保証するものではありません。NULL は「-」で表示。</p>\n",
    );
    html.push_str("</section>\n");
}

#[allow(dead_code)]
fn render_mi_lc_card(html: &mut String, label: &str, value: &str) {
    render_mi_lc_card_with_badge(html, label, value, "");
}

// P1-1: badge_kind が空でなければ render_mi_data_label_badge で badge を付与
fn render_mi_lc_card_with_badge(html: &mut String, label: &str, value: &str, badge_kind: &str) {
    let badge_html = if badge_kind.is_empty() {
        String::new()
    } else {
        format!(" {}", render_mi_data_label_badge(badge_kind))
    };
    html.push_str(&format!(
        "<div class=\"mi-living-cost-card\">\
         <div class=\"mi-lc-label\">{label}{badge}</div>\
         <div class=\"mi-lc-value\">{value}</div>\
         </div>\n",
        label = escape_html(label),
        badge = badge_html,
        value = escape_html(value),
    ));
}

// --------------- Worker D: 末尾総合注記 ---------------

fn render_mi_footer_notes(html: &mut String) {
    // 画面専用: 印刷時は render_mi_print_annotations 側に等価情報を出すため重複回避。
    html.push_str(
        "<aside class=\"mi-footer-notes mi-screen-only\" role=\"note\" aria-label=\"表示についての注意書き\">\n\
         <strong>\u{26A0} 表示について</strong>\n\
         <ul>\n\
         <li><span class=\"mi-badge mi-badge-measured\">実測</span> \
         国勢調査 R2 (令和 2 年・2020 年) の従業地ベース実測値</li>\n\
         <li><span class=\"mi-badge mi-badge-estimated-beta\">推定</span> \
         独自モデル F2 (estimate_grade A-) による相対指標。\
         weight_source = hypothesis_v1 (e-Stat 実測値置換予定)</li>\n\
         <li><span class=\"mi-badge mi-badge-reference\">参考</span> \
         都道府県家計調査 / 最低賃金などの公開統計値。市区町村差を完全には反映していません。</li>\n\
         <li>数値は採用ターゲット候補の相対濃淡を示すもので、実数の保証ではありません。</li>\n\
         <li>全国順位は参考表示、商品判断は親市内ランキング (parent_rank) を主軸としてご利用ください。</li>\n\
         </ul>\n\
         </aside>\n",
    );
}

// --------------- P1 C: 印刷向け要約ブロック (結論 → 採用示唆) ---------------
//
// 印刷/PDF 出力時にファーストページで「採用コンサルが何を言いたいか」を伝える。
// - 配信優先度 S/A の件数 → 「配信優先度が高い地域です」
// - 厚み指数 平均 → 厚み傾向 (相対値であることを明示)
// - 市内 1 位の件数 → parent_rank ベースの示唆
// - 推定 β は指数である旨を明記 (常住地ベースの厚み)
// - 生活コスト/配信スコアは参考指標である旨
//
// Hard NG (人数化禁止) を厳守: 「候補者が○人」「推定人数」「想定人数」「母集団人数」は出さない。
// `mi-print-only` class により画面では非表示、印刷時のみ display: block。
#[allow(dead_code)]
pub(crate) fn render_mi_print_summary(html: &mut String, data: &SurveyMarketIntelligenceData) {
    // 集計値: 配信優先度 S/A 件数
    let priority_sa = data
        .recruiting_scores
        .iter()
        .filter(|s| {
            s.distribution_priority
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case("S") || p.eq_ignore_ascii_case("A"))
                .unwrap_or(false)
        })
        .count();

    // 集計値: 市内順位 1 位の件数 (parent_rank == 1)
    let parent_top_count = data
        .ward_rankings
        .iter()
        .filter(|w| w.parent_rank == 1)
        .count();

    // 集計値: 厚み指数 平均 (NULL 除外)
    let thickness_vals: Vec<f64> = data
        .recruiting_scores
        .iter()
        .filter_map(|s| s.target_thickness_index)
        .collect();
    let thickness_avg: Option<f64> = if thickness_vals.is_empty() {
        None
    } else {
        Some(thickness_vals.iter().sum::<f64>() / thickness_vals.len() as f64)
    };

    html.push_str(
        "<section class=\"mi-print-summary mi-print-only\" data-mi-section=\"print-summary\" \
         aria-label=\"採用コンサルレポート要約\">\n",
    );
    html.push_str("<h2>結論と採用示唆</h2>\n");
    html.push_str("<ul>\n");

    // 配信優先度の結論
    // P0 (2026-05-06): S/A 0 件のとき「データ不足のため特定できませんでした (要件再確認)」
    // という内部 fallback 文言を出力していたため、配信地域ランキング案内に変更。
    if priority_sa > 0 {
        html.push_str(&format!(
            "<li>配信優先度 S/A 該当が <strong>{}</strong> 件。配信優先度が高い地域です。</li>\n",
            priority_sa
        ));
    } else {
        html.push_str(
            "<li>配信優先度 S/A 該当はありません (該当なし)。\
             下表の配信地域ランキングと厚み指数を確認してください。</li>\n",
        );
    }

    // 厚み傾向の結論
    // P0 (2026-05-06): None 時は「データ不足のため算出できませんでした」という
    // 内部 fallback 文言を出していたため、推定 β 指数の設計説明のみに変更。
    match thickness_avg {
        Some(v) => {
            html.push_str(&format!(
                "<li>厚み指数の平均は <strong>{:.0}</strong> (相対指標)。\
                 常住地ベースの厚みは推定 β 指数で表示しています。</li>\n",
                v
            ));
        }
        None => {
            html.push_str(
                "<li>厚み指数は該当なし。\
                 常住地ベースの厚みは推定 β 指数で表示する設計です。</li>\n",
            );
        }
    }

    // 市内順位の示唆
    if parent_top_count > 0 {
        html.push_str(&format!(
            "<li>市内順位 1 位 (parent_rank=1) は <strong>{}</strong> 区。\
             商品判断は市内順位 (parent_rank) を主軸としてご利用ください。</li>\n",
            parent_top_count
        ));
    } else {
        html.push_str("<li>市内順位 (parent_rank) を主軸として地域選定をご検討ください。</li>\n");
    }

    // 参考指標の注意喚起
    html.push_str(
        "<li>生活コスト・最低賃金・配信スコアは参考指標です \
         (市区町村差を完全には反映していません)。</li>\n",
    );
    // 全体の前提
    html.push_str("<li>本レポートの数値は相対濃淡を示すもので、実数の保証ではありません。</li>\n");

    html.push_str("</ul>\n");
    html.push_str("</section>\n");
}

// --------------- P1 D: 印刷向け注釈・データ凡例 ---------------
//
// 印刷時のみ表示する固定の凡例ブロック。読者が紙面でも 5 つの主要ラベルの意味を確認できる。
// 機能重複回避のため、画面用の `render_mi_footer_notes` は `mi-screen-only` で出し、
// こちらは `mi-print-only` で出す (両者は印刷/画面で排他的に表示される)。
#[allow(dead_code)]
pub(crate) fn render_mi_print_annotations(html: &mut String) {
    html.push_str(
        "<aside class=\"mi-print-annotations mi-print-only\" data-mi-section=\"print-annotations\" \
         aria-label=\"データ凡例\">\n",
    );
    html.push_str("<h3>データ凡例 / 注釈</h3>\n");
    html.push_str("<ul>\n");
    html.push_str(
        "<li><strong>workplace measured</strong>: 従業地ベースの実測値 (国勢調査 R2 / 令和 2 年)</li>\n",
    );
    html.push_str(
        "<li><strong>resident estimated_beta</strong>: 常住地ベースの推定 β 指数 \
         (人数ではありません / 相対指標)</li>\n",
    );
    html.push_str("<li><strong>national_rank</strong>: 全国順位は参考表示</li>\n");
    html.push_str(
        "<li><strong>parent_rank</strong>: 市内順位を主軸として商品判断にご利用ください</li>\n",
    );
    html.push_str(
        "<li><strong>生活コスト・最低賃金</strong>: 参考指標 \
         (市区町村差を完全には反映していません)</li>\n",
    );
    html.push_str("</ul>\n");
    html.push_str("</aside>\n");
}

// --------------- Section 7-NEW (Round 8 P0-1): 職業 × 性別 × 年齢 セグメントサマリ ---------------
//
// 設計目的:
// - 対象自治体ごとに職業 Top 5 を集計し、性別比 + 年齢構成 (3 区分) + 採用示唆を出す
// - workplace basis × measured (population が Some) のみを使う
// - resident は estimated_beta で全自治体一律 200.0 のため母集団推定に使えない (本セクション対象外)
// - age='_total' / gender='total' は workplace basis に存在しないため、5歳刻み × M/F を Rust で集計
// - 1 行 = (自治体, 職業) で、1 自治体あたり Top 5 → 4 自治体 = 20 行 → 1〜2 ページ密度

const OCC_SEG_AGE_YOUNG: &[&str] = &["15-19", "20-24", "25-29"];
const OCC_SEG_AGE_MID: &[&str] = &["30-34", "35-39", "40-44", "45-49"];
const OCC_SEG_AGE_SENIOR: &[&str] = &[
    "50-54", "55-59", "60-64", "65-69", "70-74", "75-79", "80-84", "85-89", "90-94", "95+",
];

#[derive(Debug, Clone, Default)]
struct OccSegmentRow {
    municipality_code: String,
    prefecture: String,
    municipality_name: String,
    #[allow(dead_code)]
    occupation_code: String,
    occupation_name: String,
    total: i64,
    female: i64,
    young: i64,  // 15-29
    mid: i64,    // 30-49
    senior: i64, // 50+
}

/// 職業 × 性別 × 年齢 集計。`workplace basis` × `measured` (= population が Some) のみ採用。
/// age='_total' / gender='total' は workplace basis では存在しない (Local DB 確認済 2026-05-09)
/// ため、5歳刻み × M/F の raw row を Rust で合計する。同じ単位 (人数) しか扱わない設計で
/// 二重計上は発生しない。
fn aggregate_occupation_segments(cells: &[OccupationCellDto]) -> Vec<OccSegmentRow> {
    use std::collections::BTreeMap;

    // key: (muni_code, occupation_code) → 集計
    let mut acc: BTreeMap<(String, String), OccSegmentRow> = BTreeMap::new();

    for c in cells {
        if c.basis != "workplace" {
            continue;
        }
        if c.data_label != "measured" {
            continue;
        }
        let pop = match c.population {
            Some(p) if p > 0 => p,
            _ => continue,
        };
        // workplace basis は age='_total' / gender='total' を持たない前提だが、
        // 防御的に弾く (将来データ更新時の二重計上防止)。
        if c.age_class == "_total" || c.gender == "total" {
            continue;
        }
        if c.gender != "male" && c.gender != "female" {
            continue;
        }

        let key = (c.municipality_code.clone(), c.occupation_code.clone());
        let entry = acc.entry(key).or_insert_with(|| OccSegmentRow {
            municipality_code: c.municipality_code.clone(),
            prefecture: c.prefecture.clone(),
            municipality_name: c.municipality_name.clone(),
            occupation_code: c.occupation_code.clone(),
            occupation_name: c.occupation_name.clone(),
            ..Default::default()
        });

        entry.total += pop;
        if c.gender == "female" {
            entry.female += pop;
        }
        if OCC_SEG_AGE_YOUNG.iter().any(|a| *a == c.age_class) {
            entry.young += pop;
        } else if OCC_SEG_AGE_MID.iter().any(|a| *a == c.age_class) {
            entry.mid += pop;
        } else if OCC_SEG_AGE_SENIOR.iter().any(|a| *a == c.age_class) {
            entry.senior += pop;
        }
    }

    acc.into_values().filter(|r| r.total > 0).collect()
}

/// セグメント特徴から採用示唆を機械生成する。判定はすべて比率ベースで、根拠は
/// (女性比, 若年比, 中堅比, シニア比) のみ。閾値は本実装で固定する。
fn occupation_segment_insight(female_pct: f64, young_pct: f64, mid_pct: f64, senior_pct: f64) -> String {
    let mut tags: Vec<&str> = Vec::new();
    if female_pct >= 60.0 {
        tags.push("女性中心");
    } else if female_pct <= 30.0 {
        tags.push("男性中心");
    }
    if senior_pct >= 40.0 {
        tags.push("中高年厚め");
    }
    if young_pct >= 30.0 {
        tags.push("若年層厚め");
    }
    if mid_pct >= 50.0 {
        tags.push("30〜49 が主軸");
    }
    if tags.is_empty() {
        tags.push("性別・年齢に大きな偏りなし");
    }
    tags.join(" / ")
}

/// Round 8 P0-1: 職業 × 性別 × 年齢 セグメントサマリ。
/// - 対象自治体のみ (cells に既に target_municipalities フィルタ済が渡される前提)
/// - 自治体ごとに total 降順で職業 Top 5
/// - 1〜2 ページの密度を目標にしているため Top 5 で打ち切る
pub(crate) fn render_mi_occupation_segment_summary(
    html: &mut String,
    cells: &[OccupationCellDto],
) {
    html.push_str(
        "<section class=\"mi-occupation-segment\" data-mi-section=\"occupation-segment\" \
         aria-labelledby=\"mi-occseg-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-occseg-heading\">対象自治体 × 職業 × 性別 × 年齢 セグメント \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[商品コア / 国勢調査 R2]</span></h3>\n",
    );
    html.push_str(
        "<p class=\"mi-note\" style=\"font-size:11px;color:#64748b;margin:0 0 8px;\">\
         従業地ベース (実測 / 国勢調査 R2 / population 行)。各自治体について就業者数の多い職業 Top 5 を表示。\
         女性比・年齢構成は当該自治体・当該職業の従業者母集団から算出 (求人 CSV と独立)。\
         採用示唆は機械的に生成しています (女性比 ≥ 60% → 女性中心、50 歳以上比 ≥ 40% → 中高年厚め 等)。</p>\n",
    );

    let mut rows = aggregate_occupation_segments(cells);
    if rows.is_empty() {
        html.push_str(
            "<p class=\"mi-note\" style=\"font-size:11px;color:#64748b;\">\
             従業地ベース (workplace × measured) の population 行が対象自治体に存在しないため、\
             職業 × 性別 × 年齢 のセグメント表示は省略します。</p>\n",
        );
        html.push_str("</section>\n");
        return;
    }

    // 自治体ごとに total 降順で Top 5
    rows.sort_by(|a, b| {
        a.municipality_code
            .cmp(&b.municipality_code)
            .then(b.total.cmp(&a.total))
    });

    let mut by_muni: BTreeMap<String, Vec<OccSegmentRow>> = BTreeMap::new();
    for r in rows {
        by_muni.entry(r.municipality_code.clone()).or_default().push(r);
    }

    for (_muni_code, mut occs) in by_muni {
        // 念のため total 降順を再担保
        occs.sort_by(|a, b| b.total.cmp(&a.total));
        occs.truncate(5);
        if occs.is_empty() {
            continue;
        }
        let pref = occs[0].prefecture.clone();
        let name = occs[0].municipality_name.clone();

        html.push_str(&format!(
            "  <div class=\"mi-occseg-block\" style=\"margin:10px 0;page-break-inside:avoid;\">\n\
                <h4 style=\"margin:0 0 4px;color:#1e3a8a;font-size:12px;\">{pref} {name}</h4>\n",
            pref = escape_html(&pref),
            name = escape_html(&name),
        ));

        html.push_str(
            "    <table class=\"mi-occseg-table\" \
             style=\"width:100%;border-collapse:collapse;font-size:11px;\">\n\
             <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
             <th style=\"text-align:left;padding:4px 6px;\">職業</th>\
             <th style=\"text-align:right;padding:4px 6px;\">就業者</th>\
             <th style=\"text-align:right;padding:4px 6px;\">女性比</th>\
             <th style=\"text-align:right;padding:4px 6px;\">〜29</th>\
             <th style=\"text-align:right;padding:4px 6px;\">30-49</th>\
             <th style=\"text-align:right;padding:4px 6px;\">50〜</th>\
             <th style=\"text-align:left;padding:4px 6px;\">採用示唆</th>\
             </tr></thead><tbody>\n",
        );
        for r in occs {
            let total_f = r.total as f64;
            let female_pct = if total_f > 0.0 {
                100.0 * (r.female as f64) / total_f
            } else {
                0.0
            };
            let young_pct = if total_f > 0.0 {
                100.0 * (r.young as f64) / total_f
            } else {
                0.0
            };
            let mid_pct = if total_f > 0.0 {
                100.0 * (r.mid as f64) / total_f
            } else {
                0.0
            };
            let senior_pct = if total_f > 0.0 {
                100.0 * (r.senior as f64) / total_f
            } else {
                0.0
            };
            let insight = occupation_segment_insight(female_pct, young_pct, mid_pct, senior_pct);

            html.push_str(&format!(
                "<tr>\
                 <td style=\"padding:3px 6px;\">{occ}</td>\
                 <td style=\"text-align:right;padding:3px 6px;\">{tot} 人</td>\
                 <td style=\"text-align:right;padding:3px 6px;\">{f:.0}%</td>\
                 <td style=\"text-align:right;padding:3px 6px;\">{y:.0}%</td>\
                 <td style=\"text-align:right;padding:3px 6px;\">{m:.0}%</td>\
                 <td style=\"text-align:right;padding:3px 6px;\">{s:.0}%</td>\
                 <td style=\"padding:3px 6px;color:#1e3a8a;font-size:10px;\">{ins}</td>\
                 </tr>\n",
                occ = escape_html(&r.occupation_name),
                tot = format_thousands(r.total),
                f = female_pct,
                y = young_pct,
                m = mid_pct,
                s = senior_pct,
                ins = escape_html(&insight),
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str("  </div>\n");
    }

    html.push_str("</section>\n");
}

// --------------- Section 7-LEGACY: Plan B (workplace measured + resident estimated_beta) ---------------
//
// 表示分岐ルール (DISPLAY_SPEC_PLAN_B 必須):
// - workplace × measured: 人数 (population) + WORKPLACE_LABEL + MEASURED_DATA_SOURCE
// - resident × estimated_beta: 指数 (estimate_index, ".1f") + RESIDENT_LABEL + ESTIMATED_BETA_NOTE
// - resident × estimated_beta で人数を絶対に表示しない (Hard NG)
//
// Round 8 P0-1 (2026-05-09) で本セクションは call site から外された
// (`render_mi_occupation_segment_summary` に置換)。関数定義は legacy として残す
// (既存テスト互換、direct テスト 3 件: line 2207/2226/2541 が継続使用)。

#[allow(dead_code)]
pub(crate) fn render_mi_occupation_cells(html: &mut String, cells: &[OccupationCellDto]) {
    html.push_str(
        "<section class=\"mi-occupation-cells\" data-mi-section=\"occupation-cells\" \
         aria-labelledby=\"mi-occcell-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-occcell-heading\">職業×地域 セル別マトリクス \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[Plan B]</span></h3>\n",
    );

    if cells.is_empty() {
        render_mi_placeholder(
            html,
            "\u{8A72}\u{5F53}\u{306A}\u{3057}\u{3067}\u{3059}\u{3002}\u{4E0A}\u{90E8}\u{30B5}\u{30DE}\u{30EA}\u{30FC}\u{3068}\u{914D}\u{4FE1}\u{5730}\u{57DF}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{3092}\u{78BA}\u{8A8D}\u{3057}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}",
        );
        html.push_str("</section>\n");
        return;
    }

    html.push_str(
        "<table class=\"mi-occcell-table\" style=\"width:100%;border-collapse:collapse;font-size:12px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:left;padding:6px;\">職業</th>\
         <th style=\"text-align:left;padding:6px;\">区分 (basis)</th>\
         <th style=\"text-align:right;padding:6px;\">値</th>\
         <th style=\"text-align:left;padding:6px;\">出典</th>\
         </tr></thead><tbody>\n",
    );
    for c in cells.iter().take(60) {
        // XOR 不変条件: data_label に応じて population / estimate_index を排他表示
        let (basis_label, value_html, source_html) = match c.data_label.as_str() {
            "measured" => {
                // 主に workplace × measured (将来的に resident × measured も同経路)
                let label = if c.basis == "workplace" {
                    WORKPLACE_LABEL
                } else {
                    "常住地ベース (実測)"
                };
                let val = c
                    .population
                    .map(|v| format!("{} 人", format_thousands(v)))
                    .unwrap_or_else(|| "-".to_string());
                (label, val, MEASURED_DATA_SOURCE)
            }
            "estimated_beta" => {
                let label = if c.basis == "resident" {
                    RESIDENT_LABEL
                } else {
                    "従業地ベース (推定 β)"
                };
                // resident × estimated_beta は指数のみ。人数表示は絶対 NG
                let val = c
                    .estimate_index
                    .map(|v| format!("指数 {:.1}", v))
                    .unwrap_or_else(|| "-".to_string());
                (label, val, ESTIMATED_BETA_NOTE)
            }
            _ => ("区分不明", "-".to_string(), "-"),
        };

        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"padding:4px;\">{occ}</td>\
             <td style=\"padding:4px;\">{basis}</td>\
             <td style=\"text-align:right;padding:4px;\">{val}</td>\
             <td style=\"padding:4px;color:#64748b;font-size:11px;\">{src}</td></tr>\n",
            pref = escape_html(&c.prefecture),
            muni = escape_html(&c.municipality_name),
            occ = escape_html(&c.occupation_name),
            basis = escape_html(basis_label),
            val = escape_html(&value_html),
            src = escape_html(source_html),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         {WORKPLACE_LABEL} 行は人数で表示、{RESIDENT_LABEL} 行は指数 (相対値) のみ表示。<br/>\
         指数は {ESTIMATED_BETA_NOTE} に基づく相対指標であり、人数換算しない。</p>\n"
    ));
    html.push_str("</section>\n");
}

// --------------- Section 8: 政令市区別ランキング (商品の核心) ---------------
//
// parent_rank (市内順位) を主指標、national_rank (全国順位) を参考表記する。
// HTML 内で parent_rank セルが必ず national_rank セルより先に出力される (test で順序検証)。

// NOTE (2026-05-08): `#[allow(dead_code)]` は call site (render_section_market_intelligence
// L262) からは常時呼ばれているため CFG 上は dead ではないが、現 fixture (indeed_test_50.csv)
// では政令市データ非空時のみ実体出力される条件付き section。E2E spec / probe では
// 「現 fixture では 0 件が正常」として扱うこと (docs/SPEC_SELECTOR_AUDIT_2026_05_08.md §1.2 参照)。
// 本番実顧客 CSV (政令市含む) 投入時に再評価し、必要なら dead_code 属性を外す。
#[allow(dead_code)]
pub(crate) fn render_mi_parent_ward_ranking(
    html: &mut String,
    rankings: &[WardRankingRowDto],
    _code_master: &[MunicipalityCodeMasterDto],
) {
    // If the target set has no designated wards, this section is not applicable.
    // Omit it instead of exposing an internal unavailable-data state.
    if rankings.is_empty() {
        return;
    }

    html.push_str(
        "<section class=\"mi-parent-ward-ranking\" data-mi-section=\"parent-ward-ranking\" \
         aria-labelledby=\"mi-pwr-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-pwr-heading\">政令市区別ランキング \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[商品コア]</span></h3>\n",
    );

    html.push_str(&format!(
        "<p class=\"mi-note\" style=\"font-size:12px;color:#64748b;margin:0 0 8px;\">\
         表示優先: <strong>市内順位 (主)</strong> &gt; 市内総数 &gt; 全国順位 (参考)。<br/>\
         {ESTIMATED_BETA_NOTE}</p>\n"
    ));

    // parent_code でグループ化 (BTreeMap でキー昇順固定)
    let mut by_parent: BTreeMap<String, Vec<&WardRankingRowDto>> = BTreeMap::new();
    for r in rankings {
        by_parent.entry(r.parent_code.clone()).or_default().push(r);
    }

    for (parent_code, mut wards) in by_parent {
        // parent_rank 昇順
        wards.sort_by_key(|w| w.parent_rank);
        let parent_name = wards
            .first()
            .map(|w| w.parent_name.clone())
            .unwrap_or_default();
        let parent_total = wards.first().map(|w| w.parent_total).unwrap_or(0);

        html.push_str(&format!(
            "  <div class=\"mi-parent-group\" data-parent-code=\"{pc}\" \
             style=\"margin:12px 0;padding:10px;border:1px solid #e2e8f0;border-radius:4px;\">\n\
                <h4 style=\"margin:0 0 6px;color:#1e3a8a;\">{name} ({total} 区)</h4>\n",
            pc = escape_html(&parent_code),
            name = escape_html(&parent_name),
            total = parent_total,
        ));

        html.push_str(
            "    <table class=\"mi-rank-table\" style=\"width:100%;border-collapse:collapse;font-size:13px;\">\n\
             <colgroup>\
             <col class=\"mi-col-prank\" />\
             <col class=\"mi-col-name\" />\
             <col class=\"mi-col-thick\" />\
             <col class=\"mi-col-prio\" />\
             <col class=\"mi-col-nrank\" />\
             </colgroup>\n\
             <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
             <th class=\"mi-col-prank\" scope=\"col\" style=\"text-align:left;padding:6px;\">市内順位 (主)</th>\
             <th class=\"mi-col-name\" scope=\"col\" style=\"text-align:left;padding:6px;\">区名</th>\
             <th class=\"mi-col-thick\" scope=\"col\" style=\"text-align:right;padding:6px;\">厚み指数 (推定 β)</th>\
             <th class=\"mi-col-prio\" scope=\"col\" style=\"text-align:left;padding:6px;\">優先度</th>\
             <th class=\"mi-col-nrank mi-ref\" scope=\"col\" style=\"text-align:right;padding:6px;color:#64748b;font-size:11px;\">全国順位 (参考)</th>\
             </tr></thead><tbody>\n",
        );

        for w in &wards {
            let priority_lower = w.priority.to_lowercase();
            // 厚み指数 0-200 をバーで視覚化 (clamp)
            let thick_pct = (w.thickness_index / 200.0 * 100.0).clamp(0.0, 100.0);
            // 工業集積地アンカー: priority "S" を高優先帯マーカーとして 🏭 を付与
            // (DTO に is_industrial_anchor 直結フィールドが無いため Worker D は priority で代替表示)
            let anchor_html = if w.priority.eq_ignore_ascii_case("S") {
                format!(
                    "<span class=\"mi-anchor-badge\" title=\"高優先 / 集積地候補\">{}</span>",
                    ANCHOR_BADGE
                )
            } else {
                String::new()
            };
            html.push_str(&format!(
                "<tr>\
                 <th class=\"mi-col-prank mi-parent-rank\" scope=\"row\" style=\"padding:6px;text-align:left;\"><strong>{prank} 位</strong> / {ptotal} 区</th>\
                 <td class=\"mi-col-name\" style=\"padding:6px;\">{name}{anchor}</td>\
                 <td class=\"mi-col-thick mi-thickness\" style=\"text-align:right;padding:6px;\">{thick:.1}\
                 <span class=\"mi-thickness-bar-wrap\" aria-hidden=\"true\">\
                 <span class=\"mi-thickness-bar-fill\" style=\"width:{tpct:.0}%;\"></span></span></td>\
                 <td class=\"mi-col-prio mi-priority mi-priority-{plow}\" style=\"padding:6px;\">\
                 <span class=\"mi-priority-badge mi-priority-{plow}\">{prio}</span></td>\
                 <td class=\"mi-col-nrank mi-ref\" style=\"text-align:right;padding:6px;color:#64748b;font-size:11px;\">{nrank} 位 / {ntotal} 市区町村</td>\
                 </tr>\n",
                prank = w.parent_rank,
                ptotal = w.parent_total,
                name = escape_html(&w.municipality_name),
                anchor = anchor_html,
                thick = w.thickness_index,
                tpct = thick_pct,
                plow = escape_html(&priority_lower),
                prio = escape_html(&w.priority),
                nrank = w.national_rank,
                ntotal = w.national_total,
            ));
        }

        html.push_str("    </tbody></table>\n  </div>\n");
    }

    html.push_str("</section>\n");
}

// --------------- Section 1: 結論サマリーカード ---------------

/// 結論サマリーカード: 主要 KPI 4 つを 1 行に並べる。
fn render_mi_summary_card(html: &mut String, data: &SurveyMarketIntelligenceData) {
    html.push_str(
        "<section class=\"mi-summary\" data-mi-section=\"summary\" \
         aria-labelledby=\"mi-summary-heading\" \
         style=\"margin:16px 0;padding:12px;background:#f8fafc;border:1px solid #cbd5e1;border-radius:6px;\">\n"
    );
    html.push_str("<h3 id=\"mi-summary-heading\" style=\"margin:0 0 8px;\">結論サマリー</h3>\n");

    if data.is_empty() {
        render_mi_placeholder(
            html,
            "サマリー算出に必要な事前集計データが投入されていません。",
        );
        html.push_str("</section>\n");
        return;
    }

    // 集計値 (推定値中心)
    let target_count = data.recruiting_scores.len();
    let valid_priority_count = data
        .recruiting_scores
        .iter()
        .filter(|s| s.is_priority_score_in_range())
        .count();
    let avg_priority = average_score(&data.recruiting_scores);

    let high_priority_count = data
        .recruiting_scores
        .iter()
        .filter(|s| s.distribution_priority_score.unwrap_or(0.0) >= 80.0)
        .count();
    let invariant_violation = data.recruiting_scores.len() - valid_priority_count;

    html.push_str("<div class=\"mi-kpi-row\" style=\"display:flex;gap:12px;flex-wrap:wrap;\">\n");
    render_mi_kpi(
        html,
        "対象市区町村数",
        &format!("{target_count}"),
        "件",
        MEASURED_LABEL,
    );
    render_mi_kpi(
        html,
        "配信検証候補",
        &format!("{high_priority_count}"),
        "件 (スコア80+)",
        ESTIMATED_LABEL,
    );
    render_mi_kpi(
        html,
        "配信優先度 平均",
        &avg_priority
            .map(|v| format!("{v:.1}"))
            .unwrap_or("-".into()),
        "/100",
        ESTIMATED_LABEL,
    );
    render_mi_kpi(
        html,
        "通勤流入元 取得数",
        &format!("{}", data.commute_flows.len()),
        "件",
        MEASURED_LABEL,
    );
    html.push_str("</div>\n");

    if invariant_violation > 0 {
        html.push_str(&format!(
            "<p style=\"font-size:12px;color:#b45309;margin:8px 0 0;\">\
             \u{26A0} 不変条件違反 ({} 件) を検出: distribution_priority_score が 0〜100 範囲外の \
             エントリは表示から除外しています。</p>\n",
            invariant_violation
        ));
    }

    html.push_str("</section>\n");
}

// --------------- Section 2: 配信地域ランキング ---------------

/// 配信地域ランキング用に、`municipality_code` 単位で集約した 1 行分の表示データ。
///
/// 同一自治体に職業 (`occupation_code`) 別の複数行 (例: 11 職種分) が存在しても、
/// `distribution_priority_score` が最大の行を「代表」として 1 行に集約し、
/// それ以外は `other_occupation_count` (= group_size - 1) として件数のみ表示する。
///
/// MI レポート page 16 で同じ自治体名が連続表示される問題への対応 (P1)。
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AggregatedMunicipalityRow {
    municipality_code: String,
    prefecture: String,
    municipality_name: String,
    /// 代表行 (group 内 score 最大) の職業名
    top_occupation_name: String,
    /// 代表行のスコア (0.0〜100.0)
    top_score: f64,
    /// 代表行の優先度区分 (S/A/B/C/D)
    top_priority: Option<String>,
    /// 代表行の厚み指数
    top_thickness_index: Option<f64>,
    /// 代表行の競合求人数
    top_competitor_job_count: Option<i64>,
    /// 代表以外の職業件数 (= group_size - 1)。0 のとき UI で非表示。
    other_occupation_count: usize,
}

/// `municipality_code` 単位で集約し、`distribution_priority_score` 最大行を代表とする。
///
/// - 入力 `scores` は呼び出し側で不変条件 (range / scenario consistency) 通過済みを想定。
/// - 出力は `top_score` 降順でソート済み。
/// - 同点時は `prefecture` → `municipality_name` の lexicographic 順で安定化。
fn aggregate_by_municipality(
    scores: &[&MunicipalityRecruitingScore],
) -> Vec<AggregatedMunicipalityRow> {
    use std::collections::BTreeMap;
    let mut by_muni: BTreeMap<String, Vec<&MunicipalityRecruitingScore>> = BTreeMap::new();
    for s in scores {
        by_muni
            .entry(s.municipality_code.clone())
            .or_default()
            .push(s);
    }

    let mut rows: Vec<AggregatedMunicipalityRow> = by_muni
        .into_iter()
        .filter_map(|(_code, group)| {
            // group 内 score 最大の行を代表に選ぶ
            let representative = group.iter().max_by(|a, b| {
                a.distribution_priority_score
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&b.distribution_priority_score.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
            Some(AggregatedMunicipalityRow {
                municipality_code: representative.municipality_code.clone(),
                prefecture: representative.prefecture.clone(),
                municipality_name: representative.municipality_name.clone(),
                top_occupation_name: representative.occupation_name.clone(),
                top_score: representative.distribution_priority_score.unwrap_or(0.0),
                top_priority: representative.distribution_priority.clone(),
                top_thickness_index: representative.target_thickness_index,
                top_competitor_job_count: representative.competitor_job_count,
                other_occupation_count: group.len().saturating_sub(1),
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        b.top_score
            .partial_cmp(&a.top_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.prefecture.cmp(&b.prefecture))
            .then_with(|| a.municipality_name.cmp(&b.municipality_name))
    });

    rows
}

#[derive(Debug, Clone, Copy)]
struct AggregatedLivingCostRow<'a> {
    score: &'a MunicipalityRecruitingScore,
    score_value: Option<f64>,
    other_occupation_count: usize,
}

fn salary_living_value(score: &MunicipalityRecruitingScore) -> Option<f64> {
    score.salary_living_score.or(score.living_cost_score)
}

/// Aggregate salary/living-cost rows by `municipality_code`.
///
/// When one municipality has multiple occupation rows, the row with the highest
/// salary/living score is shown as the representative row.
/// This prevents page 16 from repeating the same municipality for every occupation.
fn aggregate_living_cost_by_municipality(
    scores: &[MunicipalityRecruitingScore],
) -> Vec<AggregatedLivingCostRow<'_>> {
    use std::collections::BTreeMap;

    let mut by_muni: BTreeMap<String, Vec<&MunicipalityRecruitingScore>> = BTreeMap::new();
    for score in scores {
        by_muni
            .entry(score.municipality_code.clone())
            .or_default()
            .push(score);
    }

    let mut rows: Vec<AggregatedLivingCostRow<'_>> = by_muni
        .into_values()
        .filter_map(|group| {
            let representative = group.iter().max_by(|a, b| {
                salary_living_value(a)
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&salary_living_value(b).unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        a.distribution_priority_score
                            .unwrap_or(f64::NEG_INFINITY)
                            .partial_cmp(
                                &b.distribution_priority_score.unwrap_or(f64::NEG_INFINITY),
                            )
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            })?;
            Some(AggregatedLivingCostRow {
                score: representative,
                score_value: salary_living_value(representative),
                other_occupation_count: group.len().saturating_sub(1),
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        b.score_value
            .unwrap_or(f64::NEG_INFINITY)
            .partial_cmp(&a.score_value.unwrap_or(f64::NEG_INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.score.prefecture.cmp(&b.score.prefecture))
            .then_with(|| a.score.municipality_name.cmp(&b.score.municipality_name))
            .then_with(|| a.score.municipality_code.cmp(&b.score.municipality_code))
    });

    rows
}

fn render_mi_distribution_ranking(html: &mut String, scores: &[MunicipalityRecruitingScore]) {
    // P2 (2026-05-08): mi-print-block で印刷時の表中分断を防止 (page 25 情報密度改善)
    html.push_str(
        "<section class=\"mi-ranking mi-print-block\" data-mi-section=\"distribution-ranking\" \
         aria-labelledby=\"mi-ranking-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-ranking-heading\">配信地域ランキング \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{label}]</span></h3>\n"
            .replace("{label}", ESTIMATED_LABEL)
            .as_str(),
    );

    let valid: Vec<&MunicipalityRecruitingScore> = scores
        .iter()
        .filter(|s| s.is_priority_score_in_range() && s.is_scenario_consistent())
        .collect();
    if valid.is_empty() {
        render_mi_placeholder(
            html,
            "\u{8A72}\u{5F53}\u{306A}\u{3057}\u{3067}\u{3059}\u{3002}\u{4E0A}\u{90E8}\u{30B5}\u{30DE}\u{30EA}\u{30FC}\u{3068}\u{914D}\u{4FE1}\u{5730}\u{57DF}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{3092}\u{78BA}\u{8A8D}\u{3057}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}",
        );
        html.push_str("</section>\n");
        return;
    }

    // P1 (2026-05-06): 同一自治体が職業別に複数行表示される重複問題を解消。
    // municipality_code 単位で 1 行に集約し、代表職種 + 「ほか N 職種」表示にする。
    let aggregated = aggregate_by_municipality(&valid);

    html.push_str("<table class=\"mi-table\" style=\"width:100%;border-collapse:collapse;font-size:13px;\">\n");
    html.push_str(
        "<thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">順位</th>\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:left;padding:6px;\">代表職種</th>\
         <th style=\"text-align:right;padding:6px;\">配信優先度</th>\
         <th style=\"text-align:right;padding:6px;\">厚み指数</th>\
         <th style=\"text-align:right;padding:6px;\">競合求人数</th>\
         <th style=\"text-align:left;padding:6px;\">区分</th>\
         </tr></thead><tbody>\n",
    );
    for (rank, row) in aggregated.iter().enumerate().take(20) {
        let bucket = match row.top_score {
            v if v >= 80.0 => "重点配信",
            v if v >= 65.0 => "拡張候補",
            v if v >= 50.0 => "維持/検証",
            _ => "優先度低",
        };
        // 代表職種セルに「ほか N 職種」サブテキストを併記 (N>0 のときのみ)
        let occupation_cell = if row.other_occupation_count > 0 {
            format!(
                "{occ}<span class=\"mi-rank-other-occ\" style=\"display:block;font-size:11px;color:#64748b;\">ほか {n} 職種</span>",
                occ = escape_html(&row.top_occupation_name),
                n = row.other_occupation_count,
            )
        } else {
            escape_html(&row.top_occupation_name)
        };
        // Worker E Round 3: 旧 target_population (常に None) 表示を厚み指数に置換
        // resident estimated_beta セクションでは「人」単位を表示しないルール (feedback_test_data_validation)
        html.push_str(&format!(
            "<tr><td style=\"padding:6px;\">{rank}</td>\
             <td style=\"padding:6px;\">{pref} {muni}</td>\
             <td style=\"padding:6px;\">{occ_cell}</td>\
             <td style=\"text-align:right;padding:6px;\">{score}</td>\
             <td style=\"text-align:right;padding:6px;\">{thick}</td>\
             <td style=\"text-align:right;padding:6px;\">{comp}</td>\
             <td style=\"padding:6px;color:#64748b;\">{bucket}</td></tr>\n",
            rank = rank + 1,
            pref = escape_html(&row.prefecture),
            muni = escape_html(&row.municipality_name),
            occ_cell = occupation_cell,
            score = format!("{:.1}", row.top_score),
            thick = row
                .top_thickness_index
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            comp = format_opt_i64(row.top_competitor_job_count),
            bucket = bucket,
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         配信優先度は METRICS.md §2.1 の `clamp(positive_score × (1 - penalty_reduction_pct/100), 0, 100)` で算出 [{}]。\
         「採用しやすさの断定」ではなく「検証すべき配信地域の優先順位」として扱う。\
         同一自治体で複数職種ある場合は配信優先度が最大の職種を代表として 1 行に集約。</p>\n",
        ESTIMATED_LABEL
    ));
    html.push_str("</section>\n");
}

// --------------- Section 3: 人材供給ヒートマップ (テーブル版) ---------------

fn render_mi_talent_supply(html: &mut String, cells: &[OccupationPopulationCell]) {
    html.push_str(
        "<section class=\"mi-talent\" data-mi-section=\"talent\" \
         aria-labelledby=\"mi-talent-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-talent-heading\">人材供給ヒートマップ \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        MEASURED_LABEL
    ));

    if cells.is_empty() {
        render_mi_placeholder(
            html,
            "\u{8A72}\u{5F53}\u{306A}\u{3057}\u{3067}\u{3059}\u{3002}\u{4E0A}\u{90E8}\u{30B5}\u{30DE}\u{30EA}\u{30FC}\u{3068}\u{914D}\u{4FE1}\u{5730}\u{57DF}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{3092}\u{78BA}\u{8A8D}\u{3057}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}",
        );
        html.push_str("</section>\n");
        return;
    }

    html.push_str(
        "<table class=\"mi-talent-table\" style=\"width:100%;border-collapse:collapse;font-size:12px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:left;padding:6px;\">職業</th>\
         <th style=\"text-align:left;padding:6px;\">年齢階級</th>\
         <th style=\"text-align:left;padding:6px;\">性別</th>\
         <th style=\"text-align:right;padding:6px;\">人口</th>\
         </tr></thead><tbody>\n",
    );
    for c in cells.iter().take(40) {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{muni}</td>\
             <td style=\"padding:4px;\">{occ}</td>\
             <td style=\"padding:4px;\">{age}</td>\
             <td style=\"padding:4px;\">{gender}</td>\
             <td style=\"text-align:right;padding:4px;\">{pop}</td></tr>\n",
            muni = escape_html(&c.municipality_name),
            occ = escape_html(&c.occupation_name),
            age = escape_html(&c.age_group),
            gender = escape_html(&c.gender),
            pop = format_opt_i64(c.population),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         国勢調査由来の常住地ベース人口 [{}]。タイルマップ可視化は後続スコープ。</p>\n",
        MEASURED_LABEL
    ));
    html.push_str("</section>\n");
}

// --------------- Section 4: 給与・生活コスト比較 ---------------

fn render_mi_salary_living_cost(
    html: &mut String,
    scores: &[MunicipalityRecruitingScore],
    living: &[LivingCostProxy],
) {
    // P2 (2026-05-08): mi-print-break-before で給与・生活コスト比較を独立ページに分離。
    // page 25 で配信地域ランキング + 給与生活コスト + シナリオレンジの 3 表が同居していた問題を改善。
    html.push_str(
        "<section class=\"mi-living mi-print-block mi-print-break-before\" data-mi-section=\"salary-living-cost\" aria-labelledby=\"mi-living-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-living-heading\">給与・生活コスト比較 \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        REFERENCE_LABEL
    ));

    if scores.is_empty() && living.is_empty() {
        render_mi_placeholder(
            html,
            "\u{8A72}\u{5F53}\u{306A}\u{3057}\u{3067}\u{3059}\u{3002}\u{4E0A}\u{90E8}\u{30B5}\u{30DE}\u{30EA}\u{30FC}\u{3068}\u{914D}\u{4FE1}\u{5730}\u{57DF}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{3092}\u{78BA}\u{8A8D}\u{3057}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}",
        );
        html.push_str("</section>\n");
        return;
    }

    // 主キー (municipality_code) で結合
    use std::collections::HashMap;
    let living_map: HashMap<&str, &LivingCostProxy> = living
        .iter()
        .map(|l| (l.municipality_code.as_str(), l))
        .collect();

    // Worker E Round 3: Worker A/B 投入版の新フィールドを優先使用
    //   給与中央値 → median_salary_yen (旧) は SQL から外れたため - 表示。
    //                代替として salary_living_score (新, 0-100 指数) を表示。
    //   家賃 proxy → single_household_rent_proxy (旧) も廃止 → land_price_proxy へ置換。
    //   物価指数 → cost_index (新)。retail_price_index_proxy (旧) は fallback。
    //   生活コストスコア → salary_living_score (新) を優先、living_cost_score (旧) を fallback。
    html.push_str(
        "<table class=\"mi-living-table\" style=\"width:100%;border-collapse:collapse;font-size:12px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:left;padding:6px;\">\u{4EE3}\u{8868}\u{8077}\u{7A2E}</th>\
         <th style=\"text-align:right;padding:6px;\">給与×生活 指数</th>\
         <th style=\"text-align:right;padding:6px;\">最低賃金 (時給)</th>\
         <th style=\"text-align:right;padding:6px;\">物価指数 (cost_index)</th>\
         <th style=\"text-align:right;padding:6px;\">生活コストスコア</th>\
         </tr></thead><tbody>\n",
    );
    let aggregated_living = aggregate_living_cost_by_municipality(scores);
    for row in aggregated_living.iter().take(20) {
        let s = row.score;
        let liv = living_map.get(s.municipality_code.as_str());
        let occupation_cell = if row.other_occupation_count > 0 {
            format!(
                "{occ}<span class=\"mi-rank-other-occ\" style=\"display:block;font-size:11px;color:#64748b;\">\u{307B}\u{304B} {n} \u{8077}\u{7A2E}</span>",
                occ = escape_html(&s.occupation_name),
                n = row.other_occupation_count,
            )
        } else {
            escape_html(&s.occupation_name)
        };
        // 給与×生活 指数: salary_living_score (新) 優先
        let salary_idx = s
            .salary_living_score
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".into());
        // 最低賃金 (時給): 新 LivingCostProxy.min_wage
        let min_wage_html = liv
            .and_then(|l| l.min_wage)
            .map(|v| format!("¥{}", format_thousands(v)))
            .unwrap_or_else(|| "-".into());
        // 物価指数: cost_index (新) 優先 / retail_price_index_proxy (旧) を fallback
        let price_html = liv
            .and_then(|l| l.cost_index.or(l.retail_price_index_proxy))
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".into());
        // 生活コストスコア: salary_living_score (新) を優先、living_cost_score (旧) を fallback
        let lcs_html = s
            .salary_living_score
            .or(s.living_cost_score)
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".into());
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"padding:4px;\">{occ_cell}</td>\
             <td style=\"text-align:right;padding:4px;\">{salary_idx}</td>\
             <td style=\"text-align:right;padding:4px;\">{min_wage_html}</td>\
             <td style=\"text-align:right;padding:4px;\">{price_html}</td>\
             <td style=\"text-align:right;padding:4px;\">{lcs_html}</td></tr>\n",
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            occ_cell = occupation_cell,
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         物価指数 (cost_index) は全国平均 100 を基準とする相対値 [{}]。\
         最低賃金は厚労省告示の都道府県最低賃金 (時給, 円) [{}]。\
         給与×生活 指数 / 生活コストスコアは 0-100 の相対指数 [{}]。</p>\n",
        REFERENCE_LABEL, REFERENCE_LABEL, ESTIMATED_LABEL
    ));
    html.push_str("</section>\n");
}

// --------------- Section 5: 保守/標準/強気 母集団レンジ ---------------

fn render_mi_scenario_population_range(html: &mut String, scores: &[MunicipalityRecruitingScore]) {
    // P2 (2026-05-08): mi-print-block で印刷時の表中分断を防止。
    // 直前の生活コスト比較が break-before: page で独立ページ化されているため、
    // scenario は break-before: page を付けず生活コスト直後に続けて配置する (page 数増加抑制)。
    html.push_str(
        "<section class=\"mi-scenario mi-print-block\" data-mi-section=\"scenario-population-range\" aria-labelledby=\"mi-scenario-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-scenario-heading\">保守 / 標準 / 強気 母集団レンジ \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        ESTIMATED_LABEL
    ));

    // Worker E Round 3: 新フィールド scenario_*_score (i64) を優先採用。
    //   旧 scenario_*_population は SQL から外れて常に None。
    //   両方の不変条件 (is_scenario_score_consistent / is_scenario_consistent) を満たすもののみ表示。
    let valid: Vec<&MunicipalityRecruitingScore> = scores
        .iter()
        .filter(|s| s.is_scenario_consistent() && s.is_scenario_score_consistent())
        .collect();
    let invariant_excluded = scores.len() - valid.len();

    if valid.is_empty() {
        render_mi_placeholder(
            html,
            "\u{8A72}\u{5F53}\u{306A}\u{3057}\u{3067}\u{3059}\u{3002}\u{4E0A}\u{90E8}\u{30B5}\u{30DE}\u{30EA}\u{30FC}\u{3068}\u{914D}\u{4FE1}\u{5730}\u{57DF}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{3092}\u{78BA}\u{8A8D}\u{3057}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}",
        );
        html.push_str("</section>\n");
        return;
    }

    html.push_str(
        "<table class=\"mi-scenario-table\" style=\"width:100%;border-collapse:collapse;font-size:13px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:right;padding:6px;\">保守シナリオスコア</th>\
         <th style=\"text-align:right;padding:6px;\">標準シナリオスコア</th>\
         <th style=\"text-align:right;padding:6px;\">強気シナリオスコア</th>\
         </tr></thead><tbody>\n",
    );
    for s in valid.iter().take(20) {
        // Worker E Round 3: scenario_*_score (i64, 新) を優先、旧 *_population を fallback
        let c_val = s
            .scenario_conservative_score
            .or(s.scenario_conservative_population);
        let m_val = s.scenario_standard_score.or(s.scenario_standard_population);
        let a_val = s
            .scenario_aggressive_score
            .or(s.scenario_aggressive_population);
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:4px;\">{c}</td>\
             <td style=\"text-align:right;padding:4px;\">{m}</td>\
             <td style=\"text-align:right;padding:4px;\">{a}</td></tr>\n",
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            c = format_opt_i64(c_val),
            m = format_opt_i64(m_val),
            a = format_opt_i64(a_val),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         シナリオスコアは配信ターゲット相対指数 (METRICS.md §9) [{}]。\
         「応募者数」ではなく「検証すべき配信地域の優先度」を示す。\
         保守 ≦ 標準 ≦ 強気 を満たすエントリのみ表示。</p>\n",
        ESTIMATED_LABEL
    ));
    if invariant_excluded > 0 {
        html.push_str(&format!(
            "<p style=\"font-size:11px;color:#b45309;margin:4px 0 0;\">\
             \u{26A0} 不変条件 (保守 ≦ 標準 ≦ 強気) 違反 {} 件を表示から除外。</p>\n",
            invariant_excluded
        ));
    }
    html.push_str("</section>\n");
}

// --------------- 補助: 通勤流入元 (Sankey 不実装、ランキングのみ) ---------------

fn render_mi_commute_inflow_supplement(html: &mut String, flows: &[CommuteFlowSummary]) {
    html.push_str(
        "<section class=\"mi-commute\" data-mi-section=\"commute\" \
         aria-labelledby=\"mi-commute-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-commute-heading\">通勤流入元 (補助表示) \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        MEASURED_LABEL
    ));

    if flows.is_empty() {
        render_mi_placeholder(
            html,
            "通勤流入元データを取得できません (`v2_external_commute_od` または `commute_flow_summary` 未利用)。",
        );
        html.push_str("</section>\n");
        return;
    }

    html.push_str(
        "<table style=\"width:100%;border-collapse:collapse;font-size:13px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">流入元</th>\
         <th style=\"text-align:right;padding:6px;\">通勤者数</th>\
         </tr></thead><tbody>\n",
    );
    for f in flows.iter().take(20) {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:4px;\">{n}</td></tr>\n",
            pref = escape_html(&f.origin_prefecture),
            muni = escape_html(&f.origin_municipality_name),
            n = format_opt_i64(f.flow_count),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         国勢調査 OD 行列 [{}]。Sankey 可視化は後続スコープ。</p>\n",
        MEASURED_LABEL
    ));
    html.push_str("</section>\n");
}

// --------------- 共通ヘルパー ---------------

fn render_mi_kpi(html: &mut String, label: &str, value: &str, unit: &str, label_tag: &str) {
    html.push_str(&format!(
        "<div class=\"mi-kpi\" style=\"flex:1;min-width:140px;padding:8px 10px;background:#fff;border:1px solid #e2e8f0;border-radius:4px;\">\
         <div style=\"font-size:11px;color:#64748b;\">{label} <span style=\"color:#1e3a8a;\">[{tag}]</span></div>\
         <div style=\"font-size:18px;font-weight:600;color:#0f172a;margin-top:2px;\">{value}<span style=\"font-size:11px;color:#64748b;font-weight:400;margin-left:2px;\">{unit}</span></div>\
         </div>\n",
        label = escape_html(label),
        tag = label_tag,
        value = escape_html(value),
        unit = escape_html(unit),
    ));
}

fn render_mi_placeholder(html: &mut String, _msg: &str) {
    // Client-facing PDF/HTML must not expose DB/fetch/internal fallback details.
    html.push_str(
        "<div class=\"mi-placeholder\" role=\"note\" \
         style=\"padding:10px;background:#fef3c7;border:1px solid #fcd34d;border-radius:4px;color:#92400e;font-size:13px;\">\
         \u{2139} \u{8A72}\u{5F53}\u{306A}\u{3057}</div>\n"
    );
}

fn format_opt_i64(v: Option<i64>) -> String {
    v.map(format_thousands).unwrap_or_else(|| "-".to_string())
}

fn format_thousands(v: i64) -> String {
    let s = v.abs().to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn average_score(scores: &[MunicipalityRecruitingScore]) -> Option<f64> {
    let valid: Vec<f64> = scores
        .iter()
        .filter(|s| s.is_priority_score_in_range())
        .filter_map(|s| s.distribution_priority_score)
        .collect();
    if valid.is_empty() {
        None
    } else {
        Some(valid.iter().sum::<f64>() / valid.len() as f64)
    }
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_score(code: &str, score: f64, c: i64, s: i64, a: i64) -> MunicipalityRecruitingScore {
        MunicipalityRecruitingScore {
            municipality_code: code.into(),
            prefecture: "北海道".into(),
            municipality_name: "札幌市".into(),
            distribution_priority_score: Some(score),
            target_population: Some(10_000),
            competitor_job_count: Some(500),
            median_salary_yen: Some(280_000),
            living_cost_score: Some(60.0),
            scenario_conservative_population: Some(c),
            scenario_standard_population: Some(s),
            scenario_aggressive_population: Some(a),
            ..Default::default()
        }
    }

    #[test]
    fn test_render_with_empty_data_does_not_panic_and_shows_placeholder() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(html.contains("採用マーケットインテリジェンス"));
        // P0 (2026-05-06): placeholder prefix を「データ準備中」→「該当なし」に変更
        assert!(html.contains("該当なし"));
        // 5 セクション + 1 補助セクションの placeholder が出ること (各 section 内に 1 つずつ)
        let placeholder_count = html.matches("mi-placeholder").count();
        assert!(
            placeholder_count >= 5,
            "placeholder が 5 セクション以上に出る (実際 {})",
            placeholder_count
        );
    }

    #[test]
    fn test_render_includes_all_three_label_types() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            living_cost_proxies: vec![LivingCostProxy {
                municipality_code: "01101".into(),
                single_household_rent_proxy: Some(45_000),
                retail_price_index_proxy: Some(101.5),
                ..Default::default()
            }],
            commute_flows: vec![CommuteFlowSummary {
                origin_prefecture: "北海道".into(),
                origin_municipality_name: "江別市".into(),
                flow_count: Some(8000),
                ..Default::default()
            }],
            occupation_populations: vec![OccupationPopulationCell {
                municipality_name: "札幌市".into(),
                occupation_name: "輸送・機械運転".into(),
                age_group: "30-39".into(),
                gender: "male".into(),
                population: Some(12_345),
                ..Default::default()
            }],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);

        // 3 種類のラベル全てが HTML に含まれる
        assert!(html.contains(MEASURED_LABEL), "実測 ラベル必須");
        assert!(html.contains(ESTIMATED_LABEL), "推定 ラベル必須");
        assert!(html.contains(REFERENCE_LABEL), "参考 ラベル必須");
    }

    #[test]
    fn test_distribution_ranking_excludes_invariant_violation() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score("01101", 85.0, 100, 300, 500), // 適合
                MunicipalityRecruitingScore {
                    municipality_code: "01102".into(),
                    distribution_priority_score: Some(150.0), // 範囲外 → 除外
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);
        // 01101 (適合) はランキングに表示
        assert!(html.contains("85"), "適合スコアは表示");
        // 01102 (範囲外) は表示されない (municipality_code 文字列で確認)
        assert!(!html.contains("150"), "範囲外スコアは表示されない");
        // 不変条件違反の警告が出る
        assert!(html.contains("不変条件違反"), "違反警告必須");
    }

    #[test]
    fn test_scenario_ranking_filters_inconsistent_entries() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score("01101", 75.0, 100, 300, 500), // 保守 ≦ 標準 ≦ 強気
                sample_score("01102", 70.0, 500, 300, 100), // 順序逆転 → 除外
            ],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);
        // 不変条件違反のメッセージが出る
        assert!(
            html.contains("保守 ≦ 標準 ≦ 強気") || html.contains("不変条件"),
            "シナリオ違反の説明文必須"
        );
    }

    #[test]
    fn test_format_thousands() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(123), "123");
        assert_eq!(format_thousands(1_234), "1,234");
        assert_eq!(format_thousands(1_234_567), "1,234,567");
        assert_eq!(format_thousands(-1_234), "-1,234");
    }

    #[test]
    fn test_average_score_skips_invalid() {
        let scores = vec![
            sample_score("01", 80.0, 1, 1, 1),
            sample_score("02", 60.0, 1, 1, 1),
            MunicipalityRecruitingScore {
                distribution_priority_score: Some(150.0), // 不変条件違反
                ..Default::default()
            },
        ];
        let avg = average_score(&scores);
        assert_eq!(avg, Some(70.0)); // (80 + 60) / 2 = 70.0
    }

    #[test]
    fn test_average_score_empty_returns_none() {
        let scores: Vec<MunicipalityRecruitingScore> = vec![];
        assert_eq!(average_score(&scores), None);

        // 全部不変条件違反
        let scores = vec![MunicipalityRecruitingScore {
            distribution_priority_score: Some(-10.0),
            ..Default::default()
        }];
        assert_eq!(average_score(&scores), None);
    }

    // ============================================================
    // Phase 3 Step 5 Phase 4: Plan B 表示テスト
    // ============================================================

    fn make_workplace_measured_cell(name: &str, pop: i64) -> OccupationCellDto {
        OccupationCellDto {
            municipality_code: "14103".into(),
            prefecture: "神奈川県".into(),
            municipality_name: name.into(),
            basis: "workplace".into(),
            occupation_code: "08_生産工程".into(),
            occupation_name: "生産工程従事者".into(),
            age_class: "all".into(),
            gender: "all".into(),
            population: Some(pop),
            estimate_index: None,
            data_label: "measured".into(),
            source_name: "国勢調査".into(),
            source_year: 2020,
            weight_source: None,
        }
    }

    fn make_resident_estimated_beta_cell(name: &str, idx: f64) -> OccupationCellDto {
        OccupationCellDto {
            municipality_code: "14103".into(),
            prefecture: "神奈川県".into(),
            municipality_name: name.into(),
            basis: "resident".into(),
            occupation_code: "08_生産工程".into(),
            occupation_name: "生産工程従事者".into(),
            age_class: "all".into(),
            gender: "all".into(),
            population: None,
            estimate_index: Some(idx),
            data_label: "estimated_beta".into(),
            source_name: "Model F2".into(),
            source_year: 2020,
            weight_source: Some("hypothesis_v1".into()),
        }
    }

    fn make_ranking_row(
        muni: &str,
        prank: i64,
        ptotal: i64,
        nrank: i64,
        ntotal: i64,
    ) -> WardRankingRowDto {
        WardRankingRowDto {
            municipality_code: "14103".into(),
            municipality_name: muni.into(),
            parent_code: "14100".into(),
            parent_name: "横浜市".into(),
            parent_rank: prank,
            parent_total: ptotal,
            national_rank: nrank,
            national_total: ntotal,
            thickness_index: 142.5,
            priority: "A".into(),
        }
    }

    #[test]
    fn test_workplace_measured_renders_population_with_label() {
        let mut html = String::new();
        let cells = vec![make_workplace_measured_cell("横浜市鶴見区", 12_345)];
        render_mi_occupation_cells(&mut html, &cells);
        assert!(
            html.contains(WORKPLACE_LABEL),
            "workplace ラベル必須: {WORKPLACE_LABEL}"
        );
        assert!(
            html.contains("12,345 人"),
            "人数 (3 桁区切り + '人') が表示されること: HTML={html}"
        );
        assert!(
            html.contains(MEASURED_DATA_SOURCE),
            "出典 (国勢調査 R2) 表示必須"
        );
    }

    #[test]
    fn test_resident_estimated_beta_does_not_render_population() {
        let mut html = String::new();
        let cells = vec![make_resident_estimated_beta_cell("横浜市鶴見区", 142.5)];
        render_mi_occupation_cells(&mut html, &cells);

        assert!(html.contains(RESIDENT_LABEL), "常住地ラベル必須");
        assert!(html.contains("指数"), "指数表記必須");
        assert!(html.contains("142.5"), "estimate_index .1f 表示必須");

        // resident estimated_beta 行で「{数値} 人」パターンが出ないこと
        // (人 だけは「人気」「人材」等と区別、数値直後の「人」のみ検査)
        // 簡易チェック: 「12,345 人」「100 人」のような形が含まれない
        let has_pop_pattern = html
            .lines()
            .any(|line| line.contains(" 人") && !line.contains("指数"));
        assert!(
            !has_pop_pattern,
            "estimated_beta 行に人数パターンが含まれてはいけない: {html}"
        );

        // Hard NG 用語が含まれない
        for forbidden in [
            "推定人数",
            "想定人数",
            "母集団人数",
            "estimated_population",
            "target_count",
        ] {
            assert!(
                !html.contains(forbidden),
                "Hard NG 用語 '{forbidden}' が含まれている"
            );
        }
    }

    #[test]
    fn test_parent_rank_renders_before_national_rank() {
        let mut html = String::new();
        let rankings = vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)];
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);

        let p_idx = html
            .find("市内順位 (主)")
            .or_else(|| html.find("mi-parent-rank"));
        let n_idx = html.find("全国順位 (参考)").or_else(|| html.find("mi-ref"));
        assert!(p_idx.is_some(), "parent_rank 関連 HTML が含まれること");
        assert!(n_idx.is_some(), "national_rank 関連 HTML が含まれること");
        assert!(
            p_idx.unwrap() < n_idx.unwrap(),
            "parent_rank が national_rank より前に出ること: parent={:?} national={:?}",
            p_idx,
            n_idx
        );
        // 数値の確認 (parent_rank の値「3 位」)
        assert!(html.contains("3 位"), "parent_rank '3 位' が表示");
    }

    #[test]
    fn test_parent_ward_ranking_empty_does_not_panic() {
        let mut html = String::new();
        render_mi_parent_ward_ranking(&mut html, &[], &[]);
        // Empty designated-ward rankings are omitted rather than rendered as an internal fallback.
        assert!(
            html.is_empty(),
            "empty rankings should omit the section: {html}"
        );
    }

    #[test]
    fn test_rendered_html_has_no_forbidden_terms() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![
                make_workplace_measured_cell("横浜市鶴見区", 12_345),
                make_resident_estimated_beta_cell("横浜市鶴見区", 142.5),
            ],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);

        for forbidden in [
            "推定人数",
            "想定人数",
            "母集団人数",
            "estimated_population",
            "target_count",
            "estimated_worker_count",
            "resident_population_estimate",
            "convert_index_to_population",
        ] {
            assert!(
                !html.contains(forbidden),
                "Hard NG 用語 '{forbidden}' が出力 HTML に含まれている"
            );
        }
    }

    #[test]
    fn test_parent_ward_ranking_groups_by_parent_code() {
        let mut html = String::new();
        let rankings = vec![
            make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917),
            make_ranking_row("横浜市青葉区", 5, 18, 30, 1917),
        ];
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);
        // 親市名が 1 回 (グループ化されているため)
        let yokohama_count = html.matches("横浜市</h4>").count() + html.matches("横浜市 (").count();
        assert!(yokohama_count >= 1, "親市見出しが少なくとも 1 回出る");
        // 両方の区名が表示
        assert!(html.contains("鶴見区"), "鶴見区表示");
        assert!(html.contains("青葉区"), "青葉区表示");
    }

    #[test]
    fn test_occupation_cells_section_skipped_when_empty_in_top_render() {
        // occupation_cells が空 → 新セクション (`mi-occupation-cells`) が出ない
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(
            !html.contains("mi-occupation-cells"),
            "occupation_cells が空のとき新セクションは出さない"
        );
        // 親 wrapper / 既存 placeholder は出る
        assert!(html.contains("採用マーケットインテリジェンス"));
    }

    #[test]
    fn test_summary_card_summary_has_target_count() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score("01101", 85.0, 100, 300, 500),
                sample_score("01102", 70.0, 50, 100, 200),
                sample_score("01103", 50.0, 10, 30, 50),
            ],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);
        // 対象市区町村数 3
        assert!(html.contains("対象市区町村数"));
        assert!(html.contains(">3<")); // KPI value (between HTML tags)
                                       // 重点配信 (>= 80) は 01101 のみ
        assert!(html.contains("重点配信候補"));
    }

    // ============================================================
    // Phase 3 Step 5 Phase 6 (Worker P6): テスト深耕
    //
    // 1. parent_rank 表示順 強化 (複数 parent / 複数 ward での順序保証)
    // 2. variant 完全分離 (Full / Public で Step5 マーカーが一切出ないこと)
    // 3. empty fallback (空データで panic しない / placeholder 出力)
    // ============================================================

    /// `<tr>...</tr>` ブロック単位で文字列を切り出す簡易ヘルパー
    fn find_row_blocks(html: &str) -> Vec<(usize, usize)> {
        let mut blocks = Vec::new();
        let mut start = 0usize;
        while let Some(open_off) = html[start..].find("<tr") {
            let abs_open = start + open_off;
            if let Some(close_off) = html[abs_open..].find("</tr>") {
                let abs_close = abs_open + close_off + 5;
                blocks.push((abs_open, abs_close));
                start = abs_close;
            } else {
                break;
            }
        }
        blocks
    }

    /// 複数 parent / 複数 ward でも parent_rank が national_rank より前に出ること。
    #[test]
    fn parent_rank_appears_strictly_before_national_rank_in_html() {
        let rankings = vec![
            make_ranking_row("横浜市鶴見区", 1, 18, 5, 1917),
            make_ranking_row("横浜市西区", 2, 18, 12, 1917),
            WardRankingRowDto {
                municipality_code: "27127".into(),
                municipality_name: "大阪市北区".into(),
                parent_code: "27100".into(),
                parent_name: "大阪市".into(),
                parent_rank: 1,
                parent_total: 24,
                national_rank: 8,
                national_total: 1917,
                thickness_index: 138.0,
                priority: "A".into(),
            },
        ];
        let mut html = String::new();
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);

        // 各 <tr> ブロック内で「mi-parent-rank」が「mi-ref」より前に出ること
        let blocks = find_row_blocks(&html);
        let mut checked = 0usize;
        for (s, e) in &blocks {
            let block = &html[*s..*e];
            let p = block.find("mi-parent-rank");
            let n = block.find("mi-ref");
            if let (Some(pi), Some(ni)) = (p, n) {
                assert!(
                    pi < ni,
                    "行ブロックで mi-parent-rank が mi-ref より後ろ (parent={}, ref={})",
                    pi,
                    ni
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 1,
            "少なくとも 1 行で順序検証が走ること (実際: {} 行検査)",
            checked
        );
    }

    /// Full variant では Step 5 マーカーが一切 HTML に出ないこと。
    ///
    /// 設計: `render_section_market_intelligence` は variant ガードの内側で呼ばれる。
    /// Full では `show_market_intelligence_sections() == false` なので呼ばれず、
    /// 結果として Step5 マーカーは出力されない。本テストはガード分岐を直接シミュレート。
    #[test]
    fn full_variant_html_does_not_contain_any_step5_marker() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![
                make_workplace_measured_cell("横浜市鶴見区", 12_345),
                make_resident_estimated_beta_cell("横浜市鶴見区", 142.5),
            ],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };

        // Full variant ガードを再現
        let mut html = String::new();
        if ReportVariant::Full.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut html, &data);
        }

        let step5_markers = [
            "mi-parent-ward-ranking",
            "mi-parent-rank",
            "mi-thickness",
            "mi-rank-table",
            "従業地ベース",
            "常住地ベース",
            "市内順位",
            "検証済み推定 β",
            "Model F2",
        ];
        for marker in &step5_markers {
            assert!(
                !html.contains(marker),
                "Full variant に Step 5 マーカー '{}' が混入",
                marker
            );
        }
        assert!(
            html.is_empty(),
            "Full variant では section 自体が呼ばれず空 HTML"
        );
    }

    /// Public variant でも同様に Step 5 マーカーが一切出ないこと。
    #[test]
    fn public_variant_html_does_not_contain_any_step5_marker() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![make_workplace_measured_cell("横浜市鶴見区", 12_345)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };

        let mut html = String::new();
        if ReportVariant::Public.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut html, &data);
        }

        let step5_markers = [
            "mi-parent-ward-ranking",
            "mi-parent-rank",
            "従業地ベース",
            "常住地ベース",
            "検証済み推定 β",
        ];
        for marker in &step5_markers {
            assert!(
                !html.contains(marker),
                "Public variant に Step 5 マーカー '{}' が混入",
                marker
            );
        }
        assert!(
            html.is_empty(),
            "Public variant では section 自体が呼ばれず空 HTML"
        );
    }

    /// 空データで render_mi_parent_ward_ranking が panic しない + placeholder 出力。
    /// Empty data for render_mi_parent_ward_ranking does not panic and emits no internal fallback.
    #[test]
    fn empty_data_renders_placeholder_not_panic() {
        let mut html = String::new();
        render_mi_parent_ward_ranking(&mut html, &[], &[]);
        assert!(
            html.is_empty(),
            "empty rankings should omit the section: {html}"
        );
    }

    /// 空 occupation_cells で render_mi_occupation_cells が panic しない。
    #[test]
    fn empty_occupation_cells_renders_placeholder_or_empty() {
        let mut html = String::new();
        render_mi_occupation_cells(&mut html, &[]);
        // 空 or placeholder どちらも許容 (panic しないこと自体が主要 invariant)
        // 何かが書かれている場合は Hard NG が混入していないこと
        for forbidden in ["推定人数", "想定人数", "母集団人数"] {
            assert!(
                !html.contains(forbidden),
                "空入力で Hard NG '{}' が出力されている",
                forbidden
            );
        }
    }

    #[test]
    fn placeholder_does_not_expose_internal_data_state() {
        let mut html = String::new();
        render_mi_placeholder(
            &mut html,
            "municipality_occupation_population internal fallback",
        );
        assert!(
            html.contains("\u{8A72}\u{5F53}\u{306A}\u{3057}"),
            "neutral placeholder is rendered: {html}"
        );
        for forbidden in [
            "\u{30C7}\u{30FC}\u{30BF}\u{4E0D}\u{8DB3}",
            "\u{8981}\u{4EF6}\u{518D}\u{78BA}\u{8A8D}",
            "\u{30C7}\u{30FC}\u{30BF}\u{6E96}\u{5099}\u{4E2D}",
            "\u{672A}\u{96C6}\u{8A08}",
            "\u{53C2}\u{8003}\u{8868}\u{793A}\u{306A}\u{3057}",
            "\u{5B9F}\u{6E2C}\u{5024}\u{6E96}\u{5099}\u{4E2D}",
            "\u{672A}\u{6295}\u{5165}",
            "municipality_occupation_population",
        ] {
            assert!(
                !html.contains(forbidden),
                "placeholder exposes forbidden term '{}': {html}",
                forbidden
            );
        }
    }

    // ============================================================
    // Round 2 Worker D: KPI cards / living cost panel / badges / print
    // ============================================================

    #[test]
    fn kpi_cards_show_in_market_intelligence_only() {
        // KPI カードは render_section_market_intelligence の冒頭で出力される。
        // Full / Public variant では section 自体が呼ばれないため出ない。
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);

        // KPI grid マーカーが存在
        assert!(
            html.contains("mi-kpi-grid"),
            "KPI grid CSS class が含まれること"
        );
        assert!(
            html.contains("mi-kpi-card"),
            "KPI card CSS class が含まれること"
        );
        assert!(html.contains("主要指標サマリ"), "KPI 見出しが含まれること");
        assert!(html.contains("配信優先度 A 件数"), "KPI A 件数ラベル");
        assert!(html.contains("厚み指数 平均"), "KPI 厚み指数ラベル");

        // 凡例 (3 ラベル) が出ること
        assert!(html.contains("mi-badge-measured"));
        assert!(html.contains("mi-badge-estimated-beta"));
        assert!(html.contains("mi-badge-reference"));

        // Hard NG 用語が混入していないこと
        for forbidden in [
            "推定人数",
            "想定人数",
            "母集団人数",
            "estimated_population",
            "target_count",
        ] {
            assert!(
                !html.contains(forbidden),
                "Hard NG 用語 '{}' が KPI に含まれている",
                forbidden
            );
        }
    }

    #[test]
    fn living_cost_panel_handles_null_values_gracefully() {
        // 全 NULL のデータで panel が "-" を表示する (ゼロ埋め禁止)
        let mut html = String::new();
        let living: Vec<LivingCostProxy> = vec![];
        let scores: Vec<MunicipalityRecruitingScore> = vec![];
        render_mi_living_cost_panel(&mut html, &living, &scores);

        assert!(html.contains("mi-living-cost-panel"));
        assert!(html.contains("生活コスト"));
        // NULL は「-」表示
        assert!(html.contains(">-<"), "NULL 値が「-」で表示されること");
        // ゼロ埋め禁止 (「0.0」「0 円」が値として現れないこと)
        // (見出しや凡例で 0 が出る可能性があるため、value class 内のみ厳格チェック)
        assert!(!html.contains("class=\"mi-lc-value\">0<"), "ゼロ埋めは禁止");
        assert!(
            !html.contains("class=\"mi-lc-value\">0.0<"),
            "ゼロ埋めは禁止"
        );

        // フッター注記が必須
        assert!(
            html.contains("市区町村ごとの実態を保証するものではありません"),
            "実態保証しない旨の注記必須"
        );

        // 参考統計バッジ
        assert!(
            html.contains("mi-badge-reference"),
            "参考バッジが付与されること"
        );
    }

    #[test]
    fn priority_badges_render_correctly() {
        // 各 priority (S/A/B/C/D) で対応する CSS class が出力されること
        let mut html = String::new();
        let rankings = vec![
            WardRankingRowDto {
                priority: "S".into(),
                parent_rank: 1,
                parent_total: 18,
                national_rank: 5,
                national_total: 1917,
                thickness_index: 180.0,
                municipality_code: "14101".into(),
                municipality_name: "横浜市西区".into(),
                parent_code: "14100".into(),
                parent_name: "横浜市".into(),
            },
            WardRankingRowDto {
                priority: "A".into(),
                parent_rank: 2,
                parent_total: 18,
                national_rank: 12,
                national_total: 1917,
                thickness_index: 142.0,
                municipality_code: "14102".into(),
                municipality_name: "横浜市神奈川区".into(),
                parent_code: "14100".into(),
                parent_name: "横浜市".into(),
            },
            WardRankingRowDto {
                priority: "B".into(),
                parent_rank: 5,
                parent_total: 18,
                national_rank: 88,
                national_total: 1917,
                thickness_index: 100.0,
                municipality_code: "14103".into(),
                municipality_name: "横浜市鶴見区".into(),
                parent_code: "14100".into(),
                parent_name: "横浜市".into(),
            },
            WardRankingRowDto {
                priority: "C".into(),
                parent_rank: 10,
                parent_total: 18,
                national_rank: 200,
                national_total: 1917,
                thickness_index: 60.0,
                municipality_code: "14104".into(),
                municipality_name: "横浜市港北区".into(),
                parent_code: "14100".into(),
                parent_name: "横浜市".into(),
            },
            WardRankingRowDto {
                priority: "D".into(),
                parent_rank: 18,
                parent_total: 18,
                national_rank: 800,
                national_total: 1917,
                thickness_index: 30.0,
                municipality_code: "14105".into(),
                municipality_name: "横浜市瀬谷区".into(),
                parent_code: "14100".into(),
                parent_name: "横浜市".into(),
            },
        ];
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);

        // S/A/B/C/D 全 class が出ること
        assert!(html.contains("mi-priority-s"), "priority S class");
        assert!(html.contains("mi-priority-a"), "priority A class");
        assert!(html.contains("mi-priority-b"), "priority B class");
        assert!(html.contains("mi-priority-c"), "priority C class");
        assert!(html.contains("mi-priority-d"), "priority D class");

        // priority badge wrapper が出ること
        assert!(html.contains("mi-priority-badge"), "priority badge wrapper");

        // S 行に anchor 🏭 が付くこと
        assert!(html.contains("\u{1F3ED}"), "S priority に anchor バッジ");

        // 厚み指数バー
        assert!(
            html.contains("mi-thickness-bar-wrap"),
            "thickness bar wrapper"
        );
        assert!(html.contains("mi-thickness-bar-fill"), "thickness bar fill");
    }

    #[test]
    fn print_media_keeps_kpi_text_readable() {
        // <style> ブロックに @media print が含まれ、KPI を display:none していないこと
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);

        // @media print ブロックが含まれる
        assert!(html.contains("@media print"), "print 用 CSS ブロックが必須");

        // KPI を print 時に隠していないこと
        assert!(
            !html.contains(".mi-kpi-card { display: none"),
            "print で KPI を非表示にしない"
        );
        assert!(
            !html.contains(".mi-kpi-grid { display: none"),
            "print で KPI grid を非表示にしない"
        );

        // print 時に block fallback (横並び解除) されること
        assert!(
            html.contains(".mi-kpi-grid { display: block"),
            "print 時は block fallback"
        );

        // フッター注記が含まれる (text-only でも読める)
        assert!(html.contains("国勢調査 R2"), "実測注記の text 表現");
        assert!(html.contains("Model F2"), "推定 β 注記の text 表現");
    }

    // ============================================================
    // Round 3 Worker E: 新フィールド (target_thickness_index / cost_index /
    // salary_living_score / distribution_priority / scenario_*_score) 接続テスト
    // ============================================================

    /// Worker B 投入版の MunicipalityRecruitingScore (新フィールドあり) を作成
    fn sample_score_v2(
        code: &str,
        priority_grade: &str,
        thickness: f64,
        salary_living: f64,
        scenario_c: i64,
        scenario_s: i64,
        scenario_a: i64,
    ) -> MunicipalityRecruitingScore {
        MunicipalityRecruitingScore {
            municipality_code: code.into(),
            prefecture: "北海道".into(),
            municipality_name: "札幌市".into(),
            distribution_priority: Some(priority_grade.into()),
            distribution_priority_score: Some(75.0),
            target_thickness_index: Some(thickness),
            salary_living_score: Some(salary_living),
            scenario_conservative_score: Some(scenario_c),
            scenario_standard_score: Some(scenario_s),
            scenario_aggressive_score: Some(scenario_a),
            competitor_job_count: Some(300),
            // 旧 *_population フィールドは fetch SQL で常に None になる前提
            ..Default::default()
        }
    }

    #[test]
    fn kpi_cards_count_priority_a_correctly() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score_v2("01101", "S", 180.0, 70.0, 10, 20, 30),
                sample_score_v2("01102", "A", 142.0, 65.0, 10, 20, 30),
                sample_score_v2("01103", "A", 130.0, 60.0, 10, 20, 30),
                sample_score_v2("01104", "B", 90.0, 50.0, 10, 20, 30),
                sample_score_v2("01105", "C", 50.0, 40.0, 10, 20, 30),
            ],
            ..Default::default()
        };
        render_mi_kpi_cards(&mut html, &data);
        // 配信優先度 A 件数 = 2 (priority "A" のみ)
        assert!(html.contains("配信優先度 A 件数"), "A 件数 KPI ラベル必須");
        assert!(html.contains(">2<"), "A 件数 = 2 が表示されること: {html}");
        // P0 (2026-05-06): KPI 側ラベルを「配信検証候補」にリネーム
        // (ヒーロー Card 1「重点配信候補 (S+A)」と数値矛盾を起こさないため)
        // S/A 計算: priority IN ('S','A') = 3 件 → fallback で score 80+ を使わずそのまま 3
        assert!(html.contains("配信検証候補"), "配信検証候補 KPI ラベル必須");
        assert!(html.contains(">3<"), "配信検証候補 (S+A) = 3");
    }

    #[test]
    fn kpi_cards_average_thickness_index_excludes_null() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score_v2("01101", "A", 100.0, 60.0, 10, 20, 30),
                sample_score_v2("01102", "A", 200.0, 60.0, 10, 20, 30),
                // NULL の thickness は集計から除外される
                MunicipalityRecruitingScore {
                    municipality_code: "01103".into(),
                    target_thickness_index: None,
                    distribution_priority: Some("B".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        render_mi_kpi_cards(&mut html, &data);
        // (100 + 200) / 2 = 150 (NULL 除外、3 で割らない)
        assert!(
            html.contains("150"),
            "thickness 平均 150 が出ること: {html}"
        );
    }

    #[test]
    fn living_cost_panel_uses_real_cost_index_field() {
        // Worker A 投入版の cost_index フィールドが優先されること
        let mut html = String::new();
        let living = vec![LivingCostProxy {
            municipality_code: "01101".into(),
            cost_index: Some(98.5),
            min_wage: Some(960),
            salary_real_terms_proxy: Some(1.05),
            // 旧フィールドは None で良い (SQL から外れる)
            retail_price_index_proxy: None,
            ..Default::default()
        }];
        let scores: Vec<MunicipalityRecruitingScore> = vec![];
        render_mi_living_cost_panel(&mut html, &living, &scores);
        assert!(html.contains("98.5"), "cost_index 値表示: {html}");
        assert!(html.contains("960"), "最低賃金 960 円表示: {html}");
        assert!(html.contains("1.05"), "salary_real_terms_proxy 値表示");
    }

    #[test]
    fn parent_ranking_renders_thickness_bar_from_index() {
        let mut html = String::new();
        let rankings = vec![WardRankingRowDto {
            priority: "A".into(),
            thickness_index: 142.0,
            parent_rank: 3,
            parent_total: 18,
            national_rank: 12,
            national_total: 1917,
            municipality_code: "14103".into(),
            municipality_name: "横浜市鶴見区".into(),
            parent_code: "14100".into(),
            parent_name: "横浜市".into(),
        }];
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);
        // バーラッパー / fill が出力される
        assert!(
            html.contains("mi-thickness-bar-wrap"),
            "thickness バー wrapper"
        );
        assert!(
            html.contains("mi-thickness-bar-fill"),
            "thickness バー fill"
        );
        // バー幅は 142/200*100 = 71% 付近
        assert!(
            html.contains("width:71%") || html.contains("width: 71%"),
            "thickness 71% 幅: {html}"
        );
    }

    #[test]
    fn distribution_ranking_does_not_render_target_population() {
        // 旧 target_population (常に None) を表示しないこと。
        // 数値ありの target_population を渡しても HTML に「対象人口」見出しが出ないこと。
        let mut html = String::new();
        let scores = vec![MunicipalityRecruitingScore {
            municipality_code: "01101".into(),
            prefecture: "北海道".into(),
            municipality_name: "札幌市".into(),
            distribution_priority_score: Some(85.0),
            target_thickness_index: Some(120.0),
            // 旧フィールドに値があってもレンダリングされてはいけない
            target_population: Some(99_999),
            ..Default::default()
        }];
        render_mi_distribution_ranking(&mut html, &scores);
        // 「対象人口」見出しは存在しないこと (置換済み)
        assert!(
            !html.contains("対象人口"),
            "旧『対象人口』見出しは削除されているはず"
        );
        // 厚み指数列が出る
        assert!(
            html.contains("厚み指数"),
            "厚み指数列ヘッダーが追加されていること"
        );
        // 旧 target_population 値が出ないこと (Hard NG: resident estimated_beta で人数表示しない)
        assert!(
            !html.contains("99,999"),
            "target_population 値が表示されてはいけない"
        );
        // 新 thickness 値は表示される
        assert!(html.contains("120.0"), "target_thickness_index 表示");
    }

    #[test]
    fn scenario_columns_use_score_not_population() {
        let mut html = String::new();
        let scores = vec![sample_score_v2("01101", "A", 120.0, 65.0, 42, 60, 80)];
        render_mi_scenario_population_range(&mut html, &scores);
        // 新 column 見出し: 保守シナリオスコア
        assert!(
            html.contains("保守シナリオスコア"),
            "新ヘッダー『保守シナリオスコア』必須"
        );
        assert!(
            html.contains("標準シナリオスコア"),
            "新ヘッダー『標準シナリオスコア』必須"
        );
        assert!(
            html.contains("強気シナリオスコア"),
            "新ヘッダー『強気シナリオスコア』必須"
        );
        // 旧 % 表記は消えていること
        assert!(!html.contains("保守 (1%)"), "旧『保守 (1%)』ヘッダーは削除");
        // scenario_*_score (i64) の値が出ること
        assert!(html.contains(">42<"), "保守スコア 42 表示");
        assert!(html.contains(">60<"), "標準スコア 60 表示");
        assert!(html.contains(">80<"), "強気スコア 80 表示");
    }

    // ============================================================
    // P0: 配信ヒーローバー / colgroup / data-label badge / 印刷 CSS
    // ============================================================

    #[test]
    fn test_p0_hero_bar_renders_three_cards() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 1, 18, 5, 1741)],
            occupation_cells: vec![make_workplace_measured_cell("横浜市鶴見区", 50_000)],
            ..Default::default()
        };
        render_mi_hero_bar(&mut html, &data);
        assert!(html.contains("mi-hero-bar"), "mi-hero-bar セクション必須");
        let card_count = html.matches("mi-hero-card").count();
        // mi-hero-card はカード本体 3 個。mi-hero-primary は 1 個目に追加されるが、
        // class 文字列としては "mi-hero-card mi-hero-primary" の中に "mi-hero-card" を含むため 3 でカウント
        assert!(
            card_count >= 3,
            "hero card 3 枚以上 (実際 {card_count}): {html}"
        );
        assert!(html.contains("重点配信候補"), "Card 1 ラベル");
        assert!(html.contains("市内 1 位市区"), "Card 2 ラベル");
        assert!(
            html.contains("職業人口"),
            "Card 3 (workplace measured) ラベル"
        );
        assert!(
            html.contains("50,000"),
            "workplace measured 合計が 3 桁区切りで表示"
        );
    }

    #[test]
    fn test_p0_hero_bar_third_card_falls_back_when_no_workplace_measured() {
        let mut html = String::new();
        // resident estimated_beta だけ (workplace measured 無し)
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 1, 18, 5, 1741)],
            occupation_cells: vec![make_resident_estimated_beta_cell("横浜市鶴見区", 142.5)],
            ..Default::default()
        };
        render_mi_hero_bar(&mut html, &data);
        // Hard NG 厳守: resident estimated_beta は人数化しない
        assert!(
            !html.contains("人</span>"),
            "fallback 時に '人' 単位を出してはいけない (人数化 NG)"
        );
        assert!(
            html.contains("(\u{6307}\u{6570})"),
            "fallback display should use index label without internal wording"
        );
        // Hard NG 用語混入チェック
        for ng in &[
            "推定人数",
            "想定人数",
            "母集団人数",
            "estimated_population",
            "estimated_worker_count",
            "resident_population_estimate",
            "target_count",
        ] {
            assert!(!html.contains(ng), "Hard NG 用語混入: {ng}");
        }
    }

    #[test]
    fn test_p0_parent_ranking_colgroup_fixes_widths() {
        let mut html = String::new();
        let rankings = vec![make_ranking_row("横浜市鶴見区", 1, 18, 5, 1741)];
        render_mi_parent_ward_ranking(&mut html, &rankings, &[]);
        assert!(html.contains("<colgroup>"), "<colgroup> 必須");
        assert!(html.contains("mi-col-prank"), "市内順位列 class");
        assert!(html.contains("mi-col-name"), "区名列 class");
        assert!(html.contains("mi-col-thick"), "厚み指数列 class");
        assert!(html.contains("mi-col-prio"), "優先度列 class");
        assert!(html.contains("mi-col-nrank"), "全国順位列 class");
        // 主従関係: parent_rank セルは <th scope="row">
        assert!(
            html.contains("scope=\"row\""),
            "市内順位セルは <th scope=\"row\"> で強調"
        );
    }

    #[test]
    fn test_p0_data_label_badge_renders_correct_kind() {
        // P1-1 で 4 種統一 (measured / estimated_beta / reference / insufficient)
        let m = render_mi_data_label_badge("measured");
        let e = render_mi_data_label_badge("estimated_beta");
        let r = render_mi_data_label_badge("reference");
        assert!(m.contains("mi-badge-measured") && m.contains(MEASURED_LABEL));
        assert!(e.contains("mi-badge-estimated-beta") && e.contains(ESTIMATED_LABEL));
        assert!(r.contains("mi-badge-reference") && r.contains(REFERENCE_LABEL));
    }

    // --------------- P1-1: 4 種統一 badge unit tests ---------------

    #[test]
    fn data_label_badge_renders_insufficient_kind() {
        // P1-1: insufficient kind は mi-badge-insufficient + INSUFFICIENT_LABEL を出す
        let html = render_mi_data_label_badge("insufficient");
        assert!(
            html.contains("mi-badge-insufficient"),
            "insufficient kind は mi-badge-insufficient class を出す: {html}"
        );
        assert!(
            html.contains(INSUFFICIENT_LABEL),
            "insufficient ラベルテキスト「該当なし」が含まれること: {html}"
        );
    }

    #[test]
    fn data_label_badge_unknown_kind_returns_empty() {
        // P1-1: 不明 kind は空文字列 (badge 非表示) — 旧 reference fallback は廃止
        let unk = render_mi_data_label_badge("unknown");
        assert!(
            unk.is_empty(),
            "不明な kind は空文字列を返す (バッジ非表示): {unk}"
        );
        // 空 kind も同様
        assert!(render_mi_data_label_badge("").is_empty());
    }

    #[test]
    fn living_cost_panel_shows_insufficient_when_all_null() {
        // P1-1: cost_index / min_wage / salary_real_terms_proxy がすべて None のとき、
        //       各カードに mi-badge-insufficient が表示される
        let mut html = String::new();
        let living: Vec<LivingCostProxy> = vec![];
        let scores: Vec<MunicipalityRecruitingScore> = vec![];
        render_mi_living_cost_panel(&mut html, &living, &scores);

        assert!(
            html.contains("mi-badge-insufficient"),
            "全 NULL 時に insufficient バッジが付与されること: {html}"
        );
        assert!(
            html.contains(INSUFFICIENT_LABEL),
            "「該当なし」ラベルが表示されること: {html}"
        );
        // Hard NG: 人数化禁止は維持
        for forbidden in [
            "推定人数",
            "想定人数",
            "母集団人数",
            "estimated_population",
            "target_count",
        ] {
            assert!(
                !html.contains(forbidden),
                "Hard NG 用語 '{}' が混入",
                forbidden
            );
        }
    }

    #[test]
    fn kpi_card_shows_insufficient_when_value_none() {
        // P1-1: 厚み指数 平均 が None のとき (recruiting_scores も ward_rankings も空)、
        //       insufficient badge が KPI カードに付与される
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_mi_kpi_cards(&mut html, &data);

        assert!(
            html.contains("mi-badge-insufficient"),
            "全データ未投入時に insufficient バッジが KPI に付与されること: {html}"
        );
        // 厚み指数 平均 ラベルは存在
        assert!(html.contains("厚み指数 平均"));
    }

    #[test]
    fn test_p0_print_css_includes_page_break_inside_avoid_and_color_adjust() {
        // MI_STYLE_BLOCK は static const、render_section_market_intelligence で必ず出る
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(
            html.contains("print-color-adjust: exact"),
            "print-color-adjust: exact を主要セクションに付与"
        );
        assert!(
            html.contains("page-break-inside: avoid"),
            "tr の page-break-inside: avoid 必須"
        );
        // hero grid は print でも 3 枚横並び維持
        assert!(
            html.contains("grid-template-columns: repeat(3, 1fr)"),
            "印刷時に hero 3 枚横並び維持"
        );
    }

    // ============================================================
    // P1 B+C+D: 印刷/PDF 統合テスト
    // ============================================================

    /// P1 C: MarketIntelligence variant で印刷向け要約ブロックがレンダされる
    #[test]
    fn print_summary_block_renders_under_mi_variant() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ..Default::default()
        };
        render_section_market_intelligence(&mut html, &data);

        // 要約セクションのマーカー
        assert!(
            html.contains("mi-print-summary"),
            "印刷向け要約 section の class が出力されること"
        );
        assert!(
            html.contains("結論と採用示唆"),
            "印刷向け要約の見出しが出力されること"
        );
        // mi-print-only クラスで画面非表示制御
        assert!(
            html.contains("mi-print-only"),
            "mi-print-only クラスが要約に付与されること"
        );
    }

    /// P1 C: 印刷向け要約は人数表記 (Hard NG) を絶対に出さない
    #[test]
    fn print_summary_does_not_render_population_numbers() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score_v2("01101", "S", 180.0, 70.0, 10, 20, 30),
                sample_score_v2("01102", "A", 142.0, 65.0, 10, 20, 30),
            ],
            ..Default::default()
        };
        let mut summary_html = String::new();
        render_mi_print_summary(&mut summary_html, &data);

        // Hard NG 用語の混入を禁止
        for forbidden in [
            "推定人数",
            "想定人数",
            "母集団人数",
            "候補者が",
            "人見込み",
            "estimated_population",
            "target_count",
        ] {
            assert!(
                !summary_html.contains(forbidden),
                "Hard NG 用語 '{}' が要約に混入: {}",
                forbidden,
                summary_html
            );
        }

        // section 統合テストでも同様にチェック
        render_section_market_intelligence(&mut html, &data);
        for forbidden in ["推定人数", "想定人数", "母集団人数"] {
            assert!(
                !html.contains(forbidden),
                "Hard NG 用語 '{}' が section に混入",
                forbidden
            );
        }
    }

    /// P1 D: 印刷向け注釈ブロックに 5 つの主要凡例が含まれる
    #[test]
    fn print_annotations_lists_5_required_legends() {
        let mut html = String::new();
        render_mi_print_annotations(&mut html);

        // 5 つの必須凡例
        assert!(
            html.contains("workplace measured"),
            "workplace measured 凡例必須"
        );
        assert!(
            html.contains("resident estimated_beta"),
            "resident estimated_beta 凡例必須"
        );
        assert!(html.contains("national_rank"), "national_rank 凡例必須");
        assert!(html.contains("parent_rank"), "parent_rank 凡例必須");
        assert!(html.contains("生活コスト"), "生活コスト 凡例必須");

        // mi-print-only で画面非表示
        assert!(html.contains("mi-print-only"));
        // resident estimated_beta で「人数ではありません」と明記
        assert!(
            html.contains("人数ではありません"),
            "resident estimated_beta が人数ではない旨を明示"
        );
    }

    /// P1 B: mi-print-only / mi-screen-only の表示切替 CSS が定義されている
    #[test]
    fn print_only_class_hidden_on_screen_visible_on_print() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);

        // 通常時 (画面) では mi-print-only は非表示
        assert!(
            html.contains(".mi-print-only { display: none"),
            "通常時に mi-print-only が display: none で隠されること"
        );
        // 通常時 (画面) では mi-screen-only は表示
        assert!(
            html.contains(".mi-screen-only { display: block"),
            "通常時に mi-screen-only が display: block で表示されること"
        );
        // @media print 内で mi-print-only が表示される
        assert!(
            html.contains(".mi-print-only { display: block !important"),
            "@media print で mi-print-only が display: block されること"
        );
        assert!(
            html.contains(".mi-screen-only { display: none !important"),
            "@media print で mi-screen-only が非表示にされること"
        );
    }

    /// P1 B: hero card / kpi card / table tr に break-inside: avoid が適用される
    #[test]
    fn page_break_avoid_applied_to_hero_kpi_card_table_row() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);

        // break-inside: avoid (CSS3 標準) が hero/kpi/print-summary/print-annotations に適用
        assert!(
            html.contains("break-inside: avoid"),
            "break-inside: avoid が CSS に含まれること"
        );
        // table tr の break-inside: avoid (P1 強化版)
        assert!(
            html.contains("table.mi-rank-table tr"),
            "table.mi-rank-table tr セレクタによる行分断防止"
        );
        // 見出し直後の改ページ防止
        assert!(
            html.contains("break-after: avoid"),
            "見出し直後 (h2/h3) の改ページ防止 break-after: avoid"
        );
        // @page A4 portrait
        assert!(
            html.contains("@page") && html.contains("A4 portrait"),
            "@page A4 portrait 設定が含まれること"
        );
    }

    /// P1 C: 要約文が中立表現 (人数を断言しない) を使用していること
    #[test]
    fn print_summary_uses_neutral_phrasing() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![
                sample_score_v2("01101", "S", 180.0, 70.0, 10, 20, 30),
                sample_score_v2("01102", "A", 142.0, 65.0, 10, 20, 30),
            ],
            ..Default::default()
        };
        render_mi_print_summary(&mut html, &data);

        // 中立表現 OK ワード
        assert!(
            html.contains("配信優先度が高い地域です") || html.contains("配信優先度 S/A"),
            "配信優先度の中立表現が含まれること"
        );
        assert!(
            html.contains("推定 β 指数"),
            "常住地ベースが推定 β 指数である旨を明示"
        );
        assert!(
            html.contains("参考指標"),
            "生活コスト等が参考指標である旨を明示"
        );

        // NG 表現 (営業断言調) は禁止
        for forbidden in ["候補者が", "人いま", "人見込み", "確実に", "100%"] {
            assert!(!html.contains(forbidden), "NG 表現 '{}' が混入", forbidden);
        }
    }

    /// P1: Full / Public variant では print summary / annotations が出力されない
    #[test]
    fn full_variant_does_not_include_print_summary_or_annotations() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ..Default::default()
        };

        // Full variant ガード
        let mut full_html = String::new();
        if ReportVariant::Full.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut full_html, &data);
        }
        assert!(
            !full_html.contains("mi-print-summary"),
            "Full variant に印刷要約が混入しないこと"
        );
        assert!(
            !full_html.contains("mi-print-annotations"),
            "Full variant に印刷注釈が混入しないこと"
        );

        // Public variant ガード
        let mut public_html = String::new();
        if ReportVariant::Public.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut public_html, &data);
        }
        assert!(
            !public_html.contains("mi-print-summary"),
            "Public variant に印刷要約が混入しないこと"
        );
        assert!(
            !public_html.contains("mi-print-annotations"),
            "Public variant に印刷注釈が混入しないこと"
        );
    }

    // ============================================================
    // P0 (2026-05-06): 印刷 PDF 内部 fallback 文言除去ガード
    // 客観レビュー C 判定の修正:
    //   - 「データ不足のため特定できませんでした (要件再確認)」
    //   - 「データ準備中」
    //   - 「未集計」「参考表示なし」「本条件では表示対象がありません」「Sample」
    // が render_mi_print_summary / render_mi_hero_bar 出力に出ないことを保証する。
    // ============================================================

    /// 内部 fallback 文言 (NG 7 種) のリスト。テスト共有定数。
    const INTERNAL_FALLBACK_NG: &[&str] = &[
        "データ不足",
        "要件再確認",
        "データ準備中",
        "未集計",
        "参考表示なし",
        "本条件では表示対象がありません",
        "Sample",
    ];

    #[test]
    fn print_summary_does_not_contain_internal_fallback_terms() {
        // S/A 0 件 + 厚み指数なしの最悪ケース (旧実装で「データ不足」「要件再確認」が出ていた)
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_mi_print_summary(&mut html, &data);
        for term in INTERNAL_FALLBACK_NG {
            assert!(
                !html.contains(term),
                "render_mi_print_summary に内部 fallback 文言 '{}' が混入: {}",
                term,
                html
            );
        }
    }

    #[test]
    fn hero_bar_does_not_contain_internal_fallback_terms() {
        // ward_rankings 空 (Card 2 が None) + occupation_cells 空 (Card 3 fallback)
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_mi_hero_bar(&mut html, &data);
        for term in INTERNAL_FALLBACK_NG {
            assert!(
                !html.contains(term),
                "render_mi_hero_bar に内部 fallback 文言 '{}' が混入: {}",
                term,
                html
            );
        }
    }

    #[test]
    fn hero_second_card_does_not_duplicate_insufficient_label() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_mi_hero_bar(&mut html, &data);
        assert!(html.contains("\u{653F}\u{4EE4}\u{5E02}\u{533A}\u{30E9}\u{30F3}\u{30AD}\u{30F3}\u{30B0}\u{5BFE}\u{8C61}\u{5916}"), "Card 2 context should be explanatory: {html}");
        assert_eq!(
            html.matches("\u{8A72}\u{5F53}\u{306A}\u{3057}").count(),
            1,
            "Card 2 should not duplicate neutral empty label: {html}"
        );
    }

    #[test]
    fn print_summary_uses_該当なし_for_zero_priority_sa() {
        // S/A 該当 0 件のとき「該当なし」+ 配信地域ランキング案内を出す
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_mi_print_summary(&mut html, &data);
        assert!(
            html.contains("該当なし"),
            "S/A 0 件のとき『該当なし』表示が必須: {html}"
        );
        assert!(
            html.contains("配信地域ランキング"),
            "S/A 0 件のとき配信地域ランキング案内が必須: {html}"
        );
    }

    #[test]
    fn hero_first_card_and_kpi_use_distinct_labels() {
        // Card 1「重点配信候補 (S + A)」と KPI「配信検証候補 (スコア80+)」が
        // 別ラベルで表示されること (0 件 vs 11 件矛盾の解消)
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData {
            recruiting_scores: vec![sample_score("01101", 85.0, 100, 300, 500)],
            ..Default::default()
        };
        render_mi_hero_bar(&mut html, &data);
        render_mi_kpi_cards(&mut html, &data);

        assert!(
            html.contains("重点配信候補 (S + A)"),
            "ヒーロー Card 1 は『重点配信候補 (S + A)』ラベル: {html}"
        );
        assert!(
            html.contains("配信検証候補"),
            "KPI 側は『配信検証候補』ラベル (スコア 80+ 計算): {html}"
        );
    }

    #[test]
    fn hero_bar_break_inside_avoid_in_print_media() {
        // @media print { .mi-hero-bar { break-inside: avoid; } } を含む
        // (P0 2026-05-06: hero がページまたぎで分断されないこと)
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(
            html.contains(".mi-hero-bar { break-inside: avoid"),
            "@media print 内に .mi-hero-bar の break-inside: avoid が必須: print CSS 抜粋確認"
        );
    }

    /// P1 B (2026-05-06): @page が A4 縦・margin 12mm 14mm を厳密に指定する
    ///
    /// 監査結果 (`docs/PRINT_PDF_P1_ROOT_CAUSE_AUDIT.md`) で下端余白が
    /// 11.4pt (~4mm) になり MI 用 12mm が効いていなかったため、
    /// MI_STYLE_BLOCK の @page 指定を strict に検証する。
    #[test]
    fn page_at_rule_specifies_a4_portrait_with_12mm_margin() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(html.contains("@page"), "@page 宣言が必要");
        assert!(
            html.contains("size: A4 portrait"),
            "@page に size: A4 portrait が必要"
        );
        assert!(
            html.contains("margin: 12mm 14mm"),
            "@page margin は 12mm 14mm (上下 12mm / 左右 14mm) が必要"
        );
    }

    /// P1 B (2026-05-06): 印刷時に html/body の margin/padding が 0 にリセットされる
    ///
    /// 上位 style.rs の `body { padding: 8px 16px }` と @page margin の
    /// 二重インデントを防ぐため、@media print 内で html, body の余白を
    /// 0 !important で上書きする。
    #[test]
    fn print_media_resets_html_body_margin_to_zero() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(
            html.contains("html, body {"),
            "@media print 内に html, body セレクタが必要"
        );
        assert!(
            html.contains("margin: 0 !important"),
            "@media print で html/body の margin: 0 !important が必要"
        );
        assert!(
            html.contains("padding: 0 !important"),
            "@media print で html/body の padding: 0 !important が必要"
        );
    }

    /// P1 (2026-05-06): 注釈ブロックが単独ページ化されないよう
    /// `.mi-print-annotations` に break-before: avoid を強制する。
    ///
    /// 背景: 監査で PDF 19 ページ目が注釈 5 行 + 余白 95% の単独ページに
    /// なっていた。前ページ末尾に統合すべく break-before: avoid を必須化。
    #[test]
    fn print_annotations_has_break_before_avoid_for_compact_layout() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        // @media print 内に .mi-print-annotations セレクタが存在
        assert!(
            html.contains(".mi-print-annotations {"),
            "@media print 内に .mi-print-annotations セレクタが必要"
        );
        // break-before: avoid !important が含まれる
        assert!(
            html.contains("break-before: avoid !important"),
            ".mi-print-annotations に break-before: avoid !important が必要"
        );
        assert!(
            html.contains("page-break-before: avoid !important"),
            ".mi-print-annotations に page-break-before: avoid !important が必要 (旧仕様 fallback)"
        );
    }

    /// P1 (2026-05-06): 注釈ブロックの紙面効率を上げるため、
    /// `@media print` 内で本文 (10.5pt) より小さい 9.5pt に縮小する。
    ///
    /// 背景: 単独ページ化を防ぐ break-before: avoid と組み合わせ、
    /// 前ページ末尾に収まりやすくするためのコンパクト化。
    #[test]
    fn print_annotations_compact_font_size() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        // 印刷用 font-size 9.5pt が含まれる
        assert!(
            html.contains("font-size: 9.5pt"),
            ".mi-print-annotations の印刷時 font-size は 9.5pt (本文 10.5pt より小さい) が必要"
        );
    }

    // ============================================================
    // P1 (2026-05-06): 配信地域ランキング 重複行集約
    // ============================================================

    /// 集約用ヘルパー: 任意の (code, occupation_code, score) で
    /// 不変条件を満たす MunicipalityRecruitingScore を作る。
    fn agg_score(
        code: &str,
        pref: &str,
        muni: &str,
        occ: &str,
        score: f64,
    ) -> MunicipalityRecruitingScore {
        MunicipalityRecruitingScore {
            municipality_code: code.into(),
            prefecture: pref.into(),
            municipality_name: muni.into(),
            occupation_code: occ.into(),
            occupation_name: occ.into(),
            distribution_priority_score: Some(score),
            target_thickness_index: Some(100.0),
            // シナリオ整合: 保守 ≤ 標準 ≤ 強気
            scenario_conservative_population: Some(10),
            scenario_standard_population: Some(20),
            scenario_aggressive_population: Some(30),
            ..Default::default()
        }
    }

    #[test]
    fn ranking_aggregates_duplicate_municipalities_to_single_row() {
        // 同一 municipality_code が 11 件 → 1 行 + 「ほか 10 職種」
        let mut scores = Vec::new();
        for i in 0..11 {
            scores.push(agg_score(
                "13104",
                "東京都",
                "新宿区",
                &format!("occ_{i:02}"),
                50.0 + i as f64,
            ));
        }
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);

        // 自治体名は 1 回だけ <td> 内に出現 (集約後 1 行のため)
        let muni_cell_count = html.matches("東京都 新宿区").count();
        assert_eq!(
            muni_cell_count, 1,
            "新宿区行は集約後 1 行のみ (実際 {} 件): {html}",
            muni_cell_count
        );
        // 「ほか 10 職種」が出力される
        assert!(
            html.contains("ほか 10 職種"),
            "集約済み 10 職種を併記: {html}"
        );
    }

    #[test]
    fn ranking_uses_max_score_row_as_representative() {
        // 同一自治体で score 50/80/30 → 代表は 80 の行 (occ_top)
        let scores = vec![
            agg_score("13101", "東京都", "千代田区", "occ_low", 50.0),
            agg_score("13101", "東京都", "千代田区", "occ_top", 80.0),
            agg_score("13101", "東京都", "千代田区", "occ_mid", 30.0),
        ];
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);

        // 代表職種 occ_top が表示
        assert!(html.contains("occ_top"), "代表職種 occ_top 表示: {html}");
        // 代表スコア 80.0 が表示
        assert!(html.contains("80.0"), "代表スコア 80.0 表示: {html}");
        // ほか 2 職種
        assert!(html.contains("ほか 2 職種"), "ほか 2 職種表示: {html}");
        // 千代田区行は 1 件のみ
        assert_eq!(
            html.matches("東京都 千代田区").count(),
            1,
            "千代田区行は 1 件のみ"
        );
    }

    #[test]
    fn ranking_shows_other_occupation_count_when_above_zero() {
        // group_size=3 → 「ほか 2 職種」表示
        // group_size=1 → 「ほか N 職種」非表示
        let scores = vec![
            // 港区: 3 職種
            agg_score("13103", "東京都", "港区", "occ_a", 90.0),
            agg_score("13103", "東京都", "港区", "occ_b", 70.0),
            agg_score("13103", "東京都", "港区", "occ_c", 60.0),
            // 渋谷区: 1 職種のみ
            agg_score("13113", "東京都", "渋谷区", "occ_d", 40.0),
        ];
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);

        // group_size=3 → 「ほか 2 職種」
        assert!(
            html.contains("ほか 2 職種"),
            "港区: ほか 2 職種表示: {html}"
        );
        // group_size=1 → 「ほか 0 職種」が出てはいけない
        assert!(
            !html.contains("ほか 0 職種"),
            "単独職種で『ほか 0 職種』表示禁止: {html}"
        );
        // 渋谷区は表示されている
        assert!(html.contains("東京都 渋谷区"), "渋谷区行は表示");
    }

    #[test]
    fn ranking_resident_estimated_beta_does_not_render_population() {
        // 集約後も resident estimated_beta セクションでは人数化禁止 (Hard NG)
        let scores = vec![MunicipalityRecruitingScore {
            municipality_code: "01101".into(),
            prefecture: "北海道".into(),
            municipality_name: "札幌市".into(),
            occupation_code: "occ_x".into(),
            occupation_name: "occ_x".into(),
            distribution_priority_score: Some(75.0),
            target_thickness_index: Some(110.0),
            // 旧 target_population に値があってもレンダリングされてはならない
            target_population: Some(99_999),
            scenario_conservative_population: Some(1),
            scenario_standard_population: Some(2),
            scenario_aggressive_population: Some(3),
            ..Default::default()
        }];
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);

        // 旧見出し / 旧値が出ない
        assert!(!html.contains("対象人口"), "対象人口見出しは削除済");
        assert!(!html.contains("99,999"), "target_population 値は表示禁止");
        // Hard NG 用語の混入なし
        assert!(!html.contains("推定人数"), "Hard NG '推定人数' 混入禁止");
        assert!(!html.contains("想定人数"), "Hard NG '想定人数' 混入禁止");
        assert!(
            !html.contains("母集団人数"),
            "Hard NG '母集団人数' 混入禁止"
        );
        // 厚み指数列は表示
        assert!(html.contains("厚み指数"), "厚み指数列ヘッダ");
        assert!(html.contains("110.0"), "thickness 値表示");
    }

    #[test]
    fn ranking_aggregation_preserves_score_descending_order() {
        // 集約後の rows が score 降順
        let scores = vec![
            agg_score("13101", "東京都", "千代田区", "occ_a", 60.0),
            agg_score("13104", "東京都", "新宿区", "occ_b", 90.0),
            agg_score("13103", "東京都", "港区", "occ_c", 75.0),
            // 同一自治体の追加職種 (代表選択をテスト)
            agg_score("13101", "東京都", "千代田区", "occ_a2", 45.0),
        ];
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);

        // 出現順序: 新宿 (90) → 港 (75) → 千代田 (60)
        let pos_shinjuku = html.find("東京都 新宿区").expect("新宿区行存在");
        let pos_minato = html.find("東京都 港区").expect("港区行存在");
        let pos_chiyoda = html.find("東京都 千代田区").expect("千代田区行存在");
        assert!(
            pos_shinjuku < pos_minato,
            "新宿 (90) は 港 (75) より前: shinjuku={pos_shinjuku}, minato={pos_minato}"
        );
        assert!(
            pos_minato < pos_chiyoda,
            "港 (75) は 千代田 (60) より前: minato={pos_minato}, chiyoda={pos_chiyoda}"
        );
    }

    #[test]
    fn living_cost_aggregates_duplicate_municipalities_to_single_row() {
        let mut scores = Vec::new();
        for i in 0..4 {
            let mut score = agg_score(
                "13104",
                "Tokyo",
                "Shinjuku",
                &format!("occ_{i:02}"),
                70.0 + i as f64,
            );
            score.salary_living_score = Some(40.0 + i as f64);
            scores.push(score);
        }

        let mut html = String::new();
        render_mi_salary_living_cost(&mut html, &scores, &[]);

        assert_eq!(
            html.matches("Tokyo Shinjuku").count(),
            1,
            "salary/living table should aggregate duplicate municipality rows: {html}"
        );
        assert!(
            html.contains("\u{307B}\u{304B} 3 \u{8077}\u{7A2E}"),
            "other occupation count should be rendered: {html}"
        );
    }

    #[test]
    fn living_cost_uses_max_salary_living_score_as_representative() {
        let mut low = agg_score("13104", "Tokyo", "Shinjuku", "occ_low", 90.0);
        low.salary_living_score = Some(40.0);
        let mut top = agg_score("13104", "Tokyo", "Shinjuku", "occ_top", 80.0);
        top.salary_living_score = Some(92.0);
        let mut mid = agg_score("13104", "Tokyo", "Shinjuku", "occ_mid", 70.0);
        mid.salary_living_score = Some(65.0);

        let mut html = String::new();
        render_mi_salary_living_cost(&mut html, &[low, top, mid], &[]);

        assert!(
            html.contains("occ_top"),
            "representative occupation should be max salary/living row: {html}"
        );
        assert!(
            html.contains("92.0"),
            "representative score should be rendered: {html}"
        );
        assert!(
            !html.contains("occ_low"),
            "non-representative occupation should not be a separate row: {html}"
        );
    }

    #[test]
    fn living_cost_aggregation_preserves_score_descending_order() {
        let mut a = agg_score("13101", "Tokyo", "Chiyoda", "occ_a", 70.0);
        a.salary_living_score = Some(62.0);
        let mut b = agg_score("13104", "Tokyo", "Shinjuku", "occ_b", 70.0);
        b.salary_living_score = Some(88.0);
        let mut c = agg_score("13103", "Tokyo", "Minato", "occ_c", 70.0);
        c.salary_living_score = Some(75.0);

        let mut html = String::new();
        render_mi_salary_living_cost(&mut html, &[a, b, c], &[]);

        let pos_shinjuku = html.find("Tokyo Shinjuku").expect("Shinjuku exists");
        let pos_minato = html.find("Tokyo Minato").expect("Minato exists");
        let pos_chiyoda = html.find("Tokyo Chiyoda").expect("Chiyoda exists");
        assert!(
            pos_shinjuku < pos_minato,
            "highest score should come first: {html}"
        );
        assert!(
            pos_minato < pos_chiyoda,
            "second score should come before lowest: {html}"
        );
    }

    // ============================================================
    // P2 (2026-05-08): page 25 情報密度改善 - 印刷改ページ制御
    // 配信地域ランキング / 給与・生活コスト比較 / シナリオレンジの 3 表が
    // 同居していた問題を、mi-print-block (分断防止) +
    // mi-print-break-before (独立ページ化) で解消する。
    // ============================================================

    /// 配信地域ランキング section に mi-print-block class が付与され、
    /// 印刷時に表ブロックが途中で分断されないこと。
    #[test]
    fn print_block_class_added_to_distribution_ranking() {
        let scores = vec![agg_score("13104", "Tokyo", "Shinjuku", "occ_a", 75.0)];
        let mut html = String::new();
        render_mi_distribution_ranking(&mut html, &scores);
        assert!(
            html.contains("mi-ranking mi-print-block"),
            "配信地域ランキング section に mi-print-block class が必要: {html}"
        );
        // section 開始タグに含まれていること (内側の table 等ではなく wrapper)
        assert!(
            html.contains("<section class=\"mi-ranking mi-print-block\""),
            "section 開始タグの class 属性に mi-print-block を含めること: {html}"
        );
    }

    /// 給与・生活コスト比較 section に mi-print-break-before class が付与され、
    /// 印刷時に独立ページとして配置されること (page 25 の 3 表混在を回避)。
    #[test]
    fn print_break_before_class_added_to_salary_living_cost() {
        let scores = vec![agg_score("13104", "Tokyo", "Shinjuku", "occ_a", 75.0)];
        let mut html = String::new();
        render_mi_salary_living_cost(&mut html, &scores, &[]);
        assert!(
            html.contains("mi-print-break-before"),
            "給与・生活コスト比較 section に mi-print-break-before class が必要: {html}"
        );
        // 同時に mi-print-block も付いていること (分断防止)
        assert!(
            html.contains("mi-print-block"),
            "給与・生活コスト比較 section にも mi-print-block class が必要: {html}"
        );
        assert!(
            html.contains("<section class=\"mi-living mi-print-block mi-print-break-before\""),
            "section 開始タグの class 属性に両 class を含めること: {html}"
        );
    }

    /// シナリオレンジ section に mi-print-block class が付与されること。
    /// (mi-print-break-before は付けない: 直前の生活コスト直後に配置して page 数を抑える)
    #[test]
    fn print_block_class_added_to_scenario_population_range() {
        let scores = vec![agg_score("13104", "Tokyo", "Shinjuku", "occ_a", 75.0)];
        let mut html = String::new();
        render_mi_scenario_population_range(&mut html, &scores);
        assert!(
            html.contains("mi-scenario mi-print-block"),
            "シナリオレンジ section に mi-print-block class が必要: {html}"
        );
        // section 開始タグの class 属性に mi-print-break-before は付与しない
        // (生活コスト直後に続けて配置 → page 数増加抑制)
        assert!(
            !html.contains("mi-scenario mi-print-block mi-print-break-before"),
            "シナリオレンジには mi-print-break-before を付けない (page 数抑制): {html}"
        );
    }

    /// `@media print` 内に `.mi-print-block` および `.mi-print-break-before`
    /// の対応 CSS rules が含まれていること。
    #[test]
    fn print_block_break_inside_avoid_in_media_print() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        // mi-print-block: 表中分断防止
        assert!(
            html.contains(".mi-print-block {"),
            "@media print 内に .mi-print-block セレクタが必要"
        );
        assert!(
            html.contains("break-inside: avoid !important"),
            ".mi-print-block に break-inside: avoid !important が必要"
        );
        assert!(
            html.contains("page-break-inside: avoid !important"),
            ".mi-print-block に page-break-inside: avoid !important (旧仕様 fallback) が必要"
        );
        // mi-print-break-before: 独立ページ化
        assert!(
            html.contains(".mi-print-break-before {"),
            "@media print 内に .mi-print-break-before セレクタが必要"
        );
        assert!(
            html.contains("break-before: page !important"),
            ".mi-print-break-before に break-before: page !important が必要"
        );
        assert!(
            html.contains("page-break-before: always !important"),
            ".mi-print-break-before に page-break-before: always !important (旧仕様 fallback) が必要"
        );
    }

    // ----- P2-Round6-B: data-mi-section テスト識別子契約 (案 B 実装検証) -----
    //
    // 設計意図 (docs/SPEC_SELECTOR_AUDIT_2026_05_08.md §5 案 B):
    // - class はスタイル用途、`data-mi-section` はテスト/probe 識別子という関心分離
    // - root section に `data-mi-section="market-intelligence"` を付け、E2E spec が
    //   従来探していた架空 `data-section="market-intelligence"` を実体ある属性に置換
    // - 主要 print block にも個別 section 名を付与し、probe で安定取得可能にする
    // - Full / Public variant では section 自体が呼ばれず空 HTML になる (variant guard)
    //   → data-mi-section も自然に出ないこと (negative assertion)
    // - parent-ward-ranking は現 fixture では rankings.is_empty() で early return される
    //   ことが正常仕様 (fixture 政令市未含有時)。逆証明テストで早期 return を保証する。

    /// root section (採用マーケットインテリジェンス wrapper) に
    /// `data-mi-section="market-intelligence"` 属性が付与されていること。
    /// monitoring grep / E2E spec の安定 selector として機能することを保証する。
    #[test]
    fn root_mi_section_has_data_mi_section_attribute() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        assert!(
            html.contains("data-mi-section=\"market-intelligence\""),
            "root section に data-mi-section=\"market-intelligence\" が必要: {html}"
        );
        // root section の class と data 属性が同一タグに付いていること (順序依存しない部分一致)
        assert!(
            html.contains("class=\"mi-root\" data-mi-section=\"market-intelligence\""),
            "root section の class と data-mi-section が同一 <section> 開始タグに含まれること: {html}"
        );
    }

    /// 主要 print block (distribution-ranking / salary-living-cost /
    /// scenario-population-range / print-summary / print-annotations) の各 section に
    /// `data-mi-section="..."` がそれぞれ 1 件以上含まれること。
    #[test]
    fn print_blocks_have_data_mi_section_attribute() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        render_section_market_intelligence(&mut html, &data);
        let expected_blocks = [
            "data-mi-section=\"distribution-ranking\"",
            "data-mi-section=\"salary-living-cost\"",
            "data-mi-section=\"scenario-population-range\"",
            "data-mi-section=\"print-summary\"",
            "data-mi-section=\"print-annotations\"",
        ];
        for marker in &expected_blocks {
            assert!(
                html.contains(marker),
                "印刷主要ブロック '{marker}' が data-mi-section 属性付きで出力されていること: {html}"
            );
        }
    }

    /// Full variant では `render_section_market_intelligence` が呼ばれず
    /// (variant guard `show_market_intelligence_sections() == false`)、
    /// `data-mi-section` 属性がそもそも出力されないこと。
    #[test]
    fn full_variant_does_not_emit_data_mi_section() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![make_workplace_measured_cell("横浜市鶴見区", 12_345)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };

        let mut html = String::new();
        if ReportVariant::Full.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut html, &data);
        }
        assert!(
            !html.contains("data-mi-section="),
            "Full variant に data-mi-section 属性が混入してはならない: {html}"
        );
    }

    /// Public variant でも同様に `data-mi-section` が出力されないこと。
    #[test]
    fn public_variant_does_not_emit_data_mi_section() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![make_workplace_measured_cell("横浜市鶴見区", 12_345)],
            ward_rankings: vec![make_ranking_row("横浜市鶴見区", 3, 18, 12, 1917)],
            ..Default::default()
        };

        let mut html = String::new();
        if ReportVariant::Public.show_market_intelligence_sections() {
            render_section_market_intelligence(&mut html, &data);
        }
        assert!(
            !html.contains("data-mi-section="),
            "Public variant に data-mi-section 属性が混入してはならない: {html}"
        );
    }

    /// `render_mi_parent_ward_ranking` は rankings 空入力で early return し、
    /// 一切出力しないこと (現 fixture では政令市データ非空時のみ実体出力される正常仕様)。
    /// 逆証明: `data-mi-section="parent-ward-ranking"` も `mi-parent-ward-ranking` も
    /// 空入力では出ないことで、fixture 由来の 0 件が「dead code 起因の漏れ」ではなく
    /// 「条件付き出力の正常 skip」であることを担保する。
    #[test]
    fn parent_ward_ranking_zero_rows_is_valid_for_current_fixture() {
        let mut html = String::new();
        render_mi_parent_ward_ranking(&mut html, &[], &[]);
        assert!(
            html.is_empty(),
            "rankings 空入力では parent-ward-ranking section は一切出力されないこと: {html}"
        );
        assert!(
            !html.contains("data-mi-section=\"parent-ward-ranking\""),
            "空入力で data-mi-section が出てはならない (early return 保証): {html}"
        );
        assert!(
            !html.contains("mi-parent-ward-ranking"),
            "空入力で mi-parent-ward-ranking class も出てはならない: {html}"
        );
    }
}
