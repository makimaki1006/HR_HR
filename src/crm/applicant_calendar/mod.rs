//! 応募 (0-421) の日付を「管理用カレンダー」の Google カレンダーに載せる (ADR-037、設計書 §24)。
//!
//! - `config`: 環境変数。載せる項目 (`APPLICANT_CALENDAR_PROPS`) は設定で持ち、コードに固定しない
//! - この mod: 予定の ID・形・名前を決める純粋な関数 (テストしやすいように副作用を持たない)
//! - `google`: Calendar API と、管理用カレンダーのアカウントの許可 (監査 Turso の `applicant_calendar_grants`)
//! - `sync`: 背景の周回 (HubSpot を読み、予定を作る・移す・消す)
//! - `routes`: 管理画面の API (許可・取り消し・状態)
//!
//! 予定の形 (時刻つき / 終日) は HubSpot のプロパティ定義の型 (`datetime` / `date`) で決める。
//! 予定と応募の結び付けは表を作らず、予定の ID を応募 ID と項目名から決定的に作って行う。
//! 同期の状態は Turso に書かない (プロセス内だけ)。**単一インスタンス前提**。

use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, NaiveDate, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub mod config;
pub mod google;
pub mod routes;
pub mod sync;

#[cfg(test)]
mod tests;

pub use config::CalendarConfig;
pub use google::ApplicantCalendar;
pub use sync::spawn_worker;

/// 予定の拡張プロパティ (private) に書く印。自分が作った予定だけを消す・数えるのに使う
pub const MARK_KEY: &str = "hrhr";
pub const MARK_VALUE: &str = "applicant";
pub const APPLICANT_KEY: &str = "hrhr_applicant";
pub const PROP_KEY: &str = "hrhr_prop";

/// 予定の名前の区切り (全角の縦棒)
pub const SEP: &str = "｜";

/// 日本時間 (夏時間なし)
pub fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("JST")
}

/// Google の予定 ID (base32hex の a-v と 0-9、5〜1024 文字)。
/// 応募 ID と項目名から決定的に作るので、再起動しても同じ予定を指す (二重に作らない)。
pub fn event_id(applicant_id: &str, prop: &str) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";
    let digest = Sha256::digest(format!("{applicant_id}:{prop}").as_bytes());
    // 先頭 20 バイト = 160 ビット = 32 文字
    let mut out = String::from("hrhr");
    let mut buffer: u64 = 0;
    let mut bits = 0u32;
    for b in &digest[..20] {
        buffer = (buffer << 8) | u64::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    out
}

/// 予定の日時
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// 時刻つき (HubSpot の `datetime`)
    Timed(DateTime<Utc>),
    /// 終日 (HubSpot の `date`)
    AllDay(NaiveDate),
}

/// HubSpot の値の読み方。ミリ秒・ISO 8601・`YYYY-MM-DD` のどれでも読む。
/// 型 (`datetime` / `date`) で予定の形を決める。それ以外の型・読めない値は `None` (載せない)。
pub fn when_of(kind: &str, raw: &str) -> Option<When> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let instant = if raw.bytes().all(|b| b.is_ascii_digit()) {
        raw.parse::<i64>()
            .ok()
            .and_then(DateTime::<Utc>::from_timestamp_millis)
    } else if let Ok(t) = DateTime::parse_from_rfc3339(raw) {
        Some(t.with_timezone(&Utc))
    } else {
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .map(|t| t.and_utc())
    }?;
    match kind {
        "datetime" => Some(When::Timed(instant)),
        // HubSpot の日付型は UTC の午前 0 時で持つので、UTC の日付がその日
        "date" => Some(When::AllDay(instant.date_naive())),
        _ => None,
    }
}

/// 載せたい予定 1 件
#[derive(Debug, Clone, PartialEq)]
pub struct Desired {
    pub applicant_id: String,
    pub prop: String,
    pub summary: String,
    pub description: String,
    pub when: When,
    /// ゲスト (コンサル担当)。小文字
    pub attendees: Vec<String>,
}

/// 予定の名前: 「面接｜応募者名｜会社名」。名前・会社名が空ならその部分を省く
pub fn summary(kind_label: &str, applicant_name: &str, company: &str) -> String {
    let mut parts = vec![kind_label.trim()];
    for p in [applicant_name.trim(), company.trim()] {
        if !p.is_empty() {
            parts.push(p);
        }
    }
    parts.join(SEP)
}

