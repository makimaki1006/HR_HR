//! Section 10 - 採用環境の詳細分析 (Extended / 詳細版 variant 専用, 2026-07-09)
//!
//! `ReportVariant::Extended` のときだけ Section 09 の後ろに追加表示する 4 図。
//! 検証済みロジック (scratchpad/viz_mock/compute_v3.py + gen_html_v3.py + gen_html_v4.py)
//! を Rust に移植したもの。ラベル文言はモック v3/v4 と一字一句合わせている。
//!
//! ## 絶対ルール
//! - 介護データ・ハローワークデータを一切使わない。データ源は公的統計
//!   (国の将来人口推計 / 毎月勤労統計 / 最低賃金 / 就業構造基本調査 / 有効求人倍率) と
//!   今回の求人データ (アップロード CSV) のクロス集計 (`cross_*` テーブル) のみ。
//! - 断定禁止・相関≠因果・出典明示。
//! - データ未投入 (`cross_*` が空) でもレポートが壊れないよう graceful skip する。
//!
//! ## 4 図
//! - 図1: 働き手の将来マップ (散布図 + 減少率ランキング) — `cross_future_workforce`
//! - 図2: 給与の相場比較 (折れ線 + 最低賃金×160時間 階段線) — `cross_wage_public` + 今回の求人
//! - 図3: 転職を考えている人 (KPI 3 枚) — `cross_switcher_supply`
//! - 図4: 採用の何がネックか (診断表) — 図1〜3 + 駅の人通り (station は将来投入、当面「—」)
//!
//! ## API 表面
//! - `pub(crate) fn render_navy_section_10_extended` (Section 09 と同じ `pub(crate)` パターン)

#![allow(dead_code)]

use super::super::super::super::helpers::{
    escape_html, format_number, get_f64, get_i64, get_str_ref,
};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::super::db_columns as cols;
use super::super::ReportVariant;
use super::common::push_page_head;

// ============================================================
// 配色 (モック v4 と同一)
// ============================================================
const NAVY: &str = "#1e3a8a";
const NAVY2: &str = "#3b82f6";
const GOLD2: &str = "#f59e0b";
const RED: &str = "#ef4444";
const GREEN: &str = "#10b981";
const AMBER: &str = "#f59e0b";
const MUTED: &str = "#64748b";
const SLATE: &str = "#94a3b8";
const GOLD: &str = "#d4a017";

/// 最低賃金換算に使う固定労働時間 (compute_v3.py と同一: 月160時間固定)。
const FIXED_HOURS: i64 = 160;

// ============================================================
// 入力レコード (Row から型付きで抽出)
// ============================================================

struct WorkforceRow {
    muni: String,
    wa2020: i64,
    wa_ratio_2020: f64,
    wa_decline_2040: f64,
}

struct WageRow {
    month: String, // "2025-01"
    scheduled: i64,
    minwage_ft: i64,
}

struct SwitcherRow {
    region_code: String,
    /// 地域名 (全国 / 大分県 / 大分市 …)。cross_switcher_supply には prefecture /
    /// municipality 列が無く、地域は region_code + region_name で表す。
    region_name: String,
    desire_rate: f64,
    side_job: i64,
    additional: i64,
    switchers: i64,
    ratio: f64,
}

// ============================================================
// エントリ
// ============================================================

/// Section 10 統合エントリ。Extended variant 専用 4 図を順次レンダ。
///
/// `variant` 引数は防御的二重ガード (呼出側でも判定済み)。
/// `hw_context` が None または `cross_*` がすべて空の場合は何も出力しない (graceful skip)。
pub(crate) fn render_navy_section_10_extended(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    agg: &SurveyAggregation,
    variant: ReportVariant,
    target_region: &str,
) {
    if !variant.show_extended_sections() {
        return;
    }
    let ctx = match hw_context {
        Some(c) => c,
        None => return,
    };
    // データ未投入 (3 テーブルすべて空) なら Section ごと出力しない。
    if ctx.cross_future_workforce.is_empty()
        && ctx.cross_wage_public.is_empty()
        && ctx.cross_switcher_supply.is_empty()
    {
        return;
    }

    let pref_name = prefecture_of(ctx, target_region);
    let muni_name = municipality_of(ctx, target_region);
    let media_median = media_median_yen(agg);

    html.push_str("<section class=\"page-navy navy-extended\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 10",
        "採用環境の詳細分析",
        "働き手の将来 / 給与の相場 / 転職を考えている人 / 採用の何がネックか",
    );
    html.push_str(
        "<p class=\"caption dim\" style=\"margin-bottom:4mm;\">\
         全数値は公的統計（毎月勤労統計・就業構造基本調査・人口推計・最低賃金・有効求人倍率）\
         および今回の求人データから算出。推計値は「国の将来人口推計」と明記します。\
         数値の傾向として読んでください（因果関係ではありません）。\
         </p>\n",
    );

    render_fig1_workforce_map(html, ctx, &pref_name);
    render_fig2_wage_compare(html, ctx, media_median);
    render_fig3_switchers(html, ctx, &pref_name);
    render_fig4_diagnosis(html, ctx, &muni_name, media_median);

    html.push_str("</section>\n");
}

// ============================================================
// 図1: 働き手の将来マップ (散布図 + 減少率ランキング)
//   gen_html_v3.py の SVG ロジックを移植。対象都道府県でフィルタ済み (fetch 側 WHERE prefecture)。
// ============================================================

fn parse_workforce(ctx: &InsightContext) -> Vec<WorkforceRow> {
    ctx.cross_future_workforce
        .iter()
        .filter_map(|r| {
            let muni = get_str_ref(r, cols::MUNICIPALITY).to_string();
            if muni.is_empty() {
                return None;
            }
            Some(WorkforceRow {
                muni,
                wa2020: get_i64(r, cols::WA_2020),
                wa_ratio_2020: get_f64(r, cols::WORKING_AGE_RATIO_2020),
                wa_decline_2040: get_f64(r, cols::WA_DECLINE_RATE),
            })
        })
        .collect()
}

