//! Section 02 - 地域 × 求人媒体データ連携 (Full) / 地域データ補強 (MI/Public)
//!
//! navy_report.rs の分割 (A1 Commit 4 / β Section Team / 2026-05-29) で抽出。
//!
//! 元 `navy_report/mod.rs` L1259-L1712 の以下を物理コピー:
//! - `render_navy_section_02_region`         (公開 API)
//! - `build_navy_prefecture_salary_table`    (private helper)
//! - `build_navy_region_table`               (private helper)
//! - `build_region_so_what`                  (private helper)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_02_region` (Commit 2/3 パターン踏襲:
//!   `pub(super)` は階層不足で E0364 になる)
//!
//! 内部 helper (`build_navy_prefecture_salary_table` / `build_navy_region_table` /
//! `build_region_so_what`) は本ファイル内のみで使用される。`navy_report` モジュール
//! 外への露出はない。
//!
//! common 経由参照: `push_page_head` / `push_region_scope_banner` / `push_kpi` /
//! `format_mm` / `build_navy_auto_table` は `super::common::*` および
//! `super::build_navy_auto_table` (mod.rs に残置、`pub(super)` 化) から参照。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_02_region):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::super::ReportVariant;
use super::common::{
    format_mm, push_kpi, push_page_head, push_region_scope_banner, safe_pct, safe_pct_like,
};
// build_navy_auto_table は mod.rs に残置 (Section 03/05/06/07 で共有)。
// pub(super) 化されたため `super::build_navy_auto_table` で参照可能。
use super::build_navy_auto_table;

// ============================================================
// Section 02: 地域 × 求人媒体データ連携 (Full) / 地域データ補強 (MI/Public)
// ============================================================

