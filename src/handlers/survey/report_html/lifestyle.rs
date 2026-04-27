//! 媒体分析印刷レポート: ライフスタイル特性 section
//!
//! Impl-3 (2026-04-26) 追加。担当 3 案のうち 2 案を担当する:
//!
//! - **P-1: 社会生活参加率 (v2_external_social_life)**
//!   実スキーマは `(prefecture, category, subcategory, participation_rate, survey_year)` で
//!   category は「趣味・娯楽」「スポーツ」「ボランティア活動」「学習・自己啓発」の 4 種。
//!   仕様書には「commute_time_median」と書かれているが実テーブルには存在しないため
//!   (`feedback_never_guess_data.md` 準拠)、実カテゴリの participation_rate を労働者
//!   ライフスタイル指標として表示する。
//!
//! - **P-2: ネット利用率 (v2_external_internet_usage)**
//!   実スキーマは `(prefecture, internet_usage_rate, smartphone_ownership_rate, year, ...)`。
//!   仕様書の sns_usage_rate は実テーブルに存在しないため smartphone_ownership_rate を
//!   代替指標として併記する。
//!
//! #8 (世帯所得 vs 給与) は wage.rs に配置 (給与関連の流れ)。
//!
//! 配置: report_html/mod.rs から「Section 8B: ライフスタイル特性」として呼び出される
//!       (Section 8: 最低賃金比較 と Section 9: 企業分析 の間)。
//!
//! memory ルール:
//! - `feedback_correlation_not_causation.md`: 「Indeed 適合度」等は相関注記必須
//! - `feedback_hw_data_scope.md`: 各 KPI に出典注記
//! - `feedback_test_data_validation.md` / `feedback_reverse_proof_tests.md`:
//!   逆証明テストでは「KPI が画面に出る」だけでなく「具体値・しきい値ラベル」を検証

#![allow(unused_imports, dead_code)]