/// 説明欄。個人情報は名前・会社名・リンクだけ
pub fn description(applicant_name: &str, company: &str, job: &str, link: &str) -> String {
    let mut lines = Vec::new();
    if !applicant_name.trim().is_empty() {
        lines.push(format!("応募者: {}", applicant_name.trim()));
    }
    if !company.trim().is_empty() {
        lines.push(format!("会社: {}", company.trim()));
    }
    if !job.trim().is_empty() {
        lines.push(format!("求人: {}", job.trim()));
    }
    lines.push(format!("HubSpot の応募: {link}"));
    lines.push(String::new());
    lines.push(
        "HubSpot の応募の日付から HR_HR が自動で作った予定です。日付の変更は HubSpot の応募で行ってください。"
            .into(),
    );
    lines.join("\n")
}

/// 応募の HubSpot へのリンク (既存のディープリンクと同じ形)
pub fn applicant_link(portal_id: &str, applicant_id: &str) -> String {
    format!("https://app.hubspot.com/contacts/{portal_id}/record/0-421/{applicant_id}/")
}

fn time_fields(when: When, minutes: i64, end: bool) -> Value {
    match when {
        When::Timed(t) => {
            let t = if end {
                t + ChronoDuration::minutes(minutes)
            } else {
                t
            };
            json!({
                "dateTime": t.with_timezone(&jst()).to_rfc3339(),
                "timeZone": "Asia/Tokyo",
                "date": null,
            })
        }
        When::AllDay(d) => {
            let d = if end { d.succ_opt().unwrap_or(d) } else { d };
            json!({
                "date": d.format("%Y-%m-%d").to_string(),
                "dateTime": null,
                "timeZone": null,
            })
        }
    }
}

/// 上書き (patch) 用の本文。終日⇔時刻つきを切り替えられるよう、使わない側を null で消す
pub fn patch_body(d: &Desired, minutes: i64) -> Value {
    json!({
        "summary": d.summary,
        "description": d.description,
        "start": time_fields(d.when, minutes, false),
        "end": time_fields(d.when, minutes, true),
        "attendees": d.attendees.iter().map(|e| json!({"email": e})).collect::<Vec<_>>(),
        "status": "confirmed",
        "extendedProperties": {"private": {
            MARK_KEY: MARK_VALUE,
            APPLICANT_KEY: d.applicant_id,
            PROP_KEY: d.prop,
        }},
    })
}

/// 作成 (insert) 用の本文。ID を付け、null は送らない
pub fn insert_body(d: &Desired, minutes: i64) -> Value {
    let mut v = strip_nulls(patch_body(d, minutes));
    v["id"] = json!(event_id(&d.applicant_id, &d.prop));
    v
}

fn strip_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        other => other,
    }
}

fn instant(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn same_time(existing: &Value, when: When, minutes: i64, end: bool) -> bool {
    match when {
        When::Timed(t) => {
            let want = if end {
                t + ChronoDuration::minutes(minutes)
            } else {
                t
            };
            existing.get("date").is_none_or(Value::is_null)
                && instant(&existing["dateTime"]) == Some(want)
        }
        When::AllDay(d) => {
            let want = if end { d.succ_opt().unwrap_or(d) } else { d };
            existing.get("dateTime").is_none_or(Value::is_null)
                && existing["date"].as_str() == Some(want.format("%Y-%m-%d").to_string().as_str())
        }
    }
}

/// カレンダーにある予定が、載せたい内容と同じか (同じなら書かない)
pub fn same(existing: &Value, d: &Desired, minutes: i64) -> bool {
    let text = |k: &str| existing[k].as_str().unwrap_or("");
    let mut have: Vec<String> = existing["attendees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["email"].as_str())
        .map(str::to_ascii_lowercase)
        .collect();
    have.sort();
    let mut want = d.attendees.clone();
    want.sort();
    let private = &existing["extendedProperties"]["private"];
    text("status") == "confirmed"
        && text("summary") == d.summary
        && text("description") == d.description
        && same_time(&existing["start"], d.when, minutes, false)
        && same_time(&existing["end"], d.when, minutes, true)
        && have == want
        && private[MARK_KEY] == MARK_VALUE
        && private[APPLICANT_KEY] == d.applicant_id.as_str()
        && private[PROP_KEY] == d.prop.as_str()
}
