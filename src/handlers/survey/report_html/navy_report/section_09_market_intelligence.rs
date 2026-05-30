//! Section 09 - 採用マーケットインテリジェンス (P0-8 / MarketIntelligence variant 専用)
//!
//! P0-8 (2026-05-30): `ReportVariant::MarketIntelligence` のときだけ追加表示する
//! 6 サブセクションを実装。設計準拠ドキュメント:
//! - `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` (指標定義 §3〜9)
//! - `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` v1.0 (人数表示禁止 §2)
//! - `docs/NAVY_SECTION_09_DESIGN.md` (本 commit の設計メモ)
//!
//! ## サブセクション一覧 (6 件 ≥ 要件 5 件以上)
//!
//! - 9-A 配信優先度サマリーカード (KPI 4 + 配信判断ラベル + SO WHAT)
//! - 9-B 採用ターゲット厚み (相対指数、ext_industry_employees + hw_industry_counts)
//! - 9-C 競合求人密度 (HW 求人件数 / 産業従業者規模、比率のみ)
//! - 9-D 通勤到達性 (commute_inflow_top3 + commute_self_rate)
//! - 9-E 生活コスト補正後給与魅力度 (agg median + ext_min_wage + 家計支出)
//! - 9-F 配信シナリオ濃淡バー (保守/標準/強気、9-A〜9-E の合成)
//!
//! ## 設計方針 (NAVY_SECTION_09_DESIGN.md §0)
//!
//! - variant ガード: MarketIntelligence のみ。Full / Public は呼出側でガード
//!   (mod.rs 側で `if matches!(cfg.variant, ReportVariant::MarketIntelligence)`)
//! - データソース: `hw_context` (InsightContext) のみ。新規 Turso fetch 導入なし
//! - 旧 `market_intelligence.rs` (handlers/survey/report_html/) は置換せず補完。
//!   旧モジュールは媒体分析タブ画面表示 (Turso ベース)、本 Section 09 は PDF レポート
//!   navy_report 経路 (hw_context ベース)。両モジュールは並立し、データソースで分離。
//! - DISPLAY_SPEC v1.0 §2 (人数表示禁止) を厳守: 指数 / ランク / 濃淡のみ。
//!   「○○人」「○○名」「○○万人」「○○億円」を絶対に出力しない。
//! - 「半径 5km」等の架空条件を一切記載しない (LP/サンプル素材ではなく本番レポート)。
//! - 仮説なきデータ投入禁止: 各サブセクションに必ず SO WHAT (配信判断に直結する示唆)
//!   を添える。
//! - SalesNow 文字列禁止: 「外部企業データ」「企業データベース」と記述。
//!
//! ## API 表面
//!
//! - `pub(crate) fn render_navy_section_09_market_intelligence` (Commit 2/3/4/5/6 パターン
//!   踏襲: `pub(super)` は階層不足で E0364 になるため `pub(crate)`)
//!
//! 内部 helper はすべて本ファイル内のみ使用。`navy_report` モジュール外への露出なし。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_09_market_intelligence):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::escape_html;
use super::super::super::super::insight::fetch::InsightContext;
use super::super::super::aggregator::SurveyAggregation;
use super::super::ReportVariant;
use super::common::{fmt_ratio, push_kpi, push_page_head, push_region_scope_banner};

// ============================================================
// Section 09: 採用マーケットインテリジェンス (P0-8)
// ============================================================

/// Section 09 統合エントリ。MI variant 専用 6 サブセクションを順次レンダ。
///
/// `variant` 引数は防御的二重ガード (呼出側でも matches! ガード済み)。
/// `hw_context` が None の場合は placeholder で代替し panic しない。
pub(crate) fn render_navy_section_09_market_intelligence(
    html: &mut String,
    hw_context: Option<&InsightContext>,
    agg: &SurveyAggregation,
    variant: ReportVariant,
    target_region: &str,
) {
    // 防御的ガード: 呼出側でも matches! しているが、関数単体で呼ばれた場合に備える。
    if !matches!(variant, ReportVariant::MarketIntelligence) {
        return;
    }

    html.push_str("<section class=\"page-navy navy-mi\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 09",
        "採用マーケットインテリジェンス",
        "配信優先度 / ターゲット厚み / 競合密度 / 通勤到達性 / 給与魅力度 / シナリオ濃淡",
    );
    push_region_scope_banner(html, target_region);

    // 集計範囲 / 表示単位の注記 (DISPLAY_SPEC §4.2 コンパクト注記)
    html.push_str(
        "<p class=\"caption dim\" style=\"margin-bottom:4mm;\">\
         本セクションは配信判断のための相対指標です。絶対的な就業者規模を表示しません。\
         数値はすべて 0-100 / 0-200 の指数または比率です。\
         </p>\n",
    );

    let ctx = match hw_context {
        Some(c) => Some(c),
        None => {
            html.push_str(
                "<div class=\"caption dim\" style=\"margin:6mm 0;padding:8px 12px;\
                 background:#f9fafb;border-left:3px solid #9ca3af;\">\
                 外部統計データ (InsightContext) が取得できなかったため、本セクションの\
                 各サブセクションはプレースホルダ表示となります。\
                 </div>\n",
            );
            None
        }
    };

    // 9-A 配信優先度サマリーカード (第 1 階層: 最優先)
    let positive_score = render_mi_9a_priority_summary(html, ctx);

    // 9-B 採用ターゲット厚み (第 2 階層)
    render_mi_9b_thickness_index(html, ctx);

    // 9-C 競合求人密度 (第 3 階層)
    let competition_index = render_mi_9c_competition_density(html, ctx);

    // 9-D 通勤到達性 (第 4 階層)
    let commute_reach_index = render_mi_9d_commute_reach(html, ctx);

    // 9-E 生活コスト補正後給与魅力度 (第 4 階層)
    let wage_attractiveness_index = render_mi_9e_wage_attractiveness(html, ctx, agg);

    // 9-F 配信シナリオ濃淡バー (第 5 階層: 合成)
    render_mi_9f_scenario_intensity(
        html,
        positive_score,
        competition_index,
        commute_reach_index,
        wage_attractiveness_index,
        ctx,
    );

    html.push_str("</section>\n");
}

