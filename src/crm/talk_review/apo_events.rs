//! アポの記録 (2026-11-15 ユーザー決定)。過去の期間のアポ数が後から減らないように、アポ 1 件を 1 行で控える。
//!
//! HubSpot の `hs_v2_date_entered_<ステージ>` は「最後に入った時刻」なので、アポ日確定に入り直した取引は
//! 前に入った日から消える (9 月: 今の数え方 969 件・入った回数 1,174 件)。ここでは入るたびに 1 件として残す。
//!
//! - 1 行 = 取引 1 件がアポ日確定 (`ST_APO` / `ST_APO_BPO`) に入った JST の日 1 日。同じ日に入り直した分・
//!   同じ日に BPO と新規営業の両方へ入った分は 1 件 (主キー = 取引 ID と日)。再商談・再アポ・日程の取り直しも含める。
//! - アポを付ける人は今と同じ決まり (`counts::credit_for_stage`)。値は入った時点のもの
//!   (画面で数えたときはその時の値、履歴から作り直すときは入った時刻 + `CREDIT_GRACE_MS` までの最後の値)。
//! - 9 月 1 日 (`START`) 以降を、起動時に HubSpot のステージの履歴から作り直す (一度入れた行は書き換えない)。
//!   以後は画面の数え (`counts`) が見つけたアポと、1 日 1 回の直近 14 日の履歴の読み直しで足す。
//! - 置き場所は監査 Turso の `talk_apo_events`。後で HubSpot に置き場所を作って移す (ユーザー決定)。

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};

use super::counts::{credit_for_stage, entered_ms, ApoDeal};
use crate::crm::caller_metrics::{flt, jst_day, PAGE};
use crate::crm::caller_metrics_month::jst_midnight_ms;
use crate::db::turso_http::TursoDb;
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};
use crate::hubspot::{HubSpotClient, HubSpotError};

/// 控え始める日 (JST)。これより前は数えない
pub const START: (i32, u32, u32) = (2026, 9, 1);
/// 2 回目以降の履歴の読み直しで遡る日数
pub const RESCAN_DAYS: i64 = 14;
/// 履歴から付ける人を決めるとき、入った時刻からこの間に入った値まで使う (移すときの必須欄・直後の入力)
pub const CREDIT_GRACE_MS: i64 = 60 * 60_000;
/// 1 回の読み直しの Search の上限
const MAX_SEARCHES: u32 = 200;
/// 1 文で入れる行数
const INSERT_CHUNK: usize = 100;
/// 起動時に読む 1 回の行数
const LOAD_PAGE: i64 = 1000;

pub fn start_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(START.0, START.1, START.2).expect("start day")
}

/// 控えたアポ 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApoEvent {
    pub day: NaiveDate,
    pub deal: ApoDeal,
}

fn jst_of(ms: i64) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|t| jst_day(t).0)
}

/// 履歴の 1 つ (`value` と `timestamp`)
fn hist(v: &Value, prop: &str) -> Vec<(i64, Option<String>)> {
    let mut out: Vec<(i64, Option<String>)> = v
        .pointer(&format!("/propertiesWithHistory/{prop}"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let ts = e.get("timestamp").and_then(entered_ms)?;
                    let val = e
                        .get("value")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string);
                    Some((ts, val))
                })
                .collect()
        })
        .unwrap_or_default();
    // HubSpot は新しい順で返す。同じ時刻の中の順を保つため、逆にしてから並べる
    out.reverse();
    out.sort_by_key(|(t, _)| *t);
    out
}

/// `at_ms` までに入った最後の値
fn value_at(h: &[(i64, Option<String>)], at_ms: i64) -> Option<String> {
    h.iter()
        .take_while(|(t, _)| *t <= at_ms)
        .last()
        .and_then(|(_, v)| v.clone())
}

