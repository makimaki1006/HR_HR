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
    v.push((sev, "サンプル件数".into(), body, "§2 統計信頼性".into()));

    // 2) 主要雇用形態の偏り
    let (sev, body) = if dom_emp_pct >= 85.0 {
        ("warn", format!("主要雇用形態が <strong>{:.0}%</strong> を占め、構成が偏っています。他雇用形態のサンプル不足が示唆されるため、訴求軸の単一化リスクを点検してください。", dom_emp_pct))
    } else if dom_emp_pct >= 70.0 {
        ("neu", format!("主要雇用形態の構成比は <strong>{:.0}%</strong>。やや偏り気味ですが、他雇用形態への展開余地もある水準です。", dom_emp_pct))
    } else {
        ("pos", format!("主要雇用形態の構成比は <strong>{:.0}%</strong> で、バランスの取れた構成です。", dom_emp_pct))
    };
    v.push((sev, "雇用形態構成".into(), body, "§3 雇用形態分析".into()));

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
    v.push((sev, "新着比率".into(), body, "§3 求人動向".into()));

    // 4) 給与解析率
    let (sev, body) = if salary_parse_pct >= 85 {
        ("pos", format!("給与解析率 <strong>{}%</strong> は高水準で、給与統計の信頼性は確保されています。", salary_parse_pct))
    } else if salary_parse_pct >= 60 {
        ("warn", format!("給与解析率 <strong>{}%</strong> は中程度。給与統計値の参照時には未解析分の影響を考慮してください。", salary_parse_pct))
    } else {
        ("neg", format!("給与解析率 <strong>{}%</strong> は低く、給与統計の代表性に注意が必要です。CSV の給与表記揺れを見直してください。", salary_parse_pct))
    };
    v.push((sev, "給与解析率".into(), body, "§4 給与統計".into()));

    // 5) 地域カバレッジ
    let pref_count = agg.by_prefecture.len();
    let (sev, body) = if pref_count == 0 {
        ("neu", "地域情報の抽出ができませんでした。CSV のアクセス列を確認してください。".to_string())
    } else if pref_count == 1 {
        ("neu", format!("カバー都道府県は <strong>1</strong> 都道府県。単一エリアの深掘り分析として参照可能です。"))
    } else {
        ("neu", format!("カバー都道府県は <strong>{}</strong>。複数地域比較は本レポート後半セクションで詳述します。", pref_count))
    };
    v.push((sev, "地域カバレッジ".into(), body, "§5 地域分析".into()));

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
// Section 02-08 placeholder (Phase 2-4 で本実装に差し替え)
// ============================================================

pub(super) fn render_navy_section_placeholders(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    now: &str,
) {
    let _ = (hw_context, now);
    let section_02 = match variant {
        ReportVariant::Full => "地域 × 求人媒体データ連携",
        _ => "地域データ補強",
    };
    let sections = [
        ("SECTION 02", section_02, "地域別の求人補強指標を取り扱う章。Phase 2 で実装予定。"),
        ("SECTION 03", "給与分布 統計", "Jenks 自然分割クラスタ + 給与水準ピラミッドを取り扱う章。Phase 2 で実装予定。"),
        ("SECTION 04", "採用市場 逼迫度", "有効求人倍率 / 欠員補充率 / 失業率 を統合した複合指標を取り扱う章。Phase 3 で実装予定。"),
        ("SECTION 05", "地域企業構造", "産業構成 / 法人セグメント / 規模帯ベンチマーク。Phase 3 で実装予定。"),
        ("SECTION 06", "人材デモグラフィック", "人口ピラミッド / 労働力 / 学校教育施設密度。Phase 3 で実装予定。"),
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