use super::super::super::helpers::{escape_html, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;

use super::helpers::*;

/// 「ライフスタイル特性」section 全体を描画
///
/// `hw_context` が None の場合、または social_life / internet_usage が両方空の場合、
/// section ごと出力しない（既存セクション設計を踏襲、空白セクション抑制）。
pub(super) fn render_section_lifestyle(html: &mut String, hw_context: Option<&InsightContext>) {
    let ctx = match hw_context {
        Some(c) => c,
        None => return,
    };

    let has_social = !ctx.ext_social_life.is_empty();
    let has_internet = !ctx.ext_internet_usage.is_empty();
    if !has_social && !has_internet {
        return;
    }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>ライフスタイル特性</h2>\n");

    // セクション冒頭の「読み方」吹き出し（仕様書共通設計）
    html.push_str(&render_reading_callout(
        "対象地域の労働者ライフスタイル特性（余暇活動・デジタル利用）と求人媒体適合性を確認します。\
         数値はあくまで地域全体の傾向であり、個別求人の応募行動を予測するものではありません。",
    ));

    // ---------------- P-1: 社会生活参加率 ----------------
    if has_social {
        render_social_life_block(html, ctx);
    }

    // ---------------- P-2: ネット利用率 ----------------
    if has_internet {
        render_internet_usage_block(html, ctx);
    }

    html.push_str("</div>\n");
}

// =====================================================================
// P-1: 社会生活参加率
// =====================================================================

/// 社会生活基本調査の category × participation_rate を KPI カードとして表示
fn render_social_life_block(html: &mut String, ctx: &InsightContext) {
    html.push_str("<h3>地域住民のオフ活動 参加率（社会生活基本調査）</h3>\n");
    render_figure_caption(
        html,
        "図 8B-1",
        "社会生活参加率（趣味・スポーツ・ボランティア・学習）",
    );

    // category ごとに participation_rate を取得し、ソート (高い順)
    // category は 4 種類: 趣味・娯楽 / スポーツ / ボランティア活動 / 学習・自己啓発
    let mut entries: Vec<(String, f64, String)> = Vec::new();
    for row in &ctx.ext_social_life {
        let category = get_str_ref(row, "category").to_string();
        let rate = get_f64(row, "participation_rate");
        if category.is_empty() || rate <= 0.0 {
            continue;
        }
        // 同一 category 内に subcategory が複数ある場合は最大値を採用
        let icon = category_to_icon(&category);
        if let Some(existing) = entries.iter_mut().find(|(c, _, _)| c == &category) {
            if rate > existing.1 {
                existing.1 = rate;
            }
        } else {
            entries.push((category, rate, icon.to_string()));
        }
    }

    if entries.is_empty() {
        html.push_str(
            "<p class=\"note\" style=\"color:#888;font-size:9pt;\">\
             社会生活参加率データがこの地域では取得できませんでした。\
             </p>\n",
        );
        return;
    }

    // 参加率の高い順
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // KPI カードグリッド
    html.push_str("<div class=\"stats-grid\">\n");
    for (category, rate, icon) in &entries {
        let label = format!("{} {}", icon, category);
        let value = format!("{:.1}%", rate);
        // 参加率は 0-100% で意思決定影響度がカテゴリ毎に異なるため、
        // 単純なしきい値判定でなく観測値として提示
        render_stat_box(html, &label, &value);
    }
    html.push_str("</div>\n");

    // 解釈ヒント (相関 ≠ 因果準拠)
    render_read_hint(
        html,
        "参加率が高いカテゴリは、求人広告で「働きやすさ」「ワークライフバランス」「研修制度」などの \
         訴求が地域住民の関心と整合する可能性があります。\
         参加率と採用容易性の間に直接の因果関係はなく、媒体の訴求軸選定の参考としてご利用ください。",
    );

    // 必須注記 (feedback_hw_data_scope.md 準拠)
    // 2026-04-26 Granularity: 都道府県粒度のみであることを強調
    html.push_str(
        "<p class=\"note\" style=\"font-size:9pt;color:#b45309;background:#fef3c7;padding:6px 8px;border-left:3px solid #f59e0b;border-radius:3px;margin:6px 0;\">\
         <strong>⚠ 都道府県粒度の参考値:</strong> 社会生活基本調査 2021 ベース（総務省統計局、5 年に 1 回）。\
         本データは <strong>都道府県+政令市</strong> のみで、市区町村別の差は反映されていません。\
         CSV の主要市区町村が複数都道府県にまたがる場合、都道府県平均が必ずしも \
         実際の対象地域を代表しないため、参考値としてご利用ください。\
         participation_rate は 10 歳以上人口の自己申告。\
         </p>\n",
    );
}

/// category 名から表示用 icon (絵文字禁止のためテキスト記号) を返す
fn category_to_icon(category: &str) -> &'static str {
    if category.contains("スポーツ") {
        "[SP]"
    } else if category.contains("趣味") {
        "[HB]"
    } else if category.contains("ボランティア") {
        "[VL]"
    } else if category.contains("学習") || category.contains("自己啓発") {
        "[LN]"
    } else {
        "[--]"
    }
}

// =====================================================================
// P-2: ネット利用率
// =====================================================================