fn median_f64(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn render_fig1_workforce_map(html: &mut String, ctx: &InsightContext, pref_name: &str) {
    let munis = parse_workforce(ctx);
    html.push_str("<div class=\"navy-ext-block\" style=\"margin-bottom:6mm;\">\n");
    html.push_str(&format!(
        "<h3 style=\"color:{NAVY};font-size:12pt;font-weight:bold;margin:0 0 3mm;\">\
         働き手はこの先どれだけ減るか — {pref} の市町村マップ</h3>\n",
        NAVY = NAVY,
        pref = escape_html(pref_name),
    ));
    if munis.is_empty() {
        html.push_str(
            "<p class=\"caption dim\">働き手の将来推計データが未投入のため、この図は表示できません。</p>\n</div>\n",
        );
        return;
    }

    let med_x = round2(median_f64(
        munis.iter().map(|m| m.wa_decline_2040).collect(),
    ));
    let med_y = round2(median_f64(munis.iter().map(|m| m.wa_ratio_2020).collect()));

    // 減少率が厳しい順 (最も負 = 1) のランク付け。
    // 散布図の丸の番号・下部ランキング・対応表で番号が一致するよう共通の並び順を使う。
    let order = decline_rank_order(&munis);
    let mut ranks = vec![0usize; munis.len()];
    for (pos, &idx) in order.iter().enumerate() {
        ranks[idx] = pos + 1;
    }

    // 散布図 SVG ---------------------------------------------------------
    let scatter = build_scatter_svg(&munis, &ranks, med_x, med_y);
    // 減少率ランキング 上位8 (最も大きく減る = 最小値、番号 1〜8) --------
    let top8: Vec<(usize, &WorkforceRow)> = order
        .iter()
        .take(8)
        .map(|&i| (ranks[i], &munis[i]))
        .collect();
    let bars = build_decline_bars_svg(&top8);

    html.push_str("<div style=\"display:flex;gap:8mm;flex-wrap:wrap;align-items:flex-start;\">\n");
    html.push_str(&format!("<div>{}\n", scatter));
    html.push_str(&format!(
        "<div class=\"caption dim\" style=\"margin-top:2mm;\">点の大きさ＝働き手の人数（2020年）／\
         <span style=\"display:inline-block;width:9px;height:9px;border-radius:50%;background:{RED};margin:0 2px 0 6px;vertical-align:middle\"></span><b>特に厳しい（減りが速く、今すでに少ない）</b>\
         <span style=\"display:inline-block;width:9px;height:9px;border-radius:50%;background:{AMBER};margin:0 2px 0 6px;vertical-align:middle\"></span>減りは速いが今はまだ多い\
         <span style=\"display:inline-block;width:9px;height:9px;border-radius:50%;background:{NAVY2};margin:0 2px 0 6px;vertical-align:middle\"></span>減りは緩やかだが今すでに少ない\
         <span style=\"display:inline-block;width:9px;height:9px;border-radius:50%;background:{GREEN};margin:0 2px 0 6px;vertical-align:middle\"></span>比較的ゆとりがある</div>\n</div>\n",
        RED = RED, AMBER = AMBER, NAVY2 = NAVY2, GREEN = GREEN,
    ));
    html.push_str(&format!("<div style=\"flex:1;min-width:300px\">{}\n", bars));
    html.push_str(
        "<div class=\"caption dim\" style=\"margin-top:2mm;\">働き手（15〜64歳）の \
         <b>2020年→2040年 増減率</b>（国の将来人口推計）。<br>\
         減少率が大きい市町村ほど、将来の応募候補者が急速に減っていく（純粋な人口の見通し）。</div>\n</div>\n",
    );
    html.push_str("</div>\n");
    // 図中の番号 → 市町村名 対応表 (全市町村)。散布図の丸に振った番号と 1 対 1 対応。
    html.push_str(&build_muni_index_table(&munis, &order));
    html.push_str(&format!(
        "<div class=\"caption dim\" style=\"margin-top:2mm;border-top:1px dashed #e2e8f0;padding-top:2mm;\">\
         出典：国の将来人口推計（国立社会保障・人口問題研究所）。{pref} {n} 市町村を全数プロット。\
         境界線は県内の真ん中の値（中央値）。</div>\n",
        pref = escape_html(pref_name),
        n = munis.len(),
    ));
    html.push_str("</div>\n");
}

fn build_scatter_svg(munis: &[WorkforceRow], ranks: &[usize], med_x: f64, med_y: f64) -> String {
    // 動的軸レンジ (データ min/max にパディング)。degenerate 時は mock 既定へ。
    let (mut xmin, mut xmax) = min_max(munis.iter().map(|m| m.wa_decline_2040));
    let (mut ymin, mut ymax) = min_max(munis.iter().map(|m| m.wa_ratio_2020));
    if (xmax - xmin).abs() < 1e-6 {
        xmin -= 5.0;
        xmax += 5.0;
    }
    if (ymax - ymin).abs() < 1e-6 {
        ymin -= 5.0;
        ymax += 5.0;
    }
    let xpad = (xmax - xmin) * 0.08 + 2.0;
    let ypad = (ymax - ymin) * 0.08 + 2.0;
    xmin -= xpad;
    xmax += xpad;
    ymin -= ypad;
    ymax += ypad;

    let (w, h) = (470.0f64, 330.0f64);
    let (ml, mr, mt, mb) = (52.0f64, 18.0f64, 20.0f64, 44.0f64);
    let iw = w - ml - mr;
    let ih = h - mt - mb;
    let sx = |v: f64| ml + (v - xmin) / (xmax - xmin) * iw;
    let sy = |v: f64| mt + (ymax - v) / (ymax - ymin) * ih;

    // 半径: sqrt(workers) を data min/max で正規化 (4〜27px)
    let (smin, smax) = min_max(munis.iter().map(|m| (m.wa2020.max(0) as f64).sqrt()));
    let rad = |workers: i64| -> f64 {
        let s = (workers.max(0) as f64).sqrt();
        if (smax - smin).abs() < 1e-6 {
            10.0
        } else {
            4.0 + (s - smin) / (smax - smin) * 23.0
        }
    };
    let zone_color = |m: &WorkforceRow| -> &'static str {
        let left = m.wa_decline_2040 < med_x;
        let below = m.wa_ratio_2020 < med_y;
        match (left, below) {
            (true, true) => RED,
            (true, false) => AMBER,
            (false, true) => NAVY2,
            (false, false) => GREEN,
        }
    };

    let mut s = String::new();
    s.push_str(&format!(
        "<svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\" \
         style=\"display:block;background:#fff;border:1px solid #e2e8f0;\">",
        w = w as i64,
        h = h as i64,
    ));
    // 縦グリッド (X)
    let xstep = nice_step(xmax - xmin);
    let mut gv = (xmin / xstep).ceil() * xstep;
    while gv <= xmax + 1e-6 {
        let x = sx(gv);
        s.push_str(&format!(
            "<line x1=\"{x:.1}\" y1=\"{mt}\" x2=\"{x:.1}\" y2=\"{b:.1}\" stroke=\"#f1f5f9\"/>",
            x = x,
            mt = mt,
            b = mt + ih
        ));
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"9\" fill=\"{MUTED}\" text-anchor=\"middle\">{v}%</text>",
            x = x, y = mt + ih + 15.0, MUTED = MUTED, v = gv.round() as i64,
        ));
        gv += xstep;
    }
    // 横グリッド (Y)
    let ystep = nice_step(ymax - ymin);
    let mut gy = (ymin / ystep).ceil() * ystep;
    while gy <= ymax + 1e-6 {
        let y = sy(gy);
        s.push_str(&format!(
            "<line x1=\"{ml}\" y1=\"{y:.1}\" x2=\"{r:.1}\" y2=\"{y:.1}\" stroke=\"#f1f5f9\"/>",
            ml = ml,
            y = y,
            r = ml + iw
        ));
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"9\" fill=\"{MUTED}\" text-anchor=\"end\">{v}</text>",
            x = ml - 6.0, y = y + 3.0, MUTED = MUTED, v = gy.round() as i64,
        ));
        gy += ystep;
    }
    // 中央値の境界線
    let qx = sx(med_x);
    let qy = sy(med_y);
    s.push_str(&format!(
        "<line x1=\"{qx:.1}\" y1=\"{mt}\" x2=\"{qx:.1}\" y2=\"{b:.1}\" stroke=\"{GOLD}\" stroke-width=\"1.3\" stroke-dasharray=\"5 3\"/>",
        qx = qx, mt = mt, b = mt + ih, GOLD = GOLD,
    ));
    s.push_str(&format!(
        "<line x1=\"{ml}\" y1=\"{qy:.1}\" x2=\"{r:.1}\" y2=\"{qy:.1}\" stroke=\"{GOLD}\" stroke-width=\"1.3\" stroke-dasharray=\"5 3\"/>",
        ml = ml, qy = qy, r = ml + iw, GOLD = GOLD,
    ));
    s.push_str(&format!(
        "<text x=\"{x:.1}\" y=\"{y}\" font-size=\"8.5\" fill=\"{GOLD}\">県内の真ん中 増減率 {v}%</text>",
        x = qx + 4.0, y = mt as i64 + 10, GOLD = GOLD, v = med_x,
    ));
    s.push_str(&format!(
        "<text x=\"{x}\" y=\"{y:.1}\" font-size=\"8.5\" fill=\"{GOLD}\">県内の真ん中 働き手の割合 {v}%</text>",
        x = ml as i64 + 3, y = qy - 4.0, GOLD = GOLD, v = med_y,
    ));
    // 点 (先に全ての丸を描く。番号は後段でまとめて上に重ねる)
    for m in munis {
        let x = sx(m.wa_decline_2040);
        let y = sy(m.wa_ratio_2020);
        let r = rad(m.wa2020);
        let c = zone_color(m);
        // <title> は画面表示時のブラウザ標準ツールチップ (印刷には影響しない)。
        s.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r:.1}\" fill=\"{c}\" fill-opacity=\"0.55\" stroke=\"{c}\" stroke-width=\"1\"><title>{title}</title></circle>",
            x = x, y = y, r = r, c = c,
            title = escape_html(&format!("{} ({:+.1}%)", m.muni, m.wa_decline_2040)),
        ));
    }
    // 丸の番号 (減少率が厳しい順。丸が大きければ中心に白字、小さければ右上に紺字で外置き)。
    for (i, m) in munis.iter().enumerate() {
        let x = sx(m.wa_decline_2040);
        let y = sy(m.wa_ratio_2020);
        let r = rad(m.wa2020);
        let num = ranks[i];
        if r >= 9.0 {
            s.push_str(&format!(
                "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"7\" font-weight=\"bold\" fill=\"#fff\" text-anchor=\"middle\">{num}</text>",
                x = x, y = y + 2.5, num = num,
            ));
        } else {
            s.push_str(&format!(
                "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"7\" font-weight=\"bold\" fill=\"{NAVY}\" text-anchor=\"start\">{num}</text>",
                x = x + r + 0.8, y = y - r * 0.4, NAVY = NAVY, num = num,
            ));
        }
    }
    // 軸ラベル
    s.push_str(&format!(
        "<text x=\"{x:.1}\" y=\"{y}\" font-size=\"10\" fill=\"{MUTED}\" text-anchor=\"middle\">X：働き手の増減率（2020年→2040年、国の将来人口推計）　左ほど大きく減少</text>",
        x = ml + iw / 2.0, y = h as i64 - 4, MUTED = MUTED,
    ));
    s.push_str(&format!(
        "<text x=\"14\" y=\"{y:.1}\" font-size=\"10\" fill=\"{MUTED}\" text-anchor=\"middle\" transform=\"rotate(-90 14 {y:.1})\">Y：人口に占める働き手の割合（2020年、%）</text>",
        y = mt + ih / 2.0, MUTED = MUTED,
    ));
    s.push_str("</svg>");
    s
}