/// 取引 1 件の履歴から、`[from_ms, to_ms)` にアポ日確定へ入ったアポを作る (純粋な関数)。
/// 同じ値が続く履歴はまとめ、別のステージから入った時だけを入ったとみなす。同じ JST の日は 1 件で、
/// 時刻・ステージ・付ける人はその日の最後に入ったもの (取引の `hs_v2_date_entered_*` やランクの記録と同じ時刻)
pub fn events_from_history(deal: &Value, from_ms: i64, to_ms: i64) -> Vec<ApoEvent> {
    events_and_dropped(deal, from_ms, to_ms).0
}

/// `events_from_history` と、付ける人が分からず落としたアポの数
pub fn events_and_dropped(deal: &Value, from_ms: i64, to_ms: i64) -> (Vec<ApoEvent>, usize) {
    let id = match deal.get("id") {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => return (Vec::new(), 0),
    };
    let stages = hist(deal, "dealstage");
    let props = [
        "aposyutokusya",
        "aposyutokubi",
        "bpo_26",
        "bpo_appo_date",
        "hubspot_owner_id",
    ];
    let hs: HashMap<&str, Vec<(i64, Option<String>)>> =
        props.iter().map(|p| (*p, hist(deal, p))).collect();
    let mut out: Vec<ApoEvent> = Vec::new();
    let mut dropped: HashSet<NaiveDate> = HashSet::new();
    let mut prev: Option<&str> = None;
    for (ts, val) in &stages {
        let v = val.as_deref();
        let entered = v != prev;
        prev = v;
        let Some(stage) = v.filter(|s| *s == ST_APO || *s == ST_APO_BPO) else {
            continue;
        };
        if !entered || *ts < from_ms || *ts >= to_ms {
            continue;
        }
        let Some(day) = jst_of(*ts) else {
            continue;
        };
        let at = ts + CREDIT_GRACE_MS;
        let p = json!({
            "aposyutokusya": value_at(&hs["aposyutokusya"], at),
            "aposyutokubi": value_at(&hs["aposyutokubi"], at),
            "bpo_26": value_at(&hs["bpo_26"], at),
            "bpo_appo_date": value_at(&hs["bpo_appo_date"], at),
        });
        let owner = value_at(&hs["hubspot_owner_id"], at);
        let who = credit_for_stage(stage, &p, day, owner.as_deref());
        // 同じ日にもう入っていれば、その日の最後に入ったもので置き換える
        out.retain(|e| e.day != day);
        if who.is_empty() {
            dropped.insert(day);
            continue;
        }
        dropped.remove(&day);
        out.push(ApoEvent {
            day,
            deal: ApoDeal {
                deal_id: id.clone(),
                stage: stage.to_string(),
                entered_ms: *ts,
                who,
            },
        });
    }
    (out, dropped.len())
}

/// `from` (JST の日) 以降にアポ日確定へ入ったことのある取引を、ステージの履歴から読んでアポにする。
/// 最後に入った時刻が `from` 以降の取引を探すので、`from` 以降に入ったことのある取引は全部入る。
pub async fn scan(
    client: &HubSpotClient,
    from: NaiveDate,
    now: DateTime<Utc>,
) -> Result<Vec<ApoEvent>, HubSpotError> {
    let bg = client.background();
    let from_ms = jst_midnight_ms(from);
    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut searches = 0u32;
    for stage in [ST_APO, ST_APO_BPO] {
        let prop = format!("hs_v2_date_entered_{stage}");
        let mut after: Option<String> = None;
        loop {
            searches += 1;
            if searches > MAX_SEARCHES {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let mut body = json!({
                "filterGroups": [{"filters": [flt(&prop, "GTE", json!(from_ms.to_string()))]}],
                "properties": ["hs_object_id"],
                "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
                "limit": PAGE,
            });
            if let Some(a) = &after {
                body["after"] = json!(a);
            }
            let v = bg.search("deals", body).await?;
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("deals without results".into()))?;
            for r in results {
                let id = match r.get("id") {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => v.to_string(),
                    None => continue,
                };
                if seen.insert(id.clone()) {
                    ids.push(id);
                }
            }
            match v
                .pointer("/paging/next/after")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(n) => after = Some(n.to_string()),
                None => break,
            }
        }
    }
    let history = [
        "dealstage",
        "aposyutokusya",
        "aposyutokubi",
        "bpo_26",
        "bpo_appo_date",
        "hubspot_owner_id",
    ];
    let recs = bg
        .batch_read_with_history("deals", &ids, &["dealstage"], &history)
        .await?;
    let to_ms = now.timestamp_millis() + 1;
    let (mut out, mut dropped) = (Vec::new(), 0usize);
    for r in &recs {
        let (e, d) = events_and_dropped(r, from_ms, to_ms);
        out.extend(e);
        dropped += d;
    }
    if dropped > 0 {
        tracing::warn!(dropped, "talk review: apo events without anyone to credit");
    }
    Ok(out)
}