// ============================================================
// 9-A 配信優先度サマリーカード
// ============================================================

/// 戻り値: positive_score (0-100) — 9-F の合成入力に使用。データなしは None。
fn render_mi_9a_priority_summary(html: &mut String, ctx: Option<&InsightContext>) -> Option<f64> {
    use super::super::super::super::helpers::get_f64;

    html.push_str("<div class=\"block-title\">図 9-A 配信優先度サマリー</div>\n");
    html.push_str(
        "<p class=\"caption\">求人倍率・失業率・通勤自給率・流入規模指数を統合し、配信判断のための定性ラベルを提示します。</p>\n",
    );

    let job_ratio = ctx.and_then(|c| {
        c.ext_job_ratio
            .last()
            .map(|r| get_f64(r, "ratio_total"))
            .filter(|v| *v > 0.0)
    });
    let unemployment = ctx.and_then(|c| {
        c.ext_labor_force
            .first()
            .map(|r| get_f64(r, "unemployment_rate"))
            .filter(|v| *v > 0.0)
    });
    let self_rate = ctx.map(|c| c.commute_self_rate);
    let inflow_total = ctx.map(|c| c.commute_inflow_total).unwrap_or(0);

    // 4 KPI タイル
    html.push_str("<div class=\"kpi-row kpi-row-4\">\n");
    {
        let (val, dot, foot) = match job_ratio {
            Some(v) if v >= 1.5 => (
                fmt_ratio(Some(v)),
                "warn",
                "売り手市場 (応募集めにくい)".to_string(),
            ),
            Some(v) if v >= 1.0 => (
                fmt_ratio(Some(v)),
                "neu",
                "やや売り手寄り".to_string(),
            ),
            Some(v) => (
                fmt_ratio(Some(v)),
                "pos",
                format!("買い手市場 ({:.2})", v),
            ),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "有効求人倍率", &val, "倍", dot, &foot, true);
    }
    {
        let (val, dot, foot) = match unemployment {
            Some(v) if v >= 3.5 => (
                format!("{:.1}", v),
                "pos",
                "求職プール厚い".to_string(),
            ),
            Some(v) if v >= 2.5 => (format!("{:.1}", v), "neu", "標準的".to_string()),
            Some(v) => (format!("{:.1}", v), "warn", "求職プール薄い".to_string()),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "失業率", &val, "%", dot, &foot, false);
    }
    {
        let (val, dot, foot) = match self_rate {
            Some(v) if v >= 0.7 => (
                format!("{:.1}", v * 100.0),
                "pos",
                "地元充足度 高".to_string(),
            ),
            Some(v) if v >= 0.4 => (
                format!("{:.1}", v * 100.0),
                "neu",
                "通勤圏依存中程度".to_string(),
            ),
            Some(v) => (
                format!("{:.1}", v * 100.0),
                "warn",
                "通勤圏依存大".to_string(),
            ),
            None => ("—".to_string(), "neu", "データなし".to_string()),
        };
        push_kpi(html, "通勤自給率", &val, "%", dot, &foot, false);
    }
    {
        // 流入規模指数 (0-100、人数表示は行わない: DISPLAY_SPEC §2.1 ハード NG 回避)
        let inflow_idx = compute_inflow_intensity_index(inflow_total);
        let (dot, foot) = if inflow_idx >= 70.0 {
            ("pos", "外部流入 厚い (補助配信候補多い)".to_string())
        } else if inflow_idx >= 40.0 {
            ("neu", "外部流入 中程度".to_string())
        } else if inflow_idx > 0.0 {
            ("warn", "外部流入 薄い".to_string())
        } else {
            ("neu", "データなし".to_string())
        };
        let val = if inflow_idx > 0.0 {
            format!("{:.0}", inflow_idx)
        } else {
            "—".to_string()
        };
        push_kpi(html, "流入規模指数", &val, "", dot, &foot, false);
    }
    html.push_str("</div>\n");

    // 配信優先度ラベル (DISPLAY_SPEC §3.2)
    let positive_score = compute_positive_score(job_ratio, unemployment, self_rate, inflow_total);
    let label = compute_priority_label(positive_score);
    html.push_str(&format!(
        "<div class=\"so-what\" style=\"margin-top:4mm;\">\
         <div class=\"sw-label\">配信判断</div>\
         <div class=\"sw-body\">\
         配信優先度 (推定): <strong>{}</strong> (positive_score = {})。\
         <br>SO WHAT: 配信優先度ラベルに従い「重点配信」「拡張候補」地域から媒体投下を開始する。\
         </div></div>\n",
        escape_html(label),
        match positive_score {
            Some(v) => format!("{:.0}", v),
            None => "—".to_string(),
        }
    ));

    positive_score
}

