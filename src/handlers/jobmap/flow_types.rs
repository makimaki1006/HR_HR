//! Agoop 人流データクエリの集計モード型強制
//!
//! dayflag/timezone の集計値（=2）との double count を**型システムで防御**する。
//! SQLクエリビルダは必ず `AggregateMode::where_clause()` を経由させ、
//! 生値と集計値の混在を機械的に排除する。
//!
//! # 前提（Phase 0 マスタ調査 2026-04-20 確定）
//!
//! - **dayflag**: 0=休日 / 1=平日 / **2=全日（集計値）**
//! - **timezone**: 0=昼(11-14h) / 1=深夜(1-4h) / **2=終日（集計値）**
//!
//! "=2" は0+1の合計値なので、生値とSUM()すると double count になる。
//! 本enumで4つの組合せ以外を許可しないことで、実装ミスを防ぐ。

use std::fmt;

/// dayflag × timezone の集計モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateMode {
    /// dayflag IN (0,1) AND timezone IN (0,1)
    /// 生値のみ（平日/休日 × 昼/深夜の4象限）。
    /// **分析の基本モード**。SUM等で合算しても double count しない。
    Raw,
    /// dayflag = 2 AND timezone IN (0,1)
    /// 全日集計（平休日無視）× 時間帯別（昼/深夜）。
    DayAgg,
    /// dayflag IN (0,1) AND timezone = 2
    /// 平日/休日別 × 終日集計（昼夜合算）。
    TimeAgg,
    /// dayflag = 2 AND timezone = 2
    /// 全日×終日（最大の集約、単一値）。
    FullAgg,
}

impl AggregateMode {
    /// SQLのWHERE句を返す。必ずこの文字列を経由させる。
    pub fn where_clause(&self) -> &'static str {
        match self {
            Self::Raw => "dayflag IN (0,1) AND timezone IN (0,1)",
            Self::DayAgg => "dayflag = 2 AND timezone IN (0,1)",
            Self::TimeAgg => "dayflag IN (0,1) AND timezone = 2",
            Self::FullAgg => "dayflag = 2 AND timezone = 2",
        }
    }

    /// APIパラメータ (dayflag, timezone) から AggregateMode を決定する。
    ///
    /// # Errors
    /// dayflag が 0/1/2 以外、timezone が 0/1/2 以外、
    /// または両方とも集計値でない特殊組合せの場合に `AggregateModeError` を返す。
    pub fn from_params(dayflag: i32, timezone: i32) -> Result<Self, AggregateModeError> {
        match (dayflag, timezone) {
            (0, 0) | (0, 1) | (1, 0) | (1, 1) => Ok(Self::Raw),
            (2, 0) | (2, 1) => Ok(Self::DayAgg),
            (0, 2) | (1, 2) => Ok(Self::TimeAgg),
            (2, 2) => Ok(Self::FullAgg),
            _ => Err(AggregateModeError::InvalidParams { dayflag, timezone }),
        }
    }

    /// 粒度メタデータ（APIレスポンス `meta.aggregate_mode` 用）
    pub fn label(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::DayAgg => "day_aggregated",
            Self::TimeAgg => "time_aggregated",
            Self::FullAgg => "full_aggregated",
        }
    }
}

impl fmt::Display for AggregateMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// AggregateMode への変換エラー
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateModeError {
    InvalidParams { dayflag: i32, timezone: i32 },
}

impl fmt::Display for AggregateModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParams { dayflag, timezone } => write!(
                f,
                "Invalid dayflag/timezone: dayflag={dayflag}, timezone={timezone} (valid: 0/1/2 each)"
            ),
        }
    }
}

impl std::error::Error for AggregateModeError {}

