//! 分割: report_html/wage.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

pub(super) fn render_section_min_wage(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture_salary.is_empty() {
        return;
    }

    // 都道府県ごとに最低賃金比較データを構築
    struct MinWageEntry {
        name: String,
        avg_min: i64,
        min_wage: i64,
        hourly_160: i64, // 月給÷160h
        diff_160: i64,
        ratio_160: f64,
    }
    let mut entries: Vec<MinWageEntry> = agg
        .by_prefecture_salary
        .iter()
        .filter_map(|p| {
            let mw = min_wage_for_prefecture(&p.name)?;
            if p.avg_min_salary <= 0 {
                return None;
            }
            let hourly_160 = p.avg_min_salary / super::super::aggregator::HOURLY_TO_MONTHLY_HOURS;
            let diff_160 = hourly_160 - mw;
            let ratio_160 = hourly_160 as f64 / mw as f64;
            Some(MinWageEntry {
                name: p.name.clone(),
                avg_min: p.avg_min_salary,
                min_wage: mw,
                hourly_160,
                diff_160,
                ratio_160,
            })
        })
        .collect();

    if entries.is_empty() {
        return;
    }
    entries.sort_by(|a, b| a.diff_160.cmp(&b.diff_160)); // 差が小さい順

    // 全体の平均比率
    let avg_ratio: f64 = entries.iter().map(|e| e.ratio_160).sum::<f64>() / entries.len() as f64;
    let avg_diff_pct = (avg_ratio - 1.0) * 100.0;

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>最低賃金比較</h2>\n");
    // So What + severity badge（diff < 0 は Critical、< 50 は Warning、それ以外 Positive）
    let below_count = entries.iter().filter(|e| e.diff_160 < 0).count();
    let near_count = entries
        .iter()
        .filter(|e| e.diff_160 >= 0 && e.diff_160 < 50)
        .count();
    let sev = if below_count > 0 {
        RptSev::Critical
    } else if near_count > 0 {
        RptSev::Warning
    } else {
        RptSev::Positive
    };
    html.push_str(&format!(
        "<p class=\"section-sowhat\">{} {} 県で平均下限給与の 167h 換算が最低賃金を下回る傾向。\
         差が 50 円未満（要確認）: {} 県。該当求人群は労基上要確認。</p>\n",
        severity_badge(sev),
        below_count,
        near_count
    ));
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>月給を167h（8h×20.875日、厚労省基準）で割り時給換算して最低賃金と比較。\
        全国加重平均: <strong>1,121円</strong>（2025年10月施行）\
    </p>\n",
    );

    // 概要カード
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均最低賃金比率", &format!("{:.2}倍", avg_ratio));
    render_stat_box(html, "全体差分", &format!("{:+.1}%", avg_diff_pct));
    render_stat_box(html, "分析対象", &format!("{}都道府県", entries.len()));
    html.push_str("</div>\n");

    // 最低賃金との差が小さい都道府県 10 件（差額の小さい順に整理、ソート可能テーブル）
    html.push_str("<h3>時給換算で最低賃金に近い都道府県 10 件（差額の小さい順）</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">平均月給下限</th>\
        <th style=\"text-align:right\">167h換算</th><th style=\"text-align:right\">最低賃金</th>\
        <th style=\"text-align:right\">差額</th><th style=\"text-align:right\">比率</th></tr></thead>\n<tbody>\n");
    for (i, e) in entries.iter().take(10).enumerate() {
        let diff_color = if e.diff_160 < 0 {
            "negative"
        } else if e.diff_160 < 50 {
            "color:#fb8c00;font-weight:bold"
        } else {
            ""
        };
        let diff_style = if diff_color.starts_with("color:") {
            format!(" style=\"text-align:right;{}\"", diff_color)
        } else {
            format!(" class=\"num {}\"", diff_color)
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}円</td>\
             <td{}>{:+}円</td><td class=\"num\">{:.2}倍</td></tr>\n",
            i + 1,
            escape_html(&e.name),
            format_man_yen(e.avg_min),
            format_number(e.hourly_160),
            format_number(e.min_wage),
            diff_style,
            e.diff_160,
            e.ratio_160,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 活用ポイント
    html.push_str(
        "<div class=\"note\">\
        <strong>活用ポイント:</strong> 167h=所定労働時間（8h×20.875日、厚労省「就業条件総合調査 2024」基準）で換算。\
        最低賃金水準の求人は応募者が集まりにくい傾向。+10%以上の求人を優先検討すると効率的です。\
    </div>\n",
    );

    html.push_str("</div>\n");
}

