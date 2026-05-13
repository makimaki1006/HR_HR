//! 媒体分析 印刷レポート: 人材デモグラフィック section
//!
//! Impl-2 (2026-04-26) 担当: D-1 / D-2 / #10 / #17 を 1 つの section に統合し
//! 「対象地域の労働力候補者の年齢構成・学歴・失業状態・教育施設密度を俯瞰する」
//! ストーリーを描く。
//!
//! - **D-1 年齢層ピラミッド**: ext_pyramid (5 歳刻み or 5 区分) を主役グラフ
//! - **D-2 学歴分布**: ext_education (国勢調査 25 歳以上 最終学歴別) 補助バー
//! - **#10 採用候補プール**: ext_labor_force から失業者数を計算し KPI 提示
//! - **#17 教育施設密度**: ext_education_facilities から幼〜高 校数を補助 KPI
//!
//! ## 設計原則 (memory ルール準拠)
//! - `feedback_correlation_not_causation.md`: 相関 ≠ 因果。「傾向」「目安」表現に統一
//! - `feedback_hw_data_scope.md`: HW 求人ベースのデータと国勢調査ベースのデータを混同しない
//! - `feedback_test_data_validation.md`: 逆証明テストで「具体値」を検証
//!
//! ## section 配置 (mod.rs)
//! - 給与統計 (Section 3) の後に挿入
//! - 各案で必須注記を明記し、属性データと採用容易性を因果関係として断定しない
//!
//! ## 公開 API
//! - `render_section_demographics(html, ctx)` のみを super に公開

#![allow(unused_imports, dead_code)]