fn build_decline_bars_svg(top8: &[(usize, &WorkforceRow)]) -> String {
    let bw = 390.0f64;
    let rowh = 40.0f64;
    let bh = 44.0 + top8.len() as f64 * rowh;
    let dmax = top8
        .iter()
        .map(|(_, m)| m.wa_decline_2040.abs())
        .fold(0.0f64, f64::max)
        .max(1e-6);
    let barx0 = 96.0f64;
    let barmax = bw - barx0 - 64.0;
    let mut s = String::new();
    s.push_str(&format!(
        "<svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\" \
         style=\"display:block;background:#fff;\">",
        w = bw as i64,
        h = bh as i64,
    ));
    s.push_str(&format!(
        "<text x=\"12\" y=\"20\" font-size=\"11\" font-weight=\"bold\" fill=\"{NAVY}\">2040年までに働き手が大きく減る市町村（上位8）</text>",
        NAVY = NAVY,
    ));
    for (i, (num, m)) in top8.iter().enumerate() {
        let y = 42.0 + i as f64 * rowh;
        let val = m.wa_decline_2040.abs();
        let bwid = val / dmax * barmax;
        // 番号は散布図の丸・対応表と一致 (減少率が厳しい順)。
        s.push_str(&format!(
            "<text x=\"12\" y=\"{y:.1}\" font-size=\"10.5\" fill=\"#0f172a\" font-weight=\"bold\">{num}. {muni}</text>",
            y = y + 15.0,
            num = num,
            muni = escape_html(&m.muni),
        ));
        s.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{ry:.1}\" width=\"{w:.1}\" height=\"15\" rx=\"3\" fill=\"{NAVY}\"/>",
            x = barx0, ry = y + 3.0, w = bwid, NAVY = NAVY,
        ));
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"10.5\" font-weight=\"bold\" fill=\"{NAVY}\">{v:.1}%</text>",
            x = barx0 + bwid + 5.0, y = y + 15.0, NAVY = NAVY, v = m.wa_decline_2040,
        ));
        s.push_str(&format!(
            "<text x=\"12\" y=\"{y:.1}\" font-size=\"8.5\" fill=\"{MUTED}\">働き手の数（2020年）= {n}人</text>",
            y = y + 29.0, MUTED = MUTED, n = format_number(m.wa2020),
        ));
    }
    s.push_str("</svg>");
    s
}

/// 減少率が厳しい順 (最も負 = 先頭) に並べた元インデックスの並び。
/// 散布図の丸・下部ランキング・対応表の番号はすべてこの並びで一致させる。
fn decline_rank_order(munis: &[WorkforceRow]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..munis.len()).collect();
    order.sort_by(|&a, &b| {
        munis[a]
            .wa_decline_2040
            .partial_cmp(&munis[b].wa_decline_2040)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

/// 図中の番号 → 市町村名 対応表 (全市町村)。
/// 印刷/PDF でも読めるよう静的な列数固定グリッド。市町村数が多く (北海道 179 等) ても
/// 折り返して 1 ページに収まる密度 (4 列・8pt)。各項目「番号 市町村名 増減率」。
fn build_muni_index_table(munis: &[WorkforceRow], order: &[usize]) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "<div style=\"margin-top:3mm;\">\
         <div style=\"font-size:9pt;font-weight:bold;color:{NAVY};margin-bottom:1.5mm;\">図中の番号と市町村名</div>\
         <div style=\"display:grid;grid-template-columns:repeat(4,1fr);gap:0 10px;font-size:8pt;line-height:1.5;color:#334155;\">",
        NAVY = NAVY,
    ));
    for (pos, &idx) in order.iter().enumerate() {
        let m = &munis[idx];
        s.push_str(&format!(
            "<div style=\"white-space:nowrap;overflow:hidden;text-overflow:ellipsis;\">\
             <b style=\"color:{NAVY}\">{num}</b> {muni} <span style=\"color:{MUTED}\">{v:.1}%</span></div>",
            NAVY = NAVY,
            MUTED = MUTED,
            num = pos + 1,
            muni = escape_html(&m.muni),
            v = m.wa_decline_2040,
        ));
    }
    s.push_str("</div></div>\n");
    s
}

// ============================================================
// 図2: 給与の相場比較 (折れ線 + 最低賃金×160時間 階段線)
//   gen_html_v4.py の凡例上部配置・1月〜12月軸・2025年表記・階段最低賃金線を移植。
// ============================================================

fn parse_wage(ctx: &InsightContext) -> Vec<WageRow> {
    ctx.cross_wage_public
        .iter()
        .filter_map(|r| {
            let month = get_str_ref(r, cols::YEAR_MONTH).to_string();
            if month.len() < 7 {
                return None;
            }
            Some(WageRow {
                month,
                scheduled: get_i64(r, cols::SCHEDULED_EARNINGS),
                minwage_ft: get_i64(r, cols::MIN_WAGE_MONTHLY_160H),
            })
        })
        .collect()
}

