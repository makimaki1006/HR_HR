//! 応募を電話番号で探す (headless-crm-design §21「電話番号での検索」)。
//!
//! - 番号を全部入れたとき: `+81…` の形に直し、`denwabangou` の一致で HubSpot を引く (この入れ物は使わない)。
//! - 数字 4 桁だけのとき: 直近 60 日に作られた応募の (id, 電話番号の下 4 桁) をプロセス内に持ち、そこから id を引く。
//!   HubSpot の検索は末尾一致ができないため。
//!
//! 持ち方は `caller_metrics` と同じ。初回は 60 日分を作成日で全部読み (200 件ずつ、多ければ時間で割る)、
//! 以降は前回の読み始めの 2 分前以降に更新された分だけを読んで id で上書きする。読み直しは 2 分に 1 回まで、
//! `Background` 優先度。失敗したら前回の結果を使い、30 秒は再試行しない。何も永続化しない (再起動で読み直す)。
//! **単一インスタンス前提**。
use crate::hubspot::{HubSpotClient, HubSpotError};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

const OBJECT: &str = "0-421";
const PHONE: &str = "denwabangou";
/// 下 4 桁で探せる範囲 (作成日)
pub(super) const WINDOW_DAYS: i64 = 60;
const DAY_MS: i64 = 86_400_000;
const REFRESH_MS: i64 = 120_000;
const RETRY_AFTER_FAILURE_MS: i64 = 30_000;
const OVERLAP_MS: i64 = 120_000;
const PAGE: usize = 200;
/// Search は 1 回の条件で 10,000 件まで。超えそうなら時間で半分に割る
const SPLIT_AT: u64 = 9_000;
const MAX_SEARCHES: u32 = 300;
/// 一覧の検索に渡す id の上限 (検索本文 3000 文字に収める)。新しい応募を優先する
const MAX_IDS: usize = 50;
/// 読み直しを待つ上限。超えたら前回の結果で答える (読み直しは裏で続く)
const WAIT: Duration = Duration::from_secs(6);

pub(super) enum PhoneQuery {
    /// 番号を全部入れた。`denwabangou` がこのどれかと一致するものを引く
    Full(Vec<String>),
    /// 下 4 桁
    Last4(String),
}

/// 入力が電話番号なら読み取る。数字・`+`・区切り (ハイフン・空白・かっこ) 以外が混ざっていれば None
pub(super) fn parse(input: &str) -> Option<PhoneQuery> {
    let mut plus = false;
    let mut digits = String::new();
    for (i, c) in input.trim().chars().enumerate() {
        match c {
            '0'..='9' => digits.push(c),
            '０'..='９' => digits.push(char::from(b'0' + (c as u32 - '０' as u32) as u8)),
            '+' | '＋' if i == 0 => plus = true,
            '-' | '‐' | '‑' | '–' | '—' | '−' | '－' | 'ー' | '(' | ')' | '（' | '）' | '.' =>
                {}
            c if c.is_whitespace() => {}
            _ => return None,
        }
    }
    let n = digits.len();
    if plus {
        if !(8..=15).contains(&n) {
            return None;
        }
        let mut v = vec![format!("+{digits}")];
        if let Some(rest) = digits.strip_prefix("81") {
            v.push(format!("0{rest}"));
        }
        return Some(PhoneQuery::Full(v));
    }
    if n == 4 {
        return Some(PhoneQuery::Last4(digits));
    }
    if let Some(rest) = digits.strip_prefix('0').filter(|_| n == 10 || n == 11) {
        return Some(PhoneQuery::Full(vec![format!("+81{rest}"), digits.clone()]));
    }
    if let Some(rest) = digits.strip_prefix("81").filter(|_| n == 11 || n == 12) {
        return Some(PhoneQuery::Full(vec![
            format!("+{digits}"),
            format!("0{rest}"),
        ]));
    }
    None
}

#[derive(Default)]
struct Store {
    /// id → (下 4 桁, 作成日 ms)
    rows: HashMap<String, (String, i64)>,
    last_fetch_start: Option<i64>,
}

struct Snapshot {
    as_of: i64,
    /// 下 4 桁 → (作成日 ms, id)。新しい順
    by_last4: HashMap<String, Vec<(i64, String)>>,
}

/// まだ最初の読み込みが終わっていない
pub(super) struct Loading;

#[derive(Default)]
pub(super) struct PhoneIndex {
    store: tokio::sync::Mutex<Store>,
    snapshot: RwLock<Option<Arc<Snapshot>>>,
    next_try: Mutex<Option<i64>>,
    /// 裏の読み込みが走っている間 true (二重に走らせない)
    running: AtomicBool,
}

struct RunGuard(Arc<PhoneIndex>);
impl Drop for RunGuard {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl PhoneIndex {
    fn current(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.read().ok().and_then(|g| g.clone())
    }

    fn backing_off(&self, now: i64) -> bool {
        self.next_try
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|t| now < t)
    }

    /// 古ければ裏で読み直しを 1 本だけ始める。始めた (か、すでに走っている) なら true
    fn spawn_refresh(
        self: &Arc<Self>,
        client: &Arc<HubSpotClient>,
    ) -> Option<tokio::task::JoinHandle<()>> {
        let now = now_ms();
        if self.current().is_some_and(|s| now - s.as_of < REFRESH_MS) || self.backing_off(now) {
            return None;
        }
        if self
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        let this = self.clone();
        let client = client.clone();
        Some(tokio::spawn(async move {
            let _guard = RunGuard(this.clone());
            let mut store = this.store.lock().await;
            let now = now_ms();
            match refresh(&client.background(), &mut store, now).await {
                Ok(snap) => {
                    if let Ok(mut g) = this.snapshot.write() {
                        *g = Some(Arc::new(snap));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error_kind = e.error_kind(),
                        "crm applicant phone index refresh failed"
                    );
                    if let Ok(mut g) = this.next_try.lock() {
                        *g = Some(now + RETRY_AFTER_FAILURE_MS);
                    }
                }
            }
        }))
    }

