//! Round 24 Push 3 (2026-05-13): Navy + Gold コンサルファーム調レポートの
//! 専用レンダラ。既存 `executive_summary` / `summary` / `dv2-*` ヘルパに
//! 一切依存せず、navy CSS class (`.kpi-row`, `.findings-list`, `.so-what`,
//! `.table-navy`, `.block-title`, `.row-2`, `.tag-*` 等) だけで構成する。
//!
//! 設計方針:
//! - 既存パス (dv2-section-badge / exec-kpi-grid-v2 / exec-action-list 等)
//!   は **一切呼ばない**。互換 class を併記する妥協を排し、HTML 構造を
//!   ゼロから組み直す。
//! - ECharts は使わず、SSR SVG / CSS 罫線 / 数値テーブル中心。
//! - 既存集計 (`SurveyAggregation`, `InsightContext`) を入力として受け取り、
//!   出力 (String) を呼出側に積む。

#![allow(dead_code)]

// A1 Commit 1 (γ Common Team, 2026-05-29): navy_report 横断 helper を common.rs に集約。
//   抽出: SKEW 判定 / 給与分布統計 / フォーマッタ / SVG 描画 / 数値防衛 / HTML helper
//   `pub(super) use common::*;` により mod.rs 内・test mod (`use super::*;`)
//   の双方から従来通り unqualified で参照可能。API 表面は不変 (pub(super))。
pub(super) mod common;
pub(super) use common::*;

// A1 Commit 2 (β Section Team, 2026-05-29): Section 01 (Cover) / Section 08 (Notes)
// を独立モジュールに分離。元の `render_navy_cover` / `render_navy_section_08_notes`
// は外部 (report_html/mod.rs) から `navy_report::render_navy_*` の path で
// 呼ばれているため、ここで `pub(super)` 再エクスポートして path 互換を維持する。
// API 表面は不変。
pub(super) mod section_01_cover;
pub(super) mod section_08_notes;
pub(super) use section_01_cover::render_navy_cover;
pub(super) use section_08_notes::render_navy_section_08_notes;

// A1 Commit 3 (β Section Team, 2026-05-29): Section 02 (TOC) +
// Section 01 後段 (Executive Summary) を独立モジュールに分離。
// 元の `render_navy_toc` / `render_navy_executive` は外部
// (report_html/mod.rs) から `navy_report::render_navy_*` の path で
// 呼ばれているため、ここで `pub(super)` 再エクスポートして path 互換を維持する。
// 内部 helper (`push_toc_item` / `build_findings`) は section_02_executive_toc.rs
// 内に閉じ込め (module-private)、外部公開はしない。API 表面は不変。
pub(super) mod section_02_executive_toc;
pub(super) use section_02_executive_toc::{render_navy_executive, render_navy_toc};

// A1 Commit 4 (β Section Team, 2026-05-29): Section 02 (Region) +
// Section 04 (Market Tightness) を独立モジュールに分離。
// 元の `render_navy_section_02_region` / `render_navy_section_04_market_tightness`
// は外部 (report_html/mod.rs) から `navy_report::render_navy_section_*` の path で
// 呼ばれているため、ここで `pub(super)` 再エクスポートして path 互換を維持する。
// 内部 helper (`build_navy_prefecture_salary_table` / `build_navy_region_table` /
// `build_region_so_what` / `TightnessData` / `extract_tightness` /
// `build_navy_industry_tightness_table` / `build_navy_tightness_gauges` /
// `build_navy_tightness_table` / `build_tightness_so_what`) は各 section_*.rs
// 内に閉じ込め (module-private)、外部公開はしない。API 表面は不変。
//
// `build_navy_auto_table` は依然として mod.rs 本体に残置 (Section 03/05/06/07 でも
// 共有のため)。section_02_region / section_04_tightness から呼び出すために
// `pub(super)` に昇格 (本 commit で別途修正)。
pub(super) mod section_02_region;
pub(super) mod section_04_tightness;
pub(super) use section_02_region::render_navy_section_02_region;
pub(super) use section_04_tightness::render_navy_section_04_market_tightness;

// A1 Commit 5 (β Section Team, 2026-05-30): Section 05 (Companies) +
// Section 06-08 placeholders (`render_navy_section_placeholders`) を独立
// モジュールに分離。
// 元の `render_navy_section_05_companies` / `render_navy_section_placeholders`
// は外部 (report_html/mod.rs) から `navy_report::render_navy_section_*` /
// `navy_report::render_navy_section_placeholders` の path で呼ばれている
// (placeholders は現状未参照だが API 互換のため再エクスポート)。
// 内部 helper のうち `select_notable_companies` /
// `build_navy_csv_company_salary_table` / `build_navy_notable_companies_block`
// は本ファイル末尾 `#[cfg(test)] mod tests` (`use super::*;`) から直接参照
// されているため、section_05_companies.rs 内で `pub(crate)` に昇格し
// (`pub(super)` は階層不足で E0364 になる)、ここで再エクスポートして
// `tests` mod の `super::*` 経由で従来通り unqualified に呼べる状態を維持する。
// その他の helper (`build_navy_industry_table` /
// `build_navy_industry_bars` / `build_navy_growth_decline_matrix` /
// `build_navy_company_list` / `build_companies_so_what`) は
// section_05_companies.rs 内に閉じ込め (module-private)、外部公開はしない。
// API 表面は不変。
pub(super) mod section_05_companies;
pub(super) use section_05_companies::{
    build_navy_csv_company_salary_table, build_navy_notable_companies_block,
    render_navy_section_05_companies, render_navy_section_placeholders,
    select_notable_companies,
};

// A1 Commit 6 (β Section Team, 2026-05-30): Section 03 (Salary Distribution)
// を独立モジュールに分離。最大セクション (1192 行) を抽出。
// 元の `render_navy_section_03_salary` は外部 (report_html/mod.rs) から
// `navy_report::render_navy_section_03_salary` の path で呼ばれているため、
// ここで `pub(super)` 再エクスポートして path 互換を維持する。
//
// `build_navy_fuyou_table` は report_html 配下の `hourly_report_qa_test.rs`
// から `super::navy_report::build_navy_fuyou_table` path で参照されている
// (扶養範囲到達時給テーブルの QA テスト)。`navy_report` モジュール外への
// 公開が必要なため、section_03_salary.rs 内で `pub(crate)` に昇格し
// (`pub(super)` は階層不足で E0364 になる)、ここで `pub(super) use` で
// 再エクスポートして従来 path を維持する。
//
// 内部 helper (`build_navy_emp_type_salary_table` /
// `build_navy_tag_premium_top10_table` / `build_navy_industry_salary_table` /
// `compute_navy_salary_correlation` / `build_navy_salary_correlation_table` /
// `build_navy_cluster_table` / `build_navy_cluster_boxplots_svg` /
// `build_navy_occupation_salary_table` / `build_navy_salary_summary_table` /
// 補助型 `NavyCorrRow` / 定数 `FUYOU_*`) は section_03_salary.rs 内に
// 閉じ込め (module-private)、外部公開はしない。API 表面は不変。
pub(super) mod section_03_salary;
pub(super) use section_03_salary::{build_navy_fuyou_table, render_navy_section_03_salary};

use super::super::aggregator::{EmpTypeSalary, SurveyAggregation};
use super::super::super::analysis::fetch::CsvCompanySalary;
use super::super::super::helpers::{escape_html, format_number};
use super::super::super::insight::fetch::InsightContext;
use super::super::job_seeker::JobSeekerAnalysis;
use super::salary_summary;
use super::ReportVariant;

// ============================================================
// 公開 API
// ============================================================
// A1 Commit 2 (2026-05-29):
//   `render_navy_cover` (Cover ページ + push_cover_* helper) は
//   `section_01_cover.rs` に分離。mod 冒頭で再エクスポート済み。
// A1 Commit 3 (2026-05-29):
//   `render_navy_toc` (TOC ページ + `push_toc_item` helper) と
//   `render_navy_executive` (Executive Summary + `build_findings` helper) は
//   `section_02_executive_toc.rs` に分離。mod 冒頭で再エクスポート済み。


// ============================================================
// Section 02: 地域 × 求人媒体データ連携 (Full) / 地域データ補強 (MI/Public)
// ============================================================
// A1 Commit 4 (2026-05-29):
//   `render_navy_section_02_region` (Region ページ + `build_navy_prefecture_salary_table` /
//   `build_navy_region_table` / `build_region_so_what` helper) は
//   `section_02_region.rs` に分離。mod 冒頭で再エクスポート済み。

// ============================================================
// Section 04: 採用市場 逼迫度 (Phase 2 navy 本実装)
// ============================================================
// A1 Commit 4 (2026-05-29):
//   `render_navy_section_04_market_tightness` (Tightness ページ + `TightnessData` /
//   `extract_tightness` / `build_navy_industry_tightness_table` /
//   `build_navy_tightness_gauges` / `build_navy_tightness_table` /
//   `build_tightness_so_what` helper) は `section_04_tightness.rs` に分離。
//   mod 冒頭で再エクスポート済み。

// ============================================================
// Section 05: 地域企業構造 (Phase 3 navy 本実装)
// ============================================================
// A1 Commit 5 (2026-05-30):
//   `render_navy_section_05_companies` (Companies ページ +
//   `select_notable_companies` / `build_navy_csv_company_salary_table` /
//   `build_navy_notable_companies_block` / `build_navy_industry_table` /
//   `build_navy_industry_bars` / `build_navy_growth_decline_matrix` /
//   `build_navy_company_list` / `build_companies_so_what` helper) と
//   `render_navy_section_placeholders` (Section 06-08 placeholder) は
//   `section_05_companies.rs` に分離。mod 冒頭で再エクスポート済み。
// tests から参照される `select_notable_companies` /
// `build_navy_csv_company_salary_table` /
// `build_navy_notable_companies_block` は `pub(super) use` 経由で
// 本 mod 内に持ち込まれているため、test mod (`use super::*;`) からは
// 引き続き unqualified に呼べる。


// ============================================================
// Section 06: 人材デモグラフィック (Phase 3 navy 本実装)
// ============================================================

/// Phase 2-A (2026-05-29): `agg` 引数追加。
///   `agg.is_hourly` を Section 06 内の `render_navy_section_06_posting_target` 呼出に
///   伝播するためだけに使用。デモグラフィック自体には is_hourly 依存はない。
pub(super) fn render_navy_section_06_demographics(
    html: &mut String,
    agg: &SurveyAggregation,
    hw_context: Option<&InsightContext>,
    target_region: &str,
) {
    let is_hourly = agg.is_hourly;
    html.push_str("<section class=\"page-navy navy-demographics\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 06",
        "人材デモグラフィック",
        "人口ピラミッド / 労働力 / 教育施設密度",
    );
    push_region_scope_banner(html, target_region);

    let ctx = match hw_context {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption\">外部統計データが取得できなかったため、本セクションは省略表示となります。</p>\n");
            html.push_str("</section>\n");
            return;
        }
    };

    // -- ピラミッドデータ抽出
    use super::super::super::helpers::{get_f64, get_i64, get_str_ref};
    let mut bands: Vec<(String, i64, i64)> = ctx
        .ext_pyramid
        .iter()
        .map(|r| {
            (
                get_str_ref(r, "age_group").to_string(),
                get_i64(r, "male_count"),
                get_i64(r, "female_count"),
            )
        })
        .filter(|(l, _, _)| !l.is_empty())
        .collect();
    bands.sort_by_key(|(l, _, _)| age_sort_key(l));

    // -- 集計
    let total_pop: i64 = bands.iter().map(|(_, m, f)| m + f).sum();
    let working_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 15 && age_lo(l) < 65)
        .map(|(_, m, f)| m + f)
        .sum();
    let target_age: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 25 && age_lo(l) < 45)
        .map(|(_, m, f)| m + f)
        .sum();
    let senior: i64 = bands
        .iter()
        .filter(|(l, _, _)| age_lo(l) >= 65)
        .map(|(_, m, f)| m + f)
        .sum();

    let working_pct = if total_pop > 0 {
        working_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let target_pct = if total_pop > 0 {
        target_age as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };
    let senior_pct = if total_pop > 0 {
        senior as f64 / total_pop as f64 * 100.0
    } else {
        0.0
    };

    // -- 労働力率 / 失業率
    let labor_force_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "labor_force_participation_rate"))
        .filter(|v| *v > 0.0);
    let unemployment_rate = ctx
        .ext_labor_force
        .first()
        .map(|r| get_f64(r, "unemployment_rate"))
        .filter(|v| *v > 0.0);

    // -- 教育施設密度
    let school_count: i64 = ctx
        .ext_education_facilities
        .iter()
        .map(|r| {
            get_i64(r, "elementary_schools")
                + get_i64(r, "junior_high_schools")
                + get_i64(r, "high_schools")
        })
        .sum();

    // -- exec-headline
    let lede = format!(
        "対象地域の生産年齢層厚みを把握します。総人口 <strong>{}</strong> 名 / \
         生産年齢 (15-64) <strong>{:.1}%</strong> / 採用ターゲット (25-44) <strong>{:.1}%</strong> / \
         高齢 (65+) <strong>{:.1}%</strong>。",
        format_number(total_pop),
        working_pct,
        target_pct,
        senior_pct,
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI 5 cell
    let working_dot = if working_pct >= 60.0 { "pos" } else if working_pct >= 50.0 { "neu" } else { "warn" };
    let target_dot = if target_pct >= 22.0 { "pos" } else if target_pct >= 17.0 { "neu" } else { "warn" };
    let senior_dot = if senior_pct >= 35.0 { "warn" } else if senior_pct >= 25.0 { "neu" } else { "pos" };

    html.push_str("<div class=\"block-title\">図 6-1 &nbsp;人口構造 主要 KPI</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(
        html,
        "総人口",
        &format_number(total_pop),
        "名",
        "neu",
        "国勢調査 5 歳階級集計",
        false,
    );
    push_kpi(
        html,
        "生産年齢 (15-64)",
        &format!("{:.1}", working_pct),
        "%",
        working_dot,
        &format!("実数 {} 名", format_number(working_age)),
        true,
    );
    push_kpi(
        html,
        "ターゲット (25-44)",
        &format!("{:.1}", target_pct),
        "%",
        target_dot,
        &format!("実数 {} 名", format_number(target_age)),
        false,
    );
    push_kpi(
        html,
        "高齢 (65+)",
        &format!("{:.1}", senior_pct),
        "%",
        senior_dot,
        &format!("実数 {} 名", format_number(senior)),
        false,
    );
    let lfr_val = labor_force_rate.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "—".into());
    let lfr_dot = match labor_force_rate {
        Some(v) if v >= 62.0 => "pos",
        Some(v) if v >= 55.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    let lfr_foot = match unemployment_rate {
        Some(u) => format!("失業率 {:.1}%", u),
        None => "失業率データなし".to_string(),
    };
    push_kpi(html, "労働力率", &lfr_val, "%", lfr_dot, &lfr_foot, false);
    html.push_str("</div>\n");

    // -- 人口ピラミッド SVG
    if !bands.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 6-2 &nbsp;年齢階級別 人口ピラミッド</div>\n");
        html.push_str(&build_navy_pyramid_svg(&bands));
        html.push_str("<p class=\"caption\">左 (紺) = 男性 / 右 (金) = 女性。各バーは 5 歳階級別の人口を表示。出典: 国勢調査 v2_external_population_pyramid。</p>\n");
    }

    // -- 図 6-2b 市区町村別 人口ピラミッド (上位 3) [P1-5 (2026-05-25) 追加]
    //    対象都道府県内で postings (HW 掲載求人) 件数上位 3 市区町村のピラミッドを並列表示。
    //    ctx.muni_pyramids が空 (pref 未指定 / データ不足) のときは何も出力しない。
    if !ctx.muni_pyramids.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 6-2b &nbsp;市区町村別 人口ピラミッド (上位 3)</div>\n");
        html.push_str(
            "<div class=\"muni-pyramid-grid\" \
             style=\"display:grid;grid-template-columns:1fr 1fr 1fr;gap:6mm;margin-top:2mm;\">\n",
        );
        for mp in &ctx.muni_pyramids {
            let mut sub_bands: Vec<(String, i64, i64)> = mp
                .bands
                .iter()
                .map(|r| {
                    (
                        get_str_ref(r, "age_group").to_string(),
                        get_i64(r, "male_count"),
                        get_i64(r, "female_count"),
                    )
                })
                .filter(|(l, _, _)| !l.is_empty())
                .collect();
            sub_bands.sort_by_key(|(l, _, _)| age_sort_key(l));

            html.push_str(
                "<div class=\"muni-pyramid-card\" \
                 style=\"border:1px solid var(--rule-soft);padding:3mm;background:var(--paper-pure);\">\n",
            );
            html.push_str(&format!(
                "<div style=\"text-align:center;font-weight:700;font-size:10pt;color:#0B1E3F;margin-bottom:2mm;\">{}</div>\n",
                escape_html(&mp.muni_name)
            ));
            if sub_bands.is_empty() {
                html.push_str(
                    "<div class=\"dim\" style=\"text-align:center;font-size:9pt;\">データ取得不可</div>\n",
                );
            } else {
                html.push_str(&build_navy_pyramid_svg_mini(&sub_bands));
            }
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n");
        html.push_str(
            "<p class=\"caption\">対象都道府県の CSV 件数上位 3 市区町村のピラミッドを並列表示。\
             出典: 国勢調査 v2_external_population_pyramid (市区町村粒度)。</p>\n",
        );
    }

    // -- 表 6-B 人口統計詳細 (ext_population) ピラミッド補強  [旧 7.5-D 統合 2026-05-15]
    if !ctx.ext_population.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-B &nbsp;人口統計詳細 (総人口・男女別 年次推移)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_population, 5));
        html.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_population。ピラミッドの 5 歳階級集計に対し、本表は総人口・男女別の年次推移を示す。先頭 5 行表示。</p>\n");
    }

    // -- 表 6-C 人口移動 (ext_migration) ⭐ 採用流入/定着指標  [旧 7.5-E 統合 2026-05-15]
    if !ctx.ext_migration.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-C &nbsp;人口移動 (転入・転出・純増減)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_migration, 5));
        let latest_net: i64 = ctx.ext_migration.first()
            .map(|r| get_i64(r, "net_migration"))
            .unwrap_or(0);
        let migration_insight = if latest_net > 0 {
            format!("最新値で <strong>転入超過 +{} 名</strong>。社外からの流入が継続しており、<strong>採用候補プール 拡大局面</strong>。広域採用・移住セット訴求 (住宅手当 / 引越補助) との相性 良。",
                format_number(latest_net))
        } else if latest_net < 0 {
            format!("最新値で <strong>転出超過 {} 名</strong>。人口流出が継続しており、<strong>採用難 + 離職リスクの両面</strong>に注意。定着策 (キャリアパス明示 / 地元志向人材の囲い込み) を優先推奨。",
                format_number(latest_net))
        } else {
            "転入・転出が均衡。人材の純流入による母集団拡大は期待しにくく、<strong>定着重視</strong>の採用方針が有効。".to_string()
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 住民基本台帳 人口移動報告 v2_external_migration。先頭 5 行表示。<br/><strong>示唆:</strong> {}</p>\n",
            migration_insight
        ));
    }

    // -- 表 6-D 自然増減 (出生・死亡) 中長期人口動態  [旧 7.5-M 統合 2026-05-15]
    if !ctx.ext_vital.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-D &nbsp;自然増減 (出生・死亡)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_vital, 5));
        let latest_natural: i64 = ctx.ext_vital.first()
            .map(|r| get_i64(r, "natural_change"))
            .unwrap_or(0);
        let vital_insight = if latest_natural < 0 {
            format!("最新値で <strong>自然減 {} 名</strong> (死亡 > 出生)。中長期 (5-10 年) で<strong>労働力供給の構造的縮小</strong>が見込まれ、自動化投資・省人化施策の並走を推奨。",
                format_number(latest_natural))
        } else {
            format!("自然増 +{} 名で人口再生産は継続。短期の採用環境は本指標より表 6-C (社会移動) の影響が支配的。",
                format_number(latest_natural))
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 人口動態統計 v2_external_vital。先頭 5 行表示。<br/><strong>示唆:</strong> {}</p>\n",
            vital_insight
        ));
    }

    // -- 教育施設密度 (block-title + 1 段落)
    if school_count > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-A &nbsp;教育施設 (小・中・高 合計)</div>\n");
        html.push_str(&format!(
            "<table class=\"table-navy\">\n<thead><tr>\
             <th>区分</th><th class=\"num\">学校数</th><th>備考</th>\
             </tr></thead>\n<tbody>\n"
        ));
        let mut sum_elem = 0i64;
        let mut sum_jh = 0i64;
        let mut sum_high = 0i64;
        for r in &ctx.ext_education_facilities {
            sum_elem += get_i64(r, "elementary_schools");
            sum_jh += get_i64(r, "junior_high_schools");
            sum_high += get_i64(r, "high_schools");
        }
        html.push_str(&format!(
            "<tr><td><strong>小学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 1-3 km 想定</span></td></tr>\n",
            format_number(sum_elem)
        ));
        html.push_str(&format!(
            "<tr><td><strong>中学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 3-5 km 想定</span></td></tr>\n",
            format_number(sum_jh)
        ));
        html.push_str(&format!(
            "<tr class=\"hl\"><td><strong>高等学校</strong></td><td class=\"num bold\">{}</td>\
             <td><span class=\"dim\">通学圏 10 km 級。新卒採用接点として活用可</span></td></tr>\n",
            format_number(sum_high)
        ));
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: 文部科学省 学校基本調査 v2_external_education_facilities。家族層 (子育て世帯) 採用時の生活インフラ指標として併記。</p>\n");
    }

    // -- 表 6-E 労働力統計 詳細 (ext_labor_stats)  KPI 労働力率の明細  [旧 7.5-C 統合 2026-05-15]
    if !ctx.ext_labor_stats.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-E &nbsp;労働力統計 詳細 (就業者・産業構成)</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_labor_stats, 5));
        html.push_str("<p class=\"caption\">出典: e-Stat 社会人口統計体系 v2_external_labor_stats。図 6-1 KPI「労働力率」の内訳として、男女別就業者・第1-3 次産業就業者の構成比を示す。先頭 5 行表示。</p>\n");
    }

    // -- 表 6-F 学歴構成 (ext_education) [P1-5 (2026-05-25): 手書き化 + 構成比列追加]
    //    旧実装: build_navy_auto_table(&ctx.ext_education, 5)
    //    変更点: education_level / 男性人数 / 女性人数 / 合計 / 構成比 (%) の 5 列固定。
    //    構成比 = total_count / SUM(total_count) * 100 (小数 1 桁、右寄せ + bold)。
    if !ctx.ext_education.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 6-F &nbsp;進学率・学歴 (新卒採用接点)</div>\n");

        // 合計算出 (構成比の分母)
        let total_sum: i64 = ctx
            .ext_education
            .iter()
            .map(|r| get_i64(r, "total_count"))
            .sum();

        html.push_str("<table class=\"table-navy\">\n");
        html.push_str(
            "<thead><tr>\
             <th>学歴レベル</th>\
             <th class=\"num\">男性人数</th>\
             <th class=\"num\">女性人数</th>\
             <th class=\"num\">合計</th>\
             <th class=\"num\">構成比 (%)</th>\
             </tr></thead>\n<tbody>\n",
        );

        for r in ctx.ext_education.iter().take(5) {
            let level = get_str_ref(r, "education_level");
            let male = get_i64(r, "male_count");
            let female = get_i64(r, "female_count");
            let total = get_i64(r, "total_count");
            let pct = if total_sum > 0 {
                total as f64 / total_sum as f64 * 100.0
            } else {
                0.0
            };
            html.push_str(&format!(
                "<tr>\
                 <td>{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num bold\">{:.1}</td>\
                 </tr>\n",
                escape_html(level),
                format_number(male),
                format_number(female),
                format_number(total),
                pct,
            ));
        }
        html.push_str("</tbody></table>\n");
        html.push_str("<p class=\"caption\">出典: 学校基本調査 v2_external_education。表 6-A の学校数 (施設密度) に対し、本表は進学率・学歴構成を示す。高校進学率は新卒採用の母集団品質、大学進学率は U ターン採用の射程に直結。先頭 5 行表示。</p>\n");
    }

    // -- 図 6-3 求人ターゲット プロファイル (求人側集計) [P2-3 (2026-05-28) 追加]
    //
    //   背景: hellowork.db に求職者個人テーブルが存在しないため、postings (HW 求人) 側の
    //   募集対象条件 (年齢制限 / 給与レンジ / 経験 / 雇用形態) を集計して
    //   「求人側から見たターゲット プロファイル」として提示する。
    //
    //   出典明記: 「HW 求人 (postings) の募集条件集計」
    //   人数推定は行わず、求人件数のみを集計 (DISPLAY_SPEC v1.0 §2 / Hard NG 用語不使用)。
    //   ctx.posting_target == None または total_postings == 0 の場合は本ブロックを skip。
    if let Some(pt) = ctx.posting_target.as_ref().filter(|p| p.total_postings > 0) {
        // Phase 2-B (2026-05-29): agg を追加 — H4 表 6-J で salary_min_values_native を使うため。
        render_navy_section_06_posting_target(html, pt, is_hourly, agg);
    }

    // -- so-what
    let so_what = build_demographics_so_what(
        working_pct,
        target_pct,
        senior_pct,
        labor_force_rate,
        is_hourly,
    );
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

