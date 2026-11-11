//! トークの振り返りの結果を HubSpot に残す (2026-11-11 ユーザー決定: 監査 Turso は使わず HubSpot に残す)。
//!
//! - 通話の文字起こし (`hrhr_transcript`): Zoom から取った文字起こしを通話に書き、次からはそれを読む。
//!   Zoom の録音が消えた後も、判定し直し・ヒアリング・画面の文字起こしに使える。
//! - 取引のランク (`talk_apo_rank` ほか): 決まったランクを取引に書き、再起動後はそれを読む (決め直さない)。
//!
//! プロパティは `scripts/ensure_talk_review_properties.py` で作る。HubSpot の呼び出しはすべて `Background`。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use super::apo_call::DealRank;
use super::types::TalkTurn;
use super::zoom::{TranscriptSource, ZoomError};
use crate::hubspot::{HubSpotClient, HubSpotError};

pub const TRANSCRIPT_PROP: &str = "hrhr_transcript";
pub const RANK_PROP: &str = "talk_apo_rank";
pub const RANK_ENTERED_PROP: &str = "talk_apo_rank_entered_at";
pub const RANK_CALL_PROP: &str = "talk_apo_rank_call_id";

/// HubSpot の文字の項目の上限 (65,536 字) より少し小さく切る
const MAX_CHARS: usize = 65_000;
const CUT_NOTE: &str = "（長さの上限のため、ここから後は省略）";
const REP: &str = "営業: ";
const CUSTOMER: &str = "先方: ";

/// 発言を 1 行 1 発言の文にする (純粋)。発言の中の改行は空白にする。上限を超えたら行の切れ目で切って印を付ける
pub fn to_text(turns: &[TalkTurn]) -> String {
    let mut out = String::new();
    let mut chars = 0;
    for t in turns {
        let head = if t.speaker == "rep" { REP } else { CUSTOMER };
        let line = format!("{head}{}", t.text.replace(['\r', '\n'], " "));
        let n = line.chars().count() + 1;
        if chars + n > MAX_CHARS - CUT_NOTE.chars().count() - 1 {
            out.push_str(CUT_NOTE);
            return out;
        }
        out.push_str(&line);
        out.push('\n');
        chars += n;
    }
    out.trim_end_matches('\n').to_string()
}

/// `to_text` の文を発言に戻す (純粋)。番号は 1 から振り直す (Zoom から取ったときと同じ番号になる)
pub fn from_text(s: &str) -> Vec<TalkTurn> {
    s.lines()
        .filter_map(|l| {
            let (speaker, text) = if let Some(t) = l.strip_prefix(REP) {
                ("rep", t)
            } else {
                ("customer", l.strip_prefix(CUSTOMER)?)
            };
            Some((speaker, text))
        })
        .enumerate()
        .map(|(i, (speaker, text))| TalkTurn {
            n: i as u32 + 1,
            speaker: speaker.into(),
            text: text.into(),
        })
        .collect()
}

/// 通話に残した文字起こし。無ければ `None`
pub async fn load_transcript(
    client: &HubSpotClient,
    call_id: &str,
) -> Result<Option<Vec<TalkTurn>>, HubSpotError> {
    let r = client
        .get_object("calls", call_id, &[TRANSCRIPT_PROP])
        .await?;
    Ok(r.properties
        .get(TRANSCRIPT_PROP)
        .cloned()
        .flatten()
        .map(|s| from_text(&s))
        .filter(|t| !t.is_empty()))
}

pub async fn save_transcript(
    client: &HubSpotClient,
    call_id: &str,
    turns: &[TalkTurn],
) -> Result<(), HubSpotError> {
    let props = BTreeMap::from([(TRANSCRIPT_PROP.to_string(), Some(to_text(turns)))]);
    client.patch_call(call_id, &props).await.map(|_| ())
}

