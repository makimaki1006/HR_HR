//! Excel競合調査ダッシュボードの独立出力。
//!
//! `render_html` は `CompetitorReport` (competitor_model.rs) だけを入力にする。
//! 数値の加工は `build_competitor_report` が済ませているので、ここは整形と HTML 組み立てだけ。
//! 各タブの部品は兄弟モジュール (competitor_keywords / _trends / _population / _consultation) にある。
use super::common::push_page_head;
use super::competitor_consultation;
use super::competitor_keywords as keywords;
use super::competitor_model::{
    build_competitor_report, CompetitorReport, GoogleDemand, GoogleSection, GoogleSuggestions,
    HistogramSeries, IndeedSection, KeywordRow, PopulationBand, PopulationSection, SalaryRow,
};
use super::competitor_population as population;
use super::competitor_trends as trends;
use crate::handlers::helpers::{escape_html, format_number};
use crate::handlers::survey::aggregator::SurveyAggregation;
use serde_json::Value;

/// 旧入口。集計と外部コンテキストからレポートを作って HTML にする。
pub(crate) fn render_competitor_report(
    agg: &SurveyAggregation,
    top_n: usize,
    title: &str,
    indeed: &Value,
    google: &Value,
    population: &Value,
) -> String {
    render_html(&build_competitor_report(
        agg, top_n, title, indeed, google, population,
    ))
}

/// 値を表示用の文字列にする。月給 (decimals=2) は万円で小数 2 桁、時給 (0) は円の整数。未取得は「—」。
fn fmt_salary(v: Option<f64>, decimals: u8) -> String {
    v.map(|x| format!("{:.*}", decimals as usize, x))
        .unwrap_or_else(|| "—".into())
}

pub(crate) fn render_html(report: &CompetitorReport) -> String {
    let meta = &report.meta;
    let excel = &report.excel;
    let decimals = excel.decimals;
    let mut html = format!("<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>競合調査</title><style>{}</style></head><body><nav class=\"toolbar no-print\"><a href=\"/competitor\">調査条件に戻る / PDFダウンロード</a><a href=\"/\">市場分析</a></nav><main>",include_str!("../../../../../static/css/competitor-dashboard.css"));
    html.push_str("<div class=\"report-tabs no-print\" role=\"tablist\" aria-label=\"競合調査の表示切り替え\"><button id=\"tab-excel\" role=\"tab\" aria-selected=\"true\" aria-controls=\"panel-excel\" tabindex=\"0\">給与・待遇</button><button id=\"tab-google\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-google\" tabindex=\"-1\">Google検索需要</button><button id=\"tab-indeed\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-indeed\" tabindex=\"-1\">Indeed採用レポート</button><button id=\"tab-population\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-population\" tabindex=\"-1\">人口・地域データ</button><button id=\"tab-consultation\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-consultation\" tabindex=\"-1\">採用のヒント</button></div><div id=\"panel-excel\" role=\"tabpanel\" aria-labelledby=\"tab-excel\" tabindex=\"0\">");
    let unit = meta.unit.as_str();
    html.push_str("<section class=\"excel-dashboard\" aria-label=\"競合調査ダッシュボード\"><aside class=\"summary\"><h1>競合調査</h1><table class=\"meta\">");
    for (a, b, c, d) in [
        (
            "調査名",
            meta.title.as_str(),
            "雇用形態",
            meta.employment_type.as_deref().unwrap_or("—"),
        ),
        (
            "該当都道府県",
            meta.prefecture.as_deref().unwrap_or("—"),
            "主な市町村",
            meta.municipality.as_deref().unwrap_or("—"),
        ),
    ] {
        html.push_str(&format!(
            "<tr><th>{a}</th><th>{c}</th></tr><tr><td>{}</td><td>{}</td></tr>",
            escape_html(b),
            escape_html(d)
        ));
    }
    html.push_str(&format!("<tr><th>集計対象</th><th>該当件数</th></tr><tr><td>CSV重複排除後</td><td>{}</td></tr></table><h2>給与関係（{unit}）</h2><table><tr><th></th><th colspan=\"2\">総合</th><th colspan=\"2\">人気求人</th></tr><tr><th></th><th>下限</th><th>上限</th><th>下限</th><th>上限</th></tr>",format_number(meta.total_count as i64)));
    for SalaryRow { label, values } in &excel.salary_table {
        html.push_str(&format!("<tr><th>{label}</th>"));
        for v in values {
            html.push_str(&format!("<td>{}</td>", fmt_salary(*v, decimals)));
        }
        html.push_str("</tr>");
    }
    let counts = excel.salary_counts;
    html.push_str(&format!("<tr><th>集計件数</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr></table><h2>差異（総合 − 人気求人）</h2><table><tr><th></th><th>下限</th><th>上限</th></tr>",counts[0],counts[1],counts[2],counts[3]));
    for SalaryRow { label, values } in &excel.salary_diff {
        html.push_str(&format!(
            "<tr><th>{label}</th><td>{}</td><td>{}</td></tr>",
            fmt_salary(values[0], decimals),
            fmt_salary(values[1], decimals)
        ));
    }
    html.push_str("</table>");
    keyword_table(&mut html, "求人票ワード調査（全体）", &excel.keyword_all);
    keywords::table(&mut html, &excel.keyword_comparison);
    html.push_str("<p class=\"note\">人気求人：Indeedの「人気」「超人気」タグ付き。</p></aside><div class=\"charts\">");
    for (label, series) in [
        ("上限ボリュームゾーン", &excel.histograms.upper),
        ("下限ボリュームゾーン", &excel.histograms.lower),
    ] {
        chart(
            &mut html,
            &format!("{label}（{unit}・n={}・{:.0}刻み）", series.n, series.step),
            series,
        );
    }
    keywords::all_chart(&mut html, &excel.keyword_all);
    keywords::chart(&mut html, &excel.keyword_comparison);
    html.push_str(&format!("</div></section><p class=\"caption\">CSV重複排除後 {} 件 / 上位 {} 件は収録順。占有率は語を含む求人数の割合。給与分布は{}。</p></div><div id=\"panel-google\" role=\"tabpanel\" aria-labelledby=\"tab-google\" tabindex=\"0\" hidden>",meta.total_count,excel.keyword_comparison.head_n,if meta.is_hourly{"時給の実額"}else{"月給換算"}));
    render_google(&mut html, &report.google);
    html.push_str("</div><div id=\"panel-indeed\" role=\"tabpanel\" aria-labelledby=\"tab-indeed\" tabindex=\"0\" hidden>");
    render_indeed(&mut html, &report.indeed);
    html.push_str("</div><div id=\"panel-population\" role=\"tabpanel\" aria-labelledby=\"tab-population\" tabindex=\"0\" hidden>");
    render_population(&mut html, &report.population);
    html.push_str("</div><div id=\"panel-consultation\" role=\"tabpanel\" aria-labelledby=\"tab-consultation\" tabindex=\"0\" hidden>");
    competitor_consultation::render(
        &mut html,
        &report.consultation,
        if meta.is_hourly {
            "円/時"
        } else {
            "万円/月"
        },
        decimals,
    );
    html.push_str("</div></main><script>");
    html.push_str(include_str!("../../../../../static/js/competitor-tabs.js"));
    html.push_str(include_str!(
        "../../../../../static/js/competitor-keywords.js"
    ));
    html.push_str("</script></body></html>");
    html
}

