//! Round 24 Push 3 (2026-05-13): Navy + Gold コンサルファーム調レポートの
//! 専用レンダラ。既存 `executive_summary` / `summary` / `dv2-*` ヘルパに
//! 一切依存せず、navy CSS class (`.kpi-row`, `.findings-list`, `.so-what`,
//! `.table-navy`, `.block-title`, `.row-2`, `.tag-*` 等) だけで構成する。
//!
//! 設計方針:
//! - 既存パス (dv2-section-badge / exec-kpi-grid-v2 / exec-action-list 等)
//!   は **一切呼ばない**。互換 class を併記する妥協を排し、HTML 構造を
//!   ゼロから組み直す。
//! - ECharts は使わず、SSR SVG / CSS 罫線 / 数値テーブル中心。
//! - 既存集計 (`SurveyAggregation`, `InsightContext`) を入力として受け取り、
//!   出力 (String) を呼出側に積む。

#![allow(dead_code)]

use super::super::aggregator::{EmpTypeSalary, SurveyAggregation};
use super::super::super::helpers::{escape_html, format_number};
use super::super::super::insight::fetch::InsightContext;
use super::super::job_seeker::JobSeekerAnalysis;
use super::salary_summary;
use super::ReportVariant;

// ============================================================
// 公開 API
// ============================================================

/// Cover ページ全体 (1 枚)
pub(super) fn render_navy_cover(
    html: &mut String,
    agg: &SurveyAggregation,
    variant: ReportVariant,
    now: &str,
    today_short: &str,
    target_region: &str,
) {
    let cover_lede = match variant {
        ReportVariant::Full => "ハローワーク掲載求人 + アップロード CSV クロス分析により、対象地域における求人市場の構造と機会を可視化します。",
        ReportVariant::MarketIntelligence => "アップロード CSV + 公開統計クロス分析により、採用市場・ターゲット分析と競合動向を立体的に把握します。",
        ReportVariant::Public => "アップロード CSV + 公開統計クロス分析により、対象地域の構造的特徴を把握します。",
    };

    let hl_count = format_number(agg.total_count as i64);
    let salary_headline = salary_summary::SalaryHeadline::from_aggregation(agg);
    let cover_hl = salary_headline.cover_highlight_text();

    html.push_str("<section class=\"page-navy cover-navy\" role=\"region\" aria-labelledby=\"navy-cover-title\">\n");

    // topbar
    html.push_str("<div class=\"cover-topbar\">\n");
    html.push_str("<div class=\"brand\">\n");
    html.push_str("<span class=\"brand-mark\" aria-hidden=\"true\"></span>\n");
    html.push_str("<span class=\"brand-name\">FOR A-CAREER</span>\n");
    html.push_str("</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-meta\">{} 版 &nbsp;/&nbsp; {}</div>\n",
        escape_html(today_short),
        escape_html(now)
    ));
    html.push_str("</div>\n");

    // body
    html.push_str("<div class=\"cover-body\">\n");
    html.push_str("<div class=\"cover-eyebrow\">RECRUITMENT MARKET REPORT</div>\n");
    html.push_str("<div class=\"cover-rule\" aria-hidden=\"true\"></div>\n");
    html.push_str(
        "<h1 id=\"navy-cover-title\" class=\"cover-title\">求人市場<br>総合診断レポート</h1>\n",
    );
    html.push_str(&format!(
        "<p class=\"cover-lede\">{}</p>\n",
        escape_html(cover_lede)
    ));

    // stats
    html.push_str("<div class=\"cover-stats\">\n");
    push_cover_stat(html, &hl_count, "件", "サンプル件数");
    push_cover_stat_small(html, target_region, "主要地域 (対象)");
    push_cover_stat(
        html,
        &cover_hl.value_text,
        &cover_hl.unit,
        &cover_hl.label,
    );
    push_cover_stat_small(html, variant.display_name(), "レポート版");
    html.push_str("</div>\n");

    html.push_str("</div>\n"); // /cover-body

    // footer
    html.push_str("<div class=\"cover-footer\">\n");
    push_cover_footer(html, "発行", "株式会社 For A-career");
    push_cover_footer(html, "生成日時", now);
    push_cover_footer(html, "対象地域", target_region);
    push_cover_footer(html, "取扱区分", "機密 / 社外秘");
    html.push_str("</div>\n");

    html.push_str("</section>\n");
}

fn push_cover_stat(html: &mut String, value: &str, unit: &str, label: &str) {
    html.push_str(&format!(
        "<div class=\"cover-stat\">\
         <div class=\"cs-num\">{}<span class=\"cs-unit\">{}</span></div>\
         <div class=\"cs-label\">{}</div>\
         </div>\n",
        escape_html(value),
        escape_html(unit),
        escape_html(label)
    ));
}

fn push_cover_stat_small(html: &mut String, value: &str, label: &str) {
    html.push_str(&format!(
        "<div class=\"cover-stat\">\
         <div class=\"cs-num\" style=\"font-size:18pt;\">{}</div>\
         <div class=\"cs-label\">{}</div>\
         </div>\n",
        escape_html(value),
        escape_html(label)
    ));
}

fn push_cover_footer(html: &mut String, label: &str, value: &str) {
    html.push_str(&format!(
        "<div><div class=\"cf-label\">{}</div><div class=\"cf-val\">{}</div></div>\n",
        escape_html(label),
        escape_html(value)
    ));
}

// ============================================================
// TOC
// ============================================================

pub(super) fn render_navy_toc(html: &mut String, variant: ReportVariant) {
    let section_02 = match variant {
        ReportVariant::Full => "地域 × 求人媒体データ連携",
        _ => "地域データ補強",
    };
    html.push_str("<section class=\"page-navy toc-page\" role=\"region\" aria-label=\"目次\">\n");
    push_page_head(html, "TABLE OF CONTENTS", "目次", "本レポートは A4 縦印刷を前提に構成しています");
    html.push_str("<div class=\"toc-grid\">\n");

    html.push_str("<div class=\"toc-col\">\n");
    for (no, name) in [
        ("01", "Executive Summary"),
        ("02", section_02),
        ("03", "給与分布 統計"),
        ("04", "採用市場 逼迫度"),
    ] {
        push_toc_item(html, no, name);
    }
    html.push_str("</div>\n");

    html.push_str("<div class=\"toc-col\">\n");
    for (no, name) in [
        ("05", "地域企業構造"),
        ("06", "人材デモグラフィック"),
        ("07", "最低賃金・ライフスタイル"),
        ("08", "注記・出典・免責"),
    ] {
        push_toc_item(html, no, name);
    }
    html.push_str("</div>\n");

    html.push_str("</div>\n"); // /toc-grid

    html.push_str(
        "<div class=\"toc-foot\">\
         <div class=\"tf-block\">\
         <div class=\"tf-label\">SEVERITY 凡例</div>\
         <div class=\"legend-row\">\
         <span class=\"legend-chip pos\">POSITIVE</span>\
         <span class=\"legend-chip neu\">NEUTRAL</span>\
         <span class=\"legend-chip warn\">WARN</span>\
         <span class=\"legend-chip neg\">NEGATIVE</span>\
         </div></div>\
         <div class=\"tf-block\">\
         <div class=\"tf-label\">凡例の読み方</div>\
         <p>本レポート内の指標は上記 4 段階で評価しています。NEGATIVE / WARN は\
         「改善検討」の対象、POSITIVE は「強み」として認識してください。</p>\
         </div></div>\n",
    );
    html.push_str("</section>\n");
}

fn push_toc_item(html: &mut String, no: &str, name: &str) {
    html.push_str(&format!(
        "<div class=\"toc-item\">\
         <span class=\"t-no\">{}</span>\
         <span class=\"t-name\">{}</span>\
         <span class=\"t-pg\">—</span>\
         </div>\n",
        escape_html(no),
        escape_html(name)
    ));
}

// ============================================================
// Executive Summary
// ============================================================

