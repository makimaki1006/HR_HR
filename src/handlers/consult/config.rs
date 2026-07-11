//! コンサル支援 (採用仮説ブリーフ) の閾値定数
//!
//! 計画書 §9「閾値は設定ファイルで変更可能にする」に対応し、
//! シグナル・軸判定・信頼度判定の閾値をここに一元化する。
//! テストは本モジュールの定数を直接参照し、実装と閾値がずれないようにする。
//!
//! 注意 (§19): 閾値は「観測を解釈可能な中間表現に変換する」ためのものであり、
//! 閾値を超えた事実が顧客課題の断定を意味するわけではない。

/// 継続掲載シグナル: 掲載経過「30+日前」比率がこの値以上で発火
pub const POSTING_AGE_30PLUS_RATIO_THRESHOLD: f64 = 0.4;

/// 給与シグナル: 今回CSV中央値が県所定内給与のこの倍率未満で「低め」と判定
pub const SALARY_BELOW_PREF_RATIO: f64 = 0.9;

/// 給与シグナル: 今回CSV中央値が県所定内給与のこの倍率超で「高め」と判定
pub const SALARY_ABOVE_PREF_RATIO: f64 = 1.1;

/// 最低賃金近接シグナル: 今回CSVのQ1(または時給下限中央値)が
/// 最低賃金換算値のこの倍率以下で発火
pub const MIN_WAGE_PROXIMITY_RATIO: f64 = 1.05;

/// 従業員減×募集継続シグナル: 1年人員増減率(%)がこの値未満で「減少」と判定
pub const EMPLOYEE_DECLINE_THRESHOLD_PCT: f64 = 0.0;

/// 従業員減×募集継続シグナル: 求人件数がこの値以上で「募集継続」と判定
pub const CONTINUED_POSTING_MIN_COUNT: usize = 2;

/// 働き手減少地域シグナル: 将来推計の働き手増減率(%)がこの値以下で発火 (負値=減少)
pub const WORKFORCE_DECLINE_THRESHOLD_PCT: f64 = -15.0;

/// 転職希望層シグナル: 県転職希望率が全国比この倍率未満で「薄い」
pub const SWITCHER_THIN_RATIO: f64 = 0.9;

/// 転職希望層シグナル: 県転職希望率が全国比この倍率超で「厚い」
pub const SWITCHER_THICK_RATIO: f64 = 1.1;

/// 有効求人倍率: この値以上で「高い(採用競争が厳しい可能性)」
pub const JOB_RATIO_HIGH: f64 = 1.5;

/// 有効求人倍率: この値未満で「低い(市場が比較的緩やかな可能性)」
pub const JOB_RATIO_LOW: f64 = 1.0;

/// 通勤流入シグナル: 流入合計がこの人数以上で発火
pub const COMMUTE_INFLOW_MIN: i64 = 10_000;

/// 通勤流入シグナル: 流入が流出のこの倍率超でも発火
pub const COMMUTE_INFLOW_OUTFLOW_RATIO: f64 = 1.2;

/// 新着比率シグナル: 新着比率がこの値以上で発火
pub const NEW_RATIO_HIGH: f64 = 0.3;

/// サンプル不足シグナル: 今回CSV件数がこの値未満で発火 (§19: 弱い表現に切替)
pub const MIN_SAMPLE_POSTINGS: usize = 30;

/// 求人集中シグナル: 最多掲載企業のシェアがこの値以上で発火
pub const TOP_COMPANY_SHARE_THRESHOLD: f64 = 0.3;

/// 給与パーセンタイル: 下位判定の境界 (自社給与が市場のこの分位以下)
pub const SALARY_PERCENTILE_LOW: f64 = 25.0;

/// 給与パーセンタイル: 上位判定の境界 (自社給与が市場のこの分位以上)
pub const SALARY_PERCENTILE_HIGH: f64 = 75.0;

/// 競争軸: 今回CSV求人件数がこの値以上で「高」
pub const COMPETITION_POSTINGS_HIGH: usize = 150;

/// 競争軸: 今回CSV求人件数がこの値以上で「中」 (未満は「低」)
pub const COMPETITION_POSTINGS_MEDIUM: usize = 50;

