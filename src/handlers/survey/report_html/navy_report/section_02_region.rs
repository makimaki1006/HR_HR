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
    // 2026-07-27: 表 2-D 市区町村化用。基準 (選択muni) + 同一県内近隣の市区町村統計。
    //   空なら従来の「都道府県平均」表にフォールバック。
    region_2d_stats: &[super::super::super::aggregator::RegionMuniStat],
    // 2026-07-28: 表 2-C-2 通勤流出先 TOP3 用。(dest_pref, dest_muni, 通勤者数) の降順配列。
    //   空なら表 2-C-2 は非表示。InsightContext には持たせず個別 param で受け取る
    //   (commute_inflow_top3 と同一伝搬経路にすると明示コンストラクタ多数が壊れるため)。
    commute_outflow_top3: &[(String, String, i64)],
) {
    let show_hw = matches!(variant, ReportVariant::Full);
    let is_ver10 = variant.is_ver10();

    // 2026-07-27: 基準地域 (アプリで選択した市区町村)。target_region は
    //   compose_target_region がアプリ選択を最優先で組んだ文字列
    //   ("東京都 千代田区" / "東京都" / "全国")。市区町村まで含む場合のみ Some。
    //   都道府県名・市区町村名に空白は含まれないため split_once(' ') で安全に分解できる。
    //   表 2-A の ★基準地域 バッジ / 表 2-C の中心名指し / 表 2-D の基準行に使う。
    let base_muni: Option<(&str, &str)> = target_region
        .split_once(' ')
        .filter(|(p, m)| !p.is_empty() && !m.is_empty() && *p != "全国");

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
        html.push_str(&build_navy_region_table(
            agg,
            hw_enrichment_map,
            show_hw,
            base_muni,
        ));
    }

    // -- 表 2-C 通勤流入元 TOP3 (採用範囲拡張の指針)  [旧 7.5-A 統合 2026-05-15]
    if let Some(c) = hw_context {
        if !c.commute_inflow_top3.is_empty() {
            // 2026-07-27: 「対象地域」だと何を中心とした流入か分からないため、
            //   アプリ選択の市区町村を中心 (流入先) として名指しする。
            //   OD 集計自体は元々アプリ選択 pref/muni を dest として走っている
            //   (build_insight_context に選択 pref/muni が渡るため)。
            let center = base_muni
                .map(|(p, m)| format!("{}{}", escape_html(p), escape_html(m)))
                .unwrap_or_else(|| "対象地域".to_string());
            html.push_str(&format!(
                "<div class=\"block-title block-title-spaced\">表 2-C &nbsp;通勤流入元 TOP3 (周辺地域 → {})</div>\n",
                center
            ));
            // 2026-07-28: No./流入人数の数値列に幅を取られ市区町村名が圧縮される問題対策。
            //   colgroup で列幅を明示 (順位 8% + 都道府県 20% + 市区町村 42% + 流入人数 30%)。
            html.push_str(
                "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
                <colgroup>\
                <col style=\"width:8%\"><col style=\"width:20%\">\
                <col style=\"width:42%\"><col style=\"width:30%\">\
                </colgroup>\n\
                <thead><tr>\
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
            html.push_str(&format!(
                "<p class=\"caption\">出典: 国勢調査 通勤 OD。<strong>{}</strong> への他市区町村・他都道府県からの通勤者の多い 3 自治体 (流入先はアプリで選択した市区町村)。採用範囲拡張・近隣自治体への媒体出稿の指針。</p>\n",
                center
            ));
        }
    }

    // -- 表 2-C-2 通勤流出先 TOP3 (選択市区町村から他自治体へ働きに出る通勤者の多い自治体)
    //   2026-07-28: 表 2-C (流入) と対称。選択市区町村に住みながら他地域で働く求職者が
    //   多いほど、その地域は「採用競合エリア」であり媒体出稿・給与訴求の比較対象になる。
    if !commute_outflow_top3.is_empty() {
        let center = base_muni
            .map(|(p, m)| format!("{}{}", escape_html(p), escape_html(m)))
            .unwrap_or_else(|| "対象地域".to_string());
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 2-C-2 &nbsp;通勤流出先 TOP3 ({} → 周辺地域)</div>\n",
            center
        ));
        html.push_str(
            "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
            <colgroup>\
            <col style=\"width:8%\"><col style=\"width:20%\">\
            <col style=\"width:42%\"><col style=\"width:30%\">\
            </colgroup>\n\
            <thead><tr>\
            <th>順位</th><th>都道府県</th><th>市区町村</th><th class=\"num\">流出人数</th>\
            </tr></thead>\n<tbody>\n",
        );
        for (i, (p, m, cnt)) in commute_outflow_top3.iter().take(3).enumerate() {
            html.push_str(&format!(
                "<tr><td class=\"num bold\">{}</td><td>{}</td><td>{}</td><td class=\"num bold\">{}</td></tr>\n",
                i + 1, escape_html(p), escape_html(m), format_number(*cnt)
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str(&format!(
            "<p class=\"caption\">出典: 国勢調査 通勤 OD。<strong>{}</strong> に住みながら他市区町村・他都道府県へ通勤する求職者の多い 3 自治体 (流出元はアプリで選択した市区町村)。採用競合エリアの特定・給与訴求の比較対象の指針。</p>\n",
            center
        ));
    }

    // -- 表 2-D 地域比較 (マクロ指標)
    //   2026-07-27: 「都道府県平均のみ」から「指定市区町村 + 同一県内近隣 + 県平均(参考)」に
    //   変更。元データ (v2_external_labor_force / v2_external_households) は市区町村粒度のため、
    //   顧客が自地域の立ち回りを判断できるよう市区町村単位で比較する。
    //   region_2d_stats が空 (非 SP 経路 / データ未取得) の場合は従来の県平均表にフォールバック。
    let pref_avg_unemp = hw_context.and_then(|c| c.pref_avg_unemployment_rate);
    let pref_avg_single = hw_context.and_then(|c| c.pref_avg_single_rate);
    let fmt_rate = |v: Option<f64>| -> String {
        v.map(|x| format!("{:.2}", x)).unwrap_or_else(|| "—".into())
    };
    if !region_2d_stats.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 2-D &nbsp;地域比較 &mdash; 指定市区町村 + 近隣 (失業率・単身世帯率)</div>\n");
        // 2026-07-28: 市区町村名(★基準地域バッジ含む)が圧縮されないよう colgroup で明示。
        html.push_str(
            "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
             <colgroup>\
             <col style=\"width:50%\"><col style=\"width:25%\"><col style=\"width:25%\">\
             </colgroup>\n\
             <thead><tr>\
             <th>市区町村</th><th class=\"num\">失業率 (%)</th><th class=\"num\">単身世帯率 (%)</th>\
             </tr></thead>\n<tbody>\n",
        );
        // 基準地域を先頭、続いて近隣 (呼出側で CSV 件数降順に整列済み)。
        let mut ordered: Vec<&super::super::super::aggregator::RegionMuniStat> =
            region_2d_stats.iter().collect();
        ordered.sort_by_key(|r| !r.is_base); // is_base=true を前に
        for r in ordered {
            let badge = if r.is_base {
                " <span style=\"display:inline-block;padding:1px 6px;border-radius:4px;\
                 background:#1d4ed8;color:#fff;font-size:.72em;font-weight:700;\">★基準地域</span>"
            } else {
                ""
            };
            let row_class = if r.is_base { " class=\"hl\"" } else { "" };
            html.push_str(&format!(
                "<tr{}><td>{}{}</td><td class=\"num bold\">{}</td><td class=\"num\">{}</td></tr>\n",
                row_class,
                escape_html(&r.municipality),
                badge,
                fmt_rate(r.unemployment_rate),
                fmt_rate(r.single_rate),
            ));
        }
        // 県平均 (参考) 行。
        if pref_avg_unemp.is_some() || pref_avg_single.is_some() {
            html.push_str(&format!(
                "<tr><td><span class=\"dim\">県平均 (参考)</span></td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                fmt_rate(pref_avg_unemp),
                fmt_rate(pref_avg_single),
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: SSDSE-A 市区町村集計 (失業率 = 失業者 / 労働力、単身世帯率 = 単身世帯 / 総世帯)。<strong>★基準地域</strong>: アプリで選択した市区町村。近隣は同一都道府県内で CSV 件数の多い市区町村 (最大 10)。<strong>県平均 (参考)</strong> は県全体の SUM 方式集計。Section 08 の失業率と併せて読む。「—」はデータ欠損。</p>\n");
    } else if pref_avg_unemp.is_some() || pref_avg_single.is_some() {
        // フォールバック: 従来の都道府県平均表 (非 SP 経路 / muni 未指定時)。
        html.push_str("<div class=\"block-title block-title-spaced\">表 2-D &nbsp;都道府県平均比較 (マクロ指標)</div>\n");
        // 2026-07-28: 3 列の均等割りを避け、指標ラベルに幅を配分。
        html.push_str("<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n<colgroup><col style=\"width:55%\"><col style=\"width:25%\"><col style=\"width:20%\"></colgroup>\n<thead><tr><th>指標</th><th class=\"num\">値</th><th>単位</th></tr></thead>\n<tbody>\n");
        for (label, val) in [
            ("県平均 失業率", pref_avg_unemp),
            ("県平均 単身世帯率", pref_avg_single),
        ] {
            if val.is_none() {
                continue;
            }
            html.push_str(&format!(
                "<tr><td><strong>{}</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">%</span></td></tr>\n",
                label, fmt_rate(val)
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: SSDSE-A 都道府県集計 (SUM 方式: 市町村集計を県全体で再集計)。対象地域固有の値ではなく県全体の平均値。Section 08 の失業率と併せて読む。</p>\n");
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
    let so_what = build_region_so_what(agg, pref_top_pct, n_pref, hw_context, show_hw, base_muni);
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">取るべき方針</div>\
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

    // 2026-07-28: No./n 等の数値列が均等割りで広く取られ「都道府県」「位置づけ」が
    //   圧縮される問題対策。colgroup で列幅を明示。
    let mut s = String::from(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:6%\"><col style=\"width:12%\"><col style=\"width:8%\">\
         <col style=\"width:16%\"><col style=\"width:16%\"><col style=\"width:10%\">\
         <col style=\"width:32%\">\
         </colgroup>\n<thead><tr>",
    );
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
         他媒体・公的統計との比較を行う場合は、Section 09 最低賃金表で別途確認してください。</p>\n",
        fmt_val(overall_avg),
        unit_label
    ));

    s
}

// 2026-07-27: 「中核エリア」→「掲載シェア大/中/小」に呼称変更 (何のシェアかを一読で
//   分かるように)。加えて `base_muni` (アプリで選択した基準市区町村) の行に ★基準地域
//   バッジを付け、上位 10 に無い場合は行を追加してでも表示する。
//   これにより「CSV 件数最多 (=表の並び)」と「アプリ選択の基準地域」を視覚的に分離する。
fn build_navy_region_table(
    agg: &SurveyAggregation,
    hw_enrichment_map: &std::collections::HashMap<
        String,
        super::super::super::hw_enrichment::HwAreaEnrichment,
    >,
    show_hw: bool,
    base_muni: Option<(&str, &str)>,
) -> String {
    use super::super::super::aggregator::MunicipalitySalaryAgg;

    // 2026-07-28: 列数が show_hw で 8/6 と変わるため、colgroup も分岐して用意する。
    //   No./市区町村名が均等割りで圧縮されないよう明示的に幅配分。
    let colgroup = if show_hw {
        "<colgroup>\
         <col style=\"width:5%\"><col style=\"width:9%\"><col style=\"width:14%\">\
         <col style=\"width:9%\"><col style=\"width:9%\"><col style=\"width:9%\">\
         <col style=\"width:22%\"><col style=\"width:23%\">\
         </colgroup>\n"
    } else {
        "<colgroup>\
         <col style=\"width:5%\"><col style=\"width:12%\"><col style=\"width:18%\">\
         <col style=\"width:10%\"><col style=\"width:11%\"><col style=\"width:44%\">\
         </colgroup>\n"
    };
    let mut s = format!(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n{}<thead><tr>",
        colgroup
    );
    s.push_str("<th>No.</th><th>都道府県</th><th>市区町村</th>");
    s.push_str("<th class=\"num\">CSV 件数</th>");
    s.push_str("<th class=\"num\">中央値 (万円)</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
        s.push_str("<th>3 ヶ月推移</th>");
        s.push_str("<th>1 年推移</th>");
    } else {
        s.push_str("<th>掲載シェア</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    // 1 行レンダリング (top10 行と、追加した基準地域行で共用)。
    let render_row = |s: &mut String, row: &MunicipalitySalaryAgg, rank: &str, hl: bool| {
        let key = format!("{}:{}", row.prefecture, row.name);
        let enrich = hw_enrichment_map.get(&key);
        let med_man = format!("{:.1}", row.median_salary as f64 / 10000.0);
        let is_base =
            base_muni.map_or(false, |(bp, bm)| row.prefecture == bp && row.name == bm);
        let row_class = if hl { " class=\"hl\"" } else { "" };
        let badge = if is_base {
            " <span style=\"display:inline-block;padding:1px 6px;border-radius:4px;\
             background:#1d4ed8;color:#fff;font-size:.72em;font-weight:700;\">★基準地域</span>"
        } else {
            ""
        };
        s.push_str(&format!(
            "<tr{}><td class=\"num bold\">{}</td><td>{}</td><td>{}{}</td>\
             <td class=\"num bold\">{}</td><td class=\"num\">{}</td>",
            row_class,
            rank,
            escape_html(&row.prefecture),
            escape_html(&row.name),
            badge,
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
            // MI/Public: 掲載シェア (CSV 件数シェア + tag)
            // Round 1-K (2026-06-03): safe_pct ガード
            let pct = if agg.total_count > 0 {
                safe_pct(row.count as f64 / agg.total_count as f64 * 100.0)
            } else {
                0.0
            };
            let (tag, label) = if pct >= 30.0 {
                ("pos", "掲載シェア大")
            } else if pct >= 10.0 {
                ("neu", "掲載シェア中")
            } else {
                ("neu", "掲載シェア小")
            };
            s.push_str(&format!(
                "<td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:.1}%</span></td>",
                tag, label, pct
            ));
        }
        s.push_str("</tr>\n");
    };

    // 件数最多 10 市区町村 (CSV 件数降順)
    let top10: Vec<&MunicipalitySalaryAgg> =
        agg.by_municipality_salary.iter().take(10).collect();

    if top10.is_empty() {
        let cols = if show_hw { 8 } else { 6 };
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">CSV から市区町村集計データを抽出できませんでした。</td></tr>\n",
            cols
        ));
    } else {
        for (i, row) in top10.iter().enumerate() {
            render_row(&mut s, *row, &format!("{}", i + 1), i == 0);
        }
        // 基準地域 (アプリ選択の市区町村) が上位 10 に無い場合は行を追加して必ず表示。
        if let Some((bp, bm)) = base_muni {
            let in_top10 = top10.iter().any(|r| r.prefecture == bp && r.name == bm);
            if !in_top10 {
                if let Some(base_row) = agg
                    .by_municipality_salary
                    .iter()
                    .find(|r| r.prefecture == bp && r.name == bm)
                {
                    render_row(&mut s, base_row, "—", false);
                }
            }
        }
    }
    s.push_str("</tbody></table>\n");
    if show_hw {
        // 2026-07-22 数値監査対応: この内訳は給与を確認できた求人ベース (総件数とは分母が異なる)
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数 (給与を確認できた求人ベースのため、表紙の総件数とは分母が異なる)。中央値: 月給換算済み。媒体掲載数: 求人媒体ローカル DB の現在掲載求人数。推移: 3 ヶ月前比 / 1 年前比 (Turso 時系列)。<strong>★基準地域</strong>: アプリで選択した市区町村。</p>\n");
    } else {
        // 2026-07-22 数値監査対応: この内訳は給与を確認できた求人ベース (総件数とは分母が異なる)
        s.push_str("<p class=\"caption\">CSV 件数: アップロード CSV の (都道府県, 市区町村) 別件数 (給与を確認できた求人ベースのため、表紙の総件数とは分母が異なる)。中央値: 月給換算済み。<strong>掲載シェア</strong>: アップロード CSV 件数に占める割合 (30%以上=大 / 10-30%=中 / 10%未満=小)。<strong>★基準地域</strong>: アプリで選択した市区町村。</p>\n");
    }
    s
}

fn build_region_so_what(
    agg: &SurveyAggregation,
    pref_top_pct: f64,
    n_pref: usize,
    hw_context: Option<&InsightContext>,
    show_hw: bool,
    // 2026-07-27: 基準地域 (アプリで選択した市区町村)。CSV最多基準ではなく基準地域基準で
    //   方針文を組むために使う。
    base_muni: Option<(&str, &str)>,
) -> String {
    let muni_top = agg.by_municipality_salary.first();
    // Round 1-K (2026-06-03): safe_pct ガード
    let muni_top_pct = match muni_top {
        Some(m) if agg.total_count > 0 => safe_pct(m.count as f64 / agg.total_count as f64 * 100.0),
        _ => 0.0,
    };

    // 2026-07-27: 基準地域 (アプリ選択 muni) の掲載シェアと、CSV 最多との一致判定。
    let base_stat: Option<(&str, f64)> = base_muni.and_then(|(bp, bm)| {
        agg.by_municipality_salary
            .iter()
            .find(|m| m.prefecture == bp && m.name == bm)
            .map(|m| {
                let pct = if agg.total_count > 0 {
                    safe_pct(m.count as f64 / agg.total_count as f64 * 100.0)
                } else {
                    0.0
                };
                (m.name.as_str(), pct)
            })
    });
    let base_is_top = match (base_muni, muni_top) {
        (Some((bp, bm)), Some(top)) => top.prefecture == bp && top.name == bm,
        _ => false,
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

    // 2026-07-27: 「件数最多市区町村」基準ではなく、基準地域 (アプリ選択 muni) 基準で方針を組む。
    //   基準地域があればその掲載シェアで判断し、無ければ従来どおり CSV 最多市区町村で記述する。
    let concentration_note = if let Some((base_name, base_pct)) = base_stat {
        let top_hint = if base_is_top {
            String::new()
        } else {
            format!(
                " (アップロードデータ内の掲載シェア最多は <strong>{}</strong>)",
                muni_top.map(|m| m.name.as_str()).unwrap_or("—")
            )
        };
        if base_pct >= 50.0 {
            format!(
                "基準地域 <strong>{}</strong> がアップロードデータの <strong>{:.0}%</strong> を占める<strong>1 自治体集中</strong>です{}。",
                base_name, base_pct, top_hint
            )
        } else if base_pct >= 25.0 {
            format!(
                "基準地域 <strong>{}</strong> がアップロードデータの <strong>{:.0}%</strong> を占めます。掲載シェアの大きいエリアでの面取り戦略が有効です{}。",
                base_name, base_pct, top_hint
            )
        } else {
            format!(
                "基準地域 <strong>{}</strong> の掲載シェアは <strong>{:.0}%</strong> で、データは複数エリアに分散しています。地域別の訴求軸調整が必要です{}。",
                base_name, base_pct, top_hint
            )
        }
    } else if muni_top_pct >= 50.0 {
        format!(
            "掲載シェア最多市区町村 <strong>{}</strong> が <strong>{:.0}%</strong> を占める<strong>1 自治体集中</strong>の構成です。",
            muni_top.map(|m| m.name.as_str()).unwrap_or("—"),
            muni_top_pct
        )
    } else if muni_top_pct >= 25.0 {
        format!(
            "掲載シェア最多市区町村 <strong>{}</strong> が <strong>{:.0}%</strong> を占めます。掲載シェアの大きいエリアでの面取り戦略が有効です。",
            muni_top.map(|m| m.name.as_str()).unwrap_or("—"),
            muni_top_pct
        )
    } else {
        "掲載件数は複数エリアに分散しており、地域別の訴求軸調整が必要です。".to_string()
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
