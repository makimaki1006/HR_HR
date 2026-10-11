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
    let structured = body.trim_start().lines().next().is_some_and(|line| {
        line.split_once(['：', ':'])
            .is_some_and(|(heading, _)| HRHACKER_COLUMNS.contains(&heading.trim()))
    });
    let fields = if structured {
        super::hrh_copy::fields84(body)
    } else {
        Default::default()
    };
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
    let body = published(properties)?;
    let is_hrh = properties
        .get("id_hrhakkaa")
        .and_then(|v| v.as_deref())
        .is_some_and(|v| !v.is_empty());
    let mut values = if is_hrh {
        row(body)
    } else {
        let mut values = row("");
        values.insert("仕事内容".into(), body.into());
        values
    };
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
// 判定対象の唯一の定義。画面も同じJSONを読み、本文の意味では分類しない。
pub(super) static PROTECTED_COLUMNS: std::sync::LazyLock<Vec<String>> =
    std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../frontend/src/screens/job-copy/manualEditPolicy.json"
        ))
        .expect("checked-in manual edit policy must be valid JSON")
    });
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
            PROTECTED_COLUMNS.iter().any(|column| column == **key)
                && **key != "店舗id"
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
    // 手直しは生成案の改行整形を適用せず、検証した入力値をそのまま保存する。
    result.row = req.row.clone();
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
    fn airwork_keeps_entire_body_even_when_lines_look_like_hrh_columns() {
        for body in [
            "訪問看護師募集。時給1800円。\n勤務時間：9〜18時",
            "勤務時間：9〜18時\n訪問看護師募集。時給1800円。",
        ] {
            let properties = BTreeMap::from([("shigotonaiyou".into(), Some(body.into()))]);
            let values = current_row(&properties).unwrap();
            assert_eq!(values["仕事内容"], body);
            assert_eq!(values["基本給与 最小"], "");
            let req = ManualDraftRequest {
                operation_id: uuid::Uuid::new_v4().to_string(),
                base_revision: drafts::revision(&properties),
                base_body_hash: drafts::hash(body),
                base_draft_id: None,
                row: values,
                reason: String::new(),
            };
            assert_eq!(
                snapshot(&req, &properties, None).unwrap().row["仕事内容"],
                body
            );
        }
        assert_eq!(
            row("訪問看護師募集。時給1800円。\n勤務時間：9〜18時")["仕事内容"],
            "訪問看護師募集。時給1800円。\n勤務時間：9〜18時"
        );
    }
    #[test]
    fn free_text_changes_are_not_classified_by_meaning() {
        for (key, before, after) in [
            ("案件名", "土日休みの配送", "日曜休みの配送"),
            ("案件名", "東京都の配送", "大阪府の配送"),
            ("案件名", "月給25万円の配送", "月給30万円の配送"),
            ("自由項目1の内容", "土日休み", "日曜休み"),
        ] {
            let (mut req, properties) = fixture();
            let mut base = snapshot(&req, &properties, None).unwrap();
            base.row.insert(key.into(), before.into());
            req.base_body_hash = drafts::hash(&drafts::compose_body(&base.row));
            req.row.insert(key.into(), after.into());
            let result = snapshot(&req, &properties, Some(&base)).unwrap();
            assert_eq!(result.row[key], after);
            assert!(result.manual_edit.unwrap().changed_conditions.is_empty());
        }
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
    fn every_protected_column_requires_reason_for_one_character_changes() {
        for key in PROTECTED_COLUMNS
            .iter()
            .filter(|key| key.as_str() != "店舗id")
        {
            assert!(HRHACKER_COLUMNS.contains(&key.as_str()));
            let (mut req, properties) = fixture();
            let value = format!("{} ", req.row[key]);
            req.row.insert(key.clone(), value.clone());
            assert_eq!(
                snapshot(&req, &properties, None).unwrap_err(),
                "draft_reason_required"
            );
            req.reason = "担当者が変更後の募集条件を確認したため".into();
            let saved = snapshot(&req, &properties, None).unwrap();
            assert_eq!(saved.row[key], value);
            assert_eq!(saved.review_status, drafts::DraftStatus::Pending);
            assert_eq!(saved.source_kind, DraftSource::FreeText);
            assert!(saved.manual_edit.unwrap().changed_conditions.contains(key));
        }
    }
    #[test]
    fn original_whitespace_is_preserved_and_removal_requires_reason() {
        for key in PROTECTED_COLUMNS
            .iter()
            .filter(|key| key.as_str() != "店舗id")
        {
            for suffix in [" ", "　", "\t", "\n", "\r\n"] {
                let (mut req, mut properties) = fixture();
                let original = format!("架空の条件{suffix}");
                req.row.insert(key.clone(), original.clone());
                let body = drafts::compose_body(&req.row);
                properties.insert("hrh_kyuujinhyou_honbun".into(), Some(body.clone()));
                req.base_body_hash = drafts::hash(&body);
                assert_eq!(current_row(&properties).unwrap()[key], original);
                assert_eq!(
                    snapshot(&req, &properties, None).unwrap().row[key],
                    original
                );
                req.row.insert(key.clone(), "架空の条件".into());
                assert_eq!(
                    snapshot(&req, &properties, None).unwrap_err(),
                    "draft_reason_required"
                );
                req.reason = "空白の削除を確認したため".into();
                let saved = snapshot(&req, &properties, None).unwrap();
                assert_eq!(saved.row[key], "架空の条件");
                assert!(saved.manual_edit.unwrap().changed_conditions.contains(key));
            }
        }
        let values: BTreeMap<String, String> = HRHACKER_COLUMNS
            .iter()
            .map(|key| ((*key).to_owned(), " \t先頭\r\n途中\n末尾　\n".into()))
            .collect();
        assert_eq!(row(&drafts::compose_body(&values)), values);
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
