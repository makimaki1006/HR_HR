//! 応募 → 求人票 → 取引の報告シート（読み取りのみ）。
use crate::hubspot::{HubSpotClient, HubSpotError, HubSpotRecord};
use unicode_normalization::UnicodeNormalization;

// 出典: Hubspot_Job-Posting/scripts/job_application_sync/{customer_sheet_sync.py,
// deal_master.py,deal_stages.py} (2026-10-19 照合)。
// deal_stages.WRITABLE_STAGES のみを生きた契約とする。未知のステージは含めない。
const LIVE_STAGES: &[&str] = &[
    "52016153",
    "52016154",
    "52016155",
    "1281184160",
    "1049738304",
    "121846962",
    "52016156",
    "121846961",
    "164378752",
    "164378755",
    "164378758",
    "1177549843",
    "1177549844",
    "1177549845",
    "1177549846",
    "1177549847",
    "1177549848",
    "164378760",
    "52016157",
    "1216616080",
    "1216616081",
    "1216616082",
    "1216616083",
];
pub(crate) const PROPERTIES: &[&str] = &[
    "customer_sheet_url",
    "dealstage",
    "dealname",
    "contract_start_date",
    "createdate",
    "hubspot_owner_id",
    "pipeline",
    "oubohoukokusaki",
    "ashigirijouken",
];
fn property<'a>(r: &'a HubSpotRecord, name: &str) -> &'a str {
    r.properties
        .get(name)
        .and_then(|v| v.as_deref())
        .unwrap_or("")
}
fn priority(r: &HubSpotRecord) -> u8 {
    let stage = property(r, "dealstage");
    if !LIVE_STAGES.contains(&stage) {
        return 0;
    }
    let name: String = property(r, "dealname").nfkc().collect();
    let option = stage == "1049738304"
        || [
            "求人追加",
            "AirWork広告運用",
            "一次対応",
            "エントリーフォーム",
        ]
        .iter()
        .any(|prefix| name.trim_start().starts_with(prefix));
    if option {
        1
    } else {
        2
    }
}
fn key(r: &HubSpotRecord) -> (u8, String, &str) {
    // Python の _period_key と同じ文字列比較。契約開始日は先頭10文字。
    (
        priority(r),
        property(r, "contract_start_date")
            .chars()
            .take(10)
            .collect(),
        property(r, "createdate"),
    )
}
pub(crate) fn owner_deal<'a>(
    ids: &[String],
    deals: &'a [HubSpotRecord],
) -> Option<&'a HubSpotRecord> {
    let mut best: Option<&HubSpotRecord> = None;
    for id in ids {
        if let Some(r) = deals
            .iter()
            .find(|r| &r.id == id)
            .filter(|r| !property(r, "customer_sheet_url").trim().is_empty())
        {
            // Python max 同様、同順位・同日時なら関連の先勝ち（batch応答順は使わない）。
            if best.is_none_or(|b| key(r) > key(b)) {
                best = Some(r);
            }
        }
    }
    best
}
#[derive(Default)]
pub(super) struct RelatedDeals {
    pub sheet: Option<String>,
    pub deals: Vec<HubSpotRecord>,
    pub current: Option<String>,
    pub unavailable: bool,
}
pub(super) async fn read(client: &HubSpotClient, listing_ids: &[String]) -> RelatedDeals {
    // 0-3 の API 別名 deals を使用（既存クライアントの許可リストに合わせる）。
    let result = async {
        let associations = client
            .batch_associations("0-420", "deals", listing_ids)
            .await?;
        let mut ids = Vec::new();
        for listing in listing_ids {
            for r in associations.get(listing).into_iter().flatten() {
                if !ids.contains(&r.id) {
                    ids.push(r.id.clone());
                }
            }
        }
        let deals = client.batch_read("deals", &ids, PROPERTIES).await?;
        let current_deal = owner_deal(&ids, &deals);
        let sheet = current_deal.map(|d| property(d, "customer_sheet_url").trim().to_owned());
        let current = current_deal.map(|d| d.id.clone());
        let mut ordered = Vec::new();
        for id in &ids {
            if let Some(d) = deals.iter().find(|d| &d.id == id) {
                ordered.push(d.clone());
            }
        }
        ordered.sort_by_key(|d| Some(&d.id) != current.as_ref());
        Ok::<_, HubSpotError>(RelatedDeals {
            sheet,
            deals: ordered,
            current,
            unavailable: false,
        })
    }
    .await;
    match result {
        Ok(url) => url,
        Err(_) => {
            // URL・取引情報・上流エラー本文はログへ出さない。
            tracing::warn!("applicant report sheet deal read failed; using fallback");
            RelatedDeals {
                unavailable: true,
                ..Default::default()
            }
        }
    }
}
