//! 職種推定グループ別 給与参考クロス表 (Round 3-C / 2026-05-09)
//!
//! ## 背景
//! Round 1-E 完全欠落 Top 1「職種×給与」と Round 2-4 真の未実装 #7 を消化。
//! Round 3-A (産業構成 e-Stat) + Round 3-B (業界推定×給与参考 CSV) に続き、
//! 職種粒度の給与水準表を MI PDF に追加する。
//!
//! ## 設計方針
//! - **B 案採用**: per-record 職種コード / 標準化 occupation が `SurveyAggregation`
//!   に存在しないため、`by_tag_salary` (主信号) と `by_company` (補助) のキーワード
//!   からの職種推定。「職種別」断定は科学的根拠なし → 「職種推定グループ」「参考」必須。
//! - **既存集計の再利用**: Round 3-B `industry_salary` と同パターン。新規数値ロジック
//!   なし、`aggregator` 経路で既に正規化済の `avg_salary` / `median_salary` を再利用。
//! - **MI variant 専用**: `mod.rs` で MI variant のみ呼び出し、Full / Public 不変。
//! - **件数 < 3 は「参考 (低信頼)」**: 推定誤差・サンプル不足を明示。
//!
//! ## 関連 memory ルール
//! - `feedback_correlation_not_causation.md` 「相関≠因果」: caveat に明記
//! - `feedback_neutral_expression_for_targets.md` 「中立表現」: 評価語禁止
//! - Hard NG 13 用語 + HW 連想語不混入

use super::super::aggregator::SurveyAggregation;
use super::super::super::helpers::{escape_html, format_number};
use super::helpers::{render_figure_caption, render_read_hint, render_section_howto};

/// 職種推定グループ別 給与参考 1 行分の集計結果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OccupationSalaryRow {
    /// 職種推定グループ名 (例: "看護系", "介護系")
    pub occupation: String,
    /// 推定された求人件数
    pub count: i64,
    /// 件数加重平均給与 (ネイティブ単位、円)
    pub weighted_avg: i64,
    /// median 提示用 (`CompanyAgg.median_salary` の中央値、円)
    pub median_of_company_medians: Option<i64>,
    /// "月給" / "時給"
    pub unit_label: &'static str,
    /// 「参考 (低信頼)」/ 「-」 等
    pub note: &'static str,
}

#[derive(Debug, Default)]
struct OccupationBucket {
    count: i64,
    sum_weighted_avg: i64,
    company_medians: Vec<i64>,
}

/// 求人タイトル / タグ / 会社名から職種推定グループを判定する.
///
/// `industry_mismatch::map_keyword_to_major_industry` (12 産業大分類) とは別軸で、
/// 医療福祉を 6 系に細分化した 10 グループ + 未分類。
pub(crate) fn map_keyword_to_occupation_group(s: &str) -> Option<&'static str> {
    let s = s.to_lowercase();
    // 看護系
    if s.contains("看護") || s.contains("准看") || s.contains("ナース") {
        return Some("看護系");
    }
    // リハビリ系 (理学療法士 / 作業療法士 / 言語聴覚士)
    if s.contains("理学療法") || s.contains("作業療法") || s.contains("言語聴覚")
        || s.contains("リハビリ") || s.contains("リハ職")
        || (s.contains("pt") && !s.contains("apt")) // PT は理学療法士略称
        || s.contains("ｐｔ")
    {
        return Some("リハビリ系");
    }
    // 医療技術系 (薬剤師 / 検査技師 / 栄養士 / 歯科衛生士)
    if s.contains("薬剤") || s.contains("検査技師") || s.contains("放射線")
        || s.contains("レントゲン") || s.contains("栄養士")
        || s.contains("歯科衛生士") || s.contains("臨床工学")
    {
        return Some("医療技術系");
    }
    // 福祉相談系 (相談員 / ケアマネ / 社会福祉士)
    if s.contains("相談員") || s.contains("ケアマネ") || s.contains("社会福祉士")
        || s.contains("精神保健福祉") || s.contains("ソーシャルワーカー")
        || s.contains("生活支援員")
    {
        return Some("福祉相談系");
    }
    // 介護系
    if s.contains("介護") || s.contains("ヘルパー") || s.contains("ケアワーカー")
        || s.contains("介護福祉") || s.contains("訪問介護")
    {
        return Some("介護系");
    }
    // 保育系
    if s.contains("保育") || s.contains("幼稚園教諭") || s.contains("保育士")
        || s.contains("児童指導員")
    {
        return Some("保育系");
    }
    // 調理系
    if s.contains("調理") || s.contains("厨房") || s.contains("料理人")
        || s.contains("シェフ") || s.contains("クック")
    {
        return Some("調理系");
    }
    // 運転・物流系
    if s.contains("ドライバー") || s.contains("運転手") || s.contains("配送")
        || s.contains("運送") || s.contains("物流") || s.contains("倉庫")
        || s.contains("トラック")
    {
        return Some("運転・物流系");
    }
    // 製造・建設系
    if s.contains("製造") || s.contains("工場") || s.contains("建設")
        || s.contains("建築") || s.contains("施工") || s.contains("大工")
        || s.contains("組立") || s.contains("溶接") || s.contains("塗装")
    {
        return Some("製造・建設系");
    }
    // 事務系
    if s.contains("事務") || s.contains("経理") || s.contains("総務")
        || s.contains("人事") || s.contains("受付") || s.contains("一般事務")
        || s.contains("営業事務")
    {
        return Some("事務系");
    }
    None
}