pub(crate) fn render_navy_section_02_region(
    html: &mut String,
    agg: &SurveyAggregation,
    hw_context: Option<&InsightContext>,
    hw_enrichment_map: &std::collections::HashMap<
        String,
        super::super::super::hw_enrichment::HwAreaEnrichment,
    >,
    variant: ReportVariant,
    target_region: &str,
    // 2026-07-13: Ver10 専用。表2-E (都道府県別給与 — 地域比較) を表示するか。
    //   Ver10 以外の variant では参照されない (従来どおり常に表示)。
    table2e: bool,
) {
    let show_hw = matches!(variant, ReportVariant::Full);
    let is_ver10 = variant.is_ver10();
    let title = if show_hw {
        "地域 × 求人媒体データ連携"
    } else {
        "地域データ補強"
    };
    let sub = if show_hw {
        "CSV 件数最多 市区町村に求人媒体現在件数・推移を併記"
    } else {
        "CSV 件数最多 地域の公開統計指標を併記"
    };

    html.push_str("<section class=\"page-navy navy-region\" role=\"region\">\n");
    push_page_head(html, "SECTION 02", title, sub);
    push_region_scope_banner(html, target_region);

    let n_total = agg.total_count;
    let n_pref = agg.by_prefecture.len();
    let n_muni = agg.by_municipality_salary.len();

    // -- exec-headline
    // 2026-07-13: Ver10 は冒頭の説明ブロック (exec-headline) を削除する
    //   (現場レビュー: 前置きの説明文は読み飛ばされるため、表だけ見せる)。
    if !is_ver10 {
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
    }

    // -- 都道府県カバレッジ KPI
    html.push_str("<div class=\"block-title\">図 2-1 &nbsp;都道府県カバレッジ サマリ</div>\n");
    let pref_top = agg
        .by_prefecture
        .first()
        .map(|(p, c)| (p.clone(), *c))
        .unwrap_or_default();
    // Round 1-K (2026-06-03): safe_pct ガード - 0 除算 / NaN / Inf を 0.0 に丸める
    let pref_top_pct = if n_total > 0 {
        safe_pct(pref_top.1 as f64 / n_total as f64 * 100.0)
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
        if pref_top.0.is_empty() {
            "—"
        } else {
            &pref_top.0
        },
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

    // -- 表 2-B 地域基礎情報 (可住地面積・人口密度)  [旧 7.5-O 統合 2026-05-15]
    if let Some(c) = hw_context {
        if !c.ext_geography.is_empty() {
            html.push_str("<div class=\"block-title block-title-spaced\">表 2-B &nbsp;地域基礎情報 (可住地面積・人口密度)</div>\n");
            html.push_str(&build_navy_auto_table(&c.ext_geography, 5));
            html.push_str("<p class=\"caption\">出典: SSDSE-A 地理指標 (可住地面積 / 人口密度)。先頭 5 行表示。件数最多 10 市区町村の地理規模を把握するための土台情報。</p>\n");
        }
    }

    // -- table-navy: 件数最多 10 市区町村
    // 2026-07-13: Ver10 は表2-A を削除する (現場レビュー)。
    if !is_ver10 {
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 2-A &nbsp;件数最多 10 市区町村 &mdash; CSV 集計 + {}</div>\n",
            if show_hw { "求人媒体補強" } else { "外部統計" }
        ));
        html.push_str(&build_navy_region_table(agg, hw_enrichment_map, show_hw));
    }

    // -- 表 2-C 通勤流入元 TOP3 (採用範囲拡張の指針)  [旧 7.5-A 統合 2026-05-15]
    if let Some(c) = hw_context {
        if !c.commute_inflow_top3.is_empty() {
            html.push_str("<div class=\"block-title block-title-spaced\">表 2-C &nbsp;通勤流入元 TOP3 (隣地域→対象地域)</div>\n");
            html.push_str(
                "<table class=\"table-navy\">\n<thead><tr>\
                <th>順位</th><th>都道府県</th><th>市区町村</th><th class=\"num\">流入人数</th>\
                </tr></thead>\n<tbody>\n",
            );
            for (i, (p, m, cnt)) in c.commute_inflow_top3.iter().take(3).enumerate() {
                html.push_str(&format!(
                    "<tr><td class=\"num bold\">{}</td><td>{}</td><td>{}</td><td class=\"num bold\">{}</td></tr>\n",
                    i + 1, escape_html(p), escape_html(m), format_number(*cnt)
                ));
            }
            html.push_str("</tbody></table>\n");
            html.push_str("<p class=\"caption\">出典: 国勢調査 通勤 OD。対象地域以外 (隣接市町村 / 隣接都道府県含む) からの通勤者流入元 上位 3 自治体。採用範囲拡張・近隣自治体への媒体出稿の指針。</p>\n");
        }
    }

    // -- 表 2-D 都道府県平均比較 (マクロ指標)  [旧 7.5-B 統合 2026-05-15]
    if let Some(c) = hw_context {
        let pref_avgs: Vec<(&str, Option<f64>, &str)> = vec![
            ("県平均 失業率", c.pref_avg_unemployment_rate, "%"),
            ("県平均 単身世帯率", c.pref_avg_single_rate, "%"),
        ];
        let with_val: Vec<_> = pref_avgs.iter().filter(|(_, v, _)| v.is_some()).collect();
        if !with_val.is_empty() {
            html.push_str("<div class=\"block-title block-title-spaced\">表 2-D &nbsp;都道府県平均比較 (マクロ指標)</div>\n");
            html.push_str("<table class=\"table-navy\">\n<thead><tr><th>指標</th><th class=\"num\">値</th><th>単位</th></tr></thead>\n<tbody>\n");
            for (label, val, unit) in with_val {
                let cell = val
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "—".into());
                html.push_str(&format!(
                    "<tr><td><strong>{}</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">{}</span></td></tr>\n",
                    label, cell, unit
                ));
            }
            html.push_str("</tbody></table>\n");
            html.push_str("<p class=\"caption\">出典: SSDSE-A 都道府県集計 (SUM 方式: 市町村集計を県全体で再集計)。対象地域固有の値ではなく県全体の平均値。Section 04 の失業率と併せて読む。</p>\n");
        }
    }

    // -- 表 2-E 都道府県別給与 + 地域比較 (2026-05-23 #226 統合)
    //   件数最多 10 県の平均給与と CSV 内全体加重平均との差分を可視化。
    //   既存の「件数最多 10 市区町村」と階層を変えた粒度 (県単位) で
    //   給与水準のリージョン格差を補強する。
    // 2026-07-13: Ver10 は表2-E を URL パラメータ table2e で表示制御する
    //   (既定オン)。Ver10 以外の variant では table2e=true 相当で常に表示 (byte 不変)。
    let show_table_2e = !is_ver10 || table2e;
    if show_table_2e && !agg.by_prefecture_salary.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 2-E &nbsp;都道府県別給与 &mdash; 地域比較</div>\n");
        html.push_str(&build_navy_prefecture_salary_table(agg, agg.is_hourly));
    }

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