fn render_population(html: &mut String, data: &PopulationSection) {
    html.push_str("<section class=\"page-navy\"><h1>人口・地域データ</h1>");
    let PopulationSection::Ok {
        region,
        is_national,
        reference_date,
        bands,
        shares,
        minimum_wage,
        minimum_wage_fiscal_year,
        minimum_wage_effective_date,
        minimum_wage_as_of,
        minimum_wage_source,
        minimum_wage_source_url,
        labor,
    } = data
    else {
        let PopulationSection::Unavailable { region, message } = data else {
            return;
        };
        html.push_str(&format!("<p>集計地域：{}</p>", escape_html(region)));
        html.push_str(&format!(
            "<p class=\"note\">{}</p></section>",
            escape_html(message)
        ));
        return;
    };
    html.push_str(&format!("<p>集計地域：{}</p>", escape_html(region)));
    html.push_str("<p class=\"note\">出典：国勢調査・厚生労働省・e-Stat等の保存統計。</p>");
    html.push_str(&format!(
        "<p class=\"note\">人口の基準日：{}</p>",
        reference_date
            .as_deref()
            .map(escape_html)
            .unwrap_or_else(|| "未取得".into())
    ));
    match shares {
        Some(shares) => population::render(html, shares),
        None => render_population_bands(html, bands),
    }
    if *is_national {
        html.push_str("</section>");
        return;
    }
    html.push_str("<h2>地域の最低賃金・労働統計</h2><table class=\"table-navy\"><tr><th>最低賃金（円/時）</th><td>");
    html.push_str(&number(*minimum_wage));
    let wage_year = minimum_wage_fiscal_year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>最低賃金の改定年度</th><td>{wage_year}</td></tr><tr><th>最低賃金の発効日</th><td>{}</td></tr><tr><th>最低賃金の基準日（日本時間）</th><td>{}</td></tr><tr><th>最低賃金の出典</th><td>{}",escape_html(minimum_wage_effective_date),escape_html(minimum_wage_as_of),if minimum_wage_source=="official_csv" {"厚生労働省の公式改定一覧"} else if minimum_wage_source=="database" {"外部統計データベース"} else {"—"}));
    if let Some(url) = minimum_wage_source_url {
        html.push_str(&format!(
            " / <a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">公式資料を確認</a>",
            escape_html(url)
        ));
    }
    let year = labor
        .as_ref()
        .and_then(|l| l.fiscal_year)
        .map(|year| year.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>労働統計の年度</th><td>{year}</td></tr><tr><th>完全失業率（%）</th><td>{}</td></tr><tr><th>離職率（%）</th><td>{}</td></tr></table><p class=\"note\">取得できない指標は — と表示します。</p></section>",number(labor.as_ref().and_then(|l| l.unemployment_rate)),number(labor.as_ref().and_then(|l| l.separation_rate))));
}