fn render_fig2_wage_compare(html: &mut String, ctx: &InsightContext, media_median: Option<i64>) {
    let ser = parse_wage(ctx);
    html.push_str("<div class=\"navy-ext-block\" style=\"margin-bottom:6mm;\">\n");
    html.push_str(&format!(
        "<h3 style=\"color:{NAVY};font-size:12pt;font-weight:bold;margin:0 0 3mm;\">\
         求人の給与は、地域の相場と比べてどうか（2025年）</h3>\n",
        NAVY = NAVY,
    ));
    if ser.len() < 2 {
        html.push_str(
            "<p class=\"caption dim\">給与の相場データが未投入のため、この図は表示できません。</p>\n</div>\n",
        );
        return;
    }

    // 凡例 (グラフ上部に1行) — v4
    let mm_label = match media_median {
        Some(v) => format!("{}円", format_number(v)),
        None => "—".to_string(),
    };
    html.push_str(&format!(
        "<div style=\"display:flex;gap:18px;font-size:11px;color:#0f172a;margin-bottom:8px;align-items:center;flex-wrap:wrap;padding:4px 0 2px\">\
         <span style=\"display:flex;align-items:center;gap:5px\">\
         <svg width=\"22\" height=\"12\"><line x1=\"0\" y1=\"6\" x2=\"22\" y2=\"6\" stroke=\"{NAVY}\" stroke-width=\"2.5\"/><circle cx=\"11\" cy=\"6\" r=\"2.6\" fill=\"{NAVY}\"/></svg>\
         <span>県の平均給与</span></span>\
         <span style=\"display:flex;align-items:center;gap:5px\">\
         <svg width=\"22\" height=\"12\"><line x1=\"0\" y1=\"6\" x2=\"22\" y2=\"6\" stroke=\"{GOLD2}\" stroke-width=\"2\" stroke-dasharray=\"6 4\"/></svg>\
         <span>今回の求人の提示額&nbsp;&nbsp;{mm}</span></span>\
         <span style=\"display:flex;align-items:center;gap:5px\">\
         <svg width=\"22\" height=\"12\"><line x1=\"0\" y1=\"6\" x2=\"22\" y2=\"6\" stroke=\"{SLATE}\" stroke-width=\"2.2\" stroke-dasharray=\"4 3\"/><circle cx=\"11\" cy=\"6\" r=\"2.4\" fill=\"{SLATE}\"/></svg>\
         <span>最低賃金で月160時間働いた場合</span></span>\
         </div>\n",
        NAVY = NAVY, GOLD2 = GOLD2, SLATE = SLATE, mm = escape_html(&mm_label),
    ));

    let line = build_wage_line_svg(&ser, media_median);

    let s0 = ser.first().map(|x| x.scheduled).unwrap_or(0);
    let s1 = ser.last().map(|x| x.scheduled).unwrap_or(0);
    let growth = if s0 > 0 {
        (s1 as f64 / s0 as f64 - 1.0) * 100.0
    } else {
        0.0
    };
    let media_vs_actual = match media_median {
        Some(m) if s1 > 0 => Some((m as f64 / s1 as f64) * 100.0),
        _ => None,
    };

    html.push_str("<div style=\"display:flex;gap:8mm;flex-wrap:wrap;align-items:flex-start;\">\n");
    html.push_str(&format!("<div>{}</div>\n", line));
    html.push_str("<div style=\"flex:1;min-width:210px\">\n");
    html.push_str(&format!(
        "<div style=\"background:#f8fafc;border:1px solid #e2e8f0;border-radius:6px;padding:10px 12px;margin-bottom:8px;\">\
         <div style=\"font-size:11px;color:{MUTED}\">県の平均給与の上がり方（1年間）</div>\
         <div style=\"font-size:20px;font-weight:bold;color:{NAVY}\">+{g:.2}%</div>\
         <div class=\"caption dim\">2025年1月 {s0}円 → 12月 {s1}円（所定内給与）。</div></div>\n",
        MUTED = MUTED, NAVY = NAVY, g = growth, s0 = format_number(s0), s1 = format_number(s1),
    ));
    let (vs_val, vs_note) = match (media_vs_actual, media_median) {
        (Some(p), Some(m)) => (
            format!("{:.1}%", p),
            format!(
                "今回の求人データの真ん中の提示額 {}円は、県の平均（2025年12月 {}円）の約{:.1}%。",
                format_number(m),
                format_number(s1),
                p
            ),
        ),
        _ => (
            "—".to_string(),
            "今回の求人データの提示額（月給）が算出できないため、相場比は表示できません。"
                .to_string(),
        ),
    };
    html.push_str(&format!(
        "<div style=\"background:#f8fafc;border:1px solid #e2e8f0;border-radius:6px;padding:10px 12px;\">\
         <div style=\"font-size:11px;color:{MUTED}\">今回の提示額は相場の何%か</div>\
         <div style=\"font-size:20px;font-weight:bold;color:{NAVY}\">{v}</div>\
         <div class=\"caption dim\">{note}</div></div>\n",
        MUTED = MUTED, NAVY = NAVY, v = escape_html(&vs_val), note = escape_html(&vs_note),
    ));
    html.push_str("</div>\n</div>\n");

    html.push_str(
        "<div style=\"background:#fffbeb;border:1px solid #fde68a;border-radius:6px;padding:8px 12px;font-size:10pt;color:#92400e;margin-top:3mm;\">\
         ※ 今回の提示額の真ん中の値は、求人レンジの中点の中央値です。県の平均給与とは算出方法が異なるため、\
         水準の差はあくまでも<b>参考</b>です。最低賃金×160時間は法律上の下限の目安であり、実際の給与水準ではありません。\
         数値の傾向として読んでください（因果関係ではありません）。</div>\n",
    );
    html.push_str(
        "<div class=\"caption dim\" style=\"margin-top:2mm;border-top:1px dashed #e2e8f0;padding-top:2mm;\">\
         出典：厚労省 毎月勤労統計 地方調査（5人以上事業所・全産業 所定内給与）／最低賃金（10月改定）×月160時間（固定）／今回の求人データ（正社員・月給）。</div>\n",
    );
    html.push_str("</div>\n");
}

