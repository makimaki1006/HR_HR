//! 分割: report_html/wage.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use std::collections::HashMap;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

/// 案 #8 (Impl-3): 世帯所得 vs CSV 給与競争力 比較
///
/// 配置: 最低賃金比較セクション（表 8-1）の直後に「図 8-2: 給与 vs 生活費 比較」として描画。
/// データ: `InsightContext.ext_household_spending` (v2_external_household_spending) から
///         category 別の monthly_amount を合算 → 月平均総支出（円/月）。
///         CSV 月給中央値（agg.salary_values の median）と比較し、比率を提示する。
///
/// 必須注記: 「世帯支出は 2 人以上世帯平均（家計調査）。単独世帯では生活費構造が異なる」
///
/// memory ルール:
/// - `feedback_correlation_not_causation.md`: 「応募抑制要因の可能性」は因果断定を避ける
/// - `feedback_hw_data_scope.md`: CSV と家計調査は別出典のため比較は参考値
pub(super) fn render_section_household_vs_salary(
    html: &mut String,
    agg: &SurveyAggregation,
    hw_context: Option<&super::super::super::insight::fetch::InsightContext>,
) {
    let ctx = match hw_context {
        Some(c) => c,
        None => return,
    };
    if ctx.ext_household_spending.is_empty() {
        return;
    }

    // CSV 月給中央値（is_hourly でない場合のみ意味を持つ）
    let csv_median: i64 = if agg.is_hourly {
        // 時給ベース → 167h で月換算（最低賃金比較セクションと同じ方針）
        if let Some(stats) = agg.enhanced_stats.as_ref() {
            stats.median * 167
        } else {
            0
        }
    } else {
        agg.enhanced_stats.as_ref().map(|s| s.median).unwrap_or(0)
    };

    if csv_median <= 0 {
        return;
    }

    // 世帯月平均総支出 = 「消費支出」カテゴリの値 (公式の総額)
    // 注: v2_external_household_spending は「消費支出」(親) と「食料/住居/光熱/...」
    //     (10 個のサブカテゴリ) を **両方** 保持している。サブカテゴリの合計 = 消費支出
    //     なので、全行 SUM すると 2 倍に二重計上される。
    //     (バグ修正 2026-04-27: 全行 SUM → 「消費支出」のみ抽出)
    use super::super::super::helpers::{get_f64, get_str_ref};
    let total_spending: i64 = ctx
        .ext_household_spending
        .iter()
        .find(|row| get_str_ref(row, "category") == "消費支出")
        .map(|row| get_f64(row, "monthly_amount") as i64)
        .unwrap_or_else(|| {
            // フォールバック: 「消費支出」が無ければサブカテゴリ合計
            // (10 サブカテゴリすべて、または部分集合の合計)
            ctx.ext_household_spending
                .iter()
                .filter(|row| get_str_ref(row, "category") != "消費支出")
                .map(|row| get_f64(row, "monthly_amount") as i64)
                .filter(|&v| v > 0)
                .sum()
        });

    if total_spending <= 0 {
        return;
    }

    // 比率 = 給与中央値 ÷ 月支出
    let ratio = csv_median as f64 / total_spending as f64;
    let ratio_pct = ratio * 100.0;
    let diff = csv_median - total_spending;

    // severity 判定: <90% は Critical (生活費未達), 90-100% は Warning, >=100% は Positive
    let sev = if ratio_pct < 90.0 {
        RptSev::Critical
    } else if ratio_pct < 100.0 {
        RptSev::Warning
    } else {
        RptSev::Positive
    };

    // テストの逆証明用に区切りコメント (HTML には影響しない)
    html.push_str("<!-- impl3-figure-8-2-household-vs-salary -->\n");
    html.push_str("<h3>給与中央値 vs 世帯月平均支出</h3>\n");
    render_figure_caption(
        html,
        "図 8-2",
        "CSV 月給中央値と世帯月平均支出の比較（生活費競争力）",
    );

    // 横バー比較
    let median_man = csv_median as f64 / 10_000.0;
    let spending_man = total_spending as f64 / 10_000.0;
    let max_v = median_man.max(spending_man).max(1.0);
    let median_w = (median_man / max_v * 100.0).clamp(0.0, 100.0);
    let spending_w = (spending_man / max_v * 100.0).clamp(0.0, 100.0);

    html.push_str(&format!(
        "<div style=\"margin:12px 0;font-size:10pt;\">\
         <div style=\"display:flex;align-items:center;gap:8px;margin:6px 0;\">\
            <span style=\"width:140px;color:#1f6feb;\">CSV 月給中央値</span>\
            <div style=\"flex:1;background:#f0f4f8;height:18px;position:relative;border-radius:3px;\">\
              <div style=\"background:#1f6feb;width:{:.1}%;height:100%;border-radius:3px;\"></div>\
            </div>\
            <span style=\"width:90px;text-align:right;font-weight:bold;\">{:.1} 万円</span>\
         </div>\
         <div style=\"display:flex;align-items:center;gap:8px;margin:6px 0;\">\
            <span style=\"width:140px;color:#7c3aed;\">世帯月平均支出</span>\
            <div style=\"flex:1;background:#f0f4f8;height:18px;position:relative;border-radius:3px;\">\
              <div style=\"background:#7c3aed;width:{:.1}%;height:100%;border-radius:3px;\"></div>\
            </div>\
            <span style=\"width:90px;text-align:right;font-weight:bold;\">{:.1} 万円</span>\
         </div>\
         </div>\n",
        median_w, median_man, spending_w, spending_man,
    ));

    // 比率 + severity badge
    html.push_str(&format!(
        "<p class=\"section-sowhat\" style=\"font-size:10.5pt;\">\
         {} <strong>給与/支出 比率: {:.0}%</strong> \
         （差額: {}円/月 = CSV 月給中央値 {:.1} 万円 - 世帯月平均支出 {:.1} 万円）\
         </p>\n",
        severity_badge(sev),
        ratio_pct,
        format_number(diff),
        median_man,
        spending_man,
    ));

    render_read_hint(
        html,
        "比率が 100% 未満の場合、当該地域の家計支出を CSV 月給中央値だけでは賄えない可能性があります。\
         応募抑制要因として観測される傾向があり、給与水準の見直し・各種手当の追加・住宅補助の検討材料となる場合があります。\
         ※ 比率と応募行動の関係は相関であり、因果関係を示すものではありません。",
    );

    // 必須注記
    // 2026-04-26 Granularity: 都道府県粒度のみであることを強調
    html.push_str(
        "<p class=\"note\" style=\"font-size:9pt;color:#b45309;background:#fef3c7;padding:6px 8px;border-left:3px solid #f59e0b;border-radius:3px;margin:6px 0;\">\
         <strong>⚠ 都道府県粒度の参考値:</strong> 世帯支出は 2 人以上世帯平均（家計調査、総務省統計局）。\
         本データは <strong>都道府県+政令市</strong> のみで、市区町村別の差は反映されていません。\
         単独世帯・3 人以上世帯では生活費構造が異なります。\
         CSV の主要市区町村が複数都道府県にまたがる場合、都道府県平均が必ずしも \
         実際の対象地域の生活コストを代表しないため、参考値としてご利用ください。\
         CSV 給与はアップロードされた媒体掲載値で、家計調査と直接比較する \
         ものではなく、市場内位置の参考としてご利用ください。\
         </p>\n",
    );
}

