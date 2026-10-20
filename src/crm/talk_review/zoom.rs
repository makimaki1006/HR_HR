//! Zoom Phone の録音・文字起こし。Server-to-Server OAuth (`grant_type=account_credentials`) で読み取るだけ。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use super::transcript::turns_from;
use super::types::TalkTurn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoomError {
    /// 録音・文字起こしがまだできていない (後で再試行)
    NotReady,
    /// 録音が残っていない (古くて消えた)
    Gone,
    /// 通信・混雑・認証の一時的な失敗 (後で再試行)
    Transient(String),
    /// Zoom の設定 (環境変数) が無い
    NotConfigured,
}

/// HTTP の状態から分類する (純粋。テスト用に分離)
pub fn classify_status(status: u16) -> ZoomError {
    match status {
        400 => ZoomError::NotReady,
        404 | 410 => ZoomError::Gone,
        s => ZoomError::Transient(format!("http {s}")),
    }
}

/// 文字起こしの取得元 (テストで偽物に差し替える)
pub trait TranscriptSource: Send + Sync {
    fn fetch(
        &self,
        call_log_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<TalkTurn>, ZoomError>> + Send;

    /// HubSpot の Call に録音 URL が無いとき、通話履歴から録音 ID を探す (Call Queue 経由の発信)。
    /// 相手番号と開始時刻 (ミリ秒) で照合する。見つからなければ `None`
    fn find_recording(
        &self,
        to_number: &str,
        at_ms: i64,
    ) -> impl std::future::Future<Output = Result<Option<String>, ZoomError>> + Send;
}

/// 行の `zoom_call_log_id` に録音 ID を直接入れるときの印 (通話履歴から見つけたもの)
pub const REC_PREFIX: &str = "rec:";
/// 通話履歴の一覧のページ数の上限 (1 ページ 300 件。1 日約 6,000 件)
const MAX_HISTORY_PAGES: usize = 40;
const HISTORY_TTL: Duration = Duration::from_secs(600);
const HISTORY_MAX_DAYS: usize = 16;
/// HubSpot の通話の時刻と Zoom の開始時刻を同じ電話とみなす差
pub const MATCH_WINDOW_MS: i64 = 60_000;

/// 電話番号を数字だけにして下 9 桁にする (+81 の有無・ハイフンの違いを吸収)。9 桁に満たなければ `None`
pub fn normalize_number(s: &str) -> Option<String> {
    let d: Vec<char> = s.chars().filter(char::is_ascii_digit).collect();
    (d.len() >= 9).then(|| d[d.len() - 9..].iter().collect())
}

/// 通話履歴の 1 件 (発信の相手番号と開始時刻、詳細を引く ID の候補)
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub ids: Vec<String>,
    pub number9: String,
    pub start_ms: i64,
}

