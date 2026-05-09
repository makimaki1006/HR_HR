//! 業界別 給与水準クロス表 (Round 3-B / 2026-05-06)
//!
//! ## 背景
//! Round 1-E 完全欠落 Top 2「業界×給与」と Round 2-4 真の未実装 #8 を消化。
//! Round 3-A で接続した産業構成表 (e-Stat 国勢調査 2020) は公的統計のみで、
//! 「対象 CSV 自体の業界別給与水準」は MI PDF に存在しなかった。
//! 本モジュールは CSV 由来の集計 (`SurveyAggregation`) を業界単位で再集計し、
//! 月給換算後の給与水準を業界横並びで提示する。
//!
//! ## 設計方針
//! - **既存集計の再利用**: `industry_mismatch::map_keyword_to_major_industry` を流用し、
//!   `by_company` の会社名 + `by_tag_salary` のタグ名から産業大分類を推定する。
//!   per-record データは `SurveyAggregation` に存在しないため、推定信号は
//!   - 信号 A (主信号): `CompanyAgg.name` → 産業マップ → 件数加重平均で
//!     `avg_salary` / `median_salary` を集計
//!   - 信号 B (補助): `TagSalaryAgg.tag` → 産業マップ → 件数加重で `avg_salary` のみ
//!   と段階的に拾う。`by_company` で十分カバーできる場合は B は使わない。
//! - **数値ロジックは新規作成しない**: 月給換算は既に `aggregator` 経路で済んでいる
//!   (`CompanyAgg.avg_salary` は CSV 集計時の `is_hourly` モードに従ったネイティブ単位)。
//!   `is_hourly_overall` の場合は時給で表示し、ラベルで明示区別する
//!   (`SalaryHeadline` と同じスコープ規約)。
//! - **件数 < 3 は「参考」**: 推定誤差・サンプル不足の業界は note 列で明示。
//! - **MI variant 専用**: `mod.rs` で MI variant のみ呼び出し、Full / Public 不変。
//!
//! ## 関連 memory ルール
//! - `feedback_correlation_not_causation.md` 「相関≠因果」: caveat に明記
//! - `feedback_neutral_expression_for_targets.md` 「中立表現」: 「劣位」等の評価語禁止
//! - `feedback_test_data_validation.md` 「テストでデータ中身を検証する」
//! - Hard NG 13 用語 + HW 連想語不混入 (no_forbidden_terms.rs ガード対象)

use super::super::aggregator::SurveyAggregation;
use super::super::super::helpers::{escape_html, format_number};
use super::helpers::{render_figure_caption, render_read_hint, render_section_howto};
use super::industry_mismatch::map_keyword_to_major_industry;

/// 業界別 給与水準 1 行分の集計結果.
///
/// 月給/時給の単位は `unit_label` で明示する。値はネイティブ単位 (円) で保持。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IndustrySalaryRow {
    /// 産業大分類名 (例: "医療，福祉")
    pub industry: String,
    /// 推定された求人件数 (推定信号の合計)
    pub count: i64,
    /// 当該産業の件数加重平均給与 (ネイティブ単位、円)
    pub weighted_avg: i64,
    /// 当該産業の median 提示用 (`CompanyAgg.median_salary` の単純中央値、円)
    /// 信号 B (タグ経路) からは取れないため、`by_company` 経路でのみ算出。
    pub median_of_company_medians: Option<i64>,
    /// "月給" / "時給" の表示単位
    pub unit_label: &'static str,
    /// 「参考」(件数<3) / 「-」 等の備考
    pub note: &'static str,
}

/// 産業ごとの累積バッファ (内部用)
#[derive(Debug, Default)]
struct IndustryBucket {
    count: i64,
    sum_weighted_avg: i64, // Σ (avg_salary * count)
    company_medians: Vec<i64>,
}