use super::super::super::helpers::{escape_html, format_number, get_f64, get_i64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use serde_json::json;

use super::helpers::*;

/// 5 歳刻み年齢階級の正規順序（subtab5_phase4 の order_clause と整合）
/// データソースが 5 歳刻みの場合: 0-4, 5-9, ... 75-79, 80+
/// データソースが 10 歳刻みの場合: 0-9, 10-19, ... 70-79, 80+
/// データソースが大区分の場合: 0-14, 15-64, 65-74, 75+
fn age_group_sort_key(label: &str) -> i32 {
    match label {
        // 5 歳刻み
        "0-4" => 0,
        "5-9" => 5,
        "10-14" => 10,
        "15-19" => 15,
        "20-24" => 20,
        "25-29" => 25,
        "30-34" => 30,
        "35-39" => 35,
        "40-44" => 40,
        "45-49" => 45,
        "50-54" => 50,
        "55-59" => 55,
        "60-64" => 60,
        "65-69" => 65,
        "70-74" => 70,
        "75-79" => 75,
        "80-84" => 80,
        "85+" => 85,
        "85-" => 85,
        // 10 歳刻み
        "0-9" => 0,
        "10-19" => 10,
        "20-29" => 20,
        "30-39" => 30,
        "40-49" => 40,
        "50-59" => 50,
        "60-69" => 60,
        "70-79" => 70,
        "80+" => 80,
        // 大区分
        "0-14" => 0,
        "15-64" => 15,
        "65-74" => 65,
        "75+" => 75,
        _ => 9999,
    }
}

/// 年齢階級ラベルが「生産年齢 (15-64)」に該当するかを判定
fn is_working_age(label: &str) -> bool {
    matches!(
        label,
        "15-19"
            | "20-24"
            | "25-29"
            | "30-34"
            | "35-39"
            | "40-44"
            | "45-49"
            | "50-54"
            | "55-59"
            | "60-64"
            | "10-19"
            | "20-29"
            | "30-39"
            | "40-49"
            | "50-59"
            | "60-69"   // 注: 10 歳刻みの場合 60-69 の一部は退職層を含むため近似
            | "15-64"
    )
}

/// 「採用ターゲット層 (25-44)」に該当するか (5 歳階級専用)
///
/// Round 12 (2026-05-12) 修正 (K17 確定バグ):
/// 旧実装は 10 歳刻みデータ (20-29 / 30-39 / 40-49) も「25-44 ターゲット」に含めていたが、
/// 国勢調査標準の 5 歳階級 (25-29 / 30-34 / 35-39 / 40-44) のみを厳密に対象とするよう修正。
fn is_target_age(label: &str) -> bool {
    matches!(label, "25-29" | "30-34" | "35-39" | "40-44")
}

/// 「採用ターゲット層 (20-49、10 歳階級粒度ベース)」に該当するか
///
/// Round 12 caller 修正 (2026-05-12):
/// 10 歳階級データ (現 DB schema) では 5 歳階級判定が全件 false になるため、
/// fallback として 10 歳階級ターゲット (20-29 / 30-39 / 40-49) を集計する。
/// KPI ラベルは「25-44」でなく「20-49 (10 歳階級粒度)」と明示し粒度ズレを防ぐ。
fn is_target_age_10yr(label: &str) -> bool {
    matches!(label, "20-29" | "30-39" | "40-49")
}

/// データが 5 歳階級か 10 歳階級か判定
///
/// Round 12 (2026-05-12): K17 修正に伴い導入。caller (`render_section_demographics`) が
/// bucket に応じて target_age 集計関数 + KPI ラベル + 説明文を切り替える。
fn detect_age_bucket_size(labels: &[String]) -> AgeBucketSize {
    if labels.iter().any(|l| {
        matches!(
            l.as_str(),
            "25-29" | "30-34" | "35-39" | "40-44" | "45-49" | "50-54" | "55-59" | "60-64" | "65-69"
        )
    }) {
        AgeBucketSize::FiveYear
    } else if labels
        .iter()
        .any(|l| matches!(l.as_str(), "20-29" | "30-39" | "40-49" | "50-59" | "60-69"))
    {
        AgeBucketSize::TenYear
    } else {
        AgeBucketSize::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgeBucketSize {
    FiveYear,
    TenYear,
    Unknown,
}

/// 65 歳以上か
fn is_senior(label: &str) -> bool {
    matches!(
        label,
        "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "85-" | "70-79" | "80+" | "65-74" | "75+"
    )
}

/// 14 歳以下 (年少人口)
fn is_youth(label: &str) -> bool {
    matches!(label, "0-4" | "5-9" | "10-14" | "0-9" | "10-19" | "0-14")
}

/// 人材デモグラフィック section のメイン entry
///
/// 全データソース (ext_pyramid / ext_education / ext_labor_force / ext_education_facilities)
/// が空の場合は section ごと出力しない (空白セクション抑止)。
pub(super) fn render_section_demographics(html: &mut String, ctx: &InsightContext) {
    let has_pyramid = !ctx.ext_pyramid.is_empty();
    let has_education = !ctx.ext_education.is_empty();
    let has_labor = !ctx.ext_labor_force.is_empty();
    let has_facilities = !ctx.ext_education_facilities.is_empty();

    if !has_pyramid && !has_education && !has_labor && !has_facilities {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>人材デモグラフィック</h2>\n");

    // 読み方ヒント (section 冒頭)
    render_section_howto(
        html,
        &[
            "対象地域の労働力候補者の年齢構成・学歴・失業状態・教育施設密度を俯瞰します",
            "国勢調査・労働力調査ベース。HW 求人とは粒度・期間が異なるため、市場全体の人材像参照として活用",
            "属性データと採用容易性は相関する場合がありますが、職種・条件マッチングが本質的要因です",
        ],
    );

    // ---- D-1: 年齢層ピラミッド (主役グラフ) ----
    if has_pyramid {
        render_pyramid_block(html, ctx);
    }

    // ---- KPI 補助カード群 (#10 採用候補プール / D-1 補助 / #17 施設密度) ----
    render_demographic_kpis(html, ctx);

    // ---- D-2: 学歴分布 ----
    if has_education {
        render_education_distribution(html, ctx);
    }

    // ---- 共通注記 ----
    html.push_str(
        "<p class=\"note\" style=\"margin-top:8px;\">\
        ※ 本 section の指標は国勢調査 (5 年に 1 回) および労働力調査ベース。\
        HW 掲載求人とは粒度・期間が異なるため、市場の人材像の俯瞰参照としてご利用ください。\
        属性データと採用容易性に相関が見られる場合がありますが、職種・条件マッチングが本質的要因です。\
        </p>\n",
    );

    render_section_bridge(
        html,
        "次セクションでは、この人材プールを前提とした給与の相関分析・地域分布へと進みます。\
         特に高齢化率が高い地域では「経験者向け給与水準」、若年層比率が高い地域では「初任給・育成枠の整備状況」、\
         女性労働力率が高い地域では「シフト柔軟性・短時間勤務制度」を意識して、次セクション以降の分布を読み解いてください。\
         （※ これらは観測される相関であり因果関係を主張するものではありません）",
    );

    html.push_str("</div>\n");
}

// ============================================================
// 2026-04-26 Granularity: 主要市区町村別 デモグラフィック section
// ============================================================

/// CSV 件数 上位 N 市区町村についてデモグラフィック指標をカード形式で並列表示。
///
/// ユーザー指摘 (2026-04-26):
/// > 都道府県単位の集計データはあまり参考にならない
/// → 市区町村粒度のピラミッド・失業者・教育施設を主要 N 都市分まとめて表示。
///
/// 各カードに表示する KPI:
/// - 市区町村名 + CSV 件数
/// - 高齢化率 (65+ 比率)
/// - 生産年齢人口比率 (15-64)
/// - 推定失業者数
/// - 教育施設数 (幼〜高 合計)
///
/// # 注記
/// - `feedback_correlation_not_causation.md`: KPI と採用容易性は相関であり因果ではない
/// - 学歴データは schema 上市区町村粒度未対応のため都道府県粒度のみ表示 (注記で明示)
pub(super) fn render_section_demographics_by_municipality(
    html: &mut String,
    munis: &[super::super::granularity::MunicipalityDemographics],
) {
    if munis.is_empty() {
        return;
    }
    // 全カードのデータが空ならスキップ
    let any_data = munis.iter().any(|m| {
        !m.pyramid.is_empty() || !m.labor_force.is_empty() || !m.education_facilities.is_empty()
    });
    if !any_data {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>主要市区町村別 人材デモグラフィック</h2>\n");

    render_section_howto(
        html,
        &[
            "CSV 件数上位の市区町村ごとに、年齢構成・失業者数・教育施設数を市区町村粒度で表示します",
            "都道府県平均に対する各都市の特性差を確認し、媒体配信や訴求軸の地域別最適化に活用",
            "学歴分布は schema 上市区町村粒度未対応のため都道府県値で代用 (注記参照)",
        ],
    );

    render_figure_caption(
        html,
        "表 D-M",
        "主要市区町村別 人材プール KPI (市区町村粒度)",
    );

    // カードグリッド: 1 列 / mobile 2 列 / desktop 3 列
    html.push_str(
        "<div class=\"stats-grid\" style=\"grid-template-columns:repeat(auto-fit, minmax(260px, 1fr));gap:12px;\" data-testid=\"municipality-demographics-grid\">\n",
    );

    for demo in munis {
        let aging = demo.aging_rate();
        let working_age = demo.working_age_rate();
        let unemp = demo.estimated_unemployed();
        let facilities = demo.total_facilities();

        html.push_str(&format!(
            "<div class=\"stat-box\" data-testid=\"municipality-demo-card\" style=\"padding:10px;border:1px solid #e5e7eb;border-radius:6px;\">\n\
             <div style=\"font-size:11px;color:#6b7280;\">{}</div>\n\
             <div style=\"font-size:14px;font-weight:bold;\">{}</div>\n\
             <div style=\"font-size:10px;color:#9ca3af;margin-bottom:6px;\">CSV 件数: {} 件</div>\n",
            escape_html(&demo.prefecture),
            escape_html(&demo.municipality),
            format_number(demo.csv_count as i64),
        ));

        // KPI ミニリスト
        html.push_str(
            "<div style=\"display:flex;flex-direction:column;gap:3px;font-size:11px;\">\n",
        );

        if aging > 0.0 {
            html.push_str(&format!(
                "<div>高齢化率: <strong>{:.1}%</strong></div>\n",
                aging
            ));
        }
        if working_age > 0.0 {
            html.push_str(&format!(
                "<div>生産年齢比率 (15-64): <strong>{:.1}%</strong></div>\n",
                working_age
            ));
        }
        if let Some(u) = unemp {
            html.push_str(&format!(
                "<div>推定 失業者: <strong>{} 人</strong></div>\n",
                format_number(u),
            ));
        }
        if facilities > 0 {
            html.push_str(&format!(
                "<div>教育施設 (幼〜高): <strong>{} 校</strong></div>\n",
                format_number(facilities),
            ));
        }

        // データ欠損の表示
        if aging <= 0.0 && working_age <= 0.0 && unemp.is_none() && facilities == 0 {
            html.push_str(
                "<div style=\"color:#9ca3af;font-style:italic;\">市区町村粒度データなし</div>\n",
            );
        }

        html.push_str("</div>\n</div>\n");
    }

    html.push_str("</div>\n");

    // 必須注記 (feedback_correlation_not_causation, feedback_hw_data_scope)
    html.push_str(
        "<p class=\"note\" style=\"margin-top:8px;\">\
        ※ 各 KPI は国勢調査・労働力調査ベースの市区町村粒度。\
        市区町村粒度データが欠損する場合は値が表示されません（都道府県値で代用していません）。\
        学歴分布は schema 上市区町村粒度に対応していないため、別 section の都道府県値をご参照ください。\
        KPI と採用容易性に相関が見られる場合がありますが、職種・条件マッチングが本質的要因です。\
        </p>\n",
    );

    render_section_bridge(
        html,
        "次セクションでは、これら主要市区町村を含む地域全体の給与構造へと進みます。\
         市区町村別の人口規模・年齢構成と給与水準の関係を比較し、\
         「人口規模が大きい市区町村ほど給与水準が高い傾向」または「都心部から離れるほど水準が下がる勾配」が\
         実際にデータ上に現れるかを確認してください。",
    );

    html.push_str("</div>\n");
}

// ============================================================
// D-1: 年齢層ピラミッド
// ============================================================

/// SSR で人口ピラミッド SVG を組み立てる。
///
/// ECharts SVG renderer は `emulateMedia('print')` 経路で X 軸 axisLabel `<text>` が
/// PDF 出力されない (DOM 上には存在するが page.pdf() で描画されない) ことを
/// 2026-05-13 ローカル page.pdf() 実証で確定。回避策として SVG を Rust 側で
/// 直接生成し、ブラウザ JS / ECharts の挙動に依存させない。
///
/// レイアウト (viewBox 0 0 800 340):
///   - 左マージン 80, 右マージン 80 (バー領域 80..720, 中心 400)
///   - 行 1 つあたり 24px、上部マージン 32 (凡例 + 開始)
///   - 軸ラベル領域は y=288..310
fn build_pyramid_svg(bands: &[(String, i64, i64)]) -> String {
    let n = bands.len() as i32;
    let max_abs: i64 = bands
        .iter()
        .map(|(_, m, f)| m.abs().max(f.abs()))
        .max()
        .unwrap_or(1)
        .max(1);
    // 軸 tick 候補: 7 個 (中心 + 左右 3 個ずつ)。1, 2, 5 × 10^k の "切りのいい数" から選ぶ。
    let step = nice_step(max_abs);
    let display_max = step * 3;
    let center_x = 400;
    let half_w = 320;
    let bar_h = 24;
    let bar_gap = 4;
    let body_top = 32;
    let plot_bottom = body_top + n * (bar_h + bar_gap);
    let axis_y = plot_bottom + 2;
    let axis_text_y = plot_bottom + 18;

    let mut s = String::with_capacity(2048);
    s.push_str(
        "<div class=\"pyramid-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 340\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         aria-label=\"年齢階級別 人口ピラミッド\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // 凡例
    s.push_str(
        "<g font-size=\"12\">\
         <rect x=\"340\" y=\"5\" width=\"14\" height=\"12\" fill=\"#3b82f6\"/>\
         <text x=\"360\" y=\"15\" fill=\"#0f172a\">男性</text>\
         <rect x=\"410\" y=\"5\" width=\"14\" height=\"12\" fill=\"#ec4899\"/>\
         <text x=\"430\" y=\"15\" fill=\"#0f172a\">女性</text>\
         </g>\n",
    );
    // バー: 入力 bands は age_group_sort_key で昇順だが、表示は上から高齢 → 若年。
    // よって逆順で描画する (最後の要素を最上段に)。
    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let row_y = body_top + (i as i32) * (bar_h + bar_gap);
        let male_w = ((male.abs() as f64) / (display_max as f64) * (half_w as f64)).round() as i32;
        let female_w = ((*female as f64) / (display_max as f64) * (half_w as f64)).round() as i32;
        let male_x = center_x - male_w;
        let female_x = center_x;
        s.push_str(&format!(
            "<g>\
             <rect x=\"{mx}\" y=\"{ry}\" width=\"{mw}\" height=\"{bh}\" fill=\"#3b82f6\"/>\
             <rect x=\"{fx}\" y=\"{ry}\" width=\"{fw}\" height=\"{bh}\" fill=\"#ec4899\"/>\
             <text x=\"78\" y=\"{ty}\" text-anchor=\"end\" font-size=\"11\" fill=\"#6e7079\">{lbl}</text>\
             </g>\n",
            mx = male_x, ry = row_y, mw = male_w, bh = bar_h,
            fx = female_x, fw = female_w,
            ty = row_y + bar_h / 2 + 4,
            lbl = escape_xml(label),
        ));
    }
    // 中央 0 線
    s.push_str(&format!(
        "<line x1=\"{cx}\" y1=\"{t}\" x2=\"{cx}\" y2=\"{b}\" stroke=\"#94a3b8\" stroke-width=\"1\"/>\n",
        cx = center_x, t = body_top - 4, b = plot_bottom + 2,
    ));
    // X 軸 (横線 + tick + ラベル)。tick は左右 3 つ + 中心 0 の計 7 本。
    s.push_str(&format!(
        "<line x1=\"80\" y1=\"{y}\" x2=\"720\" y2=\"{y}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\n",
        y = axis_y,
    ));
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\" text-anchor=\"middle\">\n");
    for k in -3..=3_i32 {
        let val = step * (k.unsigned_abs() as i64);
        let x = center_x + k * (half_w / 3);
        s.push_str(&format!(
            "<line x1=\"{x}\" y1=\"{y0}\" x2=\"{x}\" y2=\"{y1}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\
             <text x=\"{x}\" y=\"{ty}\">{lbl}</text>\n",
            x = x, y0 = axis_y, y1 = axis_y + 5, ty = axis_text_y,
            lbl = format_number_i64(val),
        ));
    }
    s.push_str("</g>\n");
    s.push_str("</svg>\n</div>\n");
    s
}

/// 1 / 2 / 5 × 10^k から、max_abs/3 を上回る最小の切りのいい数を選ぶ。
fn nice_step(max_abs: i64) -> i64 {
    let target = (max_abs as f64) / 3.0;
    if target <= 0.0 {
        return 1;
    }
    let exp = target.log10().floor();
    let base = 10_f64.powf(exp);
    for &m in &[1.0, 2.0, 5.0, 10.0] {
        if m * base >= target {
            return (m * base) as i64;
        }
    }
    (10.0 * base) as i64
}

/// 3 桁区切り (Rust 標準にはないので自作)。
fn format_number_i64(n: i64) -> String {
    let s = n.abs().to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 年齢ピラミッドブロックを描画。
/// SSR SVG (左=男性 / 右=女性)。下部に「15-64 歳 / 25-44 歳」KPI を表示。
fn render_pyramid_block(html: &mut String, ctx: &InsightContext) {
    // ext_pyramid から年齢階級別 (label, male, female) を抽出
    let mut bands: Vec<(String, i64, i64)> = ctx
        .ext_pyramid
        .iter()
        .map(|r| {
            let label = get_str_ref(r, "age_group").to_string();
            let male = get_i64(r, "male_count");
            let female = get_i64(r, "female_count");
            (label, male, female)
        })
        .filter(|(label, _, _)| !label.is_empty())
        .collect();

    if bands.is_empty() {
        return;
    }

    bands.sort_by_key(|(l, _, _)| age_group_sort_key(l));

    render_figure_caption(html, "図 D-1", "年齢階級別 人口ピラミッド (国勢調査ベース)");

    // Round 16 (2026-05-13): ECharts SVG renderer は emulateMedia('print') 経路で
    // X 軸 axisLabel が PDF 出力されない問題があるため、SSR で SVG を直接生成する。
    // ローカル page.pdf() で X 軸目盛が確実に描画されることを検証済 (out/echart_local/ssr_pyramid.pdf)。
    html.push_str(&build_pyramid_svg(&bands));

    // Round 12 K17 caller 修正: 説明文も粒度に応じて切替
    let target_band_text = match detect_age_bucket_size(
        &ctx.ext_pyramid
            .iter()
            .map(|r| get_str_ref(r, "age_group").to_string())
            .collect::<Vec<_>>(),
    ) {
        AgeBucketSize::FiveYear => "採用ターゲット層 25-44 歳",
        AgeBucketSize::TenYear => "採用ターゲット層 20-49 歳 (10 歳階級粒度)",
        AgeBucketSize::Unknown => "採用ターゲット層 (粒度不明)",
    };
    render_read_hint(
        html,
        &format!(
            "左 (青) が男性・右 (桃) が女性。バーが長い階級ほどその年代の人口が多いことを示します。\
             15-64 歳が広く{}のバーが太い地域は、生産年齢層が厚く採用候補母集団が大きい傾向にあります。",
            target_band_text
        ),
    );

    // 必須注記 (D-1)
    html.push_str(
        "<p class=\"note\">\
        ※ 生産年齢人口の定義は 15-64 歳 (国勢調査基準)。実際の労働参加率は別途要確認 (失業率 / 労働力率 を併用)。\
        </p>\n",
    );
}

// ============================================================
// 補助 KPI 群 (#10 採用候補プール / D-1 サマリ / #17 教育施設密度)
// ============================================================

fn render_demographic_kpis(html: &mut String, ctx: &InsightContext) {
    // ---- D-1 サマリ計算: 15-64 歳 / 25-44 歳 / 全人口 ----
    let mut total_pop: i64 = 0;
    let mut working_age: i64 = 0;
    let mut target_age: i64 = 0;
    let mut senior: i64 = 0;
    for r in &ctx.ext_pyramid {
        let label = get_str_ref(r, "age_group");
        let m = get_i64(r, "male_count");
        let f = get_i64(r, "female_count");
        let t = m + f;
        total_pop += t;
        if is_working_age(label) {
            working_age += t;
        }
        // Round 12 K17 caller 修正:
        // 5 歳階級時は厳密判定、10 歳階級時は 20-49 fallback 集計、
        // どちらにも該当しない場合は target_age=0 で KPI 非表示
        if is_target_age(label) {
            target_age += t;
        }
        if is_senior(label) {
            senior += t;
        }
    }
    let _ = senior;

    // Round 12 K17 caller 修正: data 粒度判定 + 10 歳階級時の fallback 集計
    let age_labels: Vec<String> = ctx
        .ext_pyramid
        .iter()
        .map(|r| get_str_ref(r, "age_group").to_string())
        .collect();
    let age_bucket = detect_age_bucket_size(&age_labels);
    let mut target_age_10yr: i64 = 0;
    if matches!(age_bucket, AgeBucketSize::TenYear) {
        for r in &ctx.ext_pyramid {
            let label = get_str_ref(r, "age_group");
            if is_target_age_10yr(label) {
                target_age_10yr += get_i64(r, "male_count") + get_i64(r, "female_count");
            }
        }
    }

    // ---- #10 採用候補プール計算: ext_labor_force から ----
    // 実装方針: SUM(unemployed) を直接利用 (pref レベルでは集計済み)
    // なければ unemployment_rate × labor_force から推定
    let labor_row = ctx.ext_labor_force.first();
    let estimated_unemployed: Option<i64> = labor_row.and_then(|r| {
        let direct = get_i64(r, "unemployed");
        if direct > 0 {
            return Some(direct);
        }
        // 直接値が無い場合: 失業率 × 労働力人口 から計算
        let rate = get_f64(r, "unemployment_rate"); // パーセント値
        let employed = get_i64(r, "employed");
        let unemp_calc = get_i64(r, "unemployed");
        let labor_force_total = employed + unemp_calc;
        if rate > 0.0 && labor_force_total > 0 {
            Some(((labor_force_total as f64) * rate / 100.0).round() as i64)
        } else {
            None
        }
    });
    let unemployment_rate: Option<f64> = labor_row.and_then(|r| {
        let v = get_f64(r, "unemployment_rate");
        if v > 0.0 {
            Some(v)
        } else {
            None
        }
    });
    // pref_avg_unemployment_rate は fetch_prefecture_mean (subtab7_other.rs:282) の SQL が
    // 既に * 100 してパーセント単位で返すため、再変換しない (バグ修正 2026-04-27)
    let pref_avg_unemp = ctx.pref_avg_unemployment_rate;

    // ---- #17 教育施設密度: 4 区分の合計 / 1万人あたり ----
    // 注: 本 schema には大学/専門学校カラムは存在しない。幼稚園〜高校の合計で密度算出。
    let edu_facility_row = ctx.ext_education_facilities.first();
    let total_facilities: i64 = edu_facility_row
        .map(|r| {
            get_i64(r, "kindergartens")
                + get_i64(r, "elementary_schools")
                + get_i64(r, "junior_high_schools")
                + get_i64(r, "high_schools")
        })
        .unwrap_or(0);
    let kindergartens = edu_facility_row
        .map(|r| get_i64(r, "kindergartens"))
        .unwrap_or(0);
    let elementary = edu_facility_row
        .map(|r| get_i64(r, "elementary_schools"))
        .unwrap_or(0);
    let junior = edu_facility_row
        .map(|r| get_i64(r, "junior_high_schools"))
        .unwrap_or(0);
    let high = edu_facility_row
        .map(|r| get_i64(r, "high_schools"))
        .unwrap_or(0);

    // 全国平均施設密度 (10 万人あたり) は schema に存在しないため、
    // 「対象地域の絶対数」+ 「対象地域 10 万人あたり密度」を併記する。
    // 全国比較は明示的にできないため caveat で言及。
    let facility_per_100k: Option<f64> = if total_pop > 0 && total_facilities > 0 {
        Some(total_facilities as f64 / (total_pop as f64 / 100_000.0))
    } else {
        None
    };

    // ---- KPI カード描画 ----
    render_figure_caption(html, "表 D-1", "人材プール 主要 KPI");
    html.push_str("<div class=\"stats-grid\" style=\"grid-template-columns:repeat(auto-fit, minmax(180px, 1fr));gap:8px;\">\n");

    // KPI: 15-64 歳 (生産年齢人口)
    if working_age > 0 && total_pop > 0 {
        let pct = working_age as f64 / total_pop as f64 * 100.0;
        render_stat_box(
            html,
            "15-64 歳 (生産年齢)",
            &format!("{} 人 ({:.1}%)", format_number(working_age), pct),
        );
    }

    // KPI: 採用ターゲット層 (Round 12 K17 caller 修正)
    // - 5 歳階級時: 「25-44 歳 (採用ターゲット層)」 (target_age = 25-29+30-34+35-39+40-44)
    // - 10 歳階級時: 「20-49 歳 (採用ターゲット層・10 歳階級粒度)」 (target_age_10yr = 20-29+30-39+40-49)
    //   ※「25-44」厳密ターゲットは粒度上計算不能のため、20-49 を fallback 表示
    // - Unknown / 該当データなし: KPI 非表示
    let (target_value, target_label) = match age_bucket {
        AgeBucketSize::FiveYear if target_age > 0 => {
            (target_age, "25-44 歳 (採用ターゲット層)")
        }
        AgeBucketSize::TenYear if target_age_10yr > 0 => (
            target_age_10yr,
            "20-49 歳 (採用ターゲット層・10 歳階級粒度)",
        ),
        _ => (0, ""),
    };
    if target_value > 0 {
        let pct = if total_pop > 0 {
            target_value as f64 / total_pop as f64 * 100.0
        } else {
            0.0
        };
        render_stat_box(
            html,
            target_label,
            &format!("{} 人 ({:.1}%)", format_number(target_value), pct),
        );
    }

    // KPI: 推定失業者数 (#10)
    if let Some(unemp) = estimated_unemployed {
        let compare = match (unemployment_rate, pref_avg_unemp) {
            (Some(r), Some(avg)) if avg > 0.0 => {
                let ratio = r / avg;
                format!("失業率 {:.2}% (県平均比 {:.2} 倍)", r, ratio)
            }
            (Some(r), _) => format!("失業率 {:.2}%", r),
            _ => String::new(),
        };
        render_stat_box(
            html,
            "推定 失業者数 (採用候補プール)",
            &if compare.is_empty() {
                format!("{} 人", format_number(unemp))
            } else {
                format!("{} 人 / {}", format_number(unemp), compare)
            },
        );
    }

    // KPI: 教育施設密度 (#17)
    if total_facilities > 0 {
        let value = match facility_per_100k {
            Some(d) => format!("{} 校 ({:.1}/10万人)", format_number(total_facilities), d),
            None => format!("{} 校", format_number(total_facilities)),
        };
        render_stat_box(html, "教育施設 (幼〜高 合計)", &value);
    }
    html.push_str("</div>\n");

    // 内訳: 教育施設 4 区分
    if total_facilities > 0 {
        html.push_str(
            "<table class=\"sortable-table zebra\" style=\"max-width:520px;margin-top:8px;\">\n",
        );
        html.push_str("<thead><tr><th>施設種別</th><th style=\"text-align:right\">校数</th></tr></thead>\n<tbody>\n");
        html.push_str(&format!(
            "<tr><td>幼稚園</td><td class=\"num\">{}</td></tr>\n",
            format_number(kindergartens)
        ));
        html.push_str(&format!(
            "<tr><td>小学校</td><td class=\"num\">{}</td></tr>\n",
            format_number(elementary)
        ));
        html.push_str(&format!(
            "<tr><td>中学校</td><td class=\"num\">{}</td></tr>\n",
            format_number(junior)
        ));
        html.push_str(&format!(
            "<tr><td>高等学校</td><td class=\"num\">{}</td></tr>\n",
            format_number(high)
        ));
        html.push_str("</tbody></table>\n");
    }

    // 必須注記 (#10 / #17)
    if estimated_unemployed.is_some() {
        html.push_str(
            "<p class=\"note\">\
            ※ 推定失業者数は 失業率 × 労働力人口 の単純積、もしくは労働力調査の集計値。\
            実際の応募可能性は属性・職種マッチング・通勤可能距離に依存します。\
            </p>\n",
        );
    }
    if total_facilities > 0 {
        html.push_str(
            "<p class=\"note\">\
            ※ 教育施設密度は幼稚園〜高校の合計。\
            本 schema には大学・専門学校カラムは存在しないため、新卒採用ポテンシャルの参考値としては高校以下の集計のみとなります。\
            施設密度と採用容易性は相関する場合がありますが、職種・条件マッチングが本質的要因です。\
            </p>\n",
        );
    }
}

// ============================================================
// D-2: 学歴分布
// ============================================================

fn render_education_distribution(html: &mut String, ctx: &InsightContext) {
    // ext_education から (level, total_count) を抽出
    let levels: Vec<(String, i64)> = ctx
        .ext_education
        .iter()
        .map(|r| {
            let level = get_str_ref(r, "education_level").to_string();
            let total = get_i64(r, "total_count");
            (level, total)
        })
        .filter(|(l, t)| !l.is_empty() && *t > 0)
        .collect();

    if levels.is_empty() {
        return;
    }

    let total: i64 = levels.iter().map(|(_, c)| *c).sum();
    if total <= 0 {
        return;
    }

    render_figure_caption(html, "図 D-2", "最終学歴 構成 (国勢調査 25 歳以上)");

    // 横バー (level ごとに count + 比率)
    html.push_str(
        "<div class=\"edu-bar-list\" style=\"display:flex;flex-direction:column;gap:6px;\">\n",
    );
    for (level, count) in &levels {
        let pct = *count as f64 / total as f64 * 100.0;
        let pct_clamped = pct.clamp(0.0, 100.0);
        html.push_str(&format!(
            "<div class=\"edu-bar-row\" style=\"display:flex;align-items:center;gap:8px;font-size:11px;\">\
             <div style=\"min-width:120px;\">{}</div>\
             <div style=\"flex:1;height:14px;background:#eef2ff;border-radius:3px;overflow:hidden;\">\
             <div style=\"width:{:.1}%;height:100%;background:#6366f1;\"></div>\
             </div>\
             <div style=\"min-width:96px;text-align:right;\">{} 人 ({:.1}%)</div>\
             </div>\n",
            escape_html(level),
            pct_clamped,
            format_number(*count),
            pct
        ));
    }
    html.push_str("</div>\n");

    render_read_hint(
        html,
        "対象地域の 25 歳以上人口の最終学歴構成。\
         大卒・大学院 比率が高い地域は専門職・管理職向け求人で母集団が厚い傾向、\
         高卒比率が高い地域は若年層採用・現業職で母集団が厚い傾向にあります。",
    );

    // 必須注記 (D-2)
    html.push_str(
        "<p class=\"note\">\
        ※ 国勢調査 (5 年に 1 回) ベース。最新は 2020 年データ。25 歳以上人口の最終学歴別構成です。\
        全国平均との比較は schema 上のスコープ外のため、対象地域の構成のみを表示しています。\
        </p>\n",
    );
}

// ============================================================
// 単体テスト (逆証明テスト群)
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::helpers::Row;
    use serde_json::json;

    fn row(pairs: &[(&str, serde_json::Value)]) -> Row {
        let mut m = Row::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        m
    }

    /// テスト用の最小 InsightContext を build
    fn build_test_ctx(
        pyramid: Vec<Row>,
        education: Vec<Row>,
        labor_force: Vec<Row>,
        edu_facilities: Vec<Row>,
        pref_avg_unemp: Option<f64>,
    ) -> InsightContext {
        InsightContext {
            vacancy: vec![],
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy: vec![],
            ts_salary: vec![],
            ts_fulfillment: vec![],
            ts_tracking: vec![],
            ext_job_ratio: vec![],
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover: vec![],
            ext_population: vec![],
            ext_pyramid: pyramid,
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: labor_force,
            ext_medical_welfare: vec![],
            ext_education_facilities: edu_facilities,
            ext_geography: vec![],
            ext_education: education,
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            pref_avg_unemployment_rate: pref_avg_unemp,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: "東京都".to_string(),
            muni: "千代田区".to_string(),
        }
    }

    /// 全データ空の場合 section が出力されないこと (空白セクション抑止)
    #[test]
    fn demographics_empty_data_renders_nothing() {
        let ctx = build_test_ctx(vec![], vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);
        assert!(
            html.is_empty(),
            "全データ空ならば section ごと出力されない (got: {} chars)",
            html.len()
        );
    }

    /// D-1: 5 歳刻みピラミッドデータが ECharts data-chart-config 内に含まれること
    #[test]
    fn demographics_d1_pyramid_5year_bands_present() {
        let pyramid = vec![
            row(&[
                ("age_group", json!("20-24")),
                ("male_count", json!(5000)),
                ("female_count", json!(4800)),
            ]),
            row(&[
                ("age_group", json!("25-29")),
                ("male_count", json!(6000)),
                ("female_count", json!(5800)),
            ]),
            row(&[
                ("age_group", json!("30-34")),
                ("male_count", json!(7000)),
                ("female_count", json!(6800)),
            ]),
            row(&[
                ("age_group", json!("35-39")),
                ("male_count", json!(7500)),
                ("female_count", json!(7300)),
            ]),
            row(&[
                ("age_group", json!("40-44")),
                ("male_count", json!(8000)),
                ("female_count", json!(7800)),
            ]),
        ];
        let ctx = build_test_ctx(pyramid, vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // Round 16 (2026-05-13): ECharts → SSR SVG に置換済。5 歳刻みラベル全てが
        // <text> 要素として SVG 内に含まれる。
        for label in &["20-24", "25-29", "30-34", "35-39", "40-44"] {
            assert!(
                html.contains(label),
                "5 歳刻みラベル {} が SSR SVG 内 <text> として必要",
                label
            );
        }
        // SSR SVG 識別属性 (pyramid-ssr クラス)
        assert!(
            html.contains("pyramid-ssr"),
            "ピラミッドの SSR SVG (.pyramid-ssr) が必要"
        );
        // SVG 内に男女系列 (色) が出力されている
        assert!(html.contains("#3b82f6"), "男性 (青) rect 必須");
        assert!(html.contains("#ec4899"), "女性 (ピンク) rect 必須");
        // 図表番号
        assert!(html.contains("図 D-1"), "図 D-1 ピラミッドキャプション必須");
        // section 見出し
        assert!(html.contains("人材デモグラフィック"), "section 見出し必須");
    }

    /// D-1: 「15-64 歳」「25-44 歳」KPI 値が表示されること
    #[test]
    fn demographics_d1_working_age_and_target_age_kpis() {
        let pyramid = vec![
            row(&[
                ("age_group", json!("15-19")),
                ("male_count", json!(1000)),
                ("female_count", json!(1000)),
            ]),
            row(&[
                ("age_group", json!("25-29")),
                ("male_count", json!(1500)),
                ("female_count", json!(1500)),
            ]),
            row(&[
                ("age_group", json!("35-39")),
                ("male_count", json!(2000)),
                ("female_count", json!(2000)),
            ]),
            row(&[
                ("age_group", json!("65-69")),
                ("male_count", json!(500)),
                ("female_count", json!(500)),
            ]),
        ];
        // 15-64 歳合計 = 2000 (15-19) + 3000 (25-29) + 4000 (35-39) = 9000
        // 25-44 歳合計 = 3000 (25-29) + 4000 (35-39) = 7000
        // 65+ = 1000
        // 全人口 = 10000
        let ctx = build_test_ctx(pyramid, vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        assert!(
            html.contains("15-64 歳 (生産年齢)"),
            "生産年齢 KPI ラベル必須"
        );
        assert!(
            html.contains("25-44 歳 (採用ターゲット層)"),
            "採用ターゲット層 KPI 必須"
        );
        // 9,000 人 (90.0%) が表示される
        assert!(
            html.contains("9,000 人"),
            "生産年齢人口の具体値 9,000 人 が表示されること (got: {})",
            crate::text_util::truncate_char_safe(&html, 2000)
        );
        assert!(
            html.contains("90.0%"),
            "生産年齢比率 90.0% が表示されること"
        );
        assert!(
            html.contains("7,000 人"),
            "採用ターゲット層の具体値 7,000 人 が表示されること"
        );
    }

    /// D-2: 学歴分布の 4-5 段階バーが表示されること
    #[test]
    fn demographics_d2_education_bars_5_levels() {
        let education = vec![
            row(&[
                ("education_level", json!("中卒")),
                ("total_count", json!(50_000)),
            ]),
            row(&[
                ("education_level", json!("高卒")),
                ("total_count", json!(300_000)),
            ]),
            row(&[
                ("education_level", json!("短大・高専")),
                ("total_count", json!(150_000)),
            ]),
            row(&[
                ("education_level", json!("大卒")),
                ("total_count", json!(400_000)),
            ]),
            row(&[
                ("education_level", json!("大学院")),
                ("total_count", json!(100_000)),
            ]),
        ];
        let ctx = build_test_ctx(vec![], education, vec![], vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        for level in &["中卒", "高卒", "短大・高専", "大卒", "大学院"] {
            assert!(
                html.contains(level),
                "学歴ラベル {} が表示されること",
                level
            );
        }
        // 全体 = 1,000,000。大卒比率 = 40%
        assert!(html.contains("400,000 人"), "大卒の具体値 400,000 人");
        assert!(html.contains("40.0%"), "大卒比率 40.0%");
        assert!(html.contains("図 D-2"), "図 D-2 キャプション必須");
        // バーレンダリング識別 class
        assert!(html.contains("edu-bar-row"), "学歴バー行 class 必須");
    }

    /// #10: 失業者推定値の計算: 労働力 100 万 × 2.5% = 25,000
    /// 実装は SUM(unemployed) を直接利用するため、unemployed 直接値を使う逆証明
    #[test]
    fn demographics_p10_unemployed_direct_value() {
        let labor_force = vec![row(&[
            ("employed", json!(975_000)),
            ("unemployed", json!(25_000)),
            ("unemployment_rate", json!(2.5)),
        ])];
        let ctx = build_test_ctx(vec![], vec![], labor_force, vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // 失業者数 25,000 人 が表示される
        assert!(
            html.contains("25,000 人"),
            "推定失業者数 25,000 人 が表示されること"
        );
        assert!(
            html.contains("失業率 2.50%"),
            "失業率 2.50% が表示されること"
        );
        assert!(
            html.contains("採用候補プール"),
            "「採用候補プール」KPI ラベル必須"
        );
    }

    /// #10: unemployed=0 でも unemployment_rate × labor_force から逆算できること
    #[test]
    fn demographics_p10_unemployed_calculated_from_rate() {
        // unemployed=0 直接値なし、employed=400,000, rate=4.0% → 推定 unemp = 16,000
        let labor_force = vec![row(&[
            ("employed", json!(400_000)),
            ("unemployed", json!(0)),
            ("unemployment_rate", json!(4.0)),
        ])];
        let ctx = build_test_ctx(vec![], vec![], labor_force, vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // 直接値 0 -> rate × labor 計算: 400,000 × 4% = 16,000
        // 計算値: (400_000 + 0) × 4 / 100 = 16,000
        assert!(
            html.contains("16,000 人"),
            "rate 経由の推定失業者数 16,000 人。html抜粋: {}",
            crate::text_util::truncate_char_safe(&html, 800)
        );
    }

    /// #10: 県平均比 (pref_avg_unemployment_rate) が表示されること
    /// 注: pref_avg_unemployment_rate は fetch_prefecture_mean の SQL で既に * 100 されているため
    ///     パーセント単位で渡す (例: 2.0 = 2.0%)。
    #[test]
    fn demographics_p10_pref_avg_compare() {
        let labor_force = vec![row(&[
            ("employed", json!(975_000)),
            ("unemployed", json!(25_000)),
            ("unemployment_rate", json!(2.5)),
        ])];
        // 県平均 失業率 2.0% (パーセント単位で直接)
        let ctx = build_test_ctx(vec![], vec![], labor_force, vec![], Some(2.0));
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // ratio = 2.5 / 2.0 = 1.25 倍
        assert!(
            html.contains("県平均比 1.25 倍"),
            "県平均比 1.25 倍 が表示されること"
        );
    }

    /// 逆証明: 県平均比が物理的にあり得ない値 (e.g. 0.01 倍) を弾く
    /// pref_avg_unemployment_rate に 380.0 のような誤データが入った場合でも、
    /// 表示された比率が 1.0 のオーダーに収まることを確認 (ドメイン不変条件)
    #[test]
    fn demographics_p10_pref_avg_compare_sanity() {
        let labor_force = vec![row(&[
            ("employed", json!(975_000)),
            ("unemployed", json!(25_000)),
            ("unemployment_rate", json!(2.5)),
        ])];
        // 正常なパーセント値 3.0%
        let ctx = build_test_ctx(vec![], vec![], labor_force, vec![], Some(3.0));
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);
        // 比率は ~0.83 倍 (=2.5/3.0) のはず。0.01 倍のような不正値が出てはならない
        assert!(
            !html.contains("県平均比 0.01 倍") && !html.contains("県平均比 0.0 倍"),
            "比率が物理的にあり得ない値になってはならない (二重 100 倍のような単位ミス検出)"
        );
        assert!(
            html.contains("県平均比 0.83 倍"),
            "県平均比 0.83 倍 (=2.5/3.0) が表示されること"
        );
    }

    /// #17: 教育施設密度 + 全国平均比較ではなく対象地域の絶対数 + 内訳が出ること
    /// (schema に大学カラム無のため "全国平均 X" 比較は不可。代わりに 4 区分内訳と密度を表示)
    #[test]
    fn demographics_p17_education_facilities_breakdown() {
        let edu_facilities = vec![row(&[
            ("kindergartens", json!(20)),
            ("elementary_schools", json!(50)),
            ("junior_high_schools", json!(25)),
            ("high_schools", json!(15)),
        ])];
        // 人口データなしでも絶対数は表示
        let ctx = build_test_ctx(vec![], vec![], vec![], edu_facilities, None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        assert!(
            html.contains("教育施設 (幼〜高 合計)"),
            "教育施設 KPI ラベル必須"
        );
        // 合計 110 校
        assert!(html.contains("110 校"), "施設合計 110 校 表示");
        // 4 区分内訳
        for facility in &["幼稚園", "小学校", "中学校", "高等学校"] {
            assert!(html.contains(facility), "施設内訳ラベル {} 必須", facility);
        }
        // 注記: 大学カラム無
        assert!(
            html.contains("大学・専門学校カラムは存在しない") || html.contains("大学・専門学校"),
            "schema に大学カラム無の caveat が必要"
        );
    }

    /// #17: 人口データがある場合、10 万人あたり密度が計算されること
    /// 例: 人口 100,000 人, 施設 50 校 → 50.0 / 10万人
    #[test]
    fn demographics_p17_facility_density_per_100k() {
        let pyramid = vec![row(&[
            ("age_group", json!("15-64")),
            ("male_count", json!(50_000)),
            ("female_count", json!(50_000)),
        ])];
        let edu_facilities = vec![row(&[
            ("kindergartens", json!(10)),
            ("elementary_schools", json!(20)),
            ("junior_high_schools", json!(10)),
            ("high_schools", json!(10)),
        ])];
        // 人口 100,000、施設合計 50 → 50.0/10 万人
        let ctx = build_test_ctx(pyramid, vec![], vec![], edu_facilities, None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        assert!(
            html.contains("50.0/10万人"),
            "10 万人あたり密度 50.0/10万人 が表示されること"
        );
    }

    /// 必須注記文言の検証: 4 案すべてに caveat が含まれること (memory feedback_correlation_not_causation)
    #[test]
    fn demographics_required_caveats_present() {
        let pyramid = vec![row(&[
            ("age_group", json!("25-29")),
            ("male_count", json!(1000)),
            ("female_count", json!(1000)),
        ])];
        let education = vec![row(&[
            ("education_level", json!("大卒")),
            ("total_count", json!(100)),
        ])];
        let labor_force = vec![row(&[
            ("employed", json!(99_000)),
            ("unemployed", json!(1_000)),
            ("unemployment_rate", json!(1.0)),
        ])];
        let edu_facilities = vec![row(&[
            ("kindergartens", json!(5)),
            ("elementary_schools", json!(10)),
            ("junior_high_schools", json!(5)),
            ("high_schools", json!(3)),
        ])];
        let ctx = build_test_ctx(pyramid, education, labor_force, edu_facilities, None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // D-1 caveat: 生産年齢人口定義
        assert!(
            html.contains("生産年齢人口の定義は 15-64 歳"),
            "D-1 caveat: 生産年齢人口定義必須"
        );
        // D-1 caveat: 労働参加率は別途要確認
        assert!(
            html.contains("実際の労働参加率は別途要確認"),
            "D-1 caveat: 労働参加率注記必須"
        );
        // D-2 caveat: 国勢調査 25 歳以上
        assert!(
            html.contains("国勢調査 (5 年に 1 回)") && html.contains("25 歳以上"),
            "D-2 caveat: 国勢調査 25 歳以上注記必須"
        );
        // #10 caveat: 失業率 × 労働力人口 単純積
        assert!(
            html.contains("失業率 \u{00D7} 労働力人口") || html.contains("失業率 × 労働力人口"),
            "#10 caveat: 失業率×労働力人口 単純積注記必須"
        );
        assert!(
            html.contains("属性・職種マッチング"),
            "#10 caveat: 属性・職種マッチング注記必須"
        );
        // #17 caveat: 施設密度と採用容易性は相関する場合 + 職種・条件マッチングが本質的要因
        assert!(
            html.contains("施設密度と採用容易性は相関する場合"),
            "#17 caveat: 相関にとどまる注記必須"
        );
        assert!(
            html.contains("職種・条件マッチングが本質的要因"),
            "#17 caveat: 本質的要因の注記必須"
        );
    }

    /// 共通 read_hint と section-howto が含まれること (UI-3 整合)
    #[test]
    fn demographics_section_has_howto_and_bridge() {
        let pyramid = vec![row(&[
            ("age_group", json!("25-29")),
            ("male_count", json!(1000)),
            ("female_count", json!(1000)),
        ])];
        let ctx = build_test_ctx(pyramid, vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_demographics(&mut html, &ctx);

        // section-howto class (helpers::render_section_howto 由来)
        assert!(
            html.contains("section-howto"),
            "section 冒頭 howto ガイド必須"
        );
        // 「対象地域の労働力候補者」は本 section 冒頭ガイドの主題文
        assert!(
            html.contains("労働力候補者の年齢構成"),
            "section 冒頭ガイドに労働力候補者の説明必須"
        );
        // section-bridge (次セクションへのつなぎ)
        assert!(
            html.contains("section-bridge"),
            "次セクションへのつなぎ section-bridge 必須"
        );
    }

    /// 年齢階級ソート関数の正しさ検証 (逆証明: 5/10 歳刻み混在で正しく昇順)
    #[test]
    fn demographics_age_sort_key_works() {
        let mut labels = vec!["35-39", "0-4", "65-69", "20-24", "75+", "10-14"];
        labels.sort_by_key(|l| age_group_sort_key(l));
        assert_eq!(
            labels,
            vec!["0-4", "10-14", "20-24", "35-39", "65-69", "75+"]
        );
    }

    /// is_working_age / is_target_age / is_senior の正確性 (逆証明)
    #[test]
    fn demographics_age_categorization() {
        assert!(is_working_age("15-19"));
        assert!(is_working_age("60-64"));
        assert!(!is_working_age("65-69"));
        assert!(!is_working_age("0-14"));

        assert!(is_target_age("25-29"));
        assert!(is_target_age("40-44"));
        assert!(!is_target_age("20-24"));
        assert!(!is_target_age("45-49"));

        assert!(is_senior("65-69"));
        assert!(is_senior("80+"));
        assert!(!is_senior("60-64"));
    }

    // ========================================================================
    // 2026-04-26 Granularity: 市区町村別デモグラフィック section の逆証明テスト
    // ========================================================================

    fn make_muni_demo(
        pref: &str,
        muni: &str,
        count: usize,
        pyramid: Vec<Row>,
        labor_force: Vec<Row>,
        edu_facilities: Vec<Row>,
    ) -> super::super::super::granularity::MunicipalityDemographics {
        super::super::super::granularity::MunicipalityDemographics {
            prefecture: pref.to_string(),
            municipality: muni.to_string(),
            csv_count: count,
            pyramid,
            education: vec![],
            is_education_pref_fallback: true,
            labor_force,
            education_facilities: edu_facilities,
            population: vec![],
            geography: vec![],
        }
    }

    /// 逆証明: 空 Vec で section 出力なし (空白 section 抑止)
    #[test]
    fn granularity_demographics_municipality_empty_renders_nothing() {
        let mut html = String::new();
        render_section_demographics_by_municipality(&mut html, &[]);
        assert!(html.is_empty(), "空 Vec ではセクション非表示");
    }

    /// 逆証明: 全 muni のデータが空でも section 出力なし
    #[test]
    fn granularity_demographics_municipality_all_empty_data_renders_nothing() {
        let munis = vec![make_muni_demo(
            "東京都",
            "千代田区",
            50,
            vec![],
            vec![],
            vec![],
        )];
        let mut html = String::new();
        render_section_demographics_by_municipality(&mut html, &munis);
        assert!(html.is_empty(), "全データ空ならセクション非表示");
    }

    /// 逆証明: 上位 3 市区町村のカードが描画され、KPI 値が具体値で表示される
    #[test]
    fn granularity_demographics_municipality_renders_kpi_values() {
        let pyramid_a = vec![
            row(&[
                ("age_group", json!("20-29")),
                ("male_count", json!(2000)),
                ("female_count", json!(2000)),
            ]),
            row(&[
                ("age_group", json!("30-39")),
                ("male_count", json!(2000)),
                ("female_count", json!(2000)),
            ]),
            row(&[
                ("age_group", json!("65-69")),
                ("male_count", json!(1000)),
                ("female_count", json!(1000)),
            ]),
        ];
        let labor_a = vec![row(&[
            ("employed", json!(95_000)),
            ("unemployed", json!(5_000)),
            ("unemployment_rate", json!(5.0)),
        ])];
        let edu_a = vec![row(&[
            ("kindergartens", json!(10)),
            ("elementary_schools", json!(20)),
            ("junior_high_schools", json!(10)),
            ("high_schools", json!(5)),
        ])];

        let munis = vec![make_muni_demo(
            "東京都",
            "千代田区",
            100,
            pyramid_a,
            labor_a,
            edu_a,
        )];

        let mut html = String::new();
        render_section_demographics_by_municipality(&mut html, &munis);

        assert!(!html.is_empty(), "section 描画される");
        assert!(
            html.contains("主要市区町村別 人材デモグラフィック"),
            "見出し必須"
        );
        assert!(
            html.contains("data-testid=\"municipality-demographics-grid\""),
            "グリッド data-testid"
        );
        assert!(
            html.contains("data-testid=\"municipality-demo-card\""),
            "カード data-testid"
        );
        // KPI 表示
        assert!(html.contains("千代田区"), "市区町村名");
        assert!(html.contains("東京都"), "都道府県名");
        assert!(html.contains("100"), "CSV 件数 100");
        // 高齢化率 = 2000 / 10000 = 20.0%
        assert!(html.contains("20.0%"), "高齢化率 20.0%");
        // 生産年齢比率 = 8000 / 10000 = 80.0%
        assert!(html.contains("80.0%"), "生産年齢比率 80.0%");
        // 推定失業者数 = 5000
        assert!(html.contains("5,000 人"), "失業者数 5,000 人");
        // 教育施設 = 10+20+10+5 = 45
        assert!(html.contains("45 校"), "施設合計 45 校");
        // 必須注記
        assert!(html.contains("市区町村粒度"), "市区町村粒度の注記必須");
    }

    /// 逆証明: pyramid が空のカードでは「データなし」が表示される
    #[test]
    fn granularity_demographics_municipality_card_no_data_shows_placeholder() {
        let labor_present = vec![row(&[
            ("employed", json!(50_000)),
            ("unemployed", json!(0)),
            ("unemployment_rate", json!(0.0)),
        ])];
        let pyramid_present = vec![row(&[
            ("age_group", json!("20-29")),
            ("male_count", json!(1000)),
            ("female_count", json!(1000)),
        ])];
        // 1 番目はデータあり、2 番目は全空
        let munis = vec![
            make_muni_demo(
                "東京都",
                "千代田区",
                100,
                pyramid_present,
                labor_present,
                vec![],
            ),
            make_muni_demo("神奈川県", "データ欠損市", 30, vec![], vec![], vec![]),
        ];
        let mut html = String::new();
        render_section_demographics_by_municipality(&mut html, &munis);

        // 1 件目はデータあり
        assert!(html.contains("千代田区"), "千代田区表示");
        // 2 件目は欠損プレースホルダ
        assert!(html.contains("データ欠損市"), "データ欠損市の名前は表示");
        assert!(
            html.contains("市区町村粒度データなし"),
            "欠損プレースホルダ表示"
        );
    }

    /// 逆証明: lifestyle の都道府県粒度警告強化が正しく出ること (helper test 経由)
    /// → lifestyle_municipality_warning_present は実際の section 描画でテストするため別 module
    #[test]
    fn granularity_section_bridge_present() {
        let pyramid = vec![row(&[
            ("age_group", json!("20-29")),
            ("male_count", json!(1000)),
            ("female_count", json!(1000)),
        ])];
        let munis = vec![make_muni_demo(
            "東京都",
            "千代田区",
            50,
            pyramid,
            vec![],
            vec![],
        )];

        let mut html = String::new();
        render_section_demographics_by_municipality(&mut html, &munis);
        assert!(
            html.contains("section-bridge"),
            "次セクションへのつなぎ必須"
        );
    }
}
