//! 分割: report_html/hw_enrichment.rs (物理移動・内容変更なし)

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

/// CSV 住所（prefecture × municipality）× HW DB 連携セクション
///
/// 表示項目: 都道府県 / 市区町村 / HW現在件数 / 3ヶ月推移 / 1年推移 / 欠員率（外部統計）
///
/// # データ源
/// - HW 現在件数: `hw_enrichment::enrich_areas` 相当を `hw_context` から導出
///   （postings ローカル集計は handlers 層で行う前提だが、現行シグネチャでは
///   InsightContext のみが入力のため、`ctx.vacancy` の `total_count` から近似）
/// - 3ヶ月/1年推移: `ctx.ts_counts` の snapshot_id 時系列から集計
/// - 欠員率: `ctx.vacancy` の emp_group = 正社員 の `vacancy_rate`（外部統計連携値）
/// 市区町村粒度 HW enrichment map を受け取るバージョン
///
/// handlers.rs で `hw_enrichment::enrich_areas` を呼び、(pref,muni) ごとの
/// HW 現在件数 / 推移 / 欠員率 を渡す。都道府県粒度コピーのバグを解消。
pub(super) fn render_section_hw_enrichment(
    html: &mut String,
    agg: &SurveyAggregation,
    ctx: &InsightContext,
    enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
) {
    let pairs: Vec<(String, String, usize)> = {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut v: Vec<(String, String, usize)> = Vec::new();
        for m in &agg.by_municipality_salary {
            let key = (m.prefecture.clone(), m.name.clone());
            if !m.prefecture.is_empty() && !m.name.is_empty() && seen.insert(key) {
                v.push((m.prefecture.clone(), m.name.clone(), m.count));
            }
        }
        v.sort_by(|a, b| b.2.cmp(&a.2));
        v
    };

    if pairs.is_empty() && agg.by_prefecture.is_empty() {
        return;
    }

    // フォールバック: map が空または map に無いエントリ用に ctx からの単一値を用意
    // 2026-04-26 監査 Q1.3: vacancy_rate (DB) は 0-1 比率で保存されているため、
    //   VacancyRatePct::from_ratio() で % 単位に明示変換する。
    let (fallback_3m, fallback_1y) = compute_posting_change_from_ts(ctx);
    let fallback_vacancy: Option<crate::handlers::types::VacancyRatePct> = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .filter(|v| *v > 0.0)
        .map(crate::handlers::types::VacancyRatePct::from_ratio);

    let rows: Vec<(HwAreaEnrichment, usize)> = pairs
        .iter()
        .take(15)
        .map(|(pref, muni, count)| {
            let key = format!("{}:{}", pref, muni);
            let enrich = enrichment_map
                .get(&key)
                .cloned()
                .unwrap_or_else(|| HwAreaEnrichment {
                    prefecture: pref.clone(),
                    municipality: muni.clone(),
                    hw_posting_count: 0,
                    posting_change_3m_pct: fallback_3m,
                    posting_change_1y_pct: fallback_1y,
                    vacancy_rate_pct: fallback_vacancy,
                });
            (enrich, *count)
        })
        .collect();

    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"hw-enrich-title\">\n",
    );
    html.push_str("<h2 id=\"hw-enrich-title\">第3章 地域 × HW データ連携</h2>\n");
    html.push_str(
        "<p class=\"section-header-meta\">\
         アップロード CSV の地域情報を HW postings の市区町村実件数と突合。</p>\n",
    );
    // amber バナー: 市区町村粒度の制約を明示 (UI-3)
    html.push_str(
        "<div class=\"report-banner-amber\" role=\"note\">\
         \u{26A0}\u{FE0F} <strong>データ粒度の制約</strong>: \
         3ヶ月推移・1年推移・欠員補充率は <code>ts_turso_counts</code> / 外部統計の都道府県粒度のみ取得可能で、\
         市区町村単位の差は反映されません。\
         本表では市区町村単位で取得できる「CSV 件数」「HW 現在件数」のみを表示しています。</div>\n",
    );
    // 図表番号 (図 3-1)
    html.push_str(&render_figure_number(3, 1, "CSV-HW 求人件数 概念対応図"));
    // CSV / HW の重なり Venn 概念図（数値ではなくスコープの説明）
    html.push_str(
        "<div class=\"report-venn\" aria-label=\"CSV と HW のスコープ概念図\">\
         <div class=\"report-venn-circle report-venn-csv\">\
           <span class=\"report-venn-label\">CSV</span>\
           <span class=\"report-venn-count\">媒体掲載</span>\
           <span style=\"font-size:8.5pt;\">アップロード CSV 由来</span>\
         </div>\
         <div class=\"report-venn-circle report-venn-both\">\
           <span class=\"report-venn-label\">重複領域</span>\
           <span style=\"font-size:9pt;\">同一企業の同一案件が両方に掲載</span>\
         </div>\
         <div class=\"report-venn-circle report-venn-hw\">\
           <span class=\"report-venn-label\">HW</span>\
           <span class=\"report-venn-count\">ハローワーク掲載</span>\
           <span style=\"font-size:8.5pt;\">公的職業紹介</span>\
         </div>\
         </div>\n",
    );
    html.push_str(&render_reading_callout(
        "CSV と HW は元々スコープが異なります（媒体掲載範囲 vs ハローワーク掲載求人）。\
         同一案件が両方に掲載される「重複領域」も存在しますが、本レポートでは件数の多少のみを参考値として比較しています。",
    ));
    // 2026-04-24: build_hw_enrichment_sowhat は ts_turso_counts の初期ノイズで
    //   「+374.3%」など暴れやすく誤誘導になるため非表示化。欠員率（外部統計）
    //   のみ意味があるケースで別途言及する運用にする。
    let _ = (fallback_3m, fallback_1y);
    if let Some(vrate) = fallback_vacancy {
        html.push_str(
            "<div class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">",
        );
        // 用語ツールチップ: 欠員補充率 (UI-3)
        let term_html = render_info_tooltip(
            "欠員補充率",
            "求人理由が「欠員補充」（離職・退職に伴う補充）の比率。e-Stat 雇用動向調査由来の都道府県単位値で、新規拡大採用は含まない。",
        );
        html.push_str(&format!(
            "※ {} 正社員 {} は {:.1}%。\
             この値は都道府県粒度の単一値であり、市区町村別の差は反映していません。",
            escape_html(
                &rows
                    .first()
                    .map(|(e, _)| e.prefecture.clone())
                    .unwrap_or_default()
            ),
            term_html,
            vrate.as_f64()
        ));
        html.push_str("</div>\n");
    }
    // 表番号 (表 3-1) — 「上位」は禁止ワードのため、別表現を採用
    html.push_str(&render_table_number(
        3,
        1,
        "市区町村別 CSV-HW 求人件数 対応表（CSV件数の多い 15 地域）",
    ));
    html.push_str("<table class=\"hw-enrichment-table report-zebra\">\n");
    html.push_str(
        "<thead><tr>\
         <th>都道府県</th>\
         <th>市区町村</th>\
         <th class=\"num\">CSV件数</th>\
         <th class=\"num\">HW現在件数</th>\
         </tr></thead><tbody>\n",
    );
    // 2026-04-24 ユーザー指摘:
    //   旧実装は 3ヶ月推移/1年推移/欠員率 を各行に出したが、これらは都道府県
    //   粒度の単一値を全行に同じ値で表示していたため誤誘導だった。
    //   また ts_turso_counts 由来の変動率は初期スナップショットのノイズで
    //   「+374.3%」など現実離れした値が出やすく、実用性が低い。
    //   → テーブルからは市区町村粒度で確実に取れる CSV 件数 / HW 件数 のみに
    //      絞り、推移・欠員率は「注記」として都道府県代表値で別記する。
    for (e, csv_count) in &rows {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", escape_html(&e.prefecture)));
        html.push_str(&format!("<td>{}</td>", escape_html(&e.municipality)));
        html.push_str(&format!("<td class=\"num\">{}</td>", csv_count));
        html.push_str(&format!(
            "<td class=\"num\">{}</td>",
            if e.hw_posting_count > 0 {
                format!("{}", e.hw_posting_count)
            } else {
                "—".to_string()
            }
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p class=\"print-note\">\
         ※ 表示は「CSV 件数（アップロード行数）」と「HW 現在件数（HW postings の市区町村実件数）」の 2 軸。\
         CSV 件数は対象媒体の掲載範囲に依存し、HW 件数はハローワーク側の掲載求人のみ。\
         単純比較ではなく、どのエリアに媒体側の露出が集中しているかの参考値として参照してください。</p>\n",
    );
    html.push_str("</section>\n");
}

/// ts_counts から posting_count 合計の 3m / 1y 変化率 (%) を算出
/// 戻り値: (change_3m_pct, change_1y_pct)
///
/// D-2 監査 Q1.2 対応:
/// - スナップショット数不足は None
/// - |値| > 200% / NaN / Inf は ETL 初期ノイズとして None
///   （`hw_enrichment::sanitize_change_pct` と同一ロジック）
pub(super) fn compute_posting_change_from_ts(ctx: &InsightContext) -> (Option<f64>, Option<f64>) {
    if ctx.ts_counts.is_empty() {
        return (None, None);
    }
    // snapshot_id → posting_count 合計 を集計
    use std::collections::BTreeMap;
    let mut by_snap: BTreeMap<String, f64> = BTreeMap::new();
    for r in &ctx.ts_counts {
        let snap = get_str_ref(r, "snapshot_id").to_string();
        if snap.is_empty() {
            continue;
        }
        let cnt = get_f64(r, "posting_count");
        *by_snap.entry(snap).or_insert(0.0) += cnt;
    }
    if by_snap.is_empty() {
        return (None, None);
    }
    // 昇順 → 末尾が最新
    let mut entries: Vec<(String, f64)> = by_snap.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let n = entries.len();
    let latest = entries[n - 1].1;
    if latest <= 0.0 {
        return (None, None);
    }
    // 3m 前 = -3, 1y 前 = -12（月次 snapshot 前提）
    let change_3m = if n >= 4 {
        let prev = entries[n - 4].1;
        if prev > 0.0 {
            Some((latest - prev) / prev * 100.0)
        } else {
            None
        }
    } else {
        None
    };
    let change_1y = if n >= 13 {
        let prev = entries[n - 13].1;
        if prev > 0.0 {
            Some((latest - prev) / prev * 100.0)
        } else {
            None
        }
    } else {
        None
    };
    // 暴走値除去（fetch_pref_posting_changes と同一ポリシー）
    // |値| > 200% / NaN / Inf は ETL 初期ノイズとして None
    use super::super::hw_enrichment::sanitize_change_pct;
    (
        sanitize_change_pct(change_3m),
        sanitize_change_pct(change_1y),
    )
}

/// 比較カード: 媒体（CSV）の値とHW全体の値を並列表示し、差分を算出
///
/// - `label`: 指標名
/// - `csv_value`: CSVから算出した値（整形済み文字列）
/// - `hw_value`: HWから算出した値（整形済み文字列）
/// - `diff_text`: 差分表示（正負込みのフォーマット済み文字列、Noneなら非表示）
/// - `positive`: 差分が「媒体が上回る（良い方向）」かどうか
pub(super) fn render_comparison_card(
    html: &mut String,
    label: &str,
    csv_value: &str,
    hw_value: &str,
    diff_text: Option<&str>,
    positive: bool,
) {
    html.push_str("<div class=\"comparison-card\">\n");
    html.push_str(&format!("<h3>{}</h3>\n", escape_html(label)));
    html.push_str("<div class=\"value-pair\">\n");
    html.push_str(&format!(
        "<div><span class=\"label\">媒体</span><span class=\"value\">{}</span></div>\n",
        escape_html(csv_value)
    ));
    html.push_str(&format!(
        "<div><span class=\"label\">HW</span><span class=\"value\">{}</span></div>\n",
        escape_html(hw_value)
    ));
    html.push_str("</div>\n");
    if let Some(d) = diff_text {
        let cls = if positive { "positive" } else { "negative" };
        html.push_str(&format!(
            "<div class=\"diff {}\">{}</div>\n",
            cls,
            escape_html(d)
        ));
    }
    html.push_str("</div>\n");
}

pub(super) fn render_section_hw_comparison(
    html: &mut String,
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
    ctx: &InsightContext,
) {
    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>HW市場比較</h2>\n");
    html.push_str(
        "<p class=\"guide\" style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>CSV（媒体データ）とハローワーク全体データを\
        <strong>雇用形態ごと</strong>に並列比較。媒体に出現する雇用形態を動的に検出し、\
        対応するHWデータと同条件で比較します。\
    </p>\n",
    );

    // CSV側の雇用形態を正規化してHW側のemp_group（正社員/パート/その他）にマッピング
    // - "正社員"/"正職員" → HW "正社員"
    // - "パート"/"アルバイト" → HW "パート"
    // - "契約社員"/"派遣社員"/その他 → HW "その他"
    fn normalize_emp_type(csv_type: &str) -> Option<&'static str> {
        if csv_type.contains("正社員") || csv_type.contains("正職員") {
            Some("正社員")
        } else if csv_type.contains("パート") || csv_type.contains("アルバイト") {
            Some("パート")
        } else if csv_type.contains("契約")
            || csv_type.contains("派遣")
            || csv_type.contains("嘱託")
            || csv_type.contains("臨時")
            || csv_type.contains("その他")
        {
            Some("その他")
        } else {
            None
        }
    }

    // CSV側の雇用形態を集約（HWのemp_groupごとに合算）
    use std::collections::HashMap;
    let mut csv_by_hw_group: HashMap<&str, (usize, i64)> = HashMap::new(); // (件数, 給与合計)
    for e in by_emp_type_salary {
        if let Some(hw_key) = normalize_emp_type(&e.emp_type) {
            let entry = csv_by_hw_group.entry(hw_key).or_insert((0, 0));
            entry.0 += e.count;
            entry.1 += e.avg_salary * e.count as i64;
        }
    }

    // 対象とする雇用形態を出現順に並べる（正社員 > パート > その他）
    let emp_order = ["正社員", "パート", "その他"];
    let present_groups: Vec<&str> = emp_order
        .iter()
        .filter(|g| csv_by_hw_group.contains_key(*g))
        .copied()
        .collect();

    if present_groups.is_empty() {
        html.push_str(
            "<p style=\"color:#888;font-size:10pt;\">\
            CSVデータの雇用形態が判別できなかったため、HW比較をスキップしました。</p>\n",
        );
        html.push_str("</div>\n");
        return;
    }

    // --- 雇用形態別 平均月給比較（動的生成） ---
    if !agg.is_hourly {
        html.push_str("<h3 style=\"font-size:11pt;margin:8px 0;\">雇用形態別 平均月給比較</h3>\n");
        html.push_str("<div class=\"comparison-grid\">\n");
        for &group in &present_groups {
            let (count, salary_sum) = csv_by_hw_group[group];
            let csv_avg = if count > 0 {
                salary_sum / count as i64
            } else {
                0
            };
            let csv_display = if csv_avg > 0 {
                format!("{:.1}万円 (n={})", csv_avg as f64 / 10_000.0, count)
            } else {
                format!("- (n={})", count)
            };

            // HW側: cascade は industry_raw × emp_group の複合集計のため、
            // 同じ emp_group の全業種の avg_salary_min を平均化する
            let salaries: Vec<f64> = ctx
                .cascade
                .iter()
                .filter(|r| super::super::super::helpers::get_str_ref(r, "emp_group") == group)
                .map(|r| get_f64(r, "avg_salary_min"))
                .filter(|&v| v > 0.0)
                .collect();
            let hw_avg: i64 = if !salaries.is_empty() {
                (salaries.iter().sum::<f64>() / salaries.len() as f64) as i64
            } else {
                0
            };
            let hw_display = if hw_avg > 0 {
                format!("{:.1}万円", hw_avg as f64 / 10_000.0)
            } else {
                "データなし".to_string()
            };

            let (diff_text, positive) = if csv_avg > 0 && hw_avg > 0 {
                let diff = csv_avg - hw_avg;
                let pct = diff as f64 / hw_avg as f64 * 100.0;
                (
                    Some(format!("{:+.1}万円 ({:+.1}%)", diff as f64 / 10_000.0, pct)),
                    diff >= 0,
                )
            } else {
                (None, true)
            };
            render_comparison_card(
                html,
                &format!("{} 平均月給", group),
                &csv_display,
                &hw_display,
                diff_text.as_deref(),
                positive,
            );
        }
        html.push_str("</div>\n");
    }

    // --- 雇用形態構成比（媒体 vs HW） ---
    html.push_str("<h3 style=\"font-size:11pt;margin:16px 0 8px;\">雇用形態構成比</h3>\n");
    html.push_str("<div class=\"comparison-grid\">\n");

    // 雇用形態構成比（CSV vs HW の割合）を雇用形態ごとに表示
    let csv_total: usize = csv_by_hw_group.values().map(|(c, _)| c).sum();
    let hw_total: i64 = ctx
        .vacancy
        .iter()
        .map(|r| get_f64(r, "total_count") as i64)
        .sum();

    for &group in &present_groups {
        let (csv_count, _) = csv_by_hw_group[group];
        let csv_rate = if csv_total > 0 {
            csv_count as f64 / csv_total as f64 * 100.0
        } else {
            -1.0
        };
        let csv_display = if csv_rate >= 0.0 {
            format!("{:.1}% ({}件)", csv_rate, csv_count)
        } else {
            "-".to_string()
        };

        let hw_count: i64 = ctx
            .vacancy
            .iter()
            .find(|r| super::super::super::helpers::get_str_ref(r, "emp_group") == group)
            .map(|r| get_f64(r, "total_count") as i64)
            .unwrap_or(0);
        let hw_rate = if hw_total > 0 {
            hw_count as f64 / hw_total as f64 * 100.0
        } else {
            -1.0
        };
        let hw_display = if hw_rate >= 0.0 && hw_total > 0 {
            format!("{:.1}% ({}件)", hw_rate, format_number(hw_count))
        } else {
            "データなし".to_string()
        };

        let (diff_text, positive) = if csv_rate >= 0.0 && hw_rate >= 0.0 {
            let d = csv_rate - hw_rate;
            (Some(format!("{:+.1}pt", d)), d >= 0.0)
        } else {
            (None, true)
        };
        render_comparison_card(
            html,
            &format!("{} 構成比", group),
            &csv_display,
            &hw_display,
            diff_text.as_deref(),
            positive,
        );
    }
    html.push_str("</div>\n"); // comparison-grid (構成比)

    // --- 地域人口/最低賃金の比較カード（従来通り、正社員雇用前提でない） ---
    html.push_str("<h3 style=\"font-size:11pt;margin:16px 0 8px;\">地域指標</h3>\n");
    html.push_str("<div class=\"comparison-grid\">\n");

    // --- カード3: 対象地域の人口（通勤圏優先、なければ市区町村／都道府県人口） ---
    let population: i64 = if ctx.commute_zone_total_pop > 0 {
        ctx.commute_zone_total_pop
    } else {
        ctx.ext_population
            .first()
            .map(|r| get_f64(r, "total_population") as i64)
            .unwrap_or(0)
    };
    let pop_source = if ctx.commute_zone_total_pop > 0 {
        format!("通勤圏内 {}自治体", ctx.commute_zone_count)
    } else if !ctx.muni.is_empty() {
        ctx.muni.clone()
    } else {
        ctx.pref.clone()
    };
    let pop_display = if population > 0 {
        format!("{}人", format_number(population))
    } else {
        "-".to_string()
    };
    render_comparison_card(
        html,
        "対象地域の人口",
        &pop_display,
        &pop_source,
        None,
        true,
    );

    // --- カード4: 最低賃金比較（CSV平均下限の160h換算 vs 都道府県最低賃金） ---
    if !agg.is_hourly {
        // CSV平均下限（月給→時給160h換算）
        let csv_avg_min: i64 = if !agg.by_prefecture_salary.is_empty() {
            let total: i64 = agg
                .by_prefecture_salary
                .iter()
                .filter(|p| p.avg_min_salary > 0)
                .map(|p| p.avg_min_salary)
                .sum();
            let n = agg
                .by_prefecture_salary
                .iter()
                .filter(|p| p.avg_min_salary > 0)
                .count();
            if n > 0 {
                total / n as i64
            } else {
                0
            }
        } else {
            0
        };
        let csv_hourly = csv_avg_min / super::super::aggregator::HOURLY_TO_MONTHLY_HOURS;
        let csv_display = if csv_hourly > 0 {
            format!("{}円/h", format_number(csv_hourly))
        } else {
            "-".to_string()
        };

        // 都道府県最低賃金（ctx.prefから取得）
        let mw = min_wage_for_prefecture(&ctx.pref).unwrap_or(0);
        let mw_display = if mw > 0 {
            format!("{}円/h", format_number(mw))
        } else {
            "-".to_string()
        };

        let (mw_diff_text, mw_positive) = if csv_hourly > 0 && mw > 0 {
            let d = csv_hourly - mw;
            let pct = d as f64 / mw as f64 * 100.0;
            (Some(format!("{:+}円 ({:+.1}%)", d, pct)), d >= 0)
        } else {
            (None, true)
        };
        render_comparison_card(
            html,
            "最低賃金比較（167h換算）",
            &csv_display,
            &mw_display,
            mw_diff_text.as_deref(),
            mw_positive,
        );
    }

    html.push_str("</div>\n"); // comparison-grid
    html.push_str("<div class=\"note\" style=\"font-size:9pt;color:#555;margin-top:8px;\">\
        ※HW側データは「ハローワーク掲載求人のみ」が対象であり、全求人市場を反映するものではありません。\
        媒体（CSV）との差異は、掲載媒体の選定バイアスによる可能性があります。\
    </div>\n");
    html.push_str("</div>\n");
}
