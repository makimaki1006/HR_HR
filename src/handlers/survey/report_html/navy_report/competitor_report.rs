//! Excel競合調査ダッシュボードの独立出力。
use super::common::push_page_head;
use super::section_05b_competitor::head_tag_counts;
use crate::handlers::helpers::{escape_html, format_number};
use crate::handlers::survey::aggregator::{BoundStats, SurveyAggregation};
use serde_json::Value;

pub(crate) fn render_competitor_report(
    agg: &SurveyAggregation,
    top_n: usize,
    title: &str,
    indeed: &Value,
    google: &Value,
    population: &Value,
) -> String {
    let mut html = format!("<!doctype html><html lang=\"ja\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>競合調査</title><style>{}</style></head><body><nav class=\"toolbar no-print\"><a href=\"/competitor\">調査条件に戻る</a><a href=\"/\">市場分析</a><button onclick=\"window.print()\">印刷 / PDF保存</button></nav><main>",include_str!("../../../../../static/css/competitor-dashboard.css"));
    let (head, denom) = head_tag_counts(agg, top_n);
    html.push_str("<div class=\"report-tabs no-print\" role=\"tablist\" aria-label=\"競合調査の表示切り替え\"><button id=\"tab-excel\" role=\"tab\" aria-selected=\"true\" aria-controls=\"panel-excel\" tabindex=\"0\">Excel再現</button><button id=\"tab-google\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-google\" tabindex=\"-1\">Google検索需要</button><button id=\"tab-indeed\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-indeed\" tabindex=\"-1\">Indeed採用レポート</button><button id=\"tab-population\" role=\"tab\" aria-selected=\"false\" aria-controls=\"panel-population\" tabindex=\"-1\">人口・地域データ</button></div><div id=\"panel-excel\" role=\"tabpanel\" aria-labelledby=\"tab-excel\" tabindex=\"0\">");
    let comp = &agg.competitor;
    let unit = if agg.is_hourly { "円/時" } else { "万円" };
    let scale = if agg.is_hourly { 1.0 } else { 10000.0 };
    let fmt = |v: Option<i64>| {
        v.map(|n| format!("{:.2}", n as f64 / scale))
            .unwrap_or_else(|| "—".into())
    };
    html.push_str("<!-- Design review: Philosophy 5; Hierarchy 4; Execution 4; Specificity 5; Restraint 5; Variety 4. Source: supplied Excel dashboard. --><section class=\"excel-dashboard\" aria-label=\"競合調査ダッシュボード\"><aside class=\"summary\"><h1>競合調査</h1><table class=\"meta\">");
    for (a, b, c, d) in [
        (
            "調査名",
            if title.is_empty() {
                "Indeed競合調査"
            } else {
                title
            },
            "雇用形態",
            agg.by_employment_type
                .first()
                .map(|x| x.0.as_str())
                .unwrap_or("—"),
        ),
        (
            "該当都道府県",
            agg.dominant_prefecture.as_deref().unwrap_or("—"),
            "主な市町村",
            agg.dominant_municipality.as_deref().unwrap_or("—"),
        ),
    ] {
        html.push_str(&format!(
            "<tr><th>{a}</th><th>{c}</th></tr><tr><td>{}</td><td>{}</td></tr>",
            escape_html(b),
            escape_html(d)
        ));
    }
    html.push_str(&format!("<tr><th>集計対象</th><th>該当件数</th></tr><tr><td>CSV重複排除後</td><td>{}</td></tr></table><h2>給与関係（{unit}）</h2><table><tr><th></th><th colspan=\"2\">総合</th><th colspan=\"2\">人気求人</th></tr><tr><th></th><th>下限</th><th>上限</th><th>下限</th><th>上限</th></tr>",format_number(agg.total_count as i64)));
    let (lo, hi) = if agg.is_hourly {
        (&agg.salary_min_values_native, &agg.salary_max_values_native)
    } else {
        (&agg.salary_min_values, &agg.salary_max_values)
    };
    let fallback = BoundStats::from_values(lo, hi);
    let use_fallback = comp.pop_all.min_n == 0 && comp.pop_all.max_n == 0;
    let all = if use_fallback {
        &fallback
    } else {
        &comp.pop_all
    };
    let mut modes = comp.salary_modes;
    if use_fallback {
        for (index, values) in [lo, hi].into_iter().enumerate() {
            let mut counts = std::collections::BTreeMap::new();
            for value in values {
                *counts.entry(*value).or_insert(0usize) += 1;
            }
            modes[index] = counts
                .into_iter()
                .fold(None, |best: Option<(i64, usize)>, item| {
                    if best.is_none_or(|(_, count)| item.1 > count) {
                        Some(item)
                    } else {
                        best
                    }
                })
                .map(|(v, _)| v);
        }
    }
    let pop = &comp.pop_popular;
    let rows = [
        (
            "平均値",
            [all.min_mean, all.max_mean, pop.min_mean, pop.max_mean],
        ),
        (
            "中央値",
            [
                all.min_median,
                all.max_median,
                pop.min_median,
                pop.max_median,
            ],
        ),
        ("最頻値", modes),
    ];
    for (label, values) in &rows {
        html.push_str(&format!("<tr><th>{label}</th>"));
        for v in values {
            html.push_str(&format!("<td>{}</td>", fmt(*v)));
        }
        html.push_str("</tr>");
    }
    html.push_str(&format!("<tr><th>集計件数</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr></table><h2>差異（総合 − 人気求人）</h2><table><tr><th></th><th>下限</th><th>上限</th></tr>",all.min_n,all.max_n,pop.min_n,pop.max_n));
    for (label, v) in rows {
        html.push_str(&format!(
            "<tr><th>{label}</th><td>{}</td><td>{}</td></tr>",
            fmt(v[0].zip(v[2]).map(|(a, b)| a - b)),
            fmt(v[1].zip(v[3]).map(|(a, b)| a - b))
        ));
    }
    html.push_str("</table>");
    keyword_table(
        &mut html,
        "求人票ワード調査（全体）",
        &comp.tag_counts_all,
        agg.total_count,
    );
    keyword_table(
        &mut html,
        &format!("求人票ワード調査（上位 {top_n} 件）"),
        &head,
        denom,
    );
    html.push_str("<p class=\"note\">給与比較はIndeed SPの主単位の求人を集計（SPデータがない場合、総合は給与分布と同じ対象）。人気求人はSPの「人気」「超人気」付き。最頻値は実額（同数は低い額）、未取得は —。少数の人気求人は参考値です。</p></aside><div class=\"charts\">");
    let (lo, hi) = if agg.is_hourly {
        (&agg.salary_min_values_native, &agg.salary_max_values_native)
    } else {
        (&agg.salary_min_values, &agg.salary_max_values)
    };
    for (label, values) in [("上限ボリュームゾーン", hi), ("下限ボリュームゾーン", lo)]
    {
        let step = if agg.is_hourly { 50 } else { 10000 };
        let mut bins = std::collections::BTreeMap::new();
        for value in values {
            *bins.entry(value / step * step).or_insert(0usize) += 1;
        }
        let data: Vec<_> = bins
            .into_iter()
            .map(|(v, n)| (format!("{:.0}", v as f64 / scale), n))
            .collect();
        chart(&mut html, &format!("{label}（{unit}）"), &data, true, false);
    }
    chart(
        &mut html,
        "求人票キーワード調査（全体）",
        &comp
            .tag_counts_all
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>(),
        false,
        true,
    );
    chart(
        &mut html,
        &format!("求人票キーワード調査（上位 {top_n} 件）"),
        &head.iter().take(25).cloned().collect::<Vec<_>>(),
        false,
        true,
    );
    html.push_str(&format!("</div></section><p class=\"caption\">CSV重複排除後 {} 件 / 上位 {} 件は取り込み順の先頭。ワード表は上位10語、グラフは上位25語。給与分布は{}刻みで、月給モードは既存の月給換算値を使用。給与比較表とは対象が異なる場合があります。</p></div><div id=\"panel-google\" role=\"tabpanel\" aria-labelledby=\"tab-google\" tabindex=\"0\" hidden>",agg.total_count,top_n.min(agg.total_count),if agg.is_hourly{"50円"}else{"1万円"}));
    render_google(&mut html, google);
    html.push_str("</div><div id=\"panel-indeed\" role=\"tabpanel\" aria-labelledby=\"tab-indeed\" tabindex=\"0\" hidden>");
    render_indeed(&mut html, indeed);
    html.push_str("</div><div id=\"panel-population\" role=\"tabpanel\" aria-labelledby=\"tab-population\" tabindex=\"0\" hidden>");
    render_population(&mut html, population);
    html.push_str("</div></main><script>");
    html.push_str(include_str!("../../../../../static/js/competitor-tabs.js"));
    html.push_str("</script></body></html>");
    html
}