/// 文字起こしを、通話に残したものから先に読み、無ければ Zoom から取って通話に残す
pub struct Stored<Z> {
    pub zoom: Arc<Z>,
    pub hubspot: Arc<HubSpotClient>,
}

impl<Z: TranscriptSource> TranscriptSource for Stored<Z> {
    async fn fetch(&self, call_log_id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
        self.zoom.fetch(call_log_id).await
    }

    async fn find_recording(&self, to: &str, at_ms: i64) -> Result<Option<String>, ZoomError> {
        self.zoom.find_recording(to, at_ms).await
    }

    async fn find_caller(&self, to: &str, at_ms: i64) -> Result<Option<String>, ZoomError> {
        self.zoom.find_caller(to, at_ms).await
    }

    async fn fetch_call(
        &self,
        hubspot_call_id: &str,
        call_log_id: Option<&str>,
    ) -> Result<Vec<TalkTurn>, ZoomError> {
        fetch_stored(
            &self.hubspot.background(),
            &*self.zoom,
            hubspot_call_id,
            call_log_id,
        )
        .await
    }
}

/// `Stored::fetch_call` の中身 (画面の詳細でも使う)
pub async fn fetch_stored<Z: TranscriptSource + ?Sized>(
    hubspot: &HubSpotClient,
    zoom: &Z,
    hubspot_call_id: &str,
    call_log_id: Option<&str>,
) -> Result<Vec<TalkTurn>, ZoomError> {
    match load_transcript(hubspot, hubspot_call_id).await {
        Ok(Some(t)) => return Ok(t),
        Ok(None) => {}
        Err(e) => {
            tracing::info!(error = %e.error_kind(), "talk review: stored transcript read failed")
        }
    }
    let Some(zid) = call_log_id else {
        return Err(ZoomError::Gone);
    };
    let r = zoom.fetch(zid).await;
    if let Ok(t) = &r {
        if !t.is_empty() {
            if let Err(e) = save_transcript(hubspot, hubspot_call_id, t).await {
                tracing::warn!(error = %e.error_kind(), "talk review: transcript save failed");
            }
        }
    }
    r
}

/// ランクの内部値 (`scripts/ensure_talk_review_properties.py` の RANK_OPTIONS と同じ)
pub fn rank_value(r: &DealRank) -> Option<&'static str> {
    match r {
        DealRank::Ranked { rank, .. } => Some(rank),
        DealRank::NoCall => Some("no_call"),
        DealRank::Unavailable => Some("unavailable"),
        DealRank::Pending => None,
    }
}

/// 取引に残したランク
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRank {
    /// ランクを決めたアポ (アポ日確定に入った時刻、ms)
    pub entered_ms: i64,
    pub rank: DealRank,
}

/// 内部値からランクに戻す (純粋)。知らない値は `None`
pub fn parse_rank(value: &str, call_id: Option<&str>, entered_ms: i64) -> Option<DealRank> {
    let rank: &'static str = match value {
        "S" => "S",
        "A" => "A",
        "B" => "B",
        "C" => "C",
        "no_call" => return Some(DealRank::NoCall),
        "unavailable" => return Some(DealRank::Unavailable),
        _ => return None,
    };
    let call_id = call_id.unwrap_or_default().to_string();
    Some(DealRank::Ranked {
        rank,
        calls: vec![call_id.clone()],
        call_id,
        call_ms: entered_ms,
        items: vec![],
    })
}

/// 取引に残したランクを読む (100 件ずつ)。ランクの無い取引は入れない
pub async fn load_ranks(
    client: &HubSpotClient,
    deal_ids: &[String],
) -> Result<HashMap<String, StoredRank>, HubSpotError> {
    let mut out = HashMap::new();
    for chunk in deal_ids.chunks(100) {
        let recs = client
            .batch_read(
                "deals",
                chunk,
                &[RANK_PROP, RANK_ENTERED_PROP, RANK_CALL_PROP],
            )
            .await?;
        for r in recs {
            let get = |k: &str| r.properties.get(k).cloned().flatten();
            let (Some(v), Some(at)) = (get(RANK_PROP), get(RANK_ENTERED_PROP)) else {
                continue;
            };
            let Some(entered_ms) = parse_ms(&at) else {
                continue;
            };
            if let Some(rank) = parse_rank(&v, get(RANK_CALL_PROP).as_deref(), entered_ms) {
                out.insert(r.id.clone(), StoredRank { entered_ms, rank });
            }
        }
    }
    Ok(out)
}

