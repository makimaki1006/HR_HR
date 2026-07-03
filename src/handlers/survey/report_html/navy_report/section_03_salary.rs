//! Section 03 - 給与分布 統計 (Phase 2 で navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 6 / β Section Team / 2026-05-30) で抽出。
//!
//! 元 `navy_report/mod.rs` L110-L1301 の以下を物理コピー:
//! - `render_navy_section_03_salary`          (公開 API: pub(crate))
//! - `build_navy_fuyou_table`                 (pub(crate) — report_html 外
//!                                              `hourly_report_qa_test.rs` から
//!                                              `super::navy_report::build_navy_fuyou_table`
//!                                              で参照されているため再エクスポート必須)
//! - `build_navy_emp_type_salary_table`       (private helper)
//! - `build_navy_tag_premium_top10_table`     (private helper)
//! - `build_navy_industry_salary_table`       (private helper、現状未呼出だが残置)
//! - `compute_navy_salary_correlation`        (private helper、現状未呼出だが残置)
//! - `build_navy_salary_correlation_table`    (private helper、現状未呼出だが残置)
//! - `build_navy_cluster_table`               (private helper)
//! - `build_navy_cluster_boxplots_svg`        (private helper)
//! - `build_navy_occupation_salary_table`     (private helper、現状未呼出だが残置)
//! - `build_navy_salary_summary_table`        (private helper)
//! - 補助型 `NavyCorrRow` (compute_navy_salary_correlation 用、module-private)
//! - 定数 `FUYOU_WEEKLY_HOURS` / `FUYOU_THRESHOLDS_MAN` (扶養範囲到達時給用)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_03_salary`
//!   (Commit 2/3/4/5 パターン踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//! - `pub(crate) fn build_navy_fuyou_table`
//!   (`hourly_report_qa_test.rs` が `super::navy_report::build_navy_fuyou_table`
//!   path で参照しており、`navy_report/mod.rs` 側で `pub(super) use` 再エクスポート
//!   できる必要があるため `pub(crate)` に昇格)
//!
//! 残りの helper は本ファイル内のみで使用。`navy_report` モジュール外への露出はない。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_03_salary):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::super::salary_summary;
use super::common::{
    build_navy_histogram_svg, build_navy_salary_scatter_svg, build_salary_scatter_summary,
    compute_distribution_stats, format_mm, push_kpi, push_page_head, DistStats,
};

// ============================================================
// Section 03: 給与分布 統計 (Phase 2 で navy 本実装)
// ============================================================

