//! HubSpot の定義 (プロパティ・パイプライン) の読み取り API (`GET /api/crm/metadata`)。
//!
//! 顧客の値は返さない。HubSpot への通信・トークン・リトライは `crate::hubspot::HubSpotClient`
//! (`AppState.hubspot`) を共有し、このファイルは「どの定義を取り、どう整形し、60 秒キャッシュするか」だけを持つ。
//! 認可は `crate::crm::rbac` (レコード読み取りと同じ基準)。ルート登録は `crate::crm::router`。
//!
//! 上流 (HubSpot) のエラー本文はブラウザに返さない。返すのは `error_kind` と固定文言だけ。
use crate::hubspot::{HubSpotClient, HubSpotError, RecordType};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use ts_rs::TS;

/// 定義キャッシュの有効期間
pub const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct CrmPropertyOption {
    #[serde(default, deserialize_with = "null_to_default")]
    pub label: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub value: String,
    #[serde(default, deserialize_with = "null_to_default")]
    pub hidden: bool,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPropertyDefinition {
    pub object_type: String,
    pub name: String,
    pub label: String,
    pub property_type: String,
    pub field_type: String,
    pub options: Vec<CrmPropertyOption>,
}
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct CrmStage {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPipeline {
    pub id: String,
    pub label: String,
    pub stages: Vec<CrmStage>,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmMetadataResponse {
    pub properties: Vec<CrmPropertyDefinition>,
    pub pipelines: Vec<CrmPipeline>,
    pub fetched_at: String,
    pub hubspot_ms: f64,
    pub total_ms: f64,
    pub cache_hit: bool,
}

/// HubSpot は欠落だけでなく `null` も返しうるので、null も既定値に倒す。
fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubProperty {
    name: String,
    #[serde(default, deserialize_with = "null_to_default")]
    label: String,
    #[serde(default, rename = "type", deserialize_with = "null_to_default")]
    property_type: String,
    #[serde(default, deserialize_with = "null_to_default")]
    field_type: String,
    #[serde(default, deserialize_with = "null_to_default")]
    options: Vec<CrmPropertyOption>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubStage {
    id: String,
    #[serde(default, deserialize_with = "null_to_default")]
    label: String,
    #[serde(default, deserialize_with = "null_to_default")]
    display_order: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubPipeline {
    id: String,
    #[serde(default, deserialize_with = "null_to_default")]
    label: String,
    #[serde(default, deserialize_with = "null_to_default")]
    display_order: i64,
    #[serde(default, deserialize_with = "null_to_default")]
    stages: Vec<HubStage>,
}

/// 定義の取得で 404 になるのは「レコードが無い」ではなく上流の不具合 (エンドポイントが無い)。
/// ブラウザには `not_found` ではなく `hubspot_upstream` として見せる。
fn definitions_error(e: HubSpotError) -> HubSpotError {
    match e {
        HubSpotError::NotFound => HubSpotError::Upstream { status: 404 },
        other => other,
    }
}

fn decode_error(what: &str) -> HubSpotError {
    HubSpotError::Decode(format!("{what} is not in the expected shape"))
}

/// `results` 配列を取り出す。無い・配列でないなら Decode エラー。
fn results_array(v: &Value, what: &str) -> Result<Vec<Value>, HubSpotError> {
    v.get("results")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| decode_error(what))
}

/// 定義の応答から、確認済みの項目 (`allowed_property`) だけを取り出して整形する。
/// 許可外の項目は中身を見ない (形が想定外でも全体を壊さない)。
fn parse_properties(
    object: RecordType,
    v: &Value,
) -> Result<Vec<CrmPropertyDefinition>, HubSpotError> {
    let api_name = object.api_name();
    let mut out = Vec::new();
    for item in results_array(v, "property definitions")? {
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        if !allowed_property(api_name, name) {
            continue;
        }
        let p: HubProperty = serde_json::from_value(item.clone())
            .map_err(|_| decode_error("property definition"))?;
        out.push(CrmPropertyDefinition {
            object_type: api_name.to_owned(),
            name: p.name,
            label: p.label,
            property_type: p.property_type,
            field_type: p.field_type,
            options: p.options,
        });
    }
    Ok(out)
}

pub(crate) fn parse_pipelines(v: &Value) -> Result<Vec<CrmPipeline>, HubSpotError> {
    let mut pipelines: Vec<HubPipeline> = Vec::new();
    for item in results_array(v, "pipelines")? {
        pipelines
            .push(serde_json::from_value(item).map_err(|_| decode_error("pipeline definition"))?);
    }
    pipelines.sort_by_key(|p| p.display_order);
    Ok(pipelines
        .into_iter()
        .map(|mut p| {
            p.stages.sort_by_key(|s| s.display_order);
            CrmPipeline {
                id: p.id,
                label: p.label,
                stages: p
                    .stages
                    .into_iter()
                    .map(|s| CrmStage {
                        id: s.id,
                        label: s.label,
                    })
                    .collect(),
            }
        })
        .collect())
}

/// 定義のキャッシュ (プロセス内メモリのみ、60 秒)。複数タブの同時ミスは 1 回の取得にまとめる。
#[derive(Default)]
pub struct MetadataCache {
    slot: Mutex<Option<(Instant, CrmMetadataResponse)>>,
    /// `refresh` を受け付ける最短間隔 (取得から。これより新しいキャッシュは refresh でも返す)。
    /// HubSpot の鍵は既存バッチと共有なので、連打で定義 4 本を何度も取り直さない。
    refresh_floor: Duration,
}

impl MetadataCache {
    pub fn with_refresh_floor(refresh_floor: Duration) -> Self {
        Self {
            slot: Mutex::new(None),
            refresh_floor,
        }
    }

    /// 定義を返す。`refresh` ならキャッシュを使わず取り直す。失敗はキャッシュしない。
    pub async fn get(
        &self,
        client: &HubSpotClient,
        refresh: bool,
    ) -> Result<CrmMetadataResponse, HubSpotError> {
        let started = Instant::now();
        // 同時のキャッシュミスを直列化する (重複した上流呼び出しを避ける)
        let mut slot = self.slot.lock().await;
        if let Some((stored, response)) = slot.as_ref() {
            let age = stored.elapsed();
            if (!refresh && age < CACHE_TTL) || (refresh && age < self.refresh_floor) {
                let mut response = response.clone();
                response.cache_hit = true;
                response.hubspot_ms = 0.0;
                response.total_ms = started.elapsed().as_secs_f64() * 1000.0;
                return Ok(response);
            }
        }
        let upstream = Instant::now();
        let (contacts, companies, deals, pipelines) = tokio::try_join!(
            client.property_definitions(RecordType::Contact),
            client.property_definitions(RecordType::Company),
            client.property_definitions(RecordType::Deal),
            client.deal_pipelines(),
        )
        .map_err(definitions_error)?;
        let hubspot_ms = upstream.elapsed().as_secs_f64() * 1000.0;
        let mut properties = parse_properties(RecordType::Contact, &contacts)?;
        properties.extend(parse_properties(RecordType::Company, &companies)?);
        properties.extend(parse_properties(RecordType::Deal, &deals)?);
        let response = CrmMetadataResponse {
            properties,
            pipelines: parse_pipelines(&pipelines)?,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            hubspot_ms,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
            cache_hit: false,
        };
        *slot = Some((Instant::now(), response.clone()));
        Ok(response)
    }
}

/// 現在の MOC で確認済みの項目だけを返す (アカウントの全定義は出さない)。
fn allowed_property(object_type: &str, name: &str) -> bool {
    let names: &[&str] = match object_type {
        "contacts" => &[
            "firstname",
            "lastname",
            "email",
            "phone",
            "jobtitle",
            "hubspot_owner_id",
            "hs_lead_status",
            "lifecyclestage",
            "notes_last_contacted",
        ],
        "companies" => &[
            "name",
            "domain",
            "phone",
            "industry",
            "city",
            "numberofemployees",
            "hubspot_owner_id",
            "lifecyclestage",
        ],
        "deals" => &[
            "pipeline",
            "dealstage",
            "bpo_10",
            "bpo_14",
            "bpo_13",
            "bpo_16",
            "bpo_40",
            "bpo_42",
            "bpo_45",
            "bpo_21",
            "bpo_22",
            "bpo_50",
            "bpo_24",
            "bpo_49",
            "bpo_34",
            "bpo_25",
            "bpo_8",
            "bpo_23",
            "bpo__",
            "bpo_33",
            "bpo_3",
            "bpo_4",
            // 架電結果の入力欄の「その他理由」(bpo_10 = その他 のとき)。文字列の項目
            "bpo_57",
            "bpo_18",
            "bpo_19",
            "bpo_32",
        ],
        _ => &[],
    };
    names.contains(&name)
}

#[cfg(test)]
mod tests;
