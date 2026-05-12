//! 分割: report_html/region.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref, Row};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, MunicipalitySalaryAgg, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;
use super::region_filter::filter_municipalities_by_pref;

pub(super) fn render_section_region(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（都道府県）</h2>\n");
    // So What 行: 件数の多い都道府県と割合を 1 行で提示
    if let Some((top_pref, top_count)) = agg.by_prefecture.first() {
        let pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多いのは「{}」で全体の {:.1}%（件数の多い順に整理）。</p>\n",
            escape_html(top_pref),
            pct
        ));
    }
    html.push_str(
        "<p class=\"section-xref\">関連: Section 7（市区町村）/ Section 8（最低賃金）</p>\n",
    );

    render_figure_caption(html, "表 6-1", "都道府県別 求人件数 Top 10");
    html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr></thead>\n<tbody>\n");
    let total = agg.total_count.max(1);
    for (i, (pref, count)) in agg.by_prefecture.iter().take(10).enumerate() {
        let pct = *count as f64 / total as f64 * 100.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            escape_html(pref),
            format_number(*count as i64),
            pct,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 残りの都道府県数を注記
    if agg.by_prefecture.len() > 10 {
        html.push_str(&format!(
            "<p class=\"note\">※ 他{}都道府県のデータは省略しています</p>\n",
            agg.by_prefecture.len() - 10
        ));
    }

    // 簡易ヒートマップ（Top 10 都道府県を 4 段階の濃度で可視化）
    if let Some((_, top_count)) = agg.by_prefecture.first() {
        let max_c = (*top_count as f64).max(1.0);
        render_figure_caption(
            html,
            "図 6-1",
            "都道府県別 求人件数ヒートマップ（Top 10、濃度=件数）",
        );
        html.push_str(
            "<div class=\"heatmap-grid\" role=\"img\" aria-label=\"都道府県別件数ヒートマップ\">\n",
        );
        for (pref, count) in agg.by_prefecture.iter().take(10) {
            let ratio = *count as f64 / max_c;
            let cls = if ratio >= 0.75 {
                "h-4"
            } else if ratio >= 0.5 {
                "h-3"
            } else if ratio >= 0.25 {
                "h-2"
            } else {
                "h-1"
            };
            html.push_str(&format!(
                "<div class=\"heatmap-cell {}\"><div style=\"font-weight:700;\">{}</div>\
                 <div style=\"font-size:8pt;\">{}件</div></div>\n",
                cls,
                escape_html(pref),
                format_number(*count as i64),
            ));
        }
        // 余白を埋める（10 セル未満の場合）
        let placeholder_count = 10usize.saturating_sub(agg.by_prefecture.len());
        for _ in 0..placeholder_count {
            html.push_str("<div class=\"heatmap-cell h-empty\">-</div>\n");
        }
        html.push_str("</div>\n");
        html.push_str(
            "<div class=\"heatmap-legend\">濃度: \
            <span class=\"swatch\" style=\"background:#dbeafe\"></span>少 \
            <span class=\"swatch\" style=\"background:#93c5fd\"></span> \
            <span class=\"swatch\" style=\"background:#3b82f6\"></span> \
            <span class=\"swatch\" style=\"background:#1e40af\"></span>多</div>\n",
        );
        render_read_hint(
            html,
            "色の濃い県ほど求人件数が多く、当 CSV データのカバレッジ重心です。色が薄い県は本レポートでの統計信頼性が下がる点に留意してください。",
        );
    }

    render_section_bridge(
        html,
        "次セクションでは、都道府県を市区町村レベルに掘り下げ、給与水準を併せて確認します。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// Impl-1 (2026-04-26): 媒体分析データ活用 #18 / D-3 / D-4 印刷版
// 配置: Section 6 (都道府県) と Section 7 (市区町村) の間に
//       「地域特性 補足」を追加。データソース: InsightContext のみで完結。
// =====================================================================

const REGION_EXTRAS_NATIONAL_AGING_PCT: f64 = 29.0;

fn classify_habitable_density(density: f64) -> &'static str {
    if density >= 5_000.0 {
        "都市型"
    } else if density >= 1_000.0 {
        "中間型"
    } else {
        "郊外型"
    }
}

fn calc_aging_rate_from_pyramid(pyramid: &[Row]) -> f64 {
    if pyramid.is_empty() {
        return 0.0;
    }
    let mut total: i64 = 0;
    let mut elderly: i64 = 0;
    for r in pyramid {
        let grp = get_str_ref(r, "age_group");
        let m = get_i64_local(r, "male_count");
        let f = get_i64_local(r, "female_count");
        total += m + f;
        if grp == "65-74" || grp == "75+" || grp == "70-79" || grp == "80+" {
            elderly += m + f;
        } else if grp == "60-69" {
            elderly += (m + f) / 2;
        }
    }
    if total > 0 {
        (elderly as f64 / total as f64) * 100.0
    } else {
        0.0
    }
}

fn get_i64_local(row: &Row, key: &str) -> i64 {
    super::super::super::helpers::get_i64(row, key)
}

/// 案 #18 + D-4: 可住地密度 + 高齢化率 KPI セクション（印刷版）
///
/// 必須注記:
/// - #18: 「可住地密度は地理特性。求人配信戦略との因果ではなく傾向参照」
/// - D-4: 「65 歳以上人口比率。労働人口希少性の参考指標」
pub(super) fn render_section_region_extras(html: &mut String, ctx: &InsightContext) {
    // 可住地密度
    let geo_density = ctx
        .ext_geography
        .first()
        .map(|r| {
            let d = get_f64(r, "habitable_density_per_km2");
            if d > 0.0 {
                d
            } else {
                let pop = get_i64_local(r, "total_population") as f64;
                let area = get_f64(r, "habitable_area_km2");
                if pop > 0.0 && area > 0.0 {
                    pop / area
                } else {
                    0.0
                }
            }
        })
        .unwrap_or(0.0);

    let aging_rate_from_pop = ctx
        .ext_population
        .first()
        .map(|r| get_f64(r, "aging_rate"))
        .unwrap_or(0.0);
    let aging_rate = if aging_rate_from_pop > 0.0 {
        aging_rate_from_pop
    } else {
        calc_aging_rate_from_pyramid(&ctx.ext_pyramid)
    };

    if geo_density <= 0.0 && aging_rate <= 0.0 {
        return;
    }

    html.push_str("<div class=\"section\" data-testid=\"region-extras-section\">\n");
    html.push_str("<h2>地域特性 補足（地理 / 人口構成）</h2>\n");

    render_figure_caption(html, "図 6-3", "可住地密度・高齢化率 KPI（参考指標）");

    html.push_str("<div class=\"kpi-grid\">\n");

    // 可住地密度
    if geo_density > 0.0 {
        let cls = classify_habitable_density(geo_density);
        let val_text = format!("{} 人/km²", format_number(geo_density.round() as i64));
        let compare = format!("分類: {}（基準: 5000+ 都市 / 1000+ 中間 / 未満 郊外）", cls);
        render_kpi_card_v2(html, "", "可住地密度", &val_text, "", &compare, "", cls);
    }

    // 高齢化率
    if aging_rate > 0.0 {
        let diff = aging_rate - REGION_EXTRAS_NATIONAL_AGING_PCT;
        let sign = if diff > 0.0 { "+" } else { "" };
        let compare = format!(
            "全国 {:.0}% / 差分 {}{:.1}pt",
            REGION_EXTRAS_NATIONAL_AGING_PCT, sign, diff
        );
        let status = if aging_rate >= 35.0 {
            "warn"
        } else if aging_rate <= 22.0 {
            "good"
        } else {
            ""
        };
        render_kpi_card_v2(
            html,
            "",
            "高齢化率（65+ 人口比率）",
            &format!("{:.1}", aging_rate),
            "%",
            &compare,
            status,
            "",
        );
    }

    html.push_str("</div>\n");

    render_read_hint(
        html,
        "可住地密度は地理特性で、都市型は媒体配信を狭域集中、郊外型は広域配信が向く傾向があります。\
         高齢化率は労働人口希少性の参考指標で、全国平均（29%）との差分で地域特性を把握します。\
         どちらも因果ではなく傾向参照値であり、職種・条件マッチングが本質的要因です。",
    );

    html.push_str("</div>\n");
}

/// 案 D-3: 産業別就業者構成 Top10 セクション（印刷版・データ受け渡し型）
///
/// `industry_rows` は handlers 側で fetch_industry_structure から得た上位 10 行。
/// 印刷版 render_survey_report_page の現行 API では受け取れないため、
/// 直接呼び出しは Tab UI 経由（integration.rs）で完結し、印刷版では非表示。
/// 関数自体は将来の API 拡張時に流用可能なよう pub(super) として用意のみ。
#[allow(dead_code)]
pub(super) fn render_section_industry_structure(
    html: &mut String,
    industry_rows: &[Row],
    pref: &str,
) {
    if industry_rows.is_empty() {
        return;
    }
    // 集計行 (AS=全産業 / AR=全産業(公務除く) / CR=非農林漁業(公務除く)) を除外
    // これらを含めると合計が 3 倍以上になり構成比が誤る (バグ修正 2026-04-27)
    let is_aggregate = |code: &str| matches!(code, "AS" | "AR" | "CR");
    let detail_rows: Vec<&Row> = industry_rows
        .iter()
        .filter(|r| !is_aggregate(get_str_ref(r, "industry_code")))
        .collect();
    let total: i64 = detail_rows
        .iter()
        .map(|r| get_i64_local(r, "employees_total"))
        .sum();
    if total <= 0 {
        return;
    }

    html.push_str("<div class=\"section\" data-testid=\"industry-structure-print\">\n");
    html.push_str(&format!(
        "<h2>地域の産業構成（{} 就業者 Top 10）</h2>\n",
        super::super::super::helpers::escape_html(pref)
    ));
    render_figure_caption(html, "表 6-2", "産業別就業者数 Top 10（国勢調査 2020）");

    html.push_str(
        "<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>産業</th>\
        <th style=\"text-align:right\">就業者数</th><th style=\"text-align:right\">構成比</th></tr></thead>\n<tbody>\n",
    );
    for (i, r) in detail_rows.iter().take(10).enumerate() {
        let name = get_str_ref(r, "industry_name");
        let emp = get_i64_local(r, "employees_total");
        let pct = emp as f64 / total as f64 * 100.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            super::super::super::helpers::escape_html(name),
            format_number(emp),
            pct,
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "産業分類は国勢調査 2020 ベース。HW industry_raw とは粒度が異なる可能性があります。\
         産業別就業者数と採用容易性に相関が見られる場合がありますが、職種・条件マッチングが本質的要因です。",
    );

    html.push_str("</div>\n");
}

#[cfg(test)]
mod impl1_print_tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn make_row(pairs: &[(&str, serde_json::Value)]) -> Row {
        let mut m: Row = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    fn empty_ctx() -> InsightContext {
        InsightContext {
            vacancy: vec![],
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy: vec![],
            ts_salary: vec![],
            ts_fulfillment: vec![],
            ts_tracking: vec![],
            ext_job_ratio: vec![],
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover: vec![],
            ext_population: vec![],
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            pref_avg_unemployment_rate: None,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: "東京都".to_string(),
            muni: "千代田区".to_string(),
        }
    }

    /// 印刷版 #18 + D-4: 地域特性補足セクション具体値テスト
    #[test]
    fn impl1_print_region_extras_renders_density_and_aging() {
        let ctx = InsightContext {
            ext_geography: vec![make_row(&[
                ("habitable_density_per_km2", json!(8_500.0)),
                ("habitable_area_km2", json!(11.66)),
                ("total_population", json!(60_000_i64)),
            ])],
            ext_population: vec![make_row(&[("aging_rate", json!(33.5))])],
            ..empty_ctx()
        };

        let mut html = String::new();
        render_section_region_extras(&mut html, &ctx);

        assert!(
            html.contains("data-testid=\"region-extras-section\""),
            "印刷版 region-extras section 必須"
        );
        assert!(html.contains("図 6-3"), "図番号 6-3 必須");
        // 具体値: 8500
        assert!(html.contains("8,500"), "可住地密度 8500 表示");
        // 都市型分類 (>=5000)
        assert!(html.contains("都市型"), "都市型 (>=5000) 分類");
        // 高齢化率 33.5% / 全国 29% 比較 +4.5pt
        assert!(html.contains("33.5"), "高齢化率 33.5 表示");
        assert!(html.contains("全国 29%"), "全国比較");
        assert!(html.contains("+4.5pt"), "差分 +4.5pt 具体値検証");
        // 必須注記
        assert!(html.contains("可住地密度は地理特性"), "#18 必須注記");
        assert!(html.contains("労働人口希少性の参考指標"), "D-4 必須注記");
    }

    /// 産業構造印刷版 D-3: Top10 表 + 構成比
    #[test]
    fn impl1_print_industry_section_renders_top10_with_pct() {
        let mk = |name: &str, emp: i64| -> Row {
            make_row(&[
                ("industry_name", json!(name)),
                ("employees_total", json!(emp)),
            ])
        };
        let rows = vec![
            mk("医療,福祉", 100_000),
            mk("製造業", 80_000),
            mk("卸売業,小売業", 60_000),
        ];

        let mut html = String::new();
        render_section_industry_structure(&mut html, &rows, "東京都");

        assert!(
            html.contains("data-testid=\"industry-structure-print\""),
            "印刷版 industry セクション ID 必須"
        );
        assert!(html.contains("表 6-2"), "表番号 6-2 必須");
        // total = 240_000、医療福祉 100_000 → 41.7%
        assert!(html.contains("41.7%"), "医療福祉構成比 41.7% 具体値");
        assert!(html.contains("100,000"), "医療福祉就業者数 100,000 具体値");
        // 必須注記
        assert!(html.contains("国勢調査 2020"), "D-3 必須注記");
        assert!(
            html.contains("職種・条件マッチングが本質的要因"),
            "因果ではない注記"
        );
    }

    /// 印刷版 fail-soft: 全空 ctx ではセクション非表示
    #[test]
    fn impl1_print_region_extras_hidden_when_no_data() {
        let ctx = empty_ctx();
        let mut html = String::new();
        render_section_region_extras(&mut html, &ctx);
        assert!(html.is_empty(), "全空時は印刷版セクション非表示");
    }

    #[test]
    fn municipality_salary_filters_other_pref_when_scope_is_single_pref() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![("群馬県".to_string(), 2)];
        agg.by_municipality_salary = vec![
            MunicipalitySalaryAgg {
                name: "前橋市".to_string(),
                prefecture: "群馬県".to_string(),
                count: 2,
                avg_salary: 250_000,
                median_salary: 240_000,
            },
            MunicipalitySalaryAgg {
                name: "深谷市".to_string(),
                prefecture: "埼玉県".to_string(),
                count: 1,
                avg_salary: 260_000,
                median_salary: 250_000,
            },
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        assert!(html.contains("前橋市"), "対象県の市区町村は表示する");
        assert!(
            !html.contains("深谷市"),
            "単一都道府県スコープでは他県市区町村を表示しない"
        );
    }

    #[test]
    fn municipality_salary_keeps_cross_pref_rows_when_scope_is_multi_pref() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![("北海道".to_string(), 1), ("福島県".to_string(), 1)];
        agg.by_municipality_salary = vec![
            MunicipalitySalaryAgg {
                name: "伊達市".to_string(),
                prefecture: "北海道".to_string(),
                count: 1,
                avg_salary: 250_000,
                median_salary: 240_000,
            },
            MunicipalitySalaryAgg {
                name: "伊達市".to_string(),
                prefecture: "福島県".to_string(),
                count: 1,
                avg_salary: 260_000,
                median_salary: 250_000,
            },
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        assert!(html.contains("北海道"), "多県CSVでは北海道側を残す");
        assert!(html.contains("福島県"), "多県CSVでは福島県側を残す");
        assert!(html.contains("同名"), "同名市区町村はマーカーで区別する");
    }
}

pub(super) fn render_section_municipality_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_municipality_salary.is_empty() {
        return;
    }

    let display_municipalities = if agg.by_prefecture.len() == 1 {
        let target_pref = &agg.by_prefecture[0].0;
        filter_municipalities_by_pref(&agg.by_municipality_salary, target_pref)
    } else {
        agg.by_municipality_salary.clone()
    };
    if display_municipalities.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（市区町村）</h2>\n");
    // So What: 件数の多い市区町村の給与水準が最も高い先
    if let Some(top_hi_salary) = display_municipalities
        .iter()
        .take(15)
        .max_by_key(|m| m.avg_salary)
    {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数の多い 15 市区町村のうち、平均月給が最も高いのは\
             「{} {}」で {}（同名異県を避けるため都道府県併記）。</p>\n",
            escape_html(&top_hi_salary.prefecture),
            escape_html(&top_hi_salary.name),
            escape_html(&format_man_yen(top_hi_salary.avg_salary))
        ));
    }
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>掲載件数の多い市区町村の給与水準を比較。\
        同じ都道府県内でも市区町村により給与差があります。\
    </p>\n",
    );

    // 同名市区町村の判定（伊達市・府中市など）
    use std::collections::HashMap;
    let mut name_count: HashMap<String, usize> = HashMap::new();
    for m in display_municipalities.iter().take(15) {
        *name_count.entry(m.name.clone()).or_insert(0) += 1;
    }

    // 以前は主要都道府県と異なる市区町村に「他県」マーカーを出していたが、
    // 通勤圏・広域求人では隣接県が正当に含まれるため誤判定になりやすい。
    // 同名市区町村の取り違え防止は dup_marker と都道府県列で担保する。
    render_figure_caption(
        html,
        "表 7-1",
        "市区町村別 給与水準 Top 15（同名市区町村マーク付き）",
    );
    html.push_str(
        "<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n",
    );
    for (i, m) in display_municipalities.iter().take(15).enumerate() {
        let dup_marker = if name_count.get(&m.name).copied().unwrap_or(0) > 1 {
            " <span title=\"同名市区町村あり\" style=\"color:#f59e0b;font-weight:700;font-size:9pt;\">\u{26A0} 同名</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}{}</td><td style=\"font-size:10px;color:#666\">{}</td>\
             <td class=\"num\">{}件</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&m.name),
            dup_marker,
            escape_html(&m.prefecture),
            m.count,
            format_man_yen(m.avg_salary),
            format_man_yen(m.median_salary),
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "「\u{26A0} 同名」マークは、伊達市（北海道/福島県）や府中市（東京都/広島県）のように同名の市区町村が複数存在することを示します。都道府県と組み合わせて判定してください。",
    );

    render_section_bridge(
        html,
        "次セクションでは、雇用形態（正社員/パート/派遣）の構成と単位の異なる給与を別々に確認します。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// Round 12: master / region (都道府県 × 市区町村) JOIN 整合性テスト
// =====================================================================
// 検証対象:
//   - K1: PDF 表紙の compose_target_region で「東京都 川崎市」が成立しうる構造的バグ
//        (aggregator::aggregate_records_core で dominant_prefecture と
//         dominant_municipality を **独立に** 最多選出するため pref/muni の
//         整合性が保証されない)
//   - K2: 表 7-1 (render_section_municipality_salary) の列順
//        thead/tbody は <市区町村><都道府県> の順で一貫しているが、
//        一般感覚 (都道府県→市区町村) との不一致が「逆転」と認識される
//   - master_city.csv (geo/master_city.csv) を読み込んだ municipality_code
//     先頭 2 桁と prefecture の対応一致 (read-only cross-check)
//
// 制約: アプリ側コード変更禁止。これらは pure な逆証明テスト。
// =====================================================================

#[cfg(test)]
mod round12_master_tests {
    use super::super::super::aggregator::{MunicipalitySalaryAgg, SurveyAggregation};
    use super::super::helpers::compose_target_region;
    use super::{filter_municipalities_by_pref, render_section_municipality_salary};

    // -----------------------------------------------------------------
    // ヘルパ: prefecture → prefcode (2桁) の対応表 (master_city.csv と整合)
    // -----------------------------------------------------------------
    fn pref_to_code(pref: &str) -> Option<u32> {
        match pref {
            "北海道" => Some(1),
            "青森県" => Some(2),
            "岩手県" => Some(3),
            "宮城県" => Some(4),
            "秋田県" => Some(5),
            "山形県" => Some(6),
            "福島県" => Some(7),
            "茨城県" => Some(8),
            "栃木県" => Some(9),
            "群馬県" => Some(10),
            "埼玉県" => Some(11),
            "千葉県" => Some(12),
            "東京都" => Some(13),
            "神奈川県" => Some(14),
            "新潟県" => Some(15),
            "富山県" => Some(16),
            "石川県" => Some(17),
            "福井県" => Some(18),
            "山梨県" => Some(19),
            "長野県" => Some(20),
            "岐阜県" => Some(21),
            "静岡県" => Some(22),
            "愛知県" => Some(23),
            "三重県" => Some(24),
            "滋賀県" => Some(25),
            "京都府" => Some(26),
            "大阪府" => Some(27),
            "兵庫県" => Some(28),
            "奈良県" => Some(29),
            "和歌山県" => Some(30),
            "鳥取県" => Some(31),
            "島根県" => Some(32),
            "岡山県" => Some(33),
            "広島県" => Some(34),
            "山口県" => Some(35),
            "徳島県" => Some(36),
            "香川県" => Some(37),
            "愛媛県" => Some(38),
            "高知県" => Some(39),
            "福岡県" => Some(40),
            "佐賀県" => Some(41),
            "長崎県" => Some(42),
            "熊本県" => Some(43),
            "大分県" => Some(44),
            "宮崎県" => Some(45),
            "鹿児島県" => Some(46),
            "沖縄県" => Some(47),
            _ => None,
        }
    }

    fn muni(pref: &str, name: &str, count: usize, avg: i64) -> MunicipalitySalaryAgg {
        MunicipalitySalaryAgg {
            name: name.to_string(),
            prefecture: pref.to_string(),
            count,
            avg_salary: avg,
            median_salary: avg - 10_000,
        }
    }

    // -----------------------------------------------------------------
    // (a) 正常系: 47 都道府県名は全てコード化可能
    // -----------------------------------------------------------------
    #[test]
    fn r12_all_47_prefectures_have_codes() {
        let prefs = [
            "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県",
            "茨城県", "栃木県", "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県",
            "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県", "岐阜県",
            "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府", "兵庫県",
            "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県",
            "徳島県", "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県",
            "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
        ];
        assert_eq!(prefs.len(), 47);
        for p in prefs {
            assert!(pref_to_code(p).is_some(), "{} のコードが解決できない", p);
        }
        // 不正な県名は None
        assert!(pref_to_code("江戸国").is_none());
        assert!(pref_to_code("Tokyo").is_none());
        assert!(pref_to_code("").is_none());
    }

    // -----------------------------------------------------------------
    // (a) 正常系: 政令市・著名市は都道府県と整合
    // -----------------------------------------------------------------
    #[test]
    fn r12_known_city_prefecture_pairs_are_consistent() {
        // (市区町村, 正しい都道府県) のペア
        let pairs = [
            ("川崎市", "神奈川県"),
            ("横浜市", "神奈川県"),
            ("相模原市", "神奈川県"),
            ("三芳町", "埼玉県"),
            ("深谷市", "埼玉県"),
            ("さいたま市", "埼玉県"),
            ("新宿区", "東京都"),
            ("千代田区", "東京都"),
            ("世田谷区", "東京都"),
            ("札幌市", "北海道"),
            ("仙台市", "宮城県"),
            ("大阪市", "大阪府"),
            ("京都市", "京都府"),
            ("名古屋市", "愛知県"),
            ("福岡市", "福岡県"),
        ];
        for (name, pref) in pairs {
            let m = muni(pref, name, 10, 250_000);
            assert_eq!(m.prefecture, pref);
            assert!(pref_to_code(&m.prefecture).is_some());
        }
    }

    // -----------------------------------------------------------------
    // (b) K1 逆証明: compose_target_region は pref/muni を別管理する
    //     ため「東京都 川崎市」が機械的に成立する
    //     → 表紙 cover の整合性検証ガード不在を明示
    // -----------------------------------------------------------------
    #[test]
    fn r12_k1_reproduce_cross_pref_dominant_pair() {
        // K1: dominant_prefecture と dominant_municipality は独立計算
        // のため、(pref, muni) ペアが地理的に矛盾しても compose は素通しする
        let mut agg = SurveyAggregation::default();
        agg.dominant_prefecture = Some("東京都".to_string());
        agg.dominant_municipality = Some("川崎市".to_string()); // 神奈川県の市

        let region_text = compose_target_region(&agg);

        // 現状の動作 (バグの存在を逆証明として固定): K1 が再現する
        assert_eq!(
            region_text, "東京都 川崎市",
            "K1: compose_target_region は pref/muni 整合チェックを行わないため、\
             地理的に矛盾する組合せをそのまま連結する (PDF 表紙誤表示の根本原因)"
        );

        // 同種パターン横展開: 三芳町(埼玉) を東京都に組み合わせても通る
        let mut agg2 = SurveyAggregation::default();
        agg2.dominant_prefecture = Some("東京都".to_string());
        agg2.dominant_municipality = Some("三芳町".to_string());
        assert_eq!(compose_target_region(&agg2), "東京都 三芳町");
    }

    // -----------------------------------------------------------------
    // (a) 正常系: pref/muni が地理的に正しい場合の compose
    // -----------------------------------------------------------------
    #[test]
    fn r12_compose_target_region_happy_paths() {
        let mut agg = SurveyAggregation::default();
        agg.dominant_prefecture = Some("神奈川県".to_string());
        agg.dominant_municipality = Some("川崎市".to_string());
        assert_eq!(compose_target_region(&agg), "神奈川県 川崎市");

        let mut agg2 = SurveyAggregation::default();
        agg2.dominant_prefecture = Some("埼玉県".to_string());
        agg2.dominant_municipality = Some("三芳町".to_string());
        assert_eq!(compose_target_region(&agg2), "埼玉県 三芳町");

        let mut agg3 = SurveyAggregation::default();
        agg3.dominant_prefecture = Some("東京都".to_string());
        agg3.dominant_municipality = None;
        assert_eq!(compose_target_region(&agg3), "東京都");

        let agg4 = SurveyAggregation::default();
        assert_eq!(compose_target_region(&agg4), "全国");
    }

    // -----------------------------------------------------------------
    // (b) K2 検証: 表 7-1 のヘッダ列順 (現状は <市区町村><都道府県>)
    //     一般感覚 (都道府県→市区町村) と異なるためユーザーが「逆転」と
    //     感じる現象を固定化する
    // -----------------------------------------------------------------
    #[test]
    fn r12_k2_table_header_order_municipality_then_prefecture() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![("埼玉県".to_string(), 100)];
        agg.by_municipality_salary = vec![
            muni("埼玉県", "三芳町", 5, 260_000),
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        // 現状の thead は <市区町村> → <都道府県> の順
        let idx_th_muni = html.find("<th>市区町村</th>").expect("市区町村列ヘッダ");
        let idx_th_pref = html.find("<th>都道府県</th>").expect("都道府県列ヘッダ");
        assert!(
            idx_th_muni < idx_th_pref,
            "K2 現状動作: ヘッダは <市区町村> → <都道府県> の順 \
             (一般感覚と逆だが、tbody と一貫しているため値の取り違えは起きない)"
        );

        // tbody 内に限定して順序を確認 (So What 行に「{pref} {name}」が出るので全文検索だと逆転判定になる)
        let tbody_start = html.find("<tbody>").expect("<tbody> あり");
        let tbody_end = html.find("</tbody>").expect("</tbody> あり");
        let tbody = &html[tbody_start..tbody_end];
        let idx_val_muni = tbody.find("三芳町").expect("tbody 内: 三芳町");
        let idx_val_pref = tbody.find("埼玉県").expect("tbody 内: 埼玉県");
        assert!(
            idx_val_muni < idx_val_pref,
            "K2 現状動作: tbody 値は <市区町村> → <都道府県> の順 \
             (thead と一貫 → 列内データは正しい。tbody=`{}`)",
            tbody
        );
    }

    // -----------------------------------------------------------------
    // (b) K2 同種パターン: 多件レコードでも列順は崩れない
    // -----------------------------------------------------------------
    #[test]
    fn r12_k2_table_header_consistency_across_rows() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![
            ("神奈川県".to_string(), 50),
            ("埼玉県".to_string(), 30),
        ]; // 単一県スコープではないので filter は素通し
        agg.by_municipality_salary = vec![
            muni("神奈川県", "川崎市", 30, 300_000),
            muni("神奈川県", "横浜市", 20, 310_000),
            muni("埼玉県", "三芳町", 5, 260_000),
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        // tbody 内の各 <tr> 行で「<td>name</td><td...>pref</td>」順序が維持されるか
        let tbody_start = html.find("<tbody>").expect("<tbody>");
        let tbody_end = html.find("</tbody>").expect("</tbody>");
        let tbody = &html[tbody_start..tbody_end];

        for (name, pref) in [
            ("川崎市", "神奈川県"),
            ("横浜市", "神奈川県"),
            ("三芳町", "埼玉県"),
        ] {
            // 各 name の出現位置に最も近い <tr ... </tr> 部分を取り出す
            let i_name = tbody
                .find(name)
                .unwrap_or_else(|| panic!("tbody 内に値 {} なし", name));
            let row_start = tbody[..i_name].rfind("<tr>").unwrap_or(0);
            let row_end_rel = tbody[i_name..]
                .find("</tr>")
                .unwrap_or_else(|| panic!("{} の行終端なし", name));
            let row = &tbody[row_start..(i_name + row_end_rel + 5)];
            // 行内で name と pref が共存し、name が先
            let in_row_name = row.find(name).expect("行内 name");
            let in_row_pref = row.find(pref).expect("行内 pref");
            assert!(
                in_row_name < in_row_pref,
                "{} と対応する {} は同一 <tr> 内で name → pref の順 (row=`{}`)",
                name,
                pref,
                row
            );
        }
    }

    // -----------------------------------------------------------------
    // (c) 不正組合せ検出: filter_municipalities_by_pref は (pref, name)
    //     ペアの prefecture フィールドを信頼する。レコードが「東京都 川崎市」
    //     (地理的に矛盾) を持っていても prefecture==target_pref で素通しする
    //     → これは filter 関数の責務外 (アップストリームのデータ品質責任)
    // -----------------------------------------------------------------
    #[test]
    fn r12_filter_trusts_prefecture_field_not_geography() {
        let data = vec![
            // 地理的に矛盾するレコード (上流バグの再現)
            muni("東京都", "川崎市", 100, 300_000),
            muni("神奈川県", "川崎市", 50, 300_000),
        ];

        let tokyo = filter_municipalities_by_pref(&data, "東京都");
        assert_eq!(tokyo.len(), 1);
        assert_eq!(tokyo[0].prefecture, "東京都");
        assert_eq!(tokyo[0].name, "川崎市");
        // → filter 自体は仕様通り。問題は上流 (location_parser / aggregator)。

        let kanagawa = filter_municipalities_by_pref(&data, "神奈川県");
        assert_eq!(kanagawa.len(), 1);
        assert_eq!(kanagawa[0].prefecture, "神奈川県");
    }

    // -----------------------------------------------------------------
    // (d) DB cross-check: master_city.csv の citycode 先頭 2 桁 = prefcode
    //     これは pref と city の整合の正解定義 (read-only)
    // -----------------------------------------------------------------
    #[test]
    fn r12_master_city_csv_citycode_matches_prefcode() {
        // ファイル: src/geo/master_city.csv は include_str! で取得可能
        // CARGO_MANIFEST_DIR からの相対パスを使う
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/geo/master_city.csv");
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("master_city.csv 読み込み失敗 (テスト skip 相当): {}", e);
                return;
            }
        };

        let mut lines = content.lines();
        let header = lines.next().expect("header");
        assert!(
            header.starts_with("citycode,prefcode,city_name"),
            "header 形式: {}",
            header
        );

        let mut total = 0usize;
        let mut mismatch = 0usize;
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.splitn(4, ',').collect();
            if cols.len() < 3 {
                continue;
            }
            let citycode: u32 = cols[0].parse().unwrap_or(0);
            let prefcode: u32 = cols[1].parse().unwrap_or(0);
            if citycode == 0 || prefcode == 0 {
                continue;
            }
            total += 1;
            // prefcode は citycode の先頭 1〜2 桁。4 桁 (例 1101→1) も 5 桁 (例 14131→14) も /1000 で取れる。
            let leading = citycode / 1_000;
            if leading != prefcode {
                mismatch += 1;
                eprintln!(
                    "citycode {} prefcode {} city {} → leading {} != prefcode",
                    citycode, prefcode, cols[2], leading
                );
            }
        }
        assert!(total > 1_500, "master_city.csv 行数が少なすぎる: {}", total);
        assert_eq!(
            mismatch, 0,
            "citycode 先頭 2 桁と prefcode が不一致のレコードがある"
        );
    }

    // -----------------------------------------------------------------
    // (d) DB cross-check: master_city.csv 内で「川崎市」を含む市区町村は
    //     すべて prefcode == 14 (神奈川県) 配下、または別 prefcode の
    //     「○○郡川崎町」のみであること (神奈川県川崎市と混同しないか確認)
    // -----------------------------------------------------------------
    #[test]
    fn r12_master_city_csv_kawasaki_belongs_to_kanagawa() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/geo/master_city.csv");
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut kawasaki_shi_prefs = std::collections::HashSet::new();
        let mut kawasaki_machi_prefs = std::collections::HashSet::new();

        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.splitn(4, ',').collect();
            if cols.len() < 3 {
                continue;
            }
            let prefcode: u32 = cols[1].parse().unwrap_or(0);
            let name = cols[2];
            // 「川崎市XX区」(政令市) は prefcode=14 のみのはず
            if name.starts_with("川崎市") {
                kawasaki_shi_prefs.insert(prefcode);
            }
            // 「○○郡川崎町」(同名町) は別 prefcode
            if name.ends_with("川崎町") {
                kawasaki_machi_prefs.insert(prefcode);
            }
        }
        assert_eq!(
            kawasaki_shi_prefs,
            std::collections::HashSet::from([14u32]),
            "「川崎市XX区」は神奈川県(14)配下のみであるべき。実測: {:?}",
            kawasaki_shi_prefs
        );
        // 川崎町は 4 (宮城) と 40 (福岡) に実在
        assert!(
            !kawasaki_machi_prefs.is_empty(),
            "「川崎町」は別県に存在するはず (宮城/福岡)"
        );
        assert!(
            !kawasaki_machi_prefs.contains(&13u32),
            "「東京都川崎町」は実在しない"
        );
    }

    // -----------------------------------------------------------------
    // (d) DB cross-check: 三芳町は埼玉県(11)入間郡のみ
    // -----------------------------------------------------------------
    #[test]
    fn r12_master_city_csv_miyoshi_machi_in_saitama_only() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/geo/master_city.csv");
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut found_prefs = std::collections::HashSet::new();
        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.splitn(4, ',').collect();
            if cols.len() < 3 {
                continue;
            }
            let prefcode: u32 = cols[1].parse().unwrap_or(0);
            let name = cols[2];
            if name.ends_with("三芳町") {
                found_prefs.insert(prefcode);
            }
        }
        assert!(
            found_prefs.contains(&11u32),
            "「三芳町」は埼玉県(11)に実在するはず。実測: {:?}",
            found_prefs
        );
        assert!(
            !found_prefs.contains(&13u32),
            "「東京都の三芳町」は実在しない (K1 で誤表示された場合のデータ不整合検知)"
        );
    }

    // -----------------------------------------------------------------
    // (b) 単一県スコープで他県の市区町村は除外される (既存仕様の固定)
    // -----------------------------------------------------------------
    #[test]
    fn r12_single_pref_scope_excludes_cross_pref_municipalities() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![("東京都".to_string(), 100)];
        agg.by_municipality_salary = vec![
            muni("東京都", "新宿区", 50, 280_000),
            // 上流バグで「東京都 川崎市」と誤って入ってきた場合は素通し
            muni("東京都", "川崎市", 30, 300_000),
            // 別県の値は除外される
            muni("神奈川県", "川崎市", 20, 300_000),
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        assert!(html.contains("新宿区"), "東京都の新宿区は表示");
        // 神奈川県川崎市は filter で落ちる
        let count_kanagawa = html.matches("神奈川県").count();
        assert_eq!(count_kanagawa, 0, "他県(神奈川県)レコードは除外");
    }

    // -----------------------------------------------------------------
    // (b) 多県スコープでは同名市区町村は両方残り、「同名」マーカーが付く
    // -----------------------------------------------------------------
    #[test]
    fn r12_multi_pref_scope_keeps_homonyms_with_marker() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![
            ("北海道".to_string(), 10),
            ("福島県".to_string(), 8),
        ];
        agg.by_municipality_salary = vec![
            muni("北海道", "伊達市", 10, 250_000),
            muni("福島県", "伊達市", 8, 260_000),
        ];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);

        assert!(html.contains("北海道"));
        assert!(html.contains("福島県"));
        assert!(html.contains("同名"), "同名マーカーが付く");
    }

    // -----------------------------------------------------------------
    // (c) 空データ系: render は早期 return する
    // -----------------------------------------------------------------
    #[test]
    fn r12_empty_municipality_returns_early() {
        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);
        assert!(html.is_empty(), "空 by_municipality_salary では何も書かない");
    }

    // -----------------------------------------------------------------
    // (a) 正常系: format_man_yen の出力に「万」が含まれる (列の値検証)
    // -----------------------------------------------------------------
    #[test]
    fn r12_table_contains_man_yen_formatted_salary() {
        let mut agg = SurveyAggregation::default();
        agg.by_prefecture = vec![("埼玉県".to_string(), 10)];
        agg.by_municipality_salary = vec![muni("埼玉県", "三芳町", 5, 260_000)];

        let mut html = String::new();
        render_section_municipality_salary(&mut html, &agg);
        // 26万円 (= 260_000 円) が万円表記される
        assert!(
            html.contains("26") && html.contains("万"),
            "平均月給が万円表記される: {}",
            crate::text_util::truncate_char_safe(&html, 2000)
        );
    }
}