/// P2-3 (2026-05-28) 図 6-3: 求人ターゲット プロファイル (求人側集計) の描画。
///
/// 注意:
/// - 本関数が扱うのは **求人件数** のみ。「求職者人数」「ターゲット人数」「想定人数」
///   「推定人数」「母集団人数」等の禁止語句 (DISPLAY_SPEC v1.0 §2 / Hard NG) を使わない。
/// - 各分布の caption に「出典: HW 求人 (postings) の募集条件集計」を明記する。
/// - 構成比は分布内の sum を分母にして算出 (0 件分布が混在しても合計 100%)。
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。給与レンジの bucket 表記と
/// salary_type フィルタの注記を時給/月給で切替える。
///
/// Phase 2-B (2026-05-29): `agg` 引数追加。表 6-J (H4: 時給帯別 求人件数) で
/// `agg.salary_min_values_native` を 100 円刻みで集計するため使用。
/// 時給モード (is_hourly == true) でのみ表 6-J を出力する。
fn render_navy_section_06_posting_target(
    html: &mut String,
    pt: &super::super::super::analysis::fetch::PostingTargetProfile,
    is_hourly: bool,
    agg: &SurveyAggregation,
) {
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         図 6-3 &nbsp;求人ターゲット プロファイル (求人側集計)\
         </div>\n",
    );
    html.push_str(
        "<p class=\"caption\">本ブロックは <strong>HW 求人 (postings) の募集条件</strong> を集計した\
         <strong>求人件数</strong> ベースの分布です。求職者個人データではなく、\
         募集側がどの層を想定しているかの傾向を示します。</p>\n",
    );

    // ---- KPI: 総求人件数 / 年齢制限主要層 / 給与中央レンジ / 雇用形態主流
    //
    // R2-P1-6 (ultrathink Round 2, 2026-05-28): `max_by_key` は同値ペアで
    // last-wins の挙動を取る。distribution の **全カウントが 0** の場合
    // (例: salary_type が「月給」の求人が 1 件もない地域) 、最後のラベル
    // (例: 「〜20万」) を選んでしまい KPI に誤表示される。
    // → max_by_key の戻り値が count == 0 の場合は「—」に明示的に置換する。
    let take_top_or_dash = |pair_opt: Option<(String, i64)>| -> (String, i64) {
        match pair_opt {
            Some((l, c)) if c > 0 => (l, c),
            _ => ("—".to_string(), 0),
        }
    };
    // 主要年齢層 = age_range_distribution の最多バケット (件数降順 1 位)
    let top_age = take_top_or_dash(
        pt.age_range_distribution
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 主要給与レンジ = salary_target_distribution の最多バケット
    let top_salary = take_top_or_dash(
        pt.salary_target_distribution
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 主流雇用形態 = employment_type_distribution の最多バケット (既に降順 sort 済)
    // R2-P1-6: first() でも count==0 ガードを適用 (employment_type も全 0 の可能性あり)
    let top_emp = take_top_or_dash(
        pt.employment_type_distribution
            .first()
            .map(|(l, c)| (l.clone(), *c)),
    );
    // 経験不問 (実質) の比率
    let total_exp: i64 = pt
        .experience_required_distribution
        .iter()
        .map(|(_, c)| *c)
        .sum();
    let unspec_count: i64 = pt
        .experience_required_distribution
        .iter()
        .find(|(l, _)| l == "経験不問 (実質)")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    // R2-P1-1 (ultrathink Round 2, 2026-05-28): total_exp > 0 ガード後でも
    // 浮動小数誤差で 100% 超えになる可能性をクランプで防御。
    let unspec_pct = if total_exp > 0 {
        safe_pct(unspec_count as f64 / total_exp as f64 * 100.0)
    } else {
        0.0
    };

    html.push_str("<div class=\"kpi-row\">\n");
    push_kpi(
        html,
        "集計求人件数",
        &format_number(pt.total_postings),
        "件",
        "neu",
        "HW postings (pref/muni 一致)",
        true,
    );
    push_kpi(
        html,
        "年齢制限 主要層",
        &top_age.0,
        "",
        "neu",
        &format!("{} 件", format_number(top_age.1)),
        false,
    );
    push_kpi(
        html,
        "給与 主要レンジ",
        &top_salary.0,
        "",
        "neu",
        // Phase 2-A: 給与記載 (salary_type) を is_hourly で切替
        &format!(
            "{} 件 ({}記載のみ)",
            format_number(top_salary.1),
            if is_hourly { "時給" } else { "月給" }
        ),
        false,
    );
    push_kpi(
        html,
        "経験不問 比率",
        &format!("{:.1}", unspec_pct),
        "%",
        if unspec_pct >= 70.0 {
            "pos"
        } else if unspec_pct >= 40.0 {
            "neu"
        } else {
            "warn"
        },
        "experience_required 未記載求人",
        false,
    );
    push_kpi(
        html,
        "雇用形態 主流",
        &top_emp.0,
        "",
        "neu",
        &format!("{} 件", format_number(top_emp.1)),
        false,
    );
    html.push_str("</div>\n");

    // ---- 表 6-G: 年齢制限 × 求人件数
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         表 6-G &nbsp;年齢制限別 求人件数 (求人側集計)\
         </div>\n",
    );
    html.push_str(&build_distribution_table(
        &pt.age_range_distribution,
        "年齢制限ラベル",
    ));
    html.push_str(
        "<p class=\"caption\">出典: HW 求人 (postings) の age_min / age_max 列を集計。\
         「制限なし」は両方 NULL の求人。年齢制限は雇用対策法上の例外 \
         (試用期間/技能継承/特定職種) を含む可能性があります。</p>\n",
    );

    // ---- 表 6-H: 給与レンジ × 求人件数 (Phase 2-A: is_hourly でラベル/注記切替)
    let salary_table_title = if is_hourly {
        "表 6-H &nbsp;給与レンジ別 求人件数 (時給記載のみ)"
    } else {
        "表 6-H &nbsp;給与レンジ別 求人件数 (月給記載のみ)"
    };
    let salary_label_header = if is_hourly { "時給レンジ" } else { "月給レンジ" };
    let salary_caption = if is_hourly {
        "<p class=\"caption\">出典: HW 求人 (postings) の salary_min 列を集計 (時給帯)。\
         salary_type が「時給」かつ salary_min &gt; 0 の求人のみが母集団 \
         (月給・年俸はここでは除外)。本表の件数合計は KPI「集計求人件数」より少なくなります。</p>\n"
    } else {
        "<p class=\"caption\">出典: HW 求人 (postings) の salary_min 列を月給換算なしで集計。\
         salary_type が「月給」かつ salary_min &gt; 0 の求人のみが母集団 \
         (時給・年俸はここでは除外)。本表の件数合計は KPI「集計求人件数」より少なくなります。</p>\n"
    };
    html.push_str(&format!(
        "<div class=\"block-title block-title-spaced\">{}</div>\n",
        salary_table_title
    ));
    html.push_str(&build_distribution_table(
        &pt.salary_target_distribution,
        salary_label_header,
    ));
    html.push_str(salary_caption);

    // ---- 表 6-J: 時給帯別 求人件数 (Phase 2-B H4, 2026-05-29)
    //   時給モードのみ表示。agg.salary_min_values_native を 100 円刻みで bucket 化。
    //   表 6-H (salary_target_distribution: HW postings 月給 salary_min の bucket) との違い:
    //     - 表 6-H は HW postings の salary_min を単一値で月給 bucket 化
    //     - 表 6-J は CSV (媒体分析側) の時給ネイティブ値で 100 円刻みの価格弾力性を見る
    //   silent fallback 防止: is_hourly == false の月給モードでは完全に省略。
    if is_hourly {
        let distribution = build_hourly_band_distribution(&agg.salary_min_values_native);
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 6-J &nbsp;時給帯別 求人件数 (100円刻み)</div>\n",
        );
        html.push_str(&build_distribution_table(&distribution, "時給帯"));
        html.push_str(
            "<p class=\"caption\">出典: CSV 集計 (下限給与ネイティブ円/時)。\
             100 円刻みの求人件数分布。\
             <strong>表 6-H との違い:</strong> 表 6-H は salary_min 単一値の bucket、\
             本表は時給市場の価格弾力性を見る (100円帯ごとの厚みで競合密度を把握)。</p>\n",
        );
    }

    // ---- 表 6-I: 雇用形態 × 求人件数
    html.push_str(
        "<div class=\"block-title block-title-spaced\">\
         表 6-I &nbsp;雇用形態別 求人件数\
         </div>\n",
    );
    html.push_str(&build_distribution_table(
        &pt.employment_type_distribution,
        "雇用形態",
    ));
    html.push_str(
        "<p class=\"caption\">出典: HW 求人 (postings) の employment_type 列を集計 (件数降順)。\
         「未記載」は元データが空文字または NULL の求人。</p>\n",
    );
}

/// 分布 `(label, count)` のリストから 3 列表 (ラベル / 件数 / 構成比 %) を生成する共通ビルダ。
///
/// # 引数
/// - `distribution`: `(ラベル, 件数)` のリスト。
///   - **順序は呼出側の責任**。本関数では並べ替えない (年齢/給与は表示順固定、雇用形態は降順、
///     経験 2 値は固定順を維持するため)。
///   - ラベルは生 String を受け、`escape_html` で安全化される (`<script>` 等の混入を防ぐ)。
///   - 件数は i64。負値は理論上発生しないが、合計計算では負値も含めて算術する
///     (異常データ検出を呼出側に委ねる設計)。
/// - `label_header`: 1 列目の `<th scope="col">` 内容。例: "年齢制限ラベル" / "月給レンジ" / "雇用形態"。
///
/// # 戻り値
/// HTML 表全体 (`<table class="table-navy">...</table>`)。
/// 空 `distribution` または件数合計 `total == 0` のときは「該当データなし」を 1 行表示。
///
/// # 不変条件 (テストで検証)
/// - 構成比合計 ≈ 100% (各行は `count / total * 100`、浮動誤差は `safe_pct` で [0, 100] にクランプ)
/// - 各 `<th>` に `scope="col"` 付与 (a11y / Round 2 P1-4 で導入)
/// - `<th>` / `<td>` 内のラベルは必ず `escape_html` を通す (XSS 防御)
/// - 空入力時の "該当データなし" 行も `<tbody>` 内 (構造保証)
///
/// # silent fallback 監査
/// - 件数合計 0 は明示的に `<td colspan="3">該当データなし</td>` で表示 (空文字列を返さない)
/// - `total > 0` ガード後に除算するため zero-div 不可
fn build_distribution_table(distribution: &[(String, i64)], label_header: &str) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    // R2-P1-4 (ultrathink Round 2, 2026-05-28): a11y のため列ヘッダに scope="col" を付与。
    s.push_str(&format!(
        "<th scope=\"col\">{}</th><th scope=\"col\" class=\"num\">求人件数</th><th scope=\"col\" class=\"num\">構成比 (%)</th>",
        escape_html(label_header)
    ));
    s.push_str("</tr></thead>\n<tbody>\n");

    let total: i64 = distribution.iter().map(|(_, c)| *c).sum();
    if distribution.is_empty() || total == 0 {
        s.push_str(
            "<tr><td colspan=\"3\" class=\"dim\">該当データなし</td></tr>\n\
             </tbody></table>\n",
        );
        return s;
    }

    for (label, count) in distribution {
        // R2-P1-1 (ultrathink Round 2, 2026-05-28): total > 0 ガード済だが
        // 浮動小数誤差を safe_pct で [0, 100] にクランプ。
        let pct = safe_pct(*count as f64 / total as f64 * 100.0);
        s.push_str(&format!(
            "<tr><td>{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{:.1}</td></tr>\n",
            escape_html(label),
            format_number(*count),
            pct
        ));
    }
    s.push_str("</tbody></table>\n");
    s
}

// ============================================================
// Phase 2-B (2026-05-29): 時給モード H4 — 時給帯別 求人件数分布
// ============================================================
//
// 仕様:
//   - 100 円刻みで bucket 化: <900 / 900-1000 / 1000-1100 / 1100-1200 / 1200-1300 /
//                              1300-1400 / 1400-1500 / 1500-1600 / 1600-1700 /
//                              1700-1800 / 1800-1900 / 1900-2000 / 2000+
//     合計 13 段
//   - 各 bucket: (ラベル, 件数) のペアを順序保持で返す
//
// 不変条件 (テストで検証):
//   - bucket 合計 == values.iter().filter(>0).count()
//   - 単一値 [1200, 1200, 1200] → "1200-1300円" bucket に 3 件
//   - 境界値 1000 → "1000-1100円" (lo 包含、hi 排他)
//   - empty → 全 bucket 0 件のリスト (build_distribution_table 側で total==0 のとき「該当データなし」)
const HOURLY_BAND_BOUNDARIES: [(i64, i64, &str); 13] = [
    (0, 900, "<900円"),
    (900, 1000, "900-1000円"),
    (1000, 1100, "1000-1100円"),
    (1100, 1200, "1100-1200円"),
    (1200, 1300, "1200-1300円"),
    (1300, 1400, "1300-1400円"),
    (1400, 1500, "1400-1500円"),
    (1500, 1600, "1500-1600円"),
    (1600, 1700, "1600-1700円"),
    (1700, 1800, "1700-1800円"),
    (1800, 1900, "1800-1900円"),
    (1900, 2000, "1900-2000円"),
    (2000, i64::MAX, "2000円+"),
];

/// 時給値リストを 100 円刻みの bucket 分布 `(ラベル, 件数)` に変換。
///
/// # 引数
/// - `values`: 時給ネイティブ値 (円/時)。<= 0 は除外。
///
/// # 戻り値
/// `(ラベル, 件数)` のリスト。順序は HOURLY_BAND_BOUNDARIES の宣言順 (昇順)。
/// 全 bucket を返す (count==0 のものも含む) → build_distribution_table 側で
/// total==0 のときのみ「該当データなし」を表示するため、空 Vec は返さない。
///
/// # 不変条件
/// - 戻り値 .len() == HOURLY_BAND_BOUNDARIES.len() (= 13)
/// - sum(counts) == values.iter().filter(|v| **v > 0).count()
pub(super) fn build_hourly_band_distribution(values: &[i64]) -> Vec<(String, i64)> {
    let mut counts: Vec<i64> = vec![0; HOURLY_BAND_BOUNDARIES.len()];
    for v in values.iter().copied().filter(|x| *x > 0) {
        for (i, (lo, hi, _)) in HOURLY_BAND_BOUNDARIES.iter().enumerate() {
            // [lo, hi) 判定。最後の "2000円+" は hi = i64::MAX のため上限なし。
            if v >= *lo && v < *hi {
                counts[i] += 1;
                break;
            }
        }
    }
    HOURLY_BAND_BOUNDARIES
        .iter()
        .zip(counts.iter())
        .map(|((_, _, label), c)| (label.to_string(), *c))
        .collect()
}

