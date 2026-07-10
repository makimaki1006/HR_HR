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

/// 矛盾の最大表示数 (§12.3)
pub const CONTRADICTION_MAX: usize = 5;

/// 企業名寄せの対象とする上位企業数 (掲載件数順)
pub const COMPANY_MATCH_TOP_N: usize = 5;

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
}
