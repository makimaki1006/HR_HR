//! トークの振り返りの設定 (環境変数)。値の読み取りは `from_lookup` に分けてテストで env を触らない。

use std::time::Duration;

use crate::crm::caller_metrics::APPOINTMENT_TEAMS;
use crate::crm::caller_metrics_month::SALES_TEAM;

pub const DEFAULT_MODEL: &str = "MiniMax-M3";
pub const DEFAULT_BASE_URL: &str = "https://api.minimax.io/v1";

#[derive(Debug, Clone, PartialEq)]
pub struct TalkReviewConfig {
    /// `CALL_REVIEW_ENABLED=1` のときだけ worker が動き、API が中身を返す
    pub enabled: bool,
    pub model: String,
    pub minimax_base_url: String,
    pub backfill_days: i64,
    pub interval: Duration,
    /// 判定の対象にする HubSpot のチーム。
    /// 既定は BPO (`APPOINTMENT_TEAMS` = BPO_リクロジ) と新規営業 (`SALES_TEAM`、`caller_metrics_month` が
    /// 「今月」を出すチームと同じ名前。設計書 §19)。
    pub teams: Vec<String>,
}

impl TalkReviewConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let text = |k: &str| {
            get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let teams = text("CALL_REVIEW_TEAMS")
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                let mut v: Vec<String> = APPOINTMENT_TEAMS.iter().map(|s| s.to_string()).collect();
                v.push(SALES_TEAM.to_string());
                v
            });
        Self {
            enabled: text("CALL_REVIEW_ENABLED").as_deref() == Some("1"),
            model: text("CALL_REVIEW_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            minimax_base_url: text("MINIMAX_BASE_URL")
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
                .trim_end_matches('/')
                .to_string(),
            backfill_days: text("CALL_REVIEW_BACKFILL_DAYS")
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| (1..=62).contains(n))
                .unwrap_or(14),
            interval: Duration::from_secs(
                text("CALL_REVIEW_INTERVAL_SECS")
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| *n >= 30)
                    .unwrap_or(600),
            ),
            teams,
        }
    }

    /// BPO の数え方 (アポ獲得者を優先) を使うチームか。それ以外は営業KPI の数え方 (取引の担当者)
    pub fn is_bpo_team(team: &str) -> bool {
        APPOINTMENT_TEAMS.contains(&team)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = TalkReviewConfig::from_lookup(|_| None);
        assert!(!c.enabled);
        assert_eq!(c.model, "MiniMax-M3");
        assert_eq!(c.minimax_base_url, "https://api.minimax.io/v1");
        assert_eq!(c.backfill_days, 14);
        assert_eq!(c.interval, Duration::from_secs(600));
        assert_eq!(
            c.teams,
            vec!["BPO_リクロジ".to_string(), "新規営業".to_string()]
        );
    }

    #[test]
    fn overrides_and_bad_values() {
        let c = TalkReviewConfig::from_lookup(|k| {
            match k {
                "CALL_REVIEW_ENABLED" => Some("1"),
                "CALL_REVIEW_MODEL" => Some("m"),
                "CALL_REVIEW_BACKFILL_DAYS" => Some("999"),
                "CALL_REVIEW_INTERVAL_SECS" => Some("5"),
                "CALL_REVIEW_TEAMS" => Some(" A , B ,"),
                "MINIMAX_BASE_URL" => Some("http://x/v1/"),
                _ => None,
            }
            .map(String::from)
        });
        assert!(c.enabled);
        assert_eq!(c.model, "m");
        assert_eq!(c.backfill_days, 14);
        assert_eq!(c.interval, Duration::from_secs(600));
        assert_eq!(c.teams, vec!["A", "B"]);
        assert_eq!(c.minimax_base_url, "http://x/v1");
    }
}
