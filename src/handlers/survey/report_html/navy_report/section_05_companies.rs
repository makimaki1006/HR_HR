//! Section 05 - 地域企業構造 (Phase 3 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 5 / β Section Team / 2026-05-30) で抽出。
//!
//! 元 `navy_report/mod.rs` L1305-L2167 の以下を物理コピー:
//! - `render_navy_section_placeholders`         (公開 API: pub(crate))
//! - `render_navy_section_05_companies`         (公開 API: pub(crate))
//! - `select_notable_companies`                 (pub(crate) — mod.rs `tests` mod から参照)
//! - `build_navy_csv_company_salary_table`      (pub(crate) — mod.rs `tests` mod から参照)
//! - `build_navy_notable_companies_block`       (pub(crate) — mod.rs `tests` mod から参照)
//! - `build_navy_industry_table`                (private helper)
//! - `build_navy_industry_bars`                 (private helper)
//! - `build_navy_growth_decline_matrix`         (private helper)
//! - `build_navy_company_list`                  (private helper)
//! - `build_companies_so_what`                  (private helper)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_placeholders` / `render_navy_section_05_companies`
//!   (Commit 2/3/4 パターン踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//!
//! 内部 helper のうち以下 3 つは `mod.rs` 末尾の `#[cfg(test)] mod tests`
//! (use super::*;) から直接参照されている。`pub(crate)` に昇格し
//! (`pub(super)` は階層不足で E0364 になる)、mod.rs から
//! `pub(super) use section_05_companies::{...};` で再エクスポートすることで
//! `tests` mod の `use super::*;` に乗せて従来通り unqualified で参照可能とする:
//!   - `select_notable_companies`
//!   - `build_navy_csv_company_salary_table`
//!   - `build_navy_notable_companies_block`
//!
//! 残りの helper (`build_navy_industry_table` / `build_navy_industry_bars` /
//! `build_navy_growth_decline_matrix` / `build_navy_company_list` /
//! `build_companies_so_what`) は本ファイル内のみで使用。`navy_report` モジュール
//! 外への露出はない。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_05_companies):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::analysis::fetch::CsvCompanySalary;
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::ReportVariant;
use super::common::{push_kpi, push_page_head, push_region_scope_banner};

// ============================================================
// Section 06-08 placeholder (Phase 3-4 で本実装に差し替え)
// ============================================================

pub(crate) fn render_navy_section_placeholders(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    variant: ReportVariant,
    now: &str,
) {
    let _ = (hw_context, variant, now);
    let sections = [(
        "SECTION 08",
        "注記・出典・免責",
        "データソース / 集計定義 / 免責事項。Phase 4 で実装予定。",
    )];
    for (code, title, body_text) in sections {
        html.push_str("<section class=\"page-navy\" role=\"region\">\n");
        push_page_head(
            html,
            code,
            title,
            "Round 24 段階移行: navy_report で本実装に差し替え中",
        );
        html.push_str(&format!(
            "<div class=\"so-what\" style=\"margin-top:4mm;\">\
             <div class=\"sw-label\">UNDER MIGRATION</div>\
             <div class=\"sw-body\">{}<br>本セクションは新デザイン (見本 Recruitment_Market_Report.html) に\
             基づき再構築中です。次のコミット群で navy 構造の本実装に置き換わります。</div>\
             </div>\n",
            escape_html(body_text)
        ));
        html.push_str("</section>\n");
    }
}

// ============================================================
// Section 05: 地域企業構造 — 関数本体
// ============================================================

