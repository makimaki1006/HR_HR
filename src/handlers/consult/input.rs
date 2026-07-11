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

    // ---- 公的統計 (拡充 2026-07-10) ----
    /// 純移動率 (‰。住民基本台帳人口移動報告。負値=転出超過)
    pub net_migration_rate: Option<f64>,
    /// 昼夜間人口比率 (%。国勢調査。100未満=昼間人口が流出)
    pub daytime_ratio: Option<f64>,
    /// 開業率 (%。経済センサス 開廃業)。
    /// 🔴 これは「経済センサス調査間の累計」であり年率ではない (例: 2021年度行=29.79% は
    /// 2016→2021 の約5年累計)。年率で解釈・判定する場合は `business_dynamics_interval_years`
    /// で割って年換算する (`annualized_opening_rate()`)。
    pub business_opening_rate: Option<f64>,
    /// 廃業率 (%。経済センサス 開廃業)。開業率と同じく調査間の累計。
    pub business_closure_rate: Option<f64>,
    /// 開廃業率の調査間隔 (年)。当該県の開廃業時系列で「最新年度 - その1つ前の年度」から算出。
    /// 前年度行が取れない (単年しかない) 場合は None = 年換算不能 → 年率判定は行わない。
    pub business_dynamics_interval_years: Option<f64>,
    /// 県の失業率 (%。国勢調査 労働力状態)
    pub unemployment_rate_pref: Option<f64>,
    /// 全国の失業率 (%)
    pub unemployment_rate_national: Option<f64>,
    /// 自然増減 (人。人口動態統計 出生-死亡。負値=自然減)
    pub natural_change: Option<i64>,
    /// 1畳あたり家賃 (円。住宅・土地統計 「総数/総数」の median_rent_jpy)。
    /// 🔴 これは月額家賃ではなく「1畳あたり」の家賃 (例: 東京都 総数=2,274円・大分県=917円)。
    /// 月額家賃としての比較・シグナルには使わない (勝手な畳数仮定で月額を捏造しない)。
    /// 県間の相対位置 (全国中央値比) の用途に限る。
    pub rent_per_tatami: Option<i64>,
    /// 全国の1畳あたり家賃 (円。相対位置の基準)。取得できない場合 None。
    pub rent_per_tatami_national: Option<i64>,

    // ---- 今回CSV (拡充: 求人カード観測。§5.2 A) ----
    /// 観測できた求人カードタグの種類数 (§5.0: 福利厚生の完全一覧ではない)
    pub distinct_tag_count: usize,
    /// 掲載件数上位のタグ (タグ名, 件数)
    pub top_tags: Vec<(String, usize)>,
    /// 人気/超人気バッジのある求人比率 (0.0-1.0)。取得できない場合 None
    pub popular_ratio: Option<f64>,
    /// 超人気バッジ件数
    pub super_popular_count: usize,
    /// 年間休日の中央値 (日。記載/AI抽出できた求人のみ。§5.0: 欠落は否定情報でない)
    pub annual_holidays_median: Option<i64>,
    /// 年間休日を記載/抽出できた求人数
    pub annual_holidays_n: usize,
    /// 年間休日120日以上の求人比率 (0.0-1.0)
    pub holiday_pct_ge_120: Option<f64>,
    /// 雇用形態分布 (雇用形態, 件数)。取得できたCSVのみ (§5.0: 一方のCSVにしかない)
    pub employment_type_dist: Vec<(String, usize)>,
    /// 掲載件数上位の市区町村 (市区町村名, 件数)
    pub muni_dist_top: Vec<(String, usize)>,

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

    /// 新着求人比率 (0.0-1.0)。総件数0なら None。
    pub fn new_ratio(&self) -> Option<f64> {
        if self.total_postings == 0 {
            None
        } else {
            Some(self.new_count as f64 / self.total_postings as f64)
        }
    }

    /// 年間休日を記載/抽出できた求人の比率 (0.0-1.0)。総件数0なら None。
    /// §5.0: これは「記載を確認できた」比率であり、記載がない=休日がないではない。
    pub fn holiday_mention_ratio(&self) -> Option<f64> {
        if self.total_postings == 0 {
            None
        } else {
            Some(self.annual_holidays_n as f64 / self.total_postings as f64)
        }
    }

    /// 正社員/正職員以外 (パート・アルバイト・契約等) の求人比率 (0.0-1.0)。
    /// 雇用形態分布が空なら None。
    pub fn nonregular_share(&self) -> Option<f64> {
        if self.employment_type_dist.is_empty() {
            return None;
        }
        let total: usize = self.employment_type_dist.iter().map(|(_, n)| *n).sum();
        if total == 0 {
            return None;
        }
        let regular: usize = self
            .employment_type_dist
            .iter()
            .filter(|(t, _)| t.contains("正社員") || t.contains("正職員"))
            .map(|(_, n)| *n)
            .sum();
        Some((total - regular) as f64 / total as f64)
    }

    /// 最多掲載市区町村のシェア (0.0-1.0)。分布が空/総件数0なら None。
    pub fn top_muni_share(&self) -> Option<(String, f64)> {
        if self.total_postings == 0 {
            return None;
        }
        self.muni_dist_top
            .iter()
            .max_by_key(|(_, n)| *n)
            .map(|(name, n)| (name.clone(), *n as f64 / self.total_postings as f64))
    }

    /// 年換算した開業率 (%)。累計値を調査間年数で割る。
    /// 調査間隔が取れない (None) か 0 以下なら年換算不能として None を返す。
    pub fn annualized_opening_rate(&self) -> Option<f64> {
        match (
            self.business_opening_rate,
            self.business_dynamics_interval_years,
        ) {
            (Some(cum), Some(years)) if years > 0.0 => Some(cum / years),
            _ => None,
        }
    }

    /// 年換算した廃業率 (%)。開業率と同じく累計を調査間年数で割る。
    pub fn annualized_closure_rate(&self) -> Option<f64> {
        match (
            self.business_closure_rate,
            self.business_dynamics_interval_years,
        ) {
            (Some(cum), Some(years)) if years > 0.0 => Some(cum / years),
            _ => None,
        }
    }

    /// 1畳あたり家賃の全国中央値に対する相対比 (県 / 全国)。
    /// どちらか欠損、または全国が 0 以下なら None。月額換算は行わない。
    pub fn rent_relative_to_national(&self) -> Option<f64> {
        match (self.rent_per_tatami, self.rent_per_tatami_national) {
            (Some(pref), Some(nat)) if nat > 0 => Some(pref as f64 / nat as f64),
            _ => None,
        }
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

    #[test]
    fn annualized_opening_rate_divides_cumulative_by_interval() {
        // 大分県2021: 累計29.79% / 5年 ≈ 5.96%
        let input = ConsultInput {
            business_opening_rate: Some(29.79),
            business_dynamics_interval_years: Some(5.0),
            ..Default::default()
        };
        let ann = input.annualized_opening_rate().unwrap();
        assert!((ann - 5.958).abs() < 0.01, "年換算値: {}", ann);
        // 調査間隔が無ければ年換算不能
        let no_interval = ConsultInput {
            business_opening_rate: Some(29.79),
            ..Default::default()
        };
        assert_eq!(no_interval.annualized_opening_rate(), None);
    }

    #[test]
    fn rent_relative_needs_both_pref_and_national() {
        let input = ConsultInput {
            rent_per_tatami: Some(917),
            rent_per_tatami_national: Some(1200),
            ..Default::default()
        };
        let r = input.rent_relative_to_national().unwrap();
        assert!((r - 0.764).abs() < 0.01, "相対比: {}", r);
        // 全国基準が無ければ None (月額換算はしない)
        let only_pref = ConsultInput {
            rent_per_tatami: Some(917),
            ..Default::default()
        };
        assert_eq!(only_pref.rent_relative_to_national(), None);
    }
}