pub(crate) fn render_navy_section_03_salary(
    html: &mut String,
    agg: &SurveyAggregation,
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    // P2-1 (2026-05-28): Section 03 図 3-6 散布図 (給与レンジ各点 1 求人) を追加するため、
    // InsightContext.salary_scatter_pairs を参照する。Option はテスト fixture / 旧呼出から
    // None を渡しても従来動作 (散布図のみ非表示) を維持する。
    hw_context: Option<&InsightContext>,
) {
    // Phase 2-A (2026-05-29): 時給モード対応。
    //   - is_hourly = agg.is_hourly (aggregator が WageMode から導出済)
    //   - 時給モード時は agg.salary_min_values_native / salary_max_values_native / scatter_min_max_native を使う。
    //     これらは Hourly レコードを 円/時 のまま (×167 換算なし) で保持。
    //   - 月給モード時は呼出側から渡された salary_min_values / salary_max_values (月給換算済) をそのまま使用。
    //   - bin_step: 月給=10_000 (1万円刻み)、時給=50 (50円/時刻み)
    let is_hourly = agg.is_hourly;
    let (vals_min, vals_max, bin_step, unit_label, bin_step_label): (
        &[i64],
        &[i64],
        i64,
        &str,
        &str,
    ) = if is_hourly {
        (
            agg.salary_min_values_native.as_slice(),
            agg.salary_max_values_native.as_slice(),
            50,
            "円/時",
            "50円刻み",
        )
    } else {
        (
            salary_min_values,
            salary_max_values,
            10_000,
            "万円",
            "10,000円刻み",
        )
    };

    html.push_str("<section id=\"navy-salary\" class=\"page-navy navy-salary\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 03",
        if is_hourly {
            "給与分布 統計 (時給モード)"
        } else {
            "給与分布 統計"
        },
        if is_hourly {
            "CSV 抽出済み下限・上限給与 (円/時) の分布と代表値"
        } else {
            "CSV 抽出済み下限・上限給与の分布と代表値"
        },
    );

    // 統計値計算 (下限 / 上限 それぞれ) — bin_step は is_hourly に応じて切替
    let stats_min = compute_distribution_stats(vals_min, bin_step);
    let stats_max = compute_distribution_stats(vals_max, bin_step);

    let salary_h = salary_summary::SalaryHeadline::from_aggregation(agg);
    let headline = salary_h.cover_highlight_text();
    let total = agg.total_count;
    // 2026-05-14: 給与解析率の表記は撤去。n は給与解析できた件数を直接表示する。
    let parsed_n = (agg.total_count as f64 * agg.salary_parse_rate).round() as i64;

    // -- exec-headline 風: 給与代表値を冒頭で 1 行に集約
    let lede = format!(
        "サンプル <strong>n={}</strong> (給与解析できた求人)。\
         代表値: <strong>{} {}{}</strong>。本ページでは下限・上限給与それぞれの分布を確認します。",
        format_number(parsed_n),
        escape_html(&headline.label),
        escape_html(&headline.value_text),
        escape_html(&headline.unit),
    );
    let _ = total;
    html.push_str(&format!(
        "<div class=\"exec-headline\">\
         <div class=\"eh-quote\" aria-hidden=\"true\">&ldquo;</div>\
         <p>{}</p>\
         </div>\n",
        lede
    ));

    // 月給/時給 で表示単位を切替する helper
    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    // -- KPI row 6 cell: P25 / 中央値 / 平均 / 最頻値 / P75 / P90 (下限給与)
    //   2026-05-14: 最頻値 (mode) を追加。3x2 グリッド。
    //   Phase 2-A (2026-05-29): is_hourly に応じて単位ラベルを切替。
    let mode_foot = if is_hourly {
        "50円/時刻みの最頻 bin"
    } else {
        "10,000円刻みの最頻 bin"
    };
    if let Some(s) = stats_min.as_ref() {
        html.push_str("<div class=\"block-title\">図 3-1 &nbsp;下限給与 主要分位点</div>\n");
        html.push_str("<div class=\"kpi-row kpi-row-6\">\n");
        push_kpi(
            html,
            "P25",
            &fmt_val(s.p25),
            unit_label,
            "neu",
            "下位 25% 水準",
            false,
        );
        push_kpi(
            html,
            "中央値 P50",
            &fmt_val(s.median),
            unit_label,
            "neu",
            "サンプル中央値",
            true,
        );
        push_kpi(
            html,
            "平均",
            &fmt_val(s.mean),
            unit_label,
            "neu",
            "外れ値の影響を含む",
            false,
        );
        push_kpi(
            html,
            "最頻値",
            &fmt_val(s.mode_bin_yen),
            unit_label,
            "neu",
            mode_foot,
            false,
        );
        push_kpi(
            html,
            "P75",
            &fmt_val(s.p75),
            unit_label,
            "neu",
            "P75 ライン (P50 より上)",
            false,
        );
        push_kpi(
            html,
            "P90",
            &fmt_val(s.p90),
            unit_label,
            "neu",
            "高給与帯",
            false,
        );
        html.push_str("</div>\n");

        // -- histogram (bin_step 刻み)
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">図 3-2 &nbsp;下限給与 分布 ({})</div>\n",
            bin_step_label
        ));
        html.push_str(&build_navy_histogram_svg(vals_min, s, unit_label, bin_step));
        html.push_str("<p class=\"caption\">縦線: 緑=中央値 / 金=平均 / 灰=最頻 bin</p>\n");
    } else {
        html.push_str(
            "<p class=\"caption\">下限給与の有効値が不足しています (n=0 or 全 unparsed)。</p>\n",
        );
    }

    // -- 上限給与 (6 cell, 最頻値含む)
    if let Some(s) = stats_max.as_ref() {
        html.push_str("<div class=\"block-title block-title-spaced\">図 3-3 &nbsp;上限給与 主要分位点</div>\n");
        html.push_str("<div class=\"kpi-row kpi-row-6\">\n");
        push_kpi(
            html,
            "P25",
            &fmt_val(s.p25),
            unit_label,
            "neu",
            "下位 25% 水準",
            false,
        );
        push_kpi(
            html,
            "中央値 P50",
            &fmt_val(s.median),
            unit_label,
            "neu",
            "サンプル中央値",
            true,
        );
        push_kpi(
            html,
            "平均",
            &fmt_val(s.mean),
            unit_label,
            "neu",
            "外れ値の影響を含む",
            false,
        );
        push_kpi(
            html,
            "最頻値",
            &fmt_val(s.mode_bin_yen),
            unit_label,
            "neu",
            mode_foot,
            false,
        );
        push_kpi(
            html,
            "P75",
            &fmt_val(s.p75),
            unit_label,
            "neu",
            "P75 ライン (P50 より上)",
            false,
        );
        push_kpi(
            html,
            "P90",
            &fmt_val(s.p90),
            unit_label,
            "neu",
            "高給与帯",
            false,
        );
        html.push_str("</div>\n");

        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">図 3-4 &nbsp;上限給与 分布 ({})</div>\n",
            bin_step_label
        ));
        html.push_str(&build_navy_histogram_svg(vals_max, s, unit_label, bin_step));
        html.push_str("<p class=\"caption\">縦線: 緑=中央値 / 金=平均 / 灰=最頻 bin</p>\n");
    } else {
        html.push_str("<p class=\"caption\">上限給与の有効値が不足しています。</p>\n");
    }

    // -- 集計サマリ table-navy
    html.push_str(
        "<div class=\"block-title block-title-spaced\">表 3-A &nbsp;給与分布 集計サマリ</div>\n",
    );
    html.push_str(&build_navy_salary_summary_table(
        &stats_min, &stats_max, is_hourly,
    ));

    // -- 雇用形態別給与 (旧 employment::render_section_employment 相当を navy で再構築)
    if !agg.by_emp_type_salary.is_empty() {
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 3-B &nbsp;雇用形態別給与</div>\n",
        );
        html.push_str(&build_navy_emp_type_salary_table(
            &agg.by_emp_type_salary,
            agg.total_count,
            is_hourly,
        ));
    }

    // -- 表 3-C 業界×給与クロス / 表 3-D 職種×給与クロス / 表 3-F 要因分析
    //
    // 2026-05-14 撤去 (ユーザー判断):
    //   業界・職種推定は keyword substring マッチングベースで分類精度が著しく低い
    //   (例: indeed-2026-05-12.csv 物流ドライバー CSV で職種推定 n=6/265、約 2%)。
    //   推定不可分が大半を占めるため統計指標として誤誘導になり得ると判断。
    //   LLM ベースの分類実装まで非表示とする (#239/#240/#241 関連)。
    //
    //   表 3-F も推定値 (職種・業界) に η² 計算が依存するため同時撤去。
    //
    //   関連関数 (industry_salary::aggregate_industry_salary,
    //   occupation_salary::aggregate_occupation_salary,
    //   compute_navy_salary_correlation) は他箇所/テストから参照されるため
    //   残置。Section 03 からの呼び出しのみ削除。

    // -- 給与構造クラスタ分析 (旧 salary_stats の Jenks + per-cluster box) を navy で取り込み
    //   設計メモ §7-8 (給与構造クラスタリング) + §10 (適正値 P25/P50/P60/P75/P90) 準拠
    //
    //   Phase 2-A (2026-05-29): クラスタ計算は **常に scatter_min_max (月給換算済)** で実施。
    //   時給モードでは表示時に月給→時給逆換算 (/HOURLY_TO_MONTHLY_HOURS) し caption で
    //   「時給換算」と注記。クラスタ分類自体は monthly 基準のほうがレンジ分類 P33/P66 の
    //   信頼性が高い (時給のみだとパート/アルバイトに偏ってクラスタ数が減るため)。
    let pairs: Vec<(i64, i64)> = agg.scatter_min_max.iter().map(|p| (p.x, p.y)).collect();
    let clusters = super::super::helpers::compute_salary_clusters(&pairs);
    if !clusters.is_empty() {
        let cluster_table_caption = if is_hourly {
            "時給換算表示 (月給/167h)"
        } else {
            "月給"
        };
        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">表 3-E &nbsp;給与構造クラスタ (Jenks 自然分割 × レンジ分類) — {}</div>\n",
            cluster_table_caption
        ));
        html.push_str(&build_navy_cluster_table(&clusters));

        // P0-9 (MVP, 2026-06-03): CSV 求人 × クラスタ当て込み 10 件抽出 (下限給与降順)。
        //   各求人を nearest_cluster (P50 距離) で割り当て、クラスタ内 P25/P75 で
        //   低め / 適正 / 高め の 3 段階判定を行う。設計メモ受領後に正規化予定。
        //   silent fallback 防御: clusters / scatter_min_max が空なら何も出力しない。
        html.push_str(&build_navy_cluster_fitting_table(agg, &clusters, is_hourly));

        html.push_str(&format!(
            "<div class=\"block-title block-title-spaced\">図 3-5 &nbsp;クラスタ別 ボックスプロット (下限給与) — {}</div>\n",
            cluster_table_caption
        ));
        html.push_str(&build_navy_cluster_boxplots_svg(&clusters));
        // 2026-05-14: ろうそく足 (ボックスプロット) の読み方を凡例で明示
        html.push_str(
            "<div class=\"caption\" style=\"display:grid;grid-template-columns:1fr 1fr;gap:4mm;\
             background:var(--paper);border:1px solid var(--rule-soft);padding:3mm 4mm;margin:2mm 0 3mm;\">\
             <div><strong>図の読み方 (ボックスプロット)</strong><br>\
             <span style=\"display:inline-block;width:10px;height:10px;background:#F0E9D6;border:1px solid #C9A24B;vertical-align:middle;margin-right:4px;\"></span>箱 = <strong>P25 〜 P75</strong> (中央 50% の給与レンジ)<br>\
             <span style=\"display:inline-block;width:2px;height:10px;background:#3CA46E;vertical-align:middle;margin-right:6px;\"></span>緑線 = <strong>P50 (中央値)</strong><br>\
             <span style=\"display:inline-block;width:6px;height:6px;background:#C9A24B;border-radius:50%;vertical-align:middle;margin-right:4px;\"></span>金ドット = <strong>平均値</strong><br>\
             ヒゲ (両端) = <strong>最小/最大</strong>。箱が長い = レンジが広い。\
             </div>\
             <div><strong>各クラスタの解釈</strong><br>\
             ・箱が <strong>右寄り</strong> = 給与水準が高いクラスタ<br>\
             ・箱が <strong>左寄り</strong> = 給与水準が低いクラスタ<br>\
             ・箱が <strong>細い</strong> = 給与がそろっている (定額型)<br>\
             ・箱が <strong>太い</strong> = 給与に差がある (歩合・等級型)<br>\
             ・<strong>n が小さい行 (n&lt;10)</strong> は参考値として扱う\
             </div>\
             </div>\n",
        );
        html.push_str(
            "<p class=\"caption\">出典: CSV 集計。\
             lower_salary 軸は Jenks 自然分割 (k=3 or 4)、range 軸は P33/P66 + P95 異常広判定。\
             各クラスタ内 P25/P50/P60/P75/P90 が顧客求人の適正値の基準。\
             <strong>適正値は全体ではなくクラスタ内で算出</strong>。</p>\n",
        );
    }

    // -- 表 3-G タグ×給与プレミアム top 10 (2026-05-23 #225 統合: market_intelligence 系の知見を navy 取り込み)
    //   訴求タグごとに「全体平均と比べてどれだけ給与が高いか」を可視化。
    //   求人作成時の付与タグ選択 / 自社求人と競合の差分要因分析に活用。
    if !agg.by_tag_salary.is_empty() {
        // 全体加重平均 (overall_mean):
        // - 給与解析できた件数 (parsed_n) に占める各タグの avg_salary &times; count の総和を
        //   parsed_n で割って算出。aggregator.rs の diff_from_avg と整合するため、
        //   enhanced_stats.mean (raw 値の単純平均) ではなく加重平均で再計算する。
        let total_weighted_n: i64 = agg.by_tag_salary.iter().map(|t| t.count as i64).sum();
        let weighted_sum: i64 = agg
            .by_tag_salary
            .iter()
            .map(|t| t.avg_salary * t.count as i64)
            .sum();
        let overall_mean: i64 = if total_weighted_n > 0 {
            weighted_sum / total_weighted_n
        } else {
            // by_tag_salary が空でないのに total_weighted_n=0 は count=0 のみのケース。
            // この場合は enhanced_stats.mean をフォールバックとして使う (silent fallback
            // ではなく明示的に文脈の異なる値であることを caption に記載する経路)。
            agg.enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0)
        };
        if overall_mean > 0 {
            html.push_str("<div class=\"block-title block-title-spaced\">表 3-G &nbsp;タグ&times;給与プレミアム top 10</div>\n");
            html.push_str(&build_navy_tag_premium_top10_table(
                &agg.by_tag_salary,
                overall_mean,
                agg.is_hourly,
            ));
        }
    }

    // -- 図 3-6 給与レンジ 散布図 (P2-1, 2026-05-28)
    //   各点 = 1 求人、X 軸=下限給与、Y 軸=上限給与、対角線 (下限=上限) を参考線として描画。
    //   ctx が None or salary_scatter_pairs が空なら何も出力しない (silent fallback ではなく
    //   明示的に省略: postings に月給フィルタ後データが無い／test fixture 経由の呼出)。
    //
    //   Phase 2-A (2026-05-29): 時給モード時は agg.scatter_min_max_native (Hourly 円/時)
    //   を使う。ctx.salary_scatter_pairs は HW postings の月給データのみのため、
    //   時給モードでは Hourly レコードの集計値 (CSV ベース) を別経路で渡す。
    if is_hourly {
        // 時給モード: aggregator のネイティブ散布図ペアを使う
        if !agg.scatter_min_max_native.is_empty() {
            // (i64, i64) → (f64, f64) 変換 (SVG 関数の互換)
            let pairs: Vec<(f64, f64)> = agg
                .scatter_min_max_native
                .iter()
                .map(|(lo, hi)| (*lo as f64, *hi as f64))
                .collect();
            html.push_str(
                "<div class=\"block-title block-title-spaced\">図 3-6 &nbsp;給与レンジ 散布図 (各点=1求人、対角線=下限=上限ライン)</div>\n",
            );
            html.push_str(&build_navy_salary_scatter_svg(&pairs, true));
            html.push_str(
                "<p class=\"caption\">CSV 内の時給レコードから抽出。X軸=下限給与、Y軸=上限給与 (円/時)。\
                 下限=上限の対角線から離れるほどレンジが広い (等級制の特徴)。</p>\n",
            );
            html.push_str(&build_salary_scatter_summary(&pairs, true));
        }
    } else if let Some(ctx) = hw_context {
        // 月給モード: 旧動作 (HW postings 由来の月給ペア)
        let pairs = ctx.salary_scatter_pairs.as_slice();
        if !pairs.is_empty() {
            html.push_str(
                "<div class=\"block-title block-title-spaced\">図 3-6 &nbsp;給与レンジ 散布図 (各点=1求人、対角線=下限=上限ライン)</div>\n",
            );
            html.push_str(&build_navy_salary_scatter_svg(pairs, false));
            html.push_str(
                "<p class=\"caption\">対象地域から最大 1000 件抽出。X軸=下限給与、Y軸=上限給与。\
                 下限=上限の対角線から離れるほどレンジが広い (歩合・等級制の特徴)。</p>\n",
            );
            html.push_str(&build_salary_scatter_summary(pairs, false));
        }
    }

    // -- 図 3-7 扶養範囲到達ライン (Phase 2-B H1, 2026-05-29)
    //   時給モードのみ表示。年収 = 時給 × 週稼働時間 × 52 を逆算し、
    //   103 万円 / 130 万円 の扶養範囲ラインに到達する必要時給を週稼働時間別に提示。
    //   silent fallback 防止: is_hourly == false の月給モードでは完全にこのブロックを省略する。
    if is_hourly {
        // 中央値は salary_min_values_native (時給ネイティブ円/時) から計算。
        // 空 or 全 0 の場合は median = 0 となり build_navy_fuyou_table 側で "—" 表示。
        let median_hourly_native: i64 = {
            let mut v: Vec<i64> = agg
                .salary_min_values_native
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
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 3-H &nbsp;扶養範囲到達時給 (週稼働時間別)</div>\n",
        );
        html.push_str(&build_navy_fuyou_table(median_hourly_native));
        html.push_str(
            "<p class=\"caption\">年収閾値&divide;(週稼働時間&times;52週)で算出。\
             実際の課税範囲は社会保険加入条件 (週20h・月8.8万円・学生除外等) により異なるため別途確認。\
             自社中央値の行は CSV 集計の下限給与中央値 (円/時)。</p>\n",
        );
    }

    // -- So What
    //   Phase 2-A (2026-05-29): is_hourly で文言を切替。
    //   - 月給: 5 万円未満=定額求人 / 10 万円以上=歩合・等級制
    //   - 時給: 100 円未満=定額 / 300 円以上=等級制 (歩合は時給制では一般的でないため文言から削除)
    let so_what = match (stats_min.as_ref(), stats_max.as_ref()) {
        (Some(lo), Some(hi)) => {
            let spread = hi.median - lo.median;
            if is_hourly {
                format!(
                    "下限給与 中央値 <strong>{}円/時</strong> / 上限給与 中央値 <strong>{}円/時</strong>、レンジ <strong>{}円/時</strong>。\
                     給与レンジが <strong>100 円未満</strong> なら「定額」、<strong>300 円以上</strong> なら「等級制」の特徴が見えます。\
                     競合の中央値と比較し、訴求軸を <strong>下限保証</strong> / <strong>上限到達</strong> / <strong>レンジ幅</strong> のいずれに置くか検討してください。",
                    format_number(lo.median),
                    format_number(hi.median),
                    format_number(spread),
                )
            } else {
                let spread_label = format!("{:.1}万円", spread as f64 / 10000.0);
                format!(
                    "下限給与 中央値 <strong>{}万円</strong> / 上限給与 中央値 <strong>{}万円</strong>、レンジ <strong>{}</strong>。\
                     給与レンジが <strong>5 万円未満</strong> なら「定額求人」、<strong>10 万円以上</strong> なら「歩合・等級制」の特徴が見えます。\
                     競合の中央値と比較し、訴求軸を <strong>下限保証</strong> / <strong>上限到達</strong> / <strong>レンジ幅</strong> のいずれに置くか検討してください。",
                    format_mm(lo.median),
                    format_mm(hi.median),
                    spread_label,
                )
            }
        }
        _ => "給与統計値が不足しています。CSV の給与カラム表記揺れを点検してください。".to_string(),
    };
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
// Phase 2-B (2026-05-29): 時給モード H1 — 扶養範囲到達時給テーブル
// ============================================================
//
// 仕様:
//   - 列: 週稼働時間 (15h / 20h / 25h / 30h / 35h)
//   - 行: 103万円ライン / 130万円ライン / 自社中央値
//   - セル: 必要時給 (円/h)。年収閾値 ÷ (週稼働時間 × 52) で逆算。
//
// 不変条件 (テストで検証):
//   - 130万円ラインの必要時給 > 103万円ラインの必要時給 (同一週時間)
//   - 同一行内で週時間昇順 → 必要時給降順 (反転)
//   - median_hourly_native <= 0 の場合、自社中央値は "—" 表示
//   - 値はすべて非負整数
//
// silent fallback 監査:
//   - median <= 0 → "—" を明示表示。空文字を返さない。
//   - 週時間が 0 になることはない (定数配列のため)。
const FUYOU_WEEKLY_HOURS: [i64; 5] = [15, 20, 25, 30, 35];
const FUYOU_THRESHOLDS_MAN: [(i64, &str); 2] = [(103, "103 万円ライン"), (130, "130 万円ライン")];