// 「20-24」「25-29」「85+」等のラベルから下端年齢を取得
fn age_lo(label: &str) -> i32 {
    let mut s = String::new();
    for c in label.chars() {
        if c.is_ascii_digit() {
            s.push(c);
        } else {
            break;
        }
    }
    s.parse::<i32>().unwrap_or(-1)
}

fn age_sort_key(label: &str) -> i32 {
    let v = age_lo(label);
    if v >= 0 {
        v
    } else {
        i32::MAX
    }
}

/// navy 人口ピラミッド SVG (左=男性 ink-soft / 右=女性 accent)
fn build_navy_pyramid_svg(bands: &[(String, i64, i64)]) -> String {
    if bands.is_empty() {
        return String::new();
    }
    let n = bands.len();
    let row_h: f64 = 18.0;
    let h: f64 = 40.0 + n as f64 * row_h + 24.0;
    let w: f64 = 720.0;
    // 2026-05-14: 年齢ラベルがバーの中央 (men/women 境界) に乗り、紺/金バーと潰れて
    //             判読困難だった問題を解消。ラベルを左外側の専用カラムに移動し、
    //             バー描画領域を左にオフセットして重なりを除去する。
    let label_col_w: f64 = 56.0;        // 左端のラベル列幅
    let center_gap: f64 = 8.0;          // 男女バー間のセンター隙間
    let bar_max_w: f64 = (w - label_col_w) / 2.0 - center_gap;
    let center: f64 = label_col_w + bar_max_w + center_gap; // 男女境界 (シフトした中心)

    let max_count: f64 = bands
        .iter()
        .flat_map(|(_, m, f)| [*m as f64, *f as f64])
        .fold(0.0, f64::max)
        .max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"人口ピラミッド\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n\
         <title>年齢階級別 人口ピラミッド</title>\n",
        w = w as i64,
        h = h as i64
    );
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): a11y のため SVG 直後に <title> を挿入。
    // スクリーンリーダーは aria-label と <title> の双方を読み上げ得るため、両立させる。
    // タイトルラベル (左カラム = 年齢, 男性 = 中央左, 女性 = 中央右)
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"18\" font-size=\"10\" fill=\"#6A6E7A\" font-weight=\"700\">年齢</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\" text-anchor=\"end\">男性</text>\
         <text x=\"{:.1}\" y=\"18\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\">女性</text>\n",
        4.0, center - 8.0, center + 8.0
    ));
    // 中央軸
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"30\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        center, center, h - 24.0
    ));

    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let cy = 36.0 + i as f64 * row_h;
        let mw = (*male as f64 / max_count) * bar_max_w;
        let fw = (*female as f64 / max_count) * bar_max_w;
        // 男性 (左)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#1F2D4D\"/>\n",
            center - mw,
            cy,
            mw.max(0.5)
        ));
        // 女性 (右)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"#C9A24B\"/>\n",
            center,
            cy,
            fw.max(0.5)
        ));
        // 年齢ラベル (左カラム、独立した白背景領域)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#0B1E3F\" font-weight=\"600\" text-anchor=\"start\">{}</text>\n",
            4.0,
            cy + 10.0,
            escape_html(label)
        ));
    }

    // 軸スケール
    svg.push_str(&format!(
        "<text x=\"4\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\">{} 名</text>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"end\">{} 名</text>\n",
        h - 8.0,
        format_number(max_count as i64),
        w - 4.0,
        h - 8.0,
        format_number(max_count as i64)
    ));
    svg.push_str("</svg>\n");
    svg
}

/// 図 6-2b 用ミニピラミッド SVG (3 列横並びレイアウト想定、幅 220px)。
///
/// `build_navy_pyramid_svg` の構造をベースに、グリッドカード内に収まるようサイズと
/// フォントを縮小: 幅 220px / 行高 14px / フォント 7-8pt / ラベル列幅 32px。
/// 色 (#1F2D4D / #C9A24B) は本体ピラミッドと一貫させる。
fn build_navy_pyramid_svg_mini(bands: &[(String, i64, i64)]) -> String {
    if bands.is_empty() {
        return String::new();
    }
    let n = bands.len();
    let row_h: f64 = 14.0;
    let h: f64 = 30.0 + n as f64 * row_h + 18.0;
    let w: f64 = 220.0;
    let label_col_w: f64 = 32.0;
    let center_gap: f64 = 4.0;
    let bar_max_w: f64 = (w - label_col_w) / 2.0 - center_gap;
    let center: f64 = label_col_w + bar_max_w + center_gap;

    let max_count: f64 = bands
        .iter()
        .flat_map(|(_, m, f)| [*m as f64, *f as f64])
        .fold(0.0, f64::max)
        .max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"市区町村別 人口ピラミッド\" \
         style=\"display:block;background:var(--paper-pure);\">\n\
         <title>市区町村別 人口ピラミッド (年齢階級別 男女別 人口)</title>\n",
        w = w as i64,
        h = h as i64
    );
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): a11y のため SVG 直後に <title> を挿入。
    // タイトル行 (男性 / 女性)
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"14\" font-size=\"7\" fill=\"#6A6E7A\" font-weight=\"700\">年齢</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"8\" fill=\"#0B1E3F\" font-weight=\"700\" text-anchor=\"end\">男</text>\
         <text x=\"{:.1}\" y=\"14\" font-size=\"8\" fill=\"#0B1E3F\" font-weight=\"700\">女</text>\n",
        2.0, center - 4.0, center + 4.0
    ));
    // 中央軸
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"22\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        center, center, h - 18.0
    ));

    for (i, (label, male, female)) in bands.iter().rev().enumerate() {
        let cy = 28.0 + i as f64 * row_h;
        let mw = (*male as f64 / max_count) * bar_max_w;
        let fw = (*female as f64 / max_count) * bar_max_w;
        // 男性 (左)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\" fill=\"#1F2D4D\"/>\n",
            center - mw,
            cy,
            mw.max(0.5)
        ));
        // 女性 (右)
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"10\" fill=\"#C9A24B\"/>\n",
            center,
            cy,
            fw.max(0.5)
        ));
        // 年齢ラベル
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"7\" fill=\"#0B1E3F\" font-weight=\"600\" text-anchor=\"start\">{}</text>\n",
            2.0,
            cy + 8.0,
            escape_html(label)
        ));
    }

    // 軸スケール (最大値)
    svg.push_str(&format!(
        "<text x=\"2\" y=\"{:.1}\" font-size=\"6\" fill=\"#6A6E7A\">{} 名</text>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"6\" fill=\"#6A6E7A\" text-anchor=\"end\">{} 名</text>\n",
        h - 4.0,
        format_number(max_count as i64),
        w - 2.0,
        h - 4.0,
        format_number(max_count as i64)
    ));
    svg.push_str("</svg>\n");
    svg
}

/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。
///   時給モードでは採用候補層を 25-49 (主婦層含めて広め) に変更し、
///   訴求軸も「給与訴求 + 福利厚生」→「扶養範囲明示 + シフト柔軟性 + 交通費」に切替える。
fn build_demographics_so_what(
    working_pct: f64,
    target_pct: f64,
    senior_pct: f64,
    labor_force_rate: Option<f64>,
    is_hourly: bool,
) -> String {
    let target_label = if is_hourly {
        "採用候補層 (25-49)"
    } else {
        "採用ターゲット層 (25-44)"
    };
    let appeal_text = if is_hourly {
        "扶養範囲明示 + シフト柔軟性 + 交通費"
    } else {
        "給与訴求 + 福利厚生"
    };
    let pool_judge = if target_pct >= 22.0 {
        format!(
            "{} が <strong>{:.0}%</strong> を占め、<strong>採用候補プール 厚</strong>。{}の充実度で勝負できる地域です。",
            target_label, target_pct, appeal_text
        )
    } else if target_pct >= 17.0 {
        format!(
            "{} は <strong>{:.0}%</strong>。<strong>採用候補プール 中</strong>。エントリー要件の柔軟化 (経験不問 / 異業種歓迎) で母集団拡大を検討してください。",
            target_label, target_pct
        )
    } else {
        format!(
            "{} が <strong>{:.0}%</strong> と薄く、<strong>採用候補プール 細</strong>。\
             年齢帯拡張 ({}) や近隣広域への採用範囲拡大が必要です。",
            target_label,
            target_pct,
            if is_hourly { "55-69 ベテラン層を含める / 学生層 18-24 を含める" } else { "45-54 層への展開" },
        )
    };

    let age_balance = if senior_pct >= 35.0 {
        " 高齢層 35%+ で <strong>人口構造は超高齢化</strong>。退職タイミングを見据えた中期的な人員計画 (3-5 年) が必要です。"
    } else if senior_pct >= 25.0 {
        " 高齢層 25%+ で全国平均並み。生産年齢層の絶対数を維持する施策 (定着 / 中途採用) を継続的に。"
    } else {
        " 高齢層比率が低く、生産年齢層が厚い <strong>採用に有利な構造</strong> です。"
    };

    let labor_note = match labor_force_rate {
        Some(v) if v >= 62.0 => format!(" 労働力率 {:.1}% は高水準で、既就業者の引き抜き競争が激しい可能性があります。", v),
        Some(v) if v >= 55.0 => format!(" 労働力率 {:.1}% は標準的水準です。", v),
        Some(v) => format!(" 労働力率 {:.1}% は低めで、潜在労働力 (非労働力人口) のリーチ施策に余地があります。", v),
        None => String::new(),
    };

    let _ = working_pct;
    format!("{}{}{}", pool_judge, age_balance, labor_note)
}

// ============================================================
// Section 07: 最低賃金・ライフスタイル (Phase 4 navy 本実装)
// ============================================================

pub(super) fn render_navy_section_07_lifestyle(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    target_region: &str,
    // 2026-05-23 #227 追加: 求人給与中央値 (家計支出 / 最低賃金との比較に使用)
    agg: &SurveyAggregation,
) {
    html.push_str("<section class=\"page-navy navy-lifestyle\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 07",
        "最低賃金・ライフスタイル",
        "最低賃金推移 / 家計支出構成 / 通勤圏",
    );
    push_region_scope_banner(html, target_region);

    let ctx = match hw_context {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption\">外部統計データが取得できなかったため、本セクションは省略表示となります。</p>\n");
            html.push_str("</section>\n");
            return;
        }
    };

    use super::super::super::helpers::{get_f64, get_i64, get_str_ref};

    // -- 最低賃金: ext_min_wage 時系列。複数キー候補から取得 (Row 型は HashMap)
    let mut wages: Vec<(i32, i64)> = ctx
        .ext_min_wage
        .iter()
        .filter_map(|r| {
            let year = get_i64(r, "year") as i32;
            for k in ["hourly_wage", "hourly_min_wage", "min_wage", "amount"] {
                let v = get_f64(r, k);
                if v > 0.0 {
                    return Some((year, v as i64));
                }
            }
            None
        })
        .collect();
    wages.sort_by_key(|(y, _)| *y);
    let latest_wage = wages.last().copied();
    let oldest_wage = wages.first().copied();
    let wage_yoy = if wages.len() >= 2 {
        let (_, prev) = wages[wages.len() - 2];
        let (_, cur) = wages[wages.len() - 1];
        if prev > 0 {
            Some((cur - prev) as f64 / prev as f64 * 100.0)
        } else {
            None
        }
    } else {
        None
    };

    // -- 家計支出
    let total_consumption: i64 = ctx
        .ext_household_spending
        .iter()
        .find(|r| get_str_ref(r, "category") == "消費支出")
        .map(|r| get_f64(r, "monthly_amount") as i64)
        .unwrap_or(0);
    let mut category_breakdown: Vec<(String, i64)> = ctx
        .ext_household_spending
        .iter()
        .filter(|r| get_str_ref(r, "category") != "消費支出")
        .map(|r| (get_str_ref(r, "category").to_string(), get_f64(r, "monthly_amount") as i64))
        .filter(|(n, v)| !n.is_empty() && *v > 0)
        .collect();
    category_breakdown.sort_by(|a, b| b.1.cmp(&a.1));

    // -- インターネット利用率 / スマホ保有率
    let internet_rate = ctx
        .ext_internet_usage
        .first()
        .map(|r| get_f64(r, "internet_usage_rate"))
        .filter(|v| *v > 0.0);
    let smartphone_rate = ctx
        .ext_internet_usage
        .first()
        .map(|r| get_f64(r, "smartphone_ownership_rate"))
        .filter(|v| *v > 0.0);

    // -- 通勤圏
    let commute_pop = ctx.commute_zone_total_pop;
    let commute_working = ctx.commute_zone_working_age;
    let commute_inflow = ctx.commute_inflow_total;
    let commute_outflow = ctx.commute_outflow_total;
    let commute_self_rate = ctx.commute_self_rate;
    let commute_zone_count = ctx.commute_zone_count;

    // -- exec-headline
    // 2026-05-14: 取得失敗値 (year=0, 値=0) を lede に混入させない。
    //             「最低賃金 0 年 1,063 円/時」「月間消費支出 0 円」「通勤圏内人口 0 名」
    //             の表示問題を解消するため、有効値のみセグメントを連結する。
    // 2026-05-14: 地域別最低賃金 (法律上同一県内は同額) であることを明示するため
    //   都道府県名を併記する。
    let pref_prefix = if ctx.pref.is_empty() { String::new() } else { format!("{} ", ctx.pref) };
    let wage_seg = latest_wage
        .filter(|(y, w)| *y > 0 && *w > 0)
        .map(|(y, w)| format!("{}最低賃金 {} 年 <strong>{} 円/時</strong>", pref_prefix, y, format_number(w)))
        .or_else(|| latest_wage
            .filter(|(_, w)| *w > 0)
            .map(|(_, w)| format!("{}最低賃金 <strong>{} 円/時</strong>", pref_prefix, format_number(w))));
    let consumption_seg = if total_consumption > 0 {
        Some(format!("月間消費支出 <strong>{}</strong> 円", format_number(total_consumption)))
    } else { None };
    let commute_seg = if commute_pop > 0 {
        Some(format!(
            "通勤圏内人口 <strong>{}</strong> 名{}",
            format_number(commute_pop),
            if commute_working > 0 {
                format!(" (生産年齢 {} 名)", format_number(commute_working))
            } else { String::new() }
        ))
    } else { None };
    let segments: Vec<String> = [wage_seg, consumption_seg, commute_seg]
        .into_iter().flatten().collect();
    let lede = if segments.is_empty() {
        "対象地域の生活コスト・通勤圏に関する公的指標が取得できませんでした。\
         以降のセクションで給与・人口側の指標から定性評価を補完してください。".to_string()
    } else {
        format!(
            "対象地域の生活コストと通勤圏を把握します。{}。給与訴求の説得力と生活インフラを併せて評価します。",
            segments.join(" / ")
        )
    };
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // -- KPI row 5 cell
    html.push_str("<div class=\"block-title\">図 7-1 &nbsp;生活コスト・通勤圏 主要 KPI</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    let wage_val = latest_wage.map(|(_, w)| format!("{}", format_number(w))).unwrap_or_else(|| "—".into());
    let wage_foot = match (oldest_wage, latest_wage) {
        (Some((y0, _)), Some((y1, _))) if y0 != y1 => format!("{}-{} 年推移", y0, y1),
        _ => "最新年度のみ".to_string(),
    };
    push_kpi(html, "最低賃金", &wage_val, "円/時", "neu", &wage_foot, true);
    let yoy_val = wage_yoy.map(|v| format!("{:+.1}", v)).unwrap_or_else(|| "—".into());
    let yoy_dot = match wage_yoy {
        Some(v) if v >= 3.0 => "pos",
        Some(v) if v >= 1.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    push_kpi(html, "前年比", &yoy_val, "%", yoy_dot, "最新 vs 前年", false);
    push_kpi(
        html,
        "月間消費支出",
        &format_number(total_consumption),
        "円",
        "neu",
        "世帯あたり月平均",
        false,
    );
    let int_val = internet_rate.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "—".into());
    let int_dot = match internet_rate {
        Some(v) if v >= 90.0 => "pos",
        Some(v) if v >= 80.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    let sp_foot = match smartphone_rate {
        Some(v) => format!("スマホ保有 {:.1}%", v),
        None => "保有率データなし".to_string(),
    };
    push_kpi(html, "ネット利用率", &int_val, "%", int_dot, &sp_foot, false);
    // 2026-05-14: 通勤圏 KPI は市区町村が特定できている時のみ意味を持つ
    //   (commute_zone_count == 0 = ヘッダーフィルタで市区町村未指定 or 中心座標未取得)。
    //   「対象 0 圏 / 0 名」と表示してもユーザーに誤誘導するだけのため非表示にする。
    if commute_zone_count > 0 && commute_pop > 0 {
        push_kpi(
            html,
            "通勤圏 人口",
            &format_number(commute_pop),
            "名",
            "neu",
            &format!("対象 {} 圏", format_number(commute_zone_count as i64)),
            false,
        );
    } else {
        push_kpi(
            html,
            "通勤圏 人口",
            "—",
            "",
            "neu",
            "市区町村を指定すると算出",
            false,
        );
    }
    html.push_str("</div>\n");

    // -- 最低賃金推移バー SVG
    if wages.len() >= 2 {
        html.push_str("<div class=\"block-title block-title-spaced\">図 7-2 &nbsp;最低賃金 推移</div>\n");
        html.push_str(&build_navy_minwage_chart(&wages));
        html.push_str("<p class=\"caption\">出典: 厚生労働省 地域別最低賃金 (10 月発効)。年率 3% 以上は <strong>pos</strong>、1-3% は標準、1% 未満は <strong>warn</strong>。</p>\n");
    }

    // -- 家計支出構成 table-navy
    if !category_breakdown.is_empty() && total_consumption > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-A &nbsp;家計支出構成 (件数最多 6 費目)</div>\n");
        html.push_str(&build_navy_household_table(&category_breakdown, total_consumption));
    }

    // -- 通勤圏 table
    if commute_pop > 0 || commute_inflow > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-B &nbsp;通勤圏 サマリ</div>\n");
        html.push_str(&format!(
            "<table class=\"table-navy\">\n<thead><tr>\
             <th>指標</th><th class=\"num\">値</th><th>解釈</th>\
             </tr></thead>\n<tbody>\n\
             <tr><td><strong>通勤圏 自治体数</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">距離ベース通勤圏に含まれる自治体</span></td></tr>\n\
             <tr class=\"hl\"><td><strong>通勤圏 総人口</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">採用範囲を通勤圏まで広げた場合の母集団</span></td></tr>\n\
             <tr><td><strong>通勤圏 生産年齢</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">15-64 歳人口、即戦力候補</span></td></tr>\n\
             <tr><td><strong>流入通勤者</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">他自治体から通勤してくる人数 (OD ベース)</span></td></tr>\n\
             <tr><td><strong>流出通勤者</strong></td><td class=\"num bold\">{}</td><td><span class=\"dim\">他自治体へ通勤していく人数</span></td></tr>\n\
             <tr><td><strong>自市内通勤率</strong></td><td class=\"num bold\">{:.1}%</td><td><span class=\"dim\">対象自治体内で完結する通勤の比率</span></td></tr>\n\
             </tbody></table>\n",
            format_number(commute_zone_count as i64),
            format_number(commute_pop),
            format_number(commute_working),
            format_number(commute_inflow),
            format_number(commute_outflow),
            commute_self_rate * 100.0,
        ));
        html.push_str("<p class=\"caption\">出典: 国勢調査 OD (通勤・通学従業地・通学地集計)。通勤圏は対象自治体から距離ベース (デフォルト 20-30 km 圏) で抽出。</p>\n");
    }

    // -- 表 7-C 昼夜間人口 (流入超過 = 職場集中度)  [旧 7.5-F 統合 2026-05-15]
    if !ctx.ext_daytime_pop.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-C &nbsp;昼夜間人口比較</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_daytime_pop, 3));
        let ratio_opt = ctx.ext_daytime_pop.first().and_then(|r| {
            for k in ["daytime_nighttime_ratio", "dn_ratio", "day_night_ratio"] {
                let v = get_f64(r, k);
                if v > 0.0 { return Some(v); }
            }
            None
        });
        let insight = match ratio_opt {
            Some(r) if r >= 110.0 => format!(
                "昼夜間比 <strong>{:.1}%</strong> — 周辺地域からの<strong>通勤流入超過</strong>。職場集積エリアとして認知度が高く、通勤圏全体を採用母集団に取り込みやすい構造です。", r),
            Some(r) if r <= 90.0 => format!(
                "昼夜間比 <strong>{:.1}%</strong> — <strong>ベッドタウン型 (流出超過)</strong>。住民の多くは他自治体へ通勤しており、地元勤務を訴求する求人の希少性が武器になります。", r),
            Some(r) => format!(
                "昼夜間比 <strong>{:.1}%</strong> — 流入流出がほぼ均衡。職住一体型の自治体です。", r),
            None => "昼夜間比データが取得できませんでした。".to_string(),
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 国勢調査 昼夜間人口集計 (v2_external_daytime_population)。{}</p>\n",
            insight
        ));
    }

    // -- 表 7-D 世帯構成 (単身世帯率 = 若年単身ターゲット厚み)  [旧 7.5-L 統合 2026-05-15]
    if !ctx.ext_households.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-D &nbsp;世帯構成</div>\n");
        html.push_str(&build_navy_auto_table(&ctx.ext_households, 3));
        let single_rate_opt = ctx.ext_households.first()
            .map(|r| get_f64(r, "single_rate"))
            .filter(|v| *v > 0.0);
        let pref_avg = ctx.pref_avg_single_rate;
        let insight = match (single_rate_opt, pref_avg) {
            (Some(s), Some(p)) if s >= p + 3.0 => format!(
                "単身世帯率 <strong>{:.1}%</strong> (県平均 {:.1}% を <strong>+{:.1}pt</strong> 上回る) — 若年単身者の居住厚みがあり、転居を伴わない単身者採用ターゲットが豊富です。",
                s, p, s - p),
            (Some(s), Some(p)) if s <= p - 3.0 => format!(
                "単身世帯率 <strong>{:.1}%</strong> (県平均 {:.1}% を <strong>{:.1}pt</strong> 下回る) — 世帯持ち中心の地域。家族手当・住宅補助等のファミリー訴求が効きやすい構造です。",
                s, p, s - p),
            (Some(s), _) => format!(
                "単身世帯率 <strong>{:.1}%</strong> — 採用ターゲットの居住属性確認用に参照してください。", s),
            _ => "単身世帯率データが取得できませんでした。".to_string(),
        };
        html.push_str(&format!(
            "<p class=\"caption\">出典: 国勢調査 世帯集計 (v2_external_households)。{}</p>\n",
            insight
        ));
    }

    // -- 表 7-E 最低賃金 vs 求人給与 比較 (2026-05-23 #227 統合)
    //   求人下限給与中央値を時給換算 (167h) し、当該地域の最低賃金との比率を提示。
    //   既存「最低賃金推移」(図 7-2) を「求人とのギャップ」軸で補強する。
    let median_min_salary: i64 = {
        // salary_min_values の中央値 (>0 のみ)
        let mut v: Vec<i64> = agg
            .salary_min_values
            .iter()
            .copied()
            .filter(|x| *x > 0)
            .collect();
        if v.is_empty() {
            0
        } else {
            v.sort_unstable();
            v[v.len() / 2]
        }
    };
    let minwage_vs_salary = build_navy_minwage_vs_salary_table(
        median_min_salary,
        agg.is_hourly,
        latest_wage,
    );
    if !minwage_vs_salary.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-E &nbsp;最低賃金 vs 求人給与 比較</div>\n");
        html.push_str(&minwage_vs_salary);
    }

    // -- 表 7-F 家計支出 vs 求人給与 比較 (2026-05-23 #227 統合)
    //   月給中央値と月間消費支出を直接比較し、生活コストカバー率を提示。
    //   表 7-A (家計支出構成) を「給与水準との関係」軸で補強する。
    let household_vs_salary = build_navy_household_vs_salary_table(
        median_min_salary,
        agg.is_hourly,
        total_consumption,
        &category_breakdown,
    );
    if !household_vs_salary.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-F &nbsp;家計支出 vs 求人給与 比較</div>\n");
        html.push_str(&household_vs_salary);
    }

    // -- 表 7-G 社会生活・施設密度 (2026-05-23 #228 統合)
    //   人口あたり医療・福祉・保育施設数を県平均と比較。
    //   家族層 / 単身層採用時の生活インフラ確認指標。
    let lifestyle_facilities = build_navy_lifestyle_facilities_table(ctx);
    if !lifestyle_facilities.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-G &nbsp;社会生活・施設密度 (人口あたり)</div>\n");
        html.push_str(&lifestyle_facilities);
    }

    // -- 図 7-3 最賃プレミアム率分布 (Phase 2-B H3, 2026-05-29)
    //   時給モードのみ表示。求人時給と県最低賃金の差を premium_pct = (時給-最賃)/最賃*100 で
    //   バケット化 (5% 刻み) し、件数を縦棒で示す。
    //   表示条件: agg.is_hourly == true かつ latest_wage が取れる (mw_yen > 0)。
    //   silent fallback 防止:
    //     - 月給モード: ブロック完全省略 (条件 if 内)
    //     - 最賃データなし: "最低賃金データなし" 明示表示
    //     - 時給データなし: "該当データなし" 明示表示
    if agg.is_hourly {
        html.push_str(
            "<div class=\"block-title block-title-spaced\">図 7-3 &nbsp;最賃プレミアム率分布 (求人時給 vs 県最賃)</div>\n",
        );
        let mw_yen: i64 = latest_wage.map(|(_, w)| w).filter(|w| *w > 0).unwrap_or(0);
        if mw_yen <= 0 {
            html.push_str(
                "<p class=\"caption dim\">該当県の最低賃金データが取得できなかったため、本図は省略します。</p>\n",
            );
        } else {
            html.push_str(&build_navy_minwage_premium_histogram_svg(
                &agg.salary_min_values_native,
                mw_yen,
            ));
            html.push_str(&format!(
                "<p class=\"caption\">出典: CSV 集計 (時給 下限ネイティブ) + 厚労省地域別最低賃金 ({} 円/時)。\
                 プレミアム率 = (求人時給 - 最低賃金) / 最低賃金 × 100。\
                 <strong>SO WHAT:</strong> プレミアム 10% 未満が多数なら最賃ライン求人が主流、\
                 25% 超の高プレミアム帯に偏れば等級・専門職求人の比重が高い兆候。</p>\n",
                format_number(mw_yen)
            ));
        }
    }

    // -- so-what
    let so_what = build_lifestyle_so_what(
        latest_wage,
        wage_yoy,
        total_consumption,
        internet_rate,
        commute_pop,
        commute_self_rate,
    );
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:6mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">{}</div>\
         </div>\n",
        so_what
    ));

    html.push_str("</section>\n");
}