pub(crate) fn render_navy_section_05_companies(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    by_company: &[super::super::super::aggregator::CompanyAgg],
    salesnow_segments: &super::super::super::super::company::fetch::RegionalCompanySegments,
    // 2026-05-14: 業界フィルタ指定時に同業界版を併記するための追加引数。
    //   industry_filter=Some(...) かつ segments_industry が空でない時のみ
    //   各表 (5-B/C/D/E/F) の直後に同業界版 (5-B'/C'/D'/E'/F') を描画する。
    salesnow_segments_industry: &super::super::super::super::company::fetch::RegionalCompanySegments,
    industry_filter: Option<&str>,
    variant: ReportVariant,
    target_region: &str,
) {
    let show_hw = matches!(variant, ReportVariant::Full);

    html.push_str("<section class=\"page-navy navy-companies\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 05",
        "地域企業構造",
        "産業構成 / 法人セグメント / 規模帯ベンチマーク",
    );
    push_region_scope_banner(html, target_region);

    // 2026-05-22 #246 ユーザー指摘対応: 法人セグメント 0 社時のフォールバック表示。
    // salesnow_segments + hw_industry + ext_industry_employees すべて空の場合、
    // 各表が個別に「該当企業なし」を出すよりも Section 全体に対する明示的な
    // fallback message を 1 つ出して残り表を skip する方が情報密度が高い。
    let _is_fully_empty = {
        let sn_total = salesnow_segments.pool_size;
        let sn_industry_total = salesnow_segments_industry.pool_size;
        let hw_industry_count = hw_context
            .map(|c| c.hw_industry_counts.iter().map(|(_, n)| n).sum::<i64>())
            .unwrap_or(0);
        let ext_industry_count = hw_context
            .map(|c| c.ext_industry_employees.len() as i64)
            .unwrap_or(0);
        // P2-2 (2026-05-28): csv_company_ranking もチェックに含める。
        // CSV 求人データのみあって他データソース全滅のケースで「該当データなし」と
        // 早期 return すると CSV 企業別給与ランキング (5-G/5-H) が表示されなくなる。
        let csv_ranking_count = hw_context
            .map(|c| c.csv_company_ranking.len() as i64)
            .unwrap_or(0);
        sn_total == 0
            && sn_industry_total == 0
            && hw_industry_count == 0
            && ext_industry_count == 0
            && csv_ranking_count == 0
    };
    if _is_fully_empty {
        html.push_str(
            "<div class=\"empty-section-fallback\" style=\"margin:8mm 0;padding:12px 16px;\
             background:#f3f4f6;border-left:4px solid #9ca3af;border-radius:4px;\
             font-size:10pt;line-height:1.7;\">\
             <p style=\"font-weight:600;color:#374151;margin:0 0 6px;\">\
             📍 該当地域に企業データが見つかりませんでした</p>\
             <p style=\"color:#6b7280;margin:0;font-size:9.5pt;\">\
             地域注目企業データ・ハローワーク産業構成・e-Stat 経済センサスの\
             いずれにも該当する事業所が登録されていません。以下が考えられます:</p>\
             <ul style=\"margin:6px 0 0 18px;color:#6b7280;font-size:9.5pt;\">\
             <li>該当市区町村が小規模で企業データ収録対象外</li>\
             <li>合併等で旧自治体名のため最新 DB と不一致 (例: 合併消滅町村)</li>\
             <li>業界フィルタが厳しすぎる (フィルタを外して再試行を推奨)</li>\
             </ul>\
             </div>\n",
        );
        html.push_str("</section>\n");
        return;
    }

    let industry_employees: Vec<(String, i64)> = hw_context
        .map(|ctx| {
            use super::super::super::super::helpers::{get_f64, get_str};
            ctx.ext_industry_employees
                .iter()
                .map(|r| {
                    (
                        get_str(r, "industry_name"),
                        get_f64(r, "employees_total") as i64,
                    )
                })
                .filter(|(n, c)| !n.is_empty() && *c > 0)
                .collect()
        })
        .unwrap_or_default();
    let mut industry_sorted = industry_employees.clone();
    industry_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let industry_total: i64 = industry_sorted.iter().map(|(_, c)| *c).sum();

    let hw_industry: Vec<(String, i64)> = hw_context
        .map(|ctx| ctx.hw_industry_counts.clone())
        .unwrap_or_default();
    let hw_total: i64 = hw_industry.iter().map(|(_, c)| *c).sum();

    let pool_size = salesnow_segments.pool_size;
    let n_large = salesnow_segments.large.len();
    let n_mid = salesnow_segments.mid.len();
    let n_growth = salesnow_segments.growth.len();
    let n_hiring = salesnow_segments.hiring.len();
    let n_companies_csv = by_company.len();

    let lede = format!(
        "対象地域の企業構造を把握します。国勢調査 産業大分類 <strong>{}</strong> 区分 / \
         地域企業データ <strong>{}</strong> 社{}。CSV 上にユニーク企業 <strong>{}</strong> 社が確認できます。",
        industry_sorted.len(),
        format_number(pool_size as i64),
        if show_hw && hw_total > 0 {
            format!(" / 求人媒体 産業大分類 {} 件", format_number(hw_total))
        } else {
            String::new()
        },
        format_number(n_companies_csv as i64),
    );
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    html.push_str("<div class=\"block-title\">図 5-1 &nbsp;法人セグメント (規模 × 動向)</div>\n");
    // pool_size = 0 のときは地域企業データ未取得を明示し、誤解 (0社=企業が無い) を防ぐ
    if pool_size == 0 {
        html.push_str(
            "<div class=\"so-what\" style=\"margin-top:0; margin-bottom:6mm; background: var(--rule-soft); color: var(--ink-soft);\">\
             <div class=\"sw-label\">DATA</div>\
             <div class=\"sw-body\">地域企業データ (外部企業データベース) を取得できませんでした。\
             以下の法人セグメント KPI は<strong>表示対象データなし</strong>のため、企業活動の評価には用いないでください。</div>\
             </div>\n",
        );
    }
    html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    let na = pool_size == 0;
    let kpi_val = |n: usize| {
        if na {
            "—".to_string()
        } else {
            format!("{}", n)
        }
    };
    let kpi_unit = if na { "" } else { "社" };
    push_kpi(
        html,
        "大手企業",
        &kpi_val(n_large),
        kpi_unit,
        "neu",
        "従業員 300+ 名級",
        false,
    );
    push_kpi(
        html,
        "中堅企業",
        &kpi_val(n_mid),
        kpi_unit,
        "neu",
        "従業員 50-299 名",
        false,
    );
    push_kpi(
        html,
        "急成長企業",
        &kpi_val(n_growth),
        kpi_unit,
        if n_growth > 0 { "pos" } else { "neu" },
        "1Y 人員増加率 +10% 超",
        true,
    );
    if show_hw {
        push_kpi(
            html,
            "採用活発企業",
            &kpi_val(n_hiring),
            kpi_unit,
            if n_hiring > 0 { "warn" } else { "neu" },
            "求人媒体掲載 5 件以上",
            false,
        );
    } else {
        push_kpi(
            html,
            "母集団規模",
            &if na {
                "—".to_string()
            } else {
                format_number(pool_size as i64)
            },
            kpi_unit,
            "neu",
            if na {
                "地域企業データ未取得"
            } else {
                "地域企業データ取得社数"
            },
            false,
        );
    }
    html.push_str("</div>\n");

    html.push_str("<div class=\"block-title block-title-spaced\">表 5-A &nbsp;産業大分類 構成 (件数最多 8 産業)</div>\n");
    html.push_str(&build_navy_industry_table(
        &industry_sorted,
        industry_total,
        &hw_industry,
        hw_total,
        show_hw,
    ));

    if !industry_sorted.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 5-2 &nbsp;産業大分類シェア (国勢調査)</div>\n");
        html.push_str(&build_navy_industry_bars(&industry_sorted, industry_total));
        html.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_industry_structure (都道府県粒度)。集計コード AS/AR/CR 除外。</p>\n");
    }

    // 2026-05-14: 業界フィルタが指定されている時、同業界版を併記する。
    //   各表 (5-B〜5-F) を 全業界 → 同業界 の順に描画。
    let industry_label = industry_filter
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let has_industry = industry_label.is_some();

    // 2026-05-15: 業界指定時、segments_industry が空でも『該当企業なし』を明示する
    //   (旧コード: 空なら描画スキップ → ユーザーには『業界フィルタ効いてない』に見える)
    let muni_str = hw_context.map(|c| c.muni.clone()).unwrap_or_default();
    // 2026-05-15: 業界指定時は通勤圏 (30km 圏) で SalesNow を取得しているため、
    //   「藤岡市」単独ではなく「藤岡市 周辺」と明示してユーザーに認識誤りを防ぐ。
    let muni_label = if muni_str.is_empty() {
        String::new()
    } else {
        format!("{} 周辺 × ", escape_html(&muni_str))
    };

    let empty_row_html = |colspan: i64| -> String {
        format!(
            "<table class=\"table-navy\"><tbody>\
             <tr><td colspan=\"{}\" class=\"dim\" style=\"text-align:center;padding:8mm 4mm;\">該当企業なし</td></tr>\
             </tbody></table>\n",
            colspan
        )
    };

    if !salesnow_segments.growth.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-B &nbsp;急成長企業 (全業界、1Y +10%〜+300%、件数最多 8 社)</div>\n");
        html.push_str(&build_navy_company_list(
            &salesnow_segments.growth,
            8,
            show_hw,
        ));
    }
    if has_industry {
        let ind = industry_label.as_deref().unwrap_or("");
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 5-B′ &nbsp;急成長企業 ({}{}、1Y +10%〜+300%、件数最多 8 社)</div>\n",
            muni_label, escape_html(ind)
        ));
        if !salesnow_segments_industry.growth.is_empty() {
            html.push_str(&build_navy_company_list(
                &salesnow_segments_industry.growth,
                8,
                show_hw,
            ));
        } else {
            html.push_str(&empty_row_html(if show_hw { 6 } else { 5 }));
        }
    }

    // -- 大手企業 (employee_count Top)
    if !salesnow_segments.large.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-C &nbsp;大手企業 (全業界、従業員 300+ 名級、件数最多 8 社)</div>\n");
        html.push_str(&build_navy_company_list(
            &salesnow_segments.large,
            8,
            show_hw,
        ));
    }
    if has_industry {
        let ind = industry_label.as_deref().unwrap_or("");
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 5-C′ &nbsp;大手企業 ({}{}、従業員 300+ 名級、件数最多 8 社)</div>\n",
            muni_label, escape_html(ind)
        ));
        if !salesnow_segments_industry.large.is_empty() {
            html.push_str(&build_navy_company_list(
                &salesnow_segments_industry.large,
                8,
                show_hw,
            ));
        } else {
            html.push_str(&empty_row_html(if show_hw { 6 } else { 5 }));
        }
    }

    // -- 中堅企業 (50-300 名)
    if !salesnow_segments.mid.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-D &nbsp;中堅企業 (全業界、従業員 50-299 名、件数最多 8 社)</div>\n");
        html.push_str(&build_navy_company_list(&salesnow_segments.mid, 8, show_hw));
    }
    if has_industry {
        let ind = industry_label.as_deref().unwrap_or("");
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 5-D′ &nbsp;中堅企業 ({}{}、従業員 50-299 名、件数最多 8 社)</div>\n",
            muni_label, escape_html(ind)
        ));
        if !salesnow_segments_industry.mid.is_empty() {
            html.push_str(&build_navy_company_list(
                &salesnow_segments_industry.mid,
                8,
                show_hw,
            ));
        } else {
            html.push_str(&empty_row_html(if show_hw { 6 } else { 5 }));
        }
    }

    // -- 採用活発企業 (Full のみ、求人媒体掲載 5 件以上)
    if show_hw && !salesnow_segments.hiring.is_empty() {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-E &nbsp;採用活発企業 (全業界、求人媒体掲載 5 件以上、件数最多 8 社)</div>\n");
        html.push_str(&build_navy_company_list(
            &salesnow_segments.hiring,
            8,
            show_hw,
        ));
    }
    if show_hw && has_industry {
        let ind = industry_label.as_deref().unwrap_or("");
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 5-E′ &nbsp;採用活発企業 ({}{}、求人媒体掲載 5 件以上、件数最多 8 社)</div>\n",
            muni_label, escape_html(ind)
        ));
        if !salesnow_segments_industry.hiring.is_empty() {
            html.push_str(&build_navy_company_list(
                &salesnow_segments_industry.hiring,
                8,
                show_hw,
            ));
        } else {
            html.push_str(&empty_row_html(6));
        }
    }

    // -- 規模 × 動向 6 マトリクス: 増員傾向 (large/mid/small) + 減少傾向 (large/mid/small)
    let g_large = salesnow_segments.growth_large.len();
    let g_mid = salesnow_segments.growth_mid.len();
    let g_small = salesnow_segments.growth_small.len();
    let d_large = salesnow_segments.decline_large.len();
    let d_mid = salesnow_segments.decline_mid.len();
    let d_small = salesnow_segments.decline_small.len();
    if g_large + g_mid + g_small + d_large + d_mid + d_small > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 5-F &nbsp;規模 × 動向 6 マトリクス (全業界、1Y 人員変動)</div>\n");
        html.push_str(&build_navy_growth_decline_matrix(salesnow_segments));
    }
    if has_industry {
        let ind = industry_label.as_deref().unwrap_or("");
        let ig_l = salesnow_segments_industry.growth_large.len();
        let ig_m = salesnow_segments_industry.growth_mid.len();
        let ig_s = salesnow_segments_industry.growth_small.len();
        let id_l = salesnow_segments_industry.decline_large.len();
        let id_m = salesnow_segments_industry.decline_mid.len();
        let id_s = salesnow_segments_industry.decline_small.len();
        // 2026-05-22 ユーザー指摘: データ 0 件時に「表 5-F′ ... 該当企業なし」だけが
        // 残るレイアウトを廃止。データ 0 件なら section 全体を skip して情報密度向上。
        // (旧コード: タイトル + empty_row_html を常に出力していた)
        if ig_l + ig_m + ig_s + id_l + id_m + id_s > 0 {
            html.push_str(&format!(
                "<div class=\"block-title block-title-spaced\">表 5-F′ &nbsp;規模 × 動向 6 マトリクス ({}{}、1Y 人員変動)</div>\n",
                muni_label, escape_html(ind)
            ));
            html.push_str(&build_navy_growth_decline_matrix(
                salesnow_segments_industry,
            ));
        }
    }

    // ========================================================================
    // P2-2 (2026-05-28): CSV 企業別給与ランキング (表 5-G) + 注目企業リスト (表 5-H)
    //
    // データ: ctx.csv_company_ranking (postings facility_name 別 給与中央値 上位 30 社)
    // 出典: CSV 求人データ集計 (SalesNow 由来の地域企業データとは別)
    // 表示条件: ctx.csv_company_ranking が空 (or hw_context が None) なら表示しない
    // 配置: 既存 SO WHAT の直前
    // ========================================================================
    if let Some(ctx) = hw_context {
        if !ctx.csv_company_ranking.is_empty() {
            html.push_str(&build_navy_csv_company_salary_table(
                &ctx.csv_company_ranking,
                10,
            ));
            html.push_str(&build_navy_notable_companies_block(
                &ctx.csv_company_ranking,
                5,
            ));
            html.push_str(
                "<p class=\"caption\">出典: CSV 求人データ集計。給与は月給換算後の中央値 (万円)。\
                 注目企業 = 求人数 top 5 と 給与中央値 (上限) top 5 の和集合。\
                 求人数 2 件未満の施設は代表性確保のため除外。</p>\n",
            );
        }
    }

    let so_what = build_companies_so_what(
        &industry_sorted,
        industry_total,
        pool_size,
        n_growth,
        n_hiring,
        show_hw,
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

// ============================================================
// P2-2 (2026-05-28): 注目企業選定ロジック + テーブル / ブロック描画
// ============================================================

/// 注目企業を抽出する。
///
/// 定義: **求人数 top N の集合 ∪ 給与中央値 (上限) top N の集合** (重複は 1 件にまとめる)。
///
/// 引数:
/// - `ranking`: 給与中央値 (上限) 降順でソート済の企業ランキング (fetch 側でソート済)
/// - `top_n`: 各軸の上位件数 (推奨 5)
///
/// 戻り値: 和集合の企業参照 Vec。出現順は「求人数 top N → 給与 top N の未出現分」を維持。
///
/// silent fallback 防御:
/// - `ranking` 空 → 空 Vec
/// - `top_n` 0 → 空 Vec
///
/// 不変条件: 戻り値 size <= 2 * top_n (重複なし、重複時は < 2 * top_n)
pub(crate) fn select_notable_companies<'a>(
    ranking: &'a [CsvCompanySalary],
    top_n: usize,
) -> Vec<&'a CsvCompanySalary> {
    if ranking.is_empty() || top_n == 0 {
        return Vec::new();
    }

    // 求人数 top N (降順、同値時は upper_median 降順)
    // ranking は upper_median 降順のため、元順序を壊さない indices で取得。
    let mut by_posting: Vec<usize> = (0..ranking.len()).collect();
    by_posting.sort_by(|&a, &b| {
        ranking[b]
            .posting_count
            .cmp(&ranking[a].posting_count)
            .then_with(|| {
                ranking[b]
                    .salary_upper_median
                    .partial_cmp(&ranking[a].salary_upper_median)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let posting_top: Vec<usize> = by_posting.into_iter().take(top_n).collect();

    // 給与 top N: ranking は upper_median 降順のため先頭 N 件
    let salary_top: Vec<usize> = (0..ranking.len()).take(top_n).collect();

    // 和集合: posting_top を先に出力 → salary_top の未出現分を追加
    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<&CsvCompanySalary> = Vec::new();
    for idx in posting_top.iter().chain(salary_top.iter()) {
        if seen.insert(*idx) {
            result.push(&ranking[*idx]);
        }
    }
    result
}

/// 表 5-G: 企業別給与ランキング (上位 limit 社、上限給与中央値 降順)
pub(crate) fn build_navy_csv_company_salary_table(
    ranking: &[CsvCompanySalary],
    limit: usize,
) -> String {
    // Phase 2-A (2026-05-29): 先頭エントリの native_unit を見て表示単位を切替。
    //   ranking 全体は単一 wage_mode で fetch されるため、先頭の native_unit で全行が代表される。
    //   空 ranking / 空文字列 → "月給" を旧動作互換のデフォルトに (silent fallback ではなく明示)。
    let native_unit = ranking
        .first()
        .map(|c| c.native_unit.as_str())
        .unwrap_or("");
    let is_hourly = native_unit == "時給";
    let unit_label_short: &str = if is_hourly { "円/時" } else { "万円" };
    let unit_decimals: usize = if is_hourly { 0 } else { 1 };
    let empty_msg = if is_hourly {
        "該当企業なし (求人数 2 件以上 + 時給データありの施設なし)"
    } else {
        "該当企業なし (求人数 2 件以上 + 月給データありの施設なし)"
    };

    let mut s = String::from(
        "<div class=\"block-title block-title-spaced\">\
         表 5-G &nbsp;企業別給与ランキング (CSV 求人 集計、上限給与中央値 上位 ",
    );
    s.push_str(&format!("{}", limit));
    s.push_str(" 社、求人数 2 件以上)</div>\n");
    // R2-P1-4 (ultrathink Round 2, 2026-05-28): a11y のため列ヘッダに scope="col" を付与。
    s.push_str("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th scope=\"col\">順位</th><th scope=\"col\">法人名</th>");
    s.push_str("<th scope=\"col\" class=\"num\">求人数</th>");
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">下限給与中央値<br>({})</th>",
        unit_label_short
    ));
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">上限給与中央値<br>({})</th>",
        unit_label_short
    ));
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">レンジ幅<br>({})</th>",
        unit_label_short
    ));
    s.push_str("</tr></thead>\n<tbody>\n");

    let top: Vec<&CsvCompanySalary> = ranking.iter().take(limit).collect();
    if top.is_empty() {
        s.push_str(&format!(
            "<tr><td colspan=\"6\" class=\"dim\" style=\"text-align:center;padding:8mm 4mm;\">\
             {}</td></tr>\n",
            empty_msg
        ));
    } else {
        for (i, c) in top.iter().enumerate() {
            // 不変条件: salary_upper_median >= salary_lower_median (fetch SQL でフィルタ済)
            // 二重防衛として render 側でも max(0) クランプ。
            let range_width = (c.salary_upper_median - c.salary_lower_median).max(0.0);
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            // Phase 2-A: 桁数を unit_decimals で制御 (月給=1桁、時給=0桁)
            s.push_str(&format!(
                "<tr{}><td class=\"num bold\">{}</td>\
                 <td><strong>{}</strong></td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{:.*}</td>\
                 <td class=\"num bold\">{:.*}</td>\
                 <td class=\"num\">{:.*}</td></tr>\n",
                row_class,
                i + 1,
                escape_html(&c.facility_name),
                format_number(c.posting_count),
                unit_decimals,
                c.salary_lower_median,
                unit_decimals,
                c.salary_upper_median,
                unit_decimals,
                range_width,
            ));
        }
    }
    s.push_str("</tbody></table>\n");
    s
}