pub(super) fn render_section_min_wage(
    html: &mut String,
    agg: &SurveyAggregation,
    db: Option<&LocalDb>,
    turso: Option<&TursoDb>,
) {
    if agg.by_prefecture_salary.is_empty() {
        return;
    }

    // Round 8 P2-C (2026-05-10): DB 値優先 + ハードコード fallback。
    // `v2_external_minimum_wage` (Local 47 行 / Turso 同期済) から SELECT、
    // HashMap 化して per-prefecture lookup に使う。DB 接続不可 / 該当行なしの場合は
    // helpers.rs の `min_wage_for_prefecture` ハードコード版にフォールバック。
    let mut wage_map: HashMap<String, i64> = HashMap::new();
    if let Some(d) = db {
        let rows = super::super::super::analysis::fetch::query_turso_or_local(
            turso,
            d,
            "SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage",
            &[],
            "v2_external_minimum_wage",
        );
        for r in rows {
            if let (Some(serde_json::Value::String(p)), Some(serde_json::Value::Number(w))) =
                (r.get("prefecture"), r.get("hourly_min_wage"))
            {
                if let Some(v) = w.as_i64() {
                    wage_map.insert(p.clone(), v);
                }
            }
        }
    }

    // 都道府県ごとに最低賃金比較データを構築
    struct MinWageEntry {
        name: String,
        avg_min: i64,
        min_wage: i64,
        hourly_equiv: i64, // 月給÷167h (HOURLY_TO_MONTHLY_HOURS 経由、Round 9 P2-H で命名統一)
        diff_min_wage: i64,
        ratio_min_wage: f64,
    }
    let mut entries: Vec<MinWageEntry> = agg
        .by_prefecture_salary
        .iter()
        .filter_map(|p| {
            let mw = wage_map
                .get(&p.name)
                .copied()
                .or_else(|| min_wage_for_prefecture(&p.name))?;
            if p.avg_min_salary <= 0 {
                return None;
            }
            // 2026-05-08 Round 2-2: PDF2 で出た「167h 換算 11 円 / 6 円」事故は、
            // avg_min_salary が時給値 (1,000 円台) で入っていたため発生した。
            // 月給下限平均が 50,000 円未満は時給混入の疑いが濃いため除外する。
            if !super::salary_summary::is_plausible_monthly_min_salary(p.avg_min_salary) {
                return None;
            }
            let hourly_equiv = p.avg_min_salary / super::super::aggregator::HOURLY_TO_MONTHLY_HOURS;
            let diff_min_wage = hourly_equiv - mw;
            let ratio_min_wage = hourly_equiv as f64 / mw as f64;
            Some(MinWageEntry {
                name: p.name.clone(),
                avg_min: p.avg_min_salary,
                min_wage: mw,
                hourly_equiv,
                diff_min_wage,
                ratio_min_wage,
            })
        })
        .collect();

    if entries.is_empty() {
        return;
    }
    entries.sort_by(|a, b| a.diff_min_wage.cmp(&b.diff_min_wage)); // 差が小さい順

    // 全体の平均比率
    let avg_ratio: f64 = entries.iter().map(|e| e.ratio_min_wage).sum::<f64>() / entries.len() as f64;
    let avg_diff_pct = (avg_ratio - 1.0) * 100.0;

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>最低賃金比較</h2>\n");
    // So What + severity badge（diff < 0 は Critical、< 50 は Warning、それ以外 Positive）
    let below_count = entries.iter().filter(|e| e.diff_min_wage < 0).count();
    let near_count = entries
        .iter()
        .filter(|e| e.diff_min_wage >= 0 && e.diff_min_wage < 50)
        .count();
    let sev = if below_count > 0 {
        RptSev::Critical
    } else if near_count > 0 {
        RptSev::Warning
    } else {
        RptSev::Positive
    };
    html.push_str(&format!(
        "<p class=\"section-sowhat\">{} {} 県で平均下限給与の 167h 換算が最低賃金を下回る傾向。\
         差が 50 円未満（要確認）: {} 県。該当求人群は労基上要確認。</p>\n",
        severity_badge(sev),
        below_count,
        near_count
    ));
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>月給を167h（8h×20.875日、厚労省基準）で割り時給換算して最低賃金と比較。\
        全国加重平均: <strong>1,121円</strong>（2025年10月施行）\
    </p>\n",
    );

    // 概要カード
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均最低賃金比率", &format!("{:.2}倍", avg_ratio));
    render_stat_box(html, "全体差分", &format!("{:+.1}%", avg_diff_pct));
    render_stat_box(html, "分析対象", &format!("{}都道府県", entries.len()));
    html.push_str("</div>\n");

    // 最低賃金との差が小さい都道府県 10 件（差額の小さい順に整理、ソート可能テーブル）
    html.push_str("<h3>時給換算で最低賃金に近い都道府県 10 件（差額の小さい順）</h3>\n");
    render_figure_caption(
        html,
        "表 8-1",
        "時給換算 vs 最低賃金 差額 Top 10（差小→大）",
    );

    // 差額のレンジ（バー幅計算用）
    let max_abs_diff = entries
        .iter()
        .map(|e| e.diff_min_wage.abs())
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">平均月給下限</th>\
        <th style=\"text-align:right\">167h換算</th><th style=\"text-align:right\">最低賃金</th>\
        <th style=\"text-align:right\">差額</th><th>差額バー</th><th style=\"text-align:right\">比率</th></tr></thead>\n<tbody>\n");
    for (i, e) in entries.iter().take(10).enumerate() {
        let diff_color = if e.diff_min_wage < 0 {
            "negative"
        } else if e.diff_min_wage < 50 {
            "color:#fb8c00;font-weight:bold"
        } else {
            ""
        };
        let diff_style = if diff_color.starts_with("color:") {
            format!(" style=\"text-align:right;{}\"", diff_color)
        } else {
            format!(" class=\"num {}\"", diff_color)
        };
        // 差額バー（負=赤、近接<50=橙、それ以外=緑）
        let bar_cls = if e.diff_min_wage < 0 {
            "below"
        } else if e.diff_min_wage < 50 {
            "near"
        } else {
            ""
        };
        let fill_pct = (e.diff_min_wage.abs() as f64 / max_abs_diff * 100.0).clamp(0.0, 100.0);
        let fill_left = if e.diff_min_wage < 0 {
            (50.0 - fill_pct / 2.0).clamp(0.0, 50.0)
        } else {
            50.0
        };
        let fill_w = fill_pct / 2.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}円</td>\
             <td{}>{:+}円</td>\
             <td><div class=\"minwage-diff-bar\" aria-label=\"差額\">\
                <div class=\"mwd-fill {}\" style=\"left:{:.1}%;width:{:.1}%;\"></div>\
                <div class=\"mwd-baseline\" style=\"left:50%;\"></div>\
             </div></td>\
             <td class=\"num\">{:.2}倍</td></tr>\n",
            i + 1,
            escape_html(&e.name),
            format_man_yen(e.avg_min),
            format_number(e.hourly_equiv),
            format_number(e.min_wage),
            diff_style,
            e.diff_min_wage,
            bar_cls,
            fill_left,
            fill_w,
            e.ratio_min_wage,
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "差額バーは中央線（最低賃金）からの乖離。左に伸びる赤バー=最低賃金未満、橙=50円未満で近接、緑=十分な余裕がある状態。\
         赤・橙は労務上の確認推奨です（167h は厚労省標準・端数労働日数の調整は別途要検討）。",
    );

    // 活用ポイント（feedback_correlation_not_causation.md 準拠: 因果断定を避け「傾向」「観測」で表現）
    // 2026-04-26 Granularity: 最低賃金は法定上 47 県粒度のみ
    // Round 9 P2-D' (2026-05-10): 「業界横断比較ではない」を明記 (Agent D' 推奨案 B)
    html.push_str(
        "<div class=\"note\">\
        <strong>活用ポイント:</strong> 167h=所定労働時間（8h×20.875日、厚労省「就業条件総合調査 2024」基準）で換算。\
        最低賃金水準の求人は応募者が集まりにくい傾向が観測されます。\
        +10% 以上の求人は地域内で目立つ存在感を持つ傾向があり、応募状況や採用実績に応じて検討材料の 1 つになる可能性があります。\
        ※ 給与水準と応募状況の関係は相関であり、因果関係を示すものではありません。<br/>\
        <strong>本指標の範囲:</strong> 最賃比は対象地域の最低賃金との距離を示す指標であり、\
        <strong>業界横断比較・職種横断比較の指標ではありません</strong>。CSV 給与中央値は媒体掲載求人の混合値で、\
        業界別・職種別の中央値とは粒度が異なります。業界別最賃比中央値は本レポートでは作成していません \
        (CSV に業界列がなく、推定で断定するのは採用判断として不適切なため)。\
    </div>\n",
    );
    html.push_str(
        "<p class=\"note\" style=\"font-size:9pt;color:#b45309;background:#fef3c7;padding:6px 8px;border-left:3px solid #f59e0b;border-radius:3px;margin:6px 0;\">\
        <strong>⚠ 都道府県粒度の参考値:</strong> 最低賃金は法定で都道府県（47 県）単位のみ。\
        市区町村別の差はありません（同一都道府県内では最低賃金は同一）。\
        本表は給与水準が法定下限を満たすかの確認用であり、市区町村別の競争力比較には市区町村粒度の \
        他指標（CSV 給与中央値・人材プール等）も併用してください。\
        </p>\n",
    );

    html.push_str("</div>\n");
}