pub(super) fn render_navy_executive(
    html: &mut String,
    agg: &SurveyAggregation,
    _seeker: &JobSeekerAnalysis,
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    target_region: &str,
) {
    html.push_str("<section class=\"page-navy navy-exec\" role=\"region\" aria-labelledby=\"navy-exec-title\">\n");
    push_page_head(html, "SECTION 01", "Executive Summary", "3 分で読み切れる全体要旨と優先アクション");
    html.push_str(&format!(
        "<h2 id=\"navy-exec-title\" class=\"sr-only\" style=\"position:absolute;left:-9999px;\">Executive Summary</h2>\n"
    ));

    // -- exec-headline (引用調 + 1 段落要旨)
    let total = agg.total_count;
    let salary_parse_pct = (agg.salary_parse_rate * 100.0).round() as i64;
    let new_pct = if total > 0 {
        (agg.new_count as f64 / total as f64 * 100.0).round() as i64
    } else {
        0
    };
    let dominant_emp = agg
        .by_employment_type
        .first()
        .map(|(name, c)| {
            let pct = if total > 0 {
                *c as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            format!("{} ({:.0}%)", name, pct)
        })
        .unwrap_or_else(|| "—".to_string());

    let headline_body = format!(
        "本レポートは <strong>{}</strong> を対象に、サンプル <strong>{} 件</strong> を分析した結果です。\
         主要雇用形態は <strong>{}</strong>、新着比率 <strong>{}%</strong>、給与解析率 <strong>{}%</strong>。\
         本ページでは <strong>5 KPI</strong> と <strong>5 Findings</strong> を提示し、末尾の <strong>SO WHAT</strong> で取るべき方針を集約します。",
        escape_html(target_region),
        format_number(total as i64),
        escape_html(&dominant_emp),
        new_pct,
        salary_parse_pct,
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        headline_body
    ));

    // -- kpi-row (5 cell)
    let k1 = format!("{}", format_number(total as i64));
    let k1_dot = if total >= 30 { "pos" } else if total > 0 { "warn" } else { "neg" };
    let k1_foot = if total >= 30 {
        "n>=30 で実務判断に参照可"
    } else if total > 0 {
        "n が少なく傾向参照のみ"
    } else {
        "サンプルなし"
    };

    let k3_name = agg.by_employment_type.first().map(|(n, _)| n.clone()).unwrap_or_default();
    let k3_pct = agg.by_employment_type.first().map(|(_, c)| {
        if total > 0 { *c as f64 / total as f64 * 100.0 } else { 0.0 }
    }).unwrap_or(0.0);
    let k3_value = if k3_name.is_empty() { "—".to_string() } else { k3_name.clone() };
    let k3_dot = if k3_pct >= 85.0 { "warn" } else { "neu" };
    let k3_foot = if k3_pct > 0.0 {
        format!("構成比 {:.0}%", k3_pct)
    } else {
        "—".to_string()
    };

    let salary_h = salary_summary::SalaryHeadline::from_aggregation(agg);
    let cover_hl = salary_h.cover_highlight_text();
    let _ = by_emp_type_salary;
    let _ = hw_context;
    let _ = variant;

    let k5_value = format!("{}", new_pct);
    let k5_dot = if total == 0 {
        "neu"
    } else if new_pct >= 15 {
        "pos"
    } else if new_pct < 5 {
        "warn"
    } else {
        "neu"
    };
    let k5_foot = "直近 30 日の新着求人比率";

    let k6_value = format!("{}", salary_parse_pct);
    let k6_dot = if salary_parse_pct >= 85 {
        "pos"
    } else if salary_parse_pct >= 60 {
        "warn"
    } else {
        "neg"
    };
    let k6_foot = "給与文字列から数値抽出に成功した比率";

    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(html, "サンプル件数", &k1, "件", k1_dot, k1_foot, false);
    push_kpi(html, "主要地域", target_region, "", "neu", "件数最多の地域", false);
    push_kpi(html, "主要雇用形態", &k3_value, "", k3_dot, &k3_foot, false);
    push_kpi(
        html,
        cover_hl.label.as_str(),
        cover_hl.value_text.as_str(),
        cover_hl.unit.as_str(),
        "neu",
        "本レポートの代表給与値",
        true,
    );
    push_kpi(html, "給与解析率", &k6_value, "%", k6_dot, k6_foot, false);
    html.push_str("</div>\n");

    // -- findings (KEY FINDINGS, 最大 5 件)
    let findings = build_findings(agg, total, k3_pct, new_pct, salary_parse_pct);
    html.push_str(
        "<div class=\"findings\">\n\
         <div class=\"findings-head\">\
         <div class=\"fh-no\">KEY FINDINGS</div>\
         <div class=\"fh-title\">優先確認 5 ポイント</div>\
         </div>\n",
    );
    html.push_str("<ol class=\"findings-list\">\n");
    for (i, (sev_tag, title, body, refer)) in findings.iter().enumerate() {
        let no = format!("{:02}", i + 1);
        html.push_str(&format!(
            "<li>\
             <div class=\"f-no\">{}</div>\
             <div class=\"f-body\">\
             <div class=\"f-title\"><span class=\"tag tag-{}\">{}</span> &nbsp;{}</div>\
             <p>{}</p>\
             </div>\
             <div class=\"f-ref\">{}</div>\
             </li>\n",
            no,
            sev_tag,
            severity_label(sev_tag),
            escape_html(title),
            body,
            escape_html(refer),
        ));
    }
    html.push_str("</ol>\n</div>\n");

    // -- so-what
    let new_pct_label = if total > 0 { format!("{}%", new_pct) } else { "—".to_string() };
    let so_what_body = format!(
        "サンプル件数 <strong>n={}</strong> / 給与解析率 <strong>{}%</strong> / 新着比率 <strong>{}</strong> を踏まえ、\
         <strong>給与水準と訴求軸の再点検</strong> を起点に、<strong>不足セグメント (n<30) の補完取得</strong> を併走させてください。\
         以降のセクションで具体的な分布・市場逼迫度・地域企業構造を確認します。",
        format_number(total as i64),
        salary_parse_pct,
        new_pct_label,
    );
    html.push_str(&format!(
        "<div class=\"so-what\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what_body
    ));

    html.push_str("</section>\n");
}

fn build_findings(
    agg: &SurveyAggregation,
    total: usize,
    dom_emp_pct: f64,
    new_pct: i64,
    salary_parse_pct: i64,
) -> Vec<(&'static str, String, String, String)> {
    let mut v: Vec<(&'static str, String, String, String)> = Vec::new();

    // 1) サンプル件数の信頼区間
    let (sev, body) = if total == 0 {
        ("neg", "サンプル 0 件のため統計値を提示できません。CSV 取得範囲の見直しが必要です。".to_string())
    } else if total < 30 {
        ("warn", format!("サンプル <strong>n={}</strong> は統計的信頼性が低く、外れ値の影響が大きい状態です。傾向参照に留め、母集団の追加取得を推奨します。", total))
    } else {
        ("pos", format!("サンプル <strong>n={}</strong> は実務判断に十分な水準です。後続セクションの統計値はそのまま参照できます。", total))
    };
    v.push((sev, "サンプル件数".to_string(), body, "§2 統計信頼性".to_string()));

    // 2) 主要雇用形態の偏り
    let (sev, body) = if dom_emp_pct >= 85.0 {
        ("warn", format!("主要雇用形態が <strong>{:.0}%</strong> を占め、構成が偏っています。他雇用形態のサンプル不足が示唆されるため、訴求軸の単一化リスクを点検してください。", dom_emp_pct))
    } else if dom_emp_pct >= 70.0 {
        ("neu", format!("主要雇用形態の構成比は <strong>{:.0}%</strong>。やや偏り気味ですが、他雇用形態への展開余地もある水準です。", dom_emp_pct))
    } else {
        ("pos", format!("主要雇用形態の構成比は <strong>{:.0}%</strong> で、バランスの取れた構成です。", dom_emp_pct))
    };
    v.push((sev, "雇用形態構成".to_string(), body, "§3 雇用形態分析".to_string()));

    // 3) 新着比率
    let (sev, body) = if total == 0 {
        ("neu", "サンプルなしのため新着比率の評価不能。".to_string())
    } else if new_pct >= 15 {
        ("pos", format!("直近 30 日の新着比率 <strong>{}%</strong> は活発な採用活動を示唆します。", new_pct))
    } else if new_pct < 5 {
        ("warn", format!("新着比率 <strong>{}%</strong> は低水準で、人材定着が進んでいる/求人活動が低調な可能性があります。", new_pct))
    } else {
        ("neu", format!("新着比率は <strong>{}%</strong>。標準的な水準です。", new_pct))
    };
    v.push((sev, "新着比率".to_string(), body, "§3 求人動向".to_string()));

    // 4) 給与解析率
    let (sev, body) = if salary_parse_pct >= 85 {
        ("pos", format!("給与解析率 <strong>{}%</strong> は高水準で、給与統計の信頼性は確保されています。", salary_parse_pct))
    } else if salary_parse_pct >= 60 {
        ("warn", format!("給与解析率 <strong>{}%</strong> は中程度。給与統計値の参照時には未解析分の影響を考慮してください。", salary_parse_pct))
    } else {
        ("neg", format!("給与解析率 <strong>{}%</strong> は低く、給与統計の代表性に注意が必要です。CSV の給与表記揺れを見直してください。", salary_parse_pct))
    };
    v.push((sev, "給与解析率".to_string(), body, "§4 給与統計".to_string()));

    // 5) 地域カバレッジ
    let pref_count = agg.by_prefecture.len();
    let (sev, body) = if pref_count == 0 {
        ("neu", "地域情報の抽出ができませんでした。CSV のアクセス列を確認してください。".to_string())
    } else if pref_count == 1 {
        ("neu", format!("カバー都道府県は <strong>1</strong> 都道府県。単一エリアの深掘り分析として参照可能です。"))
    } else {
        ("neu", format!("カバー都道府県は <strong>{}</strong>。複数地域比較は本レポート後半セクションで詳述します。", pref_count))
    };
    v.push((sev, "地域カバレッジ".to_string(), body, "§5 地域分析".to_string()));

    v
}

fn severity_label(tag: &str) -> &'static str {
    match tag {
        "pos" => "POS",
        "warn" => "WARN",
        "neg" => "NEG",
        _ => "NEU",
    }
}

// ============================================================
// Section 03: 給与分布 統計 (Phase 2 で navy 本実装)
// ============================================================