/// positive_score (0-100) を 4 段階の配信優先度ラベルに分類 (DISPLAY_SPEC §3.2)。
///
/// 境界: 80+ = 重点配信 / 65-79 = 拡張候補 / 50-64 = 維持/検証 / 0-49 = 優先度低
/// データなしは「判定不能」。
fn compute_priority_label(score: Option<f64>) -> &'static str {
    match score {
        Some(v) if v >= 80.0 => "重点配信",
        Some(v) if v >= 65.0 => "拡張候補",
        Some(v) if v >= 50.0 => "維持/検証",
        Some(_) => "優先度低",
        None => "判定不能",
    }
}

/// 4 入力から positive_score (0-100) を導出。METRICS.md §2.1 に準拠した近似式。
/// 全データが None なら None を返す。
fn compute_positive_score(
    job_ratio: Option<f64>,
    unemployment: Option<f64>,
    self_rate: Option<f64>,
    inflow_total: i64,
) -> Option<f64> {
    let mut weights = 0.0;
    let mut acc = 0.0;
    // 有効求人倍率: 0.5-2.0 を 0-100 にマップ (1.0 が中央 50)
    if let Some(v) = job_ratio {
        let normalized = ((v - 0.5) / 1.5 * 100.0).clamp(0.0, 100.0);
        acc += normalized * 30.0;
        weights += 30.0;
    }
    // 失業率: 高いほど求職者多い = +
    if let Some(v) = unemployment {
        let normalized = (v / 6.0 * 100.0).clamp(0.0, 100.0);
        acc += normalized * 20.0;
        weights += 20.0;
    }
    // 通勤自給率: 0-1 を 0-100 に
    if let Some(v) = self_rate {
        acc += (v * 100.0).clamp(0.0, 100.0) * 25.0;
        weights += 25.0;
    }
    // 流入規模指数
    let inflow_idx = compute_inflow_intensity_index(inflow_total);
    if inflow_idx > 0.0 {
        acc += inflow_idx * 25.0;
        weights += 25.0;
    }

    if weights <= 0.0 {
        None
    } else {
        Some((acc / weights).clamp(0.0, 100.0))
    }
}

/// 流入規模を 0-100 指数化 (DISPLAY_SPEC §2.1 人数表示禁止のため絶対値を出さない)。
/// 0-100,000 → 0-100 のログスケール近似。0 は 0 を返す。
fn compute_inflow_intensity_index(inflow_total: i64) -> f64 {
    if inflow_total <= 0 {
        return 0.0;
    }
    let v = inflow_total as f64;
    let log = (v + 1.0).log10();
    // log10(1) = 0, log10(100,000) = 5.0 → 0-100 にマップ
    ((log / 5.0) * 100.0).clamp(0.0, 100.0)
}

// ============================================================
// 9-B 採用ターゲット厚み (相対指数)
// ============================================================

