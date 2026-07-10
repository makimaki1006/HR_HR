//! 分析エンジンへの入力スナップショット
//!
//! 面談前に利用できる「市場側」データのみを保持する (計画書 §2.2 面談前 / フェーズA)。
//! 顧客ヒアリング由来のデータ (応募数・面接数等) はフェーズC/Dの領域であり含めない。
//!
//! 入力源:
//! - 今回アップロードされた媒体CSVの集計 (SurveyAggregation)
//! - 公的統計 (毎月勤労統計・最低賃金・就業構造基本調査・将来人口推計・国勢調査OD)
//! - 企業データベース (名寄せできた企業の従業員数・増減のみ)
//! - 顧客の任意入力 (提示給与・採用人数・期限・メモ)
//!
//! V2ルール: 介護データ・HW系テーブルは入力に使わない。

use serde::{Deserialize, Serialize};

/// 顧客の任意入力 (面談前に与えられる範囲のみ。§5.2 E `client_context` の部分集合)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientInput {
    /// 顧客提示給与の下限 (円。時給モードCSVなら円/時、月給モードなら円/月として扱う)
    pub target_salary_min: Option<i64>,
    /// 顧客提示給与の上限
    pub target_salary_max: Option<i64>,
    /// 採用予定人数
    pub hiring_count: Option<u32>,
    /// 採用期限 (自由記述)
    pub deadline: Option<String>,
    /// コンサル事前メモ
    pub note: Option<String>,
}

/// 企業観測 (今回CSVの掲載企業 + 企業データベース名寄せ結果)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompanyObservation {
    /// CSV上の企業名
    pub name: String,
    /// 今回CSV内の掲載件数
    pub posting_count: usize,
    /// 従業員数 (企業データベースで名寄せできた場合のみ)
    pub employee_count: Option<i64>,
    /// 1年人員増減率 (%) (名寄せできた場合のみ。正=増加、負=減少)
    pub employee_delta_1y: Option<f64>,
}

/// 分析エンジン入力 (面談前に生成できる市場側データのスナップショット)
///
/// 欠損は `None` で表現し、ゼロと区別する (§6.5)。
/// エンジンは `None` の指標について「データなし」を明示し、シグナルを発火させない。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsultInput {
    // ---- 対象 ----
    /// 対象都道府県
    pub pref: String,
    /// 対象市区町村 (取得できない場合は空)
    pub muni: String,
    /// 職種メモ (CSV支配的職種等。取得できない場合は空)
    pub occupation_note: String,
    /// データ基準日 (ブリーフ生成日)
    pub as_of: String,
    /// 利用データ一覧 (出典名)
    pub data_sources: Vec<String>,

    // ---- 今回CSV (AGGREGATED / 今回CSV粒度) ----
    /// 求人件数
    pub total_postings: usize,
    /// 新着件数
    pub new_count: usize,
    /// 時給中心CSVか (true=時給、false=月給)
    pub is_hourly: bool,
    /// 給与中央値 (月給換算。is_hourly=false のとき有効)
    pub salary_median: Option<i64>,
    /// 給与Q1 (月給換算)
    pub salary_q1: Option<i64>,
    /// 給与Q3 (月給換算)
    pub salary_q3: Option<i64>,
    /// 給与サンプル数
    pub salary_n: usize,
    /// 給与分布の生値 (月給換算・外れ値除外済。パーセンタイル計算用)
    pub salary_values: Vec<i64>,
    /// 時給下限の中央値 (円/時。is_hourly=true のとき参考値として使用)
    pub hourly_median_low: Option<i64>,
    /// 掲載経過「30+日前」比率 (0.0-1.0)。
    /// None = 現行の集計パイプラインが掲載経過テキストを保持していないため未取得。
    pub posting_age_30plus_ratio: Option<f64>,
    /// 掲載企業数
    pub company_count: usize,
    /// 掲載件数上位の企業 (企業データベース名寄せ結果込み)
    pub companies: Vec<CompanyObservation>,

    // ---- 公的統計 ----
    /// 県の所定内給与 最新月 (円/月。毎月勤労統計 地方調査)
    pub scheduled_earnings_latest: Option<f64>,
    /// 地域別最低賃金 (円/時)
    pub min_wage_hourly: Option<f64>,
    /// 最低賃金×160時間 換算 (円/月)
    pub min_wage_monthly_160h: Option<f64>,
    /// 県の有効求人倍率 (一般職業紹介状況)
    pub job_openings_ratio: Option<f64>,
    /// 県の転職希望率 (%) (就業構造基本調査)
    pub job_change_desire_rate_pref: Option<f64>,
    /// 全国の転職希望率 (%)
    pub job_change_desire_rate_national: Option<f64>,
    /// 対象市区町村の働き手増減率 (%) (将来人口推計。負値=減少)
    pub wa_decline_rate_muni: Option<f64>,
    /// 通勤流入合計 (人。国勢調査OD)
    pub commute_inflow_total: Option<i64>,
    /// 通勤流出合計 (人)
    pub commute_outflow_total: Option<i64>,
    /// 流入元上位3 (都道府県, 市区町村, 人数)
    pub commute_inflow_top3: Vec<(String, String, i64)>,

    // ---- 顧客任意入力 ----
    pub client: ClientInput,
}

impl ConsultInput {
    /// 給与分布内のパーセンタイル (0-100)。分布が空なら None。
    /// 値以下の観測数の割合で定義する (単純な経験分布)。
    pub fn salary_percentile_of(&self, value: i64) -> Option<f64> {
        if self.salary_values.is_empty() {
            return None;
        }
        let below = self.salary_values.iter().filter(|&&v| v <= value).count();
        Some(below as f64 / self.salary_values.len() as f64 * 100.0)
    }

    /// 対象地域の表示名
    pub fn region_label(&self) -> String {
        if self.muni.is_empty() {
            self.pref.clone()
        } else {
            format!("{} {}", self.pref, self.muni)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_empty_distribution_is_none() {
        let input = ConsultInput::default();
        assert_eq!(input.salary_percentile_of(250_000), None);
    }

    #[test]
    fn percentile_basic() {
        let input = ConsultInput {
            salary_values: vec![200_000, 220_000, 240_000, 260_000, 280_000],
            ..Default::default()
        };
        // 200,000以下は1/5 = 20%
        assert_eq!(input.salary_percentile_of(200_000), Some(20.0));
        // 280,000以下は5/5 = 100%
        assert_eq!(input.salary_percentile_of(280_000), Some(100.0));
        // 中央値以下は3/5 = 60%
        assert_eq!(input.salary_percentile_of(240_000), Some(60.0));
    }

    #[test]
    fn region_label_with_and_without_muni() {
        let mut input = ConsultInput {
            pref: "群馬県".to_string(),
            muni: "高崎市".to_string(),
            ..Default::default()
        };
        assert_eq!(input.region_label(), "群馬県 高崎市");
        input.muni.clear();
        assert_eq!(input.region_label(), "群馬県");
    }
}