/// `SurveyAggregation` を業界単位で再集計して `IndustrySalaryRow` の配列を返す.
///
/// # 戻り値
/// - 件数降順で Top 10 まで
/// - 産業未分類 (キーワード非マッチ) は除外 (`map_keyword_to_major_industry` が None)
/// - `by_company` が空、または全業界の件数合計が 0 の場合は空 Vec
///
/// # ネイティブ単位
/// `agg.is_hourly` が true なら時給 (円/時)、false なら月給 (円)。
/// `weighted_avg` / `median_of_company_medians` は両方とも同じ単位で出力する。
pub(super) fn aggregate_industry_salary(agg: &SurveyAggregation) -> Vec<IndustrySalaryRow> {
    let mut buckets: std::collections::HashMap<&'static str, IndustryBucket> =
        std::collections::HashMap::new();

    // 信号 A (主信号): by_company から会社名 → 産業 → 件数 + 給与
    for company in &agg.by_company {
        if company.count == 0 || company.avg_salary <= 0 {
            continue;
        }
        let Some(industry) = map_keyword_to_major_industry(&company.name) else {
            continue;
        };
        let bucket = buckets.entry(industry).or_default();
        bucket.count += company.count as i64;
        bucket.sum_weighted_avg += company.avg_salary * company.count as i64;
        if company.median_salary > 0 {
            bucket.company_medians.push(company.median_salary);
        }
    }

    // 信号 B (補助): by_tag_salary でタグ → 産業 → 件数 + 給与 (median は未取得)
    // 信号 A で全くカバーできなかった産業のみ補完 (重複加算を避けるため)
    for tag in &agg.by_tag_salary {
        if tag.count == 0 || tag.avg_salary <= 0 {
            continue;
        }
        let Some(industry) = map_keyword_to_major_industry(&tag.tag) else {
            continue;
        };
        // 既に信号 A で計上済みの industry には加算しない (二重カウント防止)
        if buckets.contains_key(industry) {
            continue;
        }
        let bucket = buckets.entry(industry).or_default();
        bucket.count += tag.count as i64;
        bucket.sum_weighted_avg += tag.avg_salary * tag.count as i64;
    }

    let unit_label: &'static str = if agg.is_hourly { "時給" } else { "月給" };

    let mut rows: Vec<IndustrySalaryRow> = buckets
        .into_iter()
        .filter(|(_, b)| b.count > 0)
        .map(|(industry, b)| {
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
            IndustrySalaryRow {
                industry: industry.to_string(),
                count: b.count,
                weighted_avg,
                median_of_company_medians: median,
                unit_label,
                note,
            }
        })
        .collect();

    rows.sort_by(|a, b| b.count.cmp(&a.count));
    rows.truncate(10);
    rows
}