fn render_mi_9b_thickness_index(html: &mut String, ctx: Option<&InsightContext>) {
    use super::super::super::super::helpers::{get_f64, get_str_ref};

    html.push_str("<div class=\"block-title\">図 9-B 採用ターゲット厚み (相対指数)</div>\n");
    html.push_str("<p class=\"caption\">産業大分類の構成比を全国平均と比較した相対指数 (100 = 全国平均、200 が上限)。絶対値の表示は行いません。</p>\n");

    let ctx = match ctx {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption dim\">取得値なし</p>\n");
            return;
        }
    };

    // hw_industry_counts (求人産業構成) と ext_industry_employees (経済センサス) を統合。
    // 比較対象は HW 求人産業構成の上位カテゴリ。
    let total_hw: i64 = ctx.hw_industry_counts.iter().map(|(_, n)| *n).sum();
    if total_hw <= 0 || ctx.hw_industry_counts.is_empty() {
        html.push_str("<p class=\"caption dim\">産業構成データなし</p>\n");
        return;
    }

    // 上位 8 産業について「構成比 ÷ 全国平均構成比 × 100」を厚み指数とみなす。
    // 全国平均は ext_industry_employees から導出 (なければ均等分布 1/N で代用)。
    let mut national_total: i64 = 0;
    let mut national_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &ctx.ext_industry_employees {
        let name = get_str_ref(r, "industry_name").to_string();
        let v = get_f64(r, "employees") as i64;
        if v > 0 && !name.is_empty() {
            national_total += v;
            *national_map.entry(name).or_insert(0) += v;
        }
    }

    html.push_str("<table class=\"table-navy\" style=\"font-size:10pt;\">\n");
    html.push_str("<thead><tr><th>産業大分類</th><th>地域構成比</th><th>全国構成比 (推定)</th><th>厚み指数</th><th>判定</th></tr></thead>\n<tbody>\n");

    let mut shown = 0;
    for (name, count) in ctx.hw_industry_counts.iter().take(8) {
        let local_share = (*count as f64) / (total_hw as f64) * 100.0;
        let national_share = if national_total > 0 {
            let n = national_map.get(name).copied().unwrap_or(0);
            (n as f64) / (national_total as f64) * 100.0
        } else {
            // 全国データなし: 上位 8 産業 1/8 を均等分布として代用
            100.0 / 8.0
        };
        let thickness = if national_share > 0.0 {
            (local_share / national_share * 100.0).clamp(0.0, 200.0)
        } else {
            0.0
        };
        let (badge, dot_class) = if thickness >= 120.0 {
            ("厚い (推定)", "pos")
        } else if thickness >= 80.0 {
            ("平均 (推定)", "neu")
        } else {
            ("薄い (推定)", "warn")
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{:.1}%</td><td>{:.1}%</td><td><strong>{:.0}</strong></td>\
             <td><span class=\"dot {}\"></span>{}</td></tr>\n",
            escape_html(name),
            local_share,
            national_share,
            thickness,
            dot_class,
            badge
        ));
        shown += 1;
    }
    if shown == 0 {
        html.push_str("<tr><td colspan=\"5\" class=\"dim\">該当データなし</td></tr>\n");
    }
    html.push_str("</tbody></table>\n");

    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:3mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">\
         厚み指数 120+ の産業を主訴求軸とし、80- の産業は別チャネル (リファラル / SNS 等) を検討する。\
         指数は (推定) です。絶対的な就業者規模を保証しません。\
         </div></div>\n",
    );
}

// ============================================================
// 9-C 競合求人密度 (クロス分析)
// ============================================================

/// 戻り値: 競合密度から導出した penalty 用指数 (0-100、高いほど競合が激しい)。
fn render_mi_9c_competition_density(
    html: &mut String,
    ctx: Option<&InsightContext>,
) -> Option<f64> {
    use super::super::super::super::helpers::{get_f64, get_str_ref};

    html.push_str("<div class=\"block-title\">図 9-C 競合求人密度 (クロス分析)</div>\n");
    html.push_str("<p class=\"caption\">産業別 HW 求人件数 ÷ 産業就業者規模で算出した相対密度比。値が高い産業は競合配信が激しいことを示唆します (実測ベース)。</p>\n");

    let ctx = match ctx {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption dim\">取得値なし</p>\n");
            return None;
        }
    };

    let mut national_map: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in &ctx.ext_industry_employees {
        let name = get_str_ref(r, "industry_name").to_string();
        let v = get_f64(r, "employees");
        if v > 0.0 && !name.is_empty() {
            *national_map.entry(name).or_insert(0.0) += v;
        }
    }

    if national_map.is_empty() || ctx.hw_industry_counts.is_empty() {
        html.push_str("<p class=\"caption dim\">外部統計または HW 産業構成が未取得</p>\n");
        return None;
    }

    html.push_str("<table class=\"table-navy\" style=\"font-size:10pt;\">\n");
    html.push_str("<thead><tr><th>産業大分類</th><th>HW 求人 構成比</th><th>就業者規模 構成比</th><th>密度比 (求人÷就業者)</th><th>判定</th></tr></thead>\n<tbody>\n");

    let total_hw: i64 = ctx.hw_industry_counts.iter().map(|(_, n)| *n).sum();
    let total_emp: f64 = national_map.values().sum();
    let mut sum_ratio = 0.0;
    let mut count = 0;

    for (name, hw_count) in ctx.hw_industry_counts.iter().take(8) {
        let emp = national_map.get(name).copied().unwrap_or(0.0);
        if emp <= 0.0 || total_hw <= 0 || total_emp <= 0.0 {
            continue;
        }
        let hw_share = (*hw_count as f64) / (total_hw as f64) * 100.0;
        let emp_share = emp / total_emp * 100.0;
        let density_ratio = hw_share / emp_share;
        let (badge, dot_class) = if density_ratio >= 1.5 {
            ("競合 激しい", "warn")
        } else if density_ratio >= 0.8 {
            ("競合 標準", "neu")
        } else {
            ("競合 薄い", "pos")
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{:.1}%</td><td>{:.1}%</td><td><strong>{:.2}</strong></td>\
             <td><span class=\"dot {}\"></span>{}</td></tr>\n",
            escape_html(name),
            hw_share,
            emp_share,
            density_ratio,
            dot_class,
            badge
        ));
        sum_ratio += density_ratio;
        count += 1;
    }
    if count == 0 {
        html.push_str("<tr><td colspan=\"5\" class=\"dim\">該当データなし</td></tr>\n");
    }
    html.push_str("</tbody></table>\n");

    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:3mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">\
         密度比の低い産業帯 (1.0 未満) は配信単価を抑制でき、密度比 1.5+ の産業帯では訴求差別化 (給与・働き方・福利厚生) に投資配分する。\
         </div></div>\n",
    );

    if count > 0 {
        let avg = sum_ratio / (count as f64);
        Some((avg * 50.0).clamp(0.0, 100.0))
    } else {
        None
    }
}

