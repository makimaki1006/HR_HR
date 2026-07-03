//! Section 04 - 採用市場 逼迫度 (Phase 2 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 4 / β Section Team / 2026-05-29) で抽出。
//!
//! 元 `navy_report/mod.rs` L1714-L2274 の以下を物理コピー:
//! - `TightnessData`                            (private struct)
//! - `extract_tightness`                        (private helper)
//! - `render_navy_section_04_market_tightness`  (公開 API)
//! - `build_navy_industry_tightness_table`      (private helper)
//! - `build_navy_tightness_gauges`              (private helper)
//! - `build_navy_tightness_table`               (private helper)
//! - `build_tightness_so_what`                  (private helper)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_04_market_tightness` (Commit 2/3 パターン踏襲:
//!   `pub(super)` は階層不足で E0364 になる)
//!
//! 内部 helper はすべて本ファイル内のみで使用される。`navy_report` モジュール
//! 外への露出はない。
//!
//! common 経由参照: `push_page_head` / `push_region_scope_banner` / `push_kpi` /
//! `fmt_ratio` / `fmt_pct` / `fmt_pct_from_ratio` / `severity_label` / `leak` は
//! `super::common::*` から直接 import。`build_navy_auto_table` は mod.rs 残置のため
//! `super::build_navy_auto_table` 経由。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_04_tightness):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::ReportVariant;
use super::common::{
    fmt_pct, fmt_pct_from_ratio, fmt_ratio, leak, push_kpi, push_page_head,
    push_region_scope_banner, severity_label,
};
// build_navy_auto_table は mod.rs に残置 (Section 02/03/05/06/07 で共有)。
// pub(super) 化されたため `super::build_navy_auto_table` で参照可能。
use super::build_navy_auto_table;

// ============================================================
// Section 04: 採用市場 逼迫度 (Phase 2 navy 本実装)
// ============================================================

struct TightnessData {
    job_ratio: Option<f64>,             // 有効求人倍率
    vacancy_rate: Option<f64>,          // HW 欠員補充率 (0-1)
    unemployment: Option<f64>,          // 失業率 (%)
    unemployment_national: Option<f64>, // 県平均失業率 (%) — 実体は fetch_prefecture_mean (WHERE prefecture=?) 由来。名称は歴史的経緯
    separation: Option<f64>,            // 離職率 (%)
    entry: Option<f64>,                 // 入職率 (%)
}