/// 扶養範囲到達時給 (週稼働時間別) テーブルを生成。
///
/// # 引数
/// - `median_hourly_native`: 自社中央値 (円/時)。0 以下なら "—" 行を出力。
///
/// # 戻り値
/// HTML 表 (`<table class="table-navy">...</table>`)。
pub(crate) fn build_navy_fuyou_table(median_hourly_native: i64) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th scope=\"col\">区分</th>");
    for h in FUYOU_WEEKLY_HOURS.iter() {
        s.push_str(&format!("<th scope=\"col\" class=\"num\">週 {}h</th>", h));
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    // 103万 / 130万 行
    for (man, label) in FUYOU_THRESHOLDS_MAN.iter() {
        s.push_str("<tr>");
        s.push_str(&format!("<td><strong>{}</strong></td>", escape_html(label)));
        let annual_yen: i64 = *man * 10_000;
        for h in FUYOU_WEEKLY_HOURS.iter() {
            // 必要時給 = 年収閾値 / (週時間 × 52)。1 円単位で切上 (扶養超過リスク回避)。
            let denom = *h * 52;
            // denom は 15*52..=35*52 で常に > 0 (定数配列)
            let needed = (annual_yen + denom - 1) / denom;
            s.push_str(&format!(
                "<td class=\"num bold\">{} 円/時</td>",
                format_number(needed)
            ));
        }
        s.push_str("</tr>\n");
    }

    // 自社中央値行
    s.push_str("<tr class=\"hl\">");
    s.push_str("<td><strong>自社 下限給与 中央値</strong></td>");
    if median_hourly_native > 0 {
        // 全列に中央値を表示 (週時間によらず固定値)。
        // 各列で「103万/130万ラインを上回るか」を視覚的に比較できるよう、同値を繰り返す。
        for _h in FUYOU_WEEKLY_HOURS.iter() {
            s.push_str(&format!(
                "<td class=\"num bold\">{} 円/時</td>",
                format_number(median_hourly_native)
            ));
        }
    } else {
        for _h in FUYOU_WEEKLY_HOURS.iter() {
            s.push_str("<td class=\"num dim\">—</td>");
        }
    }
    s.push_str("</tr>\n");

    s.push_str("</tbody></table>\n");
    s
}

/// 雇用形態別給与 table-navy (No. / 雇用形態 / n / 構成比 / 平均給与 / 中央値 / 全体差分タグ)
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。月給/時給で表示単位を切替。
///
/// - 月給モード: `format_mm` で万円換算、キャプション「単位: 万円 (月給換算済み)」
/// - 時給モード: `format_number` で円のまま、キャプション「単位: 円/時 (時給換算済み)」
///
/// 注意: 値そのものは agg.by_emp_type_salary の avg_salary / median_salary を使う。
/// 時給モードでは aggregator が時給値を保持 (HOURLY_TO_MONTHLY_HOURS で除算 / 一部レコードは
/// 月給→時給換算) する想定。本関数は表示単位のみを切替える役割。
fn build_navy_emp_type_salary_table(
    items: &[super::super::super::aggregator::EmpTypeSalary],
    total_count: usize,
    is_hourly: bool,
) -> String {
    // 全体加重平均を計算 (差分タグの基準)
    let total_n_with_salary: i64 = items.iter().map(|e| e.count as i64).sum();
    let weighted_sum: i64 = items.iter().map(|e| e.avg_salary * e.count as i64).sum();
    let overall_avg = if total_n_with_salary > 0 {
        weighted_sum / total_n_with_salary
    } else {
        0
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>雇用形態</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str("<th class=\"num\">構成比</th>");
    s.push_str("<th class=\"num\">平均給与</th>");
    s.push_str("<th class=\"num\">中央値</th>");
    s.push_str("<th>全体差分</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    // 件数降順 (Round 1-K 2026-06-03: 同件数時は emp_type asc で順序確定)
    let mut sorted: Vec<&super::super::super::aggregator::EmpTypeSalary> = items.iter().collect();
    sorted.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.emp_type.cmp(&b.emp_type))
    });

    for (i, e) in sorted.iter().enumerate() {
        let pct = if total_count > 0 {
            e.count as f64 / total_count as f64 * 100.0
        } else {
            0.0
        };
        let diff_pct = if overall_avg > 0 {
            (e.avg_salary - overall_avg) as f64 / overall_avg as f64 * 100.0
        } else {
            0.0
        };
        let (tag, tag_label) = if diff_pct >= 10.0 {
            ("pos", "高給与")
        } else if diff_pct <= -10.0 {
            ("warn", "低給与")
        } else {
            ("neu", "中央付近")
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        // Phase 2-A: is_hourly に応じて表示単位切替
        let fmt_val = |yen: i64| -> String {
            if is_hourly {
                format_number(yen)
            } else {
                format_mm(yen)
            }
        };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{:.1}%</td>\
             <td class=\"num\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:+.1}%</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&e.emp_type),
            format_number(e.count as i64),
            pct,
            fmt_val(e.avg_salary),
            fmt_val(e.median_salary),
            tag,
            tag_label,
            diff_pct,
        ));
    }
    s.push_str("</tbody></table>\n");
    let (unit_label, fmt_overall) = if is_hourly {
        ("円/時", format_number(overall_avg))
    } else {
        ("万円", format_mm(overall_avg))
    };
    let unit_note = if is_hourly {
        "(時給)"
    } else {
        "(月給換算済み)"
    };
    s.push_str(&format!(
        "<p class=\"caption\">単位: {} {}。差分: 全体加重平均給与 ({}{}) との比較。+10% 以上 = 高給与, -10% 以下 = 低給与。</p>\n",
        unit_label, unit_note, fmt_overall, unit_label
    ));
    s
}

