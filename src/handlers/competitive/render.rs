use axum::response::Html;

use crate::handlers::overview::format_number;
use super::analysis::AnalysisData;
use super::fetch::{CompStats, PostingRow, SalaryStats};
use super::utils::{escape_html, truncate_str};

/// 競合調査タブの初期HTML
pub(crate) fn render_competitive(
    job_type: &str,
    stats: &CompStats,
    pref_options: &[String],
    ftype_options: &[(String, i64)],
    stype_options: &[(String, i64)],
) -> String {
    let pref_labels: Vec<String> = stats.pref_ranking.iter().map(|(p, _)| format!("\"{}\"", p)).collect();
    let pref_values: Vec<String> = stats.pref_ranking.iter().map(|(_, v)| v.to_string()).collect();

    let pref_rows: String = stats
        .pref_ranking
        .iter()
        .enumerate()
        .map(|(i, (name, cnt))| {
            format!(
                r#"<tr><td class="text-center">{}</td><td>{}</td><td class="text-right">{}</td></tr>"#,
                i + 1, name, format_number(*cnt)
            )
        })
        .collect();

    let pref_option_html: String = pref_options
        .iter()
        .map(|p| format!(r#"<option value="{p}">{p}</option>"#))
        .collect::<Vec<_>>()
        .join("\n");

    include_str!("../../../templates/tabs/competitive.html")
        .replace("{{JOB_TYPE}}", job_type)
        .replace("{{TOTAL_POSTINGS}}", &format_number(stats.total_postings))
        .replace("{{TOTAL_FACILITIES}}", &format_number(stats.total_facilities))
        .replace("{{PREF_LABELS}}", &format!("[{}]", pref_labels.join(",")))
        .replace("{{PREF_VALUES}}", &format!("[{}]", pref_values.join(",")))
        .replace("{{PREF_ROWS}}", &pref_rows)
        .replace("{{PREF_OPTIONS}}", &pref_option_html)
        .replace("{{FTYPE_CHECKBOXES}}", &{
            let mut html = String::new();
            for (i, (jt, cnt)) in ftype_options.iter().enumerate() {
                let raw = jt.replace('"', "&quot;");
                let disp = escape_html(jt);
                let cnt_s = format_number(*cnt);
                html.push_str(&format!(
                    r#"<label class="flex items-center gap-2 py-1 px-2 hover:bg-slate-700 rounded cursor-pointer">
                        <input type="checkbox" class="ftype-major-cb rounded" value="{raw}" data-group="g{i}"
                            onchange="onMajorToggle(this)">
                        <span class="text-sm text-white flex-1">{disp}</span>
                        <span class="text-xs text-slate-400">{cnt_s}</span>
                    </label>"#,
                ));
            }
            html
        })
        .replace("{{STYPE_OPTIONS}}", &{
            let mut html = String::new();
            for (name, cnt) in stype_options {
                let raw = name.replace('"', "&quot;");
                let disp = escape_html(name);
                let cnt_s = format_number(*cnt);
                html.push_str(&format!(
                    r#"<option value="{raw}">{disp} ({cnt_s})</option>"#,
                ));
            }
            html
        })
}

/// 求人一覧テーブル（HTMXパーシャル）
pub(crate) fn render_posting_table(
    _job_type: &str,
    pref: &str,
    muni: &str,
    postings: &[PostingRow],
    stats: &SalaryStats,
    page: i64,
    total_pages: i64,
    total: i64,
    nearby: bool,
    radius_km: f64,
    emp: &str,
    stype: &str,
    ftype: &str,
) -> Html<String> {
    let show_distance = nearby && postings.iter().any(|p| p.distance_km.is_some());

    let mut html = String::new();

    // 統計サマリー
    let nearby_label = if nearby { format!("（半径{}km）", radius_km) } else { String::new() };
    if stats.has_data {
        html.push_str(&format!(
            r#"<div class="stat-card mb-4">
                <h3 class="text-sm text-slate-400 mb-2">給与統計（{} {}{} / {}件）</h3>
                <div class="overflow-x-auto">
                <table class="data-table text-xs">
                    <thead><tr><th></th><th class="text-right">月給下限</th><th class="text-right">月給上限</th></tr></thead>
                    <tbody>
                        <tr><td class="text-slate-300">最頻値（1万円単位）</td><td class="text-right">{}</td><td class="text-right">{}</td></tr>
                        <tr><td class="text-slate-300">中央値</td><td class="text-right">{}</td><td class="text-right">{}</td></tr>
                        <tr><td class="text-slate-300">平均値</td><td class="text-right">{}</td><td class="text-right">{}</td></tr>
                    </tbody>
                </table>
                </div>
                <div class="mt-2 text-xs text-slate-400">
                    平均年間休日: {}
                </div>
            </div>"#,
            pref, muni, &nearby_label,
            total,
            stats.salary_min_mode, stats.salary_max_mode,
            stats.salary_min_median, stats.salary_max_median,
            stats.salary_min_avg, stats.salary_max_avg,
            stats.avg_holidays,
        ));
    }

    // ページ情報
    html.push_str(&format!(
        r#"<div class="flex justify-between items-center mb-2">
            <span class="text-sm text-slate-400">全{}件中 {}〜{}件</span>
            <a href="/api/report?prefecture={}&municipality={}&employment_type={}&nearby={}&radius_km={}&service_type={}&facility_type={}"
               target="_blank"
               class="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white text-sm rounded-lg transition">
               HTMLレポート出力
            </a>
        </div>"#,
        total,
        (page - 1) * 50 + 1,
        ((page - 1) * 50 + postings.len() as i64).min(total),
        urlencoding::encode(pref),
        urlencoding::encode(muni),
        urlencoding::encode(emp),
        nearby,
        radius_km,
        urlencoding::encode(stype),
        urlencoding::encode(ftype),
    ));

    // カラム表示トグルボタン
    html.push_str(r#"<div class="mb-2"><button id="comp-col-toggle" onclick="toggleCompColumns()" class="px-3 py-1 bg-slate-700 hover:bg-slate-600 text-slate-300 text-xs rounded transition">全カラム表示</button></div>"#);

    // テーブル
    html.push_str(r#"<div class="overflow-x-auto"><table class="data-table text-xs">"#);
    html.push_str("<thead><tr>");
    html.push_str(r#"<th class="text-center" style="width:30px">#</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">求人番号</th>"#);
    html.push_str("<th>事業所名</th>");
    html.push_str("<th>産業</th>");
    html.push_str("<th>エリア</th>");
    html.push_str("<th>雇用形態</th>");
    html.push_str(r#"<th class="comp-col-extra" style="display:none">給与区分</th>"#);
    html.push_str(r#"<th class="text-right">月給下限</th>"#);
    html.push_str(r#"<th class="text-right">月給上限</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">職種詳細</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">学歴</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none;min-width:180px">応募要件</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none;min-width:120px">必要経験</th>"#);
    html.push_str("<th>昇給・賞与</th>");
    html.push_str(r#"<th class="comp-col-extra" style="display:none">勤務時間</th>"#);
    html.push_str("<th>従業員数</th>");
    html.push_str(r#"<th class="text-right">年間休日</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">管轄HW</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">募集理由</th>"#);
    html.push_str(r#"<th class="comp-col-extra" style="display:none">セグメント</th>"#);
    if show_distance {
        html.push_str(r#"<th class="text-right">距離</th>"#);
    }
    html.push_str("</tr></thead><tbody>");

    let start_num = (page - 1) * 50;
    for (i, p) in postings.iter().enumerate() {
        let fname = truncate_str(&escape_html(&p.facility_name), 40);
        let area = format!("{} {}", p.prefecture, p.municipality);
        let sal_type = escape_html(&p.salary_type);
        let sal_min = if p.salary_min > 0 { format_number(p.salary_min) } else { "-".to_string() };
        let sal_max = if p.salary_max > 0 { format_number(p.salary_max) } else { "-".to_string() };
        let reqs = escape_html(&p.requirements);
        let holidays = if p.annual_holidays > 0 { p.annual_holidays.to_string() } else { "-".to_string() };
        let job_num = escape_html(&p.job_number);
        let hw_office = truncate_str(&escape_html(&p.hello_work_office), 15);
        let recruit_reason = truncate_str(&escape_html(&p.recruitment_reason), 20);
        let jt = truncate_str(&escape_html(&p.job_type), 20);
        let working_hrs = truncate_str(&escape_html(&p.working_hours), 30);
        let occ_detail = truncate_str(&escape_html(&p.occupation_detail), 30);
        let education = if p.education_required.is_empty() { "-".to_string() } else { escape_html(&p.education_required) };
        let exp_req = if p.experience_required.is_empty() { "-".to_string() } else { truncate_str(&escape_html(&p.experience_required), 40) };

        // 昇給・賞与を結合表示
        let raise_bonus = {
            let mut parts = Vec::new();
            if !p.raise_amount.is_empty() {
                parts.push(format!("昇給:{}", p.raise_amount));
            }
            if !p.bonus_amount.is_empty() {
                if p.bonus_months > 0.0 {
                    parts.push(format!("賞与:{}({}ヶ月)", p.bonus_amount, p.bonus_months));
                } else {
                    parts.push(format!("賞与:{}", p.bonus_amount));
                }
            }
            if parts.is_empty() { "-".to_string() } else { truncate_str(&escape_html(&parts.join(" ")), 35) }
        };
        let emp_count = if p.employee_count > 0 { format!("{}人", p.employee_count) } else { "-".to_string() };

        let seg_label = if p.tier3_label_short.is_empty() {
            "-".to_string()
        } else {
            truncate_str(&escape_html(&p.tier3_label_short), 25)
        };
        html.push_str(&format!(
            r#"<tr><td class="text-center">{}</td><td class="comp-col-extra font-mono text-xs" style="display:none">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class="comp-col-extra" style="display:none">{}</td><td class="text-right">{}</td><td class="text-right">{}</td><td class="comp-col-extra text-xs" style="display:none">{}</td><td class="comp-col-extra text-xs" style="display:none">{}</td><td class="comp-col-extra" style="display:none"><div class="cell-wrap">{}</div></td><td class="comp-col-extra text-xs" style="display:none">{}</td><td class="text-xs">{}</td><td class="comp-col-extra text-xs" style="display:none">{}</td><td class="text-right">{}</td><td class="text-right">{}</td>"#,
            start_num + i as i64 + 1, job_num, fname, jt, area, escape_html(&p.employment_type),
            sal_type, sal_min, sal_max,
            occ_detail, education,
            reqs, exp_req, raise_bonus, working_hrs,
            emp_count, holidays,
        ));
        html.push_str(&format!(
            r#"<td class="comp-col-extra text-xs" style="display:none">{}</td><td class="comp-col-extra text-xs" style="display:none">{}</td><td class="comp-col-extra text-xs" style="display:none">{}</td>"#,
            hw_office, recruit_reason, seg_label,
        ));
        if show_distance {
            let dist = p.distance_km.map(|d| format!("{:.1}km", d)).unwrap_or("-".to_string());
            html.push_str(&format!(r#"<td class="text-right">{}</td>"#, dist));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table></div>");

    // ページネーション
    if total_pages > 1 {
        html.push_str(r#"<div class="flex justify-center gap-2 mt-4">"#);
        let base_url = format!(
            "/api/competitive/filter?prefecture={}&municipality={}&employment_type={}&nearby={}&radius_km={}&service_type={}&facility_type={}",
            urlencoding::encode(pref),
            urlencoding::encode(muni),
            urlencoding::encode(emp),
            nearby,
            radius_km,
            urlencoding::encode(stype),
            urlencoding::encode(ftype),
        );
        if page > 1 {
            html.push_str(&format!(
                r##"<button class="px-3 py-1 bg-slate-700 hover:bg-slate-600 rounded text-sm" hx-get="{}&page={}" hx-target="#comp-results" hx-swap="innerHTML">前へ</button>"##,
                base_url, page - 1
            ));
        }
        html.push_str(&format!(
            r#"<span class="px-3 py-1 text-sm text-slate-400">{} / {} ページ</span>"#,
            page, total_pages
        ));
        if page < total_pages {
            html.push_str(&format!(
                r##"<button class="px-3 py-1 bg-slate-700 hover:bg-slate-600 rounded text-sm" hx-get="{}&page={}" hx-target="#comp-results" hx-swap="innerHTML">次へ</button>"##,
                base_url, page + 1
            ));
        }
        html.push_str("</div>");
    }

    Html(html)
}

/// HTMLレポート生成（A4横向き印刷対応）
pub(crate) fn render_report_html(
    job_type: &str,
    pref: &str,
    muni: &str,
    emp: &str,
    postings: &[PostingRow],
    stats: &SalaryStats,
    today: &str,
    nearby: bool,
    radius_km: f64,
) -> Html<String> {
    let region = if muni.is_empty() {
        pref.to_string()
    } else if nearby {
        format!("{} {}（半径{}km）", pref, muni, radius_km)
    } else {
        format!("{} {}", pref, muni)
    };
    let emp_label = if emp.is_empty() || emp == "全て" { String::new() } else { format!(" x {}", emp) };

    let show_distance = nearby && postings.iter().any(|p| p.distance_km.is_some());

    let mut table_rows = String::new();
    for (i, p) in postings.iter().enumerate() {
        let fname = truncate_str(&escape_html(&p.facility_name), 40);
        let area = format!("{} {}", escape_html(&p.prefecture), escape_html(&p.municipality));
        let sal_type = escape_html(&p.salary_type);
        let sal_min = if p.salary_min > 0 { format_number(p.salary_min) } else { "-".to_string() };
        let sal_max = if p.salary_max > 0 { format_number(p.salary_max) } else { "-".to_string() };
        let reqs = escape_html(&p.requirements);
        let holidays = if p.annual_holidays > 0 { p.annual_holidays.to_string() } else { "-".to_string() };
        let dist_cell = if show_distance {
            let d = p.distance_km.map(|d| format!("{:.1}km", d)).unwrap_or("-".to_string());
            format!(r#"<td class="num">{}</td>"#, d)
        } else {
            String::new()
        };

        let occ_detail = if p.occupation_detail.is_empty() { "-".to_string() } else { truncate_str(&escape_html(&p.occupation_detail), 30) };
        let education = if p.education_required.is_empty() { "-".to_string() } else { escape_html(&p.education_required) };
        let exp_req = if p.experience_required.is_empty() { "-".to_string() } else { truncate_str(&escape_html(&p.experience_required), 40) };
        let raise_bonus = {
            let mut parts = Vec::new();
            if !p.raise_amount.is_empty() { parts.push(format!("昇給:{}", p.raise_amount)); }
            if !p.bonus_amount.is_empty() {
                if p.bonus_months > 0.0 { parts.push(format!("賞与:{}({}ヶ月)", p.bonus_amount, p.bonus_months)); }
                else { parts.push(format!("賞与:{}", p.bonus_amount)); }
            }
            if parts.is_empty() { "-".to_string() } else { escape_html(&parts.join(" ")) }
        };
        let emp_count = if p.employee_count > 0 { format!("{}人", p.employee_count) } else { "-".to_string() };

        let seg = if p.tier3_label_short.is_empty() {
            "-".to_string()
        } else {
            escape_html(&p.tier3_label_short)
        };
        table_rows.push_str(&format!(
            r#"<tr><td style="text-align:center">{}</td><td class="font-mono">{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class="num">{}</td><td class="num">{}</td><td>{}</td><td>{}</td><td style="max-width:250px;word-break:break-all">{}</td><td>{}</td><td>{}</td><td>{}</td><td class="num">{}</td><td class="num">{}</td>"#,
            i + 1,
            escape_html(&p.job_number),
            fname,
            truncate_str(&escape_html(&p.job_type), 20),
            area,
            escape_html(&p.employment_type),
            sal_type, sal_min, sal_max,
            occ_detail, education,
            reqs, exp_req, raise_bonus,
            truncate_str(&escape_html(&p.working_hours), 30),
            emp_count, holidays,
        ));
        table_rows.push_str(&format!(
            r#"<td>{}</td><td>{}</td><td>{}</td>{}</tr>"#,
            truncate_str(&escape_html(&p.hello_work_office), 15),
            truncate_str(&escape_html(&p.recruitment_reason), 20),
            seg, dist_cell,
        ));
    }

    let distance_th = if show_distance { r#"<th>距離</th>"# } else { "" };

    let stats_html = if stats.has_data {
        format!(
            r#"<h2>給与統計サマリー</h2>
            <table>
                <thead><tr><th></th><th>月給下限</th><th>月給上限</th></tr></thead>
                <tbody>
                    <tr><td>最頻値（1万円単位）</td><td class="num">{}</td><td class="num">{}</td></tr>
                    <tr><td>中央値</td><td class="num">{}</td><td class="num">{}</td></tr>
                    <tr><td>平均値</td><td class="num">{}</td><td class="num">{}</td></tr>
                </tbody>
            </table>
            <p>件数: {} | 平均年間休日: {}</p>"#,
            stats.salary_min_mode, stats.salary_max_mode,
            stats.salary_min_median, stats.salary_max_median,
            stats.salary_min_avg, stats.salary_max_avg,
            stats.count, stats.avg_holidays,
        )
    } else {
        String::new()
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>競合調査レポート - {job_type} x {region}{emp_label}</title>
<style>
@page {{ size: A4 landscape; margin: 10mm; }}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
body {{ font-family: "Yu Gothic", "Meiryo", sans-serif; font-size: 11px; color: #333; background: #fff; padding: 15px; }}
h1 {{ font-size: 18px; color: #1a5276; margin-bottom: 8px; border-bottom: 2px solid #1a5276; padding-bottom: 4px; }}
h2 {{ font-size: 14px; color: #2c3e50; margin: 16px 0 8px 0; }}
.meta {{ font-size: 11px; color: #666; margin-bottom: 12px; }}
.meta span {{ margin-right: 16px; }}
table {{ width: 100%; border-collapse: collapse; margin-bottom: 20px; }}
th {{ background-color: #2c3e50; color: #fff; font-weight: bold; text-align: center; padding: 6px 4px; font-size: 10px; white-space: nowrap; border: 1px solid #1a252f; }}
td {{ padding: 5px 4px; border: 1px solid #ddd; font-size: 10px; vertical-align: top; }}
tr:nth-child(even) {{ background-color: #f8f9fa; }}
.num {{ text-align: right; white-space: nowrap; }}
@media print {{
    body {{ padding: 0; font-size: 9px; }}
    th, td {{ font-size: 8px; padding: 3px 2px; }}
}}
</style>
</head>
<body>
<h1>競合調査レポート</h1>
<div class="meta">
    <span>産業: {job_type}</span>
    <span>地域: {region}</span>
    <span>生成日: {today}</span>
    <span>{count}件</span>
</div>

{stats_html}

<h2>求人一覧</h2>
<table>
<thead>
<tr>
    <th>#</th><th>求人番号</th><th>事業所名</th><th>産業</th><th>エリア</th>
    <th>雇用形態</th><th>給与区分</th><th>月給下限</th><th>月給上限</th>
    <th>職種詳細</th><th>学歴</th>
    <th>応募要件</th><th>必要経験</th><th>昇給・賞与</th><th>勤務時間</th>
    <th>年間休日</th><th>従業員数</th><th>管轄HW</th><th>募集理由</th>
    <th>セグメント</th>{distance_th}
</tr>
</thead>
<tbody>
{table_rows}
</tbody>
</table>
</body>
</html>"#,
        job_type = escape_html(job_type),
        region = escape_html(&region),
        emp_label = escape_html(&emp_label),
        today = today,
        count = postings.len(),
        stats_html = stats_html,
        distance_th = distance_th,
        table_rows = table_rows,
    );

    Html(html)
}

pub(crate) fn render_analysis_html(job_type: &str, data: &AnalysisData) -> String {
    render_analysis_html_with_scope(job_type, "全国", data)
}

pub(crate) fn render_analysis_html_with_scope(job_type: &str, scope: &str, data: &AnalysisData) -> String {
    if data.total == 0 {
        return format!(
            r#"<p class="text-slate-400 text-sm">「{}」の求人データがありません</p>"#,
            escape_html(job_type)
        );
    }

    let emp_chart_data: String = data.employment_dist.iter()
        .map(|(name, cnt)| format!(r#"{{"value":{},"name":"{}"}}"#, cnt, escape_html(name)))
        .collect::<Vec<_>>()
        .join(",");

    let sal_type_data: String = data.salary_type_dist.iter()
        .map(|(name, cnt)| format!(r#"{{"value":{},"name":"{}"}}"#, cnt, escape_html(name)))
        .collect::<Vec<_>>()
        .join(",");

    let range_labels: String = data.salary_range_dist.iter()
        .map(|(label, _)| format!(r#""{}""#, label))
        .collect::<Vec<_>>()
        .join(",");
    let range_values: String = data.salary_range_dist.iter()
        .map(|(_, cnt)| cnt.to_string())
        .collect::<Vec<_>>()
        .join(",");

    format!(
        r##"<div class="space-y-4">
    <h3 class="text-lg font-bold text-white">求人データ分析 <span class="text-sm font-normal text-slate-400">（{scope} / {total}件）</span></h3>

    <!-- KPIサマリー -->
    <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
        <div class="stat-card text-center">
            <div class="text-xs text-slate-400">月給平均（中央値）</div>
            <div class="text-2xl font-bold text-cyan-400">{salary_avg_fmt}</div>
            <div class="text-xs text-slate-500">中央値: {salary_median_fmt}</div>
        </div>
        <div class="stat-card text-center">
            <div class="text-xs text-slate-400">平均年間休日</div>
            <div class="text-2xl font-bold text-purple-400">{holidays_avg}日</div>
            <div class="text-xs text-slate-500">({holidays_with_data}件のデータ)</div>
        </div>
        <div class="stat-card text-center">
            <div class="text-xs text-slate-400">総求人件数</div>
            <div class="text-2xl font-bold text-emerald-400">{total}</div>
        </div>
    </div>

    <!-- チャート2列 -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <!-- 雇用形態分布 -->
        <div class="stat-card">
            <h4 class="text-sm text-slate-400 mb-2">雇用形態別分布</h4>
            <div class="echart" style="height:280px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "legend": {{"bottom": 0, "textStyle": {{"color": "#94a3b8", "fontSize": 11}}}},
                "color": ["#3b82f6", "#22c55e", "#eab308", "#ef4444", "#8b5cf6"],
                "series": [{{"type": "pie", "radius": ["40%","65%"], "center": ["50%","45%"],
                    "label": {{"formatter": "{{b}}\n{{d}}%", "color": "#e2e8f0", "fontSize": 11}},
                    "data": [{emp_chart_data}]
                }}]
            }}'></div>
        </div>

        <!-- 給与区分分布 -->
        <div class="stat-card">
            <h4 class="text-sm text-slate-400 mb-2">給与区分別分布</h4>
            <div class="echart" style="height:280px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "legend": {{"bottom": 0, "textStyle": {{"color": "#94a3b8", "fontSize": 11}}}},
                "color": ["#06b6d4", "#8b5cf6", "#f97316", "#64748b"],
                "series": [{{"type": "pie", "radius": ["40%","65%"], "center": ["50%","45%"],
                    "label": {{"formatter": "{{b}}\n{{d}}%", "color": "#e2e8f0", "fontSize": 11}},
                    "data": [{sal_type_data}]
                }}]
            }}'></div>
        </div>

        <!-- 月給レンジ分布 -->
        <div class="stat-card col-span-1 md:col-span-2">
            <h4 class="text-sm text-slate-400 mb-2">月給レンジ分布（下限額）</h4>
            <div class="echart" style="height:280px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis"}},
                "xAxis": {{"type": "category", "data": [{range_labels}], "axisLabel": {{"color": "#94a3b8", "fontSize": 10}}}},
                "yAxis": {{"type": "value", "axisLabel": {{"color": "#94a3b8"}}}},
                "series": [{{"type": "bar", "data": [{range_values}],
                    "itemStyle": {{"color": {{"type": "linear", "x": 0, "y": 0, "x2": 0, "y2": 1,
                        "colorStops": [{{"offset": 0, "color": "#06b6d4"}}, {{"offset": 1, "color": "#3b82f6"}}]
                    }}, "borderRadius": [4,4,0,0]}},
                    "barWidth": "60%"
                }}],
                "grid": {{"left": "12%", "right": "5%", "bottom": "12%"}}
            }}'></div>
        </div>
    </div>
</div>"##,
        scope = escape_html(scope),
        total = format_number(data.total),
        salary_avg_fmt = if data.salary_avg > 0 { format!("{}円", format_number(data.salary_avg)) } else { "-".to_string() },
        salary_median_fmt = if data.salary_median > 0 { format!("{}円", format_number(data.salary_median)) } else { "-".to_string() },
        holidays_avg = data.holidays_avg,
        holidays_with_data = format_number(data.holidays_with_data),
        emp_chart_data = emp_chart_data,
        sal_type_data = sal_type_data,
        range_labels = range_labels,
        range_values = range_values,
    )
}
