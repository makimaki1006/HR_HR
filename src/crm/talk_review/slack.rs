//! アポの判定結果を Slack のチャンネルに 1 件ずつ送る (2026-11-05 ユーザー決定: アポ 1 件ごと、Incoming Webhook)。
//! 送るのはランク S と A のアポだけ (2026-11-10 ユーザー決定)。
//!
//! 送るのは、日程が決まった電話にヒアリングを付けたとき (= HubSpot でアポ日確定が確かめられたとき) だけ。
//! 中身は 担当者・電話の時刻・アポのランク (`apo_rank.rs`)・ヒアリングの答え・振り返りと HubSpot へのリンク。
//! 文字起こしの本文と引用は送らない。
//!
//! - `TALK_REVIEW_SLACK_WEBHOOK_URL`: チャンネルの Incoming Webhook の URL (秘密。ログに出さない)。無ければ何も送らない
//! - `TALK_REVIEW_SLACK_TEAMS`: 送る担当者のチーム (カンマ区切り)。既定は BPO (`APPOINTMENT_TEAMS`)
//! - `TALK_REVIEW_APP_URL`: 振り返りの画面の URL の頭。既定 `https://hr-hw.onrender.com`
//!
//! §23 では「HR_HR は Slack の鍵を持たない」としていたが、取引に付かない電話の判定結果を送るため、
//! このチャンネルの Webhook だけは HR_HR が持つ (2026-11-05 ユーザー決定)。
//! 送れなかった分は 3 回まで送り直し、それでも駄目なら捨てる (再起動をまたいで覚えない。二重に送らないことを優先する)。

use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use serde_json::json;

use super::apo_rank;
use super::judge::StoredItem;
use crate::crm::caller_metrics::APPOINTMENT_TEAMS;
use crate::hubspot::deep_link::hubspot_portal_id;

/// これより古い電話は送らない (起動直後に昔の分をまとめて送らないため。ヒアリングはアポの記録を 72 時間まで待つ)
pub const MAX_AGE: chrono::Duration = chrono::Duration::hours(96);
const ATTEMPTS: u32 = 3;

/// 送る答えの項目と表示名 (checks.json の順)
const HEARING: [(&str, &str); 6] = [
    ("aq_budget", "予算"),
    ("aq_timing", "入社時期"),
    ("aq_job", "職種"),
    ("aq_reason", "募集の理由"),
    ("aq_decider", "判断する人"),
    ("aq_attend", "判断する人の同席"),
];

#[derive(Clone)]
pub struct SlackNotifier {
    webhook: String,
    teams: Vec<String>,
    app_url: String,
    http: reqwest::Client,
}

/// 1 件の通知の材料
pub struct ApoNotice<'a> {
    pub call_id: &'a str,
    pub owner_id: &'a str,
    pub owner_name: &'a str,
    pub team: &'a str,
    pub call_at: DateTime<Utc>,
    pub items: &'a [StoredItem],
}