// ---------------------------------------------------------------- Turso

/// 作り直しが終わった印の行 (取引 ID が空)
const DONE_DAY: &str = "0000-00-00";

/// 9/1 から作り直し終えた印を入れる (全部の行を書けた後に 1 回)
pub fn mark_rebuilt(t: &TursoDb, now: &str) -> Result<(), String> {
    t.execute(
        "INSERT OR IGNORE INTO talk_apo_events \
         (deal_id, entered_day, stage, entered_ms, owner_id, source, created_at) \
         VALUES ('', ?1, '', 0, '', 'rebuild_done', ?2)",
        &[&DONE_DAY, &now],
    )
}

/// 控えたアポを全部読む (日・取引の順に `LOAD_PAGE` 行ずつ)。2 つめは作り直しを終えた印があるか
pub fn load(t: &TursoDb) -> Result<(Vec<ApoEvent>, bool), String> {
    let rebuilt = !t
        .query(
            "SELECT 1 AS x FROM talk_apo_events WHERE deal_id = '' AND entered_day = ?1",
            &[&DONE_DAY],
        )?
        .is_empty();
    let mut out = Vec::new();
    // 印の行 (0000-00-00) より後から読む
    let (mut day, mut deal) = (DONE_DAY.to_string(), String::new());
    loop {
        let rows = t.query(
            "SELECT deal_id, entered_day, stage, entered_ms, owner_id FROM talk_apo_events \
             WHERE (entered_day, deal_id) > (?1, ?2) \
             ORDER BY entered_day, deal_id LIMIT ?3",
            &[&day, &deal, &LOAD_PAGE],
        )?;
        let n = rows.len();
        for r in &rows {
            let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let (id, d) = (s("deal_id"), s("entered_day"));
            day = d.clone();
            deal = id.clone();
            let (Ok(dd), Some(ms)) = (
                NaiveDate::parse_from_str(&d, "%Y-%m-%d"),
                r.get("entered_ms").and_then(Value::as_i64),
            ) else {
                continue;
            };
            let owner = s("owner_id");
            if owner.is_empty() {
                continue;
            }
            out.push(ApoEvent {
                day: dd,
                deal: ApoDeal {
                    deal_id: id,
                    stage: s("stage"),
                    entered_ms: ms,
                    who: vec![owner],
                },
            });
        }
        if (n as i64) < LOAD_PAGE {
            return Ok((out, rebuilt));
        }
    }
}

