//! 案件 → 担当者・会社の関連ラベル (「主」「Primary」等) の定義 (6 時間キャッシュ)。
//!
//! 架電ワークスペース (`workspace`) は、案件の本体と担当者・会社の関連 ID を 1 回の
//! `GET deals/{id}?associations=..,contacts,companies` で読む (v3)。v3 の関連はラベルの名前を返さず、
//! `type` に型名 (`deal_to_company` / `deal_to_company_unlabeled` / 数字の typeId 等) だけを返す。
//! そこで関連ラベルの定義 (`GET /crm/v4/associations/deals/{contacts|companies}/labels`) を引いて、
//! 型名 → typeId → ラベル名 に直す。直せない型名があれば、呼び出し側は従来の v4 の関連 (ラベル付き) を読み直す。
//!
//! 実データでの確認 (2026-10-08、読み取りのみ。案件 100 件): v3 の型名と v4 の関連ラベルの対応は
//! `deal_to_company` = typeId 5 (`Primary`)、`deal_to_company_unlabeled` = 341 (無ラベル)、
//! `deal_to_contact` = 3 (無ラベル) で、関連の並び順も v4 と一致した。
//! 案件 → 担当者・会社に利用者定義のラベル (USER_DEFINED) はまだ無いため、その v3 の型名の形は未確認。
//! 数字の typeId か、定義のラベル名と一致する型名なら引ける。どちらでもなければ「直せない」として v4 に戻す。
//!
//! ## HubSpot 呼び出し回数
//! キャッシュが冷えているときだけ 2 回 (案件 → 担当者・案件 → 会社 を並列)。失敗は [`FAILURE_TTL`] だけ覚える。

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::hubspot::{AssociationLabelDef, HubSpotClient, HubSpotError};

/// 定義を使い回す時間
pub const ASSOC_LABELS_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// 失敗を覚えておく時間 (詳細を開くたびに失敗する呼び出しを繰り返さない)
pub const FAILURE_TTL: Duration = Duration::from_secs(60);

/// HubSpot 定義済みの v3 の型名 → typeId (案件 → 担当者・会社)。実データで確認したものだけ
const HUBSPOT_DEFINED_TYPE_NAMES: &[(&str, u64)] = &[
    ("deal_to_contact", 3),
    ("deal_to_company", 5),
    ("deal_to_company_unlabeled", 341),
];

/// 案件 → 担当者・案件 → 会社 の関連ラベルの定義
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DealAssocLabelDefs {
    pub contacts: Vec<AssociationLabelDef>,
    pub companies: Vec<AssociationLabelDef>,
}

#[derive(Default)]
struct Slot {
    ok: Option<(Instant, Arc<DealAssocLabelDefs>)>,
    failed: Option<(Instant, HubSpotError)>,
}

pub struct AssocLabelCache {
    slot: Mutex<Slot>,
    ttl: Duration,
    failure_ttl: Duration,
}

impl Default for AssocLabelCache {
    fn default() -> Self {
        Self {
            slot: Mutex::new(Slot::default()),
            ttl: ASSOC_LABELS_TTL,
            failure_ttl: FAILURE_TTL,
        }
    }
}

impl AssocLabelCache {
    /// 定義を返す。冷えていれば 2 回 (並列) 読む。同時に冷えた要求は 1 回の取得にまとめる (ロックを持ったまま読む)
    pub async fn get(
        &self,
        client: &HubSpotClient,
    ) -> Result<Arc<DealAssocLabelDefs>, HubSpotError> {
        let mut slot = self.slot.lock().await;
        if let Some((at, defs)) = slot.ok.as_ref() {
            if at.elapsed() < self.ttl {
                return Ok(defs.clone());
            }
        }
        if let Some((at, e)) = slot.failed.as_ref() {
            if at.elapsed() < self.failure_ttl {
                return Err(e.clone());
            }
        }
        let fetched = tokio::try_join!(
            client.association_labels("deals", "contacts"),
            client.association_labels("deals", "companies"),
        );
        match fetched {
            Ok((contacts, companies)) => {
                let defs = Arc::new(DealAssocLabelDefs {
                    contacts,
                    companies,
                });
                slot.ok = Some((Instant::now(), defs.clone()));
                slot.failed = None;
                Ok(defs)
            }
            Err(e) => {
                slot.failed = Some((Instant::now(), e.clone()));
                Err(e)
            }
        }
    }
}

/// v3 の型名 (1 件の関連に付いた全部) → ラベル名 (v4 の `associationTypes[].label` のうち null でないものと同じ並び)。
/// 直せない型名が 1 つでもあれば `None` (呼び出し側は v4 の関連を読み直す)。
pub fn resolve_labels(type_names: &[String], defs: &[AssociationLabelDef]) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for name in type_names {
        let type_id = name.trim().parse::<u64>().ok().or_else(|| {
            HUBSPOT_DEFINED_TYPE_NAMES
                .iter()
                .find(|(n, _)| *n == name.as_str())
                .map(|(_, id)| *id)
        });
        // 定義に無い typeId・定義のラベル名と一致しない型名は「直せない」(None)。黙って無ラベルにしない
        let label = match type_id {
            Some(id) => defs.iter().find(|d| d.type_id == id)?.label.clone(),
            // 型名がラベル名そのものになっている形 (未確認の形への備え)
            None => defs
                .iter()
                .find(|d| d.label.as_deref() == Some(name.as_str()))?
                .label
                .clone(),
        };
        if let Some(l) = label {
            if !out.contains(&l) {
                out.push(l);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(cat: &str, id: u64, label: Option<&str>) -> AssociationLabelDef {
        AssociationLabelDef {
            category: cat.to_string(),
            type_id: id,
            label: label.map(str::to_string),
        }
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn 実データの会社の型名は_primary_に直る() {
        let defs = vec![
            def("HUBSPOT_DEFINED", 341, None),
            def("HUBSPOT_DEFINED", 5, Some("Primary")),
        ];
        assert_eq!(
            resolve_labels(
                &names(&["deal_to_company", "deal_to_company_unlabeled"]),
                &defs
            ),
            Some(names(&["Primary"]))
        );
        assert_eq!(
            resolve_labels(&names(&["deal_to_company_unlabeled"]), &defs),
            Some(vec![])
        );
        assert_eq!(resolve_labels(&[], &defs), Some(vec![]));
    }

    #[test]
    fn 担当者の既定の関連は無ラベル() {
        let defs = vec![def("HUBSPOT_DEFINED", 3, None)];
        assert_eq!(
            resolve_labels(&names(&["deal_to_contact"]), &defs),
            Some(vec![])
        );
    }

    #[test]
    fn 数字の型名は定義の_type_id_で引き_ラベル名の型名も引ける() {
        let defs = vec![
            def("HUBSPOT_DEFINED", 3, None),
            def("USER_DEFINED", 17, Some("主")),
        ];
        assert_eq!(
            resolve_labels(&names(&["deal_to_contact", "17"]), &defs),
            Some(names(&["主"]))
        );
        assert_eq!(resolve_labels(&names(&["主"]), &defs), Some(names(&["主"])));
    }

    #[test]
    fn 直せない型名があれば_none() {
        let defs = vec![def("USER_DEFINED", 17, Some("主"))];
        assert_eq!(
            resolve_labels(&names(&["deal_to_contact_main"]), &defs),
            None
        );
        // 定義に無い typeId (数字でも、定義済みの型名でも)
        assert_eq!(resolve_labels(&names(&["99"]), &defs), None);
        assert_eq!(resolve_labels(&names(&["deal_to_company"]), &defs), None);
    }
}