fn extract_tightness(ctx: &InsightContext) -> TightnessData {
    use super::super::super::super::helpers::{get_f64, get_str_ref};
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

pub(crate) fn render_navy_section_04_market_tightness(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    target_region: &str,
) {
    html.push_str("<section class=\"page-navy navy-tightness\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 04",
        "採用市場 逼迫度",
        "有効求人倍率 / 失業率 / 離職率 を統合した複合指標",
    );
    push_region_scope_banner(html, target_region);

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
            Some(v) if v >= 1.5 => (
                fmt_ratio(Some(v)),
                "warn",
                "1.5 以上は採用難度 高 (応募集めにくい)".to_string(),
            ),
            Some(v) if v >= 1.0 => (
                fmt_ratio(Some(v)),
                "neu",
                "1.0 以上は売り手市場".to_string(),
            ),
            Some(v) => (
                fmt_ratio(Some(v)),
                "pos",
                format!("1.0 未満 ({:.2}) は買い手市場", v),
            ),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "有効求人倍率", &val, "倍", dot, &foot, true);
    }
    if show_vacancy {
        let (val, dot, foot) = match d.and_then(|d| d.vacancy_rate) {
            Some(v) if v >= 0.25 => (
                fmt_pct_from_ratio(Some(v)),
                "warn",
                "25% 超は採用難度 高".to_string(),
            ),
            Some(v) if v >= 0.15 => (
                fmt_pct_from_ratio(Some(v)),
                "neu",
                "15-25% は標準的".to_string(),
            ),
            Some(v) => (
                fmt_pct_from_ratio(Some(v)),
                "pos",
                "15% 未満は採用充足".to_string(),
            ),
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
                let dot = if u < 2.5 { "warn" } else { "neu" };
                // |差| < 0.05pt は対象=県単位のため同値になる。無意味な「差 -0.0pt」を非表示
                let foot = if diff.abs() < 0.05 {
                    format!("県平均 {:.1}% (対象=県のため同値)", n)
                } else {
                    format!("県平均 {:.1}% / 差 {:+.1}pt", n, diff)
                };
                (format!("{:.1}", u), dot, foot)
            }
            (Some(u), None) => (format!("{:.1}", u), "neu", "県平均データなし".to_string()),
            _ => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "失業率", &val, "%", dot, &foot, false);
    }
    {
        let (val, dot, foot) = match d.and_then(|d| d.separation) {
            Some(v) if v >= 15.0 => (
                format!("{:.1}", v),
                "neu",
                "15% 超はやや高め (県 / 業界単位の値で対象地域固有ではない)".to_string(),
            ),
            Some(v) if v >= 10.0 => (
                format!("{:.1}", v),
                "neu",
                "10-15% は標準的水準".to_string(),
            ),
            Some(v) => (
                format!("{:.1}", v),
                "pos",
                "10% 未満は定着率 高".to_string(),
            ),
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
    html.push_str(
        "<div class=\"block-title block-title-spaced\">表 4-A &nbsp;採用市場 指標サマリ</div>\n",
    );
    html.push_str(&build_navy_tightness_table(d, show_vacancy));

    // -- 産業別 採用ニーズ密度 (国勢調査就業者数 + 求人媒体掲載数のクロス)
    // 媒体分析 / Market Intelligence variant でも hw_industry_counts は populate されるため
    // variant に依存せず ctx 由来データの有無で判定する。
    if let Some(ctx) = hw_context {
        // 2026-05-17: 表 4-B の silent skip を fallback 表示に変更 (#244 描画漏れ調査)
        //   旧: !ctx.ext_industry_employees.is_empty() && !ctx.hw_industry_counts.is_empty() のみ描画
        //   新: block-title は常時出し、データ欠損時は欠落データを明示
        html.push_str("<div class=\"block-title block-title-spaced\">表 4-B &nbsp;産業別 採用ニーズ密度 (件数最多 8 産業)</div>\n");
        if !ctx.ext_industry_employees.is_empty() && !ctx.hw_industry_counts.is_empty() {
            html.push_str(&build_navy_industry_tightness_table(ctx));
        } else {
            let missing = match (
                ctx.ext_industry_employees.is_empty(),
                ctx.hw_industry_counts.is_empty(),
            ) {
                (true, true) => "国勢調査 産業構造 + 求人媒体 産業集計",
                (true, false) => "国勢調査 産業構造 (v2_external_industry_structure)",
                (false, true) => "求人媒体 産業集計 (対象地域に分類済み求人なし)",
                _ => "",
            };
            html.push_str(&format!(
                "<table class=\"table-navy\"><tbody>\
                 <tr><td class=\"dim\" style=\"text-align:center;padding:8mm 4mm;\">\
                 産業別 採用ニーズ密度は <strong>{}</strong> が取得できなかったため算出されません。\
                 表 4-A の指標サマリ + 表 4-C/D で代替評価してください。\
                 </td></tr></tbody></table>\n",
                missing
            ));
        }

        // -- 表 4-C 事業所統計 (採用競合規模)  [旧 7.5-G 統合 2026-05-15]
        if !ctx.ext_establishments.is_empty() {
            html.push_str(
                "<div class=\"block-title block-title-spaced\">\
                 表 4-C &nbsp;事業所統計 (採用競合規模)\
                 </div>\n",
            );
            html.push_str(&build_navy_auto_table(&ctx.ext_establishments, 8));
            html.push_str(
                "<p class=\"caption\">\
                 出典: 公的統計 (e-Stat) 事業所データ。\
                 集計単位: 対象地域・全産業の事業所数および従業者数。\
                 </p>\n",
            );
        }

        // -- 表 4-D 開廃業動態 (市場成長性)  [旧 7.5-H 統合 2026-05-15]
        if !ctx.ext_business_dynamics.is_empty() {
            html.push_str(
                "<div class=\"block-title block-title-spaced\">\
                 表 4-D &nbsp;開廃業動態 (開業率・廃業率)\
                 </div>\n",
            );
            html.push_str(&build_navy_auto_table(&ctx.ext_business_dynamics, 6));
            use super::super::super::super::helpers::get_f64;
            let (open, close) = ctx
                .ext_business_dynamics
                .first()
                .map(|r| (get_f64(r, "opening_rate"), get_f64(r, "closure_rate")))
                .unwrap_or((f64::NAN, f64::NAN));
            let comment = if open.is_finite() && close.is_finite() {
                let net = open - close;
                format!(
                    "開業率 <strong>{:.1}%</strong> / 廃業率 <strong>{:.1}%</strong> \
                     (差 {:+.1}pt)。全国参考値は開業 5.0% / 廃業 4.0%。\
                     いずれも単年の値であり、市場フェーズ (成長・成熟・再編) の判定には\
                     複数年の推移確認が必要なため、本表では単年からの局面判定は行いません。\
                     基準年次は表本体の年次列を参照してください。",
                    open, close, net
                )
            } else {
                "開業率・廃業率のいずれかが取得できないため、市場動態のコメントは割愛します。"
                    .to_string()
            };
            html.push_str(&format!("<p class=\"caption\">{}</p>\n", comment));
        }
    }

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

// 産業別 採用ニーズ密度: 国勢調査就業者数 + 求人媒体掲載数 → 求人/就業者 比率
fn build_navy_industry_tightness_table(ctx: &InsightContext) -> String {
    use super::super::super::super::helpers::{get_f64, get_str};
    let industry_emp: Vec<(String, i64)> = ctx
        .ext_industry_employees
        .iter()
        .map(|r| {
            (
                get_str(r, "industry_name"),
                get_f64(r, "employees_total") as i64,
            )
        })
        .filter(|(n, c)| !n.is_empty() && *c > 0)
        .collect();
    let hw_map: std::collections::HashMap<&str, i64> = ctx
        .hw_industry_counts
        .iter()
        .map(|(n, c)| (n.as_str(), *c))
        .collect();

    // industry_name → (employees, hw_count, density per 10,000 employees)
    let mut rows: Vec<(String, i64, i64, f64)> = industry_emp
        .iter()
        .map(|(name, emp)| {
            let hw = hw_map.get(name.as_str()).copied().unwrap_or(0);
            let density = if *emp > 0 {
                hw as f64 * 10000.0 / *emp as f64
            } else {
                0.0
            };
            (name.clone(), *emp, hw, density)
        })
        .collect();
    // 求人密度降順 (Round 1-K 2026-06-03: tiebreaker で順序確定)
    rows.sort_by(|a, b| {
        b.3.partial_cmp(&a.3)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    rows.truncate(8);

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>産業大分類</th>");
    s.push_str("<th class=\"num\">就業者数</th>");
    s.push_str("<th class=\"num\">媒体掲載数</th>");
    s.push_str("<th class=\"num\">求人/就業者 1万人比</th>");
    s.push_str("<th>採用ニーズ密度</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    if rows.is_empty() {
        s.push_str(
            "<tr><td colspan=\"6\" class=\"dim\">産業別データを取得できませんでした。</td></tr>\n",
        );
    } else {
        // density の全産業平均 (上位 8 内)
        let avg_density: f64 = rows.iter().map(|r| r.3).sum::<f64>() / rows.len() as f64;
        for (i, (name, emp, hw, density)) in rows.iter().enumerate() {
            let (tag, label) = if *density >= avg_density * 1.5 {
                ("warn", "高密度 (求人/就業者比 高)")
            } else if *density >= avg_density * 0.8 {
                ("neu", "標準的")
            } else {
                ("neu", "低密度")
            };
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            s.push_str(&format!(
                "<tr{}>\
                 <td class=\"num bold\">{}</td>\
                 <td><strong>{}</strong></td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num bold\">{}</td>\
                 <td class=\"num bold\">{:.2}</td>\
                 <td><span class=\"tag tag-{}\">{}</span></td>\
                 </tr>\n",
                row_class,
                i + 1,
                escape_html(name),
                format_number(*emp),
                format_number(*hw),
                density,
                tag,
                label,
            ));
        }
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">求人/就業者 1万人比 = 媒体掲載数 × 10,000 / 就業者数。\
                平均比 +50% で「高密度」、平均比 ±20% 以内で「標準」と判定。\
                就業者数 (国勢調査) と媒体掲載数 (ローカル DB) を組み合わせた業界別需給代理指標。</p>\n");
    s
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
        let sev = if s >= 70.0 {
            "warn"
        } else if s >= 40.0 {
            "neu"
        } else {
            "pos"
        };
        items.push(("有効求人倍率", s, leak(&format!("{:.2} 倍", r)), sev));
    }
    if show_vacancy {
        if let Some(v) = d.vacancy_rate {
            let s = (v / 0.30).clamp(0.0, 1.0) * 100.0;
            let sev = if s >= 70.0 {
                "warn"
            } else if s >= 40.0 {
                "neu"
            } else {
                "pos"
            };
            items.push(("HW 欠員補充率", s, leak(&format!("{:.1}%", v * 100.0)), sev));
        }
    }
    if let Some(u) = d.unemployment {
        let s = ((6.0 - u) / 4.5).clamp(0.0, 1.0) * 100.0;
        let sev = if s >= 70.0 {
            "warn"
        } else if s >= 40.0 {
            "neu"
        } else {
            "pos"
        };
        items.push(("失業率 (低=採用難)", s, leak(&format!("{:.1}%", u)), sev));
    }
    if let Some(sep) = d.separation {
        let s = ((sep - 5.0) / 15.0).clamp(0.0, 1.0) * 100.0;
        let sev = if s >= 70.0 {
            "warn"
        } else if s >= 40.0 {
            "neu"
        } else {
            "pos"
        };
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
            bar_x,
            cy + 8.0,
            seg_x1 - bar_x
        ));
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"#FAEBD2\"/>\n",
            seg_x1,
            cy + 8.0,
            seg_x2 - seg_x1
        ));
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"12\" fill=\"#F4DDD7\"/>\n",
            seg_x2,
            cy + 8.0,
            bar_w - (seg_x2 - bar_x)
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
            marker_x - 1.5,
            cy + 4.0,
            marker_color
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
    // Round 1-K K-1: 失業率は % 値 (0-100 想定)。SQL 改修で二重×100 (380% 等) が
    // 混入した場合、< 2.5 判定外で「求職者は相対的に多い」等と誤判定するため、
    // 値域外を「データ異常」として中立扱いする。
    if let Some(u) = unemp {
        debug_assert!(
            u < 100.0,
            "unemployment_rate out of expected range (0-100%): {} (double-×100?)",
            u
        );
        if !(0.0..100.0).contains(&u) {
            tracing::warn!(
                target: "navy_report",
                rate = u,
                "unemployment_rate out of expected range (expected 0-100%); SQL unit change suspected"
            );
        }
    }
    let (val, tag, cmt) = match unemp {
        Some(u) if !(0.0..100.0).contains(&u) => ("—".to_string(), "neu", "データ異常"),
        Some(u) if u < 2.5 => (format!("{:.1}%", u), "warn", "低失業=採用難度 高"),
        Some(u) if u < 3.5 => (format!("{:.1}%", u), "neu", "標準的水準"),
        Some(u) => (
            format!("{:.1}%", u),
            "neu",
            "全国並み〜やや高め (求職者は相対的に多い)",
        ),
        None => ("—".to_string(), "neu", "—"),
    };
    let nat_str = nat
        .map(|n| format!("県平均 {:.1}%", n))
        .unwrap_or_else(|| "—".to_string());
    s.push_str(&row("失業率", val, &nat_str, tag, cmt));
    let (val, tag, cmt) = match d.and_then(|d| d.separation) {
        Some(v) if v >= 15.0 => (format!("{:.1}%", v), "neu", "やや高め (県/業界単位)"),
        Some(v) if v >= 10.0 => (format!("{:.1}%", v), "neu", "標準水準"),
        Some(v) => (format!("{:.1}%", v), "pos", "定着率 高"),
        None => ("—".to_string(), "neu", "—"),
    };
    s.push_str(&row("離職率", val, "全国 14.6%", tag, cmt));
    if let Some(d) = d {
        let (val, tag, cmt) = match d.entry {
            Some(v) if v >= 16.0 => (format!("{:.1}", v), "neu", "全国比で高い (要確認水準)"),
            Some(v) if v >= 10.0 => (format!("{:.1}", v), "neu", "標準水準"),
            Some(v) => (format!("{:.1}", v), "neu", "入職停滞"),
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
             離職率が高めの場合は (県/業界単位の値である点に留意しつつ) <strong>定着率向上施策</strong> の併走も検討余地があります。",
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
// テスト (テスト品質強化, 2026-06-05): データ妥当性 / 境界 / 不変条件
// 対象純粋関数: severity_label / build_tightness_so_what /
//              build_navy_tightness_table / build_navy_tightness_gauges
//
// 重点: MEMORY feedback_reverse_proof_tests.md (失業率 380% 流出) の逆証明。
//      失業率は % 値であり [0,100) 範囲外は「データ異常」として中立扱いされること。
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn make_data(
        job_ratio: Option<f64>,
        vacancy_rate: Option<f64>,
        unemployment: Option<f64>,
        separation: Option<f64>,
        entry: Option<f64>,
    ) -> TightnessData {
        TightnessData {
            job_ratio,
            vacancy_rate,
            unemployment,
            unemployment_national: Some(2.5),
            separation,
            entry,
        }
    }

    // --- severity_label (common 再エクスポート) ---------------------------

    // [不変条件] tag → ラベルの全分岐 + 未知タグの中立 fallback。
    #[test]
    fn severity_label_maps_all_known_tags() {
        assert_eq!(severity_label("pos"), "POS");
        assert_eq!(severity_label("warn"), "WARN");
        assert_eq!(severity_label("neg"), "NEG");
        assert_eq!(severity_label("neu"), "NEU");
        // 未知タグは NEU に倒れる (silent fallback ではなく明示的中立)
        assert_eq!(severity_label("unknown"), "NEU");
        assert_eq!(severity_label(""), "NEU");
    }

    // --- build_navy_tightness_table: 失業率の値域防御 (逆証明) --------------

    // [逆証明/ドメイン不変条件] 失業率の値域外 (0-100% 範囲外) は "データ異常" で中立化。
    //   MEMORY: unemployment 380% 流出事故の再発防止。値域外を pos/warn と誤判定しない。
    //   注: 本体に `debug_assert!(u < 100.0)` があり debug ビルド (= cargo test) では
    //       u>=100 で panic するため、ここでは負値 (-1.0 < 100.0 で assert は通過しつつ
    //       0.0..100.0 範囲外) を使って「データ異常」分岐を逆証明する。
    #[test]
    fn tightness_table_rejects_unemployment_out_of_range() {
        let d = make_data(None, None, Some(-1.0), None, None);
        let html = build_navy_tightness_table(Some(&d), false);
        assert!(
            html.contains("データ異常"),
            "out-of-range unemployment must be flagged as データ異常: {}",
            html
        );
        // 値域外を低失業 (warn) や高失業側 (neu) と誤判定していないこと
        assert!(
            !html.contains("低失業=採用難度 高"),
            "out-of-range must NOT be classified as 低失業: {}",
            html
        );
        assert!(
            !html.contains("求職者は相対的に多い"),
            "out-of-range must NOT be classified as 全国並み〜やや高め: {}",
            html
        );
    }

    // [境界] 失業率 0.0 ちょうどは範囲内 (下限 inclusive、< 2.5) → 低失業 warn。
    //   範囲判定 `(0.0..100.0).contains` の下限境界が inclusive であることを確認。
    #[test]
    fn tightness_table_unemployment_zero_is_in_range() {
        let d = make_data(None, None, Some(0.0), None, None);
        let html = build_navy_tightness_table(Some(&d), false);
        assert!(
            !html.contains("データ異常"),
            "unemployment=0.0 is in-range (lower inclusive), not anomaly: {}",
            html
        );
        assert!(
            html.contains("低失業=採用難度 高"),
            "0.0% (<2.5) should be 低失業 warn: {}",
            html
        );
    }

    // [境界] 失業率 2.5 未満は「低失業=採用難度 高」(warn)。閾値直下/直上を検証。
    #[test]
    fn tightness_table_unemployment_severity_boundaries() {
        // 2.4% (< 2.5) → 低失業 warn
        let low = make_data(None, None, Some(2.4), None, None);
        assert!(
            build_navy_tightness_table(Some(&low), false).contains("低失業=採用難度 高"),
            "2.4% should be 低失業 warn"
        );
        // 3.0% (2.5-3.5) → 標準的水準
        let mid = make_data(None, None, Some(3.0), None, None);
        assert!(
            build_navy_tightness_table(Some(&mid), false).contains("標準的水準"),
            "3.0% should be 標準的水準"
        );
        // 4.0% (>=3.5) → 全国並み〜やや高め (neu、強み断定はしない)
        let high = make_data(None, None, Some(4.0), None, None);
        assert!(
            build_navy_tightness_table(Some(&high), false).contains("求職者は相対的に多い"),
            "4.0% should be 全国並み〜やや高め"
        );
    }

    // [境界] 有効求人倍率 1.5 以上=warn / 1.0-1.5=売り手 / 1.0 未満=買い手。
    #[test]
    fn tightness_table_job_ratio_boundaries() {
        let h15 =
            build_navy_tightness_table(Some(&make_data(Some(1.5), None, None, None, None)), false);
        assert!(
            h15.contains("応募集めにくい"),
            "1.5 -> 応募集めにくい: {}",
            h15
        );
        let h12 =
            build_navy_tightness_table(Some(&make_data(Some(1.2), None, None, None, None)), false);
        assert!(h12.contains("売り手市場"), "1.2 -> 売り手市場: {}", h12);
        let h08 =
            build_navy_tightness_table(Some(&make_data(Some(0.8), None, None, None, None)), false);
        assert!(h08.contains("買い手市場"), "0.8 -> 買い手市場: {}", h08);
    }

    // [境界] None データ (全指標欠損) でも panic せず、"—" 行 + table を返す。
    #[test]
    fn tightness_table_none_data_no_panic() {
        let html = build_navy_tightness_table(None, false);
        assert!(html.contains("<table"), "table should render: {}", html);
        assert!(
            html.contains("有効求人倍率"),
            "label rows present: {}",
            html
        );
        assert!(html.contains("—"), "missing values shown as dash: {}", html);
    }

    // --- build_tightness_so_what: 警戒指標カウントによる総合判定 ------------

    // [境界] 警戒指標 2 つ以上 → 採用難度 高。
    #[test]
    fn so_what_two_alerts_is_high_difficulty() {
        // job_ratio>=1.5 と 低失業<2.5 の 2 指標警戒
        let d = make_data(Some(1.6), None, Some(2.0), None, None);
        let html = build_tightness_so_what(Some(&d), false);
        assert!(html.contains("採用難度 高"), "2 alerts -> 高: {}", html);
    }

    // [境界] 警戒指標 1 つ → 採用難度 中。
    #[test]
    fn so_what_one_alert_is_medium_difficulty() {
        // job_ratio>=1.5 のみ警戒 (失業率 4.0 は安全圏)
        let d = make_data(Some(1.6), None, Some(4.0), None, None);
        let html = build_tightness_so_what(Some(&d), false);
        assert!(html.contains("採用難度 中"), "1 alert -> 中: {}", html);
    }

    // [境界] 警戒指標 0 → 採用難度 低。
    #[test]
    fn so_what_zero_alerts_is_low_difficulty() {
        // 全指標が安全圏: job_ratio 0.8, 失業率 5.0, 離職率 8.0
        let d = make_data(Some(0.8), None, Some(5.0), Some(8.0), None);
        let html = build_tightness_so_what(Some(&d), false);
        assert!(html.contains("採用難度 低"), "0 alerts -> 低: {}", html);
    }

    // [境界] None データでは指標説明のみの文言を返し panic しない。
    #[test]
    fn so_what_none_data_returns_placeholder() {
        let html = build_tightness_so_what(None, false);
        assert!(
            html.contains("外部統計データが取得できなかった"),
            "none -> placeholder text: {}",
            html
        );
    }

    // --- build_navy_tightness_gauges: スコア正規化 (0-100 クランプ) ---------

    // [不変条件] ゲージスコアは 0-100 に正規化され、極端な入力でも SVG が壊れない。
    //   失業率 0% (非常に逼迫) でも有効求人倍率 10.0 (>2 で 100 clamp) でも描画される。
    #[test]
    fn gauges_render_with_extreme_values() {
        let d = make_data(Some(10.0), None, Some(0.0001), Some(50.0), None);
        let svg = build_navy_tightness_gauges(&d, false);
        assert!(svg.contains("<svg"), "svg should render: {}", svg);
        // marker_x 計算は score/100 なので clamp により NaN/極端座標を出さない。
        // 座標値に "NaN" や "inf" が含まれていないこと
        assert!(!svg.contains("NaN"), "no NaN coords: {}", svg);
        assert!(
            !svg.to_lowercase().contains("inf\""),
            "no inf coords: {}",
            svg
        );
    }

    // [境界] 全指標 None ではゲージ用データ不足の caption を返す (silent な空 SVG ではない)。
    #[test]
    fn gauges_empty_data_returns_caption() {
        let d = make_data(None, None, None, None, None);
        let html = build_navy_tightness_gauges(&d, false);
        assert!(
            html.contains("データが不足"),
            "empty gauges -> caption: {}",
            html
        );
        assert!(!html.contains("<svg"), "no svg when no data: {}", html);
    }
}