// ============================================================
// 9-D 通勤到達性
// ============================================================

/// 戻り値: 通勤到達性指数 (0-100、9-F の合成入力)。
fn render_mi_9d_commute_reach(html: &mut String, ctx: Option<&InsightContext>) -> Option<f64> {
    html.push_str("<div class=\"block-title\">図 9-D 通勤到達性</div>\n");
    html.push_str("<p class=\"caption\">通勤流入元 TOP3 と通勤自給率を統合した通勤圏到達性指数。流入元は補助配信地域の候補です。</p>\n");

    let ctx = match ctx {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption dim\">取得値なし</p>\n");
            return None;
        }
    };

    // 流入元 TOP3 テーブル (構成比のみ。絶対値は出さない)
    let total = ctx.commute_inflow_total;
    if !ctx.commute_inflow_top3.is_empty() && total > 0 {
        html.push_str("<table class=\"table-navy\" style=\"font-size:10pt;\">\n");
        html.push_str("<thead><tr><th>順位</th><th>都道府県</th><th>市区町村</th><th>流入構成比</th></tr></thead>\n<tbody>\n");
        for (i, (pref, muni, n)) in ctx.commute_inflow_top3.iter().enumerate() {
            let share = (*n as f64) / (total as f64) * 100.0;
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><strong>{:.1}%</strong></td></tr>\n",
                i + 1,
                escape_html(pref),
                escape_html(muni),
                share
            ));
        }
        html.push_str("</tbody></table>\n");
    } else {
        html.push_str("<p class=\"caption dim\">通勤流入元データなし</p>\n");
    }

    // 到達性指数: 自給率 × 流入規模 × 通勤圏カバレッジ の合成 (0-100)
    let self_idx = (ctx.commute_self_rate * 100.0).clamp(0.0, 100.0);
    let inflow_idx = compute_inflow_intensity_index(total);
    let zone_idx = if ctx.commute_zone_count > 0 {
        ((ctx.commute_zone_count as f64) / 20.0 * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let reach = if self_idx > 0.0 || inflow_idx > 0.0 || zone_idx > 0.0 {
        Some(((self_idx + inflow_idx + zone_idx) / 3.0).clamp(0.0, 100.0))
    } else {
        None
    };

    html.push_str("<div class=\"kpi-row kpi-row-3\" style=\"margin-top:3mm;\">\n");
    push_kpi(
        html,
        "通勤自給指数",
        &format!("{:.0}", self_idx),
        "",
        "neu",
        "0-100 / 高いほど地元充足",
        false,
    );
    push_kpi(
        html,
        "流入規模指数",
        &format!("{:.0}", inflow_idx),
        "",
        "neu",
        "0-100 / 高いほど補助配信候補多い",
        false,
    );
    push_kpi(
        html,
        "通勤圏カバレッジ",
        &format!("{:.0}", zone_idx),
        "",
        "neu",
        &format!("通勤圏 {} 市区町村", ctx.commute_zone_count),
        false,
    );
    html.push_str("</div>\n");

    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:3mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">\
         流入元 TOP3 を補助配信地域として追加投下する。通勤自給指数が低い場合は外縁部訴求 (車通勤可・住宅補助) を強化する。\
         </div></div>\n",
    );

    reach
}

// ============================================================
// 9-E 生活コスト補正後給与魅力度
// ============================================================

