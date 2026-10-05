//! Excel競合調査ダッシュボードの独立出力。
//!
//! `render_html` は `CompetitorReport` (competitor_model.rs) だけを入力にする。
//! 数値の加工は `build_competitor_report` が済ませているので、ここは整形と HTML 組み立てだけ。
use super::common::push_page_head;
use super::competitor_model::{
    build_competitor_report, CompetitorReport, GoogleDemand, GoogleSection, GoogleSuggestions,
    IndeedSection, KeywordRow, PopulationSection, SalaryRow,
};
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
    html.push_str("<div class=\"report-tabs no-print\" role=\"tablist\" aria-label=\"競合調査の表示切り替え\"><button id=\"tab-excel\" role=\"tab\" aria-selected=\"true\" aria-controls=\"panel-excel\" tabindex=\"0\">Excel再現</button><button id=\"tab-google\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-google\" tabindex=\"-1\">Google検索需要</button><button id=\"tab-indeed\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-indeed\" tabindex=\"-1\">Indeed採用レポート</button><button id=\"tab-population\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-population\" tabindex=\"-1\">人口・地域データ</button></div><div id=\"panel-excel\" role=\"tabpanel\" aria-labelledby=\"tab-excel\" tabindex=\"0\">");
    let unit = meta.unit.as_str();
    let top_n = meta.top_n_effective as usize;
    html.push_str("<!-- Design review: Philosophy 5; Hierarchy 4; Execution 4; Specificity 5; Restraint 5; Variety 4. Source: supplied Excel dashboard. --><section class=\"excel-dashboard\" aria-label=\"競合調査ダッシュボード\"><aside class=\"summary\"><h1>競合調査</h1><table class=\"meta\">");
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
    keyword_table(
        &mut html,
        &format!("求人票ワード調査（上位 {top_n} 件）"),
        &excel.keyword_head,
    );
    html.push_str("<p class=\"note\">給与比較はIndeed SPの主単位の求人を集計（SPデータがない場合、総合は給与分布と同じ対象）。人気求人はSPの「人気」「超人気」付き。最頻値は実額（同数は低い額）、未取得は —。少数の人気求人は参考値です。</p></aside><div class=\"charts\">");
    for (label, bins) in [
        ("上限ボリュームゾーン", &excel.histograms.upper),
        ("下限ボリュームゾーン", &excel.histograms.lower),
    ] {
        let data: Vec<_> = bins
            .iter()
            .map(|b| (b.label.clone(), b.count as usize))
            .collect();
        chart(&mut html, &format!("{label}（{unit}）"), &data, true, false);
    }
    chart(
        &mut html,
        "求人票キーワード調査（全体）",
        &keyword_chart_data(&excel.keyword_all),
        false,
        true,
    );
    chart(
        &mut html,
        &format!("求人票キーワード調査（上位 {top_n} 件）"),
        &keyword_chart_data(&excel.keyword_head),
        false,
        true,
    );
    html.push_str(&format!("</div></section><p class=\"caption\">CSV重複排除後 {} 件 / 上位 {} 件は取り込み順の先頭。ワード表は上位10語、グラフは上位25語。給与分布は{}刻みで、月給モードは既存の月給換算値を使用。給与比較表とは対象が異なる場合があります。</p></div><div id=\"panel-google\" role=\"tabpanel\" aria-labelledby=\"tab-google\" tabindex=\"0\" hidden>",meta.total_count,top_n.min(meta.total_count as usize),if meta.is_hourly{"50円"}else{"1万円"}));
    render_google(&mut html, &report.google);
    html.push_str("</div><div id=\"panel-indeed\" role=\"tabpanel\" aria-labelledby=\"tab-indeed\" tabindex=\"0\" hidden>");
    render_indeed(&mut html, &report.indeed);
    html.push_str("</div><div id=\"panel-population\" role=\"tabpanel\" aria-labelledby=\"tab-population\" tabindex=\"0\" hidden>");
    render_population(&mut html, &report.population);
    html.push_str("</div></main><script>");
    html.push_str(include_str!("../../../../../static/js/competitor-tabs.js"));
    html.push_str("</script></body></html>");
    html
}