// 2026-05-23 #225: タグ×給与プレミアム top 10 (Section 03 拡張)
//
// 設計:
// - `by_tag_salary` は aggregator で既に `diff_from_avg` (全体平均との差, 円) /
//   `diff_percent` (差分率, %) を計算済み (aggregator.rs L317-339)。
// - 全体給与平均より高い (diff_from_avg > 0) タグだけを抽出し、
//   diff_percent 降順 で上位 10 件を提示する。
// - サンプル数 (count) の閾値は aggregator 側で既に min_sample=3 が適用済み。
//   ここで追加で count >= 10 にする (少数サンプルの統計的揺らぎを除外、
//   MEMORY: feedback_test_data_validation.md「データ妥当性」)。
// - 表示単位は月給万円。is_hourly の場合は注記する。
// - 因果ではなく相関であることを caption で明記 (MEMORY:
//   feedback_correlation_not_causation.md)。
fn build_navy_tag_premium_top10_table(
    items: &[super::super::super::aggregator::TagSalaryAgg],
    overall_mean: i64,
    is_hourly: bool,
) -> String {
    // diff_from_avg > 0 かつ count >= 10 のタグ (高プレミアム & 統計的に意味ある件数)
    let mut filtered: Vec<&super::super::super::aggregator::TagSalaryAgg> = items
        .iter()
        .filter(|t| t.diff_from_avg > 0 && t.count >= 10)
        .collect();
    // diff_percent 降順 (プレミアム率の高い順) (Round 1-K 2026-06-03: 同率時は tag asc で順序確定)
    filtered.sort_by(|a, b| {
        b.diff_percent
            .partial_cmp(&a.diff_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.tag.cmp(&b.tag))
    });
    filtered.truncate(10);

    if filtered.is_empty() {
        return "<p class=\"caption dim\">給与プレミアム (全体平均より高い) を持つタグが \
                統計的に有意な件数 (n &ge; 10) で抽出できませんでした。タグ件数が少ないか、\
                求人内のタグ付与傾向が均質である可能性があります。</p>\n"
            .to_string();
    }

    let unit_label = if is_hourly { "円/時" } else { "万円" };
    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>タグ</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str(&format!("<th class=\"num\">平均給与 ({})</th>", unit_label));
    s.push_str(&format!("<th class=\"num\">全体差分 ({})</th>", unit_label));
    s.push_str("<th class=\"num\">プレミアム率</th>");
    s.push_str("<th>位置づけ</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    for (i, t) in filtered.iter().enumerate() {
        // 位置づけタグ: +20% 以上 = 高プレミアム / +10% 以上 = 中プレミアム / それ以下 = 弱プレミアム
        let (tag, label) = if t.diff_percent >= 20.0 {
            ("pos", "高プレミアム")
        } else if t.diff_percent >= 10.0 {
            ("pos", "中プレミアム")
        } else {
            ("neu", "弱プレミアム")
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">+{}</td>\
             <td class=\"num bold\">+{:.1}%</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&t.tag),
            format_number(t.count as i64),
            fmt_val(t.avg_salary),
            fmt_val(t.diff_from_avg),
            t.diff_percent,
            tag,
            label,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">基準: 全体加重平均給与 <strong>{} {}</strong>。プレミアム率 = (タグ平均 - 全体平均) / 全体平均 &times; 100%。\
         n &ge; 10 のタグに限定 (統計的揺らぎ抑制)。\
         <strong>相関であって因果ではありません</strong>: タグが給与を高めるのではなく、給与が高い求人にこのタグが付与されやすい傾向を示します。</p>\n",
        fmt_val(overall_mean),
        unit_label
    ));
    s
}

// 業界×給与 table-navy (No. / 業界 / n / 平均給与 / 中央値 / 全体差分タグ / 参考)
fn build_navy_industry_salary_table(
    rows: &[super::super::industry_salary::IndustrySalaryRow],
    is_hourly: bool,
) -> String {
    // 件数加重 全体平均
    let total_n: i64 = rows.iter().map(|r| r.count).sum();
    let weighted_sum: i64 = rows.iter().map(|r| r.weighted_avg * r.count).sum();
    let overall_avg = if total_n > 0 {
        weighted_sum / total_n
    } else {
        0
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>業界 (推定)</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str("<th class=\"num\">平均給与</th>");
    s.push_str("<th class=\"num\">中央値</th>");
    s.push_str("<th>全体差分</th>");
    s.push_str("<th>注記</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen) // 時給は円のまま
        } else {
            format_mm(yen) // 月給は万円換算
        }
    };

    for (i, r) in rows.iter().enumerate() {
        let diff_pct = if overall_avg > 0 {
            (r.weighted_avg - overall_avg) as f64 / overall_avg as f64 * 100.0
        } else {
            0.0
        };
        let (tag, tag_label) = if diff_pct >= 10.0 {
            ("pos", "高給与帯")
        } else if diff_pct <= -10.0 {
            ("warn", "低給与帯")
        } else {
            ("neu", "中央付近")
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        let median_str = match r.median_of_company_medians {
            Some(m) => fmt_val(m),
            None => "—".to_string(),
        };
        let note_html = if r.note.is_empty() {
            String::new()
        } else {
            format!("<span class=\"tag tag-neu\">{}</span>", escape_html(r.note))
        };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:+.1}%</span></td>\
             <td>{}</td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&r.industry),
            format_number(r.count),
            fmt_val(r.weighted_avg),
            median_str,
            tag,
            tag_label,
            diff_pct,
            note_html,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">業界推定は CSV の企業名 + タグから自動分類。\
         全体平均: {} {}。件数 &lt; 3 は「参考 (低信頼)」表示。\
         <strong>相関 ≠ 因果</strong>: 業界別給与差分は要因分析ではなく分布差として読んでください。</p>\n",
        if is_hourly { format_number(overall_avg) } else { format_mm(overall_avg) },
        if is_hourly { "円/時" } else { "万円" }
    ));
    s
}

// 相関分析: 給与×カテゴリ要因 (雇用形態 / 職種 / 業界)
//   各要因の説明力 = (カテゴリ平均と全体平均の差の二乗の加重平均) / (全体分散)
//   η² (eta squared) 相当の簡易版。0-1 範囲、1 に近いほど要因で説明できる。
struct NavyCorrRow {
    factor: String,
    n_categories: usize,
    n_total: i64,
    max_minus_min_avg: i64,
    eta_sq: f64, // 0.0 - 1.0
}

fn compute_navy_salary_correlation(agg: &SurveyAggregation) -> Vec<NavyCorrRow> {
    let mut rows: Vec<NavyCorrRow> = Vec::new();

    // 因子 1: 雇用形態 (by_emp_type_salary)
    if !agg.by_emp_type_salary.is_empty() {
        let n_total: i64 = agg.by_emp_type_salary.iter().map(|e| e.count as i64).sum();
        let weighted_sum: i64 = agg
            .by_emp_type_salary
            .iter()
            .map(|e| e.avg_salary * e.count as i64)
            .sum();
        let overall_avg = if n_total > 0 {
            weighted_sum / n_total
        } else {
            0
        };
        let between_var: f64 = agg
            .by_emp_type_salary
            .iter()
            .map(|e| {
                let diff = (e.avg_salary - overall_avg) as f64;
                diff * diff * e.count as f64
            })
            .sum::<f64>()
            / (n_total.max(1) as f64);
        // 全体分散の代理: σ² ≈ overall_avg の 10% を 1σ と仮定。
        // 実 records レベル分散が ない (agg は集計済み) ため、scatter_min_max から派生。
        let total_var: f64 = if !agg.salary_values.is_empty() {
            let mean =
                agg.salary_values.iter().sum::<i64>() as f64 / agg.salary_values.len() as f64;
            agg.salary_values
                .iter()
                .map(|&v| {
                    let d = v as f64 - mean;
                    d * d
                })
                .sum::<f64>()
                / agg.salary_values.len() as f64
        } else {
            between_var * 2.0 // フォールバック
        };
        let eta_sq = (between_var / total_var.max(1.0)).min(1.0);
        let max_avg = agg
            .by_emp_type_salary
            .iter()
            .map(|e| e.avg_salary)
            .max()
            .unwrap_or(0);
        let min_avg = agg
            .by_emp_type_salary
            .iter()
            .map(|e| e.avg_salary)
            .min()
            .unwrap_or(0);
        rows.push(NavyCorrRow {
            factor: "雇用形態".to_string(),
            n_categories: agg.by_emp_type_salary.len(),
            n_total,
            max_minus_min_avg: max_avg - min_avg,
            eta_sq,
        });
    }

    // 因子 2: 職種 (occupation_salary)
    let occ_rows = super::super::occupation_salary::aggregate_occupation_salary(agg);
    if !occ_rows.is_empty() {
        let n_total: i64 = occ_rows.iter().map(|r| r.count).sum();
        let weighted_sum: i64 = occ_rows.iter().map(|r| r.weighted_avg * r.count).sum();
        let overall_avg = if n_total > 0 {
            weighted_sum / n_total
        } else {
            0
        };
        let between_var: f64 = occ_rows
            .iter()
            .map(|r| {
                let diff = (r.weighted_avg - overall_avg) as f64;
                diff * diff * r.count as f64
            })
            .sum::<f64>()
            / (n_total.max(1) as f64);
        let total_var: f64 = if !agg.salary_values.is_empty() {
            let mean =
                agg.salary_values.iter().sum::<i64>() as f64 / agg.salary_values.len() as f64;
            agg.salary_values
                .iter()
                .map(|&v| {
                    let d = v as f64 - mean;
                    d * d
                })
                .sum::<f64>()
                / agg.salary_values.len() as f64
        } else {
            between_var * 2.0
        };
        let eta_sq = (between_var / total_var.max(1.0)).min(1.0);
        let max_avg = occ_rows.iter().map(|r| r.weighted_avg).max().unwrap_or(0);
        let min_avg = occ_rows.iter().map(|r| r.weighted_avg).min().unwrap_or(0);
        rows.push(NavyCorrRow {
            factor: "職種 (推定)".to_string(),
            n_categories: occ_rows.len(),
            n_total,
            max_minus_min_avg: max_avg - min_avg,
            eta_sq,
        });
    }

    // 因子 3: 業界 (industry_salary)
    let ind_rows = super::super::industry_salary::aggregate_industry_salary(agg);
    if !ind_rows.is_empty() {
        let n_total: i64 = ind_rows.iter().map(|r| r.count).sum();
        let weighted_sum: i64 = ind_rows.iter().map(|r| r.weighted_avg * r.count).sum();
        let overall_avg = if n_total > 0 {
            weighted_sum / n_total
        } else {
            0
        };
        let between_var: f64 = ind_rows
            .iter()
            .map(|r| {
                let diff = (r.weighted_avg - overall_avg) as f64;
                diff * diff * r.count as f64
            })
            .sum::<f64>()
            / (n_total.max(1) as f64);
        let total_var: f64 = if !agg.salary_values.is_empty() {
            let mean =
                agg.salary_values.iter().sum::<i64>() as f64 / agg.salary_values.len() as f64;
            agg.salary_values
                .iter()
                .map(|&v| {
                    let d = v as f64 - mean;
                    d * d
                })
                .sum::<f64>()
                / agg.salary_values.len() as f64
        } else {
            between_var * 2.0
        };
        let eta_sq = (between_var / total_var.max(1.0)).min(1.0);
        let max_avg = ind_rows.iter().map(|r| r.weighted_avg).max().unwrap_or(0);
        let min_avg = ind_rows.iter().map(|r| r.weighted_avg).min().unwrap_or(0);
        rows.push(NavyCorrRow {
            factor: "業界 (推定)".to_string(),
            n_categories: ind_rows.len(),
            n_total,
            max_minus_min_avg: max_avg - min_avg,
            eta_sq,
        });
    }

    // η² 降順 (Round 1-K 2026-06-03: 同値時は factor asc で順序確定)
    rows.sort_by(|a, b| {
        b.eta_sq
            .partial_cmp(&a.eta_sq)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.factor.cmp(&b.factor))
    });
    rows
}