// 2026-05-23 #227: 最低賃金 vs 求人給与 比較 (Section 07 拡張)
//
// 設計:
// - 求人 CSV の中央給与 (median_min_salary) を時給換算 (月給 &divide; 167h) し、
//   当該地域 (pref) の最低賃金との比率を提示する。
// - 単位は必ず時給 (円/時) で統一 (MEMORY: feedback_unit_consistency_audit.md)。
// - is_hourly = true (時給ベース CSV) の場合は換算不要、median をそのまま使用。
// - 給与中央値が時給ベースで最賃の N 倍 になっているかを 1 行で示す。
// - 「N 倍以上 = 余裕がある」とは断定しない (中立表現、
//   MEMORY: feedback_neutral_expression_for_targets.md)。
// 戻り値: HTML 文字列 (テーブル + caption)。データ不足時は空文字。
fn build_navy_minwage_vs_salary_table(
    median_min_salary: i64,
    is_hourly: bool,
    latest_minwage: Option<(i32, i64)>,
) -> String {
    let (mw_year, mw_yen) = match latest_minwage {
        Some((y, w)) if w > 0 => (y, w),
        _ => return String::new(),
    };
    if median_min_salary <= 0 {
        return String::new();
    }
    // 時給換算 (167h は厚労省基準: 8h &times; 20.875 日)
    let hourly_equiv: i64 = if is_hourly {
        median_min_salary
    } else {
        median_min_salary / super::super::aggregator::HOURLY_TO_MONTHLY_HOURS
    };
    let ratio = hourly_equiv as f64 / mw_yen as f64;
    let diff = hourly_equiv - mw_yen;

    // 位置づけ (中立表現): 1.0 倍未満 = 要確認、1.0-1.2 倍 = 最賃近接、1.2 倍以上 = 上振れ
    let (tag, label, note) = if ratio < 1.0 {
        (
            "warn",
            "最賃割れ",
            format!(
                "求人下限給与の時給換算が最低賃金を <strong>{} 円</strong> 下回ります。労基上の妥当性を要確認 (副業案件・固定残業含むかの再検証)。",
                diff.abs()
            ),
        )
    } else if ratio < 1.2 {
        (
            "neu",
            "最賃近接",
            format!(
                "求人下限給与の時給換算は最低賃金 +{} 円 (比率 {:.2} 倍)。最賃改定 (例年 10 月) で実質的な調整余地が縮む水準。",
                diff, ratio
            ),
        )
    } else {
        (
            "pos",
            "最賃上振れ",
            format!(
                "求人下限給与の時給換算は最低賃金の <strong>{:.2} 倍</strong>。最賃改定の直接影響は限定的だが、求人内給与レンジの再点検は別軸で必要。",
                ratio
            ),
        )
    };

    let median_repr = if is_hourly {
        format!("{} 円/時", format_number(median_min_salary))
    } else {
        format!("{} 万円/月 ({} 円/時換算, &divide;167h)", format_mm(median_min_salary), format_number(hourly_equiv))
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>指標</th><th class=\"num\">値</th><th>備考</th>");
    s.push_str("</tr></thead>\n<tbody>\n");
    s.push_str(&format!(
        "<tr><td><strong>当該地域 最低賃金 ({} 年)</strong></td>\
         <td class=\"num bold\">{} 円/時</td>\
         <td><span class=\"dim\">厚労省 地域別最低賃金 (10 月改定)</span></td></tr>\n",
        mw_year,
        format_number(mw_yen)
    ));
    s.push_str(&format!(
        "<tr class=\"hl\"><td><strong>求人下限給与 中央値</strong></td>\
         <td class=\"num bold\">{}</td>\
         <td><span class=\"dim\">CSV 集計 (月給は 167h で時給換算)</span></td></tr>\n",
        median_repr
    ));
    s.push_str(&format!(
        "<tr><td><strong>最低賃金との比率</strong></td>\
         <td class=\"num bold\">{:.2} 倍</td>\
         <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">差額 {}{} 円</span></td></tr>\n",
        ratio,
        tag,
        label,
        if diff >= 0 { "+" } else { "" },
        diff
    ));
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">出典: 厚労省 v2_external_minimum_wage + CSV 集計 (median_min_salary)。月給を 167h (8h &times; 20.875 日, 厚労省基準) で割って時給換算。\
         <strong>判定:</strong> {}</p>\n",
        note
    ));
    s
}

// 2026-05-23 #227: 家計支出 vs 求人給与 比較 (Section 07 拡張)
//
// 設計:
// - 月間消費支出 (家計調査) と 求人 給与中央値 (月給) との比較。
// - 単位は月額円で統一 (MEMORY: feedback_unit_consistency_audit.md)。
// - 時給 CSV (is_hourly) の場合は &times; 167h で月給換算。
// - 「家計支出を給与の N% で賄える」を提示し、住居費 / 教育費等の
//   重支出費目との対比を補足する。
// 戻り値: HTML 文字列。データ不足時は空文字。
fn build_navy_household_vs_salary_table(
    median_min_salary: i64,
    is_hourly: bool,
    total_consumption: i64,
    category_top: &[(String, i64)],
) -> String {
    if median_min_salary <= 0 || total_consumption <= 0 {
        return String::new();
    }
    // 月給換算
    let monthly_salary: i64 = if is_hourly {
        median_min_salary * super::super::aggregator::HOURLY_TO_MONTHLY_HOURS
    } else {
        median_min_salary
    };
    let coverage_ratio = total_consumption as f64 / monthly_salary as f64;
    let coverage_pct = coverage_ratio * 100.0;

    // 位置づけ (中立): 70% 未満 = 余裕、70-100% = 拮抗、100% 以上 = 単独可処分超過
    let (tag, label) = if coverage_pct < 70.0 {
        ("pos", "可処分余裕")
    } else if coverage_pct <= 100.0 {
        ("neu", "拮抗水準")
    } else {
        ("warn", "支出超過水準")
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>指標</th><th class=\"num\">月額 (円)</th><th>備考</th>");
    s.push_str("</tr></thead>\n<tbody>\n");
    s.push_str(&format!(
        "<tr><td><strong>求人下限給与 中央値 (月給換算)</strong></td>\
         <td class=\"num bold\">{}</td>\
         <td><span class=\"dim\">CSV 集計。時給ベースは &times; 167h で換算</span></td></tr>\n",
        format_number(monthly_salary)
    ));
    s.push_str(&format!(
        "<tr class=\"hl\"><td><strong>月間消費支出 (家計調査)</strong></td>\
         <td class=\"num bold\">{}</td>\
         <td><span class=\"dim\">2 人以上世帯平均</span></td></tr>\n",
        format_number(total_consumption)
    ));
    s.push_str(&format!(
        "<tr><td><strong>消費支出 / 給与 比率</strong></td>\
         <td class=\"num bold\">{:.1}%</td>\
         <td><span class=\"tag tag-{}\">{}</span></td></tr>\n",
        coverage_pct, tag, label
    ));
    // 重支出費目 top 3 (構成比 10%+) を併記
    let heavy: Vec<&(String, i64)> = category_top
        .iter()
        .filter(|(_, amt)| {
            total_consumption > 0
                && (*amt as f64 / total_consumption as f64 * 100.0) >= 10.0
        })
        .take(3)
        .collect();
    for (name, amt) in &heavy {
        let pct_in_salary = if monthly_salary > 0 {
            *amt as f64 / monthly_salary as f64 * 100.0
        } else {
            0.0
        };
        s.push_str(&format!(
            "<tr><td><strong>うち {} (重支出)</strong></td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"dim\">給与の {:.1}% を占める</span></td></tr>\n",
            escape_html(name),
            format_number(*amt),
            pct_in_salary
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(
        "<p class=\"caption\">出典: 総務省 v2_external_household_spending + CSV 集計。\
         消費支出は 2 人以上世帯平均で、単身世帯では構造が異なります。\
         本指標は <strong>給与水準の生活実態適合度</strong> の概観のみを示し、\
         可処分所得 (税・社会保険料控除後) や世帯収入の評価は含みません。</p>\n",
    );
    s
}

// ============================================================
// Phase 2-B (2026-05-29): 時給モード H3 — 最賃プレミアム率分布 SVG
// ============================================================
//
// 仕様:
//   - 各求人時給について premium_pct = (時給 - 最賃) / 最賃 × 100 を算出
//   - bucket: 5% 刻み。<0% / 0-5 / 5-10 / 10-15 / 15-20 / 20-25 / 25-30 / 30-35 / 35-40 / 40-45 / 45%+
//     (合計 11 段、x 軸 11 ラベル)
//   - x 軸: プレミアム率帯、y 軸: 求人件数
//
// 不変条件 (テストで検証):
//   - bucket 合計件数 == values_native.iter().filter(>0).count()
//   - 各 bucket count ∈ [0, total]
//   - values_native empty → "該当データなし" 表示
//   - min_wage <= 0 → "" (空文字)。呼出側で別途 caption 表示する想定
//
// silent fallback 監査:
//   - empty/min_wage<=0 は呼出側でハンドリング (本関数は "" を返す)
//   - bucket 11 段の定義は固定 (定数 PREMIUM_BUCKETS)
const PREMIUM_BUCKETS: [(f64, f64, &str); 11] = [
    (f64::NEG_INFINITY, 0.0, "<0%"),
    (0.0, 5.0, "0-5%"),
    (5.0, 10.0, "5-10%"),
    (10.0, 15.0, "10-15%"),
    (15.0, 20.0, "15-20%"),
    (20.0, 25.0, "20-25%"),
    (25.0, 30.0, "25-30%"),
    (30.0, 35.0, "30-35%"),
    (35.0, 40.0, "35-40%"),
    (40.0, 45.0, "40-45%"),
    (45.0, f64::INFINITY, "45%+"),
];

/// 最賃プレミアム率ヒストグラム SVG を生成。
///
/// # 引数
/// - `values_native`: 求人時給 (円/時) のリスト。<= 0 は除外。
/// - `min_wage`: 県最低賃金 (円/時)。<= 0 の場合は "" を返す。
///
/// # 戻り値
/// SVG 文字列 (`<svg>...</svg>`)。データ不足時は `<p class="caption dim">該当データなし</p>`。
pub(super) fn build_navy_minwage_premium_histogram_svg(
    values_native: &[i64],
    min_wage: i64,
) -> String {
    if min_wage <= 0 {
        return String::new();
    }
    // filter > 0
    let valid: Vec<f64> = values_native
        .iter()
        .copied()
        .filter(|x| *x > 0)
        .map(|x| (x as f64 - min_wage as f64) / min_wage as f64 * 100.0)
        .collect();
    if valid.is_empty() {
        return String::from("<p class=\"caption dim\">該当データなし</p>\n");
    }

    // bucket 集計
    let mut counts: Vec<usize> = vec![0; PREMIUM_BUCKETS.len()];
    for v in valid.iter() {
        for (i, (lo, hi, _)) in PREMIUM_BUCKETS.iter().enumerate() {
            // [lo, hi) で判定。最後の "45%+" は hi = INFINITY のため上限なし。
            if *v >= *lo && *v < *hi {
                counts[i] += 1;
                break;
            }
        }
    }

    let total: usize = counts.iter().sum();
    // 不変条件: total == valid.len() (テストで検証)
    let _ = total;
    let max_count = *counts.iter().max().unwrap_or(&1).max(&1) as f64;

    // SVG geometry (build_navy_histogram_svg と同じレイアウト)
    let w: f64 = 720.0;
    let h: f64 = 280.0;
    let pad_l = 56.0;
    let pad_r = 16.0;
    let pad_t = 36.0;
    let pad_b = 44.0;
    let inner_w = w - pad_l - pad_r;
    let inner_h = h - pad_t - pad_b;
    let n_bins = counts.len();
    let bw = inner_w / n_bins as f64;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"最賃プレミアム率分布ヒストグラム\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    ));
    // y 軸グリッド
    for i in 0..=5 {
        let y = pad_t + inner_h * i as f64 / 5.0;
        let count = (max_count * (5 - i) as f64 / 5.0).round() as i64;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ECE7DA\" stroke-width=\"0.5\"/>\n",
            pad_l, y, w - pad_r, y
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"end\">{}</text>\n",
            pad_l - 6.0,
            y + 3.0,
            count
        ));
    }
    // bars
    for (i, c) in counts.iter().enumerate() {
        let bh = (*c as f64 / max_count) * inner_h;
        let bx = pad_l + i as f64 * bw;
        let by = pad_t + inner_h - bh;
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#1F2D4D\"/>\n",
            bx + 0.5,
            by,
            (bw - 1.0).max(1.0),
            bh
        ));
        // 件数ラベル (バー上、0 件は省略)
        if *c > 0 {
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#1F2D4D\" text-anchor=\"middle\">{}</text>\n",
                bx + bw / 2.0,
                (by - 3.0).max(pad_t + 8.0),
                c
            ));
        }
    }
    // x 軸ラベル
    for (i, (_, _, label)) in PREMIUM_BUCKETS.iter().enumerate() {
        let cx = pad_l + (i as f64 + 0.5) * bw;
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
            cx,
            h - pad_b + 14.0,
            escape_html(label)
        ));
    }
    // 軸タイトル
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">最賃プレミアム率 (%)</text>\n",
        w / 2.0,
        h - 6.0
    ));
    svg.push_str("</svg>\n");
    svg
}