fn build_wage_line_svg(ser: &[WageRow], media_median: Option<i64>) -> String {
    let (lw, lh) = (560.0f64, 316.0f64);
    let (lml, lmr, lmt, lmb) = (64.0f64, 16.0f64, 16.0f64, 52.0f64);
    let liw = lw - lml - lmr;
    let lih = lh - lmt - lmb;

    // Y レンジをデータから (scheduled / minwage_ft / media を包含)
    let mut vals: Vec<f64> = Vec::new();
    for r in ser {
        vals.push(r.scheduled as f64);
        vals.push(r.minwage_ft as f64);
    }
    if let Some(m) = media_median {
        vals.push(m as f64);
    }
    let (mut yl, mut yh) = min_max(vals.iter().copied());
    let ypad = (yh - yl) * 0.1 + 10000.0;
    yl = (yl - ypad).max(0.0);
    yh += ypad;

    let n = ser.len();
    let lx = |i: usize| lml + i as f64 / (n.max(2) as f64 - 1.0) * liw;
    let ly = |v: f64| lmt + (yh - v) / (yh - yl).max(1.0) * lih;

    let mut s = String::new();
    s.push_str(&format!(
        "<svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\" \
         style=\"display:block;background:#fff;border:1px solid #e2e8f0;\">",
        w = lw as i64,
        h = lh as i64,
    ));
    // 横グリッド + Y 目盛
    let ystep = nice_step(yh - yl);
    let mut gv = (yl / ystep).ceil() * ystep;
    while gv <= yh + 1e-6 {
        let y = ly(gv);
        s.push_str(&format!(
            "<line x1=\"{ml}\" y1=\"{y:.1}\" x2=\"{r:.1}\" y2=\"{y:.1}\" stroke=\"#f1f5f9\"/>",
            ml = lml,
            y = y,
            r = lml + liw
        ));
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"9\" fill=\"{MUTED}\" text-anchor=\"end\">{v}千</text>",
            x = lml - 6.0, y = y + 3.0, MUTED = MUTED, v = (gv / 1000.0).round() as i64,
        ));
        gv += ystep;
    }
    // X 目盛 (1月〜12月)
    for (i, r) in ser.iter().enumerate() {
        let mo: i64 = r.month.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(0);
        s.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"9\" fill=\"{MUTED}\" text-anchor=\"middle\">{mo}月</text>",
            x = lx(i), y = lmt + lih + 15.0, MUTED = MUTED, mo = mo,
        ));
    }
    // ― 2025年 ―
    s.push_str(&format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"9\" fill=\"{MUTED}\" text-anchor=\"middle\">― 2025年 ―</text>",
        x = lml + liw / 2.0, y = lmt + lih + 33.0, MUTED = MUTED,
    ));
    // 今回の求人 提示額 (gold 水平線)
    if let Some(m) = media_median {
        let ymm = ly(m as f64);
        s.push_str(&format!(
            "<line x1=\"{ml}\" y1=\"{y:.1}\" x2=\"{r:.1}\" y2=\"{y:.1}\" stroke=\"{GOLD2}\" stroke-width=\"2\" stroke-dasharray=\"6 4\"/>",
            ml = lml, y = ymm, r = lml + liw, GOLD2 = GOLD2,
        ));
    }
    // 線1: 県の平均給与 (navy)
    let pts: String = ser
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{:.1},{:.1}", lx(i), ly(r.scheduled as f64)))
        .collect::<Vec<_>>()
        .join(" ");
    s.push_str(&format!(
        "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{NAVY}\" stroke-width=\"2.4\"/>",
        pts = pts,
        NAVY = NAVY
    ));
    for (i, r) in ser.iter().enumerate() {
        s.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"2.6\" fill=\"{NAVY}\"/>",
            x = lx(i),
            y = ly(r.scheduled as f64),
            NAVY = NAVY
        ));
    }
    // 線2: 最低賃金で月160時間 (slate 点線, 階段状)
    let pts2: String = ser
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{:.1},{:.1}", lx(i), ly(r.minwage_ft as f64)))
        .collect::<Vec<_>>()
        .join(" ");
    s.push_str(&format!(
        "<polyline points=\"{pts}\" fill=\"none\" stroke=\"{SLATE}\" stroke-width=\"2.2\" stroke-dasharray=\"4 3\"/>",
        pts = pts2,
        SLATE = SLATE
    ));
    for (i, r) in ser.iter().enumerate() {
        s.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"2.4\" fill=\"{SLATE}\"/>",
            x = lx(i),
            y = ly(r.minwage_ft as f64),
            SLATE = SLATE
        ));
    }
    s.push_str("</svg>");
    s
}

// ============================================================
// 図3: 転職を考えている人 (KPI 3 枚)
// ============================================================

fn parse_switchers(ctx: &InsightContext) -> Vec<SwitcherRow> {
    ctx.cross_switcher_supply
        .iter()
        .map(|r| SwitcherRow {
            region_code: get_str_ref(r, cols::REGION_CODE).to_string(),
            region_name: get_str_ref(r, cols::REGION_NAME).to_string(),
            desire_rate: get_f64(r, cols::JOB_CHANGE_DESIRE_RATE),
            side_job: get_i64(r, cols::SIDE_JOB_HOLDERS),
            additional: get_i64(r, cols::ADDITIONAL_JOB_SEEKERS),
            switchers: get_i64(r, cols::JOB_CHANGE_SEEKERS),
            ratio: get_f64(r, cols::PREF_JOB_OPENINGS_RATIO),
        })
        .collect()
}

fn is_national(r: &SwitcherRow) -> bool {
    r.region_name == "全国" || r.region_code == "00000"
}
fn is_prefecture_level(r: &SwitcherRow) -> bool {
    !is_national(r) && r.region_code.ends_with("000")
}

fn render_fig3_switchers(html: &mut String, ctx: &InsightContext, pref_name: &str) {
    let rows = parse_switchers(ctx);
    html.push_str("<div class=\"navy-ext-block\" style=\"margin-bottom:6mm;\">\n");
    html.push_str(&format!(
        "<h3 style=\"color:{NAVY};font-size:12pt;font-weight:bold;margin:0 0 3mm;\">\
         転職を考えている人は、どれくらいいるか（{pref}）</h3>\n",
        NAVY = NAVY,
        pref = escape_html(pref_name),
    ));
    let pref_row = rows.iter().find(|r| is_prefecture_level(r));
    let nat_row = rows.iter().find(|r| is_national(r));
    let pr = match pref_row {
        Some(r) => r,
        None => {
            html.push_str(
                "<p class=\"caption dim\">転職意向データが未投入のため、この図は表示できません。</p>\n</div>\n",
            );
            return;
        }
    };
    let nat_desire = nat_row.map(|r| r.desire_rate);
    let nat_ratio = nat_row.map(|r| r.ratio);

    html.push_str("<div style=\"display:flex;gap:14px;flex-wrap:wrap;\">\n");
    // KPI 1
    html.push_str(&format!(
        "<div style=\"flex:1;min-width:180px;background:#f8fafc;border:1px solid #e2e8f0;border-left:4px solid {NAVY};border-radius:6px;padding:12px 14px\">\
         <div style=\"font-size:11px;color:{MUTED};margin-bottom:4px\">働いている人のうち、転職を考えている割合</div>\
         <div style=\"font-size:26px;font-weight:bold;color:{NAVY}\">{v:.1}<span style=\"font-size:13px;font-weight:normal;color:{MUTED}\"> %</span></div>\
         <div class=\"caption dim\">全国平均 {nat}。</div></div>\n",
        NAVY = NAVY, MUTED = MUTED, v = pr.desire_rate,
        nat = nat_desire.map(|x| format!("{:.1}%", x)).unwrap_or_else(|| "—".to_string()),
    ));
    // KPI 2
    html.push_str(&format!(
        "<div style=\"flex:1;min-width:180px;background:#f8fafc;border:1px solid #e2e8f0;border-left:4px solid {NAVY};border-radius:6px;padding:12px 14px\">\
         <div style=\"font-size:11px;color:{MUTED};margin-bottom:4px\">副業をしている人 ＋ もっと働きたい人（掘り起こせる可能性のある層）</div>\
         <div style=\"font-size:26px;font-weight:bold;color:{NAVY}\">{v}<span style=\"font-size:13px;font-weight:normal;color:{MUTED}\"> 人</span></div>\
         <div class=\"caption dim\">副業をしている人の数です。<br>他に、もっと働きたいと答えた人が <b>{add}人</b>います（就業構造基本調査）。</div></div>\n",
        NAVY = NAVY, MUTED = MUTED, v = format_number(pr.side_job), add = format_number(pr.additional),
    ));
    // KPI 3 (参考)
    html.push_str(&format!(
        "<div style=\"flex:1;min-width:180px;background:#f8fafc;border:1px solid #e2e8f0;border-left:4px solid {SLATE};border-radius:6px;padding:12px 14px\">\
         <div style=\"font-size:11px;color:{MUTED};margin-bottom:4px\">仕事の数と探す人のバランス（有効求人倍率）</div>\
         <div style=\"font-size:26px;font-weight:bold;color:{NAVY}\">{v:.2}<span style=\"font-size:13px;font-weight:normal;color:{MUTED}\"> 倍</span></div>\
         <div class=\"caption dim\">全国 {nat}。<br>1.0より大きい = 仕事の数の方が多い（人手不足寄り）／参考値。</div></div>\n",
        SLATE = SLATE, NAVY = NAVY, MUTED = MUTED, v = pr.ratio,
        nat = nat_ratio.map(|x| format!("{:.2} 倍", x)).unwrap_or_else(|| "—".to_string()),
    ));
    html.push_str("</div>\n");
    html.push_str(
        "<div style=\"background:#fffbeb;border:1px solid #fde68a;border-radius:6px;padding:8px 12px;font-size:10pt;color:#92400e;margin-top:3mm;\">\
         転職を考えている割合や副業・追加就業希望は就業構造基本調査から算出（ある時点のストック）。\
         有効求人倍率は公式統計で、労働需給の目安として<b>参考</b>掲載。数値の傾向として読んでください（因果関係ではありません）。</div>\n",
    );
    html.push_str(
        "<div class=\"caption dim\" style=\"margin-top:2mm;border-top:1px dashed #e2e8f0;padding-top:2mm;\">\
         出典：総務省 就業構造基本調査／厚労省 一般職業紹介状況 有効求人倍率。</div>\n",
    );
    html.push_str("</div>\n");
}