pub(super) fn render_navy_section_03_salary(
    html: &mut String,
    agg: &SurveyAggregation,
    salary_min_values: &[i64],
    salary_max_values: &[i64],
) {
    html.push_str("<section class=\"page-navy navy-salary\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 03",
        "給与分布 統計",
        "CSV 抽出済み下限・上限給与の分布と代表値",
    );

    // 統計値計算 (下限 / 上限 それぞれ)
    let stats_min = compute_distribution_stats(salary_min_values);
    let stats_max = compute_distribution_stats(salary_max_values);

    let salary_h = salary_summary::SalaryHeadline::from_aggregation(agg);
    let headline = salary_h.cover_highlight_text();
    let total = agg.total_count;
    let parse_pct = (agg.salary_parse_rate * 100.0).round() as i64;

    // -- exec-headline 風: 給与代表値を冒頭で 1 行に集約
    let lede = format!(
        "サンプル <strong>n={}</strong> / 給与解析率 <strong>{}%</strong>。\
         代表値: <strong>{} {}{}</strong>。本ページでは下限・上限給与それぞれの分布を確認します。",
        format_number(total as i64),
        parse_pct,
        escape_html(&headline.label),
        escape_html(&headline.value_text),
        escape_html(&headline.unit),
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI row 5 cell: P25 / 中央値 / 平均 / P75 / P90 (下限給与)
    if let Some(s) = stats_min.as_ref() {
        html.push_str("<div class=\"block-title\">図 3-1 &nbsp;下限給与 主要分位点 (月給換算 / 万円)</div>\n");
        html.push_str("<div class=\"kpi-row\">\n");
        push_kpi(html, "P25", &format_mm(s.p25), "万円", "neu", "下位 25% 水準", false);
        push_kpi(html, "中央値 P50", &format_mm(s.median), "万円", "neu", "サンプル中央値", true);
        push_kpi(html, "平均", &format_mm(s.mean), "万円", "neu", "外れ値の影響を含む", false);
        push_kpi(html, "P75", &format_mm(s.p75), "万円", "neu", "P75 ライン (P50 より上)", false);
        push_kpi(html, "P90", &format_mm(s.p90), "万円", "neu", "高給与帯", false);
        html.push_str("</div>\n");

        // -- histogram (10,000円刻み, 月給万単位)
        html.push_str("<div class=\"block-title block-title-spaced\">図 3-2 &nbsp;下限給与 分布 (10,000円刻み)</div>\n");
        html.push_str(&build_navy_histogram_svg(salary_min_values, s));
        html.push_str("<p class=\"caption\">縦線: 緑=中央値 / 金=平均 / 灰=最頻 bin</p>\n");
    } else {
        html.push_str("<p class=\"caption\">下限給与の有効値が不足しています (n=0 or 全 unparsed)。</p>\n");
    }

    // -- 上限給与
    if let Some(s) = stats_max.as_ref() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 3-3 &nbsp;上限給与 主要分位点 (月給換算 / 万円)</div>\n");
        html.push_str("<div class=\"kpi-row\">\n");
        push_kpi(html, "P25", &format_mm(s.p25), "万円", "neu", "下位 25% 水準", false);
        push_kpi(html, "中央値 P50", &format_mm(s.median), "万円", "neu", "サンプル中央値", true);
        push_kpi(html, "平均", &format_mm(s.mean), "万円", "neu", "外れ値の影響を含む", false);
        push_kpi(html, "P75", &format_mm(s.p75), "万円", "neu", "P75 ライン (P50 より上)", false);
        push_kpi(html, "P90", &format_mm(s.p90), "万円", "neu", "高給与帯", false);
        html.push_str("</div>\n");

        html.push_str("<div class=\"block-title block-title-spaced\">図 3-4 &nbsp;上限給与 分布 (10,000円刻み)</div>\n");
        html.push_str(&build_navy_histogram_svg(salary_max_values, s));
        html.push_str("<p class=\"caption\">縦線: 緑=中央値 / 金=平均 / 灰=最頻 bin</p>\n");
    } else {
        html.push_str("<p class=\"caption\">上限給与の有効値が不足しています。</p>\n");
    }

    // -- 集計サマリ table-navy
    html.push_str("<div class=\"block-title block-title-spaced\">表 3-A &nbsp;給与分布 集計サマリ (月給換算 / 万円)</div>\n");
    html.push_str(&build_navy_salary_summary_table(&stats_min, &stats_max));

    // -- So What
    let so_what = match (stats_min.as_ref(), stats_max.as_ref()) {
        (Some(lo), Some(hi)) => {
            let spread = hi.median - lo.median;
            let spread_label = format!("{:.1}万円", spread as f64 / 10000.0);
            format!(
                "下限給与 中央値 <strong>{}万円</strong> / 上限給与 中央値 <strong>{}万円</strong>、レンジ <strong>{}</strong>。\
                 給与レンジが <strong>5 万円未満</strong> なら「定額求人」、<strong>10 万円以上</strong> なら「歩合・等級制」の特徴が見えます。\
                 競合の中央値と比較し、訴求軸を <strong>下限保証</strong> / <strong>上限到達</strong> / <strong>レンジ幅</strong> のいずれに置くか検討してください。",
                format_mm(lo.median),
                format_mm(hi.median),
                spread_label,
            )
        }
        _ => "給与統計値が不足しています。CSV の給与カラム表記揺れを点検してください。".to_string(),
    };
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

// 分布統計 (月給換算済の i64 円 を入力。万円単位での出力用)
struct DistStats {
    n: usize,
    p25: i64,
    median: i64,
    p75: i64,
    p90: i64,
    mean: i64,
    min: i64,
    max: i64,
    mode_bin_yen: i64, // 10000 円刻み bin の代表値
    bins: Vec<usize>,  // ヒストグラム頻度
    bin_step: i64,     // bin 幅 (円)
    bin_start: i64,    // bin 0 の下端 (円)
}

fn compute_distribution_stats(values: &[i64]) -> Option<DistStats> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<i64> = values.iter().copied().filter(|x| *x > 0).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    let pct = |p: f64| -> i64 {
        let idx = ((n as f64 - 1.0) * p).round() as usize;
        v[idx.min(n - 1)]
    };
    let p25 = pct(0.25);
    let median = pct(0.5);
    let p75 = pct(0.75);
    let p90 = pct(0.90);
    let min = v[0];
    let max = v[n - 1];
    let sum: i64 = v.iter().sum();
    let mean = sum / n as i64;

    // ヒストグラム: 10,000 円刻みで P95 まで (それ以上は overflow バケット)
    let bin_step: i64 = 10_000;
    let bin_start: i64 = (min / bin_step) * bin_step;
    let p95 = pct(0.95);
    let upper = (p95 / bin_step + 1) * bin_step;
    let n_bins = (((upper - bin_start) / bin_step).max(1) as usize) + 1; // 最後はoverflow
    let mut bins = vec![0usize; n_bins];
    for &x in &v {
        let idx = ((x - bin_start) / bin_step) as i64;
        let idx_u = idx.clamp(0, (n_bins - 1) as i64) as usize;
        bins[idx_u] += 1;
    }
    // mode = 最頻 bin
    let (mode_idx, _) = bins
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .unwrap_or((0, &0));
    let mode_bin_yen = bin_start + mode_idx as i64 * bin_step + bin_step / 2;

    Some(DistStats {
        n,
        p25,
        median,
        p75,
        p90,
        mean,
        min,
        max,
        mode_bin_yen,
        bins,
        bin_step,
        bin_start,
    })
}

fn format_mm(yen: i64) -> String {
    format!("{:.1}", yen as f64 / 10000.0)
}

// navy ヒストグラム SVG (固定 720×280 / 罫線 var(--rule) / バー var(--ink-soft))
fn build_navy_histogram_svg(_values: &[i64], s: &DistStats) -> String {
    let w: f64 = 720.0;
    let h: f64 = 280.0;
    let pad_l = 56.0;
    let pad_r = 16.0;
    let pad_t = 16.0;
    let pad_b = 44.0;
    let inner_w = w - pad_l - pad_r;
    let inner_h = h - pad_t - pad_b;
    let n_bins = s.bins.len();
    let max_count = *s.bins.iter().max().unwrap_or(&1).max(&1) as f64;
    let bw = inner_w / n_bins as f64;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"給与ヒストグラム\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    ));
    // y 軸グリッド + ラベル (5 段)
    for i in 0..=5 {
        let y = pad_t + inner_h * i as f64 / 5.0;
        let count = (max_count * (5 - i) as f64 / 5.0).round() as i64;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ECE7DA\" stroke-width=\"0.5\"/>\n",
            pad_l,
            y,
            w - pad_r,
            y
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"end\">{}</text>\n",
            pad_l - 6.0,
            y + 3.0,
            count
        ));
    }
    // bars
    for (i, c) in s.bins.iter().enumerate() {
        let bh = (*c as f64 / max_count) * inner_h;
        let bx = pad_l + i as f64 * bw;
        let by = pad_t + inner_h - bh;
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#1F2D4D\"/>\n",
            bx + 0.5,
            by,
            (bw - 1.0).max(1.0),
            bh
        ));
    }
    // x 軸ラベル: bin の代表値 (10,000 円 ⇒ 万円表記、~6 ラベル)
    let label_step = (n_bins / 6).max(1);
    for (i, _c) in s.bins.iter().enumerate() {
        if i % label_step == 0 || i == n_bins - 1 {
            let cx = pad_l + (i as f64 + 0.5) * bw;
            let yen = s.bin_start + i as i64 * s.bin_step;
            let label = if i == n_bins - 1 && n_bins > 1 {
                format!("{}+", yen as f64 / 10000.0)
            } else {
                format!("{}", yen as f64 / 10000.0)
            };
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
                cx,
                h - pad_b + 14.0,
                label
            ));
        }
    }
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">月給 (万円)</text>\n",
        w / 2.0,
        h - 6.0
    ));
    // 中央値 (緑), 平均 (gold), 最頻 (灰) 縦線
    let x_of = |yen: i64| -> f64 {
        let bin_idx = ((yen - s.bin_start) as f64 / s.bin_step as f64).max(0.0);
        pad_l + (bin_idx + 0.5) * bw
    };
    let lines = [
        (x_of(s.median), "#1F6B43", "P50"),
        (x_of(s.mean), "#C9A24B", "平均"),
        (x_of(s.mode_bin_yen), "#9CA0AB", "最頻"),
    ];
    for (x, color, lbl) in lines {
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\" stroke-dasharray=\"3 2\"/>\n",
            x, pad_t, x, pad_t + inner_h, color
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"{}\" text-anchor=\"middle\" font-weight=\"700\">{}</text>\n",
            x, pad_t - 4.0, color, lbl
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

// navy 集計テーブル (下限 / 上限 × n/P25/P50/平均/P75/P90/min/max)
fn build_navy_salary_summary_table(
    lo: &Option<DistStats>,
    hi: &Option<DistStats>,
) -> String {
    let mut s = String::new();
    s.push_str("<table class=\"table-navy\">\n");
    s.push_str("<thead><tr>\
                <th>区分</th><th class=\"num\">n</th>\
                <th class=\"num\">最小</th>\
                <th class=\"num\">P25</th>\
                <th class=\"num\">中央値</th>\
                <th class=\"num\">平均</th>\
                <th class=\"num\">P75</th>\
                <th class=\"num\">P90</th>\
                <th class=\"num\">最大</th>\
                </tr></thead>\n<tbody>\n");
    let row = |label: &str, st: &Option<DistStats>| -> String {
        match st {
            Some(s) => format!(
                "<tr><td><strong>{}</strong></td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num dim\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num bold\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num dim\">{}</td>\
                 </tr>\n",
                label,
                format_number(s.n as i64),
                format_mm(s.min),
                format_mm(s.p25),
                format_mm(s.median),
                format_mm(s.mean),
                format_mm(s.p75),
                format_mm(s.p90),
                format_mm(s.max)
            ),
            None => format!(
                "<tr><td><strong>{}</strong></td><td colspan=\"8\" class=\"dim\">—</td></tr>\n",
                label
            ),
        }
    };
    s.push_str(&row("下限給与", lo));
    s.push_str(&row("上限給与", hi));
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">単位: 万円 (月給換算)。年俸は除外。時給×167 / 日給×21 で月給換算済み。</p>\n");
    s
}

// ============================================================
// Section 02: 地域 × 求人媒体データ連携 (Full) / 地域データ補強 (MI/Public)
// ============================================================

