//! 分割: report_html/executive_summary.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;


/// 仕様書 3章: 5 KPI + 推奨優先アクション 3 件 + スコープ注意 2 行
/// 1 ページ完結、表紙直後に配置。アクションは severity 高い順に上から最大 3 件。
pub(super) fn render_section_executive_summary(
    html: &mut String,
    agg: &SurveyAggregation,
    _seeker: &JobSeekerAnalysis,
    _by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
) {
    html.push_str("<section class=\"section exec-summary\" role=\"region\" aria-labelledby=\"exec-sum-title\">\n");
    html.push_str("<h2 id=\"exec-sum-title\">Executive Summary</h2>\n");
    html.push_str(&format!(
        "<p class=\"section-header-meta\">対象: {} / 3分間で読み切れる全体要旨</p>\n",
        escape_html(&compose_target_region(agg))
    ));

    // ---- 5 KPI ----
    // 仕様書 3.3 の定義に厳密に従う
    // K1: サンプル件数
    let k1_value = format_number(agg.total_count as i64);
    // K2: 主要地域
    let k2_value = compose_target_region(agg);
    // K3: 主要雇用形態（件数最多）
    let k3_value: String = if let Some((name, count)) = agg.by_employment_type.first() {
        let pct = if agg.total_count > 0 {
            *count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        format!("{} ({:.0}%)", name, pct)
    } else {
        "-".to_string()
    };
    // K4: 給与中央値（雇用形態グループ別のネイティブ単位を優先）
    // 2026-04-24 Phase 2: 正社員 月給 / パート 時給 を並列表示して
    //   「月給/時給の単位が混ざって直感と合わない」問題を解消
    let k4_value = {
        let mut parts: Vec<String> = Vec::new();
        for g in &agg.by_emp_group_native {
            if g.count == 0 {
                continue;
            }
            let v_str = if g.native_unit == "時給" {
                format!("{}円", format_number(g.median))
            } else {
                format!("{:.1}万円", g.median as f64 / 10_000.0)
            };
            parts.push(format!(
                "{} ({}): {} (n={})",
                g.group_label, g.native_unit, v_str, g.count
            ));
        }
        if parts.is_empty() {
            match &agg.enhanced_stats {
                Some(s) if s.count > 0 => {
                    if agg.is_hourly {
                        format!("時給 {} 円", format_number(s.median))
                    } else {
                        format!("月給 {} 円", format_number(s.median))
                    }
                }
                _ => "算出不能 (サンプル不足)".to_string(),
            }
        } else {
            parts.join(" / ")
        }
    };
    // K5: 新着比率
    let k5_value = if agg.total_count > 0 && agg.new_count > 0 {
        format!(
            "{:.1}%",
            agg.new_count as f64 / agg.total_count as f64 * 100.0
        )
    } else if agg.total_count == 0 {
        "-".to_string()
    } else {
        "0.0%".to_string()
    };

    html.push_str("<div class=\"exec-kpi-grid\">\n");
    render_kpi_card(html, "サンプル件数", &k1_value, "件");
    render_kpi_card(html, "主要地域", &k2_value, "");
    render_kpi_card(html, "主要雇用形態", &k3_value, "");
    render_kpi_card(html, "給与中央値", &k4_value, "");
    render_kpi_card(html, "新着比率", &k5_value, "");
    html.push_str("</div>\n");

    // ---- 推奨優先アクション 3 件 ----
    html.push_str("<h3>推奨優先アクション候補（件数・差分条件を満たすもの）</h3>\n");
    let actions = build_exec_actions(agg, by_emp_type_salary, hw_context);
    if actions.is_empty() {
        html.push_str(
            "<div class=\"exec-summary-action\"><div class=\"action-body\">\
            現時点では該当条件を満たすアクション候補はありません。\
            各セクションの詳細を順にご確認ください。</div></div>\n",
        );
    } else {
        html.push_str("<div class=\"exec-action-list\">\n");
        for (idx, (sev, title, body, xref)) in actions.iter().enumerate() {
            html.push_str("<div class=\"exec-summary-action\">\n");
            html.push_str("<div class=\"action-head\">");
            html.push_str(&severity_badge(*sev));
            html.push_str(&format!(
                " <span>{}. {}</span>",
                idx + 1,
                escape_html(title)
            ));
            html.push_str("</div>\n");
            html.push_str(&format!(
                "<div class=\"action-body\" contenteditable=\"true\" spellcheck=\"false\">{}</div>\n",
                escape_html(body)
            ));
            html.push_str(&format!(
                "<div class=\"action-xref\">{}</div>\n",
                escape_html(xref)
            ));
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n");
    }

    // ---- スコープ注意書き (必須 / 仕様書 3.5) ----
    // 2026-04-24 修正: CSV は Indeed/求人ボックス等の媒体由来なので「HW 掲載求人のみ」
    // 表現は誤り。CSV 側と HW 側それぞれのスコープを明示。
    let outlier_note = if agg.outliers_removed_total > 0 {
        format!(
            "<br>\u{203B} 給与統計は IQR 法（Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR）で外れ値 {} 件を除外した後の値です（除外前 {} 件、除外後 {} 件）。\
            雇用形態グループ別集計も各グループ内で同手法の外れ値除外を適用済。",
            agg.outliers_removed_total,
            agg.salary_values_raw_count,
            agg.salary_values_raw_count.saturating_sub(agg.outliers_removed_total),
        )
    } else {
        "<br>\u{203B} 給与統計は IQR 法（Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR）で外れ値除外を適用済（除外対象なし）。".to_string()
    };

    html.push_str(&format!(
        "<div class=\"exec-scope-note\">\
        \u{203B} 本レポートはアップロード CSV（媒体: Indeed / 求人ボックス等）の分析が主で、\
        HW データは比較参考値として併記しています。CSV はスクレイピング範囲に依存し、\
        HW は掲載求人に限定されるため、どちらも全求人市場の代表ではありません。<br>\
        \u{203B} 示唆は相関に基づく仮説であり、因果を証明するものではない。\
        実施判断は現場文脈に依存します。{}\
        </div>\n",
        outlier_note
    ));

    html.push_str("</section>\n");
}

/// Executive Summary の 3 件アクションを算出（severity 降順、最大3件）
/// 仕様書 3.4 の閾値と文言テンプレートに従う
pub(super) fn build_exec_actions(
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
) -> Vec<(RptSev, String, String, String)> {
    let mut out: Vec<(RptSev, String, String, String)> = Vec::new();

    // A: 給与ギャップ（当サンプル中央値 vs HW 市場中央値）
    // 月給データのときのみ有効（is_hourly 時はスキップ）
    if !agg.is_hourly {
        let csv_median = agg.enhanced_stats.as_ref().map(|s| s.median).unwrap_or(0);
        let hw_median: i64 = if let Some(ctx) = hw_context {
            // ts_salary の avg_salary_min 値を平均化して参考値に
            let vals: Vec<f64> = ctx
                .ts_salary
                .iter()
                .map(|r| get_f64(r, "avg_salary_min"))
                .filter(|&v| v > 0.0)
                .collect();
            if !vals.is_empty() {
                (vals.iter().sum::<f64>() / vals.len() as f64) as i64
            } else {
                0
            }
        } else {
            0
        };
        if csv_median > 0 && hw_median > 0 {
            let diff = hw_median - csv_median;
            let abs_diff = diff.abs();
            if abs_diff >= 20_000 {
                let direction = if diff > 0 {
                    "引き上げる"
                } else {
                    "再確認する"
                };
                out.push((
                    RptSev::Critical,
                    format!(
                        "給与下限を月 {:+.1} 万円 {} 候補",
                        diff as f64 / 10_000.0,
                        direction
                    ),
                    format!(
                        "当サンプル中央値 {:.1} 万円 / 該当市区町村 HW 中央値 {:.1} 万円で {:.1} 万円差。",
                        csv_median as f64 / 10_000.0,
                        hw_median as f64 / 10_000.0,
                        abs_diff as f64 / 10_000.0
                    ),
                    "(Section 6 / Section 8 参照)".to_string(),
                ));
            } else if abs_diff >= 10_000 {
                let direction = if diff > 0 {
                    "引き上げる"
                } else {
                    "再確認する"
                };
                out.push((
                    RptSev::Warning,
                    format!(
                        "給与下限を月 {:+.1} 万円 {} 候補",
                        diff as f64 / 10_000.0,
                        direction
                    ),
                    format!(
                        "当サンプル中央値 {:.1} 万円 / 該当市区町村 HW 中央値 {:.1} 万円で {:.1} 万円差。",
                        csv_median as f64 / 10_000.0,
                        hw_median as f64 / 10_000.0,
                        abs_diff as f64 / 10_000.0
                    ),
                    "(Section 6 / Section 8 参照)".to_string(),
                ));
            }
        }
    }

    // B: 雇用形態構成差（正社員構成比 vs HW）
    if let Some(ctx) = hw_context {
        // CSV 側: 正社員(正職員含む)構成比
        let total_emp: usize = by_emp_type_salary.iter().map(|e| e.count).sum();
        let fulltime_count: usize = by_emp_type_salary
            .iter()
            .filter(|e| e.emp_type.contains("正社員") || e.emp_type.contains("正職員"))
            .map(|e| e.count)
            .sum();
        let csv_rate = if total_emp > 0 {
            fulltime_count as f64 / total_emp as f64 * 100.0
        } else {
            -1.0
        };
        // HW 側
        let hw_total: f64 = ctx.vacancy.iter().map(|r| get_f64(r, "total_count")).sum();
        let hw_ft: f64 = ctx
            .vacancy
            .iter()
            .filter(|r| super::super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
            .map(|r| get_f64(r, "total_count"))
            .sum();
        let hw_rate = if hw_total > 0.0 {
            hw_ft / hw_total * 100.0
        } else {
            -1.0
        };
        if csv_rate >= 0.0 && hw_rate >= 0.0 {
            let diff = (csv_rate - hw_rate).abs();
            if diff >= 15.0 {
                out.push((
                    RptSev::Warning,
                    "雇用形態「正社員」の構成比を見直す候補".to_string(),
                    format!(
                        "当サンプル {:.1}% / HW 市場 {:.1}% で {:.1}pt 差。",
                        csv_rate, hw_rate, diff
                    ),
                    "(Section 4 参照)".to_string(),
                ));
            }
        }
    }

    // C: タグプレミアム（diff_percent > 5%, count >= 10 の最大 1 件）
    let candidate_tag = agg
        .by_tag_salary
        .iter()
        .filter(|t| t.count >= 10 && t.diff_percent.abs() > 5.0)
        .max_by(|a, b| {
            a.diff_percent
                .abs()
                .partial_cmp(&b.diff_percent.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if let Some(t) = candidate_tag {
        let direction = if t.diff_from_avg > 0 {
            "プレミアム要因の可能性"
        } else {
            "ディスカウント要因の可能性"
        };
        out.push((
            RptSev::Info,
            format!("訴求タグ「{}」の給与差分", t.tag),
            format!(
                "該当タグ平均が全体比 {:+.1} 万円 ({:+.1}%、n={})。{}（相関であり因果は別途検討）。",
                t.diff_from_avg as f64 / 10_000.0,
                t.diff_percent,
                t.count,
                direction
            ),
            "(Section 10 参照)".to_string(),
        ));
    }

    // severity 降順で並べて最大 3 件
    out.sort_by_key(|(sev, _, _, _)| match sev {
        RptSev::Critical => 0,
        RptSev::Warning => 1,
        RptSev::Info => 2,
        RptSev::Positive => 3,
    });
    out.truncate(3);
    out
}