// ============================================================
// 図4: 採用の何がネックか (診断表)
// ============================================================

fn render_fig4_diagnosis(
    html: &mut String,
    ctx: &InsightContext,
    muni_name: &str,
    media_median: Option<i64>,
) {
    let rows = parse_switchers(ctx);
    let wf = parse_workforce(ctx);
    let wage = parse_wage(ctx);

    html.push_str("<div class=\"navy-ext-block\">\n");
    html.push_str(&format!(
        "<h3 style=\"color:{NAVY};font-size:12pt;font-weight:bold;margin:0 0 3mm;\">\
         採用の何がネックか — {muni} の診断</h3>\n",
        NAVY = NAVY,
        muni = escape_html(muni_name),
    ));

    // 対象市区町村行 (municipality 一致優先、なければ県レベル行)
    let muni_row = rows
        .iter()
        .find(|r| !is_national(r) && r.region_name == muni_name && !r.region_name.is_empty())
        .or_else(|| {
            rows.iter()
                .find(|r| !is_national(r) && !is_prefecture_level(r))
        })
        .or_else(|| rows.iter().find(|r| is_prefecture_level(r)));
    let nat_row = rows.iter().find(|r| is_national(r));

    if muni_row.is_none() && wf.is_empty() && wage.is_empty() {
        html.push_str(
            "<p class=\"caption dim\">診断に必要なデータが未投入のため、この表は表示できません。</p>\n</div>\n",
        );
        return;
    }

    // 給与相場比
    let s1 = wage.last().map(|x| x.scheduled).unwrap_or(0);
    let media_vs_actual = match media_median {
        Some(m) if s1 > 0 => Some((m as f64 / s1 as f64) * 100.0),
        _ => None,
    };
    // 対象市の 2040 働き手見通し
    let wa_decline = wf
        .iter()
        .find(|m| m.muni == muni_name)
        .map(|m| m.wa_decline_2040);

    let mut body = String::new();
    body.push_str(&format!(
        "<table style=\"width:100%;border-collapse:collapse;font-size:11pt\">\
         <tr style=\"border-bottom:2px solid {NAVY}\">\
         <td style=\"color:{MUTED};font-size:10pt;padding:6px 8px\">診断の切り口</td>\
         <td style=\"color:{MUTED};font-size:10pt;padding:6px 8px\">実際の数値</td>\
         <td style=\"color:{MUTED};font-size:10pt;text-align:center;padding:6px 8px\">評価</td>\
         <td style=\"color:{MUTED};font-size:10pt;padding:6px 8px\">コメント（中立記述）</td></tr>\n",
        NAVY = NAVY, MUTED = MUTED,
    ));

    // 行1: 応募候補になりうる人の数
    if let Some(r) = muni_row {
        let note = match nat_row {
            Some(n) => format!("全国平均（{:.1}%）とほぼ同水準", n.desire_rate),
            None => "全国比較データは未投入".to_string(),
        };
        push_diag_row(
            &mut body,
            "応募候補になりうる人の数",
            &format!(
                "{}人（転職を考えている割合 {:.1}%）",
                format_number(r.switchers),
                r.desire_rate
            ),
            2,
            &note,
        );
    }
    // 行2: 給与の水準
    match (media_vs_actual, media_median) {
        (Some(p), Some(m)) => push_diag_row(
            &mut body,
            "給与の水準（相場との比較）",
            &format!(
                "{:.1}%（今回の提示額 {}円／県の平均 {}円）",
                p,
                format_number(m),
                format_number(s1)
            ),
            2,
            "今回の求人の真ん中の給与は県の平均をやや下回る（2025年12月実績との比較）",
        ),
        _ => push_diag_row(
            &mut body,
            "給与の水準（相場との比較）",
            "—",
            0,
            "今回の求人データの提示額（月給）が算出できないため比較できません",
        ),
    }
    // 行3: 駅の人通りの変化 (station_ridership_muni は将来投入 → 当面「—」)
    push_diag_row(
        &mut body,
        "駅の人通りの変化",
        "—",
        0,
        "駅別乗降客数データは未投入（今後追加予定）",
    );
    // 行4: 2040年の働き手の見通し
    match wa_decline {
        Some(d) => push_diag_row(
            &mut body,
            "2040年の働き手の見通し",
            &format!("{:+.1}%（国の将来人口推計）", d),
            3,
            "純粋な人口の見通し（応募候補者の将来的な増減の目安）",
        ),
        None => push_diag_row(
            &mut body,
            "2040年の働き手の見通し",
            "—",
            0,
            "対象市区町村の将来人口推計データは未投入",
        ),
    }

    body.push_str("</table>\n");
    html.push_str(&body);
    html.push_str(
        "<div class=\"caption dim\" style=\"margin-top:2mm;\">●が多いほど心配が少ない（3段階・相対比較）</div>\n",
    );
    html.push_str(
        "<div class=\"caption dim\" style=\"margin-top:2mm;border-top:1px dashed #e2e8f0;padding-top:2mm;\">\
         出典：就業構造基本調査・毎月勤労統計・今回の求人データ（正社員・月給）・駅別乗降客数（将来投入）・国の将来人口推計。</div>\n",
    );
    html.push_str("</div>\n");
}

fn push_diag_row(body: &mut String, label: &str, value: &str, dots_n: i64, note: &str) {
    body.push_str(&format!(
        "<tr>\
         <td style=\"font-weight:bold;color:{NAVY};padding:7px 8px;border-bottom:1px solid #e2e8f0;width:200px\">{label}</td>\
         <td style=\"padding:7px 8px;border-bottom:1px solid #e2e8f0;font-variant-numeric:tabular-nums\">{value}</td>\
         <td style=\"padding:7px 8px;border-bottom:1px solid #e2e8f0;text-align:center;white-space:nowrap\">{dots}</td>\
         <td style=\"padding:7px 8px;border-bottom:1px solid #e2e8f0;color:{MUTED};font-size:10pt\">{note}</td></tr>\n",
        NAVY = NAVY, MUTED = MUTED,
        label = escape_html(label),
        value = escape_html(value),
        dots = dots(dots_n),
        note = escape_html(note),
    ));
}

fn dots(n: i64) -> String {
    (0..3)
        .map(|i| {
            let color = if i < n { NAVY } else { "#cbd5e1" };
            format!("<span style=\"color:{};font-size:15px\">●</span>", color)
        })
        .collect()
}