/// MI variant PDF に「業界別 給与水準」セクションを描画する.
///
/// # 表示しない条件 (fail-soft)
/// - `aggregate_industry_salary` の結果が空
///
/// # ラベル規約
/// - 単位は `agg.is_hourly` に従い「月給 (円)」 or 「時給 (円/時)」
/// - 値が 0 件 / 算出不能なら "-"
/// - 件数 < 3 件は「参考」列で明示 (推定誤差大)
pub(super) fn render_section_industry_salary(html: &mut String, agg: &SurveyAggregation) {
    let rows = aggregate_industry_salary(agg);
    if rows.is_empty() {
        return;
    }
    let unit_yen_label = if agg.is_hourly {
        "円/時"
    } else {
        "円"
    };
    let manyen_or_yen_label = if agg.is_hourly {
        "円/時"
    } else {
        "万円"
    };

    html.push_str(
        "<div class=\"section\" data-testid=\"industry-salary-section\">\n",
    );
    html.push_str("<h2>業界推定グループ別 給与参考</h2>\n");

    render_section_howto(
        html,
        &[
            "アップロードした CSV を企業名・タグから推定した業界グループ単位で集約し、給与の参考値を提示します",
            "原 CSV に業界列が無いため、企業名・タグのキーワードから推定したグループです（公的産業分類とは一致しない場合があります）",
            "件数 3 件未満のグループは「参考 (低信頼)」と表示します",
        ],
    );

    render_figure_caption(
        html,
        "表 6-3",
        "業界推定グループ別 給与参考（企業名・タグ由来の推定、件数 Top 10）",
    );

    // 推定・参考であることを表内直前に明示（必須注記）
    html.push_str(
        "<p class=\"mi-table-note\" style=\"font-size:9pt;color:#6b7280;margin-bottom:6px;\">\
        \u{26A0} 推定・参考値: 本表は CSV に業界列がないため、企業名・タグから推定した業界グループです。\
        給与値は求人 CSV 上の給与情報を月給換算した参考値であり、\
        公的産業分類（e-Stat 経済センサス等）や法人 DB の正式業界分類とは一致しない場合があります。\
        全体給与中央値（表紙ハイライト KPI）と一致しない指標です。\
        件数 3 件以上を集計対象とし、3 件未満は「参考 (低信頼)」として併記します。\
        </p>\n",
    );

    html.push_str(
        "<table class=\"sortable-table zebra\" data-testid=\"industry-salary-table\">\n",
    );
    html.push_str(&format!(
        "<thead><tr>\
        <th>#</th>\
        <th>業界推定グループ</th>\
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
            name = escape_html(&r.industry),
            count = format_number(r.count),
            avg = avg_text,
            med = median_text,
            unit = manyen_or_yen_label,
            note = r.note,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 単位明記 + 推定限界 + 因果非主張 caveat
    html.push_str(&format!(
        "<p class=\"caveat\" style=\"font-size:9pt;color:#475569;margin-top:8px;\">\
        \u{26A0} 業界推定は企業名・タグ列のキーワードからの推定（例:「メディカル」「ケアセンター」「建設」等）で、原 CSV に業界列がない場合に限界があります。\
        参考平均は企業別件数による重み付け平均、推定グループ中央値は企業別中央値（`CompanyAgg.median_salary`）の中央値で算出した近似値です（per-record の中央値とは異なります）。\
        値の単位は{unit_native}（{unit_yen}）。本表は CSV ベースの参考値であり、地域全体の業界給与水準を代表するものではありません。\
        全体給与中央値（表紙ハイライト KPI）と直接比較できる指標ではありません。\
        本表は相関の可視化であり、因果の証明ではありません。\
        </p>\n",
        unit_native = if agg.is_hourly { "時給" } else { "月給" },
        unit_yen = unit_yen_label,
    ));

    render_read_hint(
        html,
        "業界推定グループ間で給与の参考値に差が見られる場合、業務内容・経験要件・労働時間などの複合要因を示唆します。\
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
    use crate::handlers::survey::aggregator::{CompanyAgg, EmpGroupNativeAgg, TagSalaryAgg};

    fn agg_with_companies(companies: Vec<CompanyAgg>) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = companies.iter().map(|c| c.count).sum();
        agg.is_hourly = false;
        agg.by_company = companies;
        agg
    }

    fn co(name: &str, count: usize, avg: i64, median: i64) -> CompanyAgg {
        CompanyAgg {
            name: name.to_string(),
            count,
            avg_salary: avg,
            median_salary: median,
        }
    }

    /// 会社名から業界推定 → 同業界の件数を加算、加重平均を計算する.
    #[test]
    fn industry_salary_aggregates_by_industry() {
        let agg = agg_with_companies(vec![
            co("メディカルケア株式会社", 10, 250_000, 240_000),
            co("○○病院", 5, 280_000, 270_000),
            co("□□建設株式会社", 4, 300_000, 290_000),
        ]);
        let rows = aggregate_industry_salary(&agg);
        // 医療, 福祉系 (メディカル + 病院) と 建設業 の 2 業界
        assert_eq!(rows.len(), 2, "2 業界に集約されるはず: {:?}", rows);
        // 件数最多は 医療,福祉 (10 + 5 = 15 件)
        assert_eq!(rows[0].industry, "医療，福祉");
        assert_eq!(rows[0].count, 15);
        // 加重平均 = (250000*10 + 280000*5) / 15 = 4_900_000 / 15 = 326_666... → 整数除算で 260_000
        assert_eq!(rows[0].weighted_avg, (250_000 * 10 + 280_000 * 5) / 15);
        assert_eq!(rows[1].industry, "建設業");
        assert_eq!(rows[1].count, 4);
    }

    /// 業界推定不能 (キーワード非マッチ) な会社は「未分類」扱いせず除外される.
    #[test]
    fn industry_salary_excludes_unclassifiable_companies() {
        let agg = agg_with_companies(vec![
            co("ABC123", 5, 250_000, 240_000), // どのキーワードにもマッチしない
            co("メディカル株式会社", 3, 260_000, 255_000),
        ]);
        let rows = aggregate_industry_salary(&agg);
        // ABC123 は除外、医療,福祉 のみ
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].industry, "医療，福祉");
        assert_eq!(rows[0].count, 3);
    }

    /// 件数 3 件未満は note="参考" でマークされる (推定誤差大).
    #[test]
    fn industry_salary_low_count_marked_as_reference() {
        let agg = agg_with_companies(vec![
            co("メディカル株式会社", 2, 250_000, 245_000), // count=2 < 3
        ]);
        let rows = aggregate_industry_salary(&agg);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].count, 2);
        assert_eq!(
            rows[0].note, "参考 (低信頼)",
            "件数<3 は「参考 (低信頼)」マーク必須"
        );
    }

    /// is_hourly=true なら unit_label="時給"、HTML も時給ラベルを採用.
    #[test]
    fn industry_salary_hourly_mode_uses_hourly_unit_label() {
        let mut agg = agg_with_companies(vec![co("メディカル株式会社", 10, 1_200, 1_180)]);
        agg.is_hourly = true;
        let rows = aggregate_industry_salary(&agg);
        assert_eq!(rows[0].unit_label, "時給");
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        assert!(
            html.contains("時給 参考平均"),
            "is_hourly=true の場合、時給ラベルが見出しに出ること"
        );
        assert!(
            !html.contains("月給 参考平均"),
            "is_hourly=true の場合、月給ラベルは出ないこと"
        );
    }

    /// HW / 求人倍率 / 有効求人倍率 / ハローワーク / 欠員補充率 の HW 連想語を出力に含めない.
    #[test]
    fn industry_salary_section_does_not_emit_hw_terms() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 10, 250_000, 245_000)]);
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        // 監査対象 HW 連想語 (Round 2-1/2.5/2.7-B HW 言及最小化方針)
        for forbidden in [
            "ハローワーク",
            "HW 求人",
            "有効求人倍率",
            "欠員補充率",
            "求人継続率",
        ] {
            assert!(
                !html.contains(forbidden),
                "Round 3-B 章に HW 連想語 '{}' が混入してはならない",
                forbidden
            );
        }
    }

    /// `by_company` が空ならセクション全体を出さない (fail-soft).
    #[test]
    fn industry_salary_section_skipped_when_empty() {
        let agg = SurveyAggregation::default();
        let rows = aggregate_industry_salary(&agg);
        assert!(rows.is_empty());
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        assert_eq!(html, "", "空集計時は何も出力しない (fail-soft)");
    }

    /// 信号 B (`by_tag_salary`) は信号 A でカバー済みの業界には加算しない (二重カウント防止).
    /// 信号 A 未カバーの業界のみ補完する。
    #[test]
    fn industry_salary_does_not_double_count_company_and_tag_signals() {
        let mut agg = agg_with_companies(vec![
            co("メディカル株式会社", 10, 250_000, 245_000), // 医療,福祉 を信号 A でカバー
        ]);
        // 信号 B: 別タグ「建設」(信号 A 未カバー) と「介護」(信号 A カバー済み)
        agg.by_tag_salary = vec![
            TagSalaryAgg {
                tag: "建設".to_string(),
                count: 4,
                avg_salary: 300_000,
                diff_from_avg: 0,
                diff_percent: 0.0,
            },
            TagSalaryAgg {
                tag: "介護".to_string(),
                count: 99,
                avg_salary: 200_000,
                diff_from_avg: 0,
                diff_percent: 0.0,
            },
        ];
        let rows = aggregate_industry_salary(&agg);
        // 医療,福祉 は 信号 A の 10 件のみ (介護タグは加算しない)
        let medical = rows.iter().find(|r| r.industry == "医療，福祉").unwrap();
        assert_eq!(
            medical.count, 10,
            "信号 A でカバー済みの業界には信号 B を加算しないこと"
        );
        // 建設業 は 信号 B のみで補完される
        let construction = rows.iter().find(|r| r.industry == "建設業");
        assert!(
            construction.is_some(),
            "信号 A 未カバーの業界は信号 B で補完されること"
        );
        assert_eq!(construction.unwrap().count, 4);
    }

    /// 件数降順で Top 10 まで返却 (それを超える業界は切り捨て).
    #[test]
    fn industry_salary_returns_top10_descending_by_count() {
        // 信号 A だけで複数業界を作る (キーワード差し分け)
        let companies = vec![
            co("メディカル A", 100, 250_000, 240_000), // 医療,福祉
            co("○○病院", 50, 260_000, 250_000),       // 医療,福祉 (同業界)
            co("建設会社 X", 80, 300_000, 290_000),    // 建設業
            co("○○商店", 70, 220_000, 215_000),       // 卸売業，小売業
            co("○○農園", 60, 200_000, 195_000),       // 農業，林業
            co("○○漁業", 40, 240_000, 235_000),       // 漁業
            co("○○ホテル", 30, 230_000, 225_000),     // 宿泊業，飲食サービス業
            co("○○運輸", 20, 280_000, 275_000),       // 運輸業，郵便業
            co("ソフトウェア株式会社", 15, 350_000, 345_000), // 情報通信業
            co("学習塾○○", 10, 240_000, 235_000),     // 教育，学習支援業
            co("○○銀行", 5, 320_000, 315_000),        // 金融業，保険業
        ];
        let agg = agg_with_companies(companies);
        let rows = aggregate_industry_salary(&agg);
        assert!(rows.len() <= 10, "Top 10 までに切り詰められること");
        // 件数降順
        for w in rows.windows(2) {
            assert!(w[0].count >= w[1].count, "件数降順であること: {:?}", rows);
        }
    }

    /// 月給値 (yen) は「万円」、時給値 (円/時) はそのまま整形される.
    #[test]
    fn industry_salary_format_value_text_uses_correct_unit() {
        // 月給 250,000 円 → "25.0" (万円)
        assert_eq!(format_value_text(250_000, false), "25.0");
        // 時給 1,200 円 → "1,200" (format_number は 3 桁区切り)
        let hourly = format_value_text(1_200, true);
        assert!(
            hourly.contains("1,200") || hourly.contains("1200"),
            "時給は format_number で整形 (3 桁区切り or プレーン): got {:?}",
            hourly
        );
    }

    // 派生テスト: グループ ネイティブ集計 (`by_emp_group_native`) は本セクションでは使わない.
    // (Round 3-B のスコープは産業別給与のみで、雇用形態×給与は別セクション)
    #[test]
    fn industry_salary_does_not_use_emp_group_native_aggregation() {
        let mut agg = agg_with_companies(vec![]);
        agg.by_emp_group_native = vec![EmpGroupNativeAgg {
            group_label: "正社員".to_string(),
            native_unit: "月給".to_string(),
            count: 100,
            median: 300_000,
            ..Default::default()
        }];
        // by_company が空である限り、行は出ない (by_emp_group_native は無視される)
        let rows = aggregate_industry_salary(&agg);
        assert!(
            rows.is_empty(),
            "by_emp_group_native は本集計で使用しないため、空集計のままであること"
        );
    }

    // ============================================================
    // Round 3-B' 補正テスト: 表現層を「推定・参考」に揃える
    // ============================================================

    /// 見出しは断定表現（「業界別 給与水準」）を使わず、「業界推定」「参考」を含む.
    #[test]
    fn industry_salary_heading_uses_estimation_phrasing() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 10, 250_000, 245_000)]);
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        // 「業界推定」または「推定グループ」を含む
        assert!(
            html.contains("業界推定") || html.contains("推定グループ"),
            "見出しに「業界推定」「推定グループ」のいずれかを含むこと"
        );
        // 「参考」表現を含む
        assert!(html.contains("参考"), "見出し or 注記に「参考」を含むこと");
        // 断定タイトル「>業界別 給与水準<」は不在
        assert!(
            !html.contains(">業界別 給与水準<"),
            "断定タイトル「業界別 給与水準」を h2 に使ってはならない"
        );
    }

    /// 注記に CSV 業界列不在・公的分類との不一致・全体中央値との非一致を含む.
    #[test]
    fn industry_salary_note_includes_caveat() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 10, 250_000, 245_000)]);
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        assert!(html.contains("推定"), "注記に「推定」を含むこと");
        assert!(html.contains("参考値"), "注記に「参考値」を含むこと");
        assert!(
            html.contains("公的産業分類") && html.contains("一致しない"),
            "注記に「公的産業分類…一致しない」を含むこと"
        );
        assert!(
            html.contains("全体給与中央値"),
            "注記に「全体給与中央値（表紙ハイライト KPI）と一致しない」旨を含むこと"
        );
    }

    /// 件数 < 3 は「参考 (低信頼)」と表示される.
    #[test]
    fn industry_salary_low_confidence_label_for_count_under_3() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 2, 250_000, 245_000)]);
        let rows = aggregate_industry_salary(&agg);
        assert_eq!(rows[0].note, "参考 (低信頼)");
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        assert!(
            html.contains("参考 (低信頼)"),
            "HTML 出力に「参考 (低信頼)」表示を含むこと"
        );
    }

    /// 表現層が「業界別」「業種別」と断定しないこと.
    #[test]
    fn industry_salary_does_not_assert_industry_classification() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 10, 250_000, 245_000)]);
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        // 見出しレベルでの「業界別」「業種別」断定を禁止
        assert!(
            !html.contains(">業界別 "),
            "見出し / セルで「業界別 」断定表現を使わないこと"
        );
        assert!(
            !html.contains(">業種別 "),
            "見出し / セルで「業種別 」断定表現を使わないこと"
        );
    }

    /// 列ヘッダが Round 3-B' の表現に揃っている.
    #[test]
    fn industry_salary_column_headers_use_reference_phrasing() {
        let agg = agg_with_companies(vec![co("メディカル株式会社", 10, 250_000, 245_000)]);
        let mut html = String::new();
        render_section_industry_salary(&mut html, &agg);
        assert!(
            html.contains("業界推定グループ"),
            "列ヘッダに「業界推定グループ」を含むこと"
        );
        assert!(
            html.contains("参考平均"),
            "列ヘッダに「参考平均」を含むこと"
        );
        assert!(
            html.contains("推定グループ中央値"),
            "列ヘッダに「推定グループ中央値」を含むこと"
        );
        assert!(html.contains("信頼度"), "列ヘッダに「信頼度」を含むこと");
    }
}