pub(super) fn render_section_company(html: &mut String, by_company: &[CompanyAgg]) {
    if by_company.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>企業分析</h2>\n");

    // So What 行: 件数の多い法人と給与水準の傾向を 1 行で
    if let Some(top) = by_company.iter().max_by_key(|c| c.count) {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 掲載件数が最も多い法人は「{}」（{} 件、平均月給 {}）。\
             件数・給与の分布は以下のテーブルを参照（ソート可能）。</p>\n",
            escape_html(&top.name),
            format_number(top.count as i64),
            escape_html(&format_man_yen(top.avg_salary))
        ));
    }

    // 企業数サマリー
    html.push_str(&format!(
        "<p>分析対象企業数: <strong>{}</strong>社（給与情報のある求人を持つ企業のみ）</p>\n",
        format_number(by_company.len() as i64)
    ));

    // 市場集中度（HHI: Herfindahl-Hirschman Index）の計算と表示
    // HHI = Σ(各企業の求人シェア%)² / 公正取引委員会基準:
    //   < 1500: 分散型市場 / 1500-2500: 中程度集中 / > 2500: 集中型市場
    // サンプル数不足（企業数<3）時は非表示
    if by_company.len() >= 3 {
        let total_count: i64 = by_company.iter().map(|c| c.count as i64).sum();
        if total_count > 0 {
            let hhi: f64 = by_company
                .iter()
                .map(|c| {
                    let share_pct = c.count as f64 / total_count as f64 * 100.0;
                    share_pct * share_pct
                })
                .sum();
            let (judgment, color) = if hhi < 1500.0 {
                ("分散型市場（競合多数・多様な選択肢）", "var(--c-success)")
            } else if hhi < 2500.0 {
                ("中程度集中（主要プレイヤー複数）", "var(--c-warning)")
            } else {
                ("集中型市場（少数企業が支配的）", "var(--c-danger)")
            };
            html.push_str(&format!(
                "<p style=\"margin:8px 0;font-size:10pt;\">\
                 <strong>市場集中度（HHI）: <span style=\"color:{}\">{:.0}</span></strong> \
                 / 判定: <span style=\"color:{}\">{}</span> \
                 <span style=\"font-size:9pt;color:#888;\">（公正取引委員会基準: &lt;1500=分散 / 1500-2500=中程度 / &gt;2500=集中）</span>\
                 </p>\n",
                color, hhi, color, judgment
            ));
        }
    }

    // 掲載件数の多い法人 15 件（件数の多い順に整理、ソート可能テーブル）
    let mut by_count = by_company.to_vec();
    by_count.sort_by(|a, b| b.count.cmp(&a.count));

    html.push_str("<h3>掲載件数の多い法人 15 件（給与情報あり）</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">給与付き求人数</th><th style=\"text-align:right\">平均月給</th></tr></thead>\n<tbody>\n");
    for (i, c) in by_count.iter().take(15).enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&c.name),
            format_number(c.count as i64),
            format_man_yen(c.avg_salary),
        ));
    }
    html.push_str("</tbody></table>\n");

    // 平均給与の多い法人 15 件（サンプル数に応じて閾値動的調整）
    let multi_count = by_company.iter().filter(|c| c.count >= 2).count();
    let min_count_threshold = if multi_count >= 15 { 2 } else { 1 };
    let mut by_salary: Vec<&CompanyAgg> = by_company
        .iter()
        .filter(|c| c.count >= min_count_threshold && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        let title = if min_count_threshold >= 2 {
            "給与水準の高い法人 15 件（給与付き2件以上の企業）"
        } else {
            "給与水準の高い法人 15 件（給与付き、1件求人含む。※1件は参考値）"
        };
        html.push_str(&format!("<h3>{}</h3>\n", title));
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">給与付き求人数</th></tr></thead>\n<tbody>\n");
        for (i, c) in by_salary.iter().take(15).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(&c.name),
                format_man_yen(c.avg_salary),
                format_number(c.count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}