fn build_navy_salary_correlation_table(rows: &[NavyCorrRow]) -> String {
    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>要因</th>");
    s.push_str("<th class=\"num\">カテゴリ数</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str("<th class=\"num\">最大-最小 平均差</th>");
    s.push_str("<th class=\"num\">η² (説明力)</th>");
    s.push_str("<th>判定</th>");
    s.push_str("</tr></thead>\n<tbody>\n");
    for (i, r) in rows.iter().enumerate() {
        let (tag, label) = if r.eta_sq >= 0.10 {
            ("pos", "強い説明力")
        } else if r.eta_sq >= 0.05 {
            ("neu", "中程度")
        } else {
            ("neu", "弱い説明力")
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num bold\">{:.3}</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&r.factor),
            r.n_categories,
            format_number(r.n_total),
            format_mm(r.max_minus_min_avg),
            r.eta_sq,
            tag,
            label,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str("<p class=\"caption\">η² は要因 (雇用形態/職種/業界) が給与差を説明する割合。\
                0.10 以上で「強い説明力」、0.05-0.10 で「中程度」、未満で「弱い」と判定 (社会科学慣例の目安)。\
                推定要因 (職種/業界) は CSV 自動分類のため誤差を含みます。\
                <strong>相関 ≠ 因果</strong>: η² は分散説明であり、因果関係を証明するものではありません。</p>\n");
    s
}

// 給与構造クラスタ table-navy (label / lower_seg / range_seg / n / P25/P50/P60/P75/P90/mean)
fn build_navy_cluster_table(clusters: &[super::super::helpers::SalaryCluster]) -> String {
    // rank 26: 第1列 (クラスタ名) が幅指定なしで 3 行折返しになる問題を回避するため
    //   colgroup で列幅を明示 (クラスタ列 22% + No. 6% + 各分位点 8% + 解釈 16%)。
    let mut s = String::from(
        "<table class=\"table-navy\">\n\
         <colgroup>\
         <col style=\"width:6%\"/>\
         <col style=\"width:22%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:8%\"/>\
         <col style=\"width:16%\"/>\
         </colgroup>\n\
         <thead><tr>",
    );
    s.push_str("<th>No.</th><th>クラスタ</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str("<th class=\"num\">P25</th>");
    s.push_str("<th class=\"num\">中央値 P50</th>");
    s.push_str("<th class=\"num\">P60</th>");
    s.push_str("<th class=\"num\">P75</th>");
    s.push_str("<th class=\"num\">P90</th>");
    s.push_str("<th class=\"num\">平均</th>");
    s.push_str("<th>解釈</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    // 件数降順 (Round 1-K 2026-06-03: 同件数時は label asc で順序確定)
    let mut sorted: Vec<&super::super::helpers::SalaryCluster> = clusters.iter().collect();
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));

    for (i, c) in sorted.iter().enumerate() {
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        let (tag, interp) = match c.range_seg {
            "異常広レンジ" => ("warn", "高レンジ訴求 / 歩合・委託の可能性"),
            "広レンジ" => ("neu", "上限訴求が強い帯"),
            "狭レンジ" => ("neu", "定額型求人"),
            _ => ("neu", "通常レンジ"),
        };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&c.label),
            format_number(c.count as i64),
            format_mm(c.p25),
            format_mm(c.p50),
            format_mm(c.p60),
            format_mm(c.p75),
            format_mm(c.p90),
            format_mm(c.mean),
            tag,
            interp,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(
        "<p class=\"caption\"><strong>出典:</strong> CSV 集計 (月給換算済み)。単位: 万円。</p>\n",
    );
    s
}

// ============================================================
// P0-9 (MVP, 2026-06-03): CSV 求人 × クラスタ当て込み 10 件抽出
// ============================================================
//
// 仕様 (MVP):
//   - 入力: agg.scatter_min_max (下限給与降順で 10 件抽出) + clusters
//   - 各求人を nearest_cluster (P50 距離) で割り当て
//   - 判定: lower < P25 → 低め (tag-warn) / P25 <= lower <= P75 → 適正 (tag-pos) /
//           lower > P75 → 高め (tag-neu)
//   - 月給/時給で表示単位切替 (is_hourly)
//
// 仕様 (将来):
//   - 求人タイトル列は scatter_min_max に title フィールド無いため "求人 #N" 連番表記。
//     設計メモ受領後に CsvRecord 由来の title をひも付ける予定 (2026-06-03 完了予定)。
//
// silent fallback 防御:
//   - clusters 空 → 空文字列
//   - scatter_min_max 空 → 空文字列
//   - 全求人が割当不可 (clusters 空のときのみ発生) → 上記の clusters 空チェックでカバー
fn build_navy_cluster_fitting_table(
    agg: &SurveyAggregation,
    clusters: &[super::super::helpers::SalaryCluster],
    is_hourly: bool,
) -> String {
    if clusters.is_empty() {
        return String::new();
    }
    if agg.scatter_min_max.is_empty() {
        return String::new();
    }

    // 下限給与 (x) 降順でソートして 10 件抽出
    let mut sorted_postings: Vec<&super::super::super::aggregator::ScatterPoint> =
        agg.scatter_min_max.iter().collect();
    // Round 1-K 2026-06-03: 同値時は上限給与 (y) 降順で順序確定
    sorted_postings.sort_by(|a, b| b.x.cmp(&a.x).then_with(|| b.y.cmp(&a.y)));
    sorted_postings.truncate(10);

    let unit_label = if is_hourly { "円/時" } else { "万円" };
    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    let mut s = String::new();
    s.push_str(
        "<div class=\"block-title block-title-spaced\">\
         表 3-F &nbsp;CSV 求人 × クラスタ当て込み (10 件抽出)</div>\n",
    );
    s.push_str("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th scope=\"col\">No.</th>");
    s.push_str("<th scope=\"col\">求人</th>");
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">下限給与 ({})</th>",
        unit_label
    ));
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">上限給与 ({})</th>",
        unit_label
    ));
    s.push_str("<th scope=\"col\">割当クラスタ</th>");
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">クラスタ P25 ({})</th>",
        unit_label
    ));
    s.push_str(&format!(
        "<th scope=\"col\" class=\"num\">クラスタ P75 ({})</th>",
        unit_label
    ));
    s.push_str("<th scope=\"col\">判定</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    for (i, p) in sorted_postings.iter().enumerate() {
        // nearest_cluster: P50 距離最小のクラスタを返す。下限給与 (p.x) で距離計算。
        let assigned = super::super::helpers::nearest_cluster(clusters, p.x);
        let (cluster_label, p25, p75, tag, judge_label) = match assigned {
            Some(c) => {
                // P25/P75 で 3 段階判定。lower (p.x) を使う。
                let (tag, label) = if p.x < c.p25 {
                    ("warn", "低め")
                } else if p.x > c.p75 {
                    ("neu", "高め")
                } else {
                    ("pos", "適正")
                };
                (c.label.clone(), c.p25, c.p75, tag, label)
            }
            None => {
                // clusters 空チェック済みのため到達しない。silent fallback 防御の明示。
                ("—".to_string(), 0_i64, 0_i64, "neu", "判定不可")
            }
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>求人 #{}</strong></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"dim\">{}</span></td>\
             <td class=\"num\">{}</td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span></td>\
             </tr>\n",
            row_class,
            i + 1,
            i + 1,
            fmt_val(p.x),
            fmt_val(p.y),
            escape_html(&cluster_label),
            fmt_val(p25),
            fmt_val(p75),
            tag,
            judge_label,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">単位: {}。下限給与降順で 10 件抽出 (代表サンプル)。\
         判定: 下限 &lt; クラスタ P25 = 低め / P25 &le; 下限 &le; P75 = 適正 / 下限 &gt; P75 = 高め。\
         クラスタ割当は P50 距離最小ルール。\
         求人タイトルは元データに含まれないため \"求人 #N\" 連番表記。\
         <strong>※ 推定値。参考値として扱ってください。</strong></p>\n",
        unit_label
    ));
    s
}