/// 信頼度判定 High: 独立根拠がこの数以上
pub const CONFIDENCE_HIGH_MIN_SOURCES: usize = 3;

/// 信頼度判定 Medium: 独立根拠がこの数以上
pub const CONFIDENCE_MEDIUM_MIN_SOURCES: usize = 2;

/// 仮説TOP選定数
pub const HYPOTHESIS_TOP_N: usize = 5;

/// 矛盾の最大表示数 (2026-07-10 強化で 5→10。市場側データの組み合わせを増やしたため)
pub const CONTRADICTION_MAX: usize = 10;

/// 企業名寄せの対象とする上位企業数 (掲載件数順)
pub const COMPANY_MATCH_TOP_N: usize = 5;

// =============================================================================
// 拡充シグナル (2026-07-10 商談準備レポート強化) の閾値
// いずれも公的統計 (v2_external_*) または媒体CSV集計・企業データベース由来のみを入力とする。
// HW求人・時系列テーブルは一切参照しない。
// =============================================================================

/// 転出超過シグナル: 純移動率 (‰) がこの値以下で「転出超過」と判定 (負値=転出超過)
pub const NET_MIGRATION_OUTFLOW_THRESHOLD_PERMILLE: f64 = -2.0;

/// 昼間人口流出型シグナル: 昼夜間人口比率 (%) がこの値未満で「昼間流出型」と判定
pub const DAYTIME_RATIO_OUTFLOW_THRESHOLD: f64 = 97.0;

/// 開廃業シグナル: 廃業率が開業率をこの差 (ポイント) 以上上回ると「廃業超過」と判定。
/// 開業率・廃業率とも「年換算値」で比較する (元値は経済センサス調査間の累計のため)。
pub const CLOSURE_OVER_OPENING_MARGIN_PCT: f64 = 0.0;

/// 開業活発シグナル: 開業率 (%) がこの値以上で「開業が活発」と判定。
///
/// 🔴 元データ (経済センサス 開廃業) の opening_rate は「調査間の累計」であり年率ではない
/// (例: 大分県 2021年度行=29.79% は 2016→2021 の約5年累計)。本閾値は **年換算後** の
/// 開業率 (累計 ÷ 調査間年数) に対して適用する。年換算 約6% では発火させない水準に設定する
/// (2016→2021 の 29.79% は年換算 約5.96% → 発火しない)。
pub const OPENING_RATE_ACTIVE_THRESHOLD: f64 = 6.5;

/// 失業率シグナル: 県失業率が全国比この倍率未満で「労働需給が締まっている」と判定
pub const UNEMPLOYMENT_TIGHT_RATIO: f64 = 0.9;

/// 失業率シグナル: 県失業率が全国比この倍率超で「余剰寄り」と判定
pub const UNEMPLOYMENT_SLACK_RATIO: f64 = 1.1;

/// 家賃負担シグナル (廃止): median_rent_jpy の実体は「1畳あたり家賃 (円)」であり月額家賃
/// ではないため、家賃×給与の月額バランス判定は成立しない (勝手な畳数仮定で月額を捏造しない)。
/// 本定数は退行防止のため残すが、S-22 の月額バランス判定では使用しない (判定材料不足に降格)。
pub const RENT_BURDEN_RATIO_THRESHOLD: f64 = 0.30;

/// 開廃業の年換算に用いる調査間隔のデフォルト年数。fiscal_year の前行との差が取れない
/// (=単年しか無い) 場合の保守的なフォールバック。経済センサスは概ね数年間隔のため、
/// 年換算できないときはシグナルを発火させない方針 (このデフォルトは表示補助用)。
pub const BUSINESS_DYNAMICS_DEFAULT_INTERVAL_YEARS: f64 = 5.0;

/// 年間休日記載シグナル: 年間休日を記載/抽出できた求人比率がこの値未満で「記載が薄い」
pub const HOLIDAY_MENTION_THIN_RATIO: f64 = 0.5;

/// 年間休日水準シグナル: 年間休日120日以上の求人比率がこの値未満で「休日面で見劣り」
pub const HOLIDAY_GE120_LOW_RATIO: f64 = 0.3;