pub(super) fn render_section_tag_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_tag_salary.is_empty() && agg.by_tags.is_empty() {
        return;
    }

    let overall_mean = agg.enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>タグ×給与相関分析</h2>\n");
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各タグが付いた求人の平均給与と、全体平均との差を示します。\
        正の値（緑）=そのタグが付くと給与が高い傾向、負の値（赤）=低い傾向。\
    </p>\n",
    );

    html.push_str(&format!(
        "<p>全体平均月給: <strong>{}</strong></p>\n",
        format_man_yen(overall_mean)
    ));

    // タグ件数のツリーマップ（テーブルの上に配置）
    if !agg.by_tag_salary.is_empty() {
        let tree_data: Vec<serde_json::Value> = agg
            .by_tag_salary
            .iter()
            .map(|t| json!({"name": &t.tag, "value": t.count}))
            .collect();
        let config = json!({
            "tooltip": {"formatter": "{b}: {c}件"},
            "series": [{
                "type": "treemap",
                "data": tree_data,
                "roam": false,
                "label": {"show": true, "formatter": "{b}\n{c}件", "fontSize": 10},
                "breadcrumb": {"show": false},
                "levels": [{"colorSaturation": [0.3, 0.7]}]
            }]
        });
        html.push_str(&render_echart_div(&config.to_string(), 250));
    }

    if !agg.by_tag_salary.is_empty() {
        // 有意タグのフィルタリング:
        // 1. 出現率50%超のタグは共通属性として除外（全求人の半数以上に付く「交通費支給」等は差分がゼロに収束）
        // 2. 差分 |diff_percent| >= 2% のタグのみハイライト（それ未満は参考扱い）
        let total_records = agg.total_count as f64;
        let significant: Vec<&TagSalaryAgg> = agg
            .by_tag_salary
            .iter()
            .filter(|t| {
                let frequency = t.count as f64 / total_records;
                frequency < 0.5 && t.diff_percent.abs() >= 2.0
            })
            .collect();
        let display_tags: Vec<&TagSalaryAgg> = if significant.is_empty() {
            // フォールバック: 有意なタグがない場合は全タグを表示
            agg.by_tag_salary.iter().collect()
        } else {
            significant
        };
        if agg.by_tag_salary.len() > display_tags.len() {
            html.push_str(&format!(
                "<p class=\"note\" style=\"font-size:9pt;color:#888;\">※{}タグから{}タグに絞り込み表示中（出現率50%超の共通タグと差分±2%未満を除外）</p>\n",
                agg.by_tag_salary.len(), display_tags.len()
            ));
        }
        // タグ別給与差分テーブル（ソート可能・完全版）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th>\
            <th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">全体比</th></tr></thead>\n<tbody>\n");
        for (i, ts) in display_tags.iter().enumerate() {
            let diff_class = if ts.diff_from_avg > 0 {
                "positive"
            } else if ts.diff_from_avg < 0 {
                "negative"
            } else {
                ""
            };
            let diff_sign = if ts.diff_from_avg > 0 { "+" } else { "" };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num {diff_class}\">{sign}{diff}万円 ({sign}{pct:.1}%)</td></tr>\n",
                i + 1,
                escape_html(&ts.tag),
                format_number(ts.count as i64),
                format_man_yen(ts.avg_salary),
                diff = format!("{:.1}", ts.diff_from_avg as f64 / 10_000.0),
                sign = diff_sign,
                pct = ts.diff_percent,
            ));
        }
        html.push_str("</tbody></table>\n");
    } else {
        // フォールバック: 件数のみテーブル（ソート可能）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th></tr></thead>\n<tbody>\n");
        for (i, (tag, count)) in agg.by_tags.iter().take(20).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(tag),
                format_number(*count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}