/// 注目企業リスト (求人数 top N ∩ 給与 top N の和集合) を 5-H として描画
///
/// Phase 2-A (2026-05-29): 先頭エントリの native_unit で表示単位を切替。
pub(crate) fn build_navy_notable_companies_block(
    ranking: &[CsvCompanySalary],
    top_n: usize,
) -> String {
    let notable = select_notable_companies(ranking, top_n);
    if notable.is_empty() {
        return String::new();
    }
    // Phase 2-A: notable[0] の native_unit を見て表示単位を切替
    let is_hourly = notable
        .first()
        .map(|c| c.native_unit.as_str() == "時給")
        .unwrap_or(false);
    let unit_label = if is_hourly { "円/時" } else { "万円" };
    let decimals = if is_hourly { 0 } else { 1 };

    let mut s = String::from(
        "<div class=\"block-title block-title-spaced\">\
         表 5-H &nbsp;注目企業リスト (求人数上位 ∩ 給与上位、和集合)</div>\n",
    );
    // R2-P1-4 (ultrathink Round 2, 2026-05-28): a11y のため列ヘッダに scope="col" を付与。
    s.push_str("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th scope=\"col\">No.</th><th scope=\"col\">法人名</th>");
    s.push_str("<th scope=\"col\" class=\"num\">求人数</th>");
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">給与レンジ ({})</th>",
        unit_label
    ));
    s.push_str("</tr></thead>\n<tbody>\n");

    for (i, c) in notable.iter().enumerate() {
        s.push_str(&format!(
            "<tr><td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{:.*}〜{:.*}</td></tr>\n",
            i + 1,
            escape_html(&c.facility_name),
            format_number(c.posting_count),
            decimals,
            c.salary_lower_median,
            decimals,
            c.salary_upper_median,
        ));
    }
    s.push_str("</tbody></table>\n");
    s
}