/// 構成比が成立しないとき (総人口が無い・男女合計と合わない等) は割合を出さず、人数だけを表にする。
fn render_population_bands(html: &mut String, bands: &[PopulationBand]) {
    html.push_str("<div class=\"population-grid\"><div><h2>人口ピラミッド</h2>");
    if bands.is_empty() {
        html.push_str("<p class=\"note\">人口データがありません。</p>");
    } else if bands.iter().any(|b| b.male.is_none() || b.female.is_none()) {
        html.push_str(
            "<p class=\"note\">欠測のためグラフの表示を保留。取得済みの人数は表に表示します。</p>",
        );
    } else {
        let complete: Vec<(String, i64, i64)> = bands
            .iter()
            .filter_map(|b| Some((b.age_group.clone(), b.male?, b.female?)))
            .collect();
        html.push_str("<p class=\"note\">左：男性 / 右：女性</p>");
        html.push_str(&super::section_06_demographics::build_navy_pyramid_svg(
            &complete,
        ));
    }
    html.push_str("</div><div><h2>年齢別人口</h2><table class=\"table-navy\"><thead><tr><th>年齢</th><th>男性</th><th>女性</th><th>合計</th></tr></thead><tbody>");
    for b in bands {
        let display = |n: Option<i64>| n.map(format_number).unwrap_or_else(|| "—".into());
        html.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",escape_html(&b.age_group),display(b.male),display(b.female),display(b.male.zip(b.female).and_then(|(m,f)| m.checked_add(f)))));
    }
    if bands.is_empty() {
        html.push_str("<tr><td colspan=\"4\">データなし</td></tr>");
    }
    html.push_str("</tbody></table></div></div>");
}

fn keyword_table(html: &mut String, label: &str, data: &[KeywordRow]) {
    html.push_str(&format!("<h2>{}</h2><table class=\"words\"><tr><th>上位10件</th><th>件数</th><th>求人数</th><th>占有率</th></tr>",escape_html(label)));
    for row in data.iter().take(10) {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.0}%</td></tr>",
            escape_html(&row.word),
            row.count,
            row.jobs,
            row.share_pct
        ));
    }
    if data.is_empty() {
        html.push_str("<tr><td colspan=\"4\">キーワードデータがありません</td></tr>");
    }
    html.push_str("</table>");
}

/// 給与分布の縦棒グラフ。空の給与区間も 0 件の棒 (高さ 0) として残し、軸を連続にする。
fn chart(html: &mut String, label: &str, series: &HistogramSeries) {
    let data = &series.bins;
    html.push_str(&format!(
        "<figure class=\"chart salary-chart wide\"><figcaption>{}</figcaption>",
        escape_html(label)
    ));
    let Some(summary) = series.summary.as_deref() else {
        html.push_str("<p class=\"note\">集計できるデータがありません</p></figure>");
        return;
    };
    let (w, h, bottom) = (900.0, 200.0, 35.0);
    let plot = h - bottom - 24.0;
    let peak = data.iter().map(|x| x.count).max().unwrap_or(1).max(1);
    let tick_step = peak.div_ceil(4);
    let max = (tick_step * 4) as f64;
    let gap = (w - 64.0) / data.len() as f64;
    let peaks = data.iter().filter(|b| b.count == peak).count();
    html.push_str(&format!(
        "<p class=\"salary-chart-summary\"><span class=\"salary-swatch\"></span>{}</p>",
        escape_html(summary)
    ));
    html.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{}\"><title>{}</title>",
        escape_html(label),
        escape_html(label)
    ));
    for i in 0..=4 {
        let y = 24.0 + plot * (1.0 - i as f64 / 4.0);
        html.push_str(&format!("<path d=\"M 48 {y} H {}\" stroke=\"#e3e9ed\"/><text x=\"40\" y=\"{}\" text-anchor=\"end\" font-size=\"14\" fill=\"#536169\">{:.0}</text>",w-16.0,y+5.0,max*i as f64/4.0));
    }
    html.push_str("<text x=\"8\" y=\"14\" font-size=\"13\" fill=\"#536169\">件数</text>");
    let label_stride = data.len().div_ceil(12);
    for (i, b) in data.iter().enumerate() {
        let n = b.count;
        let x = 48.0 + i as f64 * gap + gap * 0.08;
        let bar = plot * n as f64 / max;
        let y = 24.0 + plot - bar;
        let lx = x + gap * 0.42;
        let ly = plot + 49.0;
        let fill = if n == peak { "#007d79" } else { "#5d83b8" };
        html.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{bar}\" rx=\"2\" fill=\"{fill}\"><title>{}: {n}件</title></rect>",gap*0.84,escape_html(&b.label)));
        if n == peak && peaks == 1 {
            html.push_str(&format!("<text x=\"{lx}\" y=\"{}\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"700\" fill=\"#006666\">{n}件</text>",y-7.0));
        }
        if i % label_stride == 0
            || (i == data.len() - 1 && i % label_stride >= label_stride / 2 && label_stride > 1)
        {
            html.push_str(&format!("<text transform=\"translate({lx} {ly}) rotate(0)\" text-anchor=\"middle\" font-size=\"14\" fill=\"#536169\">{}</text>",escape_html(&b.label)));
        }
    }
    html.push_str("</svg></figure>");
}