pub(super) fn render_section_company(html: &mut String, by_company: &[CompanyAgg]) {
    if by_company.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>企業分析</h2>\n");

    // So What 行: 件数の多い法人と給与水準の傾向を 1 行で
    if let Some(top) = by_company.iter().max_by_key(|c| c.count) {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 掲載件数が最も多い法人は「{}」（{} 件、平均月給 {}）。\
             件数・給与の分布は以下のテーブルを参照（ソート可能）。</p>\n",
            escape_html(&top.name),
            format_number(top.count as i64),
            escape_html(&format_man_yen(top.avg_salary))
        ));
    }

    // 企業数サマリー
    html.push_str(&format!(
        "<p>分析対象企業数: <strong>{}</strong>社（給与情報のある求人を持つ企業のみ）</p>\n",
        format_number(by_company.len() as i64)
    ));

    // 市場集中度（HHI: Herfindahl-Hirschman Index）の計算と表示
    // HHI = Σ(各企業の求人シェア%)² / 公正取引委員会基準:
    //   < 1500: 分散型市場 / 1500-2500: 中程度集中 / > 2500: 集中型市場
    // サンプル数不足（企業数<3）時は非表示
    if by_company.len() >= 3 {
        let total_count: i64 = by_company.iter().map(|c| c.count as i64).sum();
        if total_count > 0 {
            let hhi: f64 = by_company
                .iter()
                .map(|c| {
                    let share_pct = c.count as f64 / total_count as f64 * 100.0;
                    share_pct * share_pct
                })
                .sum();
            let (judgment, color) = if hhi < 1500.0 {
                ("分散型市場（競合多数・多様な選択肢）", "var(--c-success)")
            } else if hhi < 2500.0 {
                ("中程度集中（主要プレイヤー複数）", "var(--c-warning)")
            } else {
                ("集中型市場（少数企業が支配的）", "var(--c-danger)")
            };
            html.push_str(&format!(
                "<p style=\"margin:8px 0;font-size:10pt;\">\
                 <strong>市場集中度（HHI）: <span style=\"color:{}\">{:.0}</span></strong> \
                 / 判定: <span style=\"color:{}\">{}</span> \
                 <span style=\"font-size:9pt;color:#888;\">（公正取引委員会基準: &lt;1500=分散 / 1500-2500=中程度 / &gt;2500=集中）</span>\
                 </p>\n",
                color, hhi, color, judgment
            ));
        }
    }

    // 掲載件数の多い法人 15 件（件数の多い順に整理、ソート可能テーブル）
    let mut by_count = by_company.to_vec();
    by_count.sort_by(|a, b| b.count.cmp(&a.count));

    html.push_str("<h3>掲載件数の多い法人 15 件（給与情報あり）</h3>\n");
    render_figure_caption(
        html,
        "表 9-1",
        "掲載件数の多い法人 Top 15（件数 + 平均月給 2 軸）",
    );

    // 件数バー + 平均月給ドットの 2 軸表示用に最大値計算
    let max_count = by_count
        .iter()
        .take(15)
        .map(|c| c.count)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let max_salary = by_count
        .iter()
        .take(15)
        .map(|c| c.avg_salary)
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">給与付き求人数</th><th>件数バー</th><th style=\"text-align:right\">平均月給</th></tr></thead>\n<tbody>\n");
    for (i, c) in by_count.iter().take(15).enumerate() {
        let count_pct = (c.count as f64 / max_count * 100.0).clamp(0.0, 100.0);
        let salary_pct = (c.avg_salary as f64 / max_salary * 100.0).clamp(0.0, 100.0);
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
             <td><div class=\"minwage-diff-bar\" aria-label=\"件数比\" style=\"max-width:140px;\">\
               <div class=\"mwd-fill\" style=\"left:0;width:{:.1}%;background:var(--c-primary-light);\"></div>\
               <div class=\"mwd-baseline\" style=\"left:{:.1}%;background:var(--c-warning);\" title=\"平均月給比\"></div>\
             </div></td>\
             <td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&c.name),
            format_number(c.count as i64),
            count_pct,
            salary_pct,
            format_man_yen(c.avg_salary),
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "青バー = 件数比、橙の縦線 = 平均月給比（いずれも最大値 100% 基準）。\
         件数バーが長く橙線が右寄りなら「規模も給与も高い法人」、件数バーが長く橙線が左寄りなら\
         「件数は多いが給与が抑えめ」の傾向（採用ボリューム重視の可能性）です。",
    );

    // 平均給与の多い法人 15 件（サンプル数に応じて閾値動的調整）
    let multi_count = by_company.iter().filter(|c| c.count >= 2).count();
    let min_count_threshold = if multi_count >= 15 { 2 } else { 1 };
    let mut by_salary: Vec<&CompanyAgg> = by_company
        .iter()
        .filter(|c| c.count >= min_count_threshold && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        let title = if min_count_threshold >= 2 {
            "給与水準の高い法人 15 件（給与付き2件以上の企業）"
        } else {
            "給与水準の高い法人 15 件（給与付き、1件求人含む。※1件は参考値）"
        };
        html.push_str(&format!("<h3>{}</h3>\n", title));
        render_figure_caption(html, "表 9-2", "給与水準の高い法人 Top 15");
        html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">給与付き求人数</th></tr></thead>\n<tbody>\n");
        for (i, c) in by_salary.iter().take(15).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(&c.name),
                format_man_yen(c.avg_salary),
                format_number(c.count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}