fn build_navy_industry_table(
    industry_sorted: &[(String, i64)],
    industry_total: i64,
    hw_industry: &[(String, i64)],
    hw_total: i64,
    show_hw: bool,
) -> String {
    let hw_map: std::collections::HashMap<&str, i64> =
        hw_industry.iter().map(|(n, c)| (n.as_str(), *c)).collect();

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>産業大分類</th>");
    s.push_str("<th class=\"num\">就業者数</th>");
    s.push_str("<th class=\"num\">シェア</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
        s.push_str("<th class=\"num\">媒体シェア</th>");
        s.push_str("<th>差分</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let top8: Vec<&(String, i64)> = industry_sorted.iter().take(8).collect();
    if top8.is_empty() {
        let cols = if show_hw { 7 } else { 4 };
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">国勢調査産業構造データを取得できませんでした。</td></tr>\n",
            cols
        ));
    } else {
        for (i, (name, employees)) in top8.iter().enumerate() {
            let share_pct = if industry_total > 0 {
                *employees as f64 / industry_total as f64 * 100.0
            } else {
                0.0
            };
            let row_class = if i == 0 { " class=\"hl\"" } else { "" };
            s.push_str(&format!(
                "<tr{}><td class=\"num bold\">{}</td><td><strong>{}</strong></td>\
                 <td class=\"num bold\">{}</td><td class=\"num\">{:.1}%</td>",
                row_class,
                i + 1,
                escape_html(name),
                format_number(*employees),
                share_pct
            ));
            if show_hw {
                let hw_count = hw_map.get(name.as_str()).copied().unwrap_or(0);
                let hw_share = if hw_total > 0 {
                    hw_count as f64 / hw_total as f64 * 100.0
                } else {
                    0.0
                };
                let diff = hw_share - share_pct;
                let (tag, label) = if diff >= 5.0 {
                    ("warn", "媒体側に偏り")
                } else if diff <= -5.0 {
                    ("neu", "就業者構成優位")
                } else {
                    ("neu", "ほぼ均衡")
                };
                s.push_str(&format!(
                    "<td class=\"num\">{}</td><td class=\"num\">{:.1}%</td>\
                     <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:+.1}pt</span></td>",
                    format_number(hw_count),
                    hw_share,
                    tag,
                    label,
                    diff
                ));
            }
            s.push_str("</tr>\n");
        }
    }
    s.push_str("</tbody></table>\n");
    if show_hw {
        s.push_str("<p class=\"caption\">就業者数は国勢調査ベース、媒体掲載数は求人媒体ローカル DB。差分 (媒体シェア − 就業者シェア) は採用需要の偏りを示します。</p>\n");
    } else {
        s.push_str("<p class=\"caption\">出典: 国勢調査 v2_external_industry_structure (都道府県粒度)。集計コード AS/AR/CR 除外。</p>\n");
    }
    s
}

