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
    render_navy_section_05_companies, render_navy_section_placeholders, select_notable_companies,
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

// A1 Commit 7 (β Section Team, 2026-05-30): Section 06 (人材デモグラフィック) +
// Section 07 (最低賃金・ライフスタイル) を独立モジュールに分離。
// 元の `render_navy_section_06_demographics` / `render_navy_section_07_lifestyle`
// は外部 (report_html/mod.rs) から `navy_report::render_navy_section_*` の path で
// 呼ばれているため、ここで `pub(super)` 再エクスポートして path 互換を維持する。
//
// `build_navy_minwage_premium_histogram_svg` (Section 07 H3) は report_html 配下の
// `hourly_report_qa_test.rs` から `super::navy_report::build_*` path で参照されている
// (時給モードフィーチャの QA テスト)。`navy_report` モジュール外への公開が必要なため、
// section ファイル内で `pub(crate)` に昇格し (`pub(super)` は階層不足で E0364 になる)、
// ここで `pub(super) use` で再エクスポートして従来 path を維持する。
//
// 2026-06-01: 図 6-3 / 表 6-G/H/I/J (HW postings 求人側集計ブロック) を削除。
// これに伴い `render_navy_section_06_posting_target` / `build_distribution_table` /
// `build_hourly_band_distribution` の再エクスポートを撤去。`hourly_report_qa_test.rs`
// 側の H4 / H-INT-03 テスト (表 6-J 関連) も同時削除済み。
//
// `label_for_column` は `build_navy_auto_table` (mod.rs に残置) から参照されるため、
// section_07_lifestyle.rs 内で `pub(crate)` に昇格し、ここで `pub(super) use` で
// 再エクスポートする。`build_navy_auto_table` 側はこれまで通り unqualified で呼べる。
//
// 内部 helper (`age_lo` / `age_sort_key` / `build_navy_pyramid_svg` /
// `build_navy_pyramid_svg_mini` / `build_demographics_so_what` /
// `build_navy_minwage_vs_salary_table` / `build_navy_household_vs_salary_table` /
// `build_navy_lifestyle_facilities_table` / `build_navy_minwage_chart` /
// `build_navy_household_table` / `build_lifestyle_so_what` /
// 定数 `PREMIUM_BUCKETS`) は各 section_*.rs 内に閉じ込め (module-private)、
// 外部公開はしない。API 表面は不変。
pub(super) mod section_06_demographics;
pub(super) mod section_07_lifestyle;
pub(super) use section_06_demographics::{
    build_navy_pyramid_svg, build_navy_pyramid_svg_mini, render_navy_section_06_demographics,
};
pub(super) use section_07_lifestyle::{
    build_navy_minwage_premium_histogram_svg, label_for_column, render_navy_section_07_lifestyle,
};

// 2026-06-24: Section 07.5 (求人ボックス 年間休日 × 給与 詳細) を独立モジュールとして追加。
// 求人ボックス CSV の description から年間休日を抽出した個別求人一覧 +
// カテゴリ分布 + 給与帯別 平均年間休日 を表示する。Indeed CSV では自動スキップ。
pub(super) mod section_07_5_jobbox_detail;
pub(super) use section_07_5_jobbox_detail::render_navy_section_jobbox_detail;

// 2026-06-30: Section 07.6 (人気度シグナル) を独立モジュールとして追加。
// Indeed (SP) CSV の `css-u74ql7` 列から抽出した「人気」「超人気」タグの
// 集計を表示。人気タグが 1 件もなければ (Indeed SP 以外) セクションごとスキップ。
pub(super) mod section_07_6_popularity;
pub(super) use section_07_6_popularity::render_navy_section_popularity;

// P0-8 (2026-05-30): Section 09 (Market Intelligence variant 専用) を独立モジュールに追加。
// MarketIntelligence variant のときだけ 6 サブセクションを追加表示する。
// 旧 `market_intelligence.rs` (handlers/survey/report_html/) は媒体分析タブ画面表示
// (Turso ベース) として温存し、本 Section 09 は PDF レポート navy_report 経路
// (hw_context ベース) として並立する。両モジュールはデータソースで分離。
// 設計詳細: docs/NAVY_SECTION_09_DESIGN.md
pub(super) mod section_09_market_intelligence;
pub(super) use section_09_market_intelligence::render_navy_section_09_market_intelligence;

use super::super::super::analysis::fetch::CsvCompanySalary;
use super::super::super::helpers::{escape_html, format_number};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{EmpTypeSalary, SurveyAggregation};
use super::super::job_seeker::JobSeekerAnalysis;
use super::salary_summary;
use super::ReportVariant;

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
    // 決定性保証 (2026-07-06): HashMap の反復順は非決定的。
    // sort_by_key は安定ソートなので、先に keys.sort() で辞書順に揃えると
    // 同一 priority 値どうしの相対順序も固定される。
    // 結果: どの実行順でも同じ HTML 列順が出力される。
    keys.sort();
    let priority = [
        "year",
        "fiscal_year",
        "reference_year",
        "reference_date",
        "prefecture",
        "municipality",
        "industry_name",
        "industry",
        "category",
        "subcategory",
        "name",
        "label",
        // 地理指標: 表 2-B (section_02_region) で habitable_area_km2 が確実に
        // 表示されるよう優先度を付与。HashMap の反復順は非決定的なため、
        // priority=99 のまま放置すると truncate(6) で切り落とされる恐れがある。
        // これら 3 列は v2_external_geography 専用であり、他の ext_* テーブルには
        // 存在しないため、他セクションの auto_table 出力には影響しない。
        // (2026-07-03 修正: 表 2-B ラベルと表示列の不一致を解消)
        "habitable_area_km2",         // 可住地面積 → 表 2-B タイトルに明記
        "population_density_per_km2", // 人口密度   → 表 2-B タイトルに明記
        "total_area_km2",             // 総面積     → 市の規模感の基礎情報として優先
                                      // habitable_density_per_km2 (可住地密度) は上記 2 列からの
                                      // 派生値のため priority=99 のまま → 7 列目で truncate(6) 落ち
    ];
    keys.sort_by_key(|k| priority.iter().position(|p| *p == k.as_str()).unwrap_or(99));
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
                    if str_val.is_empty() {
                        "—".to_string()
                    } else {
                        escape_html(&str_val)
                    }
                }
                Some(jv) if jv.is_i64() || jv.is_u64() => format_number(get_i64(r, k)),
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
                    if jv.as_bool() == Some(true) {
                        "✓".to_string()
                    } else {
                        "—".to_string()
                    }
                }
                Some(jv) if jv.is_null() => "—".to_string(),
                None => "—".to_string(),
                Some(_) => "—".to_string(), // 配列やオブジェクト等は表示しない
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
#[cfg(test)]
mod tests;
