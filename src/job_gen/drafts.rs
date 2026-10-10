//! 求人の案の保存形式。求人本文と履歴の正本はHubSpotだけ。
use super::{
    fact_extract,
    hrhacker::HRHACKER_COLUMNS,
    types::{ExtractedFacts, FACT_KEYS},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const BODY: &str = "jobgen_draft_body";
pub const CREATED: &str = "jobgen_draft_created_at";
pub const STATUS: &str = "jobgen_draft_status";
pub const SOURCE: &str = "jobgen_draft_source";
pub const FACTS: &str = "jobgen_draft_facts";
pub const PROPERTIES: [&str; 5] = [BODY, CREATED, STATUS, SOURCE, FACTS];
const MAX_PROPERTY_CHARS: usize = 65_536;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
pub enum DraftSource {
    Csv,
    Pdf,
    Excel,
    FreeText,
    Url,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    Pending,
    Adopted,
    Rejected,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveDraftRequest {
    pub operation_id: String,
    pub base_revision: String,
    pub created_at: String,
    pub row: BTreeMap<String, String>,
    pub facts: ExtractedFacts,
    pub source_text: String,
    pub source_kind: DraftSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReviewDraftRequest {
    pub operation_id: String,
    pub base_revision: String,
    pub draft_id: String,
    pub status: DraftStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DraftOperationStatus {
    Saved,
    Pending,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DraftOperationResponse {
    pub status: DraftOperationStatus,
    pub code: String,
    pub operation_id: String,
    pub draft: Option<DraftSnapshot>,
    pub revision: Option<String>,
}
/// 自己完結した控えで、項目ごとの履歴時刻のずれ・同じ本文の再保存にも対応する。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DraftSnapshot {
    pub schema_version: u8,
    pub draft_id: String,
    pub created_at: String,
    pub source_kind: DraftSource,
    pub review_status: DraftStatus,
    pub row: BTreeMap<String, String>,
    pub facts: ExtractedFacts,
}
pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
pub fn revision(properties: &BTreeMap<String, Option<String>>) -> String {
    let values: Vec<_> = PROPERTIES
        .iter()
        .map(|p| {
            let value = properties.get(*p).and_then(|v| v.as_deref()).unwrap_or("");
            if *p == CREATED {
                let date = DateTime::parse_from_rfc3339(value)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
                    .or_else(|| {
                        value
                            .parse::<i64>()
                            .ok()
                            .and_then(DateTime::from_timestamp_millis)
                    });
                if let Some(date) = date {
                    return date.timestamp_millis().to_string();
                }
            }
            value.to_owned()
        })
        .collect();
    hash(&serde_json::to_string(&values).expect("strings serialize"))
}

pub fn valid_operation(id: &str, base: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
        && base.len() == 64
        && base.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn compose_body(row: &BTreeMap<String, String>) -> String {
    HRHACKER_COLUMNS
        .iter()
        .map(|c| format!("{c}：{}", row.get(*c).map(String::as_str).unwrap_or("")))
        .collect::<Vec<_>>()
        .join("\n")
}
impl DraftSnapshot {
    pub fn from_request(req: &SaveDraftRequest, now: DateTime<Utc>) -> Result<Self, &'static str> {
        if !valid_operation(&req.operation_id, &req.base_revision)
            || req.source_text.len() > 1_000_000
            || req.source_text.trim().is_empty()
            || req
                .row
                .keys()
                .any(|k| !HRHACKER_COLUMNS.contains(&k.as_str()))
            || req.facts.keys().any(|k| !FACT_KEYS.contains(&k.as_str()))
        {
            return Err("draft_invalid_input");
        }
        if !req.row.values().any(|v| !v.trim().is_empty()) {
            return Err("draft_empty");
        }
        let created = DateTime::parse_from_rfc3339(&req.created_at)
            .map_err(|_| "draft_invalid_input")?
            .with_timezone(&Utc);
        if created > now + chrono::Duration::minutes(5) {
            return Err("draft_invalid_input");
        }
        let raw = serde_json::to_value(&req.facts).map_err(|_| "draft_invalid_input")?;
        let facts = fact_extract::verify(&req.source_text, &raw);
        let row = HRHACKER_COLUMNS
            .iter()
            .map(|c| {
                (
                    (*c).to_owned(),
                    req.row
                        .get(*c)
                        .cloned()
                        .unwrap_or_default()
                        .replace("\r\n", "\n"),
                )
            })
            .collect();
        let result = Self {
            schema_version: 1,
            draft_id: req.operation_id.clone(),
            created_at: created.to_rfc3339(),
            source_kind: req.source_kind,
            review_status: DraftStatus::Pending,
            row,
            facts,
        };
        result.properties()?;
        Ok(result)
    }
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && uuid::Uuid::parse_str(&self.draft_id).is_ok()
            && DateTime::parse_from_rfc3339(&self.created_at).is_ok()
            && self.row.len() == 84
            && self
                .row
                .keys()
                .all(|k| HRHACKER_COLUMNS.contains(&k.as_str()))
            && self.facts.keys().all(|k| FACT_KEYS.contains(&k.as_str()))
    }
    pub fn properties(&self) -> Result<BTreeMap<String, Option<String>>, &'static str> {
        let snapshot = serde_json::to_string(self).map_err(|_| "draft_invalid_input")?;
        let body = compose_body(&self.row);
        if snapshot.chars().count() > MAX_PROPERTY_CHARS
            || body.chars().count() > MAX_PROPERTY_CHARS
        {
            return Err("draft_too_long");
        }
        Ok(BTreeMap::from([
            (BODY.into(), Some(body)),
            (
                CREATED.into(),
                Some(
                    DateTime::parse_from_rfc3339(&self.created_at)
                        .map_err(|_| "draft_invalid_input")?
                        .timestamp_millis()
                        .to_string(),
                ),
            ),
            (
                STATUS.into(),
                Some(
                    serde_json::to_value(self.review_status)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
            ),
            (
                SOURCE.into(),
                Some(
                    serde_json::to_value(self.source_kind)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
            ),
            (FACTS.into(), Some(snapshot)),
        ]))
    }
}
/// 過去の状態変更は同じ案にまとめる。異常な履歴は飛ばし、未取得を通知する。
pub fn history(data: &Value) -> (Vec<DraftSnapshot>, bool) {
    let entries = data["propertiesWithHistory"][FACTS].as_array();
    let mut values: Vec<(String, String)> = entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                entry["timestamp"].as_str()?.to_owned(),
                entry["value"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let mut incomplete = entries.is_some_and(|v| {
        v.len() >= 20
            || v.iter().any(|e| {
                e["timestamp"]
                    .as_str()
                    .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                    .is_none()
            })
    });
    values.sort_by_key(|v| {
        DateTime::parse_from_rfc3339(&v.0)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    });
    // 現在値も読む。履歴の保持上限・履歴未取得でも最新の案を表示できる。
    if let Some(current) = data["properties"][FACTS].as_str().filter(|s| !s.is_empty()) {
        values.push((String::new(), current.to_owned()));
    }
    let mut by_id = BTreeMap::new();
    for (order, (_, raw)) in values.into_iter().enumerate() {
        match serde_json::from_str::<DraftSnapshot>(&raw) {
            Ok(snapshot) if snapshot.valid() => {
                by_id.insert(snapshot.draft_id.clone(), (order, snapshot));
            }
            _ => {
                if !raw.is_empty() {
                    incomplete = true;
                }
            }
        }
    }
    let mut snapshots: Vec<_> = by_id.into_values().collect();
    // 生成が早い案を後から保存しても、現在値を最新の保存として扱う。
    snapshots.sort_by_key(|(order, _)| *order);
    (
        snapshots
            .into_iter()
            .map(|(_, snapshot)| snapshot)
            .collect(),
        incomplete,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn request() -> SaveDraftRequest {
        serde_json::from_value(serde_json::json!({"operation_id":"10000000-0000-4000-8000-000000000001","base_revision":revision(&BTreeMap::new()),"created_at":"2026-10-10T00:00:00Z","source_kind":"csv","source_text":"給与は月給270,000円〜300,000円。勤務地は大分県大分市。","row":{"案件名":"配送スタッフ","仕事内容":"日用品を配送します。","給与形態":"月給","基本給与 最小":"270000","基本給与 最大":"300000"},"facts":{"salary":{"value":"月給270,000円〜300,000円","evidence_quote":"給与は月給270,000円〜300,000円。","status":"verified"},"work_location":{"value":"大分県大分市","evidence_quote":"勤務地は大分県大分市。","status":"verified"}}})).unwrap()
    }
    #[test]
    fn payload_has_all_84_columns_in_order_and_only_five_draft_properties() {
        let s = DraftSnapshot::from_request(&request(), Utc::now()).unwrap();
        let p = s.properties().unwrap();
        let body = p[BODY].as_ref().unwrap();
        assert_eq!(s.row.len(), 84);
        assert_eq!(p.len(), 5);
        assert_eq!(s.review_status, DraftStatus::Pending);
        assert_eq!(p[CREATED].as_deref(), Some("1791590400000"));
        assert_eq!(s.created_at, "2026-10-10T00:00:00+00:00");
        let mut api_properties = p.clone();
        api_properties.insert(CREATED.into(), Some("2026-10-10T00:00:00.000Z".into()));
        assert_eq!(revision(&api_properties), revision(&p));
        let headers: Vec<_> = body
            .lines()
            .map(|line| line.split_once('：').unwrap().0)
            .collect();
        assert_eq!(headers, HRHACKER_COLUMNS);
        assert!(body.contains("基本給与 最小：270000\n基本給与 最大：300000"));
        assert_eq!(s.facts["salary"].status, "verified");
    }
    #[test]
    fn forged_fact_empty_row_unknown_columns_and_oversize_are_refused() {
        let mut r = request();
        r.facts.get_mut("salary").unwrap().evidence_quote = "時給9000円".into();
        assert_eq!(
            DraftSnapshot::from_request(&r, Utc::now()).unwrap().facts["salary"].status,
            "rejected"
        );
        r.row.clear();
        assert_eq!(
            DraftSnapshot::from_request(&r, Utc::now()).unwrap_err(),
            "draft_empty"
        );
        r.row.insert("秘密".into(), "x".into());
        assert_eq!(
            DraftSnapshot::from_request(&r, Utc::now()).unwrap_err(),
            "draft_invalid_input"
        );
        r.row = request().row;
        r.row.insert("仕事内容".into(), "あ".repeat(65_536));
        assert_eq!(
            DraftSnapshot::from_request(&r, Utc::now()).unwrap_err(),
            "draft_too_long"
        );
    }
    #[test]
    fn latest_saved_draft_is_current_even_when_generated_before_history() {
        let current = DraftSnapshot::from_request(&request(), Utc::now()).unwrap();
        let mut newer_generation = current.clone();
        newer_generation.draft_id = "20000000-0000-4000-8000-000000000002".into();
        newer_generation.created_at = "2026-10-11T00:00:00Z".into();
        let data = serde_json::json!({"properties":{FACTS:serde_json::to_string(&current).unwrap()},"propertiesWithHistory":{FACTS:[{"timestamp":"2026-10-11T01:00:00Z","value":serde_json::to_string(&newer_generation).unwrap()},{"timestamp":"2026-10-11T02:00:00Z","value":serde_json::to_string(&current).unwrap()}]}});
        let (items, incomplete) = history(&data);
        assert!(!incomplete);
        assert_eq!(items.len(), 2);
        assert_eq!(items.last().unwrap().draft_id, current.draft_id);
        assert_eq!(items[0].draft_id, newer_generation.draft_id);
    }
    #[test]
    fn histories_restore_identical_bodies_as_separate_drafts_and_merge_status_changes() {
        let first =
            DraftSnapshot::from_request(&request(), "2026-10-10T00:00:00Z".parse().unwrap())
                .unwrap();
        let mut second = first.clone();
        second.draft_id = "20000000-0000-4000-8000-000000000002".into();
        second.created_at = "2026-10-11T00:00:00Z".into();
        let mut adopted = second.clone();
        adopted.review_status = DraftStatus::Adopted;
        let data = serde_json::json!({"properties":{FACTS:serde_json::to_string(&adopted).unwrap()},"propertiesWithHistory":{FACTS:[{"timestamp":"2026-10-11T01:00:00Z","value":serde_json::to_string(&second).unwrap()},{"timestamp":"2026-10-10T00:00:00Z","value":serde_json::to_string(&first).unwrap()}]}});
        let (items, incomplete) = history(&data);
        assert!(!incomplete);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].draft_id, first.draft_id);
        assert_eq!(items[1].review_status, DraftStatus::Adopted);
        assert_eq!(items[0].row, items[1].row);
        let mut bad = data;
        bad["propertiesWithHistory"][FACTS]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"timestamp":"2026-10-11T02:00:00Z","value":"broken"}));
        assert!(history(&bad).1);
        assert_eq!(history(&bad).0.len(), 2);
    }
}