/// 数値の整形。整数は桁区切り、小数は 2 桁。未取得は「—」。
fn number(value: Option<f64>) -> String {
    value
        .map(|n| {
            if n.fract() == 0.0 {
                format_number(n as i64)
            } else {
                format!("{n:.2}")
            }
        })
        .unwrap_or_else(|| "—".into())
}

fn render_indeed(html: &mut String, data: &IndeedSection) {
    html.push_str("<section class=\"page-navy\" id=\"competitor-indeed\">");
    push_page_head(
        html,
        "採用市場",
        "Indeed採用レポート",
        "選択した職種・都道府県の月別データ",
    );
    let (title, region, rows) = match data {
        IndeedSection::Unavailable { message } => {
            html.push_str(&format!(
                "<p class=\"note\">{}</p></section>",
                escape_html(message)
            ));
            return;
        }
        IndeedSection::Ok {
            title,
            region,
            rows,
            ..
        } => (title, region, rows),
    };
    html.push_str(&format!("<p>{} / {}</p><p class=\"note\">出典：Indeed採用市場レポート｜全給与形態。閲覧人数は応募数ではありません。</p>",escape_html(title),escape_html(region)));
    html.push_str("<div class=\"trend-grid\">");
    for (label, unit, color, ratio, pick) in [
        (
            "求人数の推移",
            "件",
            "#007d79",
            false,
            (|r: &super::competitor_model::IndeedRow| r.job)
                as fn(&super::competitor_model::IndeedRow) -> Option<f64>,
        ),
        ("求人を見た人数の推移", "人", "#4472c4", false, |r| r.ctk),
        ("募集企業数の推移", "社", "#8567a5", false, |r| {
            r.emp
        }),
        (
            "1求人あたりに見た人数",
            "人/求人",
            "#b37d20",
            true,
            |r| r.spp,
        ),
    ] {
        let points: Vec<trends::Point<'_>> =
            rows.iter().map(|r| (r.month.as_str(), pick(r))).collect();
        trends::chart(html, label, &points, unit, color, ratio);
    }
    html.push_str("</div>");
    html.push_str("<table class=\"table-navy\"><thead><tr><th>月</th><th>求人数</th><th>求人を見た人数</th><th>募集企業数</th><th>1求人あたりに見た人数</th></tr></thead><tbody>");
    if rows.is_empty() {
        html.push_str(
            "<tr><td colspan=\"5\">月別データがありません。0件を意味しません。</td></tr>",
        );
    }
    for row in rows {
        html.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",escape_html(&row.month),number(row.job),number(row.ctk),number(row.emp),number(row.spp)));
    }
    html.push_str("</tbody></table></section>");
}

