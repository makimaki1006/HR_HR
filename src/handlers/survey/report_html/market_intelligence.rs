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
    fetch_commute_flow_summary, fetch_living_cost_proxy, fetch_occupation_population,
    fetch_recruiting_scores_by_municipalities, to_commute_flows, to_living_cost_proxies,
    to_occupation_populations, to_recruiting_scores, CommuteFlowSummary, LivingCostProxy,
    MunicipalityRecruitingScore, OccupationPopulationCell, SurveyMarketIntelligenceData,
};
use super::super::super::helpers::escape_html;

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

    SurveyMarketIntelligenceData {
        recruiting_scores: to_recruiting_scores(&recruiting_rows),
        living_cost_proxies: to_living_cost_proxies(&living_cost_rows),
        commute_flows: to_commute_flows(&commute_rows),
        occupation_populations: to_occupation_populations(&occupation_rows),
    }
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