/// 戻り値: 給与魅力度指数 (0-100、9-F の合成入力)。
fn render_mi_9e_wage_attractiveness(
    html: &mut String,
    ctx: Option<&InsightContext>,
    agg: &SurveyAggregation,
) -> Option<f64> {
    use super::super::super::super::helpers::get_f64;

    html.push_str("<div class=\"block-title\">図 9-E 生活コスト補正後給与魅力度</div>\n");
    html.push_str("<p class=\"caption\">求人給与中央値を最低賃金 / 家計支出と比較した相対魅力度 (参考指標)。生活コスト補正は概算であり、契約条件 (家賃補助 / 通勤手当) を含みません。</p>\n");

    let ctx = match ctx {
        Some(c) => c,
        None => {
            html.push_str("<p class=\"caption dim\">取得値なし</p>\n");
            return None;
        }
    };

    let min_wage = ctx
        .ext_min_wage
        .last()
        .map(|r| get_f64(r, "min_wage_hourly"))
        .filter(|v| *v > 0.0);
    let household_spending = ctx
        .ext_household_spending
        .last()
        .map(|r| get_f64(r, "monthly_spending"))
        .filter(|v| *v > 0.0);

    // agg から月給中央値 / 時給中央値を取得 (silent fallback 防御: median が 0 / 欠損は None)
    let salary_median = agg
        .enhanced_stats
        .as_ref()
        .map(|s| s.median)
        .filter(|v| *v > 0);
    let is_hourly = agg.is_hourly;

    html.push_str("<div class=\"kpi-row kpi-row-3\">\n");
    {
        let (val, unit, foot) = match salary_median {
            Some(m) if is_hourly => (
                format!("{}", m),
                "円/時",
                "求人給与 中央値".to_string(),
            ),
            Some(m) => (
                format!("{:.1}", (m as f64) / 10_000.0),
                "万円",
                "求人給与 中央値".to_string(),
            ),
            None => ("—".to_string(), "", "データなし".to_string()),
        };
        push_kpi(html, "求人給与", &val, unit, "neu", &foot, true);
    }
    {
        let (val, foot) = match min_wage {
            Some(v) => (format!("{:.0}", v), "最低賃金 (円/時)".to_string()),
            None => ("—".to_string(), "データなし".to_string()),
        };
        push_kpi(html, "最低賃金", &val, "円", "neu", &foot, false);
    }
    {
        // 給与プレミアム指数: 求人時給 / 最低賃金 を 100 が等価とした指数
        let (val, foot, dot) = match (salary_median, min_wage) {
            (Some(m), Some(mw)) => {
                let hourly = if is_hourly {
                    m as f64
                } else {
                    (m as f64) / 160.0
                };
                let premium = (hourly / mw * 100.0).clamp(0.0, 200.0);
                let (d, f) = if premium >= 130.0 {
                    ("pos", "最賃比 +30% 以上 (高プレミアム)")
                } else if premium >= 110.0 {
                    ("neu", "最賃比 +10-30% (標準帯)")
                } else {
                    ("warn", "最賃比 +10% 未満 (薄い)")
                };
                (format!("{:.0}", premium), f.to_string(), d)
            }
            _ => ("—".to_string(), "データなし".to_string(), "neu"),
        };
        push_kpi(html, "給与プレミアム指数", &val, "", dot, &foot, false);
    }
    html.push_str("</div>\n");

    // 家計支出比較 (参考表示、絶対値は万円単位の構成比的に出す)
    if let Some(spending) = household_spending {
        html.push_str(&format!(
            "<p class=\"caption\" style=\"margin-top:3mm;\">家計支出 (参考): 月額 <strong>{:.1} 万円</strong> 相当。\
             求人給与中央値との生活コスト補正は今後の commit で本格対応予定です。</p>\n",
            spending / 10_000.0
        ));
    }

    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:3mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">\
         給与プレミアム指数が 110 未満の場合は家賃補助・通勤手当・賞与等の付帯条件を訴求に追加する。\
         本指標は (参考) 値であり、契約条件全体を反映しません。\
         </div></div>\n",
    );

    // 給与魅力度指数を返す (合成用)
    match (salary_median, min_wage) {
        (Some(m), Some(mw)) => {
            let hourly = if is_hourly {
                m as f64
            } else {
                (m as f64) / 160.0
            };
            let premium = (hourly / mw * 100.0).clamp(0.0, 200.0);
            // 80 (= 最賃以下) を 0、100 (等価) を 25、160 以上を 100。
            Some(((premium - 80.0) / 80.0 * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    }
}

// ============================================================
// 9-F 配信シナリオ濃淡バー
// ============================================================

fn render_mi_9f_scenario_intensity(
    html: &mut String,
    positive_score: Option<f64>,
    competition_index: Option<f64>,
    commute_reach_index: Option<f64>,
    wage_attractiveness_index: Option<f64>,
    ctx: Option<&InsightContext>,
) {
    let _ = ctx; // 将来 Turso v2_municipality_target_thickness 接続時に使用

    html.push_str(
        "<div class=\"block-title\">図 9-F 配信シナリオ濃淡 (保守 / 標準 / 強気)</div>\n",
    );
    html.push_str("<p class=\"caption\">配信予算配分の意思決定材料となる 3 段階濃淡。数値は指数 (0-100) です。応募見込数の換算は行いません。</p>\n");

    let (cons, std_idx, agg_idx) = compute_scenario_indices(
        positive_score,
        competition_index,
        commute_reach_index,
        wage_attractiveness_index,
    );

    html.push_str("<table class=\"table-navy\" style=\"font-size:10pt;\">\n");
    html.push_str("<thead><tr><th>シナリオ</th><th>濃淡 (推定)</th><th>指数</th><th>意思決定</th></tr></thead>\n<tbody>\n");
    for (name, idx, decision) in &[
        ("保守", cons, "既存経験者・近接地域中心。低リスク投下。"),
        ("標準", std_idx, "通勤圏 + 近接職種を含む標準見積。"),
        (
            "強気",
            agg_idx,
            "未経験歓迎・外縁部配信を広げたテスト投下。",
        ),
    ] {
        let bar = render_intensity_bar(*idx);
        let val = match *idx {
            Some(v) => format!("{:.0}", v),
            None => "—".to_string(),
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td><strong>{}</strong></td><td>{}</td></tr>\n",
            name, bar, val, decision
        ));
    }
    html.push_str("</tbody></table>\n");

    // 不変条件チェック (METRICS.md §9 制約: 保守 <= 標準 <= 強気)
    if let (Some(c), Some(s), Some(a)) = (cons, std_idx, agg_idx) {
        if !(c <= s && s <= a) {
            html.push_str(
                "<p class=\"caption dim\" style=\"color:#b45309;\">\
                 ⚠ シナリオ濃淡の単調性 (保守 ≤ 標準 ≤ 強気) を満たしていません。データ更新待ち。\
                 </p>\n",
            );
        }
    }

    html.push_str(
        "<div class=\"so-what\" style=\"margin-top:3mm;\">\
         <div class=\"sw-label\">SO WHAT</div>\
         <div class=\"sw-body\">\
         配信予算を保守/標準/強気の 3 段階で分散し、強気シナリオは外縁部・近接職種向けにテスト投下する。\
         本濃淡は (推定) であり、応募見込数を保証するものではありません。\
         </div></div>\n",
    );
}

/// 9-A〜9-E の入力から (保守, 標準, 強気) 3 指数を導出 (0-100)。
///
/// METRICS.md §2.1 に準拠した近似:
/// - 標準 = base × (1 - penalty/100), penalty = competition_index を 0-30 に scale
/// - 保守 = 標準 × 0.5
/// - 強気 = min(標準 × 1.6, 100)
///
/// base = positive_score, commute_reach_index, wage_attractiveness_index の単純平均 (None 除外)
fn compute_scenario_indices(
    positive_score: Option<f64>,
    competition_index: Option<f64>,
    commute_reach_index: Option<f64>,
    wage_attractiveness_index: Option<f64>,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let mut total = 0.0;
    let mut n = 0.0;
    for v in [positive_score, commute_reach_index, wage_attractiveness_index]
        .iter()
        .flatten()
    {
        total += v;
        n += 1.0;
    }
    let base = if n > 0.0 { Some(total / n) } else { None };

    let penalty = competition_index
        .map(|v| (v / 100.0 * 30.0).clamp(0.0, 30.0))
        .unwrap_or(0.0);

    let standard = base.map(|b| (b * (1.0 - penalty / 100.0)).clamp(0.0, 100.0));
    let conservative = standard.map(|s| (s * 0.5).clamp(0.0, 100.0));
    let aggressive = standard.map(|s| (s * 1.6).clamp(0.0, 100.0));

    (conservative, standard, aggressive)
}

/// 0-100 指数を 12 段階の ▆ バーで可視化 (絶対値表示禁止のため CSS バー方式)。
fn render_intensity_bar(idx: Option<f64>) -> String {
    let v = match idx {
        Some(v) => v,
        None => return "—".to_string(),
    };
    let filled = ((v / 100.0) * 12.0).round() as usize;
    let filled = filled.min(12);
    let mut s = String::new();
    for _ in 0..filled {
        s.push('▆');
    }
    for _ in filled..12 {
        s.push('▁');
    }
    s
}

// ============================================================
// Tests (5 件 ≥ 要件)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_priority_label_classifies_scores() {
        // DISPLAY_SPEC §3.2 4 段階閾値: 80+ / 65-79 / 50-64 / 0-49
        assert_eq!(compute_priority_label(Some(85.0)), "重点配信");
        assert_eq!(compute_priority_label(Some(80.0)), "重点配信"); // 境界
        assert_eq!(compute_priority_label(Some(79.9)), "拡張候補");
        assert_eq!(compute_priority_label(Some(65.0)), "拡張候補"); // 境界
        assert_eq!(compute_priority_label(Some(50.0)), "維持/検証"); // 境界
        assert_eq!(compute_priority_label(Some(49.9)), "優先度低");
        assert_eq!(compute_priority_label(Some(0.0)), "優先度低");
        assert_eq!(compute_priority_label(None), "判定不能");
    }

    #[test]
    fn compute_inflow_intensity_index_handles_zero_and_max() {
        assert_eq!(compute_inflow_intensity_index(0), 0.0);
        assert_eq!(compute_inflow_intensity_index(-100), 0.0);
        // 10,000 ≒ log10(10,001) / 5 * 100 ≒ 80.0
        let v = compute_inflow_intensity_index(10_000);
        assert!(v > 70.0 && v < 90.0, "expected 70-90, got {}", v);
        // 100,000 → 100
        let max = compute_inflow_intensity_index(100_000);
        assert!((95.0..=100.0).contains(&max));
    }

    #[test]
    fn compute_scenario_indices_monotonic_when_inputs_valid() {
        // 保守 ≤ 標準 ≤ 強気 を満たすこと (METRICS §9 制約)
        let (c, s, a) = compute_scenario_indices(Some(80.0), Some(20.0), Some(70.0), Some(60.0));
        let c = c.unwrap();
        let s = s.unwrap();
        let a = a.unwrap();
        assert!(c <= s, "conservative ({}) <= standard ({}) must hold", c, s);
        assert!(s <= a, "standard ({}) <= aggressive ({}) must hold", s, a);
        assert!((0.0..=100.0).contains(&c));
        assert!((0.0..=100.0).contains(&s));
        assert!((0.0..=100.0).contains(&a));
    }

    #[test]
    fn compute_scenario_indices_returns_none_when_all_inputs_none() {
        let (c, s, a) = compute_scenario_indices(None, None, None, None);
        assert!(c.is_none());
        assert!(s.is_none());
        assert!(a.is_none());
    }

    #[test]
    fn section_09_full_variant_outputs_nothing() {
        // Full variant では関数単体で呼ばれても、内部防御ガードで何も出力しない
        let mut html = String::new();
        let agg = SurveyAggregation::default();
        render_navy_section_09_market_intelligence(
            &mut html,
            None,
            &agg,
            ReportVariant::Full,
            "東京都",
        );
        assert!(
            html.is_empty(),
            "Full variant で内部ガード突破: {} bytes",
            html.len()
        );
    }

    #[test]
    fn section_09_public_variant_outputs_nothing() {
        let mut html = String::new();
        let agg = SurveyAggregation::default();
        render_navy_section_09_market_intelligence(
            &mut html,
            None,
            &agg,
            ReportVariant::Public,
            "東京都",
        );
        assert!(html.is_empty(), "Public variant で内部ガード突破");
    }

    #[test]
    fn section_09_mi_variant_outputs_section_tag() {
        // MI variant では hw_context = None でも section タグと navy-mi クラスを出力する
        let mut html = String::new();
        let agg = SurveyAggregation::default();
        render_navy_section_09_market_intelligence(
            &mut html,
            None,
            &agg,
            ReportVariant::MarketIntelligence,
            "東京都 港区",
        );
        assert!(html.contains("navy-mi"), "navy-mi クラスが出力されていない");
        assert!(html.contains("SECTION 09"), "SECTION 09 ラベル欠落");
        assert!(
            html.contains("採用マーケットインテリジェンス"),
            "セクションタイトル欠落"
        );
    }

    #[test]
    fn section_09_does_not_emit_population_numbers() {
        // DISPLAY_SPEC v1.0 §2.1 ハード NG: 「○○万人」「○○億円」を一切出力しない。
        // 「○○人」「○○名」は熟語 (人口/人材/人数/人件) との衝突を避けるため、
        // 数字直結のみ厳格に検査する。MI variant + hw_context = None + agg default で
        // 出力される placeholder + KPI フォールバックを対象に逆証明。
        let mut html = String::new();
        let agg = SurveyAggregation::default();
        render_navy_section_09_market_intelligence(
            &mut html,
            None,
            &agg,
            ReportVariant::MarketIntelligence,
            "東京都 港区",
        );

        // 完全禁止パターン (DISPLAY_SPEC §2.1)
        let forbidden_patterns: &[&str] = &["万人", "億円", "万円採用", "○人見込み"];
        for pat in forbidden_patterns {
            assert!(
                !html.contains(pat),
                "Hard NG パターン '{}' が出力に含まれている (DISPLAY_SPEC §2.1)",
                pat
            );
        }

        // 数字 + 「人」「名」の直結パターン検出 (熟語との衝突回避)
        // 「人口」「人材」「人数」「人件」直前の数字は許容、それ以外は NG。
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_digit() {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b',') {
                    i += 1;
                }
                // i 以降に「人」/「名」が直結?
                if i + 3 <= bytes.len() {
                    let next_char = &bytes[i..i + 3];
                    if next_char == [0xE4, 0xBA, 0xBA] {
                        // 「人」 = E4 BA BA
                        // 次の char が「口」「材」「数」「件」なら許容
                        let allow = if i + 6 <= bytes.len() {
                            let after = &bytes[i + 3..i + 6];
                            after == [0xE5, 0x8F, 0xA3]  // 口
                                || after == [0xE6, 0x9D, 0x90]  // 材
                                || after == [0xE6, 0x95, 0xB0]  // 数
                                || after == [0xE4, 0xBB, 0xB6] // 件
                        } else {
                            true
                        };
                        if !allow {
                            let digits = std::str::from_utf8(&bytes[start..i]).unwrap_or("?");
                            panic!(
                                "Hard NG: 数字 + 「人」直結パターン検出 ('{} 人') — DISPLAY_SPEC §2.1 違反",
                                digits
                            );
                        }
                    } else if next_char == [0xE5, 0x90, 0x8D] {
                        // 「名」 = E5 90 8D
                        let digits = std::str::from_utf8(&bytes[start..i]).unwrap_or("?");
                        panic!(
                            "Hard NG: 数字 + 「名」直結パターン検出 ('{} 名') — DISPLAY_SPEC §2.1 違反",
                            digits
                        );
                    }
                }
            } else {
                i += 1;
            }
        }
    }
}