impl SlackNotifier {
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let text = |k: &str| {
            get(k)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let webhook = text("TALK_REVIEW_SLACK_WEBHOOK_URL")?;
        if !webhook.starts_with("https://hooks.slack.com/") {
            tracing::warn!("talk review: TALK_REVIEW_SLACK_WEBHOOK_URL が Slack の Webhook の形でないため送りません");
            return None;
        }
        let teams = text("TALK_REVIEW_SLACK_TEAMS")
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| APPOINTMENT_TEAMS.iter().map(|s| s.to_string()).collect());
        let app_url = app_url_from(&get);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .ok()?;
        Some(Self {
            webhook,
            teams,
            app_url,
            http,
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(webhook: String) -> Self {
        Self {
            webhook,
            teams: vec!["BPO_リクロジ".into()],
            app_url: "https://example.test".into(),
            http: reqwest::Client::new(),
        }
    }

    /// このチームの担当者の電話を送るか
    pub fn covers(&self, team: &str) -> bool {
        self.teams.iter().any(|t| t == team)
    }

    /// 送る。ランクが S・A でない (ヒアリングの対象外を含む) ・古い電話・対象外のチームは送らない。送ったら true
    pub async fn send(&self, n: &ApoNotice<'_>, now: DateTime<Utc>) -> bool {
        if !self.covers(n.team) || now - n.call_at > MAX_AGE {
            return false;
        }
        // 送るのは S と A だけ (2026-11-10 ユーザー決定)
        if !apo_rank::rank(n.items).is_some_and(|r| super::slack_bot::NOTIFY_RANKS.contains(&r)) {
            return false;
        }
        let Some(text) = message(n, &self.app_url) else {
            return false;
        };
        let body = json!({ "text": text });
        for attempt in 1..=ATTEMPTS {
            match self.http.post(&self.webhook).json(&body).send().await {
                Ok(r) if r.status().is_success() => return true,
                Ok(r) => {
                    tracing::warn!(
                        status = r.status().as_u16(),
                        attempt,
                        "talk review: slack post failed"
                    )
                }
                Err(e) => {
                    tracing::warn!(
                        timeout = e.is_timeout(),
                        attempt,
                        "talk review: slack post failed"
                    )
                }
            }
            tokio::time::sleep(Duration::from_secs(2 * attempt as u64)).await;
        }
        false
    }
}

/// 振り返りの画面の URL の頭 (`TALK_REVIEW_APP_URL`、既定 `https://hr-hw.onrender.com`)
pub fn app_url_from(get: &impl Fn(&str) -> Option<String>) -> String {
    get("TALK_REVIEW_APP_URL")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://hr-hw.onrender.com".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn jst(t: DateTime<Utc>) -> DateTime<FixedOffset> {
    t.with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
}

/// Slack に送る文 (mrkdwn)。ランクが付かない電話は `None`
pub fn message(n: &ApoNotice<'_>, app_url: &str) -> Option<String> {
    let rank = apo_rank::rank(n.items)?;
    let at = jst(n.call_at);
    let day = at.format("%Y-%m-%d").to_string();
    let answers: Vec<String> = HEARING
        .iter()
        .map(|(id, label)| {
            let a = n
                .items
                .iter()
                .find(|i| i.id == *id)
                .and_then(|i| i.answer.as_deref())
                .unwrap_or("不明");
            format!("{label}: {a}")
        })
        .collect();
    let review = format!(
        "{app_url}/app/admin?view=talk&tab=after&from={day}&to={day}&owner={}&call={}",
        n.owner_id, n.call_id
    );
    let hubspot = format!(
        "https://app.hubspot.com/contacts/{}/record/0-48/{}/",
        hubspot_portal_id(),
        n.call_id
    );
    Some(format!(
        "*アポ ランク {rank}*　{}（{}）{}\n{}\n<{review}|振り返りで開く>　<{hubspot}|HubSpot の通話>",
        n.owner_name,
        n.team,
        at.format("%m/%d %H:%M"),
        answers.join(" / "),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: &str, answer: &str) -> StoredItem {
        StoredItem {
            id: id.into(),
            status: "done".into(),
            evidence: vec![],
            quote: Some("引用は送らない".into()),
            advice: None,
            note: None,
            answer: Some(answer.into()),
            asked: Some(true),
            answered: Some(true),
            counted_from: None,
        }
    }

    fn notifier(teams: Option<&str>) -> Option<SlackNotifier> {
        SlackNotifier::from_lookup(|k| match k {
            "TALK_REVIEW_SLACK_WEBHOOK_URL" => {
                Some("https://hooks.slack.com/services/T/B/x".into())
            }
            "TALK_REVIEW_SLACK_TEAMS" => teams.map(Into::into),
            _ => None,
        })
    }

    #[test]
    fn message_has_rank_answers_and_links_but_no_quotes() {
        let items = vec![
            it("aq_budget", "具体額"),
            it("aq_timing", "3か月以内"),
            it("aq_job", "現場系"),
            it("aq_reason", "欠員"),
            it("aq_decider", "本人が判断者"),
        ];
        let n = ApoNotice {
            call_id: "123",
            owner_id: "77",
            owner_name: "山田",
            team: "BPO_リクロジ",
            call_at: "2026-11-05T01:30:00Z".parse().unwrap(),
            items: &items,
        };
        let m = message(&n, "https://example.test").unwrap();
        assert!(
            m.starts_with("*アポ ランク S*　山田（BPO_リクロジ）11/05 10:30\n"),
            "{m}"
        );
        assert!(m.contains("予算: 具体額 / 入社時期: 3か月以内 / 職種: 現場系 / 募集の理由: 欠員 / 判断する人: 本人が判断者 / 判断する人の同席: 不明"));
        assert!(m.contains("<https://example.test/app/admin?view=talk&tab=after&from=2026-11-05&to=2026-11-05&owner=77&call=123|振り返りで開く>"));
        assert!(m.contains("/record/0-48/123/|HubSpot の通話>"));
        assert!(!m.contains("引用は送らない"));
        // ヒアリングの対象でない電話は送らない
        let none = ApoNotice { items: &[], ..n };
        assert_eq!(message(&none, "https://example.test"), None);
    }

    #[test]
    fn config_needs_a_slack_webhook_and_defaults_to_the_bpo_team() {
        assert!(SlackNotifier::from_lookup(|_| None).is_none());
        assert!(SlackNotifier::from_lookup(
            |k| (k == "TALK_REVIEW_SLACK_WEBHOOK_URL").then(|| "https://example.com/x".into())
        )
        .is_none());
        let n = notifier(None).unwrap();
        assert!(n.covers("BPO_リクロジ"));
        assert!(!n.covers("新規営業"));
        let n = notifier(Some("新規営業, BPO_リクロジ")).unwrap();
        assert!(n.covers("新規営業") && n.covers("BPO_リクロジ"));
    }
}