/// 訴求タグ多様性シグナル: 観測できた求人カードタグの種類数がこの値未満で「訴求が薄い」
pub const TAG_VARIETY_THIN_THRESHOLD: usize = 6;

/// 人気バッジ集中シグナル: 人気表示のある求人比率がこの値以上で発火
pub const POPULAR_BADGE_HIGH_RATIO: f64 = 0.25;

/// 掲載地域集中シグナル: 最多市区町村の掲載シェアがこの値以上で発火
pub const MUNI_CONCENTRATION_THRESHOLD: f64 = 0.5;

/// 成長企業シグナル: この人員増減率(%)以上を「増加企業」とみなす
pub const EMPLOYEE_GROWTH_THRESHOLD_PCT: f64 = 3.0;

/// 非正規比率シグナル: 正社員/正職員以外の求人比率がこの値以上で発火
pub const NONREGULAR_SHARE_HIGH_RATIO: f64 = 0.5;

/// 矛盾検出で追加根拠として最低限必要な発火シグナル数 (退行防止用の定数)
pub const CONTRADICTION_MIN_SIGNALS: usize = 1;

// =============================================================================
// フェーズD (2026-07-11) ヒアリング後の仮説更新 + 個社別アクションメモ の判定閾値
// いずれもヒアリング回答 (consult_hearing_results) と面談前の仮説を入力とする。
// HW求人・時系列・介護需要は一切参照しない。
// =============================================================================

/// アクションメモの優先施策として提示する最大件数 (§14.3。上位3〜5件)
pub const ACTION_MEMO_MAX_ACTIONS: usize = 5;

/// アクションメモの優先施策として提示する最小件数の目安 (§14.3。上位3〜5件)
pub const ACTION_MEMO_MIN_ACTIONS: usize = 3;

/// ファネル自動判定: 「初回連絡までの時間」が2日以降のとき応募後対応を支持方向にする条件値
pub const FIRST_CONTACT_SLOW_VALUE: &str = "2日以降";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // 閾値定数間の大小関係を退行防止として固定する意図的な定数アサーション
    #[allow(clippy::assertions_on_constants)]
    fn thresholds_are_internally_consistent() {
        assert!(SALARY_BELOW_PREF_RATIO < SALARY_ABOVE_PREF_RATIO);
        assert!(JOB_RATIO_LOW < JOB_RATIO_HIGH);
        assert!(SWITCHER_THIN_RATIO < SWITCHER_THICK_RATIO);
        assert!(SALARY_PERCENTILE_LOW < SALARY_PERCENTILE_HIGH);
        assert!(COMPETITION_POSTINGS_MEDIUM < COMPETITION_POSTINGS_HIGH);
        assert!(CONFIDENCE_MEDIUM_MIN_SOURCES < CONFIDENCE_HIGH_MIN_SOURCES);
        assert!(HYPOTHESIS_TOP_N >= 1 && CONTRADICTION_MAX >= 1);
    }

    #[test]
    // P0-1: 開業率の年換算閾値は「大分県2021 累計29.79% → 年換算(÷5)≈5.96%」で
    // 発火しない水準であること (閾値は年率基準)。実データ由来の退行防止。
    #[allow(clippy::assertions_on_constants)]
    fn opening_rate_threshold_is_annual_basis() {
        let cumulative_2021 = 29.79_f64;
        let interval_years = 5.0_f64; // 2016 → 2021
        let annualized = cumulative_2021 / interval_years;
        assert!(
            annualized < OPENING_RATE_ACTIVE_THRESHOLD,
            "年換算 {:.2}% は閾値 {:.1}% 未満で開業活発シグナルは発火しない",
            annualized,
            OPENING_RATE_ACTIVE_THRESHOLD
        );
        // 元の累計値 (誤って年率扱いした場合) では逆に閾値を超えてしまうことを明示
        assert!(
            cumulative_2021 > OPENING_RATE_ACTIVE_THRESHOLD,
            "累計値をそのまま使うと誤発火する (年換算が必須である証跡)"
        );
    }
}
