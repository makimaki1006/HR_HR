//! 応募の編集可能な範囲。送信・台帳・再送は既存 write の経路を使う。
use super::*;
use crate::crm::{
    pending::Step,
    property_catalog::{self, CrmCatalogProperty},
    write::{self, CrmDealPatchRequest},
};
use std::collections::BTreeMap;

pub const WRITE_SCOPE_MESSAGE: &str =
    "HubSpot の鍵に応募を書き込む権限がありません。何も保存されていません。";
const RESULT: &[&str] = &[
    "taioushubetsu",
    "ichijitaioujisshinaiyou",
    "renrakukakuninmokuteki",
    "yakusokukadennichiji",
    "ichijitaiou_baitai_message_nichiji",
    "ouboshahenobaitaimerusoufu",
    "senkoushoruikaishuu",
    "ouboshajouhoukanrishito",
    "senkoushoruiteishutsukigenbi",
    "ashigiriketteibi",
    "bpotaishougairiyuu",
];
fn allowed(name: &str) -> bool {
    RESULT.contains(&name)
        || (HEARING.contains(&name)
            && ![
                "shuugyoujoukyou",
                "nyuushakiboujiki",
                "kokyakushiitotenkijoukyou",
                "bposagyousuteetasu",
            ]
            .contains(&name))
        || [
            "meeruadoresu",
            "denwabangou",
            "mensetsukanounichiji",
            "renrakukanouyoubijikantai",
        ]
        .contains(&name)
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantEditField {
    pub name: String,
    pub label: String,
    pub property_type: String,
    pub field_type: String,
    pub options: Vec<crate::handlers::crm_metadata::CrmPropertyOption>,
    pub section: String,
}
pub(super) fn editable(defs: &Value) -> Vec<ApplicantEditField> {
    values(&defs["properties"])
        .filter_map(|p| {
            let (name, label, pt, ft) = (
                text(p, "name"),
                text(p, "label"),
                text(p, "type"),
                text(p, "fieldType"),
            );
            if !allowed(&name)
                || label.is_empty()
                || !property_catalog::is_writable(p, &name, &label, &pt, &ft)
                || p["hidden"] == true
                || p["archived"] == true
                || p["dataSensitivity"]
                    .as_str()
                    .is_some_and(|s| s != "non_sensitive")
            {
                return None;
            }
            Some(ApplicantEditField {
                section: if RESULT.contains(&name.as_str()) {
                    "結果"
                } else if ["meeruadoresu", "denwabangou"].contains(&name.as_str()) {
                    "連絡先"
                } else {
                    "ヒアリング"
                }
                .into(),
                name,
                label,
                property_type: pt,
                field_type: ft,
                options: values(&p["options"])
                    .filter(|o| o["hidden"] != true)
                    .map(|o| crate::handlers::crm_metadata::CrmPropertyOption {
                        hidden: false,
                        label: text(o, "label"),
                        value: text(o, "value"),
                    })
                    .collect(),
            })
        })
        .collect()
}
pub(super) fn write_pipeline(defs: &Value) -> Option<&Value> {
    values(&defs["pipelines"]).find(|p| p["id"] == PIPELINE)
}
pub(in crate::crm) async fn build_write_step(
    state: &ApplicantsState,
    client: &HubSpotClient,
    id: &str,
    req: &CrmDealPatchRequest,
) -> Result<Step, Response> {
    let invalid = || error_json(StatusCode::BAD_REQUEST, "invalid_applicant_patch");
    if req.objects.is_some()
        || req.set.len() > write::MAX_SET_PROPS
        || req.base.len() > write::MAX_SET_PROPS
    {
        return Err(invalid());
    }
    let defs = state
        .definitions(client)
        .await
        .map_err(|e| applicant_error(&e))?;
    let pipeline = write_pipeline(&defs).ok_or_else(invalid)?;
    let pipeline_id = text(pipeline, "id");
    if req.base.get("hs_pipeline").and_then(|v| v.as_deref()) != Some(&pipeline_id) {
        return Err(invalid());
    }
    let fields = editable(&defs);
    let mut set = BTreeMap::new();
    let mut types = BTreeMap::new();
    let mut base = BTreeMap::from([("hs_pipeline".into(), Some(pipeline_id.clone()))]);
    for (name, value) in &req.set {
        let def = fields
            .iter()
            .find(|p| &p.name == name)
            .ok_or_else(invalid)?;
        let catalog = CrmCatalogProperty {
            name: name.clone(),
            label: def.label.clone(),
            label_en: None,
            property_type: def.property_type.clone(),
            field_type: def.field_type.clone(),
            options: def.options.clone(),
            owner_ref: false,
            writable: true,
        };
        write::validate_value(&catalog, value).map_err(|_| invalid())?;
        let old = req.base.get(name).ok_or_else(invalid)?;
        if write::norm(old.as_deref(), &def.property_type)
            != write::norm(value.as_deref(), &def.property_type)
        {
            set.insert(name.clone(), value.clone());
            base.insert(name.clone(), old.clone());
            types.insert(name.clone(), def.property_type.clone());
        }
    }
    if let Some(stage) = &req.stage {
        if stage.pipeline_id != pipeline_id
            || !values(&pipeline["stages"]).any(|s| s["id"] == stage.stage_id)
        {
            return Err(invalid());
        }
        let old = req.base.get("hs_pipeline_stage").ok_or_else(invalid)?;
        if old.as_deref() != Some(&stage.stage_id) {
            set.insert("hs_pipeline_stage".into(), Some(stage.stage_id.clone()));
            base.insert("hs_pipeline_stage".into(), old.clone());
        }
    }
    if set.is_empty() {
        return Err(invalid());
    }
    Ok(Step {
        object: OBJECT.into(),
        id: id.into(),
        base,
        set,
        types,
        stage: None,
        done: false,
    })
}