/// 新しいアポを入れる (`INSERT OR IGNORE`。入れた行は書き換えない)。`INSERT_CHUNK` 行ずつ 1 文
pub fn insert(t: &TursoDb, events: &[ApoEvent], now: &str, source: &str) -> Result<(), String> {
    for chunk in events.chunks(INSERT_CHUNK) {
        let mut sql = String::from(
            "INSERT OR IGNORE INTO talk_apo_events \
             (deal_id, entered_day, stage, entered_ms, owner_id, source, created_at) VALUES ",
        );
        let mut vals: Vec<(String, String, String, i64, String)> = Vec::new();
        for (i, e) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            let b = i * 7;
            sql.push_str(&format!(
                "(?{},?{},?{},?{},?{},?{},?{})",
                b + 1,
                b + 2,
                b + 3,
                b + 4,
                b + 5,
                b + 6,
                b + 7
            ));
            vals.push((
                e.deal.deal_id.clone(),
                e.day.format("%Y-%m-%d").to_string(),
                e.deal.stage.clone(),
                e.deal.entered_ms,
                e.deal.who.first().cloned().unwrap_or_default(),
            ));
        }
        let (src, at) = (source.to_string(), now.to_string());
        let mut args: Vec<&dyn crate::db::turso_http::ToSqlTurso> = Vec::new();
        for v in &vals {
            args.extend([
                &v.0 as &dyn crate::db::turso_http::ToSqlTurso,
                &v.1,
                &v.2,
                &v.3,
                &v.4,
                &src,
                &at,
            ]);
        }
        t.execute(&sql, &args)?;
    }
    Ok(())
}