/// HubSpot の日時 (ISO 8601 か ms の数字)
fn parse_ms(s: &str) -> Option<i64> {
    s.parse::<i64>().ok().or_else(|| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&Utc).timestamp_millis())
    })
}

/// 決まったランクを取引に書く
pub async fn save_rank(
    client: &HubSpotClient,
    deal_id: &str,
    entered_ms: i64,
    rank: &DealRank,
) -> Result<(), HubSpotError> {
    let Some(v) = rank_value(rank) else {
        return Ok(());
    };
    let call = match rank {
        DealRank::Ranked { call_id, .. } => call_id.clone(),
        _ => String::new(),
    };
    let at = DateTime::from_timestamp_millis(entered_ms)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default();
    let props = BTreeMap::from([
        (RANK_PROP.to_string(), Some(v.to_string())),
        (RANK_ENTERED_PROP.to_string(), Some(at)),
        (RANK_CALL_PROP.to_string(), Some(call)),
    ]);
    client
        .patch_object("deals", deal_id, &props)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(n: u32, speaker: &str, text: &str) -> TalkTurn {
        TalkTurn {
            n,
            speaker: speaker.into(),
            text: text.into(),
        }
    }

    #[test]
    fn transcript_round_trips_with_the_same_numbers() {
        let turns = vec![
            turn(1, "rep", "お世話になっております。"),
            turn(2, "customer", "はい、\n担当です"),
            turn(3, "rep", "来週の火曜はいかがですか"),
        ];
        let text = to_text(&turns);
        assert_eq!(
            text,
            "営業: お世話になっております。\n先方: はい、 担当です\n営業: 来週の火曜はいかがですか"
        );
        let back = from_text(&text);
        assert_eq!(back.len(), 3);
        assert_eq!(back[1], turn(2, "customer", "はい、 担当です"));
        assert_eq!(back[2].n, 3);
    }

    #[test]
    fn long_transcript_is_cut_at_a_line_with_a_note() {
        let turns: Vec<TalkTurn> = (1..=2000)
            .map(|n| turn(n, "rep", &"あ".repeat(50)))
            .collect();
        let text = to_text(&turns);
        assert!(text.chars().count() <= MAX_CHARS);
        assert!(text.ends_with(CUT_NOTE));
        // 切った印の行は発言にしない
        let back = from_text(&text);
        assert!(back.iter().all(|t| t.text.chars().count() == 50));
        assert!(back.len() > 1000 && back.len() < 2000);
    }

    #[test]
    fn rank_values_round_trip() {
        for v in ["S", "A", "B", "C"] {
            let r = parse_rank(v, Some("call-1"), 5).unwrap();
            assert_eq!(rank_value(&r), Some(v));
            let DealRank::Ranked { call_id, .. } = r else {
                panic!()
            };
            assert_eq!(call_id, "call-1");
        }
        assert_eq!(parse_rank("no_call", None, 5), Some(DealRank::NoCall));
        assert_eq!(
            parse_rank("unavailable", None, 5),
            Some(DealRank::Unavailable)
        );
        assert_eq!(parse_rank("X", None, 5), None);
        assert_eq!(rank_value(&DealRank::Pending), None);
        assert_eq!(
            parse_ms("2026-11-01T00:00:00.000Z"),
            Some(1_793_491_200_000)
        );
        assert_eq!(parse_ms("1793491200000"), Some(1_793_491_200_000));
    }
}