/// `call_history` の応答の `call_logs[]` から取り出す。着信・番号なし・時刻なしは除く
pub fn history_entries(v: &Value) -> Vec<HistoryEntry> {
    let text = |e: &Value, k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
    v.get("call_logs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    if text(e, "direction").is_some_and(|d| d.eq_ignore_ascii_case("inbound")) {
                        return None;
                    }
                    let number9 = normalize_number(&text(e, "callee_number")?)?;
                    let start_ms = DateTime::parse_from_rfc3339(&text(e, "start_time")?)
                        .ok()?
                        .timestamp_millis();
                    let mut ids: Vec<String> = Vec::new();
                    for k in ["id", "call_path_id"] {
                        if let Some(i) = text(e, k).filter(|i| !i.is_empty() && !ids.contains(i)) {
                            ids.push(i);
                        }
                    }
                    (!ids.is_empty()).then_some(HistoryEntry {
                        ids,
                        number9,
                        start_ms,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 相手番号が同じで、開始時刻の差が `MATCH_WINDOW_MS` 以内のうち最も近いもの
pub fn pick_match<'a>(
    entries: &'a [HistoryEntry],
    number9: &str,
    at_ms: i64,
) -> Option<&'a HistoryEntry> {
    entries
        .iter()
        .filter(|e| e.number9 == number9 && (e.start_ms - at_ms).abs() <= MATCH_WINDOW_MS)
        .min_by_key(|e| (e.start_ms - at_ms).abs())
}

/// 通話履歴の詳細 (`call_elements[]`) から録音 ID を取る。Call Queue の要素を先に見る
pub fn recording_id_from(detail: &Value) -> Option<String> {
    let els = detail.get("call_elements").and_then(Value::as_array)?;
    let rec = |e: &Value| {
        e.get("recording_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    els.iter()
        .filter(|e| e.get("caller_ext_type").and_then(Value::as_str) == Some("call_queue"))
        .find_map(rec)
        .or_else(|| els.iter().find_map(rec))
}

struct Token {
    value: String,
    until: Instant,
}

type HistoryCache = HashMap<NaiveDate, (Instant, Arc<Vec<HistoryEntry>>)>;

pub struct ZoomClient {
    account_id: String,
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    token: tokio::sync::Mutex<Option<Token>>,
    /// JST の日ごとの通話履歴 (Call ごとに取り直さない)
    history: tokio::sync::Mutex<HistoryCache>,
}

impl ZoomClient {
    pub fn from_env() -> Option<Arc<Self>> {
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Some(Arc::new(Self {
            account_id: get("ZOOM_ACCOUNT_ID")?,
            client_id: get("ZOOM_CLIENT_ID")?,
            client_secret: get("ZOOM_CLIENT_SECRET")?,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .ok()?,
            token: tokio::sync::Mutex::new(None),
            history: tokio::sync::Mutex::new(HashMap::new()),
        }))
    }

    async fn bearer(&self) -> Result<String, ZoomError> {
        let mut g = self.token.lock().await;
        if let Some(t) = g.as_ref() {
            if Instant::now() < t.until {
                return Ok(t.value.clone());
            }
        }
        let r = self
            .http
            .post("https://zoom.us/oauth/token")
            .query(&[
                ("grant_type", "account_credentials"),
                ("account_id", self.account_id.as_str()),
            ])
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .send()
            .await
            .map_err(|e| ZoomError::Transient(format!("token: {}", e.without_url())))?;
        if !r.status().is_success() {
            return Err(ZoomError::Transient(format!(
                "token http {}",
                r.status().as_u16()
            )));
        }
        let v: Value = r
            .json()
            .await
            .map_err(|_| ZoomError::Transient("token body".into()))?;
        let value = v
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| ZoomError::Transient("token missing".into()))?
            .to_string();
        let ttl = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
        *g = Some(Token {
            value: value.clone(),
            until: Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
        });
        Ok(value)
    }

    async fn get_json(&self, url: &str) -> Result<Value, ZoomError> {
        let tok = self.bearer().await?;
        let r = self
            .http
            .get(url)
            .bearer_auth(&tok)
            .send()
            .await
            .map_err(|e| ZoomError::Transient(format!("get: {}", e.without_url())))?;
        let st = r.status();
        if st.as_u16() == 401 {
            // トークンが失効していた。捨てて次の試行でやり直す
            *self.token.lock().await = None;
        }
        if !st.is_success() {
            return Err(classify_status(st.as_u16()));
        }
        r.json()
            .await
            .map_err(|_| ZoomError::Transient("body".into()))
    }
}

impl ZoomClient {
    /// その日 (JST) の通話履歴。ページ送りには上限がある
    async fn history_for(&self, day: NaiveDate) -> Result<Arc<Vec<HistoryEntry>>, ZoomError> {
        let mut g = self.history.lock().await;
        if let Some((at, v)) = g.get(&day) {
            if at.elapsed() < HISTORY_TTL {
                return Ok(v.clone());
            }
        }
        let d = day.to_string();
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_HISTORY_PAGES {
            let mut q = vec![
                ("from", d.as_str()),
                ("to", d.as_str()),
                ("page_size", "300"),
            ];
            if let Some(t) = token.as_deref() {
                q.push(("next_page_token", t));
            }
            let url =
                reqwest::Url::parse_with_params("https://api.zoom.us/v2/phone/call_history", &q)
                    .map_err(|_| ZoomError::Transient("url".into()))?;
            let v = self.get_json(url.as_str()).await?;
            out.extend(history_entries(&v));
            match v
                .get("next_page_token")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(t) => token = Some(t.to_string()),
                None => break,
            }
        }
        let out = Arc::new(out);
        if g.len() >= HISTORY_MAX_DAYS {
            g.clear();
        }
        g.insert(day, (Instant::now(), out.clone()));
        Ok(out)
    }
}

impl TranscriptSource for ZoomClient {
    async fn find_recording(
        &self,
        to_number: &str,
        at_ms: i64,
    ) -> Result<Option<String>, ZoomError> {
        let (Some(n9), Some(at)) = (
            normalize_number(to_number),
            DateTime::<Utc>::from_timestamp_millis(at_ms),
        ) else {
            return Ok(None);
        };
        let list = self
            .history_for(crate::crm::caller_metrics::jst_day(at).0)
            .await?;
        let Some(entry) = pick_match(&list, &n9, at_ms) else {
            return Ok(None);
        };
        for id in &entry.ids {
            match self
                .get_json(&format!("https://api.zoom.us/v2/phone/call_history/{id}"))
                .await
            {
                Ok(v) => return Ok(recording_id_from(&v)),
                Err(ZoomError::Gone) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }

    async fn fetch(&self, call_log_id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
        // 通話履歴から見つけた録音 ID は、そのまま文字起こしを取る
        let id = if let Some(rec) = call_log_id.strip_prefix(REC_PREFIX) {
            rec.to_string()
        } else {
            let rec = self
                .get_json(&format!(
                    "https://api.zoom.us/v2/phone/call_logs/{call_log_id}/recordings"
                ))
                .await?;
            // 応答は録音 1 件のオブジェクト (試作は rec["id"])。配列で返る場合は先頭
            let rec = rec.as_array().and_then(|a| a.first()).unwrap_or(&rec);
            rec.get("id")
                .and_then(Value::as_str)
                .ok_or(ZoomError::NotReady)?
                .to_string()
        };
        let tx = self
            .get_json(&format!(
                "https://api.zoom.us/v2/phone/recording_transcript/download/{id}"
            ))
            .await?;
        let turns = turns_from(&tx);
        // 200 でも発言が 1 件も無ければ、文字起こしがまだ空 (後で取り直す)
        if turns.is_empty() {
            return Err(ZoomError::NotReady);
        }
        Ok(turns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(400), ZoomError::NotReady);
        assert_eq!(classify_status(404), ZoomError::Gone);
        assert_eq!(
            classify_status(429),
            ZoomError::Transient("http 429".into())
        );
        assert_eq!(
            classify_status(503),
            ZoomError::Transient("http 503".into())
        );
    }

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }

    #[test]
    fn numbers_match_on_the_last_nine_digits() {
        assert_eq!(
            normalize_number("+81 3-1234-5678").as_deref(),
            Some("312345678")
        );
        assert_eq!(
            normalize_number("03-1234-5678").as_deref(),
            Some("312345678")
        );
        assert_eq!(
            normalize_number("+819012345678").as_deref(),
            Some("012345678")
        );
        assert_eq!(
            normalize_number("090-1234-5678").as_deref(),
            Some("012345678")
        );
        assert_eq!(normalize_number("1234"), None);
        assert_eq!(normalize_number(""), None);
    }

    #[test]
    fn history_entries_skip_inbound_and_incomplete() {
        let v = json!({"call_logs": [
            {"id": "L1", "call_path_id": "P1", "direction": "outbound",
             "callee_number": "+81312345678", "start_time": "2026-10-15T01:00:05Z"},
            {"id": "L2", "direction": "inbound", "callee_number": "+81312345678",
             "start_time": "2026-10-15T01:00:05Z"},
            {"id": "L3", "callee_number": "", "start_time": "2026-10-15T01:00:05Z"},
            {"id": "L4", "callee_number": "0312345678"},
            {"call_path_id": "P5", "callee_number": "0398765432", "start_time": "2026-10-15T02:00:00Z"},
        ]});
        let e = history_entries(&v);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].ids, vec!["L1", "P1"]);
        assert_eq!(e[0].number9, "312345678");
        assert_eq!(e[0].start_ms, ms("2026-10-15T01:00:05Z"));
        assert_eq!(e[1].ids, vec!["P5"]);
        assert!(history_entries(&json!({})).is_empty());
    }

    #[test]
    fn pick_match_takes_the_closest_within_sixty_seconds() {
        let e = |id: &str, n: &str, t: &str| HistoryEntry {
            ids: vec![id.into()],
            number9: n.into(),
            start_ms: ms(t),
        };
        let list = vec![
            e("far", "312345678", "2026-10-15T01:01:01Z"),
            e("near2", "312345678", "2026-10-15T01:00:50Z"),
            e("near1", "312345678", "2026-10-15T01:00:04Z"),
            e("other", "399999999", "2026-10-15T01:00:00Z"),
        ];
        let at = ms("2026-10-15T01:00:00Z");
        // 差 4 秒が最も近い。61 秒差は窓の外、別の番号は除く
        assert_eq!(pick_match(&list, "312345678", at).unwrap().ids[0], "near1");
        assert_eq!(
            pick_match(&list[..1], "312345678", at),
            None,
            "61 秒差は別の電話"
        );
        assert_eq!(pick_match(&list, "311111111", at), None);
        // ちょうど 60 秒は含む
        let edge = vec![e("edge", "312345678", "2026-10-15T01:01:00Z")];
        assert_eq!(pick_match(&edge, "312345678", at).unwrap().ids[0], "edge");
    }

    #[test]
    fn recording_id_comes_from_the_call_queue_element() {
        let v = json!({"call_elements": [
            {"caller_ext_type": "user", "recording_id": "USER_REC"},
            {"caller_ext_type": "call_queue", "recording_id": "CQ_REC"},
        ]});
        assert_eq!(recording_id_from(&v).as_deref(), Some("CQ_REC"));
        // Call Queue の要素に無ければ、録音のある他の要素
        let v2 = json!({"call_elements": [
            {"caller_ext_type": "call_queue"},
            {"caller_ext_type": "user", "recording_id": "USER_REC"},
        ]});
        assert_eq!(recording_id_from(&v2).as_deref(), Some("USER_REC"));
        assert_eq!(
            recording_id_from(&json!({"call_elements": [{"caller_ext_type": "call_queue"}]})),
            None
        );
        assert_eq!(recording_id_from(&json!({})), None);
    }
}
