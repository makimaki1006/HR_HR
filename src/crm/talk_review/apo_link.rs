//! ヒアリングの対象を HubSpot のアポの記録で決める (2026-11-04 ユーザー決定)。設計書 §22 の「ヒアリングの対象と過去の行」。
//!
//! 電話の 1 時間前から 72 時間後までに、その電話の取引 (電話に付いた取引と、電話の担当者 (コンタクト) に付いた取引) が
//! アポ日確定 (`ST_APO` / `ST_APO_BPO`) に入っていれば、その電話をヒアリングの対象にする。
//! モデルの「日程が決まった」の判定は使わない (2026-11-03 の下見で、モデルが日程が決まったとした 128 本のうち
//! アポの記録があったのは 62 本で、読んだ食い違いはすべて記録の方が正しかった)。
//!
//! HubSpot の呼び出し (全部読み取り。呼ぶ側が `Background` の優先度にする。N = 電話の数、C = 担当者の数、D = 取引の数):
//! 電話→取引 ⌈N/100⌉ + 電話→担当者 ⌈N/100⌉ + 担当者→取引 ⌈C/100⌉ + 取引の読み取り ⌈D/100⌉。

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::DateTime;
use serde_json::Value;

use super::counts::entered_ms;
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};
use crate::hubspot::{HubSpotClient, HubSpotError};

/// 電話より前に入ったアポ日確定を同じ電話のアポとみなす幅 (記録の時刻のずれの余裕)
pub const BEFORE_MS: i64 = 3_600_000;
/// 電話の後に入ったアポ日確定を同じ電話のアポとみなす幅。これを過ぎて記録が無ければ対象外と決める
pub const AFTER_MS: i64 = 72 * 3_600_000;

/// アポ日確定に入った時刻が、電話の時刻から見て窓の中か
pub fn in_window(call_ms: i64, entered: i64) -> bool {
    (call_ms - BEFORE_MS..=call_ms + AFTER_MS).contains(&entered)
}

/// アポの記録を待つ期間が過ぎたか (過ぎて記録が無ければ対象外と決めてよい)
pub fn waited_enough(call_ms: i64, now_ms: i64) -> bool {
    now_ms > call_ms + AFTER_MS
}

/// `call_at` (RFC3339) → ms
pub fn call_ms(call_at: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(call_at)
        .ok()
        .map(|t| t.timestamp_millis())
}

fn entered_props() -> [String; 2] {
    [
        format!("hs_v2_date_entered_{ST_APO}"),
        format!("hs_v2_date_entered_{ST_APO_BPO}"),
    ]
}

/// 取引のアポ日確定に入った時刻 (ms) を、電話ごとに集めた取引から突き合わせる (純粋)。
/// `call_deals` は電話 → 取引 ID、`entered` は取引 ID → アポ日確定に入った時刻
pub fn matched(
    calls: &[(String, i64)],
    call_deals: &HashMap<String, BTreeSet<String>>,
    entered: &HashMap<String, Vec<i64>>,
) -> HashSet<String> {
    calls
        .iter()
        .filter(|(id, at)| {
            call_deals.get(id).is_some_and(|ds| {
                ds.iter()
                    .filter_map(|d| entered.get(d))
                    .flatten()
                    .any(|e| in_window(*at, *e))
            })
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// 電話 (HubSpot の Call の ID, 時刻 ms) のうち、窓の中にアポ日確定の記録がある電話の ID
pub async fn matched_calls(
    client: &HubSpotClient,
    calls: &[(String, i64)],
) -> Result<HashSet<String>, HubSpotError> {
    if calls.is_empty() {
        return Ok(HashSet::new());
    }
    let ids: Vec<String> = calls.iter().map(|(id, _)| id.clone()).collect();
    let direct = client.batch_associations("calls", "deals", &ids).await?;
    let contacts = client.batch_associations("calls", "contacts", &ids).await?;
    let contact_ids: Vec<String> = contacts
        .values()
        .flatten()
        .map(|r| r.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let contact_deals = client
        .batch_associations("contacts", "deals", &contact_ids)
        .await?;
    let mut call_deals: HashMap<String, BTreeSet<String>> = HashMap::new();
    for id in &ids {
        let set = call_deals.entry(id.clone()).or_default();
        set.extend(direct.get(id).into_iter().flatten().map(|r| r.id.clone()));
        for c in contacts.get(id).into_iter().flatten() {
            set.extend(
                contact_deals
                    .get(&c.id)
                    .into_iter()
                    .flatten()
                    .map(|r| r.id.clone()),
            );
        }
    }
    let deal_ids: Vec<String> = call_deals
        .values()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let props = entered_props();
    let prop_refs: Vec<&str> = props.iter().map(String::as_str).collect();
    let mut entered: HashMap<String, Vec<i64>> = HashMap::new();
    for r in client.batch_read("deals", &deal_ids, &prop_refs).await? {
        let times: Vec<i64> = props
            .iter()
            .filter_map(|p| r.properties.get(p).cloned().flatten())
            .filter_map(|s| entered_ms(&Value::String(s)))
            .collect();
        if !times.is_empty() {
            entered.insert(r.id, times);
        }
    }
    Ok(matched(calls, &call_deals, &entered))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    #[test]
    fn window_is_one_hour_before_to_72_hours_after() {
        let t = 1_000 * H;
        assert!(in_window(t, t - H), "1 時間前まで");
        assert!(!in_window(t, t - H - 1));
        assert!(in_window(t, t + 72 * H), "72 時間後まで");
        assert!(!in_window(t, t + 72 * H + 1));
        assert!(!waited_enough(t, t + 72 * H));
        assert!(waited_enough(t, t + 72 * H + 1));
    }

    #[test]
    fn a_call_matches_through_its_own_or_its_contacts_deals() {
        let t = 1_000 * H;
        let calls = vec![
            ("c1".to_string(), t),
            ("c2".to_string(), t),
            ("c3".to_string(), t),
        ];
        let call_deals = HashMap::from([
            ("c1".to_string(), BTreeSet::from(["d1".to_string()])),
            (
                "c2".to_string(),
                BTreeSet::from(["d2".to_string(), "d3".to_string()]),
            ),
            ("c3".to_string(), BTreeSet::from(["d4".to_string()])),
        ]);
        let entered = HashMap::from([
            ("d1".to_string(), vec![t + 30 * 60_000]),
            // 別の取引のずっと前のアポは当たらず、もう 1 件の取引の 2 日後のアポで当たる
            ("d2".to_string(), vec![t - 30 * 24 * H]),
            ("d3".to_string(), vec![t + 48 * H]),
            // 電話より前に決まっていたアポ (確認の電話) は当たらない
            ("d4".to_string(), vec![t - 3 * 24 * H]),
        ]);
        let m = matched(&calls, &call_deals, &entered);
        assert_eq!(
            m,
            HashSet::from(["c1".to_string(), "c2".to_string()]),
            "c3 は前に決まっていたアポだけ"
        );
    }
}