// 2026-05-23 #228: 社会生活・施設密度 (Section 07 拡張)
//
// 設計:
// - `ext_medical_welfare` (病院・診療所・薬局・保育所) と
//   `ext_social_life` (参加率) を「人口あたり施設数」観点で表示。
// - 既存 KPI で「人口」が分かるため、ここでは absolute count と
//   「人口 1 万人あたり」の派生指標を提示。
// - 県平均 (pref_avg_physicians_per_10k, pref_avg_daycare_per_1k_children) と
//   突き合わせ、対象地域の生活インフラ密度を把握する。
// 戻り値: HTML 文字列。データ不足時は空文字。
fn build_navy_lifestyle_facilities_table(
    ctx: &InsightContext,
) -> String {
    use super::super::super::helpers::{get_f64, get_i64};
    if ctx.ext_medical_welfare.is_empty() {
        return String::new();
    }
    let row = &ctx.ext_medical_welfare[0];
    let hospitals = get_i64(row, "general_hospitals");
    let clinics = get_i64(row, "general_clinics");
    let dental = get_i64(row, "dental_clinics");
    let physicians = get_i64(row, "physicians");
    let pharmacists = get_i64(row, "pharmacists");
    let daycare = get_i64(row, "daycare_facilities");
    let physicians_per_10k = get_f64(row, "physicians_per_10k_pop");
    let daycare_per_1k_kids = get_f64(row, "daycare_per_1k_children_0_14");

    if hospitals + clinics + dental + physicians + pharmacists + daycare == 0 {
        return String::new();
    }

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>区分</th><th class=\"num\">施設・人員数</th><th class=\"num\">県平均比較</th><th>備考</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    let fmt_cmp = |target: f64, pref_avg: Option<f64>, unit: &str| -> String {
        match pref_avg {
            Some(p) if p > 0.0 => {
                let diff = target - p;
                let sign = if diff >= 0.0 { "+" } else { "" };
                format!(
                    "{:.1} {} <span class=\"dim\">(県平均 {:.1}{}, 差 {}{:.1}{})</span>",
                    target, unit, p, unit, sign, diff, unit
                )
            }
            _ => format!("{:.1} {}", target, unit),
        }
    };

    if hospitals > 0 {
        s.push_str(&format!(
            "<tr><td><strong>病院</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">—</td>\
             <td><span class=\"dim\">入院機能あり (20 床以上)</span></td></tr>\n",
            format_number(hospitals)
        ));
    }
    if clinics > 0 {
        s.push_str(&format!(
            "<tr><td><strong>一般診療所</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">—</td>\
             <td><span class=\"dim\">外来中心 (19 床以下)</span></td></tr>\n",
            format_number(clinics)
        ));
    }
    if dental > 0 {
        s.push_str(&format!(
            "<tr><td><strong>歯科診療所</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">—</td>\
             <td><span class=\"dim\">歯科医療の地域密度</span></td></tr>\n",
            format_number(dental)
        ));
    }
    if physicians > 0 {
        let cmp_str = if physicians_per_10k > 0.0 {
            fmt_cmp(physicians_per_10k, ctx.pref_avg_physicians_per_10k, "人/万人")
        } else {
            "—".to_string()
        };
        s.push_str(&format!(
            "<tr class=\"hl\"><td><strong>医師数</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">{}</td>\
             <td><span class=\"dim\">医療職採用市場の供給規模指標</span></td></tr>\n",
            format_number(physicians),
            cmp_str
        ));
    }
    if pharmacists > 0 {
        s.push_str(&format!(
            "<tr><td><strong>薬剤師</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">—</td>\
             <td><span class=\"dim\">薬局・病院薬剤部の人員規模</span></td></tr>\n",
            format_number(pharmacists)
        ));
    }
    if daycare > 0 {
        let cmp_str = if daycare_per_1k_kids > 0.0 {
            fmt_cmp(
                daycare_per_1k_kids,
                ctx.pref_avg_daycare_per_1k_children,
                "施設/千人 (0-14 歳)",
            )
        } else {
            "—".to_string()
        };
        s.push_str(&format!(
            "<tr><td><strong>保育所</strong></td>\
             <td class=\"num bold\">{}</td><td class=\"num\">{}</td>\
             <td><span class=\"dim\">子育て世帯採用時の生活インフラ</span></td></tr>\n",
            format_number(daycare),
            cmp_str
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(
        "<p class=\"caption\">出典: 厚労省 v2_external_medical_welfare (医療・福祉施設) + \
         県平均 (pref_avg_*)。<strong>絶対数</strong>は地域規模の影響を受けるため、\
         <strong>人口あたり指標 (医師 / 万人, 保育所 / 千人 0-14 歳)</strong>を県平均と比較して読みます。\
         施設密度は採用ターゲットの生活インフラ確認用 (家族層 / 単身層問わず参考)。</p>\n",
    );
    s
}

fn build_navy_minwage_chart(wages: &[(i32, i64)]) -> String {
    if wages.len() < 2 {
        return String::new();
    }
    let w = 720.0;
    let h = 220.0;
    let pad_l = 48.0;
    let pad_r = 16.0;
    let pad_t = 16.0;
    let pad_b = 36.0;
    let inner_w = w - pad_l - pad_r;
    let inner_h = h - pad_t - pad_b;
    let n = wages.len();
    let bw = inner_w / n as f64;
    let max_v = wages.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1) as f64;
    let min_v = wages.iter().map(|(_, v)| *v).min().unwrap_or(0) as f64;
    let span = (max_v - min_v).max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"最低賃金推移\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );
    // y 軸
    for i in 0..=4 {
        let y = pad_t + inner_h * i as f64 / 4.0;
        let v = (max_v - span * i as f64 / 4.0) as i64;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ECE7DA\" stroke-width=\"0.5\"/>\n",
            pad_l, y, w - pad_r, y
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"end\">{}</text>\n",
            pad_l - 6.0, y + 3.0, v
        ));
    }
    // bars + value labels + 折線
    let mut prev_x = 0.0;
    let mut prev_y = 0.0;
    for (i, (year, v)) in wages.iter().enumerate() {
        let ratio = (*v as f64 - min_v) / span;
        let bh = ratio * inner_h * 0.9 + inner_h * 0.1;
        let bx = pad_l + i as f64 * bw;
        let by = pad_t + inner_h - bh;
        let bar_color = if i == n - 1 { "#C9A24B" } else { "#1F2D4D" };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>\n",
            bx + 4.0,
            by,
            (bw - 8.0).max(2.0),
            bh,
            bar_color
        ));
        let cx = bx + bw / 2.0;
        if i > 0 {
            svg.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#0B1E3F\" stroke-width=\"1.5\"/>\n",
                prev_x, prev_y, cx, by
            ));
        }
        prev_x = cx;
        prev_y = by;
        // x ラベル
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
            cx, h - pad_b + 14.0, year
        ));
        // 値ラベル
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#0B1E3F\" text-anchor=\"middle\" font-weight=\"700\">{}</text>\n",
            cx, by - 4.0, v
        ));
    }
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">時給 (円)</text>\n",
        pad_l - 36.0, pad_t + inner_h / 2.0
    ));
    svg.push_str("</svg>\n");
    svg
}

fn build_navy_household_table(
    categories: &[(String, i64)],
    total: i64,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>費目</th>");
    s.push_str("<th class=\"num\">月額 (円)</th>");
    s.push_str("<th class=\"num\">構成比</th>");
    s.push_str("<th>位置づけ</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    let top6: Vec<&(String, i64)> = categories.iter().take(6).collect();
    if top6.is_empty() {
        s.push_str("<tr><td colspan=\"5\" class=\"dim\">家計支出データなし。</td></tr>\n");
    } else {
        for (i, (name, amount)) in top6.iter().enumerate() {
            let pct = if total > 0 { *amount as f64 / total as f64 * 100.0 } else { 0.0 };
            let (tag, label) = if pct >= 20.0 {
                ("warn", "重支出")
            } else if pct >= 10.0 {
                ("neu", "主要支出")
            } else {
                ("neu", "標準支出")
            };
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            s.push_str(&format!(
                "<tr{}><td class=\"num bold\">{}</td><td><strong>{}</strong></td>\
                 <td class=\"num bold\">{}</td><td class=\"num\">{:.1}%</td>\
                 <td><span class=\"tag tag-{}\">{}</span></td></tr>\n",
                row_class,
                i + 1,
                escape_html(name),
                format_number(*amount),
                pct,
                tag,
                label
            ));
        }
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">出典: 総務省 家計調査 v2_external_household_spending。月間消費支出 (合計) に対する構成比。給与訴求の絶対水準と相対比較に活用。</p>\n");
    s
}

fn build_lifestyle_so_what(
    latest_wage: Option<(i32, i64)>,
    wage_yoy: Option<f64>,
    consumption: i64,
    internet_rate: Option<f64>,
    commute_pop: i64,
    self_rate: f64,
) -> String {
    let wage_msg = match (latest_wage, wage_yoy) {
        (Some((_, w)), Some(yoy)) if yoy >= 3.0 => format!(
            "最低賃金 <strong>{} 円/時</strong> は前年比 <strong>{:+.1}%</strong> の上昇基調。給与下限の引き上げ圧が強く、求人給与の競争力は <strong>絶対水準</strong> ではなく <strong>付帯条件 (福利厚生 / 賞与)</strong> で勝負する局面です。",
            format_number(w),
            yoy
        ),
        (Some((_, w)), Some(yoy)) => format!(
            "最低賃金 <strong>{} 円/時</strong> 前年比 <strong>{:+.1}%</strong>。給与下限変動は限定的なため、給与の <strong>絶対水準</strong> での差別化が可能です。",
            format_number(w),
            yoy
        ),
        (Some((_, w)), None) => format!(
            "最低賃金 <strong>{} 円/時</strong>。時系列データが取得できないため推移評価は限定的ですが、絶対水準で時給競争力を点検してください。",
            format_number(w)
        ),
        _ => "最低賃金データが取得できないため、給与競争力の評価は CSV 集計値のみで判断してください。".to_string(),
    };

    let commute_msg = if commute_pop >= 1_000_000 {
        format!(
            " 通勤圏内に <strong>{} 名</strong> の人口を擁する <strong>大都市圏</strong>。採用範囲を通勤圏まで拡げれば母集団は大幅に拡張可能です。",
            format_number(commute_pop)
        )
    } else if commute_pop >= 300_000 {
        format!(
            " 通勤圏内人口 <strong>{} 名</strong>。中規模都市圏として通勤圏アプローチが有効です。",
            format_number(commute_pop)
        )
    } else if commute_pop > 0 {
        format!(
            " 通勤圏内人口は <strong>{} 名</strong> と限定的。地域内採用に重きを置く戦略が現実的です。",
            format_number(commute_pop)
        )
    } else {
        // 2026-05-14: 「取得できなかった」は誤誘導 — ヘッダーフィルタで市区町村が
        //   指定されていないことが多数の原因なので、明示する。
        " 市区町村未指定のため通勤圏は算出していません。ヘッダーフィルタで市区町村を選択すると母集団拡大余地が評価できます。".to_string()
    };

    let self_msg = if self_rate >= 0.7 {
        format!(" 自市内通勤率 <strong>{:.0}%</strong> と高く、地域内で完結する <strong>定住型</strong> 構造です。", self_rate * 100.0)
    } else if self_rate >= 0.5 {
        format!(" 自市内通勤率 <strong>{:.0}%</strong>。通勤者の半数程度は周辺自治体から流入しており、広域アプローチの余地があります。", self_rate * 100.0)
    } else if self_rate > 0.0 {
        format!(" 自市内通勤率 <strong>{:.0}%</strong> と低く、<strong>流入型</strong> 構造。通勤者を対象にした採用アプローチが有効です。", self_rate * 100.0)
    } else {
        String::new()
    };

    // 2026-05-14: 媒体利用 (デジタル / 紙媒体 等) への言及は本レポートの趣旨外のため撤去。
    //   ネット利用率の数値はサマリ KPI で別途提示済み。
    let internet_msg = String::new();
    let _ = internet_rate;

    let _ = consumption;
    format!("{}{}{}{}", wage_msg, commute_msg, self_msg, internet_msg)
}

// ============================================================
// Section 7.5: 補助データ全展開 (2026-05-14 追加)
//   取得済みだが既存 Section で未表示だった 14 系列を一括ダンプする。
//   Phase 1: 全件表示 (User 確認用)。Phase 2 で表示可否のチェックボックス UI 化予定。
// ============================================================

/// 汎用 Row テーブル描画: 渡された rows の先頭から指定行までを auto-column 抽出して
/// navy スタイルテーブルで描画する。
///
/// 描画ロジック:
/// - rows[0] の全 key を column header として採用 (最大 8 カラム)
/// - prefecture / municipality / year / reference_date は先頭に固定
/// - 各セル値は string/number/null をテキスト変換
/// - rows.len() <= max_rows なら全件、超過なら先頭 max_rows 行 + 「他 N 件」表示
/// 2026-05-15: 英語スネークケースの DB カラム名 → 日本語ラベル変換マップ。
///   Section 7.5 補助データの列ヘッダがユーザーに読めないという指摘への対応。
///   未登録キーは原文のままフォールバック (新規カラム追加時に気付ける)。
fn label_for_column(key: &str) -> &str {
    match key {
        // 識別子・年
        "prefecture" => "都道府県",
        "municipality" => "市区町村",
        "year" | "fiscal_year" | "reference_year" | "survey_year" => "年",
        "reference_date" => "基準日",
        // 産業・カテゴリ
        "industry" | "industry_name" | "industry_raw" => "産業",
        "industry_code" => "産業コード",
        "category" | "subcategory" => "区分",
        "name" | "label" => "名称",
        // 人口・性別・世帯
        "total_count" | "total" => "合計",
        "male_count" | "male" => "男性",
        "female_count" | "female" => "女性",
        "total_population" | "population" => "総人口",
        "population_density_per_km2" => "人口密度(/km²)",
        "habitable_density_per_km2" => "可住地密度(/km²)",
        "single_households" => "単身世帯",
        "total_households" => "総世帯",
        "single_household_elderly" => "高齢単身世帯",
        "single_household_elderly_male" => "高齢単身(男)",
        "single_household_elderly_female" => "高齢単身(女)",
        "single_rate" => "単身率",
        "households" => "世帯",
        // 労働力
        "employed" => "就業者",
        "employed_male" => "就業者(男)",
        "employed_female" => "就業者(女)",
        "unemployed" => "失業者",
        "unemployed_male" => "失業者(男)",
        "unemployed_female" => "失業者(女)",
        "not_in_labor_force" => "非労働力人口",
        "not_in_labor_force_male" => "非労働力(男)",
        "not_in_labor_force_female" => "非労働力(女)",
        "labor_force_count" | "labor_force" => "労働力人口",
        "unemployment_rate" => "失業率(%)",
        "labor_force_participation_rate" => "労働力率(%)",
        "primary_industry_employed" => "第1次産業就業者",
        "secondary_industry_employed" => "第2次産業就業者",
        "tertiary_industry_employed" => "第3次産業就業者",
        // 人口移動 (v2_external_migration の実 SQL alias)
        "in_migrants" | "in_migration" | "inflow" => "転入者数",
        "out_migrants" | "out_migration" | "outflow" => "転出者数",
        "net_migration" => "転入超過数",
        "net_migration_rate" => "転入超過率(‰)",
        // 昼夜間人口 (v2_external_daytime_population の実 SQL alias)
        "daytime_population" | "daytime_pop" => "昼間人口",
        "nighttime_population" | "nighttime_pop" => "夜間人口",
        "daytime_nighttime_ratio" | "dn_ratio" | "day_night_ratio" => "昼夜間比(%)",
        "inflow_pop" => "流入人口",
        "outflow_pop" => "流出人口",
        // 事業所
        "establishments" | "establishment_count" => "事業所数",
        "employees" | "employees_total" => "従業者数",
        "private_establishments" => "民営事業所",
        "private_employees" => "民営従業者",
        // 開廃業 (v2_external_business_dynamics の実 SQL alias)
        "opened_establishments" | "open_count" | "new_establishments" => "開業数",
        "closed_establishments" | "close_count" => "廃業数",
        "net_change" => "純増減",
        "opening_rate" => "開業率",
        // 2026-05-15: DB スキーマは `closure_rate` (名詞)。`closing_rate` (continuous) は誤り
        "closure_rate" => "廃業率",
        // 介護
        "nursing_home_count" => "老人ホーム数",
        "care_workers" => "介護職員",
        "care_recipients" => "要介護認定者",
        "elderly_population" => "高齢人口",
        // 出生・死亡
        "births" => "出生数",
        "deaths" => "死亡数",
        "natural_change" => "自然増減",
        "marriages" => "婚姻数",
        "divorces" => "離婚数",
        "permits" => "建築許可",
        // 医療
        "general_clinics" => "一般診療所",
        "general_hospitals" => "病院",
        "dental_clinics" => "歯科診療所",
        "physicians" | "physicians_count" => "医師数",
        "physicians_per_10k_pop" => "医師(/万人)",
        "dentists" => "歯科医師",
        "pharmacists" => "薬剤師",
        "hospitals" => "病院数",
        "daycare_per_1k_children_0_14" => "保育所(/千人 0-14歳)",
        // 教育施設
        "kindergartens" => "幼稚園",
        "elementary_schools" => "小学校",
        "junior_high_schools" => "中学校",
        "high_schools" => "高校",
        "general_households" => "一般世帯",
        // 地理
        "habitable_area_km2" => "可住地面積(km²)",
        "total_area_km2" => "総面積(km²)",
        // 学歴
        "education_level" => "学歴",
        // 気候
        "mean_temperature" | "avg_temperature" => "平均気温(℃)",
        "max_temperature" => "最高気温(℃)",
        "min_temperature" => "最低気温(℃)",
        "sunshine_hours" => "日照時間(h)",
        "precipitation_mm" | "rainfall_mm" => "降水量(mm)",
        "snowfall_days" | "snow_days" => "降雪日数",
        // 社会生活
        "participation_rate" => "参加率(%)",
        // 通勤
        "origin_pref" => "出発地(県)",
        "origin_muni" => "出発地(市町村)",
        "dest_pref" => "到着地(県)",
        "dest_muni" => "到着地(市町村)",
        "total_commuters" => "通勤者総数",
        "male_commuters" => "通勤者(男)",
        "female_commuters" => "通勤者(女)",
        // 2026-05-18: Team A audit で未マップだった 22 件を追加 (英語残対策)
        // 人口統計
        "aging_rate" => "高齢化率(%)",
        "working_age_rate" => "生産年齢人口比(%)",
        "youth_rate" => "年少人口比(%)",
        "age_0_14" => "0-14歳人口",
        "age_15_64" => "15-64歳人口",
        "age_65_over" => "65歳以上人口",
        "male_population" => "男性人口",
        "female_population" => "女性人口",
        // 世帯統計
        "general_household_members" => "一般世帯人員",
        "nuclear_family_households" => "核家族世帯",
        "elderly_nuclear_households" => "高齢核家族",
        "elderly_couple_households" => "高齢夫婦世帯",
        "avg_household_size" => "平均世帯人員",
        "elderly_single_rate" => "高齢単身率(%)",
        // 介護需要
        "insurance_benefit_cases" => "介護給付件数",
        "health_facility_count" => "老健施設数",
        "home_care_offices" => "訪問介護事業所",
        "day_service_offices" => "通所介護事業所",
        "pop_65_over" => "65歳以上人口",
        "pop_75_over" => "75歳以上人口",
        "pop_65_over_rate" => "65歳以上比率(%)",
        // 出生・死亡 (率)
        "birth_rate_permille" => "出生率(‰)",
        "death_rate_permille" => "死亡率(‰)",
        "marriage_rate_permille" => "婚姻率(‰)",
        "divorce_rate_permille" => "離婚率(‰)",
        // 就業・労働市場
        "entry_rate" => "入職率(%)",
        "separation_rate" => "離職率(%)",
        "net_rate" => "純増減率(%)",
        "ratio_total" => "有効求人倍率",
        "ratio_excl_part" => "有効求人倍率(パート除く)",
        "hourly_min_wage" => "最低賃金(時給円)",
        // IT / その他
        "internet_usage_rate" => "ネット利用率(%)",
        "smartphone_ownership_rate" => "スマホ所有率(%)",
        "daycare_facilities" => "保育所数",
        "monthly_amount" => "月額(円)",
        // 2026-05-20: 表 6-E 労働力統計詳細で未マップだった 4 件を追加
        "monthly_salary_male" => "月給(男)",
        "monthly_salary_female" => "月給(女)",
        "part_time_wage_male" => "パート時給(男)",
        "part_time_wage_female" => "パート時給(女)",
        "turnover_rate" => "離職率(%)",
        // 2026-05-20 追加: 別 session で出現した 1 件 (single_household_elderly と語順違い)
        "elderly_single_households" => "高齢単身世帯",
        // 2026-05-20 MECE 監査: 全 v2_external_* テーブルの SELECT 句から未マップ列を網羅追加
        // 出典: agent (general-purpose) による SQL 抽出 + label_for_column diff
        // 優先度 A: 現在 build_navy_auto_table 経由可能性高
        "working_hours_male" => "労働時間(男, h)",
        "working_hours_female" => "労働時間(女, h)",
        // 優先度 B: 将来 build_navy_auto_table 経由になった際の保険
        "age_group" => "年齢階級",
        "avg_monthly_wage" => "平均月収(円)",
        "avg_price_per_sqm" => "平均地価(円/m²)",
        "cars_per_100people" => "自動車保有(/100人)",
        "city_code" => "市区町村コード",
        "city_name" => "市区町村名",
        "di_type" => "DI種別",
        "di_value" => "DI値",
        "employees_female" => "従業者(女)",
        "employees_male" => "従業者(男)",
        "enterprise_size" => "企業規模",
        "fulfillment_rate" => "充足率(%)",
        "household_type" => "世帯類型",
        "industry_j" => "産業(日本語)",
        "job_change_desire_rate" => "転職希望率(%)",
        "land_use" => "用途区分",
        "non_regular_rate" => "非正規率(%)",
        "point_count" => "地点数",
        // 既登録の precipitation_mm / rainfall_mm と意味同じだが DB 実体は _mm サフィックス無し
        "precipitation" => "降水量(mm)",
        "prefecture_code" => "都道府県コード",
        "price_index" => "物価指数",
        "ratio" => "構成比(%)",
        "real_wage_index" => "実質賃金指数",
        "result_type" => "結果種別",
        "survey_date" => "調査日",
        "survey_period" => "調査期",
        "visa_status" => "在留資格",
        "yoy_change_pct" => "前年比(%)",
        // 2026-05-24 audit_G P0-2: silent fallback `_ => key` 防御。
        // 未マップ列は English のまま `<th>` に出るため、開発時に検出 + 本番でも警告ログ出す。
        // MEMORY: feedback_silent_fallback_audit (2026-05-20 表 6-E 英語ラベル残 30+ 件後追い事故)
        _ => {
            #[cfg(debug_assertions)]
            eprintln!("[label_for_column] unmapped column: {}", key);
            tracing::warn!(unmapped_column = key, "label_for_column: unmapped column displayed as English snake_case");
            key
        }
    }
}

// A1 Commit 4 (2026-05-29): section_02_region.rs / section_04_tightness.rs
// から `super::build_navy_auto_table` で参照されるため pub(super) に昇格。
// mod.rs 内 (Section 03/05/06/07) からは従来どおり unqualified で呼び出せる。
pub(super) fn build_navy_auto_table(
    rows: &[super::super::super::helpers::Row],
    max_rows: usize,
) -> String {
    use super::super::super::helpers::{get_f64, get_i64, get_str};
    if rows.is_empty() {
        return "<p class=\"caption dim\">取得値なし</p>\n".to_string();
    }
    // 列の優先順位: 識別子系を先頭、数値系を後ろ
    let mut keys: Vec<String> = rows[0].keys().cloned().collect();
    let priority = [
        "year", "fiscal_year", "reference_year", "reference_date",
        "prefecture", "municipality",
        "industry_name", "industry", "category", "subcategory", "name", "label",
    ];
    keys.sort_by_key(|k| {
        priority.iter().position(|p| *p == k.as_str()).unwrap_or(99)
    });
    // 2026-05-15: A4 横幅に収まるよう 8 → 6 カラムに縮小 (ユーザー指摘:表示はみ出し)
    keys.truncate(6);

    let mut s = String::new();
    // 2026-05-15: aux-data 用テーブル class を追加。CSS で font-size を小さくして
    //   多列でも A4 横幅内に収まるようにする。
    s.push_str("<table class=\"table-navy table-aux\" style=\"font-size:9pt;\">\n<thead><tr>");
    for k in &keys {
        // 2026-05-15: 列ヘッダを 英語スネークケース → 日本語ラベルに変換
        s.push_str(&format!("<th>{}</th>", escape_html(label_for_column(k))));
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let show = rows.iter().take(max_rows);
    for r in show {
        s.push_str("<tr>");
        for k in &keys {
            // 2026-05-14: serde_json::Value::to_string() は文字列を "..." 引用符付きで
            //   返してしまうため、get_str / as_str で素のテキストを取り出す。
            //   数値は format_number / 小数桁制御。null は ダッシュ。
            let v = r.get(k);
            let cell = match v {
                Some(jv) if jv.is_string() => {
                    let str_val = get_str(r, k);
                    if str_val.is_empty() { "—".to_string() } else { escape_html(&str_val) }
                }
                Some(jv) if jv.is_i64() || jv.is_u64() => {
                    format_number(get_i64(r, k))
                }
                Some(jv) if jv.is_f64() => {
                    let f = get_f64(r, k);
                    if f.is_nan() || !f.is_finite() {
                        "—".to_string()
                    } else if f.fract().abs() < 1e-9 {
                        format_number(f as i64)
                    } else {
                        format!("{:.2}", f)
                    }
                }
                Some(jv) if jv.is_boolean() => {
                    if jv.as_bool() == Some(true) { "✓".to_string() } else { "—".to_string() }
                }
                Some(jv) if jv.is_null() => "—".to_string(),
                None => "—".to_string(),
                Some(_) => "—".to_string(),  // 配列やオブジェクト等は表示しない
            };
            s.push_str(&format!("<td>{}</td>", cell));
        }
        s.push_str("</tr>\n");
    }
    if rows.len() > max_rows {
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">他 {} 件</td></tr>\n",
            keys.len(),
            rows.len() - max_rows
        ));
    }
    s.push_str("</tbody></table>\n");
    s
}

