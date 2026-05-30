//! Section 07 - 最低賃金・ライフスタイル (Phase 4 navy 本実装)
//!
//! navy_report.rs の分割 (A1 Commit 7 / β Section Team / 2026-05-30) で抽出。
//!
//! 元 `navy_report/mod.rs` L1178-L2464 の以下を物理コピー:
//! - `render_navy_section_07_lifestyle`           (公開 API: pub(crate))
//! - `build_navy_minwage_vs_salary_table`         (private helper)
//! - `build_navy_household_vs_salary_table`       (private helper)
//! - `build_navy_minwage_premium_histogram_svg`   (pub(crate) — report_html 外
//!                                                  `hourly_report_qa_test.rs` から
//!                                                  `super::navy_report::build_navy_minwage_premium_histogram_svg`
//!                                                  で参照されているため再エクスポート必須)
//! - `build_navy_lifestyle_facilities_table`      (private helper)
//! - `build_navy_minwage_chart`                   (private helper)
//! - `build_navy_household_table`                 (private helper)
//! - `build_lifestyle_so_what`                    (private helper)
//! - `label_for_column`                           (pub(crate) — `build_navy_auto_table`
//!                                                  (mod.rs 残置) から参照されるため、
//!                                                  mod.rs 側で `pub(super) use` 再エクスポート
//!                                                  できる必要があり `pub(crate)` に昇格)
//! - 定数 `PREMIUM_BUCKETS` (build_navy_minwage_premium_histogram_svg 用、module-private)
//!
//! API 表面:
//! - `pub(crate) fn render_navy_section_07_lifestyle`
//!   (Commit 2/3/4/5/6 パターン踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//! - `pub(crate) fn build_navy_minwage_premium_histogram_svg`
//!   (`hourly_report_qa_test.rs` 参照のため再エクスポート必須)
//! - `pub(crate) fn label_for_column`
//!   (`build_navy_auto_table` から `super::label_for_column` で参照される。
//!   mod.rs の `pub(super) use` 再エクスポートを成立させるため `pub(crate)` に昇格)
//!
//! 残りの helper は本ファイル内のみで使用。`navy_report` モジュール外への露出はない。
//!
//! `build_navy_auto_table` は mod.rs に残置 (Section 03/05/06/07 で共有)。
//! `super::build_navy_auto_table` で参照する。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_07_lifestyle):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::build_navy_auto_table;
use super::common::{format_mm, push_kpi, push_page_head, push_region_scope_banner};

// ============================================================
// Section 07: 最低賃金・ライフスタイル (Phase 4 navy 本実装)
// ============================================================