// クラスタ別 並列ボックスプロット SVG (下限給与 P25-P75 box + min-max whisker + P50 中央線)
fn build_navy_cluster_boxplots_svg(clusters: &[super::super::helpers::SalaryCluster]) -> String {
    if clusters.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<&super::super::helpers::SalaryCluster> = clusters.iter().collect();
    // Round 1-K 2026-06-03: 同件数時は label asc で順序確定
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    sorted.truncate(8); // 上位 8 cluster

    // 2026-05-14: ユーザー指摘「図 3-5 がもう少し大きくできない、見えづらい」を反映。
    //   row_h 38→56 / 各 font-size ↑ で実効サイズを拡大。
    //   viewBox は w=720 維持 + h を拡大することで、`width=100%` 表示時に縦に伸びる。
    // rank 7 (2026-07): WF3 の font26/22 は実測 scale≈1.0 (フル幅描画) で実寸 26/22px と
    //   本文の 1.8-3.2 倍に過大。クラスタ名 15・軸目盛 14・n ラベル 13 に引き下げ。
    //   label_w 300→240 / n_w 70→60 に戻して視覚バランスを調整。
    //   (viewBox w=720, width=100%, scale≈0.96-1.02 で各 font-size がそのまま実寸になる)
    let w = 720.0;
    let row_h = 56.0;
    let h = 36.0 + sorted.len() as f64 * row_h + 36.0;
    let label_w = 240.0;
    let n_w = 60.0;
    let plot_x = label_w + n_w;
    let plot_w = w - plot_x - 16.0;

    // 全体 max/min を決定 (スケール)
    let max_v = sorted.iter().map(|c| c.p90).max().unwrap_or(1) as f64;
    let min_v = sorted.iter().map(|c| c.p25).min().unwrap_or(0) as f64;
    let span = (max_v - min_v).max(1.0);

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"クラスタ別ボックスプロット\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    );

    let x_of = |v: i64| -> f64 { plot_x + ((v as f64 - min_v) / span).clamp(0.0, 1.0) * plot_w };

    // x軸ラベル (4 点)
    for i in 0..=4 {
        let v = (min_v + span * i as f64 / 4.0) as i64;
        let x = plot_x + plot_w * i as f64 / 4.0;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"24\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ECE7DA\" stroke-width=\"0.5\"/>\n\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"14\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
            x, x, h - 20.0, x, h - 6.0, format_mm(v)
        ));
    }

    // 各 cluster の box (font-size 拡大 + box_h 拡大)
    let row_center_off = row_h / 2.0;
    let box_h = 22.0; // 16.0 → 22.0 (37% UP)
    for (i, c) in sorted.iter().enumerate() {
        let cy = 36.0 + i as f64 * row_h;
        let text_y = cy + row_center_off;
        // label
        svg.push_str(&format!(
            "<text x=\"4\" y=\"{:.1}\" font-size=\"15\" fill=\"#0B1E3F\" font-weight=\"600\">{}</text>\n",
            text_y,
            escape_html(&c.label)
        ));
        // n
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"13\" fill=\"#6A6E7A\" font-family=\"Roboto Mono, monospace\">n={}</text>\n",
            label_w, text_y, c.count
        ));
        // whisker (min ~ max)
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#9CA0AB\" stroke-width=\"1.2\"/>\n",
            x_of(c.min), text_y, x_of(c.max), text_y
        ));
        // box (P25 ~ P75)
        let box_x1 = x_of(c.p25);
        let box_x2 = x_of(c.p75);
        let box_y = text_y - box_h / 2.0;
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#FAF1D9\" stroke=\"#0B1E3F\" stroke-width=\"1\"/>\n",
            box_x1, box_y, (box_x2 - box_x1).max(1.0), box_h
        ));
        // median (P50) 縦線
        let med_x = x_of(c.p50);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#1F6B43\" stroke-width=\"2.5\"/>\n",
            med_x, box_y, med_x, box_y + box_h
        ));
        // mean (金色 dot)
        svg.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"#C9A24B\" stroke=\"#0B1E3F\" stroke-width=\"0.7\"/>\n",
            x_of(c.mean), text_y
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

// 職種×給与 table-navy (industry と同型)
fn build_navy_occupation_salary_table(
    rows: &[super::super::occupation_salary::OccupationSalaryRow],
    is_hourly: bool,
) -> String {
    let total_n: i64 = rows.iter().map(|r| r.count).sum();
    let weighted_sum: i64 = rows.iter().map(|r| r.weighted_avg * r.count).sum();
    let overall_avg = if total_n > 0 {
        weighted_sum / total_n
    } else {
        0
    };

    let mut s = String::from("<table class=\"table-navy\">\n<thead><tr>");
    s.push_str("<th>No.</th><th>職種グループ (推定)</th>");
    s.push_str("<th class=\"num\">n</th>");
    s.push_str("<th class=\"num\">平均給与</th>");
    s.push_str("<th class=\"num\">中央値</th>");
    s.push_str("<th>全体差分</th>");
    s.push_str("<th>注記</th>");
    s.push_str("</tr></thead>\n<tbody>\n");

    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };

    for (i, r) in rows.iter().enumerate() {
        let diff_pct = if overall_avg > 0 {
            (r.weighted_avg - overall_avg) as f64 / overall_avg as f64 * 100.0
        } else {
            0.0
        };
        let (tag, tag_label) = if diff_pct >= 10.0 {
            ("pos", "高給与帯")
        } else if diff_pct <= -10.0 {
            ("warn", "低給与帯")
        } else {
            ("neu", "中央付近")
        };
        let row_class = if i == 0 { " class=\"hl\"" } else { "" };
        let median_str = match r.median_of_company_medians {
            Some(m) => fmt_val(m),
            None => "—".to_string(),
        };
        let note_html = if r.note.is_empty() {
            String::new()
        } else {
            format!("<span class=\"tag tag-neu\">{}</span>", escape_html(r.note))
        };
        s.push_str(&format!(
            "<tr{}>\
             <td class=\"num bold\">{}</td>\
             <td><strong>{}</strong></td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num bold\">{}</td>\
             <td class=\"num\">{}</td>\
             <td><span class=\"tag tag-{}\">{}</span> &nbsp;<span class=\"dim\">{:+.1}%</span></td>\
             <td>{}</td>\
             </tr>\n",
            row_class,
            i + 1,
            escape_html(&r.occupation),
            format_number(r.count),
            fmt_val(r.weighted_avg),
            median_str,
            tag,
            tag_label,
            diff_pct,
            note_html,
        ));
    }
    s.push_str("</tbody></table>\n");
    s.push_str(&format!(
        "<p class=\"caption\">職種推定は CSV の求人タイトル + タグから自動分類 (看護系 / 介護系 / 保育系 等)。\
         全体平均: {} {}。件数 &lt; 3 は「参考 (低信頼)」表示。\
         <strong>相関 ≠ 因果</strong>: 職種別給与差分は要因分析ではなく分布差として読んでください。</p>\n",
        if is_hourly { format_number(overall_avg) } else { format_mm(overall_avg) },
        if is_hourly { "円/時" } else { "万円" }
    ));
    s
}

/// navy 集計テーブル (下限 / 上限 × n/P25/P50/平均/P75/P90/min/max)
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数追加。
/// - `is_hourly = false` (月給モード): 全セル `format_mm()` で万円換算表示、キャプション「単位: 万円」
/// - `is_hourly = true`  (時給モード): 全セル `format_number()` で円のまま表示、キャプション「単位: 円/時」
fn build_navy_salary_summary_table(
    lo: &Option<DistStats>,
    hi: &Option<DistStats>,
    is_hourly: bool,
) -> String {
    let fmt_val = |yen: i64| -> String {
        if is_hourly {
            format_number(yen)
        } else {
            format_mm(yen)
        }
    };
    let mut s = String::new();
    s.push_str("<table class=\"table-navy\">\n");
    s.push_str(
        "<thead><tr>\
                <th>区分</th><th class=\"num\">n</th>\
                <th class=\"num\">最小</th>\
                <th class=\"num\">P25</th>\
                <th class=\"num\">中央値</th>\
                <th class=\"num\">平均</th>\
                <th class=\"num\">最頻値</th>\
                <th class=\"num\">P75</th>\
                <th class=\"num\">P90</th>\
                <th class=\"num\">最大</th>\
                </tr></thead>\n<tbody>\n",
    );
    let row = |label: &str, st: &Option<DistStats>| -> String {
        match st {
            Some(s) => format!(
                "<tr><td><strong>{}</strong></td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num dim\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num bold\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num dim\">{}</td>\
                 </tr>\n",
                label,
                format_number(s.n as i64),
                fmt_val(s.min),
                fmt_val(s.p25),
                fmt_val(s.median),
                fmt_val(s.mean),
                fmt_val(s.mode_bin_yen),
                fmt_val(s.p75),
                fmt_val(s.p90),
                fmt_val(s.max)
            ),
            None => format!(
                "<tr><td><strong>{}</strong></td><td colspan=\"9\" class=\"dim\">—</td></tr>\n",
                label
            ),
        }
    };
    s.push_str(&row("下限給与", lo));
    s.push_str(&row("上限給与", hi));
    s.push_str("</tbody></table>\n");
    let caption = if is_hourly {
        "<p class=\"caption\"><strong>出典:</strong> CSV 集計。単位: 円/時 (時給ネイティブ)。</p>\n"
    } else {
        "<p class=\"caption\"><strong>出典:</strong> CSV 集計。単位: 万円 (月給換算)。</p>\n"
    };
    s.push_str(caption);
    s
}