pub(super) fn render_navy_section_02_region(
    html: &mut String,
    agg: &SurveyAggregation,
    hw_context: Option<&InsightContext>,
    hw_enrichment_map: &std::collections::HashMap<String, super::super::hw_enrichment::HwAreaEnrichment>,
    variant: ReportVariant,
) {
    let show_hw = matches!(variant, ReportVariant::Full);
    let title = if show_hw { "地域 × 求人媒体データ連携" } else { "地域データ補強" };
    let sub = if show_hw {
        "CSV 件数最多 市区町村に求人媒体現在件数・推移を併記"
    } else {
        "CSV 件数最多 地域の公開統計指標を併記"
    };

    html.push_str("<section class=\"page-navy navy-region\" role=\"region\">\n");
    push_page_head(html, "SECTION 02", title, sub);

    let n_total = agg.total_count;
    let n_pref = agg.by_prefecture.len();
    let n_muni = agg.by_municipality_salary.len();

    // -- exec-headline
    let lede = format!(
        "対象 <strong>{}</strong> 都道府県 / <strong>{}</strong> 市区町村、サンプル <strong>n={}</strong>。\
         本ページでは件数最多 <strong>10</strong> 市区町村を抜粋し、{}を一覧化します。",
        n_pref,
        n_muni,
        format_number(n_total as i64),
        if show_hw {
            "CSV 集計値と求人媒体現在件数 (掲載求人ベース)"
        } else {
            "CSV 集計値と公開統計の地域指標"
        }
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- 都道府県カバレッジ KPI
    html.push_str("<div class=\"block-title\">図 2-1 &nbsp;都道府県カバレッジ サマリ</div>\n");
    let pref_top = agg
        .by_prefecture
        .first()
        .map(|(p, c)| (p.clone(), *c))
        .unwrap_or_default();
    let pref_top_pct = if n_total > 0 {
        pref_top.1 as f64 / n_total as f64 * 100.0
    } else {
        0.0
    };
    html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    push_kpi(
        html,
        "対象都道府県数",
        &format!("{}", n_pref),
        "県",
        "neu",
        "CSV から抽出された都道府県",
        false,
    );
    push_kpi(
        html,
        "対象市区町村数",
        &format!("{}", n_muni),
        "市町",
        "neu",
        "CSV から抽出された市区町村",
        false,
    );
    push_kpi(
        html,
        "件数最多 県",
        if pref_top.0.is_empty() { "—" } else { &pref_top.0 },
        "",
        "neu",
        "CSV 件数最多 1 県",
        true,
    );
    push_kpi(
        html,
        "最多県シェア",
        &format!("{:.1}", pref_top_pct),
        "%",
        if pref_top_pct >= 85.0 { "warn" } else { "neu" },
        "n に占める割合",
        false,
    );
    html.push_str("</div>\n");

    // -- table-navy: 件数最多 10 市区町村
    html.push_str(&format!(
        "<div class=\"block-title block-title-spaced\">表 2-A &nbsp;件数最多 10 市区町村 &mdash; CSV 集計 + {}</div>\n",
        if show_hw { "求人媒体補強" } else { "外部統計" }
    ));
    html.push_str(&build_navy_region_table(agg, hw_enrichment_map, show_hw));

    // -- so-what
    let so_what = build_region_so_what(agg, pref_top_pct, n_pref, hw_context, show_hw);
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

fn build_navy_region_table(
    agg: &SurveyAggregation,
    hw_enrichment_map: &std::collections::HashMap<String, super::super::hw_enrichment::HwAreaEnrichment>,
    show_hw: bool,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>都道府県</th><th>市区町村</th>");
    s.push_str("<th class=\"num\">CSV 件数</th>");
    s.push_str("<th class=\"num\">中央値 (万円)</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
        s.push_str("<th>3 ヶ月推移</th>");
        s.push_str("<th>1 年推移</th>");
    } else {
        s.push_str("<th>位置づけ</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    // 件数最多 10 市区町村 (CSV 件数降順)
    let top10: Vec<&super::super::aggregator::MunicipalitySalaryAgg> =
        agg.by_municipality_salary.iter().take(10).collect();

    if top10.is_empty() {
        s.push_str("<tr><td colspan=\"6\" class=\"dim\">CSV から市区町村集計データを抽出できませんでした。</td></tr>\n");
    } else {
        for (i, row) in top10.iter().enumerate() {
            let key = format!("{}:{}", row.prefecture, row.name);
            let enrich = hw_enrichment_map.get(&key);
            let med_man = format!("{:.1}", row.median_salary as f64 / 10000.0);
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            s.push_str(&format!(
                "<tr{}><td class=\"num bold\">{}</td><td>{}</td><td>{}</td>\
                 <td class=\"num bold\">{}</td><td class=\"num\">{}</td>",
                row_class,
                i + 1,
                escape_html(&row.prefecture),
                escape_html(&row.name),
                format_number(row.count as i64),
                med_man
            ));
            if show_hw {
                let posting = enrich
                    .map(|e| format_number(e.hw_posting_count))
                    .unwrap_or_else(|| "—".into());
                let trend_3m = enrich
                    .map(|e| {
                        let label = e.change_label_3m();
                        let tag = match label {
                            "大きく増加" | "緩やかに増加" => "pos",
                            "横ばい" => "neu",
                            _ => "warn",
                        };
                        format!(
                            "<span class=\"tag tag-{}\">{}{}</span>",
                            tag,
                            label,
                            e.posting_change_3m_pct
                                .map(|v| format!(" ({:+.1}%)", v))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| "<span class=\"dim\">—</span>".into());
                let trend_1y = enrich
                    .map(|e| {
                        let label = e.change_label_1y();
                        let tag = match label {
                            "大きく増加" | "緩やかに増加" => "pos",
                            "横ばい" => "neu",
                            _ => "warn",
                        };
                        format!(
                            "<span class=\"tag tag-{}\">{}{}</span>",
                            tag,
                            label,
                            e.posting_change_1y_pct
                                .map(|v| format!(" ({:+.1}%)", v))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_else(|| "<span class=\"dim\">—</span>".into());
                s.push_str(&format!(
                    "<td class=\"num\">{}</td><td>{}</td><td>{}</td>",
                    posting, trend_3m, trend_1y
                ));
            } else {
                // MI/Public: 位置づけ (シェア + tag)
                let pct = if agg.total_count > 0 {
                    row.count as f64 / agg.total_count as f64 * 100.0
                } else {
                    0.0
                };
                let tag = if pct >= 30.0 {
                    "pos"
                } else if pct >= 10.0 {
                    "neu"
                } else {
                    "neu"
                };
                let label = if pct >= 30.0 {
                    "中核エリア"
                } else if pct >= 10.0 {
                    "主要エリア"
                } else {
                    "周辺エリア"
                };
                s.push_str(&format!(
                    "<td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:.1}%</span></td>",
                    tag, label, pct
                ));
            }
            s.push_str("</tr>\n");
        }
    }
    s.push_str("</tbody></table>\n");
    if show_hw {
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数。中央値: 月給換算済み。媒体掲載数: 求人媒体ローカル DB の現在掲載求人数。推移: 3 ヶ月前比 / 1 年前比 (Turso 時系列)。</p>\n");
    } else {
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数。中央値: 月給換算済み。位置づけ: n に占める割合に基づき中核 (30%+) / 主要 (10-30%) / 周辺 (-10%) に分類。</p>\n");
    }
    s
}

fn build_region_so_what(
    agg: &SurveyAggregation,
    pref_top_pct: f64,
    n_pref: usize,
    hw_context: Option<&InsightContext>,
    show_hw: bool,
) -> String {
    let muni_top = agg.by_municipality_salary.first();
    let muni_top_pct = match muni_top {
        Some(m) if agg.total_count > 0 => m.count as f64 / agg.total_count as f64 * 100.0,
        _ => 0.0,
    };

    let geo_judge = if n_pref == 1 {
        "<strong>単一県集中</strong>"
    } else if pref_top_pct >= 70.0 {
        "<strong>1 県主導 (他県補助)</strong>"
    } else if n_pref >= 5 {
        "<strong>広域分散</strong>"
    } else {
        "<strong>複数県均衡</strong>"
    };

    let concentration_note = if muni_top_pct >= 50.0 {
        format!(
            "件数最多市区町村 <strong>{}</strong> が <strong>{:.0}%</strong> を占め、エリア依存度が極めて高い構成です。",
            muni_top.map(|m| m.name.as_str()).unwrap_or("—"),
            muni_top_pct
        )
    } else if muni_top_pct >= 25.0 {
        format!(
            "件数最多市区町村 <strong>{}</strong> が <strong>{:.0}%</strong> を占めます。中核エリア + 主要エリアでの面取り戦略が有効です。",
            muni_top.map(|m| m.name.as_str()).unwrap_or("—"),
            muni_top_pct
        )
    } else {
        "件数は複数エリアに分散しており、地域別の訴求軸調整が必要です。".to_string()
    };

    let hw_note = if show_hw && hw_context.is_some() {
        " 求人媒体側の <strong>3 ヶ月 / 1 年推移</strong> も併せて確認し、減少基調のエリアは <strong>媒体露出強化</strong>、増加基調のエリアは <strong>競合増加に伴う差別化</strong> を検討してください。"
    } else {
        ""
    };

    format!(
        "対象地域の構造は {} です。{}{}",
        geo_judge, concentration_note, hw_note
    )
}

// ============================================================
// Section 04: 採用市場 逼迫度 (Phase 2 navy 本実装)
// ============================================================

struct TightnessData {
    job_ratio: Option<f64>,       // 有効求人倍率
    vacancy_rate: Option<f64>,    // HW 欠員補充率 (0-1)
    unemployment: Option<f64>,    // 失業率 (%)
    unemployment_national: Option<f64>, // 全国平均失業率 (%)
    separation: Option<f64>,      // 離職率 (%)
    entry: Option<f64>,           // 入職率 (%)
}

fn extract_tightness(ctx: &InsightContext) -> TightnessData {
    use super::super::super::helpers::{get_f64, get_str_ref};
    let job_ratio = ctx
        .ext_job_ratio
        .last()
        .map(|r| get_f64(r, "ratio_total"))
        .filter(|v| *v > 0.0);
    let vacancy_rate = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .filter(|v| *v > 0.0);
    let unemployment = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "unemployment_rate"))
        .filter(|v| *v > 0.0);
    let (separation, entry) = ctx
        .ext_turnover
        .last()
        .map(|r| (get_f64(r, "separation_rate"), get_f64(r, "entry_rate")))
        .map(|(s, e)| (Some(s).filter(|v| *v > 0.0), Some(e).filter(|v| *v > 0.0)))
        .unwrap_or((None, None));
    TightnessData {
        job_ratio,
        vacancy_rate,
        unemployment,
        unemployment_national: ctx.pref_avg_unemployment_rate,
        separation,
        entry,
    }
}

pub(super) fn render_navy_section_04_market_tightness(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
) {
    html.push_str("<section class=\"page-navy navy-tightness\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 04",
        "採用市場 逼迫度",
        "有効求人倍率 / 失業率 / 離職率 を統合した複合指標",
    );

    let data = hw_context.map(extract_tightness);
    let show_vacancy = matches!(variant, ReportVariant::Full); // HW 欠員補充率は Full のみ

    let lede = match data.as_ref() {
        Some(d) => format!(
            "対象地域の採用難度を測る 4 指標を提示します。\
             有効求人倍率 <strong>{}</strong> / 失業率 <strong>{}</strong> / 離職率 <strong>{}</strong>{}。",
            fmt_ratio(d.job_ratio),
            fmt_pct(d.unemployment),
            fmt_pct(d.separation),
            if show_vacancy {
                format!(" / HW 欠員補充率 <strong>{}</strong>", fmt_pct_from_ratio(d.vacancy_rate))
            } else {
                String::new()
            }
        ),
        None => "外部統計データが取得できなかったため、本セクションは指標のみのプレースホルダで出力します。".to_string(),
    };
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI row (4 cell Full / 3 cell MI/Public)
    let d = data.as_ref();
    html.push_str("<div class=\"block-title\">図 4-1 &nbsp;採用難度 主要 4 指標</div>\n");
    if show_vacancy {
        html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    } else {
        html.push_str("<div class=\"kpi-row kpi-row-3\">\n");
    }
    {
        let (val, dot, foot) = match d.and_then(|d| d.job_ratio) {
            Some(v) if v >= 1.5 => (fmt_ratio(Some(v)), "warn", "1.5 以上は採用難度 高 (応募集めにくい)".to_string()),
            Some(v) if v >= 1.0 => (fmt_ratio(Some(v)), "neu", "1.0 以上は売り手市場".to_string()),
            Some(v) => (fmt_ratio(Some(v)), "pos", format!("1.0 未満 ({:.2}) は買い手市場", v)),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "有効求人倍率", &val, "倍", dot, &foot, true);
    }
    if show_vacancy {
        let (val, dot, foot) = match d.and_then(|d| d.vacancy_rate) {
            Some(v) if v >= 0.25 => (fmt_pct_from_ratio(Some(v)), "warn", "25% 超は採用難度 高".to_string()),
            Some(v) if v >= 0.15 => (fmt_pct_from_ratio(Some(v)), "neu", "15-25% は標準的".to_string()),
            Some(v) => (fmt_pct_from_ratio(Some(v)), "pos", "15% 未満は採用充足".to_string()),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "HW 欠員補充率", &val, "%", dot, &foot, false);
    }
    {
        let unemp = d.and_then(|d| d.unemployment);
        let nat = d.and_then(|d| d.unemployment_national);
        let (val, dot, foot) = match (unemp, nat) {
            (Some(u), Some(n)) => {
                let diff = u - n;
                let dot = if u < 2.5 { "warn" } else if u < 3.5 { "neu" } else { "pos" };
                let foot = format!("全国平均 {:.1}% / 差 {:+.1}pt", n, diff);
                (format!("{:.1}", u), dot, foot)
            }
            (Some(u), None) => (format!("{:.1}", u), "neu", "全国平均データなし".to_string()),
            _ => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "失業率", &val, "%", dot, &foot, false);
    }
    {
        let (val, dot, foot) = match d.and_then(|d| d.separation) {
            Some(v) if v >= 15.0 => (format!("{:.1}", v), "warn", "15% 超は離職多発エリア / 業界".to_string()),
            Some(v) if v >= 10.0 => (format!("{:.1}", v), "neu", "10-15% は標準的水準".to_string()),
            Some(v) => (format!("{:.1}", v), "pos", "10% 未満は定着率 高".to_string()),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "離職率", &val, "%", dot, &foot, false);
    }
    html.push_str("</div>\n");

    // -- gauge SVG (4 軸正規化、横バー)
    if let Some(d) = data.as_ref() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 4-2 &nbsp;採用難度 ゲージ (正規化 0-100)</div>\n");
        html.push_str(&build_navy_tightness_gauges(d, show_vacancy));
        html.push_str("<p class=\"caption\">ゲージは 0 (緩やか) - 100 (厳しい) に正規化。緑帯=安全 / 金帯=注意 / 赤帯=採用難度 高。</p>\n");
    }

    // -- table-navy 集計
    html.push_str("<div class=\"block-title block-title-spaced\">表 4-A &nbsp;採用市場 指標サマリ</div>\n");
    html.push_str(&build_navy_tightness_table(d, show_vacancy));

    // -- so-what 採用難度総合評価
    let so_what = build_tightness_so_what(d, show_vacancy);
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

fn fmt_ratio(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.2}", x),
        None => "—".to_string(),
    }
}
fn fmt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.1}%", x),
        None => "—".to_string(),
    }
}
fn fmt_pct_from_ratio(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.1}", x * 100.0),
        None => "—".to_string(),
    }
}