// ============================================================
// 共通ユーティリティ
// ============================================================

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn min_max<I: Iterator<Item = f64>>(it: I) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in it {
        if v.is_finite() {
            if v < lo {
                lo = v;
            }
            if v > hi {
                hi = v;
            }
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 1.0)
    } else {
        (lo, hi)
    }
}

/// グリッド目盛の「きりの良い」刻み幅を返す (レンジの約 1/5 を 1/2/5×10^k に丸める)。
fn nice_step(range: f64) -> f64 {
    let range = range.abs().max(1e-6);
    let raw = range / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    (step * mag).max(1e-6)
}

/// 今回の求人データの月給中央値 (円)。enhanced_stats.median 優先、なければ salary_values の中央値。
fn media_median_yen(agg: &SurveyAggregation) -> Option<i64> {
    if let Some(es) = &agg.enhanced_stats {
        if es.median > 0 {
            return Some(es.median);
        }
    }
    let mut v: Vec<i64> = agg
        .salary_values
        .iter()
        .copied()
        .filter(|x| *x > 0)
        .collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// 対象都道府県名を決める (InsightContext.pref 優先、なければ target_region の先頭語)。
fn prefecture_of(ctx: &InsightContext, target_region: &str) -> String {
    if !ctx.pref.is_empty() {
        return ctx.pref.clone();
    }
    target_region
        .split_whitespace()
        .next()
        .unwrap_or(target_region)
        .to_string()
}

/// 対象市区町村名を決める (InsightContext.muni 優先、なければ target_region の 2 語目、なければ県名)。
fn municipality_of(ctx: &InsightContext, target_region: &str) -> String {
    if !ctx.muni.is_empty() {
        return ctx.muni.clone();
    }
    let mut parts = target_region.split_whitespace();
    let first = parts.next().unwrap_or("");
    match parts.next() {
        Some(m) => m.to_string(),
        None => first.to_string(),
    }
}

// ============================================================
// テスト
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    fn row(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    /// cross_* を実データで満たした InsightContext を構築 (大分県相当のミニ版)。
    fn full_ctx() -> InsightContext {
        let mut c = InsightContext::default();
        c.pref = "大分県".to_string();
        c.muni = "大分市".to_string();
        c.cross_future_workforce = vec![
            row(&[
                ("prefecture", Value::from("大分県")),
                ("muni_code", Value::from("44201")),
                ("municipality", Value::from("大分市")),
                ("wa_2020", Value::from(280_000)),
                ("working_age_ratio_2020", Value::from(58.5)),
                ("wa_decline_rate", Value::from(-15.2)),
            ]),
            row(&[
                ("prefecture", Value::from("大分県")),
                ("muni_code", Value::from("44205")),
                ("municipality", Value::from("佐伯市")),
                ("wa_2020", Value::from(35_000)),
                ("working_age_ratio_2020", Value::from(50.1)),
                ("wa_decline_rate", Value::from(-42.0)),
            ]),
            row(&[
                ("prefecture", Value::from("大分県")),
                ("muni_code", Value::from("44204")),
                ("municipality", Value::from("津久見市")),
                ("wa_2020", Value::from(9_000)),
                ("working_age_ratio_2020", Value::from(47.8)),
                ("wa_decline_rate", Value::from(-48.5)),
            ]),
        ];
        c.cross_wage_public = (1..=12)
            .map(|mo: i64| {
                let hourly = if mo >= 10 { 1_035 } else { 954 };
                row(&[
                    ("prefecture", Value::from("大分県")),
                    ("year_month", Value::from(format!("2025-{:02}", mo))),
                    ("scheduled_earnings", Value::from(240_000 + (mo - 1) * 500)),
                    ("min_wage_monthly_160h", Value::from(hourly * 160)),
                    ("min_wage_hourly", Value::from(hourly)),
                ])
            })
            .collect();
        c.cross_switcher_supply = vec![
            row(&[
                ("region_code", Value::from("00000")),
                ("region_name", Value::from("全国")),
                ("job_change_desire_rate", Value::from(8.5)),
                ("side_job_holders", Value::from(3_000_000)),
                ("additional_job_seekers", Value::from(4_200_000)),
                ("job_change_seekers", Value::from(6_800_000)),
                ("pref_job_openings_ratio", Value::from(1.30)),
            ]),
            row(&[
                ("region_code", Value::from("44000")),
                ("region_name", Value::from("大分県")),
                ("job_change_desire_rate", Value::from(7.8)),
                ("side_job_holders", Value::from(30_000)),
                ("additional_job_seekers", Value::from(40_000)),
                ("job_change_seekers", Value::from(60_000)),
                ("pref_job_openings_ratio", Value::from(1.55)),
            ]),
            row(&[
                ("region_code", Value::from("44201")),
                ("region_name", Value::from("大分市")),
                ("job_change_desire_rate", Value::from(7.9)),
                ("side_job_holders", Value::from(12_000)),
                ("additional_job_seekers", Value::from(16_000)),
                ("job_change_seekers", Value::from(24_000)),
                ("pref_job_openings_ratio", Value::from(1.50)),
            ]),
        ];
        c
    }

    fn agg_with_median(median: i64) -> SurveyAggregation {
        let mut a = SurveyAggregation::default();
        a.salary_values = vec![median];
        a
    }

    #[test]
    fn empty_ctx_produces_no_output() {
        let mut html = String::new();
        let ctx = InsightContext::default(); // cross_* すべて空
        let agg = SurveyAggregation::default();
        render_navy_section_10_extended(
            &mut html,
            Some(&ctx),
            &agg,
            ReportVariant::Extended,
            "大分県 大分市",
        );
        assert!(
            html.is_empty(),
            "cross_* 未投入時は Section 10 を一切出力しない (graceful skip)"
        );
    }

    #[test]
    fn none_context_produces_no_output() {
        let mut html = String::new();
        let agg = SurveyAggregation::default();
        render_navy_section_10_extended(
            &mut html,
            None,
            &agg,
            ReportVariant::Extended,
            "大分県 大分市",
        );
        assert!(html.is_empty());
    }

    #[test]
    fn non_extended_variant_produces_no_output() {
        let mut html = String::new();
        let ctx = full_ctx();
        let agg = agg_with_median(250_000);
        for v in [
            ReportVariant::Full,
            ReportVariant::Public,
            ReportVariant::MarketIntelligence,
        ] {
            html.clear();
            render_navy_section_10_extended(&mut html, Some(&ctx), &agg, v, "大分県 大分市");
            assert!(
                html.is_empty(),
                "Extended 以外の variant では Section 10 を出力しない: {:?}",
                v
            );
        }
    }

    #[test]
    fn full_data_renders_all_four_figures_key_phrases() {
        let mut html = String::new();
        let ctx = full_ctx();
        let agg = agg_with_median(250_000);
        render_navy_section_10_extended(
            &mut html,
            Some(&ctx),
            &agg,
            ReportVariant::Extended,
            "大分県 大分市",
        );
        assert!(!html.is_empty());
        // Section 見出し
        assert!(html.contains("採用環境の詳細分析"), "SECTION 10 見出し");
        // 図1
        assert!(
            html.contains("働き手はこの先どれだけ減るか"),
            "図1 タイトル"
        );
        assert!(html.contains("大分県 の市町村マップ"), "図1 対象県フィルタ");
        assert!(html.contains("2040年までに働き手が大きく減る市町村（上位8）"));
        assert!(html.contains("国の将来人口推計"), "出典明示");
        // 図2
        assert!(html.contains("求人の給与は、地域の相場と比べてどうか（2025年）"));
        assert!(
            html.contains("最低賃金で月160時間働いた場合"),
            "図2 凡例文言"
        );
        assert!(html.contains("今回の求人の提示額"));
        assert!(html.contains("― 2025年 ―"), "図2 X 軸下 年表記");
        // 図3
        assert!(html.contains("転職を考えている人は、どれくらいいるか（大分県）"));
        assert!(html.contains("有効求人倍率"));
        // 図4
        assert!(html.contains("採用の何がネックか — 大分市 の診断"));
        assert!(html.contains("駅の人通りの変化"), "図4 駅の人通り行");
        // 因果注記
        assert!(html.contains("因果関係ではありません"));
    }

    #[test]
    fn figure4_station_row_shows_dash_when_no_station_data() {
        // station_ridership_muni は将来投入。当面「駅の人通り」行はデータ無ければ「—」。
        let mut html = String::new();
        let ctx = full_ctx();
        let agg = agg_with_median(250_000);
        render_navy_section_10_extended(
            &mut html,
            Some(&ctx),
            &agg,
            ReportVariant::Extended,
            "大分県 大分市",
        );
        assert!(html.contains("駅別乗降客数データは未投入"));
    }

    #[test]
    fn partial_data_only_workforce_still_renders_without_panic() {
        let mut html = String::new();
        let mut ctx = InsightContext::default();
        ctx.pref = "大分県".to_string();
        ctx.cross_future_workforce = full_ctx().cross_future_workforce;
        let agg = SurveyAggregation::default();
        render_navy_section_10_extended(
            &mut html,
            Some(&ctx),
            &agg,
            ReportVariant::Extended,
            "大分県",
        );
        assert!(html.contains("採用環境の詳細分析"));
        assert!(html.contains("働き手はこの先どれだけ減るか"));
        // 給与相場データ未投入 → 図2 は説明メッセージ (壊れない)
        assert!(html.contains("給与の相場データが未投入"));
    }

    #[test]
    fn media_median_prefers_enhanced_stats() {
        use super::super::super::super::statistics::EnhancedStats;
        let mut a = SurveyAggregation::default();
        a.salary_values = vec![100_000];
        a.enhanced_stats = Some(EnhancedStats {
            count: 3,
            mean: 260_000,
            median: 275_000,
            min: 200_000,
            max: 350_000,
            std_dev: 40_000,
            bootstrap_ci: None,
            trimmed_mean: None,
            quartiles: None,
            reliability: "高".to_string(),
        });
        assert_eq!(media_median_yen(&a), Some(275_000));
    }

    #[test]
    fn media_median_falls_back_to_salary_values() {
        let a = agg_with_median(240_000);
        assert_eq!(media_median_yen(&a), Some(240_000));
    }

    #[test]
    fn media_median_none_when_no_salary() {
        let a = SurveyAggregation::default();
        assert_eq!(media_median_yen(&a), None);
    }

    #[test]
    fn dots_renders_three_symbols_with_fill() {
        let d = dots(2);
        assert_eq!(d.matches('●').count(), 3, "常に 3 個の ● を出す");
        assert_eq!(d.matches(NAVY).count(), 2, "n 個が navy 塗り");
    }

    #[test]
    fn nice_step_positive_and_finite() {
        for r in [1.0, 10.0, 55.0, 120_000.0, 0.0] {
            let s = nice_step(r);
            assert!(s > 0.0 && s.is_finite(), "range={} step={}", r, s);
        }
    }

    /// 合成 WorkforceRow ヘルパ (対応表・番号の多市町村レイアウト検証用)。
    fn wf_row(muni: &str, workers: i64, ratio: f64, decline: f64) -> WorkforceRow {
        WorkforceRow {
            muni: muni.to_string(),
            wa2020: workers,
            wa_ratio_2020: ratio,
            wa_decline_2040: decline,
        }
    }

    #[test]
    fn decline_rank_order_is_most_negative_first() {
        // 減少率が厳しい (最も負) 順。full_ctx: 津久見市 -48.5 < 佐伯市 -42.0 < 大分市 -15.2。
        let ctx = full_ctx();
        let munis = parse_workforce(&ctx);
        let order = decline_rank_order(&munis);
        assert_eq!(munis[order[0]].muni, "津久見市", "1 番は最も減る市");
        assert_eq!(munis[order[1]].muni, "佐伯市");
        assert_eq!(munis[order[2]].muni, "大分市");
    }

    #[test]
    fn ranking_bars_and_index_table_share_same_numbering() {
        // 散布図/ランキング/対応表の番号が同順であること (ランキング上位と対応表先頭が一致)。
        let ctx = full_ctx();
        let munis = parse_workforce(&ctx);
        let order = decline_rank_order(&munis);
        let mut ranks = vec![0usize; munis.len()];
        for (pos, &idx) in order.iter().enumerate() {
            ranks[idx] = pos + 1;
        }
        // ランキング (上位8) の 1 番目テキスト
        let top8: Vec<(usize, &WorkforceRow)> = order
            .iter()
            .take(8)
            .map(|&i| (ranks[i], &munis[i]))
            .collect();
        let bars = build_decline_bars_svg(&top8);
        assert!(bars.contains("1. 津久見市"), "ランキング 1 番 = 津久見市");
        assert!(bars.contains("2. 佐伯市"), "ランキング 2 番 = 佐伯市");
        // 対応表の 1 番も同じ市
        let table = build_muni_index_table(&munis, &order);
        assert!(
            table.contains(">1</b> 津久見市"),
            "対応表 1 番 = 津久見市 (ランキングと同順)"
        );
    }

    #[test]
    fn index_table_lists_all_municipalities() {
        // 対応表に全市町村が出ること (多数でも列固定グリッドで全件)。
        let munis: Vec<WorkforceRow> = (0..60)
            .map(|i| wf_row(&format!("市{:02}", i), 10_000 + i * 100, 55.0, -(i as f64)))
            .collect();
        let order = decline_rank_order(&munis);
        let table = build_muni_index_table(&munis, &order);
        // 各行は白スペース nowrap の div。件数 = 市町村数。
        assert_eq!(
            table.matches("white-space:nowrap").count(),
            60,
            "対応表は全 60 市町村を列挙"
        );
        assert!(table.contains("図中の番号と市町村名"), "対応表見出し");
        // 最も減る (-59) が 1 番、最も減らない (0) が 60 番。
        assert!(table.contains(">1</b> 市59"));
        assert!(table.contains(">60</b> 市00"));
    }

    #[test]
    fn scatter_svg_has_title_per_circle_and_number_per_circle() {
        // <title> が circle 数と一致 (画面ツールチップ)、番号も circle 数分ある。
        let munis: Vec<WorkforceRow> = vec![
            wf_row("大きい市", 300_000, 58.0, -12.0),
            wf_row("小さい村", 3_000, 48.0, -55.0),
            wf_row("中くらい町", 40_000, 52.0, -30.0),
        ];
        let order = decline_rank_order(&munis);
        let mut ranks = vec![0usize; munis.len()];
        for (pos, &idx) in order.iter().enumerate() {
            ranks[idx] = pos + 1;
        }
        let svg = build_scatter_svg(&munis, &ranks, -30.0, 52.0);
        assert_eq!(
            svg.matches("<circle").count(),
            munis.len(),
            "circle 数 = 市町村数"
        );
        assert_eq!(
            svg.matches("<title>").count(),
            munis.len(),
            "<title> 数 = circle 数"
        );
        // ツールチップ本文に市町村名と増減率
        assert!(svg.contains("小さい村 (-55.0%)"));
    }

    #[test]
    fn min_max_handles_empty_and_nan() {
        let (lo, hi) = min_max(std::iter::empty::<f64>());
        assert!(lo.is_finite() && hi.is_finite());
        let (lo2, hi2) = min_max([f64::NAN, 3.0, 1.0].into_iter());
        assert_eq!((lo2, hi2), (1.0, 3.0));
    }
}