pub(crate) fn render_navy_section_07_lifestyle(
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

    use super::super::super::super::helpers::{get_f64, get_i64, get_str_ref};

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
        .map(|r| {
            (
                get_str_ref(r, "category").to_string(),
                get_f64(r, "monthly_amount") as i64,
            )
        })
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
    let pref_prefix = if ctx.pref.is_empty() {
        String::new()
    } else {
        format!("{} ", ctx.pref)
    };
    let wage_seg = latest_wage
        .filter(|(y, w)| *y > 0 && *w > 0)
        .map(|(y, w)| {
            format!(
                "{}最低賃金 {} 年 <strong>{} 円/時</strong>",
                pref_prefix,
                y,
                format_number(w)
            )
        })
        .or_else(|| {
            latest_wage.filter(|(_, w)| *w > 0).map(|(_, w)| {
                format!(
                    "{}最低賃金 <strong>{} 円/時</strong>",
                    pref_prefix,
                    format_number(w)
                )
            })
        });
    let consumption_seg = if total_consumption > 0 {
        Some(format!(
            "月間消費支出 <strong>{}</strong> 円",
            format_number(total_consumption)
        ))
    } else {
        None
    };
    let commute_seg = if commute_pop > 0 {
        Some(format!(
            "通勤圏内人口 <strong>{}</strong> 名{}",
            format_number(commute_pop),
            if commute_working > 0 {
                format!(" (生産年齢 {} 名)", format_number(commute_working))
            } else {
                String::new()
            }
        ))
    } else {
        None
    };
    let segments: Vec<String> = [wage_seg, consumption_seg, commute_seg]
        .into_iter()
        .flatten()
        .collect();
    let lede = if segments.is_empty() {
        "対象地域の生活コスト・通勤圏に関する公的指標が取得できませんでした。\
         以降のセクションで給与・人口側の指標から定性評価を補完してください。"
            .to_string()
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
    let wage_val = latest_wage
        .map(|(_, w)| format!("{}", format_number(w)))
        .unwrap_or_else(|| "—".into());
    let wage_foot = match (oldest_wage, latest_wage) {
        (Some((y0, _)), Some((y1, _))) if y0 != y1 => format!("{}-{} 年推移", y0, y1),
        _ => "最新年度のみ".to_string(),
    };
    push_kpi(
        html,
        "最低賃金",
        &wage_val,
        "円/時",
        "neu",
        &wage_foot,
        true,
    );
    let yoy_val = wage_yoy
        .map(|v| format!("{:+.1}", v))
        .unwrap_or_else(|| "—".into());
    let yoy_dot = match wage_yoy {
        Some(v) if v >= 3.0 => "pos",
        Some(v) if v >= 1.0 => "neu",
        Some(_) => "warn",
        None => "neu",
    };
    push_kpi(
        html,
        "前年比",
        &yoy_val,
        "%",
        yoy_dot,
        "最新 vs 前年",
        false,
    );
    push_kpi(
        html,
        "月間消費支出",
        &format_number(total_consumption),
        "円",
        "neu",
        "世帯あたり月平均",
        false,
    );
    let int_val = internet_rate
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "—".into());
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
    push_kpi(
        html,
        "ネット利用率",
        &int_val,
        "%",
        int_dot,
        &sp_foot,
        false,
    );
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
        html.push_str(
            "<div class=\"block-title block-title-spaced\">図 7-2 &nbsp;最低賃金 推移</div>\n",
        );
        html.push_str(&build_navy_minwage_chart(&wages));
        html.push_str("<p class=\"caption\">出典: 厚生労働省 地域別最低賃金 (10 月発効)。年率 3% 以上は <strong>pos</strong>、1-3% は標準、1% 未満は <strong>warn</strong>。</p>\n");
    }

    // -- 家計支出構成 table-navy
    if !category_breakdown.is_empty() && total_consumption > 0 {
        html.push_str("<div class=\"block-title block-title-spaced\">表 7-A &nbsp;家計支出構成 (件数最多 6 費目)</div>\n");
        html.push_str(&build_navy_household_table(
            &category_breakdown,
            total_consumption,
        ));
    }

    // -- 通勤圏 table
    if commute_pop > 0 || commute_inflow > 0 {
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 7-B &nbsp;通勤圏 サマリ</div>\n",
        );
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
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 7-C &nbsp;昼夜間人口比較</div>\n",
        );
        html.push_str(&build_navy_auto_table(&ctx.ext_daytime_pop, 3));
        let ratio_opt = ctx.ext_daytime_pop.first().and_then(|r| {
            for k in ["daytime_nighttime_ratio", "dn_ratio", "day_night_ratio"] {
                let v = get_f64(r, k);
                if v > 0.0 {
                    return Some(v);
                }
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
        html.push_str(
            "<div class=\"block-title block-title-spaced\">表 7-D &nbsp;世帯構成</div>\n",
        );
        html.push_str(&build_navy_auto_table(&ctx.ext_households, 3));
        let single_rate_opt = ctx
            .ext_households
            .first()
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
    let minwage_vs_salary =
        build_navy_minwage_vs_salary_table(median_min_salary, agg.is_hourly, latest_wage);
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
        median_min_salary / super::super::super::aggregator::HOURLY_TO_MONTHLY_HOURS
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
        format!(
            "{} 万円/月 ({} 円/時換算, &divide;167h)",
            format_mm(median_min_salary),
            format_number(hourly_equiv)
        )
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
        median_min_salary * super::super::super::aggregator::HOURLY_TO_MONTHLY_HOURS
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
            total_consumption > 0 && (*amt as f64 / total_consumption as f64 * 100.0) >= 10.0
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
pub(crate) fn build_navy_minwage_premium_histogram_svg(
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
fn build_navy_lifestyle_facilities_table(ctx: &InsightContext) -> String {
    use super::super::super::super::helpers::{get_f64, get_i64};
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
            fmt_cmp(
                physicians_per_10k,
                ctx.pref_avg_physicians_per_10k,
                "人/万人",
            )
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

fn build_navy_household_table(categories: &[(String, i64)], total: i64) -> String {
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
            let pct = if total > 0 {
                *amount as f64 / total as f64 * 100.0
            } else {
                0.0
            };
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
pub(crate) fn label_for_column(key: &str) -> &str {
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
            tracing::warn!(
                unmapped_column = key,
                "label_for_column: unmapped column displayed as English snake_case"
            );
            key
        }
    }
}
