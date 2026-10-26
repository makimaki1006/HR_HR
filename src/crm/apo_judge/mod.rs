//! アポの自動判定 (ADR-035、設計書 §23)。取引が「アポ日確定」に入ったら、アポを取った電話の文字起こしを
//! Zoom から取り、MiniMax でアポの質を点数・区分にして、取引のプロパティ (`apo_ai_*`) に書く。
//! Slack への通知は HR_HR からは送らない。HubSpot のワークフローが `apo_ai_grade` を見て担当者に送る。
//!
//! - `judge`: プロンプト (`prompt.md`)・出力の読み取り・区分の決定
//! - `worker`: 背景で対象の取引を探し、電話を特定して文字起こしを取り、判定して書く
//!
//! 判定済みかどうかは HubSpot の `apo_ai_judged_at` の有無だけで決める (新しい表は作らない)。
//! 既定 OFF (`APO_JUDGE_ENABLED=1` のときだけ動く)。**単一インスタンス前提** (他の worker と同じ)。

use std::time::Duration;

use crate::crm::talk_review::config::DEFAULT_MODEL;
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};

pub mod judge;
pub mod worker;

pub use worker::spawn_worker;

/// 取引のプロパティ名 (`scripts/ensure_apo_ai_properties.py` と同じ)
pub const PROP_SCORE: &str = "apo_ai_score";
pub const PROP_GRADE: &str = "apo_ai_grade";
pub const PROP_REASON: &str = "apo_ai_reason";
pub const PROP_JUDGED_AT: &str = "apo_ai_judged_at";
pub const PROP_CALL_ID: &str = "apo_ai_call_id";

#[derive(Debug, Clone, PartialEq)]
pub struct ApoJudgeConfig {
    /// `APO_JUDGE_ENABLED=1`
    pub enabled: bool,
    /// `APO_JUDGE_DRY_RUN=1`: 探して判定するが HubSpot には書かない (件数と区分だけログに出す)
    pub dry_run: bool,
    pub interval: Duration,
    /// アポ日確定に入ってからこの時間以内の取引だけを見る
    pub lookback: chrono::Duration,
    /// 文字起こしが取れるのを待つ時間 (ステージに入ってから)。過ぎたら「文字起こしなし」を書く
    pub wait: chrono::Duration,
    /// 対象のステージ ID (既定は新規営業と BPO のアポ日確定)
    pub stages: Vec<String>,
    /// 1 周に判定する取引の最大数
    pub max_per_cycle: usize,
    /// 電話として使う通話時間の下限 (秒)
    pub min_talk_sec: i64,
    pub model: String,
    pub minimax_base_url: String,
}

impl ApoJudgeConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let text = |k: &str| {
            get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let num = |k: &str, lo: i64, hi: i64, default: i64| {
            text(k)
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| (lo..=hi).contains(n))
                .unwrap_or(default)
        };
        let stages = text("APO_JUDGE_STAGES")
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![ST_APO.to_string(), ST_APO_BPO.to_string()]);
        Self {
            enabled: text("APO_JUDGE_ENABLED").as_deref() == Some("1"),
            dry_run: text("APO_JUDGE_DRY_RUN").as_deref() == Some("1"),
            interval: Duration::from_secs(num("APO_JUDGE_INTERVAL_SECS", 30, 3600, 300) as u64),
            lookback: chrono::Duration::hours(num("APO_JUDGE_LOOKBACK_HOURS", 1, 720, 72)),
            wait: chrono::Duration::hours(num("APO_JUDGE_WAIT_HOURS", 1, 48, 6)),
            stages,
            max_per_cycle: num("APO_JUDGE_MAX_PER_CYCLE", 1, 100, 20) as usize,
            min_talk_sec: num("APO_JUDGE_MIN_TALK_SEC", 0, 3600, 60),
            model: text("APO_JUDGE_MODEL")
                .or_else(|| text("CALL_REVIEW_MODEL"))
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            minimax_base_url: text("MINIMAX_BASE_URL")
                .unwrap_or_else(|| crate::crm::talk_review::config::DEFAULT_BASE_URL.to_string())
                .trim_end_matches('/')
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = ApoJudgeConfig::from_lookup(|_| None);
        assert!(!c.enabled && !c.dry_run);
        assert_eq!(c.interval, Duration::from_secs(300));
        assert_eq!(c.lookback, chrono::Duration::hours(72));
        assert_eq!(c.wait, chrono::Duration::hours(6));
        assert_eq!(c.stages, vec!["52035886", "1095457875"]);
        assert_eq!((c.max_per_cycle, c.min_talk_sec), (20, 60));
        assert_eq!(c.model, "MiniMax-M3");
    }

    #[test]
    fn overrides_and_bad_values() {
        let c = ApoJudgeConfig::from_lookup(|k| {
            match k {
                "APO_JUDGE_ENABLED" => Some("1"),
                "APO_JUDGE_DRY_RUN" => Some("1"),
                "APO_JUDGE_INTERVAL_SECS" => Some("5"),
                "APO_JUDGE_LOOKBACK_HOURS" => Some("24"),
                "APO_JUDGE_WAIT_HOURS" => Some("100"),
                "APO_JUDGE_STAGES" => Some(" 111 , abc, 222 "),
                "APO_JUDGE_MODEL" => Some("m"),
                _ => None,
            }
            .map(String::from)
        });
        assert!(c.enabled && c.dry_run);
        // 範囲外は既定
        assert_eq!(c.interval, Duration::from_secs(300));
        assert_eq!(c.wait, chrono::Duration::hours(6));
        assert_eq!(c.lookback, chrono::Duration::hours(24));
        // 数字でないステージは捨てる
        assert_eq!(c.stages, vec!["111", "222"]);
        assert_eq!(c.model, "m");
    }
}