fn build_navy_industry_bars(industry_sorted: &[(String, i64)], total: i64) -> String {
    let top10: Vec<&(String, i64)> = industry_sorted.iter().take(10).collect();
    if top10.is_empty() || total <= 0 {
        return String::new();
    }
    let w = 720.0;
    let row_h = 24.0;
    let label_w = 200.0;
    let val_w = 90.0;
    let bar_x = label_w;
    let bar_w = w - label_w - val_w - 16.0;
    let h = top10.len() as f64 * row_h + 20.0;

    let max_share = top10
        .iter()
        .map(|(_, c)| *c as f64 / total as f64)
        .fold(0.0, f64::max)
        .max(0.01);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"産業構成バー\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );
    for (i, (name, count)) in top10.iter().enumerate() {
        let share = *count as f64 / total as f64;
        let cy = 10.0 + i as f64 * row_h;
        let bw_cur = bar_w * (share / max_share);
        svg.push_str(&format!(
            "<text x=\"4\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"600\">{}</text>\n",
            cy + 14.0,
            escape_html(name)
        ));
        let bar_color = if i == 0 { "#C9A24B" } else { "#1F2D4D" };
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"14\" fill=\"{}\"/>\n",
            bar_x,
            cy + 4.0,
            bw_cur.max(0.5),
            bar_color
        ));
        // Round 1-K K-1: share は 0-1 比率前提 (SalesNow セグメント比率)。
        // 外部 API 改修で 0-100% 値や数値型が混入した場合、表示が 100 倍ずれる。
        debug_assert!(
            (0.0..=1.0).contains(&share),
            "salesnow segment share out of expected range (0-1): {} (already %?)",
            share
        );
        if !(0.0..=1.0).contains(&share) {
            tracing::warn!(
                target: "navy_report",
                share = share,
                "salesnow segment share out of expected range (expected 0-1); upstream unit change suspected"
            );
        }
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" fill=\"#0B1E3F\" font-family=\"Roboto Mono, monospace\" font-weight=\"700\" text-anchor=\"end\">{:.1}%</text>\n",
            w - 6.0,
            cy + 14.0,
            share * 100.0
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