pub(super) fn render_section_tag_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_tag_salary.is_empty() && agg.by_tags.is_empty() {
        return;
    }

    let overall_mean = agg.enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>タグ×給与相関分析</h2>\n");
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各タグが付いた求人の平均給与と、全体平均との差を示します。\
        正の値（緑）=そのタグが付くと給与が高い傾向、負の値（赤）=低い傾向。\
    </p>\n",
    );

    html.push_str(&format!(
        "<p>全体平均月給: <strong>{}</strong></p>\n",
        format_man_yen(overall_mean)
    ));

    // タグ件数のツリーマップ（テーブルの上に配置）
    if !agg.by_tag_salary.is_empty() {
        render_figure_caption(html, "図 10-1", "訴求タグ件数 ツリーマップ（面積=件数）");
        let tree_data: Vec<serde_json::Value> = agg
            .by_tag_salary
            .iter()
            .map(|t| json!({"name": &t.tag, "value": t.count}))
            .collect();
        let config = json!({
            "tooltip": {"formatter": "{b}: {c}件"},
            "series": [{
                "type": "treemap",
                "data": tree_data,
                "roam": false,
                "label": {"show": true, "formatter": "{b}\n{c}件", "fontSize": 10},
                "breadcrumb": {"show": false},
                "levels": [{"colorSaturation": [0.3, 0.7]}]
            }]
        });
        html.push_str(&render_echart_div(&config.to_string(), 250));
        render_read_hint(
            html,
            "面積が大きいタグほど多く付与されています。下のテーブルでは「件数 10 件以上 + 全体比 ±2% 以上」のタグに絞り、\
             給与水準との関連を示しています（相関であり因果関係ではありません）。",
        );
    }

    if !agg.by_tag_salary.is_empty() {
        // 有意タグのフィルタリング:
        // 1. 出現率50%超のタグは共通属性として除外（全求人の半数以上に付く「交通費支給」等は差分がゼロに収束）
        // 2. 差分 |diff_percent| >= 2% のタグのみハイライト（それ未満は参考扱い）
        let total_records = agg.total_count as f64;
        let significant: Vec<&TagSalaryAgg> = agg
            .by_tag_salary
            .iter()
            .filter(|t| {
                let frequency = t.count as f64 / total_records;
                frequency < 0.5 && t.diff_percent.abs() >= 2.0
            })
            .collect();
        let display_tags: Vec<&TagSalaryAgg> = if significant.is_empty() {
            // フォールバック: 有意なタグがない場合は全タグを表示
            agg.by_tag_salary.iter().collect()
        } else {
            significant
        };
        if agg.by_tag_salary.len() > display_tags.len() {
            html.push_str(&format!(
                "<p class=\"note\" style=\"font-size:9pt;color:#888;\">※{}タグから{}タグに絞り込み表示中（出現率50%超の共通タグと差分±2%未満を除外）</p>\n",
                agg.by_tag_salary.len(), display_tags.len()
            ));
        }
        // タグ別給与差分テーブル（ソート可能・完全版）
        render_figure_caption(
            html,
            "表 10-1",
            "タグ別 給与差分（全体比、件数 10+、|差分| 2% 以上）",
        );
        html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th>\
            <th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">全体比</th></tr></thead>\n<tbody>\n");
        for (i, ts) in display_tags.iter().enumerate() {
            let diff_class = if ts.diff_from_avg > 0 {
                "positive"
            } else if ts.diff_from_avg < 0 {
                "negative"
            } else {
                ""
            };
            let diff_sign = if ts.diff_from_avg > 0 { "+" } else { "" };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num {diff_class}\">{sign}{diff}万円 ({sign}{pct:.1}%)</td></tr>\n",
                i + 1,
                escape_html(&ts.tag),
                format_number(ts.count as i64),
                format_man_yen(ts.avg_salary),
                diff = format!("{:.1}", ts.diff_from_avg as f64 / 10_000.0),
                sign = diff_sign,
                pct = ts.diff_percent,
            ));
        }
        html.push_str("</tbody></table>\n");
    } else {
        // フォールバック: 件数のみテーブル（ソート可能）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th></tr></thead>\n<tbody>\n");
        for (i, (tag, count)) in agg.by_tags.iter().take(20).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(tag),
                format_number(*count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}

// =====================================================================
// Impl-3 案 #8 逆証明テスト: 世帯所得 vs CSV 給与
//
// `feedback_reverse_proof_tests.md` 準拠で「セクション存在」だけでなく
// 「具体値（比率, 差額）が画面に出る」「必須注記文言が含まれる」を検証。
// =====================================================================
#[cfg(test)]
mod household_vs_salary_tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn make_row(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    fn ctx_with_spending(
        rows: Vec<HashMap<String, serde_json::Value>>,
    ) -> super::super::super::super::insight::fetch::InsightContext {
        use super::super::super::super::insight::fetch::InsightContext;
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
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: rows,
            ext_climate: vec![],
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            pref_avg_unemployment_rate: None,
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
            muni: String::new(),
        }
    }

    fn agg_with_median(median: i64) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = 100;
        agg.is_hourly = false;
        agg.enhanced_stats = Some(super::super::super::statistics::EnhancedStats {
            count: 100,
            mean: median,
            median,
            min: median,
            max: median,
            std_dev: 0,
            bootstrap_ci: None,
            trimmed_mean: None,
            quartiles: None,
            reliability: "low".to_string(),
        });
        agg
    }

    /// hw_context = None で section 非出力
    #[test]
    fn test_household_vs_salary_skipped_when_no_context() {
        let mut html = String::new();
        let agg = agg_with_median(250_000);
        render_section_household_vs_salary(&mut html, &agg, None);
        assert!(html.is_empty(), "hw_context=None で section 非出力");
    }

    /// ext_household_spending 空で section 非出力
    #[test]
    fn test_household_vs_salary_skipped_when_no_spending() {
        let mut html = String::new();
        let agg = agg_with_median(250_000);
        let ctx = ctx_with_spending(vec![]);
        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));
        assert!(
            html.is_empty(),
            "ext_household_spending=空 で section 非出力"
        );
    }

    /// 給与中央値 25万 vs 世帯支出 28万 → 比率 89% (Critical) + 必須注記
    #[test]
    fn test_household_vs_salary_critical_ratio_89pct() {
        let mut html = String::new();
        let agg = agg_with_median(250_000); // 月給中央値 25 万

        // 世帯月平均支出 28 万 = 280,000 円 (1 カテゴリで集約)
        let rows = vec![make_row(&[
            ("prefecture", json!("東京都")),
            ("category", json!("総支出")),
            ("monthly_amount", json!(280_000.0)),
            ("reference_year", json!(2023)),
        ])];
        let ctx = ctx_with_spending(rows);

        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));

        // セクション
        assert!(html.contains("給与中央値 vs 世帯月平均支出"), "h3 タイトル");
        assert!(html.contains("図 8-2"), "図番号");

        // 比率: 25 / 28 = 0.892857... → 89%
        assert!(
            html.contains("給与/支出 比率: 89%"),
            "比率 89% が画面に出る"
        );

        // 具体値 (横バーラベルで表示)
        assert!(html.contains("25.0 万円"), "CSV 月給中央値 25.0 万円");
        assert!(html.contains("28.0 万円"), "世帯月平均支出 28.0 万円");

        // Critical severity badge (<90%)
        assert!(
            html.contains("\u{25B2}\u{25B2} 重大"),
            "Critical severity badge"
        );

        // 必須注記
        assert!(
            html.contains("世帯支出は 2 人以上世帯平均"),
            "必須注記: 2 人以上世帯平均"
        );
        assert!(
            html.contains("単独世帯") && html.contains("生活費構造が異なります"),
            "必須注記: 単独世帯では生活費構造が異なる"
        );

        // 相関注記
        assert!(
            html.contains("因果関係を示すものではありません"),
            "相関≠因果の注記"
        );
    }

    /// 比率 100% 以上 → Positive
    #[test]
    fn test_household_vs_salary_positive_when_salary_above_spending() {
        let mut html = String::new();
        let agg = agg_with_median(350_000); // 35 万

        let rows = vec![make_row(&[
            ("prefecture", json!("東京都")),
            ("category", json!("総支出")),
            ("monthly_amount", json!(280_000.0)),
            ("reference_year", json!(2023)),
        ])];
        let ctx = ctx_with_spending(rows);

        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));

        // 35 / 28 = 125%
        assert!(html.contains("給与/支出 比率: 125%"), "比率 125%");
        // Positive badge
        assert!(
            html.contains("\u{25EF} 良好"),
            "Positive severity badge (>=100%)"
        );
    }

    /// 逆証明: 「消費支出」(親) とサブカテゴリ (子) の二重計上を防ぐ
    /// e-Stat 家計調査の v2_external_household_spending は親「消費支出」と
    /// 10 個のサブカテゴリを両方保持するため、全行 SUM すると 2 倍になる。
    /// 「消費支出」が存在する場合はそれを優先採用する。
    #[test]
    fn test_household_vs_salary_uses_total_when_present_no_double_count() {
        let mut html = String::new();
        let agg = agg_with_median(250_000); // 25 万

        // 「消費支出」(親) = 28万 + サブカテゴリ合計 28万 が入っている状態
        let rows = vec![
            make_row(&[
                ("category", json!("消費支出")),
                ("monthly_amount", json!(280_000.0)),
            ]),
            make_row(&[
                ("category", json!("食料")),
                ("monthly_amount", json!(80_000.0)),
            ]),
            make_row(&[
                ("category", json!("住居")),
                ("monthly_amount", json!(50_000.0)),
            ]),
            make_row(&[
                ("category", json!("光熱・水道")),
                ("monthly_amount", json!(30_000.0)),
            ]),
            make_row(&[
                ("category", json!("交通・通信")),
                ("monthly_amount", json!(40_000.0)),
            ]),
            make_row(&[
                ("category", json!("その他の消費支出")),
                ("monthly_amount", json!(80_000.0)),
            ]),
        ];
        let ctx = ctx_with_spending(rows);
        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));

        // 「消費支出」のみを採用するため total_spending = 28 万、ratio = 25/28 = 89%
        // (二重計上していたら total = 56 万、ratio = 25/56 = 45% という不正な値になる)
        assert!(
            html.contains("給与/支出 比率: 89%"),
            "「消費支出」(親) を優先採用、サブカテゴリは合算しない (二重計上防止)"
        );
        // ドメイン不変条件: 比率は物理的にあり得る範囲 (50% 未満は二重計上のシグナル)
        assert!(
            !html.contains("給与/支出 比率: 45%"),
            "比率 45% は 消費支出+サブ合計 の二重計上の証拠 (バグ)"
        );
    }

    /// 多カテゴリの monthly_amount が合算される (逆証明: SUM ロジック)
    #[test]
    fn test_household_vs_salary_sums_categories() {
        let mut html = String::new();
        let agg = agg_with_median(250_000); // 25 万

        // 食料 8万 + 住居 5万 + 光熱 3万 + 交通 4万 + その他 8万 = 28 万
        let rows = vec![
            make_row(&[
                ("category", json!("食料")),
                ("monthly_amount", json!(80_000.0)),
            ]),
            make_row(&[
                ("category", json!("住居")),
                ("monthly_amount", json!(50_000.0)),
            ]),
            make_row(&[
                ("category", json!("光熱・水道")),
                ("monthly_amount", json!(30_000.0)),
            ]),
            make_row(&[
                ("category", json!("交通・通信")),
                ("monthly_amount", json!(40_000.0)),
            ]),
            make_row(&[
                ("category", json!("その他")),
                ("monthly_amount", json!(80_000.0)),
            ]),
        ];
        let ctx = ctx_with_spending(rows);

        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));

        // SUM = 28 万 → 25/28 = 89%
        assert!(
            html.contains("給与/支出 比率: 89%"),
            "多カテゴリ集計 SUM=28万 で比率 89%"
        );
        assert!(html.contains("28.0 万円"), "合算後 28.0 万円表示");
    }

    /// 2026-04-26 Granularity: 世帯支出 (#8) の都道府県粒度警告が強化されている
    #[test]
    fn granularity_household_spending_pref_only_warning_strengthened() {
        let agg = agg_with_median(250_000);
        let mut html = String::new();
        let rows = vec![make_row(&[
            ("prefecture", json!("東京都")),
            ("category", json!("食料")),
            ("monthly_amount", json!(80_000.0)),
        ])];
        let ctx = ctx_with_spending(rows);
        render_section_household_vs_salary(&mut html, &agg, Some(&ctx));

        assert!(
            html.contains("都道府県粒度の参考値"),
            "wage 世帯支出: 都道府県粒度の警告強化必須"
        );
        assert!(
            html.contains("市区町村別の差は反映されていません"),
            "wage 世帯支出: 市区町村別差注記必須"
        );
    }
}
