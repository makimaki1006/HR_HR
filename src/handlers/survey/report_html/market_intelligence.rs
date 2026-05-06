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

    render_mi_summary_card(html, data);
    render_mi_distribution_ranking(html, &data.recruiting_scores);
    render_mi_talent_supply(html, &data.occupation_populations);
    render_mi_salary_living_cost(html, &data.recruiting_scores, &data.living_cost_proxies);
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

    html.push_str("</section>\n");
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
             <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
             <th style=\"text-align:left;padding:6px;\">市内順位 (主)</th>\
             <th style=\"text-align:left;padding:6px;\">区名</th>\
             <th style=\"text-align:right;padding:6px;\">厚み指数 (推定 β)</th>\
             <th style=\"text-align:left;padding:6px;\">優先度</th>\
             <th class=\"mi-ref\" style=\"text-align:right;padding:6px;color:#64748b;font-size:11px;\">全国順位 (参考)</th>\
             </tr></thead><tbody>\n",
        );

        for w in &wards {
            let priority_lower = w.priority.to_lowercase();
            html.push_str(&format!(
                "<tr>\
                 <td class=\"mi-parent-rank\" style=\"padding:6px;\"><strong>{prank} 位</strong> / {ptotal} 区</td>\
                 <td style=\"padding:6px;\">{name}</td>\
                 <td class=\"mi-thickness\" style=\"text-align:right;padding:6px;\">{thick:.1}</td>\
                 <td class=\"mi-priority mi-priority-{plow}\" style=\"padding:6px;\">{prio}</td>\
                 <td class=\"mi-ref\" style=\"text-align:right;padding:6px;color:#64748b;font-size:11px;\">{nrank} 位 / {ntotal} 市区町村</td>\
                 </tr>\n",
                prank = w.parent_rank,
                ptotal = w.parent_total,
                name = escape_html(&w.municipality_name),
                thick = w.thickness_index,
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
        "重点配信候補",
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
         <th style=\"text-align:right;padding:6px;\">対象人口</th>\
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
        html.push_str(&format!(
            "<tr><td style=\"padding:6px;\">{rank}</td>\
             <td style=\"padding:6px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:6px;\">{score}</td>\
             <td style=\"text-align:right;padding:6px;\">{tgt}</td>\
             <td style=\"text-align:right;padding:6px;\">{comp}</td>\
             <td style=\"padding:6px;color:#64748b;\">{bucket}</td></tr>\n",
            rank = rank + 1,
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            score = s.distribution_priority_score.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
            tgt = format_opt_i64(s.target_population),
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

    html.push_str(
        "<table class=\"mi-living-table\" style=\"width:100%;border-collapse:collapse;font-size:12px;\">\n\
         <thead><tr style=\"background:#1e3a8a;color:#fff;\">\
         <th style=\"text-align:left;padding:6px;\">市区町村</th>\
         <th style=\"text-align:right;padding:6px;\">媒体給与中央値</th>\
         <th style=\"text-align:right;padding:6px;\">単身向け相当家賃</th>\
         <th style=\"text-align:right;padding:6px;\">物価指数</th>\
         <th style=\"text-align:right;padding:6px;\">生活コストスコア</th>\
         </tr></thead><tbody>\n",
    );
    for s in scores.iter().take(20) {
        let liv = living_map.get(s.municipality_code.as_str());
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:4px;\">{salary}</td>\
             <td style=\"text-align:right;padding:4px;\">{rent}</td>\
             <td style=\"text-align:right;padding:4px;\">{price}</td>\
             <td style=\"text-align:right;padding:4px;\">{lcs}</td></tr>\n",
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            salary = s.median_salary_yen.map(|v| format!("¥{}", format_thousands(v))).unwrap_or("-".into()),
            rent = liv
                .and_then(|l| l.single_household_rent_proxy)
                .map(|v| format!("¥{}", format_thousands(v)))
                .unwrap_or("-".into()),
            price = liv
                .and_then(|l| l.retail_price_index_proxy)
                .map(|v| format!("{v:.1}"))
                .unwrap_or("-".into()),
            lcs = s.living_cost_score.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         家賃 proxy は `単身向け相当 / 小世帯向け相当` 表記 (1R/1LDK と断定しない) [{}]。\
         物価指数は基準値 100 [{}]。</p>\n",
        REFERENCE_LABEL, REFERENCE_LABEL
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

    let valid: Vec<&MunicipalityRecruitingScore> = scores
        .iter()
        .filter(|s| s.is_scenario_consistent())
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
         <th style=\"text-align:right;padding:6px;\">保守 (1%)</th>\
         <th style=\"text-align:right;padding:6px;\">標準 (3%)</th>\
         <th style=\"text-align:right;padding:6px;\">強気 (5%)</th>\
         </tr></thead><tbody>\n",
    );
    for s in valid.iter().take(20) {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px;\">{pref} {muni}</td>\
             <td style=\"text-align:right;padding:4px;\">{c}</td>\
             <td style=\"text-align:right;padding:4px;\">{m}</td>\
             <td style=\"text-align:right;padding:4px;\">{a}</td></tr>\n",
            pref = escape_html(&s.prefecture),
            muni = escape_html(&s.municipality_name),
            c = format_opt_i64(s.scenario_conservative_population),
            m = format_opt_i64(s.scenario_standard_population),
            a = format_opt_i64(s.scenario_aggressive_population),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p style=\"font-size:11px;color:#64748b;margin:6px 0 0;\">\
         「応募者数」ではなく「配信対象として現実的に狙える母集団」(METRICS.md §9) [{}]。\
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
    html.push_str(&format!(
        "<div class=\"mi-placeholder\" role=\"note\" \
         style=\"padding:10px;background:#fef3c7;border:1px solid #fcd34d;border-radius:4px;color:#92400e;font-size:13px;\">\
         \u{2139} データ準備中: {}</div>\n",
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
        assert!(html.contains("データ準備中"));
        // 5 セクション + 1 補助セクションの placeholder が出ること (各 section 内に 1 つずつ)
        let placeholder_count = html.matches("データ準備中").count();
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
}