// 規模 × 動向 6 マトリクス: 大企業 / 中小 / 零細 × 増員 / 減少
fn build_navy_growth_decline_matrix(
    seg: &super::super::super::super::company::fetch::RegionalCompanySegments,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>規模帯</th>");
    s.push_str("<th class=\"num\">増員傾向 (+5%超)</th>");
    s.push_str("<th class=\"num\">減少傾向 (-5%未満)</th>");
    s.push_str("<th>解釈</th>");
    s.push_str("</tr></thead>\n<tbody>\n");
    let rows = [
        (
            "大企業 (300+ 名)",
            seg.growth_large.len(),
            seg.decline_large.len(),
        ),
        (
            "中小企業 (50-299 名)",
            seg.growth_mid.len(),
            seg.decline_mid.len(),
        ),
        (
            "零細企業 (-49 名)",
            seg.growth_small.len(),
            seg.decline_small.len(),
        ),
    ];
    for (label, g, d) in rows {
        let (tag, interp) = if g > d && g >= 3 {
            ("pos", "純増基調")
        } else if d > g && d >= 3 {
            ("warn", "純減基調")
        } else if g + d == 0 {
            ("neu", "該当企業なし")
        } else {
            ("neu", "拮抗")
        };
        s.push_str(&format!(
            "<tr><td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span></td></tr>\n",
            label, g, d, tag, interp
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(
        "<p class=\"caption\">出典: 地域企業データ employee_delta_1y。\
                増員傾向 = +5% 超 / 減少傾向 = -5% 未満。\
                減少傾向は離職多発だけでなく組織改編・自然減・配置転換も含むため、\
                単純な離職率指標とは区別してください。</p>\n",
    );
    s
}

fn build_navy_company_list(
    companies: &[super::super::super::super::company::fetch::NearbyCompany],
    take: usize,
    show_hw: bool,
) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>企業名</th><th>産業</th>");
    s.push_str("<th class=\"num\">従業員数</th>");
    s.push_str("<th class=\"num\">1Y 増減</th>");
    if show_hw {
        s.push_str("<th class=\"num\">媒体掲載数</th>");
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let top: Vec<_> = companies.iter().take(take).collect();
    if top.is_empty() {
        let cols = if show_hw { 6 } else { 5 };
        s.push_str(&format!(
            "<tr><td colspan=\"{}\" class=\"dim\">該当企業データなし。</td></tr>\n",
            cols
        ));
    } else {
        for (i, c) in top.iter().enumerate() {
            // 2026-05-14: employee_delta_1y は DB に % 単位で格納 (5.0 = +5%)。
            // 旧コードは `delta * 100.0` で表示していたため +33.2 が +3320% と
            // 誤表示されていた (feedback_unit_consistency_audit / 表 5-B 信頼性
            // 指摘 2026-05-14 の真因)。フィルタ側 (fetch.rs <=300.0) は % 前提で
            // 正しく動作していたが、表示層だけが旧 ratio 前提のままだった。
            let delta = c.employee_delta_1y;
            let delta_tag = if delta >= 5.0 {
                "pos"
            } else if delta <= -5.0 {
                "warn"
            } else {
                "neu"
            };
            s.push_str(&format!(
                "<tr><td class=\"num bold\">{}</td><td><strong>{}</strong></td><td><span class=\"dim\">{}</span></td>\
                 <td class=\"num bold\">{}</td>\
                 <td class=\"num\"><span class=\"tag tag-{}\">{:+.1}%</span></td>",
                i + 1,
                escape_html(&c.company_name),
                escape_html(&c.sn_industry),
                format_number(c.employee_count),
                delta_tag,
                delta
            ));
            if show_hw {
                s.push_str(&format!(
                    "<td class=\"num\">{}</td>",
                    if c.hw_posting_count > 0 {
                        format_number(c.hw_posting_count)
                    } else {
                        "—".to_string()
                    }
                ));
            }
            s.push_str("</tr>\n");
        }
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">地域企業データ より、1 年人員増加率 +10% 超を「急成長」と定義。</p>\n");
    s
}

fn build_companies_so_what(
    industry_sorted: &[(String, i64)],
    industry_total: i64,
    pool_size: usize,
    n_growth: usize,
    n_hiring: usize,
    show_hw: bool,
) -> String {
    let top_industry = industry_sorted.first();
    let top_share = match top_industry {
        Some((_, c)) if industry_total > 0 => *c as f64 / industry_total as f64 * 100.0,
        _ => 0.0,
    };
    let top_name = top_industry.map(|(n, _)| n.as_str()).unwrap_or("—");

    let concentration = if top_share >= 25.0 {
        format!(
            "<strong>{}</strong> が <strong>{:.0}%</strong> を占める <strong>主産業依存型</strong> です。",
            top_name, top_share
        )
    } else if top_share >= 15.0 {
        format!(
            "<strong>{}</strong> 中心 (<strong>{:.0}%</strong>) ながら複数産業が並走する <strong>複合型</strong> 構造です。",
            top_name, top_share
        )
    } else if top_share > 0.0 {
        "産業が <strong>分散型</strong> に広がり、特定業界依存が低い構造です。".to_string()
    } else {
        "産業構成データが取得できなかったため、業種傾向は判定困難です。".to_string()
    };

    let growth_note = if n_growth >= 10 {
        format!(
            "急成長企業 <strong>{}</strong> 社が地域に存在し、人材移動が活発な可能性があります。",
            n_growth
        )
    } else if n_growth >= 3 {
        format!(
            "急成長企業 <strong>{}</strong> 社が確認でき、新規参入 / 採用強化中の競合として注視が必要です。",
            n_growth
        )
    } else {
        format!(
            "急成長セグメントは <strong>{}</strong> 社で、競合の人員拡大局面は限定的です。",
            n_growth
        )
    };

    let hw_note = if show_hw && n_hiring >= 5 {
        format!(
            " 媒体上で <strong>採用活発企業 {}</strong> 社が確認でき、競合との掲載重複度は高めです。応募導線・募集要項の差別化が必要です。",
            n_hiring
        )
    } else {
        String::new()
    };

    let pool_note = if pool_size == 0 {
        " (地域企業データが取得できなかったため、競合分析は限定的です)"
    } else {
        ""
    };

    format!("{} {}{}{}", concentration, growth_note, hw_note, pool_note)
}