/// `SurveyAggregation` を職種推定グループ単位で再集計する.
///
/// # 戻り値
/// - 件数降順で Top 10 まで
/// - 推定不能 (キーワード非マッチ) は除外
/// - `by_tag_salary` / `by_company` が空の場合は空 Vec
pub(super) fn aggregate_occupation_salary(agg: &SurveyAggregation) -> Vec<OccupationSalaryRow> {
    let mut buckets: std::collections::HashMap<&'static str, OccupationBucket> =
        std::collections::HashMap::new();

    // 信号 A (主): by_tag_salary タグ → 職種グループ → 件数 + avg_salary
    for tag in &agg.by_tag_salary {
        if tag.count == 0 || tag.avg_salary <= 0 {
            continue;
        }
        let Some(group) = map_keyword_to_occupation_group(&tag.tag) else {
            continue;
        };
        let bucket = buckets.entry(group).or_default();
        bucket.count += tag.count as i64;
        bucket.sum_weighted_avg += tag.avg_salary * tag.count as i64;
    }

    // 信号 B (補助): by_company 会社名 → 職種グループ。信号 A 未カバーのみ加算 (二重カウント防止)
    for company in &agg.by_company {
        if company.count == 0 || company.avg_salary <= 0 {
            continue;
        }
        let Some(group) = map_keyword_to_occupation_group(&company.name) else {
            continue;
        };
        if buckets.contains_key(group) {
            // 信号 A 既カバー → median のみ補完 (avg は二重カウント回避)
            if company.median_salary > 0 {
                buckets
                    .get_mut(group)
                    .unwrap()
                    .company_medians
                    .push(company.median_salary);
            }
            continue;
        }
        let bucket = buckets.entry(group).or_default();
        bucket.count += company.count as i64;
        bucket.sum_weighted_avg += company.avg_salary * company.count as i64;
        if company.median_salary > 0 {
            bucket.company_medians.push(company.median_salary);
        }
    }

    let unit_label: &'static str = if agg.is_hourly { "時給" } else { "月給" };

    let mut rows: Vec<OccupationSalaryRow> = buckets
        .into_iter()
        .filter(|(_, b)| b.count > 0)
        .map(|(group, b)| {
            let weighted_avg = b.sum_weighted_avg / b.count;
            let median = if b.company_medians.is_empty() {
                None
            } else {
                let mut v = b.company_medians.clone();
                v.sort();
                let n = v.len();
                Some(if n.is_multiple_of(2) {
                    (v[n / 2 - 1] + v[n / 2]) / 2
                } else {
                    v[n / 2]
                })
            };
            let note = if b.count < 3 { "参考 (低信頼)" } else { "" };
            OccupationSalaryRow {
                occupation: group.to_string(),
                count: b.count,
                weighted_avg,
                median_of_company_medians: median,
                unit_label,
                note,
            }
        })
        .collect();

    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| b.weighted_avg.cmp(&a.weighted_avg)));
    rows.truncate(10);
    rows
}