/// 日ごとにまとめる (同じ取引・同じ日は最初の 1 件)
pub fn by_day(events: Vec<ApoEvent>) -> BTreeMap<NaiveDate, Vec<ApoDeal>> {
    let mut out: BTreeMap<NaiveDate, Vec<ApoDeal>> = BTreeMap::new();
    for e in events {
        let v = out.entry(e.day).or_default();
        if !v.iter().any(|d| d.deal_id == e.deal.deal_id) {
            v.push(e.deal);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn h(items: &[(&str, &str)]) -> Value {
        // HubSpot と同じく新しい順で返す
        let mut v: Vec<Value> = items
            .iter()
            .map(|(t, val)| json!({"timestamp": t, "value": val}))
            .collect();
        v.reverse();
        Value::Array(v)
    }

    #[test]
    fn re_entries_count_on_each_day_and_same_day_once() {
        let deal = json!({"id": "D1", "propertiesWithHistory": {
            "dealstage": h(&[
                ("2026-09-03T01:00:00Z", "51997752"),
                ("2026-09-03T02:00:00Z", ST_APO),
                ("2026-09-10T02:00:00Z", "52035888"),
                ("2026-09-10T03:00:00Z", ST_APO),   // 同じ日でない → 2 件目
                ("2026-09-10T04:00:00Z", "51997752"),
                ("2026-09-10T05:00:00Z", ST_APO),   // 同じ日 → 数えない
                ("2026-10-01T05:00:00Z", ST_APO),   // 同じ値が続くだけ → 入ったとみなさない
            ]),
            "hubspot_owner_id": h(&[("2026-08-01T00:00:00Z", "S1")]),
        }});
        let ev = events_from_history(
            &deal,
            ms("2026-08-31T15:00:00Z"),
            ms("2026-12-01T00:00:00Z"),
        );
        let days: Vec<String> = ev.iter().map(|e| e.day.to_string()).collect();
        assert_eq!(days, vec!["2026-09-03", "2026-09-10"]);
        assert!(ev.iter().all(|e| e.deal.who == vec!["S1".to_string()]));
    }

    #[test]
    fn credit_uses_the_value_at_the_time_of_entry() {
        // 9/3 に取得者 A、10/20 に B へ書き換え。9/3 のアポは A、10/20 のアポは B
        let deal = json!({"id": "D2", "propertiesWithHistory": {
            "dealstage": h(&[
                ("2026-09-03T02:00:00Z", ST_APO),
                ("2026-10-01T02:00:00Z", "52035888"),
                ("2026-10-20T02:00:00Z", ST_APO),
            ]),
            "aposyutokusya": h(&[("2026-09-03T02:00:01Z", "A"), ("2026-10-20T02:00:01Z", "B")]),
            "aposyutokubi": h(&[("2026-09-03T02:00:01Z", "2026-09-03"), ("2026-10-20T02:00:01Z", "2026-10-20")]),
            "hubspot_owner_id": h(&[("2026-08-01T00:00:00Z", "S1")]),
        }});
        let ev = events_from_history(
            &deal,
            ms("2026-08-31T15:00:00Z"),
            ms("2026-12-01T00:00:00Z"),
        );
        let who: Vec<String> = ev.iter().map(|e| e.deal.who[0].clone()).collect();
        assert_eq!(who, vec!["A", "B"]);
    }

    #[test]
    fn bpo_stage_goes_to_the_bpo_taker_and_before_start_is_skipped() {
        let deal = json!({"id": "D3", "propertiesWithHistory": {
            "dealstage": h(&[
                ("2026-08-20T02:00:00Z", ST_APO_BPO),   // 9/1 より前 → 数えない
                ("2026-08-25T02:00:00Z", "x"),
                ("2026-09-05T02:00:00Z", ST_APO_BPO),
            ]),
            "bpo_26": h(&[("2026-08-20T02:00:00Z", "B1")]),
            "bpo_appo_date": h(&[("2026-09-05T02:00:00Z", "2026-09-05")]),
            "hubspot_owner_id": h(&[("2026-08-01T00:00:00Z", "S1")]),
        }});
        let ev = events_from_history(
            &deal,
            ms("2026-08-31T15:00:00Z"),
            ms("2026-12-01T00:00:00Z"),
        );
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].day.to_string(), "2026-09-05");
        assert_eq!(ev[0].deal.who, vec!["B1".to_string()]);
        assert_eq!(ev[0].deal.stage, ST_APO_BPO);
    }

    #[test]
    fn day_is_jst() {
        // UTC 9/30 16:00 = JST 10/1
        let deal = json!({"id": "D4", "propertiesWithHistory": {
            "dealstage": h(&[("2026-09-30T16:00:00Z", ST_APO)]),
            "hubspot_owner_id": h(&[("2026-08-01T00:00:00Z", "S1")]),
        }});
        let ev = events_from_history(
            &deal,
            ms("2026-08-31T15:00:00Z"),
            ms("2026-12-01T00:00:00Z"),
        );
        assert_eq!(ev[0].day.to_string(), "2026-10-01");
    }

    #[test]
    fn by_day_keeps_one_per_deal_and_day() {
        let e = |id: &str, d: &str, st: &str| ApoEvent {
            day: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            deal: ApoDeal {
                deal_id: id.into(),
                stage: st.into(),
                entered_ms: 1,
                who: vec!["S".into()],
            },
        };
        let m = by_day(vec![
            e("1", "2026-09-01", ST_APO_BPO),
            e("1", "2026-09-01", ST_APO),
            e("1", "2026-09-02", ST_APO),
        ]);
        assert_eq!(m.values().map(Vec::len).sum::<usize>(), 2);
        assert_eq!(
            m[&NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()][0].stage,
            ST_APO_BPO
        );
    }
}

/// 本物の HubSpot で 9/1 以降を作り直し、月ごとの件数だけを出す (読み取りのみ。手で動かす)。
/// `HUBSPOT_ACCESS_TOKEN=... cargo test --lib apo_events::live -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn rebuild_counts_by_month() {
        let token = std::env::var("HUBSPOT_ACCESS_TOKEN").expect("token");
        let client = HubSpotClient::for_production(token, Default::default()).unwrap();
        let ev = scan(&client, start_day(), Utc::now()).await.unwrap();
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for e in &ev {
            *m.entry(e.day.format("%Y-%m").to_string()).or_default() += 1;
        }
        let deals: HashSet<&str> = ev.iter().map(|e| e.deal.deal_id.as_str()).collect();
        println!("events={} deals={} by_month={:?}", ev.len(), deals.len(), m);
    }
}