fn render_google(html: &mut String, data: &GoogleSection) {
    html.push_str("<section class=\"page-navy\" id=\"competitor-google\">");
    push_page_head(
        html,
        "検索需要",
        "Google広告APIの検索需要",
        "月間検索数と推移",
    );
    html.push_str("<p class=\"note\">出典：Google広告 Keyword Planner API｜検索数は推定値。広告競合度は広告主間の競合です。</p>");
    let (keyword, region, demand, suggestions) = match data {
        GoogleSection::NotRequested { message } | GoogleSection::Error { message } => {
            html.push_str(&format!("<p>{}</p></section>", escape_html(message)));
            return;
        }
        GoogleSection::Ok {
            keyword,
            region,
            demand,
            suggestions,
        } => (keyword, region, demand, suggestions),
    };
    html.push_str(&format!(
        "<p>検索語: {} / 指定地域: {}</p>",
        escape_html(keyword),
        if region.is_empty() {
            "全国".into()
        } else {
            escape_html(region)
        }
    ));
    if let GoogleDemand::Ok {
        region_name,
        keywords,
    } = demand
    {
        match region_name {
            None if !region.is_empty() => html.push_str(
                "<p class=\"note\">指定地域を解決できなかったため全国の検索需要です。</p>",
            ),
            Some(name) => html.push_str(&format!(
                "<p class=\"note\">取得地域: {}</p>",
                escape_html(name)
            )),
            None => {}
        }
        html.push_str("<table class=\"table-navy\"><thead><tr><th>検索語</th><th>平均月間検索数</th><th>広告競合度</th></tr></thead><tbody>");
        if keywords.is_empty() {
            html.push_str("<tr><td colspan=\"3\">検索需要のデータがありません。</td></tr>");
        }
        for row in keywords {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td>{}</td></tr>",
                escape_html(&row.keyword),
                number(row.avg_monthly),
                escape_html(&row.competition)
            ));
        }
        html.push_str("</tbody></table>");
        for row in keywords {
            let points: Vec<trends::Point<'_>> = row
                .monthly_12m
                .iter()
                .map(|m| (m.month.as_str(), m.search_volume))
                .collect();
            trends::chart(
                html,
                &format!("{}：月間検索数の推移", row.keyword),
                &points,
                "回/月",
                "#007d79",
                false,
            );
            html.push_str(&format!("<div class=\"block-title\">{} の月別検索数</div><table class=\"table-navy\"><thead><tr><th>月</th><th>検索数</th></tr></thead><tbody>",escape_html(&row.keyword)));
            for month in &row.monthly_12m {
                html.push_str(&format!(
                    "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                    escape_html(&month.month),
                    number(month.search_volume)
                ));
            }
            html.push_str("</tbody></table>");
        }
    } else {
        // 外部APIの生エラーは資格情報を含む可能性があるので本文に転記しない。
        html.push_str("<p class=\"note\">Google検索需要を取得できませんでした。</p>");
    }
    // 取得できなかった関連語は空の見出しを出さない。
    if let GoogleSuggestions::Ok {
        region_name,
        suggestions,
    } = suggestions
    {
        html.push_str("<div class=\"block-title\">関連キーワード（上位20件）</div>");
        match region_name {
            None => html.push_str("<p class=\"note\">関連キーワードの取得地域：全国（地域指定なし・地域未解決）。指定地域の需要とは限りません。</p>"),
            Some(name) => html.push_str(&format!(
                "<p class=\"note\">関連キーワードの取得地域：{}</p>",
                escape_html(name)
            )),
        }
        html.push_str("<table class=\"table-navy\"><thead><tr><th>関連語</th><th>平均月間検索数</th></tr></thead><tbody>");
        if suggestions.is_empty() {
            html.push_str("<tr><td colspan=\"2\">関連キーワードのデータがありません。需要0を意味しません。</td></tr>");
        }
        for row in suggestions {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                escape_html(&row.keyword),
                number(row.avg_monthly)
            ));
        }
        html.push_str(
            "</tbody></table><p class=\"note\">求人票の訴求語と検索語を比較できます。</p>",
        );
    }
    html.push_str("</section>");
}

#[cfg(test)]
mod tests {
    use super::super::competitor_model::{google_section, population_section};
    use super::*;
    use crate::handlers::survey::aggregator::{aggregate_records_with_mode, salary_fixture};
    use crate::handlers::survey::upload::WageMode;
    use serde_json::json;

    fn indeed(v: Value) -> IndeedSection {
        super::super::competitor_model::build_competitor_report(
            &aggregate_records_with_mode(
                &salary_fixture::records("月給 20万円", "大阪府 大阪市"),
                WageMode::Monthly,
            ),
            1,
            "",
            &v,
            &Value::Null,
            &Value::Null,
        )
        .indeed
    }

    #[test]
    fn population_missing_is_retained_and_not_zero() {
        let mut html = String::new();
        render_population(
            &mut html,
            &population_section(
                &json!({"status":"ok","bands":[{"age_group":"20-24","male_count":null,"female_count":100}]}),
            ),
        );
        assert!(html.contains("<td>20-24</td><td class=\"num\">—</td><td class=\"num\">100</td><td class=\"num\">—</td>"));
        assert!(html.contains("グラフの表示を保留"));
        assert!(!html.contains("<svg"));
        html.clear();
        render_population(
            &mut html,
            &population_section(
                &json!({"status":"ok","bands":[{"age_group":"20-24","male_count":0,"female_count":100}]}),
            ),
        );
        assert!(html.contains("<td>20-24</td><td class=\"num\">0</td><td class=\"num\">100</td><td class=\"num\">100</td>"));
        assert!(!html.contains("グラフの表示を保留"));
        assert!(html.contains("<svg"));
    }