/// 採用難度ゲージ (横バー、4 軸 or 3 軸)
fn build_navy_tightness_gauges(d: &TightnessData, show_vacancy: bool) -> String {
    // 各指標を 0-100 に正規化:
    // - 有効求人倍率: 0.5→0, 1.0→50, 2.0→100 (>2 で 100 clamp)
    // - HW 欠員補充率: 0%→0, 15%→50, 30%→100
    // - 失業率: 6%→0 (緩やか), 3%→50, 1.5%→100 (採用難度 高 = 失業率低)
    // - 離職率: 5%→0, 10%→50, 20%→100
    let mut items: Vec<(&str, f64, &str, &str)> = Vec::new(); // (label, score 0-100, fmt_val, sev)
    if let Some(r) = d.job_ratio {
        let s = ((r - 0.5) / 1.5).clamp(0.0, 1.0) * 100.0;
        let sev = if s >= 70.0 { "warn" } else if s >= 40.0 { "neu" } else { "pos" };
        items.push(("有効求人倍率", s, leak(&format!("{:.2} 倍", r)), sev));
    }
    if show_vacancy {
        if let Some(v) = d.vacancy_rate {
            let s = (v / 0.30).clamp(0.0, 1.0) * 100.0;
            let sev = if s >= 70.0 { "warn" } else if s >= 40.0 { "neu" } else { "pos" };
            items.push(("HW 欠員補充率", s, leak(&format!("{:.1}%", v * 100.0)), sev));
        }
    }
    if let Some(u) = d.unemployment {
        let s = ((6.0 - u) / 4.5).clamp(0.0, 1.0) * 100.0;
        let sev = if s >= 70.0 { "warn" } else if s >= 40.0 { "neu" } else { "pos" };
        items.push(("失業率 (低=採用難)", s, leak(&format!("{:.1}%", u)), sev));
    }
    if let Some(sep) = d.separation {
        let s = ((sep - 5.0) / 15.0).clamp(0.0, 1.0) * 100.0;
        let sev = if s >= 70.0 { "warn" } else if s >= 40.0 { "neu" } else { "pos" };
        items.push(("離職率", s, leak(&format!("{:.1}%", sep)), sev));
    }

    if items.is_empty() {
        return "<p class=\"caption\">ゲージ表示に必要なデータが不足しています。</p>\n".to_string();
    }

    let row_h = 36.0;
    let h = 30.0 + items.len() as f64 * row_h + 12.0;
    let w = 720.0;
    let label_w = 160.0;
    let val_w = 80.0;
    let bar_x = label_w;
    let bar_w = w - label_w - val_w - 16.0;

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"採用難度ゲージ\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );
    // 凡例帯 (背景: 緑→金→赤)
    let y0 = 20.0;
    for (i, item) in items.iter().enumerate() {
        let (label, score, val, sev) = (item.0, item.1, item.2, item.3);
        let cy = y0 + i as f64 * row_h;
        // ラベル
        svg.push_str(&format!(
            "<text x=\"4\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"600\">{}</text>\n",
            cy + 14.0,
            escape_html(label)
        ));
        // 背景帯 (3 セグメント: 0-40 緑薄 / 40-70 金薄 / 70-100 赤薄)
        let seg_x1 = bar_x + bar_w * 0.40;
        let seg_x2 = bar_x + bar_w * 0.70;
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"#DDEDE2\"/>\n",
            bar_x, cy + 8.0, seg_x1 - bar_x
        ));
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"#FAEBD2\"/>\n",
            seg_x1, cy + 8.0, seg_x2 - seg_x1
        ));
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"#F4DDD7\"/>\n",
            seg_x2, cy + 8.0, bar_w - (seg_x2 - bar_x)
        ));
        // フレーム
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"none\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
            bar_x, cy + 8.0, bar_w
        ));
        // マーカー (current)
        let marker_x = bar_x + bar_w * score / 100.0;
        let marker_color = match sev {
            "pos" => "#1F6B43",
            "warn" => "#A8331F",
            _ => "#0B1E3F",
        };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"3\" height=\"20\" fill=\"{}\"/>\n",
            marker_x - 1.5, cy + 4.0, marker_color
        ));
        // 値ラベル (右側)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-family=\"Roboto Mono, monospace\" font-weight=\"700\" text-anchor=\"end\">{}</text>\n",
            w - 6.0,
            cy + 18.0,
            escape_html(val)
        ));
    }
    // 凡例
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"14\" font-size=\"9\" fill=\"#6A6E7A\">0 (緩やか)</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"middle\">50</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"end\">100 (厳しい)</text>\n",
        bar_x,
        bar_x + bar_w / 2.0,
        bar_x + bar_w
    ));
    svg.push_str("</svg>\n");
    svg
}