/// internet_usage_rate + smartphone_ownership_rate を KPI として表示し、
/// 求人媒体適合度ラベル（高 / 中 / 低）を付与する
fn render_internet_usage_block(html: &mut String, ctx: &InsightContext) {
    html.push_str("<h3>デジタル利用状況（通信利用動向調査）</h3>\n");
    render_figure_caption(
        html,
        "図 8B-2",
        "ネット利用率 / スマートフォン保有率と求人媒体適合度",
    );

    // 行は通常 1 件 (prefecture 指定時) 想定
    let row = match ctx.ext_internet_usage.first() {
        Some(r) => r,
        None => return,
    };

    let internet_rate = get_f64(row, "internet_usage_rate");
    let smartphone_rate = get_f64(row, "smartphone_ownership_rate");
    let year = super::super::super::helpers::get_i64(row, "year");

    if internet_rate <= 0.0 && smartphone_rate <= 0.0 {
        html.push_str(
            "<p class=\"note\" style=\"color:#888;font-size:9pt;\">\
             ネット利用率データがこの地域では取得できませんでした。\
             </p>\n",
        );
        return;
    }

    // 適合度判定（仕様書: SNS 利用率 ≥75% 高 / 60-75% 中 / <60% 低）
    // 実データに sns_usage_rate が存在しないため、internet_usage_rate を主指標として同しきい値で判定。
    // memory feedback_correlation_not_causation.md 準拠で「相関であり因果ではない」を明記。
    let (fit_label, fit_class) = classify_online_media_fit(internet_rate);

    // 強化版 KPI カード (kpi-card-v2 が利用可能だが、stat-box で揃える)
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(
        html,
        "インターネット利用率",
        &format!("{:.1}%", internet_rate),
    );
    if smartphone_rate > 0.0 {
        render_stat_box(
            html,
            "スマートフォン保有率",
            &format!("{:.1}%", smartphone_rate),
        );
    }
    render_stat_box(html, "オンライン媒体 適合度", fit_label);
    html.push_str("</div>\n");

    // 適合度の根拠 (severity badge を流用)
    let sev = match fit_class {
        OnlineMediaFit::High => RptSev::Positive,
        OnlineMediaFit::Mid => RptSev::Info,
        OnlineMediaFit::Low => RptSev::Warning,
    };
    html.push_str(&format!(
        "<p style=\"margin:8px 0;font-size:10pt;\">\
         {} <strong>オンライン求人媒体への適合度: {}</strong> \
         <span style=\"font-size:9pt;color:#666;\">\
         （閾値: ≥75% 高 / 60-75% 中 / &lt;60% 低、internet_usage_rate ベース）\
         </span></p>\n",
        severity_badge(sev),
        escape_html(fit_label),
    ));

    // 解釈ヒント (相関 ≠ 因果準拠)
    render_read_hint(
        html,
        "ネット利用率はオンライン求人媒体への露出ポテンシャルの一指標です。\
         利用率が高いほどオンライン媒体経由の応募者層が広がる傾向が観測されますが、\
         利用率と応募実績の間に直接の因果関係はなく、媒体出稿の判断材料の 1 つとしてご利用ください。",
    );

    // 必須注記
    let year_str = if year > 0 {
        format!("{} 年", year)
    } else {
        "最新".to_string()
    };
    // 2026-04-26 Granularity: 都道府県粒度のみであることを強調
    html.push_str(&format!(
        "<p class=\"note\" style=\"font-size:9pt;color:#b45309;background:#fef3c7;padding:6px 8px;border-left:3px solid #f59e0b;border-radius:3px;margin:6px 0;\">\
         <strong>⚠ 都道府県粒度の参考値:</strong> 通信利用動向調査 {} ベース（総務省）。\
         本データは <strong>都道府県のみ</strong> で、市区町村別の差は反映されていません。\
         インターネット利用率は 6 歳以上人口の自己申告。\
         スマートフォン保有率は世帯単位での自己申告。\
         オンライン媒体適合度は当該都道府県全体の平均値であり、対象市区町村の実態とは乖離する可能性があります。\
         </p>\n",
        escape_html(&year_str),
    ));
}

/// オンライン媒体適合度の 3 段階分類
#[derive(Clone, Copy)]
pub(super) enum OnlineMediaFit {
    High,
    Mid,
    Low,
}

/// internet_usage_rate からオンライン媒体適合度ラベルを返す
/// しきい値: ≥75% 高 / 60-75% 中 / <60% 低
pub(super) fn classify_online_media_fit(rate: f64) -> (&'static str, OnlineMediaFit) {
    if rate >= 75.0 {
        ("高", OnlineMediaFit::High)
    } else if rate >= 60.0 {
        ("中", OnlineMediaFit::Mid)
    } else {
        ("低", OnlineMediaFit::Low)
    }
}