fn render_population(html: &mut String, data: &Value) {
    html.push_str("<section class=\"page-navy\"><h1>人口・地域データ</h1>");
    html.push_str(&format!("<p>集計地域：{}</p>", text(data, "region")));
    if data["status"] != "ok" {
        html.push_str(&format!(
            "<p class=\"note\">{}</p></section>",
            text(data, "message")
        ));
        return;
    }
    html.push_str("<p class=\"note\">出典：国勢調査（人口）、厚生労働省（最低賃金）、e-Stat社会人口統計体系・労働政策研究・研修機構（労働統計）。都道府県単位の外部統計です。地域の人口は求人閲覧人数・検索数・応募数とは異なります。統計ごとに調査時点は異なります。</p>");
    let mut bands: Vec<_> = data["bands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some((
                row["age_group"].as_str()?.to_owned(),
                row["male_count"].as_i64()?,
                row["female_count"].as_i64()?,
            ))
        })
        .collect();
    bands.sort_by_key(|(age, _, _)| {
        age.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
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
    html.push_str(&number(data, "minimum_wage"));
    let wage_year = data["minimum_wage_fiscal_year"]
        .as_i64()
        .map(|y| y.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>最低賃金の改定年度</th><td>{wage_year}</td></tr><tr><th>最低賃金の発効日</th><td>{}</td></tr><tr><th>最低賃金の基準日（日本時間）</th><td>{}</td></tr><tr><th>最低賃金の出典</th><td>{}",text(data,"minimum_wage_effective_date"),text(data,"minimum_wage_as_of"),if data["minimum_wage_source"]=="official_csv" {"厚生労働省の公式改定一覧"} else if data["minimum_wage_source"]=="database" {"外部統計データベース"} else {"—"}));
    let year = data["labor"]["fiscal_year"]
        .as_i64()
        .filter(|year| *year > 0)
        .map(|year| year.to_string())
        .unwrap_or_else(|| "—".into());
    html.push_str(&format!("</td></tr><tr><th>労働統計の年度</th><td>{year}</td></tr><tr><th>完全失業率（%）</th><td>{}</td></tr><tr><th>離職率（%）</th><td>{}</td></tr></table><p class=\"note\">取得できない指標は — と表示します。</p></section>",number(&data["labor"],"unemployment_rate"),number(&data["labor"],"separation_rate")));
}

fn keyword_table(html: &mut String, label: &str, data: &[(String, usize)], denom: usize) {
    html.push_str(&format!("<h2>{}</h2><table class=\"words\"><tr><th>上位10件</th><th>件数</th><th>求人数</th><th>占有率</th></tr>",escape_html(label)));
    for (word, n) in data.iter().take(10) {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{n}</td><td>{denom}</td><td>{:.0}%</td></tr>",
            escape_html(word),
            if denom > 0 {
                *n as f64 / denom as f64 * 100.0
            } else {
                0.0
            }
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

fn text(value: &Value, key: &str) -> String {
    escape_html(value.get(key).and_then(Value::as_str).unwrap_or_default())
}

fn number(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map(|n| {
            if n.fract() == 0.0 {
                format_number(n as i64)
            } else {
                format!("{n:.2}")
            }
        })
        .unwrap_or_else(|| "—".into())
}

fn render_indeed(html: &mut String, data: &Value) {
    html.push_str("<section class=\"page-navy\" id=\"competitor-indeed\">");
    push_page_head(
        html,
        "採用市場",
        "Indeed採用レポート",
        "選択した職種・都道府県の月別データ",
    );
    if data["status"] != "ok" {
        html.push_str(&format!(
            "<p class=\"note\">{}</p></section>",
            text(data, "message")
        ));
        return;
    }
    html.push_str(&format!("<p>{} / {}</p><p class=\"note\">出典: {} / 集計日: {}。{} 求人を見た人数は応募数ではありません。欠測は「—」で表示します。</p>",text(data,"title"),text(data,"region"),text(data,"source"),text(data,"built_at"),text(data,"caveat")));
    html.push_str("<table class=\"table-navy\"><thead><tr><th>月</th><th>求人数</th><th>求人を見た人数</th><th>募集企業数</th><th>1求人あたりに見た人数</th></tr></thead><tbody>");
    for row in data["rows"].as_array().into_iter().flatten() {
        html.push_str(&format!("<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",text(row,"month"),number(row,"job"),number(row,"ctk"),number(row,"emp"),number(row,"spp")));
    }
    html.push_str("</tbody></table></section>");
}

fn render_google(html: &mut String, data: &Value) {
    html.push_str("<section class=\"page-navy\" id=\"competitor-google\">");
    push_page_head(
        html,
        "検索需要",
        "Google広告APIの検索需要",
        "検索ボリューム・月別推移・関連キーワード",
    );
    html.push_str("<p class=\"note\">出典: Google広告 Keyword Planner API。検索数はGoogleの推定検索需要です。Indeedの閲覧人数・CSVの求人数・応募数とは異なる指標です。広告競合度は求人の競合数ではありません。</p>");
    if data["status"] != "ok" {
        html.push_str(&format!("<p>{}</p></section>", text(data, "message")));
        return;
    }
    html.push_str(&format!(
        "<p>検索語: {} / 指定地域: {}</p>",
        text(data, "keyword"),
        if text(data, "region").is_empty() {
            "全国".into()
        } else {
            text(data, "region")
        }
    ));
    let demand = &data["demand"];
    if demand["status"] == "ok" {
        let region = &demand["region"];
        if !data["region"].as_str().unwrap_or_default().is_empty() && region.is_null() {
            html.push_str(
                "<p class=\"note\">指定地域を解決できなかったため全国の検索需要です。</p>",
            );
        } else if !region.is_null() {
            html.push_str(&format!(
                "<p class=\"note\">取得地域: {}</p>",
                text(region, "canonical_name")
            ));
        }
        html.push_str("<table class=\"table-navy\"><thead><tr><th>検索語</th><th>平均月間検索数</th><th>広告競合度</th></tr></thead><tbody>");
        let rows = demand["keywords"].as_array();
        if rows.is_none_or(Vec::is_empty) {
            html.push_str("<tr><td colspan=\"3\">検索需要のデータがありません。</td></tr>");
        }
        for row in rows.into_iter().flatten() {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td>{}</td></tr>",
                text(row, "keyword"),
                number(row, "avg_monthly"),
                text(row, "competition")
            ));
        }
        html.push_str("</tbody></table>");
        for row in rows.into_iter().flatten() {
            html.push_str(&format!("<div class=\"block-title\">{} の月別検索数</div><table class=\"table-navy\"><thead><tr><th>月</th><th>検索数</th></tr></thead><tbody>",text(row,"keyword")));
            for month in row["monthly_12m"].as_array().into_iter().flatten() {
                html.push_str(&format!(
                    "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                    text(month, "month"),
                    number(month, "search_volume")
                ));
            }
            html.push_str("</tbody></table>");
        }
    } else {
        // 外部APIの生エラーは資格情報を含む可能性があるので本文に転記しない。
        html.push_str("<p class=\"note\">Google検索需要を取得できませんでした。API設定または接続状況を確認してください。</p>");
    }
    html.push_str("<div class=\"block-title\">関連キーワード（検索需要順・上位20件）</div>");
    let suggestions = &data["suggestions"];
    if suggestions["status"] == "ok" {
        html.push_str("<table class=\"table-navy\"><thead><tr><th>関連語</th><th>平均月間検索数</th></tr></thead><tbody>");
        for row in suggestions["suggestions"].as_array().into_iter().flatten() {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                text(row, "keyword"),
                number(row, "avg_monthly")
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

    #[test]
    fn google_error_does_not_render_raw_credentials_or_invent_search_counts() {
        let mut html = String::new();
        render_google(
            &mut html,
            &json!({"status":"ok","demand":{"status":"error","message":"secret-token"},"suggestions":{"status":"missing_credentials"}}),
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
            &json!({"status":"ok","region":"<大阪府>","bands":[{"age_group":"20〜24歳","male_count":1234,"female_count":2345}],"minimum_wage":null,"labor":{"fiscal_year":2024,"unemployment_rate":2.5}}),
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