/// API レスポンスの粒度メタ情報
#[derive(Debug, Clone, serde::Serialize)]
pub struct FlowMeta {
    /// クエリ粒度: "mesh1km" / "mesh3km" / "city"
    pub granularity: &'static str,
    /// 集計モード: "raw" / "day_aggregated" / "time_aggregated" / "full_aggregated"
    pub aggregate_mode: &'static str,
    /// データソース: "国土交通省 全国の人流オープンデータ（Agoop社提供）"
    pub data_source: &'static str,
    /// データ期間: "2019-01 〜 2021-12"
    pub data_period: &'static str,
    /// コロナバイアス注記（2020/2021データ使用時）
    pub covid_notice: Option<&'static str>,
}

impl FlowMeta {
    pub fn new(granularity: &'static str, mode: AggregateMode, year: i32) -> Self {
        let covid_notice = if year == 2020 || year == 2021 {
            Some("2020-2021年は新型コロナウイルス影響下のデータのため、通常年とは異なる人流パターンを含みます")
        } else {
            None
        };
        Self {
            granularity,
            aggregate_mode: mode.label(),
            data_source: "国土交通省 全国の人流オープンデータ（Agoop社提供）",
            data_period: "2019-01〜2021-12",
            covid_notice,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_where_clause_double_count_prevention() {
        // 最重要: 生値モードは dayflag=2 / timezone=2 を絶対に含まない
        let clause = AggregateMode::Raw.where_clause();
        assert!(clause.contains("dayflag IN (0,1)"));
        assert!(clause.contains("timezone IN (0,1)"));
        assert!(!clause.contains("dayflag = 2"));
        assert!(!clause.contains("timezone = 2"));
    }

    #[test]
    fn all_modes_produce_distinct_sql() {
        let modes = [
            AggregateMode::Raw,
            AggregateMode::DayAgg,
            AggregateMode::TimeAgg,
            AggregateMode::FullAgg,
        ];
        let clauses: Vec<_> = modes.iter().map(|m| m.where_clause()).collect();
        // 4モードのSQL句は全て異なる
        for (i, a) in clauses.iter().enumerate() {
            for (j, b) in clauses.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "Modes {:?} and {:?} produce same SQL",
                        modes[i], modes[j]
                    );
                }
            }
        }
    }

    #[test]
    fn from_params_valid_combinations() {
        assert_eq!(
            AggregateMode::from_params(0, 0).unwrap(),
            AggregateMode::Raw
        );
        assert_eq!(
            AggregateMode::from_params(1, 1).unwrap(),
            AggregateMode::Raw
        );
        assert_eq!(
            AggregateMode::from_params(2, 0).unwrap(),
            AggregateMode::DayAgg
        );
        assert_eq!(
            AggregateMode::from_params(1, 2).unwrap(),
            AggregateMode::TimeAgg
        );
        assert_eq!(
            AggregateMode::from_params(2, 2).unwrap(),
            AggregateMode::FullAgg
        );
    }

    #[test]
    fn from_params_rejects_invalid() {
        assert!(AggregateMode::from_params(3, 0).is_err());
        assert!(AggregateMode::from_params(0, 3).is_err());
        assert!(AggregateMode::from_params(-1, 0).is_err());
    }

    #[test]
    fn covid_notice_on_2020_2021() {
        let m2019 = FlowMeta::new("mesh1km", AggregateMode::Raw, 2019);
        assert!(m2019.covid_notice.is_none());

        let m2020 = FlowMeta::new("mesh1km", AggregateMode::Raw, 2020);
        assert!(m2020.covid_notice.is_some());

        let m2021 = FlowMeta::new("mesh1km", AggregateMode::Raw, 2021);
        assert!(m2021.covid_notice.is_some());
    }

    #[test]
    fn label_matches_aggregate_mode() {
        assert_eq!(AggregateMode::Raw.label(), "raw");
        assert_eq!(AggregateMode::DayAgg.label(), "day_aggregated");
        assert_eq!(AggregateMode::TimeAgg.label(), "time_aggregated");
        assert_eq!(AggregateMode::FullAgg.label(), "full_aggregated");
    }
}