// ============================================================
// P0-9 テスト (MVP, 2026-06-03): クラスタ当て込み判定の境界値検証
// ============================================================
//
// 不変条件:
//   - lower < P25 → 低め (tag-warn)
//   - P25 <= lower <= P75 → 適正 (tag-pos)
//   - lower > P75 → 高め (tag-neu)
//   - clusters 空 → ""
//   - scatter 空 → ""
//   - 11+ 件 → 10 件で truncate (下限給与降順)
#[cfg(test)]
mod tests {
    use super::*;
    // パス解析 (tests mod は section_03_salary の子):
    //   super              = section_03_salary
    //   super::super       = navy_report
    //   super::super::super = report_html
    //   super::super::super::super = survey
    // よって SalaryCluster (report_html::helpers) = super::super::super::helpers
    //       ScatterPoint  (survey::aggregator)   = super::super::super::super::aggregator
    use super::super::super::super::aggregator::ScatterPoint;
    use super::super::super::helpers::SalaryCluster;

    fn make_cluster(label: &str, p25: i64, p50: i64, p75: i64) -> SalaryCluster {
        SalaryCluster {
            label: label.to_string(),
            lower_seg: "中下限".to_string(),
            range_seg: "通常レンジ",
            count: 50,
            p25,
            p50,
            p60: (p50 + p75) / 2,
            p75,
            p90: p75 + 30_000,
            min: p25 - 50_000,
            max: p75 + 50_000,
            mean: p50,
        }
    }