// leak helper: format! の戻り String を &'static に変えるためのトリック。
// build_navy_tightness_gauges 内の (&str, ..., &str) ベクタ要素が
// 一時的に str を借りる用途。本関数は短時間のみ使う(関数内のみ参照)ので
// メモリリークは無視可能 (実利用上、Section 04 を 1 回しか呼ばないため
// 文字列の総量は最大十数バイト×4 件 = 100 バイト未満)。
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn build_navy_tightness_table(d: Option<&TightnessData>, show_vacancy: bool) -> String {
    let mut s = String::from(
        "<table class=\"table-navy\">\n\
         <thead><tr>\
         <th>指標</th><th class=\"num\">対象地域</th><th class=\"num\">参考値</th>\
         <th>採用難度</th><th>解釈</th>\
         </tr></thead>\n<tbody>\n",
    );
    let row = |label: &str, value: String, reference: &str, tag: &str, comment: &str| -> String {
        format!(
            "<tr><td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num dim\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             <td>{}</td></tr>\n",
            label,
            value,
            reference,
            tag,
            severity_label(tag),
            comment
        )
    };
    let d = d;
    // job_ratio
    let (val, tag, cmt) = match d.and_then(|d| d.job_ratio) {
        Some(v) if v >= 1.5 => (format!("{:.2}", v), "warn", "応募集めにくい (1.5+)"),
        Some(v) if v >= 1.0 => (format!("{:.2}", v), "neu", "売り手市場 (1.0-1.5)"),
        Some(v) => (format!("{:.2}", v), "pos", "買い手市場 (-1.0)"),
        None => ("—".to_string(), "neu", "—"),
    };
    s.push_str(&row("有効求人倍率", val, "全国 1.20", tag, cmt));
    if show_vacancy {
        let (val, tag, cmt) = match d.and_then(|d| d.vacancy_rate) {
            Some(v) if v >= 0.25 => (format!("{:.1}%", v * 100.0), "warn", "HW 求人埋まらず"),
            Some(v) if v >= 0.15 => (format!("{:.1}%", v * 100.0), "neu", "標準水準"),
            Some(v) => (format!("{:.1}%", v * 100.0), "pos", "充足傾向"),
            None => ("—".to_string(), "neu", "—"),
        };
        s.push_str(&row("HW 欠員補充率", val, "標準 15-25%", tag, cmt));
    }
    let unemp = d.and_then(|d| d.unemployment);
    let nat = d.and_then(|d| d.unemployment_national);
    let (val, tag, cmt) = match unemp {
        Some(u) if u < 2.5 => (format!("{:.1}%", u), "warn", "低失業=採用難度 高"),
        Some(u) if u < 3.5 => (format!("{:.1}%", u), "neu", "標準的水準"),
        Some(u) => (format!("{:.1}%", u), "pos", "求職者プールあり"),
        None => ("—".to_string(), "neu", "—"),
    };
    let nat_str = nat.map(|n| format!("全国 {:.1}%", n)).unwrap_or_else(|| "—".to_string());
    s.push_str(&row("失業率", val, &nat_str, tag, cmt));
    let (val, tag, cmt) = match d.and_then(|d| d.separation) {
        Some(v) if v >= 15.0 => (format!("{:.1}%", v), "warn", "離職多発"),
        Some(v) if v >= 10.0 => (format!("{:.1}%", v), "neu", "標準水準"),
        Some(v) => (format!("{:.1}%", v), "pos", "定着率 高"),
        None => ("—".to_string(), "neu", "—"),
    };
    s.push_str(&row("離職率", val, "全国 14.6%", tag, cmt));
    if let Some(d) = d {
        let (val, tag, cmt) = match d.entry {
            Some(v) if v >= 16.0 => (format!("{:.1}%", v), "neu", "入職活発 (転職市場活況)"),
            Some(v) if v >= 10.0 => (format!("{:.1}%", v), "neu", "標準水準"),
            Some(v) => (format!("{:.1}%", v), "neu", "入職停滞"),
            None => ("—".to_string(), "neu", "—"),
        };
        s.push_str(&row("入職率 (参考)", val, "全国 15.4%", tag, cmt));
    }
    s.push_str("</tbody></table>\n");
    if show_vacancy {
        s.push_str("<p class=\"caption\">出典: e-Stat 有効求人倍率 / 労働力調査 (失業率) / 雇用動向調査 (離職率・入職率)。求人媒体欠員補充率はローカル DB。</p>\n");
    } else {
        s.push_str("<p class=\"caption\">出典: e-Stat 有効求人倍率 / 労働力調査 (失業率) / 雇用動向調査 (離職率・入職率)。</p>\n");
    }
    s
}

fn build_tightness_so_what(d: Option<&TightnessData>, _show_vacancy: bool) -> String {
    let d = match d {
        Some(d) => d,
        None => {
            return "外部統計データが取得できなかったため、本セクションは指標説明のみとなります。CSV \
                    側のサンプル数が一定 (n>=30) ある場合、後続セクションでの判断は継続可能です。"
                .to_string()
        }
    };
    let mut alerts: Vec<&str> = Vec::new();
    if let Some(r) = d.job_ratio {
        if r >= 1.5 {
            alerts.push("有効求人倍率");
        }
    }
    if let Some(u) = d.unemployment {
        if u < 2.5 {
            alerts.push("低失業率");
        }
    }
    if let Some(s) = d.separation {
        if s >= 15.0 {
            alerts.push("離職率");
        }
    }
    if let Some(v) = d.vacancy_rate {
        if v >= 0.25 {
            alerts.push("HW 欠員補充率");
        }
    }

    if alerts.len() >= 2 {
        format!(
            "<strong>採用難度 高</strong>。{} の 2 指標以上で警戒水準。\
             <strong>給与・福利厚生による差別化</strong> と <strong>応募経路の多元化</strong> を併走させてください。\
             特に離職多発エリアの場合は <strong>定着率向上施策</strong> を組み合わせる必要があります。",
            alerts.join(" / ")
        )
    } else if alerts.len() == 1 {
        format!(
            "<strong>採用難度 中</strong>。{} で警戒水準。\
             該当指標に対応する個別施策 (給与水準 / 訴求軸 / 採用チャネル) を優先検討してください。",
            alerts[0]
        )
    } else {
        "<strong>採用難度 低</strong>。主要指標はいずれも警戒水準を下回ります。\
         CSV 上の特徴 (給与水準 / 雇用形態 / 訴求軸) を活かした候補者選別重視で問題ありません。"
            .to_string()
    }
}

// ============================================================
// Section 05: 地域企業構造 (Phase 3 navy 本実装) ※定義は別位置 (下方)
// ============================================================
// (実装は本ファイル末尾に追加 — render_navy_section_05_companies)

// ============================================================
// Section 06-08 placeholder (Phase 3-4 で本実装に差し替え)
// ============================================================

pub(super) fn render_navy_section_placeholders(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    now: &str,
) {
    let _ = (hw_context, variant, now);
    let sections = [
        ("SECTION 07", "最低賃金・ライフスタイル", "最低賃金推移 / 家計支出構成 / 通勤圏。Phase 4 で実装予定。"),
        ("SECTION 08", "注記・出典・免責", "データソース / 集計定義 / 免責事項。Phase 4 で実装予定。"),
    ];
    for (code, title, body_text) in sections {
        html.push_str("<section class=\"page-navy\" role=\"region\">\n");
        push_page_head(html, code, title, "Round 24 段階移行: navy_report で本実装に差し替え中");
        html.push_str(&format!(
            "<div class=\"so-what\" style=\"margin-top:4mm;\">\
             <div class=\"sw-label\">UNDER MIGRATION</div>\
             <div class=\"sw-body\">{}<br>本セクションは新デザイン (見本 Recruitment_Market_Report.html) に\
             基づき再構築中です。次のコミット群で navy 構造の本実装に置き換わります。</div>\
             </div>\n",
            escape_html(body_text)
        ));
        html.push_str("</section>\n");
    }
}

// ============================================================
// 共通: page-head / kpi cell
// ============================================================

fn push_page_head(html: &mut String, section_code: &str, title: &str, sub: &str) {
    html.push_str(&format!(
        "<div class=\"page-head\">\
         <div class=\"ph-sec\">{}</div>\
         <div class=\"ph-title\">{}</div>\
         <div class=\"ph-sub\">{}</div>\
         <div class=\"ph-rule\" aria-hidden=\"true\"></div>\
         </div>\n",
        escape_html(section_code),
        escape_html(title),
        escape_html(sub),
    ));
}

fn push_kpi(
    html: &mut String,
    label: &str,
    value: &str,
    unit: &str,
    dot: &str,
    foot: &str,
    emphasis: bool,
) {
    let cls = if emphasis { "kpi kpi-emphasis" } else { "kpi" };
    html.push_str(&format!(
        "<div class=\"{cls}\">\
         <div class=\"kpi-label\">{label}</div>\
         <div class=\"kpi-value\">{value}<span class=\"kpi-unit\">{unit}</span></div>\
         <div class=\"kpi-foot\"><span class=\"dot {dot}\"></span>{foot}</div>\
         </div>\n",
        cls = cls,
        label = escape_html(label),
        value = escape_html(value),
        unit = escape_html(unit),
        dot = dot,
        foot = foot,
    ));
}

// ============================================================
// Section 05: 地域企業構造 — 関数本体
// ============================================================

pub(super) fn render_navy_section_05_companies(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    by_company: &[super::super::aggregator::CompanyAgg],
    salesnow_segments: &super::super::super::company::fetch::RegionalCompanySegments,
    variant: ReportVariant,
) {
    let show_hw = matches!(variant, ReportVariant::Full);

    html.push_str("<section class=\"page-navy navy-companies\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 05",
        "地域企業構造",
        "産業構成 / 法人セグメント / 規模帯ベンチマーク",
    );

    let industry_employees: Vec<(String, i64)> = hw_context
        .map(|ctx| {
            use super::super::super::helpers::{get_f64, get_str};
            ctx.ext_industry_employees
                .iter()
                .map(|r| (get_str(r, "industry_name"), get_f64(r, "employees_total") as i64))
                .filter(|(n, c)| !n.is_empty() && *c > 0)
                .collect()
        })
        .unwrap_or_default();
    let mut industry_sorted = industry_employees.clone();
    industry_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let industry_total: i64 = industry_sorted.iter().map(|(_, c)| *c).sum();

    let hw_industry: Vec<(String, i64)> = hw_context
        .map(|ctx| ctx.hw_industry_counts.clone())
        .unwrap_or_default();
    let hw_total: i64 = hw_industry.iter().map(|(_, c)| *c).sum();

    let pool_size = salesnow_segments.pool_size;
    let n_large = salesnow_segments.large.len();
    let n_mid = salesnow_segments.mid.len();
    let n_growth = salesnow_segments.growth.len();
    let n_hiring = salesnow_segments.hiring.len();
    let n_companies_csv = by_company.len();

    let lede = format!(
        "対象地域の企業構造を把握します。国勢調査 産業大分類 <strong>{}</strong> 区分 / \
         地域企業データ <strong>{}</strong> 社{}。CSV 上にユニーク企業 <strong>{}</strong> 社が確認できます。",
        industry_sorted.len(),
        format_number(pool_size as i64),
        if show_hw && hw_total > 0 {
            format!(" / 求人媒体 産業大分類 {} 件", format_number(hw_total))
        } else {
            String::new()
        },
        format_number(n_companies_csv as i64),
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    html.push_str("<div class=\"block-title\">図 5-1 &nbsp;法人セグメント (規模 × 動向)</div>\n");
    html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    push_kpi(html, "大手企業", &format!("{}", n_large), "社", "neu", "従業員 300+ 名級", false);
    push_kpi(html, "中堅企業", &format!("{}", n_mid), "社", "neu", "従業員 50-299 名", false);
    push_kpi(
        html,
        "急成長企業",
        &format!("{}", n_growth),
        "社",
        if n_growth > 0 { "pos" } else { "neu" },
        "1Y 人員増加率 +10% 超",
        true,
    );
    if show_hw {
        push_kpi(
            html,
            "採用活発企業",
            &format!("{}", n_hiring),
            "社",
            if n_hiring > 0 { "warn" } else { "neu" },
            "求人媒体掲載 5 件以上",
            false,
        );
    } else {
        push_kpi(
            html,
            "母集団規模",
            &format!("{}", format_number(pool_size as i64)),
            "社",
            "neu",
            "地域企業データ取得社数",
            false,
        );
    }
    html.push_str("</div>\n");

    html.push_str("<div class=\"block-title block-title-spaced\">表 5-A &nbsp;産業大分類 構成 (件数最多 8 産業)</div>\n");
    html.push_str(&build_navy_industry_table(
        &industry_sorted,
        industry_total,
        &hw_industry,
        hw_total,
        show_hw,
    ));

    if !industry_sorted.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 5-2 &nbsp;産業大分類シェア (国勢調査)</div>\n");
        html.push_str(&build_navy_industry_bars(&industry_sorted, industry_total));
        html.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_industry_structure (都道府県粒度)。集計コード AS/AR/CR 除外。</p>\n");
    }

    if !salesnow_segments.growth.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-B &nbsp;急成長企業 (1Y +10% 超、件数最多 8 社)</div>\n");
        html.push_str(&build_navy_company_list(&salesnow_segments.growth, 8, show_hw));
    }

    let so_what = build_companies_so_what(
        &industry_sorted,
        industry_total,
        pool_size,
        n_growth,
        n_hiring,
        show_hw,
    );
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