    #[test]
    fn google_regions_and_empty_results_are_independent() {
        let mut html = String::new();
        let mut data = json!({"status":"ok","region":"大阪府","demand":{"status":"ok","region":{"canonical_name":"Osaka, Japan"},"keywords":[]},"suggestions":{"status":"ok","region":null,"suggestions":[]}});
        render_google(&mut html, &google_section(&data));
        assert!(html.contains("取得地域: Osaka, Japan"));
        // 関連語の地域は検索需要とは別に扱う。地域が null なら全国の注記
        assert!(html.contains("関連キーワードの取得地域：全国"));
        assert!(html.contains("需要0を意味しません"));
        data["suggestions"]["region"] = json!({"canonical_name":"Tokyo, Japan"});
        html.clear();
        render_google(&mut html, &google_section(&data));
        assert!(html.contains("関連キーワードの取得地域：Tokyo, Japan"));
        assert!(!html.contains("関連キーワードの取得地域：全国"));
        data["suggestions"]["region"] = Value::Null;
        data["demand"]["status"] = json!("error");
        data["suggestions"]["suggestions"] = json!([{"keyword":"<test>","avg_monthly":0}]);
        html.clear();
        render_google(&mut html, &google_section(&data));
        assert!(html.contains("Google検索需要を取得できません"));
        assert!(html.contains("&lt;test&gt;</td><td class=\"num\">0</td>"));
        assert!(!html.contains("需要0を意味しません"));
        // 関連語を取得できなかったときは、空の見出しを出さない。
        data["suggestions"]["status"] = json!("error");
        html.clear();
        render_google(&mut html, &google_section(&data));
        assert!(!html.contains("関連キーワード"));
    }

    #[test]
    fn indeed_empty_is_not_observed_zero() {
        let mut html = String::new();
        render_indeed(
            &mut html,
            &indeed(json!({"status":"ok","title":"t","region":"r","rows":[]})),
        );
        assert!(html.contains("0件を意味しません"));
        html.clear();
        render_indeed(
            &mut html,
            &indeed(
                json!({"status":"ok","title":"t","region":"r","rows":[{"month":"2026-01","job":0}]}),
            ),
        );
        assert!(html.contains("2026-01</td><td class=\"num\">0</td><td class=\"num\">—</td>"));
        assert!(!html.contains("0件を意味しません"));
    }

    fn series(bins: &[(&str, u32)]) -> HistogramSeries {
        let bins: Vec<_> = bins
            .iter()
            .map(|(l, c)| super::super::competitor_model::HistogramBin {
                label: (*l).to_owned(),
                count: *c,
            })
            .collect();
        super::super::competitor_model::series_from_bins(bins, 0, 1.0)
    }

    #[test]
    fn salary_peak_summary_preserves_ties_and_counts() {
        let mut html = String::new();
        chart(
            &mut html,
            "salary",
            &series(&[("34", 33), ("35", 33), ("36", 0)]),
        );
        assert!(html.contains("最多の給与帯：34〜35・35〜36 / 各33件"));
        assert_eq!(html.matches("fill=\"#007d79\"").count(), 2);
        assert!(html.contains("36: 0件"));
        assert!(!html.contains("font-weight=\"700\""));
        html.clear();
        chart(&mut html, "salary", &series(&[("30", 101), ("31", 466)]));
        assert!(html.contains("31〜32 / 466件・82.2%"));
        html.clear();
        chart(&mut html, "salary", &series(&[("30", 0)]));
        assert!(!html.contains("最多"));
    }

    #[test]
    fn salary_axis_preserves_empty_intervals_and_bounds_large_ranges() {
        let agg = aggregate_records_with_mode(
            &salary_fixture::records("月給 20万円\n月給 21万円\n月給 50万円", "大阪府 大阪市"),
            WageMode::Monthly,
        );
        let report = build_competitor_report(&agg, 1, "", &Value::Null, &Value::Null, &Value::Null);
        let lower = &report.excel.histograms.lower;
        assert_eq!(lower.bins.len(), 31);
        assert_eq!(
            (lower.bins[0].label.as_str(), lower.bins[0].count),
            ("20", 1)
        );
        assert_eq!(
            (lower.bins[1].label.as_str(), lower.bins[1].count),
            ("21", 1)
        );
        assert_eq!(
            (lower.bins[2].label.as_str(), lower.bins[2].count),
            ("22", 0)
        );
        assert_eq!(
            (lower.bins[30].label.as_str(), lower.bins[30].count),
            ("50", 1)
        );
        assert_eq!((lower.n, lower.step), (3, 1.0));
        let mut html = String::new();
        chart(&mut html, "one job", &series(&[("20", 1)]));
        for tick in [
            ">0</text>",
            ">1</text>",
            ">2</text>",
            ">3</text>",
            ">4</text>",
        ] {
            assert_eq!(html.matches(tick).count(), 1);
        }
    }