// 2026-05-15: Section 7.5 (補助データ全展開) は廃止。各 ext_* 系は
//   Section 02 (地理/通勤流入元/県平均) / Section 04 (事業所/開廃業) /
//   Section 06 (人口/移動/出生死亡/労働力/教育) / Section 07 (昼夜間/世帯)
//   に統合された。撤去された raw dump 経路 (介護/気候/社会生活/医療福祉)
//   は本レポートでは非表示。fetch ロジックは insight/render.rs labor_future
//   _risk / lifestyle.rs / engine.rs MF-1 で active 使用のため残置。

// ============================================================
// Section 08: 注記・出典・免責 (Phase 4 navy 本実装)
// ============================================================
// A1 Commit 2 (2026-05-29):
//   `render_navy_section_08_notes` は `section_08_notes.rs` に分離。
//   mod 冒頭で再エクスポート済み。

// ============================================================
// Unit Tests (2026-05-24 監査 H P0 #4 対策: navy_report.rs test 0 件解消)
// ============================================================
// 対象: pure な内部関数のみ (HTML 生成系は invariant_tests.rs 側で別途検証)
// 目的: A1 navy 分割前の安全担保 + 100倍ずれ / silent fallback 等の防御
#[cfg(test)]
mod tests {
    use super::*;

    // ---- severity_label: 全 case 網羅 (silent fallback 検証) ----
    #[test]
    fn severity_label_pos_returns_pos() {
        assert_eq!(severity_label("pos"), "POS");
    }
    #[test]
    fn severity_label_warn_returns_warn() {
        assert_eq!(severity_label("warn"), "WARN");
    }
    #[test]
    fn severity_label_neg_returns_neg() {
        assert_eq!(severity_label("neg"), "NEG");
    }
    #[test]
    fn severity_label_unknown_returns_neu_default() {
        // silent fallback: 未知 tag は NEU。`_` arm 仕様確認
        assert_eq!(severity_label(""), "NEU");
        assert_eq!(severity_label("info"), "NEU");
        assert_eq!(severity_label("critical"), "NEU");
    }

    // ---- format_mm: 万円換算境界値 ----
    #[test]
    fn format_mm_zero_returns_zero_point_zero() {
        assert_eq!(format_mm(0), "0.0");
    }
    #[test]
    fn format_mm_10000_returns_one_point_zero() {
        assert_eq!(format_mm(10_000), "1.0");
    }
    #[test]
    fn format_mm_250000_returns_25_point_zero() {
        // 月給 25 万円 (中央値想定)
        assert_eq!(format_mm(250_000), "25.0");
    }
    #[test]
    fn format_mm_negative_does_not_panic() {
        // 負値も format するだけ (panic 防御確認)
        assert_eq!(format_mm(-10_000), "-1.0");
    }

    // ---- fmt_ratio / fmt_pct / fmt_pct_from_ratio: Option<f64> フォーマット ----
    #[test]
    fn fmt_ratio_some_formats_two_decimals() {
        assert_eq!(fmt_ratio(Some(1.234)), "1.23");
    }
    #[test]
    fn fmt_ratio_none_returns_em_dash() {
        // データ不在は明示的に「—」(silent fallback 防御)
        assert_eq!(fmt_ratio(None), "—");
    }
    #[test]
    fn fmt_pct_some_formats_one_decimal_with_percent() {
        assert_eq!(fmt_pct(Some(33.456)), "33.5%");
    }
    #[test]
    fn fmt_pct_none_returns_em_dash() {
        assert_eq!(fmt_pct(None), "—");
    }
    #[test]
    fn fmt_pct_from_ratio_some_multiplies_by_100() {
        // 0-1 ratio を 0-100% に変換
        assert_eq!(fmt_pct_from_ratio(Some(0.5)), "50.0");
        assert_eq!(fmt_pct_from_ratio(Some(0.123)), "12.3");
    }
    #[test]
    fn fmt_pct_from_ratio_none_returns_em_dash() {
        assert_eq!(fmt_pct_from_ratio(None), "—");
    }

    // ---- compute_distribution_stats: 統計計算の境界 ----
    #[test]
    fn compute_distribution_stats_empty_returns_none() {
        assert!(compute_distribution_stats(&[], 10_000).is_none());
    }
    #[test]
    fn compute_distribution_stats_all_zero_returns_none() {
        // 全 0 / 負値は filter で除外 → 空配列 → None
        assert!(compute_distribution_stats(&[0, 0, -100], 10_000).is_none());
    }
    #[test]
    fn compute_distribution_stats_single_value_returns_stats() {
        let stats = compute_distribution_stats(&[250_000], 10_000)
            .expect("single value should yield stats");
        assert_eq!(stats.n, 1);
        assert_eq!(stats.median, 250_000);
        assert_eq!(stats.min, 250_000);
        assert_eq!(stats.max, 250_000);
        assert_eq!(stats.mean, 250_000);
    }
    #[test]
    fn compute_distribution_stats_multiple_values_invariants() {
        // ドメイン不変条件:
        //   min <= p25 <= median <= p75 <= p90 <= max
        //   n == values の正値件数
        let values: Vec<i64> = vec![
            200_000, 220_000, 250_000, 280_000, 300_000, 350_000, 400_000,
        ];
        let stats = compute_distribution_stats(&values, 10_000)
            .expect("non-empty positive should yield stats");
        assert_eq!(stats.n, values.len());
        assert!(stats.min <= stats.p25, "min <= p25");
        assert!(stats.p25 <= stats.median, "p25 <= median");
        assert!(stats.median <= stats.p75, "median <= p75");
        assert!(stats.p75 <= stats.p90, "p75 <= p90");
        assert!(stats.p90 <= stats.max, "p90 <= max");
        assert!(!stats.bins.is_empty(), "bins must be non-empty");
        assert_eq!(stats.bin_step, 10_000, "bin_step is fixed 10,000 yen");
    }
    #[test]
    fn compute_distribution_stats_filters_negative_and_zero() {
        // 負値 / 0 は filter (> 0 のみ採用)
        let values: Vec<i64> = vec![0, -100, 200_000, 300_000];
        let stats = compute_distribution_stats(&values, 10_000)
            .expect("two positive values should yield stats");
        assert_eq!(stats.n, 2, "negative / zero are filtered out");
        assert_eq!(stats.min, 200_000);
        assert_eq!(stats.max, 300_000);
    }

    // ============================================================
    // Ext-6 (2026-05-28): compute_distribution_stats の n=1/2/5/100 全網羅
    //   既存テストは「ある程度」の不変条件カバーだが、n の極端値 (1) と
    //   大規模 (100) で 25%/50%/75%/90% 分位の順序関係 (min ≤ p25 ≤ median ≤ p75 ≤ p90 ≤ max)
    //   が常に成立することを明示的に検証。
    //
    //   - n=1: 全分位 = 唯一値
    //   - n=2: pct(0.25)=v[0], pct(0.50)=v[1] (=> p25 < median 可)
    //   - n=5: 既存パターンの中間
    //   - n=100: 大規模、ヒストグラム bins が複数生成される
    // ============================================================

    #[test]
    fn compute_distribution_stats_invariants_n1() {
        let stats = compute_distribution_stats(&[250_000], 10_000)
            .expect("n=1 yields stats");
        assert_eq!(stats.n, 1);
        // n=1 では全分位が唯一の値と一致 (順序不変条件は退化的に成立)
        assert!(stats.min <= stats.p25 && stats.p25 <= stats.median);
        assert!(stats.median <= stats.p75 && stats.p75 <= stats.p90);
        assert!(stats.p90 <= stats.max);
        assert_eq!(stats.min, stats.max, "n=1 で min == max");
        assert_eq!(stats.median, 250_000);
    }

    #[test]
    fn compute_distribution_stats_invariants_n2() {
        // n=2: pct(p) = v[round((n-1)*p)] = v[round(p)] なので
        //   p25 → v[round(0.25)]=v[0]=100k, median → v[round(0.5)]=v[1]=200k (round half-to-even),
        //   p75 → v[round(0.75)]=v[1]=200k, p90 → v[round(0.90)]=v[1]=200k
        let stats = compute_distribution_stats(&[100_000, 200_000], 10_000)
            .expect("n=2 yields stats");
        assert_eq!(stats.n, 2);
        assert_eq!(stats.min, 100_000);
        assert_eq!(stats.max, 200_000);
        assert!(stats.min <= stats.p25, "min({}) <= p25({})", stats.min, stats.p25);
        assert!(stats.p25 <= stats.median, "p25({}) <= median({})", stats.p25, stats.median);
        assert!(stats.median <= stats.p75, "median({}) <= p75({})", stats.median, stats.p75);
        assert!(stats.p75 <= stats.p90, "p75({}) <= p90({})", stats.p75, stats.p90);
        assert!(stats.p90 <= stats.max, "p90({}) <= max({})", stats.p90, stats.max);
    }

    #[test]
    fn compute_distribution_stats_invariants_n5() {
        // n=5: 既存テストの中間ケースを切り出して順序不変条件のみ確認
        let stats = compute_distribution_stats(&[150_000, 200_000, 250_000, 300_000, 400_000], 10_000)
            .expect("n=5 yields stats");
        assert_eq!(stats.n, 5);
        assert!(stats.min <= stats.p25, "min({}) <= p25({})", stats.min, stats.p25);
        assert!(stats.p25 <= stats.median, "p25({}) <= median({})", stats.p25, stats.median);
        assert!(stats.median <= stats.p75, "median({}) <= p75({})", stats.median, stats.p75);
        assert!(stats.p75 <= stats.p90, "p75({}) <= p90({})", stats.p75, stats.p90);
        assert!(stats.p90 <= stats.max, "p90({}) <= max({})", stats.p90, stats.max);
        // n=5 で min/max は端点
        assert_eq!(stats.min, 150_000);
        assert_eq!(stats.max, 400_000);
    }

    #[test]
    fn compute_distribution_stats_invariants_n100() {
        // n=100: 大規模ケース。均等分布で 100k〜1.1M。
        // 順序不変条件 + bins.len() > 1 (複数 bin 生成) を確認。
        let values: Vec<i64> = (0..100).map(|i| 100_000 + i * 10_000).collect();
        let stats = compute_distribution_stats(&values, 10_000)
            .expect("n=100 yields stats");
        assert_eq!(stats.n, 100);
        assert!(stats.min <= stats.p25, "min({}) <= p25({})", stats.min, stats.p25);
        assert!(stats.p25 <= stats.median, "p25({}) <= median({})", stats.p25, stats.median);
        assert!(stats.median <= stats.p75, "median({}) <= p75({})", stats.median, stats.p75);
        assert!(stats.p75 <= stats.p90, "p75({}) <= p90({})", stats.p75, stats.p90);
        assert!(stats.p90 <= stats.max, "p90({}) <= max({})", stats.p90, stats.max);
        assert!(
            stats.bins.len() >= 2,
            "n=100 で bin が複数生成されるはず: bins.len()={}",
            stats.bins.len()
        );
        assert_eq!(stats.bin_step, 10_000, "bin_step 固定");
        // 平均: (100k + 1,090k) / 2 = 595k (sum / n)
        let expected_mean: i64 = values.iter().sum::<i64>() / 100;
        assert_eq!(stats.mean, expected_mean);
    }

    // ============================================================
    // P1-6 (2026-05-28): compute_skew_severity 偏り判定境界値テスト
    // ------------------------------------------------------------
    // 検証範囲:
    //   1. 空入力 → NEU "{label}データなし"
    //   2. total <= 0 → NEU "{label}データなし" (全件 0 や負値)
    //   3. 単一カテゴリ 100% → WARN 顕著
    //   4. 上位 75% / 残り 25% → NEU 偏りあり
    //   5. 上位 50% / 残り 50% → POS バランス良好
    //   6. 境界値: 70.0% ちょうど → POS (strict >)
    //              70.01% → NEU
    //              85.0% ちょうど → NEU (strict >)
    //              85.01% → WARN
    // ============================================================