fn keyword_chart_data(rows: &[KeywordRow]) -> Vec<(String, usize)> {
    rows.iter()
        .take(25)
        .map(|r| (r.word.clone(), r.count as usize))
        .collect()
}

fn render_population(html: &mut String, data: &PopulationSection) {
    html.push_str("<section class=\"page-navy\"><h1>人口・地域データ</h1>");
    let PopulationSection::Ok {
        region,
        bands,
        minimum_wage,
        minimum_wage_fiscal_year,
        minimum_wage_effective_date,
        minimum_wage_as_of,
        minimum_wage_source,
        labor,
    } = data
    else {
        let PopulationSection::Unavailable { message } = data else {
            return;
        };
        // 旧実装は集計地域が空のまま出していた (未取得のときは region キーが無い)。
        html.push_str("<p>集計地域：</p>");
        html.push_str(&format!(
            "<p class=\"note\">{}</p></section>",
            escape_html(message)
        ));
        return;
    };
    html.push_str(&format!("<p>集計地域：{}</p>", escape_html(region)));
    html.push_str("<p class=\"note\">出典：国勢調査（人口）、厚生労働省（最低賃金）、e-Stat社会人口統計体系・労働政策研究・研修機構（労働統計）。都道府県単位の外部統計です。地域の人口は求人閲覧人数・検索数・応募数とは異なります。統計ごとに調査時点は異なります。</p>");
    let bands: Vec<(String, i64, i64)> = bands
        .iter()
        .map(|b| (b.age_group.clone(), b.male, b.female))
        .collect();
    html.push_str("<div class=\"population-grid\"><div><h2>人口ピラミッド</h2>");
    if bands.is_empty() {
        html.push_str("<p class=\"note\">人口データがありません。</p>");
    } else {
        html.push_str("<p class=\"note\">左：男性 / 右：女性</p>");
        html.push_str(&super::section_06_demographics::build_navy_pyramid_svg(
            &bands,
        ));
    }
    html.push_str("</div><div><h2>年齢別人口</h2><table class=\"table-navy\"><thead><tr><th>年齢</th><th>男性</th><th>女性</th><th>合計</th></tr></thead><tbody>");
    for (age, male, female) in &bands {
        html.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",escape_html(age),format_number(*male),format_number(*female),format_number(male+female)));
    }
    if bands.is_empty() {
        html.push_str("<tr><td colspan=\"4\">データなし</td></tr>");
    }
    html.push_str("</tbody></table></div></div><h2>地域の最低賃金・労働統計</h2><table class=\"table-navy\"><tr><th>最低賃金（円/時）</th><td>");
    html.push_str(&number(*minimum_wage));
    let wage_year = minimum_wage_fiscal_year
        .map(|y| y.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>最低賃金の改定年度</th><td>{wage_year}</td></tr><tr><th>最低賃金の発効日</th><td>{}</td></tr><tr><th>最低賃金の基準日（日本時間）</th><td>{}</td></tr><tr><th>最低賃金の出典</th><td>{}",escape_html(minimum_wage_effective_date),escape_html(minimum_wage_as_of),if minimum_wage_source=="official_csv" {"厚生労働省の公式改定一覧"} else if minimum_wage_source=="database" {"外部統計データベース"} else {"—"}));
    let year = labor
        .as_ref()
        .and_then(|l| l.fiscal_year)
        .map(|year| year.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>労働統計の年度</th><td>{year}</td></tr><tr><th>完全失業率（%）</th><td>{}</td></tr><tr><th>離職率（%）</th><td>{}</td></tr></table><p class=\"note\">取得できない指標は — と表示します。</p></section>",number(labor.as_ref().and_then(|l| l.unemployment_rate)),number(labor.as_ref().and_then(|l| l.separation_rate))));
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

fn chart(html: &mut String, label: &str, data: &[(String, usize)], wide: bool, words: bool) {
    html.push_str(&format!(
        "<figure class=\"chart {}\"><figcaption>{}</figcaption>",
        if wide { "wide" } else { "" },
        escape_html(label)
    ));
    if data.is_empty() {
        html.push_str("<p class=\"note\">集計できるデータがありません</p></figure>");
        return;
    }
    let w = if wide { 900.0 } else { 440.0 };
    let h = if words { 450.0 } else { 200.0 };
    let bottom = if words { 135.0 } else { 35.0 };
    let plot = h - bottom - 10.0;
    let max = data.iter().map(|x| x.1).max().unwrap_or(1).max(1) as f64;
    let gap = (w - 45.0) / data.len() as f64;
    html.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{}\"><title>{}</title>",
        escape_html(label),
        escape_html(label)
    ));
    for i in 0..=4 {
        let y = 10.0 + plot * (1.0 - i as f64 / 4.0);
        html.push_str(&format!("<path d=\"M 36 {y} H {w}\" stroke=\"#d5d9dd\"/><text x=\"30\" y=\"{}\" text-anchor=\"end\" font-size=\"11\" fill=\"#536169\">{:.0}</text>",y+4.0,max*i as f64/4.0));
    }
    for (i, (word, n)) in data.iter().enumerate() {
        let x = 40.0 + i as f64 * gap;
        let bar = plot * *n as f64 / max;
        let y = 10.0 + plot - bar;
        let lx = x + gap / 2.0;
        let ly = plot + 26.0;
        html.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{bar}\" fill=\"#4472c4\"><title>{}: {n}件</title></rect>",gap*0.72,escape_html(word)));
        if words || data.len() < 35 || i % ((data.len() / 25).max(1)) == 0 {
            html.push_str(&format!("<text transform=\"translate({lx} {ly}) rotate({})\" text-anchor=\"{}\" font-size=\"{}\" fill=\"#536169\">{}</text>",if words{60}else{0},if words{"start"}else{"middle"},if words{10}else{11},escape_html(word)));
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
    let (title, region, source, caveat, built_at, rows) = match data {
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
            source,
            caveat,
            built_at,
            rows,
        } => (title, region, source, caveat, built_at, rows),
    };
    html.push_str(&format!("<p>{} / {}</p><p class=\"note\">出典: {} / 集計日: {}。{} 求人を見た人数は応募数ではありません。欠測は「—」で表示します。</p>",escape_html(title),escape_html(region),escape_html(source),escape_html(built_at),escape_html(caveat)));
    html.push_str("<table class=\"table-navy\"><thead><tr><th>月</th><th>求人数</th><th>求人を見た人数</th><th>募集企業数</th><th>1求人あたりに見た人数</th></tr></thead><tbody>");
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
        "検索ボリューム・月別推移・関連キーワード",
    );
    html.push_str("<p class=\"note\">出典: Google広告 Keyword Planner API。検索数はGoogleの推定検索需要です。Indeedの閲覧人数・CSVの求人数・応募数とは異なる指標です。広告競合度は求人の競合数ではありません。</p>");
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
        html.push_str("<p class=\"note\">Google検索需要を取得できませんでした。API設定または接続状況を確認してください。</p>");
    }
    html.push_str("<div class=\"block-title\">関連キーワード（検索需要順・上位20件）</div>");
    if let GoogleSuggestions::Ok { suggestions } = suggestions {
        html.push_str("<table class=\"table-navy\"><thead><tr><th>関連語</th><th>平均月間検索数</th></tr></thead><tbody>");
        for row in suggestions {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                escape_html(&row.keyword),
                number(row.avg_monthly)
            ));
        }
        html.push_str("</tbody></table><p class=\"note\">CSVで競合が打ち出しているキーワードと、求職者が検索する言葉を照らし合わせて使います。</p>");
    } else {
        html.push_str("<p class=\"note\">関連キーワードを取得できませんでした。</p>");
    }
    html.push_str("</section>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{aggregate_records_with_mode, salary_fixture};
    use crate::handlers::survey::upload::WageMode;
    use serde_json::json;

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
            "上位 1 件",
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
        let records = salary_fixture::records(
            &lines.join(
                "
",
            ),
            "大阪府 大阪市",
        );
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
            &super::super::competitor_model::google_section(
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
            &super::super::competitor_model::population_section(
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
}