fn build_navy_industry_table(
    industry_sorted: &[(String, i64)],
    industry_total: i64,
    hw_industry: &[(String, i64)],
    hw_total: i64,
    show_hw: bool,
) -> String {
    let hw_map: std::collections::HashMap<&str, i64> =
        hw_industry.iter().map(|(n, c)| (n.as_str(), *c)).collect();

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>産業大分類</th>");
    s.push_str("<th class=\"num\">就業者数</th>");
    s.push_str("<th class=\"num\">シェア</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
        s.push_str("<th class=\"num\">媒体シェア</th>");
        s.push_str("<th>差分</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let top8: Vec<&(String, i64)> = industry_sorted.iter().take(8).collect();
    if top8.is_empty() {
        let cols = if show_hw { 7 } else { 4 };
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">国勢調査産業構造データを取得できませんでした。</td></tr>\n",
            cols
        ));
    } else {
        for (i, (name, employees)) in top8.iter().enumerate() {
            let share_pct = if industry_total > 0 {
                *employees as f64 / industry_total as f64 * 100.0
            } else {
                0.0
            };
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            s.push_str(&format!(
                "<tr{}><td class=\"num bold\">{}</td><td><strong>{}</strong></td>\
                 <td class=\"num bold\">{}</td><td class=\"num\">{:.1}%</td>",
                row_class,
                i + 1,
                escape_html(name),
                format_number(*employees),
                share_pct
            ));
            if show_hw {
                let hw_count = hw_map.get(name.as_str()).copied().unwrap_or(0);
                let hw_share = if hw_total > 0 {
                    hw_count as f64 / hw_total as f64 * 100.0
                } else {
                    0.0
                };
                let diff = hw_share - share_pct;
                let (tag, label) = if diff >= 5.0 {
                    ("warn", "媒体側に偏り")
                } else if diff <= -5.0 {
                    ("neu", "就業者構成優位")
                } else {
                    ("neu", "ほぼ均衡")
                };
                s.push_str(&format!(
                    "<td class=\"num\">{}</td><td class=\"num\">{:.1}%</td>\
                     <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:+.1}pt</span></td>",
                    format_number(hw_count),
                    hw_share,
                    tag,
                    label,
                    diff
                ));
            }
            s.push_str("</tr>\n");
        }
    }
    s.push_str("</tbody></table>\n");
    if show_hw {
        s.push_str("<p class=\"caption\">就業者数は国勢調査ベース、媒体掲載数は求人媒体ローカル DB。差分 (媒体シェア − 就業者シェア) は採用需要の偏りを示します。</p>\n");
    } else {
        s.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_industry_structure (都道府県粒度)。集計コード AS/AR/CR 除外。</p>\n");
    }
    s
}