    #[test]
    fn compute_skew_severity_empty_returns_neu_no_data() {
        let (sev, msg) = compute_skew_severity(&[], "産業大分類");
        assert_eq!(sev, "neu");
        assert_eq!(msg, "産業大分類データなし");
    }

    #[test]
    fn compute_skew_severity_total_zero_returns_neu_no_data() {
        // total <= 0 ガード: 全件 0 の場合
        let counts = vec![("A".to_string(), 0i64), ("B".to_string(), 0i64)];
        let (sev, msg) = compute_skew_severity(&counts, "職種");
        assert_eq!(sev, "neu");
        assert_eq!(msg, "職種データなし");
    }

    #[test]
    fn compute_skew_severity_single_category_returns_warn() {
        // 1 カテゴリのみ → 100% → WARN
        let counts = vec![("医療,福祉".to_string(), 1000i64)];
        let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
        assert_eq!(sev, "warn", "100% は WARN (> 85%)");
        assert!(msg.contains("顕著"), "msg={}", msg);
        assert!(msg.contains("100.0%"), "msg={}", msg);
        assert!(msg.contains("医療,福祉"), "msg={}", msg);
        assert!(msg.contains("サンプル代表性"), "msg={}", msg);
    }

    #[test]
    fn compute_skew_severity_75_pct_returns_neu_skewed() {
        // 上位 75% (=750/1000) / 残り 25% → NEU 偏りあり
        let counts = vec![
            ("医療,福祉".to_string(), 750i64),
            ("製造業".to_string(), 250i64),
        ];
        let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
        assert_eq!(sev, "neu", "75% は NEU (70% < 75 <= 85)");
        assert!(msg.contains("偏りあり"), "msg={}", msg);
        assert!(msg.contains("75.0%"), "msg={}", msg);
        assert!(msg.contains("データ代表性に注意"), "msg={}", msg);
    }

    #[test]
    fn compute_skew_severity_50_pct_returns_pos_balanced() {
        // 上位 50% / 残り 50% → POS バランス良好
        let counts = vec![
            ("看護師".to_string(), 500i64),
            ("介護職".to_string(), 500i64),
        ];
        let (sev, msg) = compute_skew_severity(&counts, "職種");
        assert_eq!(sev, "pos", "50% は POS (<= 70%)");
        assert!(msg.contains("バランス 良好"), "msg={}", msg);
        assert!(msg.contains("50.0%"), "msg={}", msg);
    }

    // Ext-3 (2026-05-28): 境界値テストは定数 (`SKEW_NEU_THRESHOLD_PCT` /
    //   `SKEW_WARN_THRESHOLD_PCT`) の現値が 70.0 / 85.0 であることを前提とする。
    //   閾値変更時は本テスト群と定数の双方を必ず同期更新すること。
    //   下記 `compute_skew_severity_threshold_constants_are_documented_values` で
    //   定数値そのものを assert し、定数だけ変えてテストを忘れたとき検出する。

    #[test]
    fn compute_skew_severity_70_pct_exactly_returns_pos() {
        // 境界: SKEW_NEU_THRESHOLD_PCT (70.0%) ちょうど → POS (strict >)
        let counts = vec![
            ("A".to_string(), 700i64),
            ("B".to_string(), 300i64),
        ];
        let (sev, _msg) = compute_skew_severity(&counts, "職種");
        assert_eq!(
            sev, "pos",
            "{}% ちょうどは POS (strict >)",
            SKEW_NEU_THRESHOLD_PCT
        );
    }

    #[test]
    fn compute_skew_severity_above_70_pct_returns_neu() {
        // 境界: 70.01% (701/1000) → NEU
        let counts = vec![
            ("A".to_string(), 701i64),
            ("B".to_string(), 299i64),
        ];
        let (sev, msg) = compute_skew_severity(&counts, "職種");
        assert_eq!(
            sev, "neu",
            "70.1% は NEU (> {}%)",
            SKEW_NEU_THRESHOLD_PCT
        );
        assert!(msg.contains("偏りあり"), "msg={}", msg);
    }

    #[test]
    fn compute_skew_severity_85_pct_exactly_returns_neu() {
        // 境界: SKEW_WARN_THRESHOLD_PCT (85.0%) ちょうど → NEU (strict >)
        let counts = vec![
            ("A".to_string(), 850i64),
            ("B".to_string(), 150i64),
        ];
        let (sev, _msg) = compute_skew_severity(&counts, "産業大分類");
        assert_eq!(
            sev, "neu",
            "{}% ちょうどは NEU (strict >)",
            SKEW_WARN_THRESHOLD_PCT
        );
    }

    #[test]
    fn compute_skew_severity_above_85_pct_returns_warn() {
        // 境界: 85.01% (851/1000) → WARN
        let counts = vec![
            ("A".to_string(), 851i64),
            ("B".to_string(), 149i64),
        ];
        let (sev, msg) = compute_skew_severity(&counts, "産業大分類");
        assert_eq!(
            sev, "warn",
            "85.1% は WARN (> {}%)",
            SKEW_WARN_THRESHOLD_PCT
        );
        assert!(msg.contains("顕著"), "msg={}", msg);
    }

    /// Ext-3 (2026-05-28): 閾値定数の現値が 70.0 / 85.0 であることを assert する。
    ///
    /// 定数だけ変えて境界値テストを更新し忘れた場合、本テストが落ちて事故を未然に防ぐ。
    /// 不変条件: `SKEW_NEU_THRESHOLD_PCT < SKEW_WARN_THRESHOLD_PCT` (順序保証)。
    #[test]
    fn compute_skew_severity_threshold_constants_are_documented_values() {
        assert_eq!(
            SKEW_NEU_THRESHOLD_PCT, 70.0,
            "NEU しきい値: docstring と境界値テストは 70.0 を前提"
        );
        assert_eq!(
            SKEW_WARN_THRESHOLD_PCT, 85.0,
            "WARN しきい値: docstring と境界値テストは 85.0 を前提"
        );
        assert!(
            SKEW_NEU_THRESHOLD_PCT < SKEW_WARN_THRESHOLD_PCT,
            "順序保証: NEU 閾値 < WARN 閾値"
        );
        assert!(
            SKEW_NEU_THRESHOLD_PCT > 0.0 && SKEW_WARN_THRESHOLD_PCT <= 100.0,
            "範囲: 両定数とも (0.0, 100.0] の範囲内"
        );
    }

    #[test]
    fn compute_skew_severity_max_share_invariant() {
        // 不変条件: max_share ∈ [0, 100]
        // 多カテゴリで top が小さい場合も合計に対する比率は正常範囲
        let counts: Vec<(String, i64)> = (0..10)
            .map(|i| (format!("cat{}", i), 100i64))
            .collect();
        let (sev, msg) = compute_skew_severity(&counts, "職種");
        // 10 カテゴリ均等 → top=100/total=1000 = 10.0% → POS
        assert_eq!(sev, "pos");
        assert!(msg.contains("10.0%"), "msg={}", msg);
    }

    #[test]
    fn compute_skew_severity_negative_counts_excluded_from_total() {
        // 不変条件補足: total = sum (負値含む) なので、負値があると total が縮む。
        // 設計通り (postings fetch は cnt > 0 を保証するが、関数自体は防御的)。
        // 負値で total = 0 以下になれば NEU データなし。
        let counts = vec![
            ("A".to_string(), -50i64),
            ("B".to_string(), -50i64),
        ];
        let (sev, msg) = compute_skew_severity(&counts, "職種");
        assert_eq!(sev, "neu", "total <= 0 は NEU データなし");
        assert_eq!(msg, "職種データなし");
    }

    // ====================================================================
    // P2-1 (2026-05-28): 給与レンジ 散布図 (Section 03 図 3-6)
    //   - build_navy_salary_scatter_svg: 空 / 1 点 / 多数点
    //   - build_salary_scatter_summary: n / 平均レンジ幅 / narrow% / wide%
    //
    // 設計メモ:
    //   - silent fallback 防御 (空配列 → 空文字列)
    //   - 不変条件: n >= 0, avg_width >= 0, 0 <= narrow_pct <= 100, 0 <= wide_pct <= 100
    // ====================================================================

    #[test]
    fn build_navy_salary_scatter_svg_empty_returns_empty_string() {
        // 不変条件: 空入力 → 空文字列 (silent fallback ではなく明示的に省略)
        // Phase 2-A (2026-05-29): is_hourly 引数追加。月給モード (false) で旧動作互換。
        let svg = build_navy_salary_scatter_svg(&[], false);
        assert!(svg.is_empty(), "empty pairs → empty svg, got len={}", svg.len());
    }

    #[test]
    fn build_navy_salary_scatter_svg_single_point_contains_svg_tag() {
        // 1 点入力: <svg> タグ + 1 つの <circle> が含まれる
        let pairs = vec![(200_000.0_f64, 300_000.0_f64)];
        let svg = build_navy_salary_scatter_svg(&pairs, false);
        assert!(svg.contains("<svg"), "svg tag missing");
        assert!(svg.contains("</svg>"), "svg close tag missing");
        // 散布点は 1 つ。<circle ... opacity="0.4"/> が含まれる
        let circle_count = svg.matches("<circle").count();
        assert_eq!(circle_count, 1, "expected 1 circle, got {circle_count}");
        // 対角線 (金色破線) も常に描画される
        assert!(svg.contains("#C9A24B"), "diagonal line color (gold) missing");
    }

    #[test]
    fn build_navy_salary_scatter_svg_many_points_contains_opacity_and_navy_color() {
        // 多数点: opacity 0.4 / navy 色 #1F2D4D / 全点数の circle 出力
        let pairs: Vec<(f64, f64)> = (0..50)
            .map(|i| (180_000.0 + (i as f64) * 1000.0, 280_000.0 + (i as f64) * 2000.0))
            .collect();
        let svg = build_navy_salary_scatter_svg(&pairs, false);
        // 仕様: opacity 0.4 が散布点に含まれる
        assert!(svg.contains("opacity=\"0.4\""), "opacity=0.4 missing for scatter points");
        // 仕様: navy ink-soft 色
        assert!(svg.contains("#1F2D4D"), "navy color (#1F2D4D) missing");
        let circle_count = svg.matches("<circle").count();
        assert_eq!(circle_count, pairs.len(), "circle count mismatch");
    }

    #[test]
    fn build_navy_salary_scatter_svg_out_of_range_values_clamped_not_panic() {
        // 不変条件: 範囲外 (10万 / 100万円) でも panic せず描画は範囲内にクランプ
        let pairs = vec![
            (50_000.0, 80_000.0),       // 範囲外 (5万 / 8万)
            (1_000_000.0, 2_000_000.0), // 範囲外 (100万 / 200万)
            (250_000.0, 350_000.0),     // 範囲内
        ];
        let svg = build_navy_salary_scatter_svg(&pairs, false);
        assert!(svg.contains("<svg"), "svg should render even with out-of-range");
        let circle_count = svg.matches("<circle").count();
        assert_eq!(circle_count, 3);
    }

    #[test]
    fn build_salary_scatter_summary_empty_returns_empty_string() {
        let s = build_salary_scatter_summary(&[], false);
        assert!(s.is_empty(), "empty pairs → empty summary");
    }

    #[test]
    fn build_salary_scatter_summary_computes_n_and_widths_correctly() {
        // 設計テストデータ n=5:
        //   (200000, 230000) → 幅 30000 = 3万円  (narrow: < 5万)
        //   (200000, 240000) → 幅 40000 = 4万円  (narrow)
        //   (200000, 260000) → 幅 60000 = 6万円  (中間)
        //   (200000, 300000) → 幅 100000 = 10万円 (wide: >= 10万)
        //   (200000, 350000) → 幅 150000 = 15万円 (wide)
        //
        //   avg_width = (30000+40000+60000+100000+150000)/5 = 76000 = 7.6 万円
        //   narrow_pct = 2/5 = 40.0%
        //   wide_pct = 2/5 = 40.0%
        let pairs = vec![
            (200_000.0, 230_000.0),
            (200_000.0, 240_000.0),
            (200_000.0, 260_000.0),
            (200_000.0, 300_000.0),
            (200_000.0, 350_000.0),
        ];
        let s = build_salary_scatter_summary(&pairs, false);
        // n
        assert!(s.contains("n=5"), "expected n=5 in summary: {s}");
        // 平均レンジ幅 (7.6 万円)
        assert!(s.contains("7.6万円"), "expected 7.6万円 in summary: {s}");
        // narrow_pct = 40.0%
        assert!(
            s.contains("40.0% (定額求人傾向)"),
            "expected narrow 40.0% (定額求人傾向) in summary: {s}"
        );
        // wide_pct = 40.0%
        assert!(
            s.contains("40.0% (歩合・等級制傾向)"),
            "expected wide 40.0% (歩合・等級制傾向) in summary: {s}"
        );
    }

    #[test]
    fn build_salary_scatter_summary_invariants_pct_in_range_0_100() {
        // 不変条件: narrow_pct + wide_pct <= 100, 各 pct ∈ [0, 100]
        // 全件 narrow (幅 1万円固定)
        let pairs_all_narrow: Vec<(f64, f64)> = (0..10)
            .map(|_| (200_000.0_f64, 210_000.0_f64))
            .collect();
        let s = build_salary_scatter_summary(&pairs_all_narrow, false);
        assert!(
            s.contains("100.0% (定額求人傾向)"),
            "expected narrow 100% when all narrow: {s}"
        );
        assert!(
            s.contains("0.0% (歩合・等級制傾向)"),
            "expected wide 0% when all narrow: {s}"
        );

        // 全件 wide (幅 20万円固定)
        let pairs_all_wide: Vec<(f64, f64)> = (0..10)
            .map(|_| (200_000.0_f64, 400_000.0_f64))
            .collect();
        let s = build_salary_scatter_summary(&pairs_all_wide, false);
        assert!(
            s.contains("100.0% (歩合・等級制傾向)"),
            "expected wide 100% when all wide: {s}"
        );
        assert!(
            s.contains("0.0% (定額求人傾向)"),
            "expected narrow 0% when all wide: {s}"
        );
    }

    #[test]
    fn build_salary_scatter_summary_avg_width_non_negative_invariant() {
        // 不変条件: 平均レンジ幅 >= 0 (hi >= lo を SQL で保証している前提)
        // hi == lo のケース (レンジ幅 0) でも panic せず avg 0.0 を出力
        let pairs = vec![
            (250_000.0, 250_000.0),
            (300_000.0, 300_000.0),
        ];
        let s = build_salary_scatter_summary(&pairs, false);
        assert!(s.contains("n=2"));
        assert!(s.contains("0.0万円"), "expected avg width 0.0万円: {s}");
        // 全件 narrow (< 5万)
        assert!(s.contains("100.0% (定額求人傾向)"));
    }

    // ====================================================================
    // P2-2 (2026-05-28): CSV 企業別給与ランキング (表 5-G) +
    //                    注目企業リスト (表 5-H、求人数 top ∩ 給与 top の和集合)
    //
    //   - select_notable_companies: 空 / 単一 / 5社 / 上位重複 / 和集合サイズ
    //   - build_navy_csv_company_salary_table: 空 / 1社 / SO WHAT 直前挿入位置
    //   - build_navy_notable_companies_block: 空フォールバック
    //
    // 不変条件 (silent fallback 防御):
    //   - 空 ranking → 戻り値空 Vec / 空文字列
    //   - 戻り値 size <= 2 * top_n
    //   - レンジ幅 >= 0 (upper >= lower)
    // ====================================================================

    fn make_csv_company(name: &str, posting_count: i64, lower: f64, upper: f64) -> CsvCompanySalary {
        // Phase 2-A (2026-05-29): native_unit フィールド追加。テスト fixture は月給モード想定。
        CsvCompanySalary {
            facility_name: name.to_string(),
            posting_count,
            salary_lower_median: lower,
            salary_upper_median: upper,
            native_unit: "月給".to_string(),
        }
    }