/// MI variant PDF に「職種推定グループ別 給与参考」セクションを描画する.
pub(super) fn render_section_occupation_salary(html: &mut String, agg: &SurveyAggregation) {
    let rows = aggregate_occupation_salary(agg);
    if rows.is_empty() {
        return;
    }
    let unit_yen_label = if agg.is_hourly { "円/時" } else { "円" };
    let manyen_or_yen_label = if agg.is_hourly { "円/時" } else { "万円" };

    html.push_str("<div class=\"section\" data-testid=\"occupation-salary-section\">\n");
    html.push_str("<h2>職種推定グループ別 給与参考</h2>\n");

    render_section_howto(
        html,
        &[
            "アップロードした CSV を求人タグ・企業名のキーワードから推定した職種グループ単位で集約し、給与の参考値を提示します",
            "原 CSV に標準化された職種コードが無いため、キーワードから推定したグループです（公的職業分類とは一致しない場合があります）",
            "件数 3 件未満のグループは「参考 (低信頼)」と表示します",
        ],
    );

    render_figure_caption(
        html,
        "表 6-4",
        "職種推定グループ別 給与参考（タグ・企業名由来の推定、件数 Top 10）",
    );

    html.push_str(
        "<p class=\"mi-table-note\" style=\"font-size:9pt;color:#6b7280;margin-bottom:6px;\">\
        \u{26A0} 推定・参考値: 本表は CSV に標準化された職種コードがないため、求人タグ・企業名から推定した職種グループです。\
        給与値は求人 CSV 上の給与情報を月給換算した参考値であり、\
        公的職業分類（総務省統計局 日本標準職業分類等）や法人 DB の正式分類とは一致しない場合があります。\
        全体給与中央値（表紙ハイライト KPI）と一致しない指標です。\
        件数 3 件以上を集計対象とし、3 件未満は「参考 (低信頼)」として併記します。\
        </p>\n",
    );

    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"occupation-salary-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
        <th>#</th>\
        <th>職種推定グループ</th>\
        <th style=\"text-align:right\">件数</th>\
        <th style=\"text-align:right\">{unit} 参考平均</th>\
        <th style=\"text-align:right\">{unit} 推定グループ中央値</th>\
        <th>信頼度</th>\
        </tr></thead>\n<tbody>\n",
        unit = match agg.is_hourly {
            true => "時給",
            false => "月給",
        },
    ));

    for (i, r) in rows.iter().enumerate() {
        let avg_text = format_value_text(r.weighted_avg, agg.is_hourly);
        let median_text = match r.median_of_company_medians {
            Some(m) => format_value_text(m, agg.is_hourly),
            None => "-".to_string(),
        };
        html.push_str(&format!(
            "<tr>\
                <td>{rank}</td>\
                <td>{name}</td>\
                <td class=\"num\">{count}件</td>\
                <td class=\"num\">{avg} {unit}</td>\
                <td class=\"num\">{med} {unit}</td>\
                <td>{note}</td>\
            </tr>\n",
            rank = i + 1,
            name = escape_html(&r.occupation),
            count = format_number(r.count),
            avg = avg_text,
            med = median_text,
            unit = manyen_or_yen_label,
            note = r.note,
        ));
    }
    html.push_str("</tbody></table>\n");

    html.push_str(&format!(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
        \u{26A0} 職種推定はタグ・企業名のキーワードからの推定（例:「看護師」「介護福祉士」「リハビリ」「ドライバー」等）で、原 CSV に職種コードがない場合に限界があります。\
        参考平均は件数による重み付け平均、推定グループ中央値は企業別中央値の中央値で算出した近似値です（per-record の中央値とは異なります）。\
        値の単位は{unit_native}（{unit_yen}）。本表は CSV ベースの参考値であり、地域全体の職種別給与水準を代表するものではありません。\
        全体給与中央値（表紙ハイライト KPI）と直接比較できる指標ではありません。\
        本表は相関の可視化であり、因果の証明ではありません。\
        </p>\n",
        unit_native = if agg.is_hourly { "時給" } else { "月給" },
        unit_yen = unit_yen_label,
    ));

    render_read_hint(
        html,
        "職種推定グループ間で給与の参考値に差が見られる場合、業務内容・経験要件・労働時間・夜勤の有無等の複合要因を示唆します。\
         具体的な原因解釈は別途現場ヒアリング等で検証してください。",
    );

    html.push_str("</div>\n");
}