// 2026-05-23 #226: 都道府県別給与 + 地域比較 (Section 02 拡張: market_intelligence 系の知見を navy 取り込み)
//
// 設計:
// - `by_prefecture_salary` は aggregator が PrefectureSalaryAgg 単位で集計済み
//   (name / count / avg_salary / avg_min_salary)。
// - 件数最多の県を「対象県」とし、それ以外を「他県平均」として比較する。
// - 当該県 vs 全国 (CSV 内全件平均) / 隣接県群 (件数 2 位以下の平均) の中央給与比較を行う。
//   ※「全国」と言っても本レポートはアップロード CSV 内の県平均であり、47 県全体ではない。
//     その範囲制約を caption に明示 (MEMORY: feedback_hw_data_scope.md)。
// - 件数加重平均で算出 (件数の少ない県が同等に扱われないように)。
// - 月給/時給の単位は is_hourly フラグで切り替える。
fn build_navy_prefecture_salary_table(agg: &SurveyAggregation, is_hourly: bool) -> String {
    let total_rows: Vec<&super::super::super::aggregator::PrefectureSalaryAgg> =
        agg.by_prefecture_salary.iter().collect();
    if total_rows.is_empty() {
        return "<p class=\"caption dim\">CSV から都道府県別給与を抽出できませんでした。</p>\n"
            .to_string();
    }

    // 件数加重 全体平均 (CSV 内)
    let total_n: i64 = total_rows.iter().map(|p| p.count as i64).sum();
    let weighted_sum: i64 = total_rows
        .iter()
        .map(|p| p.avg_salary * p.count as i64)
        .sum();
    let overall_avg: i64 = if total_n > 0 {
        weighted_sum / total_n
    } else {
        0
    };

    // 件数降順 (Round 1-K 2026-06-03: 同件数時は name asc で順序確定)
    let mut sorted: Vec<&super::super::super::aggregator::PrefectureSalaryAgg> = total_rows.clone();
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));

    let unit_label = if is_hourly { "円/時" } else { "万円" };
    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>都道府県</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str(&format!("<th class=\"num\">平均給与 ({})</th>", unit_label));
    s.push_str(&format!("<th class=\"num\">全体差分 ({})</th>", unit_label));
    s.push_str("<th class=\"num\">差分率</th>");
    s.push_str("<th>位置づけ</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    // 先頭 10 県表示
    for (i, p) in sorted.iter().take(10).enumerate() {
        let diff = p.avg_salary - overall_avg;
        // Round 1-K (2026-06-03): safe_pct_like ガード - 差分 % は負数あり得るので clamp なし版
        let diff_pct = if overall_avg > 0 {
            safe_pct_like(diff as f64 / overall_avg as f64 * 100.0)
        } else {
            0.0
        };
        let (tag, label) = if diff_pct >= 5.0 {
            ("pos", "高水準")
        } else if diff_pct <= -5.0 {
            ("warn", "低水準")
        } else {
            ("neu", "中央付近")
        };
        let diff_sign = if diff >= 0 { "+" } else { "" };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{}{}</td>\
             <td class=\"num bold\">{:+.1}%</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&p.name),
            format_number(p.count as i64),
            fmt_val(p.avg_salary),
            diff_sign,
            fmt_val(diff.abs()),
            diff_pct,
            tag,
            label,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">基準: CSV 内 件数加重 全体平均給与 <strong>{} {}</strong>。\
         <strong>「全体」はアップロード CSV 内の県群を対象とした集計</strong>であり、\
         47 都道府県全体や公的統計の全国平均ではありません。\
         他媒体・公的統計との比較を行う場合は、Section 07 最低賃金表で別途確認してください。</p>\n",
        fmt_val(overall_avg),
        unit_label
    ));

    s
}

fn build_navy_region_table(
    agg: &SurveyAggregation,
    hw_enrichment_map: &std::collections::HashMap<
        String,
        super::super::super::hw_enrichment::HwAreaEnrichment,
    >,
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
    let top10: Vec<&super::super::super::aggregator::MunicipalitySalaryAgg> =
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
                // Round 1-K (2026-06-03): safe_pct ガード
                let pct = if agg.total_count > 0 {
                    safe_pct(row.count as f64 / agg.total_count as f64 * 100.0)
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
        // 2026-07-22 数値監査対応: この内訳は給与を確認できた求人ベース (総件数とは分母が異なる)
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数 (給与を確認できた求人ベースのため、表紙の総件数とは分母が異なる)。中央値: 月給換算済み。媒体掲載数: 求人媒体ローカル DB の現在掲載求人数。推移: 3 ヶ月前比 / 1 年前比 (Turso 時系列)。</p>\n");
    } else {
        // 2026-07-22 数値監査対応: この内訳は給与を確認できた求人ベース (総件数とは分母が異なる)
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数 (給与を確認できた求人ベースのため、表紙の総件数とは分母が異なる)。中央値: 月給換算済み。位置づけ: n に占める割合に基づき中核 (30%+) / 主要 (10-30%) / 周辺 (-10%) に分類。</p>\n");
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
    // Round 1-K (2026-06-03): safe_pct ガード
    let muni_top_pct = match muni_top {
        Some(m) if agg.total_count > 0 => safe_pct(m.count as f64 / agg.total_count as f64 * 100.0),
        _ => 0.0,
    };

    let geo_judge = if n_pref == 1 {
        "<strong>単一県構成</strong>"
    } else if pref_top_pct >= 70.0 {
        "<strong>1 県主導 (他県補助)</strong>"
    } else if n_pref >= 5 {
        "<strong>広域分散</strong>"
    } else {
        "<strong>複数県均衡</strong>"
    };

    let concentration_note = if muni_top_pct >= 50.0 {
        format!(
            "件数最多市区町村 <strong>{}</strong> が <strong>{:.0}%</strong> を占める<strong>1 自治体主導</strong>の構成です。",
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