// =====================================================================
// テスト (Impl-3 担当: 逆証明テスト 6 件以上)
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    /// 適合度しきい値の境界テスト (逆証明: 同じ rate で同じラベルが返る/異なる rate で異なるラベル)
    #[test]
    fn test_classify_online_media_fit_thresholds() {
        // High 境界
        assert_eq!(classify_online_media_fit(75.0).0, "高");
        assert_eq!(classify_online_media_fit(92.0).0, "高");
        // Mid 境界
        assert_eq!(classify_online_media_fit(60.0).0, "中");
        assert_eq!(classify_online_media_fit(74.9).0, "中");
        // Low 境界
        assert_eq!(classify_online_media_fit(59.9).0, "低");
        assert_eq!(classify_online_media_fit(0.0).0, "低");
    }

    /// 3 段階のラベルが互いに異なる (逆証明: distinct outputs)
    #[test]
    fn test_classify_online_media_fit_distinct() {
        let high = classify_online_media_fit(80.0).0;
        let mid = classify_online_media_fit(65.0).0;
        let low = classify_online_media_fit(50.0).0;
        assert_ne!(high, mid);
        assert_ne!(mid, low);
        assert_ne!(high, low);
    }

    fn make_row(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    /// hw_context = None で section が出力されない (空表示抑制)
    #[test]
    fn test_lifestyle_section_skipped_when_no_context() {
        let mut html = String::new();
        render_section_lifestyle(&mut html, None);
        assert!(html.is_empty(), "hw_context=None で section 非出力");
    }

    /// category アイコンマッピング: 4 カテゴリすべてが個別 icon を持つ (逆証明)
    #[test]
    fn test_category_to_icon_distinct_per_category() {
        let sport = category_to_icon("スポーツ");
        let hobby = category_to_icon("趣味・娯楽");
        let volunteer = category_to_icon("ボランティア活動");
        let learning = category_to_icon("学習・自己啓発");
        let unknown = category_to_icon("不明カテゴリ");

        assert_eq!(sport, "[SP]");
        assert_eq!(hobby, "[HB]");
        assert_eq!(volunteer, "[VL]");
        assert_eq!(learning, "[LN]");
        assert_eq!(unknown, "[--]");

        // 4 カテゴリすべてが互いに異なる
        let icons = [sport, hobby, volunteer, learning];
        let mut sorted = icons.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "4 カテゴリの icon は重複しない");
    }

    /// build_insight_context モックで P-2 ネット利用率セクションが
    /// 正確な数値・適合度・必須注記を含むこと（逆証明: 具体値を assert）
    #[test]
    fn test_internet_usage_block_emits_concrete_values_and_high_fit() {
        // 東京都想定: internet 92%, smartphone 80%
        let row = make_row(&[
            ("prefecture", json!("東京都")),
            ("internet_usage_rate", json!(92.0)),
            ("smartphone_ownership_rate", json!(80.0)),
            ("year", json!(2023)),
        ]);
        let ctx = mock_ctx_with_internet(vec![row]);

        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));

        // セクション出力確認
        assert!(html.contains("ライフスタイル特性"), "h2 タイトル");
        assert!(html.contains("デジタル利用状況"), "h3 タイトル");
        assert!(html.contains("図 8B-2"), "図番号");

        // 具体値: 92.0% / 80.0%
        assert!(html.contains("92.0%"), "internet_usage_rate 値が表示");
        assert!(html.contains("80.0%"), "smartphone_ownership_rate 値が表示");

        // 適合度: 高 (>=75%)
        assert!(html.contains("適合度: 高"), "適合度ラベルが「高」");

        // しきい値ガイド表示
        assert!(html.contains("≥75% 高"), "しきい値ガイド");

        // 必須注記 (year=2023 のため「2023 年 ベース」と組み立てられる)
        assert!(
            html.contains("通信利用動向調査") && html.contains("2023") && html.contains("ベース"),
            "必須注記: 通信利用動向調査 2023 ベース"
        );
        assert!(
            html.contains("6 歳以上人口の自己申告"),
            "必須注記: 自己申告"
        );

        // 相関注記 (memory feedback_correlation_not_causation 準拠)
        assert!(
            html.contains("因果関係はなく") || html.contains("因果関係を示すものではありません"),
            "相関≠因果の注記"
        );
    }

    /// 低適合度地域: rate <60% で適合度ラベルが「低」になる (逆証明: しきい値外側)
    #[test]
    fn test_internet_usage_block_low_fit_label() {
        let row = make_row(&[
            ("prefecture", json!("某県")),
            ("internet_usage_rate", json!(55.0)),
            ("smartphone_ownership_rate", json!(50.0)),
            ("year", json!(2023)),
        ]);
        let ctx = mock_ctx_with_internet(vec![row]);

        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));

        assert!(html.contains("適合度: 低"), "rate=55%, 60% 未満で適合度=低");
    }

    /// social_life セクション: category 別 KPI 値が表示され、必須注記を含む
    #[test]
    fn test_social_life_block_emits_categories_with_concrete_values() {
        let rows = vec![
            make_row(&[
                ("prefecture", json!("東京都")),
                ("category", json!("趣味・娯楽")),
                ("subcategory", json!("")),
                ("participation_rate", json!(78.5)),
                ("survey_year", json!(2021)),
            ]),
            make_row(&[
                ("prefecture", json!("東京都")),
                ("category", json!("スポーツ")),
                ("subcategory", json!("")),
                ("participation_rate", json!(65.2)),
                ("survey_year", json!(2021)),
            ]),
            make_row(&[
                ("prefecture", json!("東京都")),
                ("category", json!("学習・自己啓発")),
                ("subcategory", json!("")),
                ("participation_rate", json!(45.0)),
                ("survey_year", json!(2021)),
            ]),
            make_row(&[
                ("prefecture", json!("東京都")),
                ("category", json!("ボランティア活動")),
                ("subcategory", json!("")),
                ("participation_rate", json!(20.3)),
                ("survey_year", json!(2021)),
            ]),
        ];
        let ctx = mock_ctx_with_social_life(rows);

        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));

        // セクション
        assert!(html.contains("地域住民のオフ活動"), "h3 タイトル");
        assert!(html.contains("図 8B-1"), "図番号");

        // 4 カテゴリすべて表示
        assert!(html.contains("趣味・娯楽"));
        assert!(html.contains("スポーツ"));
        assert!(html.contains("学習・自己啓発"));
        assert!(html.contains("ボランティア活動"));

        // 具体値（小数点表示）
        assert!(html.contains("78.5%"), "趣味の participation_rate");
        assert!(html.contains("65.2%"), "スポーツの participation_rate");
        assert!(html.contains("45.0%"), "学習の participation_rate");
        assert!(html.contains("20.3%"), "ボランティアの participation_rate");

        // 必須注記
        assert!(
            html.contains("社会生活基本調査 2021 ベース"),
            "出典注記: 社会生活基本調査 2021"
        );

        // 相関注記
        assert!(
            html.contains("因果関係はなく") || html.contains("因果関係を示すものではありません"),
            "相関≠因果"
        );
    }

    /// 両方のデータが空なら section 自体が出力されない (空白セクション抑制)
    #[test]
    fn test_lifestyle_section_skipped_when_both_empty() {
        let ctx = mock_ctx_with_internet(vec![]);
        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));
        assert!(
            html.is_empty(),
            "social_life / internet_usage 両方空なら section 非出力"
        );
    }

    /// 2026-04-26 Granularity: 都道府県粒度警告が強化されていること (social_life)
    #[test]
    fn granularity_lifestyle_social_life_pref_only_warning_strengthened() {
        let rows = vec![make_row(&[
            ("prefecture", json!("東京都")),
            ("category", json!("趣味・娯楽")),
            ("subcategory", json!("")),
            ("participation_rate", json!(70.0)),
            ("survey_year", json!(2021)),
        ])];
        let ctx = mock_ctx_with_social_life(rows);
        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));

        // 強化された警告: 「都道府県粒度の参考値」「市区町村別の差は反映されていません」
        assert!(
            html.contains("都道府県粒度の参考値"),
            "lifestyle social_life: 都道府県粒度の警告強化必須"
        );
        assert!(
            html.contains("市区町村別の差は反映されていません"),
            "lifestyle social_life: 市区町村別差の注記必須"
        );
    }

    /// 2026-04-26 Granularity: 都道府県粒度警告が強化されていること (internet_usage)
    #[test]
    fn granularity_lifestyle_internet_usage_pref_only_warning_strengthened() {
        let rows = vec![make_row(&[
            ("prefecture", json!("東京都")),
            ("internet_usage_rate", json!(85.0)),
            ("smartphone_ownership_rate", json!(70.0)),
            ("year", json!(2023)),
        ])];
        let ctx = mock_ctx_with_internet(rows);
        let mut html = String::new();
        render_section_lifestyle(&mut html, Some(&ctx));

        assert!(
            html.contains("都道府県粒度の参考値"),
            "lifestyle internet_usage: 都道府県粒度の警告強化必須"
        );
        assert!(
            html.contains("市区町村別の差は反映されていません")
                || html.contains("対象市区町村の実態とは乖離する可能性"),
            "lifestyle internet_usage: 市区町村別差の注記必須"
        );
    }

    // ---------- mock builders ----------

    fn mock_ctx_with_internet(
        internet_rows: Vec<HashMap<String, serde_json::Value>>,
    ) -> InsightContext {
        empty_ctx_with(|c| {
            c.ext_internet_usage = internet_rows;
        })
    }

    fn mock_ctx_with_social_life(
        social_rows: Vec<HashMap<String, serde_json::Value>>,
    ) -> InsightContext {
        empty_ctx_with(|c| {
            c.ext_social_life = social_rows;
        })
    }

    fn empty_ctx_with<F: FnOnce(&mut InsightContext)>(modifier: F) -> InsightContext {
        let mut ctx = InsightContext {
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
            ext_household_spending: vec![],
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
        };
        modifier(&mut ctx);
        ctx
    }
}