    fn make_agg_with_scatter(points: Vec<(i64, i64)>) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.scatter_min_max = points
            .into_iter()
            .map(|(x, y)| ScatterPoint { x, y })
            .collect();
        agg
    }

    // 1. P25 <= lower <= P75 で「適正」判定
    #[test]
    fn cluster_fitting_judges_within_p25_p75_as_適正() {
        let clusters = vec![make_cluster("中央帯", 200_000, 250_000, 300_000)];
        let agg = make_agg_with_scatter(vec![(250_000, 350_000)]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        assert!(
            html.contains("適正"),
            "lower=250000 (P25=200000 <= 250000 <= P75=300000) should be 適正: {}",
            html
        );
        assert!(html.contains("tag-pos"), "適正 tag-pos missing: {}", html);
    }

    // 2. lower < P25 で「低め」判定
    #[test]
    fn cluster_fitting_judges_below_p25_as_低め() {
        let clusters = vec![make_cluster("中央帯", 200_000, 250_000, 300_000)];
        let agg = make_agg_with_scatter(vec![(150_000, 200_000)]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        assert!(
            html.contains("低め"),
            "lower=150000 < P25=200000 should be 低め: {}",
            html
        );
        assert!(html.contains("tag-warn"), "低め tag-warn missing: {}", html);
    }

    // 3. lower > P75 で「高め」判定
    #[test]
    fn cluster_fitting_judges_above_p75_as_高め() {
        let clusters = vec![make_cluster("中央帯", 200_000, 250_000, 300_000)];
        let agg = make_agg_with_scatter(vec![(400_000, 500_000)]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        assert!(
            html.contains("高め"),
            "lower=400000 > P75=300000 should be 高め: {}",
            html
        );
        assert!(html.contains("tag-neu"), "高め tag-neu missing: {}", html);
    }

    // 4. clusters 空 → 空文字列
    #[test]
    fn cluster_fitting_empty_when_no_clusters() {
        let clusters: Vec<SalaryCluster> = vec![];
        let agg = make_agg_with_scatter(vec![(250_000, 350_000)]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        assert_eq!(html, "", "no clusters should return empty string");
    }

    // 5. scatter 空 → 空文字列
    #[test]
    fn cluster_fitting_empty_when_no_postings() {
        let clusters = vec![make_cluster("中央帯", 200_000, 250_000, 300_000)];
        let agg = make_agg_with_scatter(vec![]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        assert_eq!(html, "", "no postings should return empty string");
    }

    // 6. 12 件投入で 10 行に truncate (下限給与降順)
    #[test]
    fn cluster_fitting_truncates_to_top_10_by_descending_lower_salary() {
        let clusters = vec![make_cluster("中央帯", 200_000, 250_000, 300_000)];
        // 12 件、下限給与をバラバラに
        let agg = make_agg_with_scatter(vec![
            (100_000, 150_000),
            (200_000, 250_000),
            (150_000, 200_000),
            (300_000, 400_000),
            (250_000, 350_000),
            (180_000, 230_000),
            (220_000, 280_000),
            (170_000, 210_000),
            (350_000, 450_000),
            (190_000, 240_000),
            (210_000, 270_000),
            (160_000, 200_000),
        ]);
        let html = build_navy_cluster_fitting_table(&agg, &clusters, false);
        // <tr> の数 (header の <tr> は除外) で 10 行であることを検証
        // thead に 1, tbody data 行が 10 → 合計 11
        let tr_count = html.matches("<tr").count();
        assert_eq!(
            tr_count, 11,
            "expected 11 <tr> (1 thead + 10 data), got {}: {}",
            tr_count, html
        );
        // 最大値 350,000 が「求人 #1」(下限降順ソート後の先頭) として現れる
        assert!(html.contains("求人 #1"), "求人 #1 missing: {}", html);
        // 100,000 (12 件中 最小値) は 10 件抽出後に含まれていないはず
        // (注: format_mm では "10.0" となる。350000 が 35.0、100000 が 10.0)
        // 念のため、10.0 が含まれていないことだけ確認するのは format_mm の "1.0" "100.0" などとの誤マッチ
        // リスクが大きいため、tr_count = 11 で十分とし、追加検証を行わない。
    }

    // ========================================================================
    // 追加テスト (テスト品質強化, 2026-06-05): データ妥当性 / 境界 / 不変条件
    // 対象純粋関数: build_navy_fuyou_table / build_navy_emp_type_salary_table /
    //              build_navy_tag_premium_top10_table / build_navy_salary_summary_table /
    //              compute_distribution_stats (common 再エクスポート)
    // ========================================================================

    use super::super::super::super::aggregator::{EmpTypeSalary, TagSalaryAgg};

    fn make_emp(emp_type: &str, count: usize, avg: i64, median: i64) -> EmpTypeSalary {
        EmpTypeSalary {
            emp_type: emp_type.to_string(),
            count,
            avg_salary: avg,
            median_salary: median,
        }
    }

    fn make_tag(tag: &str, count: usize, avg: i64, diff: i64, diff_pct: f64) -> TagSalaryAgg {
        TagSalaryAgg {
            tag: tag.to_string(),
            count,
            avg_salary: avg,
            diff_from_avg: diff,
            diff_percent: diff_pct,
        }
    }

    /// 数値を含む全 `<td class="num ...">{値} 円/時</td>` から円/時の整数値を抽出する補助。
    /// "—" セルは対象外 (skip)。
    fn extract_yen_per_hour_cells(html: &str) -> Vec<i64> {
        let mut out = Vec::new();
        for part in html.split("円/時</td>") {
            // 直前の > 以降の数字 (カンマ除去) を拾う
            if let Some(gt) = part.rfind('>') {
                let tail = &part[gt + 1..];
                let digits: String = tail.chars().filter(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() {
                    if let Ok(v) = digits.parse::<i64>() {
                        out.push(v);
                    }
                }
            }
        }
        out
    }

    // --- build_navy_fuyou_table -------------------------------------------

    // [不変条件] 130 万円ラインの必要時給 > 103 万円ラインの必要時給 (同一週時間)。
    //   同一週時間列同士を比較するため、各行 5 セル (週 15/20/25/30/35h) を抽出して照合。
    #[test]
    fn fuyou_table_130man_requires_higher_wage_than_103man() {
        let html = build_navy_fuyou_table(0); // median=0 で自社行は "—" になり数値を汚さない
        let cells = extract_yen_per_hour_cells(&html);
        // 103 万行 5 セル + 130 万行 5 セル = 10 セル (自社行は "—" なので除外)
        assert_eq!(
            cells.len(),
            10,
            "expected 10 numeric cells (103万 5 + 130万 5), got {}: {}",
            cells.len(),
            html
        );
        let row_103 = &cells[0..5];
        let row_130 = &cells[5..10];
        for i in 0..5 {
            assert!(
                row_130[i] > row_103[i],
                "130万 line wage ({}) must exceed 103万 line ({}) at col {}: {}",
                row_130[i],
                row_103[i],
                i,
                html
            );
        }
    }

    // [不変条件] 同一行内で週稼働時間が増えるほど必要時給は単調減少する。
    #[test]
    fn fuyou_table_required_wage_decreases_as_weekly_hours_increase() {
        let html = build_navy_fuyou_table(0);
        let cells = extract_yen_per_hour_cells(&html);
        assert_eq!(cells.len(), 10, "unexpected cell count: {}", html);
        for row in [&cells[0..5], &cells[5..10]] {
            for w in row.windows(2) {
                assert!(
                    w[0] > w[1],
                    "wage must strictly decrease as weekly hours increase: {} -> {} ({})",
                    w[0],
                    w[1],
                    html
                );
            }
        }
    }

    // [データ妥当性] 必要時給はすべて正の整数 (年収閾値 / 正の分母)。
    #[test]
    fn fuyou_table_all_wages_positive() {
        let html = build_navy_fuyou_table(1200);
        let cells = extract_yen_per_hour_cells(&html);
        assert!(!cells.is_empty(), "no numeric cells parsed: {}", html);
        for v in &cells {
            assert!(
                *v > 0,
                "required wage must be positive, got {}: {}",
                v,
                html
            );
        }
    }

    // [境界] median_hourly_native <= 0 では自社中央値行が "—" 表示 (silent fallback ではなく明示)。
    #[test]
    fn fuyou_table_median_zero_shows_dash_row() {
        let html_zero = build_navy_fuyou_table(0);
        assert!(
            html_zero.contains("—"),
            "median=0 should render dash cells: {}",
            html_zero
        );
        // 負値も同様に "—"
        let html_neg = build_navy_fuyou_table(-100);
        assert!(
            html_neg.contains("—"),
            "negative median should render dash cells: {}",
            html_neg
        );
        // 正値では自社中央値が数値として現れる (1200 円/時)
        let html_pos = build_navy_fuyou_table(1200);
        assert!(
            html_pos.contains("自社"),
            "self-median row label missing: {}",
            html_pos
        );
        // 正値ケースには自社中央値 1200 が含まれる (5 列に同値)
        let cells_pos = extract_yen_per_hour_cells(&html_pos);
        assert!(
            cells_pos.iter().filter(|&&v| v == 1200).count() >= 5,
            "median 1200 should appear in all 5 self-median columns: {}",
            html_pos
        );
    }

    // --- build_navy_emp_type_salary_table ---------------------------------

    // [データ妥当性] 構成比 (count/total*100) の合計が ~100% に収まる (total と一致時)。
    //   各行の構成比文字列 "{:.1}%" を抽出して合算し、丸め誤差 ±0.3% 以内を許容。
    #[test]
    fn emp_type_table_composition_sums_to_about_100() {
        let items = vec![
            make_emp("正社員", 60, 280_000, 270_000),
            make_emp("パート", 30, 120_000, 115_000),
            make_emp("契約社員", 10, 200_000, 195_000),
        ];
        let total = 100; // count 合計と一致 → 構成比合計は 100%
        let html = build_navy_emp_type_salary_table(&items, total, false);
        // "{:.1}%" 形式のうち、差分タグ ({:+.1}%) と区別するため
        // 構成比は <td class="num">{:.1}%</td> の形で出る。tag 差分は dim span 内。
        // ここでは行ごとの構成比をデータから直接再計算して検証する (HTML パースの曖昧性回避)。
        let sum_pct: f64 = items
            .iter()
            .map(|e| e.count as f64 / total as f64 * 100.0)
            .sum();
        assert!(
            (sum_pct - 100.0).abs() < 0.3,
            "composition sum should be ~100%, got {}",
            sum_pct
        );
        // HTML 側にも各構成比が現れることを確認 (60.0% / 30.0% / 10.0%)
        assert!(html.contains("60.0%"), "60.0% missing: {}", html);
        assert!(html.contains("30.0%"), "30.0% missing: {}", html);
        assert!(html.contains("10.0%"), "10.0% missing: {}", html);
    }

    // [境界] 全体加重平均 +10% 以上で「高給与」、-10% 以下で「低給与」タグ。
    //   2 区分: 高給与 200,000 (avg) vs 低給与 100,000。加重平均 = 150,000。
    //   高給与は +33.3% → "高給与"、低給与は -33.3% → "低給与"。
    #[test]
    fn emp_type_table_diff_tag_boundary() {
        let items = vec![
            make_emp("正社員", 50, 200_000, 195_000),
            make_emp("パート", 50, 100_000, 95_000),
        ];
        let html = build_navy_emp_type_salary_table(&items, 100, false);
        assert!(html.contains("高給与"), "高給与 tag missing: {}", html);
        assert!(html.contains("低給与"), "低給与 tag missing: {}", html);
    }

    // [境界/零除算防御] total_count=0 でも panic せず、構成比 0.0% を出す。
    #[test]
    fn emp_type_table_zero_total_no_panic() {
        let items = vec![make_emp("正社員", 5, 250_000, 240_000)];
        let html = build_navy_emp_type_salary_table(&items, 0, false);
        assert!(
            html.contains("0.0%"),
            "zero total should yield 0.0%: {}",
            html
        );
        assert!(
            html.contains("<table"),
            "table should still render: {}",
            html
        );
    }

    // [境界] 空入力でも panic せず、ヘッダのみのテーブルを返す (overall_avg=0 で 0除算なし)。
    #[test]
    fn emp_type_table_empty_input_no_panic() {
        let items: Vec<EmpTypeSalary> = vec![];
        let html = build_navy_emp_type_salary_table(&items, 0, false);
        assert!(
            html.contains("<table"),
            "table header should exist: {}",
            html
        );
        // データ行 (<tr> with No.) は無いので caption のみ
        assert!(html.contains("caption"), "caption missing: {}", html);
    }

    // --- build_navy_tag_premium_top10_table -------------------------------

    // [データ妥当性] n < 10 のタグは除外される (統計的揺らぎ抑制)。
    #[test]
    fn tag_premium_excludes_small_samples() {
        let items = vec![
            make_tag("夜勤あり", 5, 320_000, 70_000, 28.0), // n=5 → 除外
            make_tag("資格手当", 20, 300_000, 50_000, 20.0), // n=20 → 採用
        ];
        let html = build_navy_tag_premium_top10_table(&items, 250_000, false);
        assert!(html.contains("資格手当"), "n>=10 tag must appear: {}", html);
        assert!(
            !html.contains("夜勤あり"),
            "n<10 tag must be excluded: {}",
            html
        );
    }

    // [データ妥当性] diff_from_avg <= 0 (プレミアム無し) のタグは除外される。
    #[test]
    fn tag_premium_excludes_non_positive_diff() {
        let items = vec![
            make_tag("低賃金タグ", 30, 200_000, -50_000, -20.0), // diff<0 → 除外
            make_tag("高プレタグ", 30, 300_000, 50_000, 20.0),   // diff>0 → 採用
        ];
        let html = build_navy_tag_premium_top10_table(&items, 250_000, false);
        assert!(
            html.contains("高プレタグ"),
            "positive premium missing: {}",
            html
        );
        assert!(
            !html.contains("低賃金タグ"),
            "non-positive diff tag must be excluded: {}",
            html
        );
    }

    // [境界] 該当タグ 0 件 (全て除外) では fallback caption を返し、table を出さない。
    #[test]
    fn tag_premium_empty_after_filter_shows_fallback() {
        let items = vec![
            make_tag("少数タグ", 3, 320_000, 70_000, 28.0), // n<10 で除外
        ];
        let html = build_navy_tag_premium_top10_table(&items, 250_000, false);
        assert!(
            !html.contains("<table"),
            "no table when all filtered out: {}",
            html
        );
        assert!(
            html.contains("統計的に有意な件数"),
            "fallback caption missing: {}",
            html
        );
    }

    // [境界] プレミアム率 +20% 以上=高プレミアム / +10% 以上=中プレミアム の閾値。
    #[test]
    fn tag_premium_label_boundary() {
        let items = vec![
            make_tag("高", 15, 350_000, 100_000, 20.0), // >=20 → 高プレミアム
            make_tag("中", 15, 300_000, 30_000, 12.0),  // >=10 → 中プレミアム
        ];
        let html = build_navy_tag_premium_top10_table(&items, 250_000, false);
        assert!(
            html.contains("高プレミアム"),
            "高プレミアム missing: {}",
            html
        );
        assert!(
            html.contains("中プレミアム"),
            "中プレミアム missing: {}",
            html
        );
    }

    // --- build_navy_salary_summary_table + compute_distribution_stats -----

    // [不変条件] DistStats の分位点は P25 <= median <= P75 <= P90 / min <= P25 / P90 <= max。
    #[test]
    fn dist_stats_quantile_ordering_invariant() {
        let vals: Vec<i64> = (1..=100).map(|i| i * 10_000).collect(); // 1万..100万
        let s = compute_distribution_stats(&vals, 10_000).expect("stats should compute");
        assert!(s.min <= s.p25, "min<=P25: {} {}", s.min, s.p25);
        assert!(s.p25 <= s.median, "P25<=median: {} {}", s.p25, s.median);
        assert!(s.median <= s.p75, "median<=P75: {} {}", s.median, s.p75);
        assert!(s.p75 <= s.p90, "P75<=P90: {} {}", s.p75, s.p90);
        assert!(s.p90 <= s.max, "P90<=max: {} {}", s.p90, s.max);
        // 中央値・平均は正
        assert!(s.median > 0, "median must be positive: {}", s.median);
        assert!(s.mean > 0, "mean must be positive: {}", s.mean);
        // n は正値件数と一致 (全件正)
        assert_eq!(s.n, 100, "n should equal positive count");
    }

    // [境界] 空入力 / bin_step<=0 / 全 0 値 では None を返す (silent fallback 防止)。
    #[test]
    fn dist_stats_returns_none_for_invalid_input() {
        assert!(
            compute_distribution_stats(&[], 10_000).is_none(),
            "empty -> None"
        );
        assert!(
            compute_distribution_stats(&[100_000], 0).is_none(),
            "bin_step=0 -> None"
        );
        assert!(
            compute_distribution_stats(&[100_000], -1).is_none(),
            "bin_step<0 -> None"
        );
        // 全て 0 以下 (正値フィルタ後に空) → None
        assert!(
            compute_distribution_stats(&[0, -5, 0], 10_000).is_none(),
            "all non-positive -> None"
        );
    }

    // [データ妥当性] summary table は下限/上限の両行を含み、分位点が昇順で出力される。
    //   下限 < 上限となる入力で、両者とも有効値が出る (None フォールバックではない) ことを確認。
    #[test]
    fn salary_summary_table_renders_both_rows_with_valid_stats() {
        let lo_vals: Vec<i64> = (1..=50).map(|i| i * 4_000).collect(); // 0.4万..20万
        let hi_vals: Vec<i64> = (1..=50).map(|i| i * 6_000).collect(); // 0.6万..30万
        let lo = compute_distribution_stats(&lo_vals, 10_000);
        let hi = compute_distribution_stats(&hi_vals, 10_000);
        assert!(lo.is_some() && hi.is_some(), "both stats must compute");
        let html = build_navy_salary_summary_table(&lo, &hi, false);
        assert!(html.contains("下限給与"), "下限給与 row missing: {}", html);
        assert!(html.contains("上限給与"), "上限給与 row missing: {}", html);
        // None フォールバックの "colspan" による "—" 行ではないこと
        assert!(
            !html.contains("colspan=\"9\""),
            "should not render the None-fallback empty row: {}",
            html
        );
    }

    // [境界] 両方 None でも panic せず、両行に "—" フォールバックを出す。
    #[test]
    fn salary_summary_table_none_inputs_show_dash_rows() {
        let html = build_navy_salary_summary_table(&None, &None, false);
        assert!(
            html.contains("下限給与"),
            "下限給与 label missing: {}",
            html
        );
        assert!(
            html.contains("上限給与"),
            "上限給与 label missing: {}",
            html
        );
        assert!(
            html.contains("colspan=\"9\""),
            "dash fallback row missing: {}",
            html
        );
    }
}