/// 月給は万円表示、時給は円/時 のままで小数点 1 桁に整形する.
fn format_value_text(yen: i64, is_hourly: bool) -> String {
    if is_hourly {
        format_number(yen)
    } else {
        format!("{:.1}", yen as f64 / 10_000.0)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{CompanyAgg, TagSalaryAgg};

    fn agg_with_tags(tags: Vec<TagSalaryAgg>) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = tags.iter().map(|t| t.count).sum();
        agg.is_hourly = false;
        agg.by_tag_salary = tags;
        agg
    }

    fn tag(name: &str, count: usize, avg: i64) -> TagSalaryAgg {
        TagSalaryAgg {
            tag: name.to_string(),
            count,
            avg_salary: avg,
            diff_from_avg: 0,
            diff_percent: 0.0,
        }
    }

    fn co(name: &str, count: usize, avg: i64, median: i64) -> CompanyAgg {
        CompanyAgg {
            name: name.to_string(),
            count,
            avg_salary: avg,
            median_salary: median,
        }
    }

    /// キーワード分類器の正当性 (主要 10 グループ + 未分類).
    #[test]
    fn map_keyword_to_occupation_group_classifies_known_keywords() {
        assert_eq!(map_keyword_to_occupation_group("看護師"), Some("看護系"));
        assert_eq!(map_keyword_to_occupation_group("准看護師"), Some("看護系"));
        assert_eq!(map_keyword_to_occupation_group("介護福祉士"), Some("介護系"));
        assert_eq!(map_keyword_to_occupation_group("ヘルパー"), Some("介護系"));
        assert_eq!(map_keyword_to_occupation_group("保育士"), Some("保育系"));
        assert_eq!(map_keyword_to_occupation_group("理学療法士"), Some("リハビリ系"));
        assert_eq!(map_keyword_to_occupation_group("作業療法士"), Some("リハビリ系"));
        assert_eq!(map_keyword_to_occupation_group("薬剤師"), Some("医療技術系"));
        assert_eq!(map_keyword_to_occupation_group("ケアマネジャー"), Some("福祉相談系"));
        assert_eq!(map_keyword_to_occupation_group("調理師"), Some("調理系"));
        assert_eq!(map_keyword_to_occupation_group("ドライバー"), Some("運転・物流系"));
        assert_eq!(map_keyword_to_occupation_group("施工管理"), Some("製造・建設系"));
        assert_eq!(map_keyword_to_occupation_group("一般事務"), Some("事務系"));
        // 未分類
        assert_eq!(map_keyword_to_occupation_group("ABC123"), None);
        assert_eq!(map_keyword_to_occupation_group(""), None);
    }

    /// タグから職種推定 → 同グループの件数を加算、加重平均を計算する.
    #[test]
    fn occupation_salary_aggregates_by_occupation() {
        let agg = agg_with_tags(vec![
            tag("看護師", 10, 280_000),
            tag("准看護師", 5, 240_000),
            tag("介護福祉士", 8, 220_000),
        ]);
        let rows = aggregate_occupation_salary(&agg);
        // 看護系 (10 + 5 = 15) と 介護系 (8) の 2 グループ
        assert_eq!(rows.len(), 2, "2 グループに集約されるはず: {:?}", rows);
        assert_eq!(rows[0].occupation, "看護系");
        assert_eq!(rows[0].count, 15);
        assert_eq!(rows[0].weighted_avg, (280_000 * 10 + 240_000 * 5) / 15);
        assert_eq!(rows[1].occupation, "介護系");
        assert_eq!(rows[1].count, 8);
    }

    /// 推定不能 (キーワード非マッチ) なタグは除外される.
    #[test]
    fn occupation_salary_excludes_unclassifiable_tags() {
        let agg = agg_with_tags(vec![
            tag("ABC123", 5, 250_000), // 非マッチ → 除外
            tag("看護師", 3, 280_000),
        ]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].occupation, "看護系");
        assert_eq!(rows[0].count, 3);
    }

    /// 件数 < 3 は note="参考 (低信頼)" でマークされる.
    #[test]
    fn occupation_salary_low_count_marked_as_low_confidence() {
        let agg = agg_with_tags(vec![tag("看護師", 2, 280_000)]);
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[0].note, "参考 (低信頼)");
    }

    /// is_hourly=true なら unit_label="時給"、HTML も時給ラベル.
    #[test]
    fn occupation_salary_hourly_mode_uses_hourly_unit_label() {
        let mut agg = agg_with_tags(vec![tag("看護師", 10, 1_500)]);
        agg.is_hourly = true;
        let rows = aggregate_occupation_salary(&agg);
        assert_eq!(rows[0].unit_label, "時給");
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("時給 参考平均"), "is_hourly=true で時給ラベル");
        assert!(!html.contains("月給 参考平均"), "is_hourly=true で月給ラベル不在");
    }

    /// HW 連想語を出力に含めない.
    #[test]
    fn occupation_salary_does_not_emit_hw_terms() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        for forbidden in [
            "ハローワーク",
            "HW 求人",
            "有効求人倍率",
            "欠員補充率",
            "求人継続率",
        ] {
            assert!(
                !html.contains(forbidden),
                "Round 3-C に HW 連想語 '{}' が混入してはならない",
                forbidden
            );
        }
    }

    /// MI variant でデータあり時に出力 / 空時は fail-soft.
    #[test]
    fn occupation_salary_section_appears_in_mi_variant_only() {
        // 空 → 出力なし
        let agg = SurveyAggregation::default();
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert_eq!(html, "", "空集計時は何も出力しない");

        // データあり → 出力
        let agg2 = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html2 = String::new();
        render_section_occupation_salary(&mut html2, &agg2);
        assert!(html2.contains("職種推定グループ別 給与参考"));
    }

    /// 見出しに「職種別」断定不可、「職種推定」「参考」必須.
    #[test]
    fn occupation_salary_heading_uses_estimation_phrasing() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(
            html.contains("職種推定") || html.contains("推定グループ"),
            "見出しに「職種推定」「推定グループ」を含むこと"
        );
        assert!(html.contains("参考"), "見出し or 注記に「参考」を含むこと");
        assert!(
            !html.contains(">職種別 給与水準<"),
            "断定タイトル「職種別 給与水準」を h2 に使ってはならない"
        );
    }

    /// 注記に CSV 職種コード不在・公的職業分類との不一致・全体中央値との非一致を含む.
    #[test]
    fn occupation_salary_note_includes_caveat() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("推定"));
        assert!(html.contains("参考値"));
        assert!(
            html.contains("公的職業分類") && html.contains("一致しない"),
            "注記に「公的職業分類…一致しない」を含むこと"
        );
        assert!(
            html.contains("全体給与中央値"),
            "注記に「全体給与中央値…一致しない」を含むこと"
        );
    }

    /// 信号 A (タグ) で既カバーのグループに信号 B (会社名) の件数を二重加算しない.
    #[test]
    fn occupation_salary_does_not_double_count_tag_and_company_signals() {
        let mut agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]); // 看護系を信号 A でカバー
        agg.by_company = vec![
            co("○○病院", 100, 290_000, 285_000), // 看護系 (or 医療技術系) でカバー済 → 加算しない
            co("□□建設会社", 5, 320_000, 315_000), // 製造・建設系、信号 A 未カバー → 補完加算
        ];
        let rows = aggregate_occupation_salary(&agg);
        // 看護系: 信号 A の 10 件のみ (○○病院 は加算しない)
        let nursing = rows.iter().find(|r| r.occupation == "看護系").unwrap();
        assert_eq!(
            nursing.count, 10,
            "信号 A 既カバーの職種に信号 B を二重加算しないこと"
        );
        // 製造・建設系: 信号 B のみで補完
        let construction = rows.iter().find(|r| r.occupation == "製造・建設系");
        assert!(
            construction.is_some(),
            "信号 A 未カバーの職種は信号 B で補完されること"
        );
        assert_eq!(construction.unwrap().count, 5);
    }

    /// 件数 < 3 のときに HTML 出力に「参考 (低信頼)」が表示される.
    #[test]
    fn occupation_salary_low_confidence_label_in_html() {
        let agg = agg_with_tags(vec![tag("看護師", 2, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("参考 (低信頼)"));
    }

    /// 列ヘッダが Round 3-B/3-C 表現規約に揃う.
    #[test]
    fn occupation_salary_column_headers_use_reference_phrasing() {
        let agg = agg_with_tags(vec![tag("看護師", 10, 280_000)]);
        let mut html = String::new();
        render_section_occupation_salary(&mut html, &agg);
        assert!(html.contains("職種推定グループ"));
        assert!(html.contains("参考平均"));
        assert!(html.contains("推定グループ中央値"));
        assert!(html.contains("信頼度"));
    }
}