fn build_navy_industry_bars(industry_sorted: &[(String, i64)], total: i64) -> String {
    let top10: Vec<&(String, i64)> = industry_sorted.iter().take(10).collect();
    if top10.is_empty() || total <= 0 {
        return String::new();
    }
    let w = 720.0;
    let row_h = 24.0;
    let label_w = 200.0;
    let val_w = 90.0;
    let bar_x = label_w;
    let bar_w = w - label_w - val_w - 16.0;
    let h = top10.len() as f64 * row_h + 20.0;

    let max_share = top10
        .iter()
        .map(|(_, c)| *c as f64 / total as f64)
        .fold(0.0, f64::max)
        .max(0.01);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"産業構成バー\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );
    for (i, (name, count)) in top10.iter().enumerate() {
        let share = *count as f64 / total as f64;
        let cy = 10.0 + i as f64 * row_h;
        let bw_cur = bar_w * (share / max_share);
        svg.push_str(&format!(
            "<text x=\"4\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"600\">{}</text>\n",
            cy + 14.0,
            escape_html(name)
        ));
        let bar_color = if i == 0 { "#C9A24B" } else { "#1F2D4D" };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"{}\"/>\n",
            bar_x,
            cy + 4.0,
            bw_cur.max(0.5),
            bar_color
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-family=\"Roboto Mono, monospace\" font-weight=\"700\" text-anchor=\"end\">{:.1}%</text>\n",
            w - 6.0,
            cy + 14.0,
            share * 100.0
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

fn build_navy_company_list(
    companies: &[super::super::super::company::fetch::NearbyCompany],
    take: usize,
    show_hw: bool,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>企業名</th><th>産業</th>");
    s.push_str("<th class=\"num\">従業員数</th>");
    s.push_str("<th class=\"num\">1Y 増減</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let top: Vec<_> = companies.iter().take(take).collect();
    if top.is_empty() {
        let cols = if show_hw { 6 } else { 5 };
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">該当企業データなし。</td></tr>\n",
            cols
        ));
    } else {
        for (i, c) in top.iter().enumerate() {
            let delta = c.employee_delta_1y;
            let delta_tag = if delta >= 0.05 {
                "pos"
            } else if delta <= -0.05 {
                "warn"
            } else {
                "neu"
            };
            s.push_str(&format!(
                "<tr><td class=\"num bold\">{}</td><td><strong>{}</strong></td><td><span class=\"dim\">{}</span></td>\
                 <td class=\"num bold\">{}</td>\
                 <td class=\"num\"><span class=\"tag tag-{}\">{:+.1}%</span></td>",
                i + 1,
                escape_html(&c.company_name),
                escape_html(&c.sn_industry),
                format_number(c.employee_count),
                delta_tag,
                delta * 100.0
            ));
            if show_hw {
                s.push_str(&format!(
                    "<td class=\"num\">{}</td>",
                    if c.hw_posting_count > 0 {
                        format_number(c.hw_posting_count)
                    } else {
                        "—".to_string()
                    }
                ));
            }
            s.push_str("</tr>\n");
        }
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">地域企業データ (SalesNow) より、1 年人員増加率 +10% 超を「急成長」と定義。</p>\n");
    s
}

fn build_companies_so_what(
    industry_sorted: &[(String, i64)],
    industry_total: i64,
    pool_size: usize,
    n_growth: usize,
    n_hiring: usize,
    show_hw: bool,
) -> String {
    let top_industry = industry_sorted.first();
    let top_share = match top_industry {
        Some((_, c)) if industry_total > 0 => *c as f64 / industry_total as f64 * 100.0,
        _ => 0.0,
    };
    let top_name = top_industry.map(|(n, _)| n.as_str()).unwrap_or("—");

    let concentration = if top_share >= 25.0 {
        format!(
            "<strong>{}</strong> が <strong>{:.0}%</strong> を占める <strong>主産業依存型</strong> です。",
            top_name, top_share
        )
    } else if top_share >= 15.0 {
        format!(
            "<strong>{}</strong> 中心 (<strong>{:.0}%</strong>) ながら複数産業が並走する <strong>複合型</strong> 構造です。",
            top_name, top_share
        )
    } else if top_share > 0.0 {
        "産業が <strong>分散型</strong> に広がり、特定業界依存が低い構造です。".to_string()
    } else {
        "産業構成データが取得できなかったため、業種傾向は判定困難です。".to_string()
    };

    let growth_note = if n_growth >= 10 {
        format!(
            "急成長企業 <strong>{}</strong> 社が地域に存在し、人材移動が活発な可能性があります。",
            n_growth
        )
    } else if n_growth >= 3 {
        format!(
            "急成長企業 <strong>{}</strong> 社が確認でき、新規参入 / 採用強化中の競合として注視が必要です。",
            n_growth
        )
    } else {
        format!("急成長セグメントは <strong>{}</strong> 社で、競合の人員拡大局面は限定的です。", n_growth)
    };

    let hw_note = if show_hw && n_hiring >= 5 {
        format!(
            " 媒体上で <strong>採用活発企業 {}</strong> 社が確認でき、競合との掲載重複度は高めです。応募導線・募集要項の差別化が必要です。",
            n_hiring
        )
    } else {
        String::new()
    };

    let pool_note = if pool_size == 0 {
        " (地域企業データが取得できなかったため、競合分析は限定的です)"
    } else {
        ""
    };

    format!("{} {}{}{}", concentration, growth_note, hw_note, pool_note)
}

// ============================================================
// Section 06: 人材デモグラフィック (Phase 3 navy 本実装)
// ============================================================

pub(super) fn render_navy_section_06_demographics(
    html: &mut String,
    hw_context: Option<&InsightContext>,
) {
    html.push_str("<section class=\"page-navy navy-demographics\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 06",
        "人材デモグラフィック",
        "人口ピラミッド / 労働力 / 教育施設密度",
    );

    let ctx = match hw_context {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption\">外部統計データが取得できなかったため、本セクションは省略表示となります。</p>\n");
            html.push_str("</section>\n");
            return;
        }
    };

    // -- ピラミッドデータ抽出
    use super::super::super::helpers::{get_f64, get_i64, get_str_ref};
    let mut bands: Vec<(String, i64, i64)> = ctx
        .ext_pyramid
        .iter()
        .map(|r| {
            (
                get_str_ref(r, "age_group").to_string(),
                get_i64(r, "male_count"),
                get_i64(r, "female_count"),
            )
        })
        .filter(|(l, _, _)| !l.is_empty())
        .collect();
    bands.sort_by_key(|(l, _, _)| age_sort_key(l));

    // -- 集計
    let total_pop: i64 = bands.iter().map(|(_, m, f)| m + f).sum();
    let working_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 15 && age_lo(l) < 65)
        .map(|(_, m, f)| m + f)
        .sum();
    let target_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 25 && age_lo(l) < 45)
        .map(|(_, m, f)| m + f)
        .sum();
    let senior: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 65)
        .map(|(_, m, f)| m + f)
        .sum();

    let working_pct = if total_pop > 0 {
        working_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let target_pct = if total_pop > 0 {
        target_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let senior_pct = if total_pop > 0 {
        senior as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };

    // -- 労働力率 / 失業率
    let labor_force_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "labor_force_ratio"))
        .filter(|v| *v > 0.0);
    let unemployment_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "unemployment_rate"))
        .filter(|v| *v > 0.0);

    // -- 教育施設密度
    let school_count: i64 = ctx
        .ext_education_facilities
        .iter()
        .map(|r| {
            get_i64(r, "elementary_schools")
                + get_i64(r, "junior_high_schools")
                + get_i64(r, "high_schools")
        })
        .sum();

    // -- exec-headline
    let lede = format!(
        "対象地域の生産年齢層厚みを把握します。総人口 <strong>{}</strong> 名 / \
         生産年齢 (15-64) <strong>{:.1}%</strong> / 採用ターゲット (25-44) <strong>{:.1}%</strong> / \
         高齢 (65+) <strong>{:.1}%</strong>。",
        format_number(total_pop),
        working_pct,
        target_pct,
        senior_pct,
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI 5 cell
    let working_dot = if working_pct >= 60.0 { "pos" } else if working_pct >= 50.0 { "neu" } else { "warn" };
    let target_dot = if target_pct >= 22.0 { "pos" } else if target_pct >= 17.0 { "neu" } else { "warn" };
    let senior_dot = if senior_pct >= 35.0 { "warn" } else if senior_pct >= 25.0 { "neu" } else { "pos" };

    html.push_str("<div class=\"block-title\">図 6-1 &nbsp;人口構造 主要 KPI</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(
        html,
        "総人口",
        &format_number(total_pop),
        "名",
        "neu",
        "国勢調査 5 歳階級集計",
        false,
    );
    push_kpi(
        html,
        "生産年齢 (15-64)",
        &format!("{:.1}", working_pct),
        "%",
        working_dot,
        &format!("実数 {} 名", format_number(working_age)),
        true,
    );
    push_kpi(
        html,
        "ターゲット (25-44)",
        &format!("{:.1}", target_pct),
        "%",
        target_dot,
        &format!("実数 {} 名", format_number(target_age)),
        false,
    );
    push_kpi(
        html,
        "高齢 (65+)",
        &format!("{:.1}", senior_pct),
        "%",
        senior_dot,
        &format!("実数 {} 名", format_number(senior)),
        false,
    );
    let lfr_val = labor_force_rate.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "—".into());
    let lfr_dot = match labor_force_rate {
        Some(v) if v >= 62.0 => "pos",
        Some(v) if v >= 55.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    let lfr_foot = match unemployment_rate {
        Some(u) => format!("失業率 {:.1}%", u),
        None => "失業率データなし".to_string(),
    };
    push_kpi(html, "労働力率", &lfr_val, "%", lfr_dot, &lfr_foot, false);
    html.push_str("</div>\n");

    // -- 人口ピラミッド SVG
    if !bands.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 6-2 &nbsp;年齢階級別 人口ピラミッド</div>\n");
        html.push_str(&build_navy_pyramid_svg(&bands));
        html.push_str("<p class=\"caption\">左 (紺) = 男性 / 右 (金) = 女性。各バーは 5 歳階級別の人口を表示。出典: 国勢調査 v2_external_population_pyramid。</p>\n");
    }

    // -- 教育施設密度 (block-title + 1 段落)
    if school_count > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-A &nbsp;教育施設 (小・中・高 合計)</div>\n");
        html.push_str(&format!(
            "<table class=\"table-navy\">\n<thead><tr>\
             <th>区分</th><th class=\"num\">学校数</th><th>備考</th>\
             </tr></thead>\n<tbody>\n"
        ));
        let mut sum_elem = 0i64;
        let mut sum_jh = 0i64;
        let mut sum_high = 0i64;
        for r in &ctx.ext_education_facilities {
            sum_elem += get_i64(r, "elementary_schools");
            sum_jh += get_i64(r, "junior_high_schools");
            sum_high += get_i64(r, "high_schools");
        }
        html.push_str(&format!(
            "<tr><td><strong>小学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 1-3 km 想定</span></td></tr>\n",
            format_number(sum_elem)
        ));
        html.push_str(&format!(
            "<tr><td><strong>中学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 3-5 km 想定</span></td></tr>\n",
            format_number(sum_jh)
        ));
        html.push_str(&format!(
            "<tr class=\"hl\"><td><strong>高等学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 10 km 級。新卒採用接点として活用可</span></td></tr>\n",
            format_number(sum_high)
        ));
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: 文部科学省 学校基本調査 v2_external_education_facilities。家族層 (子育て世帯) 採用時の生活インフラ指標として併記。</p>\n");
    }

    // -- so-what
    let so_what = build_demographics_so_what(working_pct, target_pct, senior_pct, labor_force_rate);
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

// 「20-24」「25-29」「85+」等のラベルから下端年齢を取得
fn age_lo(label: &str) -> i32 {
    let mut s = String::new();
    for c in label.chars() {
        if c.is_ascii_digit() {
            s.push(c);
        } else {
            break;
        }
    }
    s.parse::<i32>().unwrap_or(-1)
}

fn age_sort_key(label: &str) -> i32 {
    let v = age_lo(label);
    if v >= 0 {
        v
    } else {
        i32::MAX
    }
}

/// navy 人口ピラミッド SVG (左=男性 ink-soft / 右=女性 accent)
fn build_navy_pyramid_svg(bands: &[(String, i64, i64)]) -> String {
    if bands.is_empty() {
        return String::new();
    }
    let n = bands.len();
    let row_h: f64 = 18.0;
    let h: f64 = 40.0 + n as f64 * row_h + 24.0;
    let w: f64 = 720.0;
    let center: f64 = w / 2.0;
    let label_w: f64 = 60.0;
    let bar_max_w: f64 = (w - label_w) / 2.0 - 10.0;

    let max_count: f64 = bands
        .iter()
        .flat_map(|(_, m, f)| [*m as f64, *f as f64])
        .fold(0.0, f64::max)
        .max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"人口ピラミッド\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );
    // タイトルラベル
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\" text-anchor=\"end\">男性</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\">女性</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">年齢</text>\n",
        center - 12.0, center + 12.0, center
    ));
    // 中央軸
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"30\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        center, center, h - 24.0
    ));

    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let cy = 36.0 + i as f64 * row_h;
        let mw = (*male as f64 / max_count) * bar_max_w;
        let fw = (*female as f64 / max_count) * bar_max_w;
        // 男性 (左)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#1F2D4D\"/>\n",
            center - mw,
            cy,
            mw.max(0.5)
        ));
        // 女性 (右)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#C9A24B\"/>\n",
            center,
            cy,
            fw.max(0.5)
        ));
        // 年齢ラベル (中央)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
            center,
            cy + 10.0,
            escape_html(label)
        ));
    }

    // 軸スケール
    svg.push_str(&format!(
        "<text x=\"4\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\">{} 名</text>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"end\">{} 名</text>\n",
        h - 8.0,
        format_number(max_count as i64),
        w - 4.0,
        h - 8.0,
        format_number(max_count as i64)
    ));
    svg.push_str("</svg>\n");
    svg
}

fn build_demographics_so_what(
    working_pct: f64,
    target_pct: f64,
    senior_pct: f64,
    labor_force_rate: Option<f64>,
) -> String {
    let pool_judge = if target_pct >= 22.0 {
        format!(
            "採用ターゲット層 (25-44) が <strong>{:.0}%</strong> を占め、<strong>採用候補プール 厚</strong>。給与訴求 + 福利厚生の充実度で勝負できる地域です。",
            target_pct
        )
    } else if target_pct >= 17.0 {
        format!(
            "採用ターゲット層 (25-44) は <strong>{:.0}%</strong>。<strong>採用候補プール 中</strong>。エントリー要件の柔軟化 (経験不問 / 異業種歓迎) で母集団拡大を検討してください。",
            target_pct
        )
    } else {
        format!(
            "採用ターゲット層 (25-44) が <strong>{:.0}%</strong> と薄く、<strong>採用候補プール 細</strong>。\
             年齢帯拡張 (45-54 層への展開) や近隣広域への採用範囲拡大が必要です。",
            target_pct
        )
    };

    let age_balance = if senior_pct >= 35.0 {
        " 高齢層 35%+ で <strong>人口構造は超高齢化</strong>。退職タイミングを見据えた中期的な人員計画 (3-5 年) が必要です。"
    } else if senior_pct >= 25.0 {
        " 高齢層 25%+ で全国平均並み。生産年齢層の絶対数を維持する施策 (定着 / 中途採用) を継続的に。"
    } else {
        " 高齢層比率が低く、生産年齢層が厚い <strong>採用に有利な構造</strong> です。"
    };

    let labor_note = match labor_force_rate {
        Some(v) if v >= 62.0 => format!(" 労働力率 {:.1}% は高水準で、既就業者の引き抜き競争が激しい可能性があります。", v),
        Some(v) if v >= 55.0 => format!(" 労働力率 {:.1}% は標準的水準です。", v),
        Some(v) => format!(" 労働力率 {:.1}% は低めで、潜在労働力 (非労働力人口) のリーチ施策に余地があります。", v),
        None => String::new(),
    };

    let _ = working_pct;
    format!("{}{}{}", pool_judge, age_balance, labor_note)
}
