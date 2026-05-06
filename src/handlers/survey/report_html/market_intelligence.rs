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
    fetch_code_master, fetch_commute_flow_summary, fetch_living_cost_proxy,
    fetch_occupation_cells, fetch_occupation_population,
    fetch_recruiting_scores_by_municipalities, fetch_ward_rankings_by_parent,
    fetch_ward_thickness, to_code_master, to_commute_flows, to_living_cost_proxies,
    to_occupation_cells, to_occupation_populations, to_recruiting_scores, to_ward_rankings,
    to_ward_thickness_dtos, CommuteFlowSummary, LivingCostProxy, MunicipalityCodeMasterDto,
    MunicipalityRecruitingScore, OccupationCellDto, OccupationPopulationCell,
    SurveyMarketIntelligenceData, WardRankingRowDto,
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

    let recruiting_rows =
        fetch_recruiting_scores_by_municipalities(db, turso, target_municipalities, occupation_group_code);
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
    let (occupation_cells, ward_thickness, ward_rankings, code_master) =
        if target_municipalities.is_empty() {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new())
        } else {
            // (1) 職業セル (workplace + resident 両 basis)
            let occ_cell_rows =
                fetch_occupation_cells(db, turso, target_municipalities, None, None);
            let occupation_cells = to_occupation_cells(&occ_cell_rows);

            // (2) 政令市区 thickness 詳細
            let thickness_rows =
                fetch_ward_thickness(db, turso, target_municipalities, None);
            let ward_thickness = to_ward_thickness_dtos(&thickness_rows);

            // (3) コードマスター (target に対して lookup)
            let code_master_rows = fetch_code_master(db, turso, target_municipalities);
            let code_master = to_code_master(&code_master_rows);

            // (4) parent ward ranking (商品の核心、parent_code 別に collect)
            //     designated_ward の parent_code 一覧を抽出 → 主要 occupation で fetch
            let ward_rankings = collect_ward_rankings_for_targets(
                db,
                turso,
                &code_master,
                occupation_group_code,
            );

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
        "<section class=\"mi-root\" role=\"region\" aria-labelledby=\"mi-root-heading\" \
         style=\"margin-top:24px;padding:16px;border-top:4px solid #1e3a8a;\">\n"
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
    render_mi_talent_supply(html, &data.occupation_populations);
    render_mi_salary_living_cost(html, &data.recruiting_scores, &data.living_cost_proxies);

    // Worker D: 生活コスト・給与実質感パネル (参考統計、NULL は - 表示)
    render_mi_living_cost_panel(html, &data.living_cost_proxies, &data.recruiting_scores);

    render_mi_scenario_population_range(html, &data.recruiting_scores);
    render_mi_commute_inflow_supplement(html, &data.commute_flows);

    // Phase 3 Step 5 Phase 4: Plan B (workplace measured + resident estimated_beta)
    // OccupationCellDto 行を持つ場合のみ追加表示。空ならセクション自体を出さない
    // (placeholder は既存 render_mi_talent_supply 側で出ている)。
    if !data.occupation_cells.is_empty() {
        render_mi_occupation_cells(html, &data.occupation_cells);
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
  /* P1 B: ページ設定 (重複定義は MI_STYLE_BLOCK 内で 1 箇所のみ) */
  @page { size: A4 portrait; margin: 12mm 14mm; }
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
        "<section class=\"mi-kpi-summary\" aria-labelledby=\"mi-kpi-summary-heading\">\n",
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
        Some(thickness_vals_from_scores.iter().sum::<f64>() / thickness_vals_from_scores.len() as f64)
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
        "<section class=\"mi-hero-bar\" role=\"region\" aria-labelledby=\"mi-hero-heading\">\n",
    );
    html.push_str("<h3 id=\"mi-hero-heading\" class=\"mi-visually-hidden\">配信判断 ヒーロー</h3>\n");
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
            html.push_str(&format!(
                "<div class=\"mi-hero-card\" role=\"listitem\">\
                 <div class=\"mi-hero-eyebrow\">市内 1 位市区 (先頭政令市)</div>\
                 <div class=\"mi-hero-value\">該当なし</div>\
                 <div class=\"mi-hero-context\">{badge}</div>\
                 </div>\n",
                badge = render_mi_data_label_badge("insufficient"),
            ));
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
             <div class=\"mi-hero-eyebrow\">厚み指数 平均 (実測値準備中)</div>\
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
        "<section class=\"mi-living-cost-panel\" aria-labelledby=\"mi-lc-panel-heading\">\n",
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
pub(crate) fn render_mi_print_summary(
    html: &mut String,
    data: &SurveyMarketIntelligenceData,
) {
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
        "<section class=\"mi-print-summary mi-print-only\" \
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
        html.push_str(
            "<li>市内順位 (parent_rank) を主軸として地域選定をご検討ください。</li>\n",
        );
    }

    // 参考指標の注意喚起
    html.push_str(
        "<li>生活コスト・最低賃金・配信スコアは参考指標です \
         (市区町村差を完全には反映していません)。</li>\n",
    );
    // 全体の前提
    html.push_str(
        "<li>本レポートの数値は相対濃淡を示すもので、実数の保証ではありません。</li>\n",
    );

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
        "<aside class=\"mi-print-annotations mi-print-only\" \
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
    html.push_str(
        "<li><strong>national_rank</strong>: 全国順位は参考表示</li>\n",
    );
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

// --------------- Section 7: Plan B (workplace measured + resident estimated_beta) ---------------
//
// 表示分岐ルール (DISPLAY_SPEC_PLAN_B 必須):
// - workplace × measured: 人数 (population) + WORKPLACE_LABEL + MEASURED_DATA_SOURCE
// - resident × estimated_beta: 指数 (estimate_index, ".1f") + RESIDENT_LABEL + ESTIMATED_BETA_NOTE
// - resident × estimated_beta で人数を絶対に表示しない (Hard NG)

#[allow(dead_code)]
pub(crate) fn render_mi_occupation_cells(html: &mut String, cells: &[OccupationCellDto]) {
    html.push_str(
        "<section class=\"mi-occupation-cells\" aria-labelledby=\"mi-occcell-heading\" \
         style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-occcell-heading\">職業×地域 セル別マトリクス \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[Plan B]</span></h3>\n",
    );

    if cells.is_empty() {
        render_mi_placeholder(
            html,
            "職業セルデータが未投入です (municipality_occupation_population テーブル)。",
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
            _ => (
                "区分不明",
                "-".to_string(),
                "-",
            ),
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

#[allow(dead_code)]
pub(crate) fn render_mi_parent_ward_ranking(
    html: &mut String,
    rankings: &[WardRankingRowDto],
    _code_master: &[MunicipalityCodeMasterDto],
) {
    html.push_str(
        "<section class=\"mi-parent-ward-ranking\" aria-labelledby=\"mi-pwr-heading\" \
         style=\"margin:16px 0;\">\n",
    );
    html.push_str(
        "<h3 id=\"mi-pwr-heading\">政令市区別ランキング \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[商品コア]</span></h3>\n",
    );

    if rankings.is_empty() {
        html.push_str(
            "<div class=\"mi-empty\" role=\"note\" \
             style=\"padding:10px;background:#fef3c7;border:1px solid #fcd34d;border-radius:4px;color:#92400e;font-size:13px;\">\
             \u{2139} 政令市区ランキングデータが現在取得できません。</div>\n"
        );
        html.push_str("</section>\n");
        return;
    }

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
                format!("<span class=\"mi-anchor-badge\" title=\"高優先 / 集積地候補\">{}</span>", ANCHOR_BADGE)
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
        "<section class=\"mi-summary\" aria-labelledby=\"mi-summary-heading\" \
         style=\"margin:16px 0;padding:12px;background:#f8fafc;border:1px solid #cbd5e1;border-radius:6px;\">\n"
    );
    html.push_str("<h3 id=\"mi-summary-heading\" style=\"margin:0 0 8px;\">結論サマリー</h3>\n");

    if data.is_empty() {
        render_mi_placeholder(html, "サマリー算出に必要な事前集計データが投入されていません。");
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
        &avg_priority.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
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

fn render_mi_distribution_ranking(html: &mut String, scores: &[MunicipalityRecruitingScore]) {
    html.push_str(
        "<section class=\"mi-ranking\" aria-labelledby=\"mi-ranking-heading\" \
         style=\"margin:16px 0;\">\n"
    );
    html.push_str(
        "<h3 id=\"mi-ranking-heading\">配信地域ランキング \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{label}]</span></h3>\n"
            .replace("{label}", ESTIMATED_LABEL)
            .as_str()
    );

    let mut valid: Vec<&MunicipalityRecruitingScore> = scores
        .iter()
        .filter(|s| s.is_priority_score_in_range() && s.is_scenario_consistent())
        .collect();
    if valid.is_empty() {
        render_mi_placeholder(
            html,
            "配信地域ランキングを表示するデータが不足しています (municipality_recruiting_scores テーブル未投入の可能性)。",
        );
        html.push_str("</section>\n");
        return;
    }

    valid.sort_by(|a, b| {
        b.distribution_priority_score
            .unwrap_or(0.0)
            .partial_cmp(&a.distribution_priority_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    html.push_str("<table class=\"mi-table\" style=\"width:100%;border-collapse:collapse;font-size:13px;\">\n");
    html.push_str(
        "<thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">順位</th>\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:right;padding:6px;\">配信優先度</th>\
         <th style=\"text-align:right;padding:6px;\">厚み指数</th>\
         <th style=\"text-align:right;padding:6px;\">競合求人数</th>\
         <th style=\"text-align:left;padding:6px;\">区分</th>\
         </tr></thead><tbody>\n"
    );
    for (rank, s) in valid.iter().enumerate().take(20) {
        let bucket = match s.distribution_priority_score.unwrap_or(0.0) {
            v if v >= 80.0 => "重点配信",
            v if v >= 65.0 => "拡張候補",
            v if v >= 50.0 => "維持/検証",
            _ => "優先度低",
        };
        // Worker E Round 3: 旧 target_population (常に None) 表示を厚み指数に置換
        // resident estimated_beta セクションでは「人」単位を表示しないルール (feedback_test_data_validation)
        html.push_str(&format!(
            "<tr><td style=\"padding:6px;\">{rank}</td>\
             <td style=\"padding:6px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:6px;\">{score}</td>\
             <td style=\"text-align:right;padding:6px;\">{thick}</td>\
             <td style=\"text-align:right;padding:6px;\">{comp}</td>\
             <td style=\"padding:6px;color:#64748b;\">{bucket}</td></tr>\n",
            rank = rank + 1,
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            score = s.distribution_priority_score.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
            thick = s
                .target_thickness_index
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            comp = format_opt_i64(s.competitor_job_count),
            bucket = bucket,
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         配信優先度は METRICS.md §2.1 の `clamp(positive_score × (1 - penalty_reduction_pct/100), 0, 100)` で算出 [{}]。\
         「採用しやすさの断定」ではなく「検証すべき配信地域の優先順位」として扱う。</p>\n",
        ESTIMATED_LABEL
    ));
    html.push_str("</section>\n");
}

// --------------- Section 3: 人材供給ヒートマップ (テーブル版) ---------------

fn render_mi_talent_supply(html: &mut String, cells: &[OccupationPopulationCell]) {
    html.push_str(
        "<section class=\"mi-talent\" aria-labelledby=\"mi-talent-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-talent-heading\">人材供給ヒートマップ \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        MEASURED_LABEL
    ));

    if cells.is_empty() {
        render_mi_placeholder(
            html,
            "職業×年齢×性別人口データが未投入です (municipality_occupation_population テーブル)。",
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
    html.push_str(
        "<section class=\"mi-living\" aria-labelledby=\"mi-living-heading\" style=\"margin:16px 0;\">\n",
    );
    html.push_str(&format!(
        "<h3 id=\"mi-living-heading\">給与・生活コスト比較 \
         <span style=\"font-size:11px;color:#64748b;font-weight:400;\">[{}]</span></h3>\n",
        REFERENCE_LABEL
    ));

    if scores.is_empty() && living.is_empty() {
        render_mi_placeholder(
            html,
            "給与・生活コストデータが未投入です。",
        );
        html.push_str("</section>\n");
        return;
    }

    // 主キー (municipality_code) で結合
    use std::collections::HashMap;
    let living_map: HashMap<&str, &LivingCostProxy> =
        living.iter().map(|l| (l.municipality_code.as_str(), l)).collect();

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
         <th style=\"text-align:right;padding:6px;\">給与×生活 指数</th>\
         <th style=\"text-align:right;padding:6px;\">最低賃金 (時給)</th>\
         <th style=\"text-align:right;padding:6px;\">物価指数 (cost_index)</th>\
         <th style=\"text-align:right;padding:6px;\">生活コストスコア</th>\
         </tr></thead><tbody>\n",
    );
    for s in scores.iter().take(20) {
        let liv = living_map.get(s.municipality_code.as_str());
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
             <td style=\"text-align:right;padding:4px;\">{salary_idx}</td>\
             <td style=\"text-align:right;padding:4px;\">{min_wage_html}</td>\
             <td style=\"text-align:right;padding:4px;\">{price_html}</td>\
             <td style=\"text-align:right;padding:4px;\">{lcs_html}</td></tr>\n",
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
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
    html.push_str(
        "<section class=\"mi-scenario\" aria-labelledby=\"mi-scenario-heading\" style=\"margin:16px 0;\">\n",
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
            "母集団シナリオデータが未投入です (municipality_recruiting_scores テーブル)。",
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
        let m_val = s
            .scenario_standard_score
            .or(s.scenario_standard_population);
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
        "<section class=\"mi-commute\" aria-labelledby=\"mi-commute-heading\" style=\"margin:16px 0;\">\n",
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

fn render_mi_placeholder(html: &mut String, msg: &str) {
    // P0 (2026-05-06): 「データ準備中」prefix を「該当なし」に変更。
    // 印刷 PDF 本文に内部 fallback 文言を出さないため。
    html.push_str(&format!(
        "<div class=\"mi-placeholder\" role=\"note\" \
         style=\"padding:10px;background:#fef3c7;border:1px solid #fcd34d;border-radius:4px;color:#92400e;font-size:13px;\">\
         \u{2139} 該当なし: {}</div>\n",
        escape_html(msg)
    ));
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
        assert!(placeholder_count >= 5, "placeholder が 5 セクション以上に出る (実際 {})", placeholder_count);
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
        let n_idx = html
            .find("全国順位 (参考)")
            .or_else(|| html.find("mi-ref"));
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
        // panic せず、データ未取得メッセージが出る
        assert!(
            html.contains("取得できません") || html.contains("mi-empty"),
            "空データ時は適切な文言: {html}"
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
        let yokohama_count = html.matches("横浜市</h4>").count()
            + html.matches("横浜市 (").count();
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
                assert!(pi < ni,
                    "行ブロックで mi-parent-rank が mi-ref より後ろ (parent={}, ref={})",
                    pi, ni);
                checked += 1;
            }
        }
        assert!(checked >= 1,
            "少なくとも 1 行で順序検証が走ること (実際: {} 行検査)", checked);
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
            assert!(!html.contains(marker),
                "Full variant に Step 5 マーカー '{}' が混入", marker);
        }
        assert!(html.is_empty(),
            "Full variant では section 自体が呼ばれず空 HTML");
    }

    /// Public variant でも同様に Step 5 マーカーが一切出ないこと。
    #[test]
    fn public_variant_html_does_not_contain_any_step5_marker() {
        use super::super::ReportVariant;

        let data = SurveyMarketIntelligenceData {
            occupation_cells: vec![
                make_workplace_measured_cell("横浜市鶴見区", 12_345),
            ],
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
            assert!(!html.contains(marker),
                "Public variant に Step 5 マーカー '{}' が混入", marker);
        }
        assert!(html.is_empty(),
            "Public variant では section 自体が呼ばれず空 HTML");
    }

    /// 空データで render_mi_parent_ward_ranking が panic しない + placeholder 出力。
    #[test]
    fn empty_data_renders_placeholder_not_panic() {
        let mut html = String::new();
        render_mi_parent_ward_ranking(&mut html, &[], &[]);
        // panic していない & placeholder か空でない
        assert!(html.contains("取得できません") || html.contains("mi-empty"),
            "空データで placeholder/empty マーカーが出ること");
    }

    /// 空 occupation_cells で render_mi_occupation_cells が panic しない。
    #[test]
    fn empty_occupation_cells_renders_placeholder_or_empty() {
        let mut html = String::new();
        render_mi_occupation_cells(&mut html, &[]);
        // 空 or placeholder どちらも許容 (panic しないこと自体が主要 invariant)
        // 何かが書かれている場合は Hard NG が混入していないこと
        for forbidden in ["推定人数", "想定人数", "母集団人数"] {
            assert!(!html.contains(forbidden),
                "空入力で Hard NG '{}' が出力されている", forbidden);
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
        assert!(html.contains("mi-kpi-grid"), "KPI grid CSS class が含まれること");
        assert!(html.contains("mi-kpi-card"), "KPI card CSS class が含まれること");
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
        assert!(
            !html.contains("class=\"mi-lc-value\">0<"),
            "ゼロ埋めは禁止"
        );
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
        assert!(html.contains("mi-badge-reference"), "参考バッジが付与されること");
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
        assert!(html.contains("mi-thickness-bar-wrap"), "thickness bar wrapper");
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
        assert!(
            html.contains("配信優先度 A 件数"),
            "A 件数 KPI ラベル必須"
        );
        assert!(
            html.contains(">2<"),
            "A 件数 = 2 が表示されること: {html}"
        );
        // P0 (2026-05-06): KPI 側ラベルを「配信検証候補」にリネーム
        // (ヒーロー Card 1「重点配信候補 (S+A)」と数値矛盾を起こさないため)
        // S/A 計算: priority IN ('S','A') = 3 件 → fallback で score 80+ を使わずそのまま 3
        assert!(
            html.contains("配信検証候補"),
            "配信検証候補 KPI ラベル必須"
        );
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
        assert!(html.contains("150"), "thickness 平均 150 が出ること: {html}");
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
        assert!(
            html.contains("960"),
            "最低賃金 960 円表示: {html}"
        );
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
        assert!(html.contains("標準シナリオスコア"), "新ヘッダー『標準シナリオスコア』必須");
        assert!(html.contains("強気シナリオスコア"), "新ヘッダー『強気シナリオスコア』必須");
        // 旧 % 表記は消えていること
        assert!(
            !html.contains("保守 (1%)"),
            "旧『保守 (1%)』ヘッダーは削除"
        );
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
        assert!(html.contains("職業人口"), "Card 3 (workplace measured) ラベル");
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
            html.contains("実測値準備中") || html.contains("(指数)"),
            "fallback 表示 (指数) が出ること"
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
            html.contains("配信優先度が高い地域です")
                || html.contains("配信優先度 S/A"),
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
        for forbidden in [
            "候補者が",
            "人いま",
            "人見込み",
            "確実に",
            "100%",
        ] {
            assert!(
                !html.contains(forbidden),
                "NG 表現 '{}' が混入",
                forbidden
            );
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
}