    /// 応募の一覧が開かれたときに呼ぶ。下 4 桁で探す前に読み込みを済ませておく
    #[cfg_attr(test, allow(dead_code))]
    pub(super) fn warm(self: &Arc<Self>, client: &Arc<HubSpotClient>) {
        if self.current().is_none() {
            let _ = self.spawn_refresh(client);
        }
    }

    /// 下 4 桁が一致する応募の id (新しい順、`MAX_IDS` 件まで)。直近 `WINDOW_DAYS` 日に作られた応募だけ
    pub(super) async fn lookup(
        self: &Arc<Self>,
        client: &Arc<HubSpotClient>,
        last4: &str,
    ) -> Result<Vec<String>, Loading> {
        let had = self.current().is_some();
        if let Some(task) = self.spawn_refresh(client) {
            if had {
                // 増分は通常 1 回の Search。終わらなければ前回の結果で答える
                let _ = tokio::time::timeout(WAIT, task).await;
            }
        }
        let snap = self.current().ok_or(Loading)?;
        let cutoff = now_ms() - WINDOW_DAYS * DAY_MS;
        Ok(snap
            .by_last4
            .get(last4)
            .into_iter()
            .flatten()
            .filter(|(created, _)| *created >= cutoff)
            .take(MAX_IDS)
            .map(|(_, id)| id.clone())
            .collect())
    }
}

fn body(prop: &str, from: i64, to: i64, cutoff: i64, after: Option<&str>) -> Value {
    let flt = super::super::caller_metrics::flt;
    let mut filters = vec![
        flt(prop, "GTE", json!(from.to_string())),
        flt(prop, "LT", json!(to.to_string())),
    ];
    if prop != "hs_createdate" {
        filters.push(flt("hs_createdate", "GTE", json!(cutoff.to_string())));
    }
    let mut body = json!({
        "filterGroups": [{"filters": filters}],
        "properties": [PHONE, "hs_createdate"],
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": PAGE,
    });
    if let Some(a) = after {
        body["after"] = json!(a);
    }
    body
}

/// (id, 下 4 桁 (番号が無い・4 桁に満たなければ None), 作成日 ms)
fn parse_row(v: &Value) -> Option<(String, Option<String>, i64)> {
    let id = match v.get("id")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let p = v.get("properties")?;
    let created = super::super::caller_metrics::parse_time_ms(p.get("hs_createdate")?)?;
    let digits: Vec<char> = p
        .get(PHONE)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    let last4 = (digits.len() >= 4).then(|| digits[digits.len() - 4..].iter().collect());
    Some((id, last4, created))
}

async fn fetch(
    client: &HubSpotClient,
    prop: &str,
    from: i64,
    to: i64,
    cutoff: i64,
    searches: &mut u32,
) -> Result<Vec<(String, Option<String>, i64)>, HubSpotError> {
    let mut out = Vec::new();
    let mut stack = vec![(from, to)];
    while let Some((a, b)) = stack.pop() {
        let mut after: Option<String> = None;
        let mut first = true;
        loop {
            *searches += 1;
            if *searches > MAX_SEARCHES {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let v = client
                .search(OBJECT, body(prop, a, b, cutoff, after.as_deref()))
                .await?;
            if first {
                first = false;
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
                if total > SPLIT_AT && b - a > 1_000 {
                    let mid = a + (b - a) / 2;
                    stack.push((mid, b));
                    stack.push((a, mid));
                    break;
                }
            }
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("applicants without results".into()))?;
            out.extend(results.iter().filter_map(parse_row));
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
    Ok(out)
}

/// 1 回の読み直し。失敗したら Store を進めない
async fn refresh(
    client: &HubSpotClient,
    store: &mut Store,
    now: i64,
) -> Result<Snapshot, HubSpotError> {
    let _task = crate::mem_watch::task("applicant_phone.refresh");
    let cutoff = now - WINDOW_DAYS * DAY_MS;
    let to = now + 3_600_000;
    let mut searches = 0u32;
    let rows = match store.last_fetch_start {
        None => fetch(client, "hs_createdate", cutoff, to, cutoff, &mut searches).await?,
        Some(prev) => {
            let from = (prev - OVERLAP_MS).max(cutoff);
            fetch(
                client,
                "hs_lastmodifieddate",
                from,
                to,
                cutoff,
                &mut searches,
            )
            .await?
        }
    };
    for (id, last4, created) in rows {
        match last4 {
            Some(d) => {
                store.rows.insert(id, (d, created));
            }
            None => {
                store.rows.remove(&id);
            }
        }
    }
    store.rows.retain(|_, (_, created)| *created >= cutoff);
    store.last_fetch_start = Some(now);
    tracing::info!(
        searches,
        applicants = store.rows.len(),
        "crm applicant phone index refreshed"
    );
    let mut by_last4: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    for (id, (d, created)) in &store.rows {
        by_last4
            .entry(d.clone())
            .or_default()
            .push((*created, id.clone()));
    }
    for v in by_last4.values_mut() {
        v.sort_by(|a, b| b.cmp(a));
    }
    Ok(Snapshot {
        as_of: now,
        by_last4,
    })
}
