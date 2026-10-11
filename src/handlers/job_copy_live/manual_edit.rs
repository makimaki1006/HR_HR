use crate::job_gen::{
    drafts::{self, DraftSnapshot, DraftSource, ManualDraftRequest, ManualEdit, SaveDraftRequest},
    hrhacker::HRHACKER_COLUMNS,
};
use std::collections::BTreeMap;

pub(super) fn published(properties: &BTreeMap<String, Option<String>>) -> Option<&str> {
    let property = if properties
        .get("id_hrhakkaa")
        .and_then(|v| v.as_deref())
        .is_some_and(|v| !v.is_empty())
    {
        "hrh_kyuujinhyou_honbun"
    } else {
        "shigotonaiyou"
    };
    properties
        .get(property)
        .and_then(|v| v.as_deref())
        .filter(|v| !v.trim().is_empty())
}
pub(super) fn row(body: &str) -> BTreeMap<String, String> {
    let fields = super::hrh_copy::fields84(body);
    let mut row: BTreeMap<_, _> = HRHACKER_COLUMNS
        .iter()
        .map(|key| {
            (
                (*key).to_owned(),
                fields.get(key).cloned().unwrap_or_default(),
            )
        })
        .collect();
    if fields.is_empty() {
        row.insert("仕事内容".into(), body.into());
    }
    row
}
pub(super) fn current_row(
    properties: &BTreeMap<String, Option<String>>,
) -> Option<BTreeMap<String, String>> {
    let mut values = row(published(properties)?);
    if let Some(id) = properties
        .get("id_hrhakkaa")
        .and_then(|v| v.as_ref())
        .filter(|v| !v.is_empty())
    {
        values.insert("求人id".into(), id.clone());
        if let Some(shop) = properties
            .get("id_shop_hrhakkaa")
            .and_then(|v| v.as_ref())
            .filter(|v| !v.is_empty())
        {
            values.insert("店舗id".into(), shop.clone());
        }
        if let Some(status) = properties
            .get("baitai_genjoukyou_hrhakkaa")
            .and_then(|v| v.as_ref())
            .filter(|v| ["公開", "非公開", "公開開始前", "公開終了"].contains(&v.as_str()))
        {
            values.insert("公開".into(), status.clone());
        }
        if let Some(raw) = properties
            .get("hrh_kyuujinhyou_gazou")
            .and_then(|v| v.as_deref())
        {
            for (slot, url) in super::listings::image_urls(raw)
                .into_iter()
                .take(3)
                .enumerate()
            {
                values.insert(format!("画像{}", slot + 1), url);
            }
        }
    }
    Some(values)
}
pub(super) fn protected(
    key: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> bool {
    key.contains("給与")
        || key.contains("残業")
        || key.contains("勤務")
        || key.contains("稼働")
        || key.contains("タスク")
        || key.contains("雇用")
        || key.contains("研修")
        || key == "通勤経路"
        || key == "最寄り駅"
        || key.starts_with("自由項目")
        || key.starts_with("仕事情報補足")
        || ["仕事内容", "キャッチコピー", "メリット"].contains(&key)
        || [before.get(key), after.get(key)]
            .into_iter()
            .flatten()
            .any(|v| {
                ["休日", "勤務地", "休暇"]
                    .iter()
                    .any(|word| v.contains(word))
            })
}
pub(super) fn snapshot(
    req: &ManualDraftRequest,
    properties: &BTreeMap<String, Option<String>>,
    base: Option<&DraftSnapshot>,
) -> Result<DraftSnapshot, &'static str> {
    let body = published(properties).ok_or("draft_body_unavailable")?;
    let current = current_row(properties).ok_or("draft_body_unavailable")?;
    let source = if let Some(base) = base {
        drafts::compose_body(&base.row)
    } else {
        body.to_owned()
    };
    if drafts::hash(&source) != req.base_body_hash {
        return Err("draft_conflict");
    }
    if req.row.len() != 84 || req.reason.chars().count() > 2000 {
        return Err("draft_invalid_input");
    }
    let baseline = base.map(|s| &s.row).unwrap_or(&current);
    // 管理番号は表示も編集もせず、サーバーで取得した値を維持する。
    if ["求人id", "店舗id", "職種id"]
        .iter()
        .any(|key| req.row.get(*key) != current.get(*key))
    {
        return Err("draft_identity_changed");
    }
    let changes: Vec<String> = HRHACKER_COLUMNS
        .iter()
        .filter(|key| {
            protected(key, &current, &req.row)
                && (req.row.get(**key) != current.get(**key)
                    || req.row.get(**key) != baseline.get(**key))
        })
        .map(|key| (*key).to_owned())
        .collect();
    if !changes.is_empty() && req.reason.trim().is_empty() {
        return Err("draft_reason_required");
    }
    let now = chrono::Utc::now();
    let request = SaveDraftRequest {
        operation_id: req.operation_id.clone(),
        base_revision: req.base_revision.clone(),
        created_at: now.to_rfc3339(),
        row: req.row.clone(),
        facts: BTreeMap::new(),
        source_text: source,
        original_source_text: None,
        source_kind: DraftSource::FreeText,
    };
    let mut result = DraftSnapshot::from_request(&request, now)?;
    result.manual_edit = Some(ManualEdit {
        published_hash: drafts::hash(body),
        base_draft_id: req.base_draft_id.clone(),
        reason: req.reason.trim().into(),
        changed_conditions: changes,
    });
    result.properties()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ManualDraftRequest, BTreeMap<String, Option<String>>) {
        let body = "求人id：secret-job\n店舗id：secret-shop\n職種id：secret-role\n案件名：架空の配送\n仕事内容：商品を配送\n雇用形態：正社員\n給与形態：月給\n基本給与 最小：250000\n基本給与 最大：280000\n勤務時間：9〜18時\n自由項目1のタイトル：休日\n自由項目1の内容：土日休み";
        let properties = BTreeMap::from([
            ("id_hrhakkaa".into(), Some("secret-job".into())),
            ("id_shop_hrhakkaa".into(), Some("secret-shop".into())),
            ("hrh_kyuujinhyou_honbun".into(), Some(body.into())),
        ]);
        (
            ManualDraftRequest {
                operation_id: uuid::Uuid::new_v4().to_string(),
                base_revision: drafts::revision(&properties),
                base_body_hash: drafts::hash(body),
                base_draft_id: None,
                row: current_row(&properties).unwrap(),
                reason: String::new(),
            },
            properties,
        )
    }
    #[test]
    fn all_84_fields_including_management_contact_and_publication_roundtrip() {
        let values: BTreeMap<String, String> = HRHACKER_COLUMNS
            .iter()
            .enumerate()
            .map(|(i, key)| ((*key).to_owned(), format!("架空値{i}")))
            .collect();
        assert_eq!(row(&drafts::compose_body(&values)), values);
        let (mut req, properties) = fixture();
        assert_eq!(req.row["求人id"], "secret-job");
        assert_eq!(req.row["店舗id"], "secret-shop");
        assert_eq!(req.row["職種id"], "secret-role");
        req.row.insert("公開".into(), "非公開".into());
        assert_eq!(
            snapshot(&req, &properties, None).unwrap().row["公開"],
            "非公開"
        );
    }
    #[test]
    fn saved_draft_with_missing_management_keeps_current_identity_without_a_reason() {
        let (mut req, properties) = fixture();
        let mut base = snapshot(&req, &properties, None).unwrap();
        for key in ["求人id", "店舗id", "職種id"] {
            base.row.insert(key.into(), String::new());
        }
        req.base_draft_id = Some(base.draft_id.clone());
        req.base_body_hash = drafts::hash(&drafts::compose_body(&base.row));
        let saved = snapshot(&req, &properties, Some(&base)).unwrap();
        assert_eq!(saved.row["求人id"], "secret-job");
        assert_eq!(saved.row["店舗id"], "secret-shop");
        assert!(saved.manual_edit.unwrap().changed_conditions.is_empty());
    }
    #[test]
    fn salary_location_hours_employment_and_holidays_need_reason() {
        for (key, value) in [
            ("基本給与 最小", "270000"),
            ("通勤経路", "東京の事業所"),
            ("勤務時間", "8〜17時"),
            ("雇用形態", "パート"),
            ("自由項目1の内容", "日曜休み"),
        ] {
            let (mut req, properties) = fixture();
            req.row.insert(key.into(), value.into());
            assert_eq!(
                snapshot(&req, &properties, None).unwrap_err(),
                "draft_reason_required"
            );
            req.reason = "担当者が変更後の募集条件を確認したため".into();
            let saved = snapshot(&req, &properties, None).unwrap();
            assert_eq!(saved.row[key], value);
            assert_eq!(saved.review_status, drafts::DraftStatus::Pending);
            assert_eq!(saved.source_kind, DraftSource::FreeText);
            assert!(saved
                .manual_edit
                .unwrap()
                .changed_conditions
                .contains(&key.into()));
        }
    }
    #[test]
    fn stale_body_forged_identity_unknown_columns_and_existing_ai_conditions_are_checked() {
        let (mut req, properties) = fixture();
        req.base_body_hash = drafts::hash("古い本文");
        assert_eq!(
            snapshot(&req, &properties, None).unwrap_err(),
            "draft_conflict"
        );
        req.base_body_hash = drafts::hash(published(&properties).unwrap());
        req.row.insert("求人id".into(), "forged".into());
        assert_eq!(
            snapshot(&req, &properties, None).unwrap_err(),
            "draft_identity_changed"
        );
        let (mut req, properties) = fixture();
        let mut base = snapshot(&req, &properties, None).unwrap();
        base.row.insert("基本給与 最小".into(), "270000".into());
        req.row = base.row.clone();
        req.base_draft_id = Some(base.draft_id.clone());
        req.base_body_hash = drafts::hash(&drafts::compose_body(&base.row));
        assert_eq!(
            snapshot(&req, &properties, Some(&base)).unwrap_err(),
            "draft_reason_required"
        );
        req.reason = "条件の変更を確認".into();
        let saved = snapshot(&req, &properties, Some(&base)).unwrap();
        assert!(
            saved.facts.values().all(|fact| fact.status != "verified"),
            "手直しした値を検証済みの事実にしない"
        );
        assert_eq!(
            saved.manual_edit.unwrap().base_draft_id,
            Some(base.draft_id.clone())
        );
        req.row.insert("知らない項目".into(), "x".into());
        assert_eq!(
            snapshot(&req, &properties, Some(&base)).unwrap_err(),
            "draft_invalid_input"
        );
    }
}