    #[test]
    fn keyword_comparison_keeps_untagged_jobs_and_counts_each_tag_once() {
        let mut records = salary_fixture::records(
            "月給 20万円\n月給 21万円\n月給 22万円\n月給 23万円",
            "大阪府 大阪市",
        );
        records[0].tags_raw = "研修あり,研修あり".into();
        records[2].tags_raw = "研修あり".into();
        // The second and fourth records have no tags. They still belong in the denominator.
        let agg = aggregate_records_with_mode(&records, WageMode::Monthly);
        let html = render_competitor_report(&agg, 2, "", &json!({}), &json!({}), &json!({}));
        assert!(
            html.contains("<td>研修あり</td><td>1</td><td>50.0%</td><td>50.0%</td><td>0.0</td>")
        );
        assert!(html.contains("母数：先頭 2 件 / 全体 4 件"));
        let capped = render_competitor_report(&agg, 45, "", &json!({}), &json!({}), &json!({}));
        assert!(capped.contains("母数：先頭 4 件 / 全体 4 件"));
        assert!(
            capped.contains("<td>研修あり</td><td>2</td><td>50.0%</td><td>50.0%</td><td>0.0</td>")
        );
    }

    #[test]
    fn standalone_report_has_salary_competitor_and_source_backed_market_data() {
        let records = salary_fixture::records(
            "時給 1000円 ~ 1200円\n時給 1100円 ~ 1300円",
            "大阪府 大阪市",
        );
        let agg = aggregate_records_with_mode(&records, WageMode::Hourly);
        let indeed = json!({"status":"ok","title":"施設長","region":"大阪府","rows":[{"month":"2026-08","job":100,"ctk":250,"emp":20,"spp":2.5}]});
        let google = json!({"status":"ok","keyword":"施設長 求人","region":"大阪府","demand":{"status":"ok","region":{"canonical_name":"Osaka, Japan"},"keywords":[{"keyword":"施設長 求人","avg_monthly":320,"competition":"HIGH","monthly_12m":[{"month":"2026-08","search_volume":390}]}]},"suggestions":{"status":"ok","suggestions":[{"keyword":"施設長 転職","avg_monthly":170}]}});
        let html = render_competitor_report(
            &agg,
            1,
            "<script>危険</script>",
            &indeed,
            &google,
            &json!({"status":"unavailable","message":"外部統計未接続"}),
        );
        for needle in [
            "下限ボリュームゾーン",
            "上限ボリュームゾーン",
            "競合調査ダッシュボード",
            "先頭 1 件",
            "250",
            "2.50",
            "320",
            "390",
            "施設長 転職",
            "170",
            "円/時",
        ] {
            assert!(html.contains(needle), "{needle}");
        }
        assert!(!html.contains("<script>危険</script>"));
        assert!(html.contains("&lt;script&gt;危険&lt;/script&gt;"));
        assert!(!html.contains("地域企業構造"));
    }

    /// 時給 16 件の平均 (下限 (8*1260+8*1265)/16 = 1262.5、上限 +150 で 1412.5) が
    /// 小数を捨てず、円の整数 (四捨五入) で出ること。月給の表示 (万円・小数 2 桁) は変えない。
    #[test]
    fn hourly_average_is_rounded_not_truncated() {
        let mut lines = Vec::new();
        for lo in [1260, 1265] {
            for _ in 0..8 {
                lines.push(format!("時給 {lo}円 ~ {}円", lo + 150));
            }
        }
        let records = salary_fixture::records(&lines.join("\n"), "大阪府 大阪市");
        let agg = aggregate_records_with_mode(&records, WageMode::Hourly);
        let html = render_competitor_report(
            &agg,
            45,
            "t",
            &json!({"status":"unavailable"}),
            &json!({"status":"unavailable"}),
            &json!({"status":"unavailable"}),
        );
        assert!(
            html.contains("<th>平均値</th><td>1263</td><td>1413</td>"),
            "{}",
            &html[html.find("<th>平均値</th>").unwrap_or(0)..][..120]
        );
        assert!(!html.contains("1262.00") && !html.contains("1412.00"));
    }

    #[test]
    fn google_error_does_not_render_raw_credentials_or_invent_search_counts() {
        let mut html = String::new();
        render_google(
            &mut html,
            &google_section(
                &json!({"status":"ok","demand":{"status":"error","message":"secret-token"},"suggestions":{"status":"missing_credentials"}}),
            ),
        );
        assert!(!html.contains("secret-token"));
        assert!(html.contains("取得できませんでした"));
        assert!(!html.contains("class=\"num\">0"));
    }

    #[test]
    fn competitor_population_uses_statistical_counts_and_keeps_missing_values() {
        let mut html = String::new();
        render_population(
            &mut html,
            &population_section(
                &json!({"status":"ok","region":"<大阪府>","bands":[{"age_group":"20〜24歳","male_count":1234,"female_count":2345}],"minimum_wage":null,"labor":{"fiscal_year":2024,"unemployment_rate":2.5}}),
            ),
        );
        assert!(html.contains("&lt;大阪府&gt;"));
        assert!(html.contains("1,234"));
        assert!(html.contains("2,345"));
        assert!(html.contains("3,579"));
        assert!(html.contains("<svg"));
        assert!(html.contains("<td>—</td>"));
        assert!(html.contains("2024"));
        assert!(html.contains("2.50"));
    }