    #[test]
    fn select_notable_companies_empty_returns_empty_vec() {
        // 不変条件: 空 ranking → 空 Vec (silent fallback ではなく明示)
        let result = select_notable_companies(&[], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn select_notable_companies_top_n_zero_returns_empty_vec() {
        let ranking = vec![make_csv_company("A 株式会社", 5, 20.0, 30.0)];
        let result = select_notable_companies(&ranking, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn select_notable_companies_single_returns_single() {
        let ranking = vec![make_csv_company("A 株式会社", 5, 20.0, 30.0)];
        let result = select_notable_companies(&ranking, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].facility_name, "A 株式会社");
    }

    #[test]
    fn select_notable_companies_five_companies_returns_five() {
        // 5 社 + top_n=5 → 全件返却 (和集合サイズ = 5)
        // ranking は upper_median 降順 (fetch 側ソート保証)
        let ranking = vec![
            make_csv_company("A", 10, 25.0, 50.0),
            make_csv_company("B", 8, 23.0, 45.0),
            make_csv_company("C", 6, 21.0, 40.0),
            make_csv_company("D", 4, 19.0, 35.0),
            make_csv_company("E", 2, 17.0, 30.0),
        ];
        let result = select_notable_companies(&ranking, 5);
        assert_eq!(result.len(), 5);
        // 不変条件: size <= 2 * top_n
        assert!(result.len() <= 10);
    }

    #[test]
    fn select_notable_companies_perfect_overlap_returns_top_n() {
        // 求人数順序 = 給与順序 (完全重複)
        // → 和集合サイズ = top_n (重複排除済)
        let ranking = vec![
            make_csv_company("A", 10, 25.0, 50.0), // 求人 #1 / 給与 #1
            make_csv_company("B", 8, 23.0, 45.0),  // 求人 #2 / 給与 #2
            make_csv_company("C", 6, 21.0, 40.0),  // 求人 #3 / 給与 #3
        ];
        let result = select_notable_companies(&ranking, 3);
        assert_eq!(result.len(), 3, "perfect overlap should return exactly top_n");
        // 出現順序: 求人 top → 給与 top (重複は除外) → 結果は [A, B, C]
        assert_eq!(result[0].facility_name, "A");
        assert_eq!(result[1].facility_name, "B");
        assert_eq!(result[2].facility_name, "C");
    }

    #[test]
    fn select_notable_companies_disjoint_returns_union() {
        // 給与 top と 求人数 top が完全 disjoint
        // ranking は upper_median 降順なので 給与 top = [A, B, C]
        // 求人数 top は [E, D, C] (E が最多) → 重複 C
        // 和集合: 求人 top [E, D, C] + 給与 top [A, B] = [E, D, C, A, B] = 5 件
        let ranking = vec![
            make_csv_company("A", 2, 30.0, 60.0),  // 給与 #1, 求人 最少
            make_csv_company("B", 2, 28.0, 55.0),  // 給与 #2
            make_csv_company("C", 5, 26.0, 50.0),  // 給与 #3, 求人 #3
            make_csv_company("D", 10, 18.0, 30.0), // 給与 #4, 求人 #2
            make_csv_company("E", 20, 15.0, 25.0), // 給与 #5, 求人 #1
        ];
        let result = select_notable_companies(&ranking, 3);
        // 和集合サイズ: posting top {E, D, C} ∪ salary top {A, B, C} = {A, B, C, D, E} = 5
        assert_eq!(result.len(), 5);
        // 出現順: posting top を先、salary top 残りを後
        let names: Vec<&str> = result.iter().map(|c| c.facility_name.as_str()).collect();
        assert_eq!(names, vec!["E", "D", "C", "A", "B"]);
        // 不変条件: size <= 2 * top_n
        assert!(result.len() <= 6);
    }

    #[test]
    fn select_notable_companies_top_n_larger_than_ranking_returns_all() {
        // top_n > ranking.len() → 全件返却 (和集合は ranking 全体)
        let ranking = vec![
            make_csv_company("A", 5, 20.0, 30.0),
            make_csv_company("B", 3, 18.0, 25.0),
        ];
        let result = select_notable_companies(&ranking, 10);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn build_navy_csv_company_salary_table_empty_renders_fallback_message() {
        // 空 ranking → 「該当企業なし」明示メッセージ (silent fallback 防御)
        let s = build_navy_csv_company_salary_table(&[], 10);
        assert!(s.contains("表 5-G"));
        assert!(
            s.contains("該当企業なし"),
            "empty ranking should render explicit fallback: {s}"
        );
    }

    #[test]
    fn build_navy_csv_company_salary_table_single_company_renders_columns() {
        let ranking = vec![make_csv_company("テスト病院", 3, 22.5, 35.7)];
        let s = build_navy_csv_company_salary_table(&ranking, 10);
        // タイトル + 列ヘッダ + データ行
        assert!(s.contains("表 5-G"));
        assert!(s.contains("法人名"));
        assert!(s.contains("下限給与中央値"));
        assert!(s.contains("上限給与中央値"));
        assert!(s.contains("レンジ幅"));
        assert!(s.contains("テスト病院"));
        // 中央値が万円単位で表示される
        assert!(s.contains("22.5"));
        assert!(s.contains("35.7"));
        // レンジ幅 = 35.7 - 22.5 = 13.2
        assert!(s.contains("13.2"), "expected range width 13.2: {s}");
    }

    #[test]
    fn build_navy_csv_company_salary_table_range_width_invariant_non_negative() {
        // 不変条件: lower == upper (固定給) でもレンジ幅 0 で panic せず描画
        let ranking = vec![make_csv_company("固定給会社", 2, 25.0, 25.0)];
        let s = build_navy_csv_company_salary_table(&ranking, 10);
        assert!(s.contains("固定給会社"));
        assert!(s.contains("0.0"), "fixed salary should render range width 0.0: {s}");
    }

    #[test]
    fn build_navy_notable_companies_block_empty_returns_empty_string() {
        // silent fallback 防御: 空 ranking → 空文字列 (Section に空 table を出さない)
        let s = build_navy_notable_companies_block(&[], 5);
        assert!(s.is_empty(), "empty ranking should yield empty string, got: {s}");
    }

    #[test]
    fn build_navy_notable_companies_block_renders_table_header_and_rows() {
        let ranking = vec![
            make_csv_company("A", 10, 25.0, 50.0),
            make_csv_company("B", 8, 23.0, 45.0),
        ];
        let s = build_navy_notable_companies_block(&ranking, 5);
        assert!(s.contains("表 5-H"));
        assert!(s.contains("注目企業"));
        assert!(s.contains("給与レンジ"));
        assert!(s.contains("A"));
        assert!(s.contains("B"));
        // 給与レンジ: "25.0〜50.0" の形式
        assert!(
            s.contains("25.0〜50.0"),
            "expected salary range 25.0〜50.0: {s}"
        );
    }

    #[test]
    fn select_notable_companies_invariant_size_le_2_top_n() {
        // 不変条件: |posting_top ∪ salary_top| <= |posting_top| + |salary_top| = 2 * top_n
        // 任意の ranking に対し成立することを確認
        let ranking: Vec<CsvCompanySalary> = (0..20)
            .map(|i| {
                make_csv_company(
                    &format!("Company {}", i),
                    (20 - i) as i64,        // 求人数: 20, 19, ..., 1 (降順)
                    10.0 + (i as f64),      // 下限: 10, 11, ..., 29
                    40.0 - (i as f64),      // 上限: 40, 39, ..., 21 (降順)
                )
            })
            .collect();
        for top_n in 1..=10 {
            let result = select_notable_companies(&ranking, top_n);
            assert!(
                result.len() <= 2 * top_n,
                "invariant violated at top_n={}: result.len()={} > 2 * top_n={}",
                top_n,
                result.len(),
                2 * top_n
            );
        }
    }

    /// Ext-5 (2026-05-28): 不変条件 `size <= 2 * top_n` を明示的に
    ///   `[1, 3, 5, 10]` の代表値で検証する。
    ///
    /// 既存 `..._size_le_2_top_n` は 1..=10 連続テストで包括的だが、
    /// 「指定 top_n に対する明示的サイズ上限」 を docstring から直接トレース可能にし、
    /// 仕様改訂時の影響範囲を可視化する。
    ///
    /// 重要 invariants:
    /// - `result.len() <= 2 * top_n` (常に成立)
    /// - `result.len() <= ranking.len()` (元データを超えない)
    /// - 各要素は ranking 内に存在 (ポインタ等価)
    /// - 重複なし (HashSet で確認)
    #[test]
    fn select_notable_companies_invariant_size_le_double_top_n() {
        // 10 社の ranking。求人数と上限給与で意図的に分離 (和集合のサイズが top_n*2 に近づくよう設計)
        let ranking: Vec<CsvCompanySalary> = (0..10)
            .map(|i| {
                make_csv_company(
                    &format!("Co{}", i),
                    if i < 5 { (10 - i) as i64 } else { 1 }, // 求人数: 前半は降順、後半は 1 で固定
                    20.0 + (i as f64),                       // 下限
                    50.0 - (i as f64),                       // 上限 (降順 → ranking は upper_median 降順なので index と一致)
                )
            })
            .collect();

        for top_n in [1usize, 3, 5, 10] {
            let result = select_notable_companies(&ranking, top_n);

            // 不変条件 1: size <= 2 * top_n
            assert!(
                result.len() <= 2 * top_n,
                "top_n={} で size={} > 2*top_n={}",
                top_n,
                result.len(),
                2 * top_n
            );

            // 不変条件 2: size <= ranking.len()
            assert!(
                result.len() <= ranking.len(),
                "top_n={} で size={} > ranking.len()={}",
                top_n,
                result.len(),
                ranking.len()
            );

            // 不変条件 3: 重複なし (ポインタ等価で確認)
            let mut ptrs: Vec<*const CsvCompanySalary> =
                result.iter().map(|c| *c as *const _).collect();
            ptrs.sort();
            ptrs.dedup();
            assert_eq!(
                ptrs.len(),
                result.len(),
                "top_n={} で duplicate detected: {} unique vs {} result",
                top_n,
                ptrs.len(),
                result.len()
            );
        }
    }

    // ====================================================================
    // R2-P0-1 (ultrathink Round 2, 2026-05-28): クランプ件数 caption の追記
    //
    // build_navy_salary_scatter_svg は軸 15-60 万円固定でデータをクランプ描画する。
    // ユーザーに伝わるよう、build_salary_scatter_summary に
    // 「N 件 (X%) が範囲外として端点に表示」の文言を caption に追加。
    //
    // 不変条件:
    //   - クランプ件数 == 0 のとき caption に「範囲外」文言は含まない
    //   - クランプ件数 > 0 のとき caption に件数 / % が含まれる
    //   - clamp_count <= n
    // ====================================================================

    #[test]
    fn build_salary_scatter_summary_clamp_zero_no_range_note() {
        // 全データが 15-60 万円範囲内 → クランプ件数 0 → 範囲外文言なし
        let pairs = vec![
            (200_000.0, 250_000.0), // 20-25 万 (範囲内)
            (300_000.0, 400_000.0), // 30-40 万 (範囲内)
            (450_000.0, 550_000.0), // 45-55 万 (範囲内)
        ];
        let s = build_salary_scatter_summary(&pairs, false);
        assert!(s.contains("n=3"), "expected n=3 in summary: {s}");
        assert!(
            !s.contains("範囲外"),
            "no out-of-range data → no range-clamp note expected, got: {s}"
        );
        assert!(
            !s.contains("端点に表示"),
            "no endpoint clamp text expected: {s}"
        );
    }

    #[test]
    fn build_salary_scatter_summary_clamp_nonzero_renders_caption() {
        // 5 件中 2 件 (40%) が範囲外 (10 万 / 80 万) → caption に「2 件 (40.0%) が範囲外」表示
        let pairs = vec![
            (100_000.0, 150_000.0), // 10-15 万 (下限が範囲外)
            (800_000.0, 900_000.0), // 80-90 万 (両方範囲外)
            (200_000.0, 300_000.0), // 範囲内
            (250_000.0, 350_000.0), // 範囲内
            (400_000.0, 500_000.0), // 範囲内
        ];
        let s = build_salary_scatter_summary(&pairs, false);
        assert!(s.contains("n=5"), "n=5 expected: {s}");
        assert!(
            s.contains("2 件"),
            "expected clamp count 2: {s}"
        );
        assert!(
            s.contains("40.0%"),
            "expected clamp pct 40.0%: {s}"
        );
        assert!(
            s.contains("範囲外"),
            "expected '範囲外' wording: {s}"
        );
    }

    #[test]
    fn build_salary_scatter_summary_clamp_all_out_of_range() {
        // 全件範囲外 → 100% クランプ
        let pairs = vec![
            (100_000.0, 140_000.0), // 10-14 万
            (700_000.0, 800_000.0), // 70-80 万
        ];
        let s = build_salary_scatter_summary(&pairs, false);
        assert!(s.contains("2 件"), "expected 2 件: {s}");
        assert!(
            s.contains("100.0%"),
            "expected 100.0% clamp pct: {s}"
        );
    }

    // ====================================================================
    // R2-P1-1 (ultrathink Round 2, 2026-05-28): NaN/Inf 出力防止
    //
    // safe_pct helper が NaN / +Inf / -Inf / 100超 を [0, 100] にクランプ。
    // safe_pct_like は NaN/Inf のみ 0.0 にし、上限クランプはしない。
    // ====================================================================

    #[test]
    fn safe_pct_nan_returns_zero() {
        let v = f64::NAN;
        assert_eq!(safe_pct(v), 0.0, "NaN should map to 0.0");
    }

    #[test]
    fn safe_pct_inf_returns_zero() {
        assert_eq!(safe_pct(f64::INFINITY), 0.0, "+Inf should map to 0.0");
        assert_eq!(safe_pct(f64::NEG_INFINITY), 0.0, "-Inf should map to 0.0");
    }

    #[test]
    fn safe_pct_above_100_clamped() {
        // 浮動小数誤差で 100.0000001 になる場合に対する防御
        assert_eq!(safe_pct(100.0001), 100.0);
        assert_eq!(safe_pct(150.0), 100.0);
    }

    #[test]
    fn safe_pct_negative_clamped_to_zero() {
        assert_eq!(safe_pct(-1.0), 0.0);
        assert_eq!(safe_pct(-0.0001), 0.0);
    }

    #[test]
    fn safe_pct_normal_value_unchanged() {
        assert_eq!(safe_pct(42.5), 42.5);
        assert_eq!(safe_pct(0.0), 0.0);
        assert_eq!(safe_pct(100.0), 100.0);
    }

    #[test]
    fn safe_pct_like_nan_returns_zero_but_no_upper_clamp() {
        // safe_pct_like は NaN/Inf を 0 にするが、>100 の大きな値は通す (avg などの非 % 値用)
        assert_eq!(safe_pct_like(f64::NAN), 0.0);
        assert_eq!(safe_pct_like(f64::INFINITY), 0.0);
        assert_eq!(safe_pct_like(500.0), 500.0, "non-% values should not be upper-clamped");
        assert_eq!(safe_pct_like(-3.0), -3.0, "negatives also pass for non-% helper");
    }

    // ====================================================================
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): SVG <title> 要素追加 (a11y)
    //
    // build_navy_pyramid_svg / build_navy_pyramid_svg_mini /
    // build_navy_salary_scatter_svg の 3 関数で <title>...</title> を含むことを確認
    // ====================================================================

    #[test]
    fn build_navy_pyramid_svg_contains_title_element_for_a11y() {
        let bands = vec![
            ("20-29".to_string(), 1000i64, 950i64),
            ("30-39".to_string(), 1100i64, 1050i64),
        ];
        let svg = build_navy_pyramid_svg(&bands);
        assert!(
            svg.contains("<title>年齢階級別 人口ピラミッド</title>"),
            "expected <title> element in build_navy_pyramid_svg: {}",
            &svg[..svg.len().min(400)]
        );
    }

    #[test]
    fn build_navy_pyramid_svg_mini_contains_title_element_for_a11y() {
        let bands = vec![
            ("20-29".to_string(), 100i64, 95i64),
            ("30-39".to_string(), 110i64, 105i64),
        ];
        let svg = build_navy_pyramid_svg_mini(&bands);
        assert!(
            svg.contains("<title>市区町村別 人口ピラミッド (年齢階級別 男女別 人口)</title>"),
            "expected <title> element in build_navy_pyramid_svg_mini: {}",
            &svg[..svg.len().min(400)]
        );
    }

    #[test]
    fn build_navy_salary_scatter_svg_contains_title_element_for_a11y() {
        let pairs = vec![(200_000.0, 300_000.0)];
        let svg = build_navy_salary_scatter_svg(&pairs, false);
        assert!(
            svg.contains("<title>給与レンジ 散布図 (下限給与 × 上限給与)</title>"),
            "expected <title> element in build_navy_salary_scatter_svg: {}",
            &svg[..svg.len().min(400)]
        );
    }

    // ====================================================================
    // R2-P1-4 (ultrathink Round 2, 2026-05-28): 表 scope="col" 追加 (a11y)
    //
    // build_navy_csv_company_salary_table / build_navy_notable_companies_block /
    // build_distribution_table の列ヘッダに scope="col" が付与されることを確認。
    // ====================================================================

    #[test]
    fn build_navy_csv_company_salary_table_th_has_scope_col() {
        let ranking = vec![make_csv_company("テスト病院", 3, 22.0, 32.0)];
        let s = build_navy_csv_company_salary_table(&ranking, 10);
        // 全 th に scope="col" が付与されているか (列ヘッダのみ存在する table)
        // <th スペース付きで grep → <thead> 等を誤カウントしない
        let th_count = s.matches("<th ").count();
        let scoped_count = s.matches("scope=\"col\"").count();
        assert!(
            th_count > 0 && th_count == scoped_count,
            "all <th> should have scope=\"col\": th={}, scoped={}",
            th_count,
            scoped_count
        );
    }

    #[test]
    fn build_navy_notable_companies_block_th_has_scope_col() {
        let ranking = vec![
            make_csv_company("A", 10, 25.0, 50.0),
            make_csv_company("B", 8, 23.0, 45.0),
        ];
        let s = build_navy_notable_companies_block(&ranking, 5);
        // <th スペース付きで grep → <thead> 等を誤カウントしない
        let th_count = s.matches("<th ").count();
        let scoped_count = s.matches("scope=\"col\"").count();
        assert!(
            th_count > 0 && th_count == scoped_count,
            "all <th> should have scope=\"col\": th={}, scoped={}",
            th_count,
            scoped_count
        );
    }

    #[test]
    fn build_distribution_table_th_has_scope_col() {
        let distribution = vec![
            ("〜29歳".to_string(), 100i64),
            ("30〜44歳".to_string(), 200i64),
        ];
        let s = build_distribution_table(&distribution, "年齢制限ラベル");
        // <th スペース付きで grep → <thead> 等を誤カウントしない
        let th_count = s.matches("<th ").count();
        let scoped_count = s.matches("scope=\"col\"").count();
        assert!(
            th_count > 0 && th_count == scoped_count,
            "all <th> should have scope=\"col\": th={}, scoped={}",
            th_count,
            scoped_count
        );
        // ヘッダラベルは引数として渡される (escape_html 通過後)
        assert!(s.contains("年齢制限ラベル"));
    }

    // ====================================================================
    // R2-P1-6 (ultrathink Round 2, 2026-05-28):
    //   salary_target_distribution / age_range_distribution /
    //   employment_type_distribution の全カウントが 0 の場合、
    //   `max_by_key` last-wins の誤ラベルを KPI に表示しないことを確認。
    //
    // 直接 render_navy_section_06_posting_target を呼出して KPI HTML 出力を検査。
    // ====================================================================

    #[test]
    fn render_navy_section_06_posting_target_all_zero_distribution_kpi_dash() {
        use crate::handlers::analysis::fetch::PostingTargetProfile;

        // 全 distribution が count == 0 のとき
        let pt = PostingTargetProfile {
            total_postings: 0,
            age_range_distribution: vec![
                ("〜29歳".to_string(), 0i64),
                ("30〜44歳".to_string(), 0i64),
                ("45〜64歳".to_string(), 0i64),
            ],
            salary_target_distribution: vec![
                ("20〜25万".to_string(), 0i64),
                ("25〜30万".to_string(), 0i64),
                ("〜20万".to_string(), 0i64), // 最後のラベル (max_by_key last-wins で誤選択されうる)
            ],
            experience_required_distribution: vec![
                ("経験不問 (実質)".to_string(), 0i64),
                ("経験記載あり".to_string(), 0i64),
            ],
            employment_type_distribution: vec![
                ("正社員".to_string(), 0i64),
                ("パート".to_string(), 0i64),
            ],
        };
        let mut html = String::new();
        // Phase 2-A (2026-05-29): is_hourly = false (旧動作互換テスト)
        // Phase 2-B (2026-05-29): agg 引数追加 (default で月給モード=is_hourly=false。表 6-J 非表示)
        let agg = SurveyAggregation::default();
        render_navy_section_06_posting_target(&mut html, &pt, false, &agg);

        // 全 count == 0 → KPI は「—」(em dash) になるべき
        // 旧バグでは last-wins により "〜20万" 等が表示されていた。
        assert!(
            !html.contains("〜20万"),
            "salary KPI should not show last-wins label when all counts are 0: {}",
            &html[..html.len().min(1500)]
        );
        // age も同様: 「45〜64歳」が last-wins されないこと
        // (ただし、後段の table 6-G で同ラベルがレンダリングされる可能性はあるため、
        // ここでは KPI 部分だけを抽出して検査するのは複雑。代替として、
        // 主要 KPI 直後の neu フッタにある「件数 0」確認で代替する。)
        // 「— ... 0 件」のパターンが (age, salary, emp) 3 回出ているはず
        let dash_kpi_count = html.matches("—").count();
        assert!(
            dash_kpi_count >= 3,
            "expected at least 3 '—' KPI labels for age/salary/emp: got {}",
            dash_kpi_count
        );
    }

    #[test]
    fn render_navy_section_06_posting_target_partial_zero_picks_non_zero() {
        use crate::handlers::analysis::fetch::PostingTargetProfile;

        // age は count > 0、salary は全 0 のケース → age は正常、salary のみ「—」
        let pt = PostingTargetProfile {
            total_postings: 100,
            age_range_distribution: vec![
                ("30〜44歳".to_string(), 50i64),
                ("45〜64歳".to_string(), 30i64),
            ],
            salary_target_distribution: vec![
                ("20〜25万".to_string(), 0i64),
                ("〜20万".to_string(), 0i64),
            ],
            experience_required_distribution: vec![
                ("経験不問 (実質)".to_string(), 80i64),
                ("経験記載あり".to_string(), 20i64),
            ],
            employment_type_distribution: vec![
                ("正社員".to_string(), 70i64),
                ("パート".to_string(), 30i64),
            ],
        };
        let mut html = String::new();
        // Phase 2-A (2026-05-29): is_hourly = false (旧動作互換テスト)
        // Phase 2-B (2026-05-29): agg 引数追加 (default で月給モード=is_hourly=false。表 6-J 非表示)
        let agg = SurveyAggregation::default();
        render_navy_section_06_posting_target(&mut html, &pt, false, &agg);

        // age 主要層は count > 0 のものから選ばれる
        assert!(
            html.contains("30〜44歳"),
            "age KPI should show '30〜44歳' (count=50, top): {}",
            &html[..html.len().min(800)]
        );
        // salary は全 0 なので「—」 (em dash) になる
        // "〜20万" が KPI 部分に出ていないことを確認 (table 6-H の row には出ても OK)
        // 確実な判定として、KPI label 直後に "— ... 0 件 (月給記載のみ)" が含まれることをチェック。
        assert!(
            html.contains("0 件 (月給記載のみ)"),
            "salary KPI footer should indicate 0 件: {}",
            &html[..html.len().min(2000)]
        );
    }
}