    #[test]
    fn national_population_hides_wage_and_labor_and_shows_reference_date() {
        let mut html = String::new();
        render_population(
            &mut html,
            &population_section(
                &json!({"status":"ok","region":"全国","reference_date":"2020-10-01",
                "bands":[{"age_group":"20-29","male_count":100,"female_count":200}],
                "totals":{"total_population":1000,"male_population":400,"female_population":600}}),
            ),
        );
        assert!(html.contains("人口の基準日：2020-10-01"));
        assert!(html.contains("総人口の100.0%"));
        assert!(!html.contains("最低賃金"));
        html.clear();
        render_population(
            &mut html,
            &population_section(
                &json!({"status":"unavailable","region":"全国","message":"全国の人口データを取得できませんでした。"}),
            ),
        );
        assert!(html.contains("集計地域：全国"));
    }

    #[test]
    fn export_marketing_trials_when_requested() {
        let Some(manifest) = std::env::var_os("COMPETITOR_TRIAL_MANIFEST") else {
            return;
        };
        use crate::handlers::survey::upload::{parse_csv_bytes_with_hints, UserSourceHint};
        let manifest: Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        let root = std::path::Path::new(manifest["output_dir"].as_str().unwrap());
        for case in manifest["cases"].as_array().unwrap() {
            let slug = case["slug"].as_str().unwrap();
            assert!(slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
            let mut records = parse_csv_bytes_with_hints(
                &std::fs::read(case["source_csv"].as_str().unwrap()).unwrap(),
                None,
                UserSourceHint::IndeedSp,
            )
            .unwrap();
            if case["monthly_only"] == true {
                records.retain(|r| {
                    r.salary_raw.contains("月給")
                        && r.salary_parsed.salary_type
                            == crate::handlers::survey::salary_parser::SalaryType::Monthly
                });
                assert!(records.iter().all(|r| r.salary_raw.contains("月給")));
            }
            assert!(!records.is_empty());
            assert!(records
                .iter()
                .all(|r| !r.job_title.is_empty() && r.url.is_some()));
            let agg = aggregate_records_with_mode(&records, WageMode::Monthly);
            assert_eq!(agg.total_count, records.len());
            let html = render_competitor_report(
                &agg,
                45,
                case["title"].as_str().unwrap(),
                &case["indeed"],
                &case["google"],
                &case["population"],
            );
            let output = root.join(slug);
            std::fs::create_dir_all(&output).unwrap();
            std::fs::write(output.join("report.html"), html).unwrap();
            std::fs::write(
                output.join("aggregation.json"),
                serde_json::to_vec_pretty(&agg).unwrap(),
            )
            .unwrap();
            std::fs::write(
                output.join("records.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
            println!("{slug}: {} unique records", agg.total_count);
        }
    }

    #[test]
    fn export_excel_source_preview_when_requested() {
        let Some(source) = std::env::var_os("COMPETITOR_EXCEL_CSV") else {
            return;
        };
        use crate::handlers::survey::upload::{parse_csv_bytes_with_hints, UserSourceHint};
        let records = parse_csv_bytes_with_hints(
            &std::fs::read(source).unwrap(),
            Some("大阪府"),
            UserSourceHint::IndeedSp,
        )
        .unwrap();
        let agg = aggregate_records_with_mode(&records, WageMode::Monthly);
        assert!(agg.total_count > 500);
        let rate = crate::minimum_wage::resolve("大阪府", None).unwrap();
        let population = json!({"status":"ok", "region":"大阪府", "bands":[],
            "minimum_wage":rate.hourly_min_wage, "minimum_wage_fiscal_year":rate.fiscal_year,
            "minimum_wage_effective_date":rate.effective_date.to_string(),
            "minimum_wage_source_url":rate.source_url, "minimum_wage_source":rate.source,
            "minimum_wage_as_of":crate::minimum_wage::japan_today().to_string(), "labor":null});
        let html = render_competitor_report(
            &agg,
            45,
            "施設長 / 大阪府",
            &json!({"status":"unavailable","message":"プレビューではIndeed採用市場データを取得していません。"}),
            &json!({"status":"unavailable","message":"プレビューではGoogle広告APIを接続していません。"}),
            &population,
        );
        let root = std::path::PathBuf::from(std::env::var_os("COMPETITOR_PREVIEW_DIR").unwrap());
        std::fs::write(root.join("report.html"), html).unwrap();
        println!(
            "Excel source: {} unique jobs, {} lower salaries, {} upper salaries",
            agg.total_count,
            agg.salary_min_values.len(),
            agg.salary_max_values.len()
        );
    }
}
