//! `GET /api/crm/property-catalog` (架電画面の「プロパティ」パネルで選べる項目の一覧。定義だけで顧客の値は返さない)。
//!
//! 案件・担当者・会社のプロパティを、HubSpot の画面と同じグループ (見出し) ごとに返す。
//! 架電画面はこの一覧から表示する項目を選び、選んだ名前を `GET /api/crm/workspace/deals/{id}` に渡す。
//! 渡された名前はここの一覧 (許可リスト) で確かめる ([`CatalogEntry::unknown`])。
//!
//! ## HubSpot 呼び出し回数 (キャッシュの有効期間 [`CATALOG_TTL`] あたり)
//! 型 (案件・担当者・会社) ごとに プロパティ定義 1 回 + グループ定義 1 回 = 計 6 回。
//! グループの表示名はプロパティ定義の応答に入っていない (グループの内部名だけ) ため、グループ定義も読む。
//! 定義は滅多に変わらないので数時間使い回す。同時のキャッシュミスは 1 回の取得にまとめる。
//! 失敗は [`FAILURE_TTL`] だけ覚えて同じ失敗を返す (詳細を開くたびに 6 回ずつ失敗する呼び出しを繰り返さない)。
//!
//! 返さないもの: HubSpot で非表示 (`hidden`) の項目と選択肢、アーカイブ済みの項目、
//! 機微情報として印の付いた項目 (`dataSensitivity` が `non_sensitive` 以外)。
//! 上流 (HubSpot) のエラー本文はブラウザに返さない (`error_kind` と固定文言だけ)。

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;
use tower_sessions::Session;
use ts_rs::TS;

use super::rbac;
use super::routes::{
    error_json, hubspot_error_response, timeout_response, CrmCtx, CRM_REQUEST_DEADLINE,
};
use crate::handlers::crm_metadata::CrmPropertyOption;
use crate::hubspot::gateway::{cache_hit, cache_miss};
use crate::hubspot::{HubSpotClient, HubSpotError, RecordType};
use crate::AppState;

/// 定義の一覧を使い回す時間
pub const CATALOG_TTL: Duration = Duration::from_secs(6 * 60 * 60);
/// 取得に失敗したとき、取り直さずに同じ失敗を返す時間
pub const FAILURE_TTL: Duration = Duration::from_secs(60);
/// 1 つの型 (案件・担当者・会社) で、一度に表示できる項目の最大数 (workspace の `*_props` も同じ上限)
pub const MAX_SELECTED_PER_OBJECT: usize = 100;
/// HubSpot の内部名の最大長 (実データの最長は 64 文字。余裕を持たせる)
const MAX_PROPERTY_NAME_LEN: usize = 100;
/// グループの定義に無いグループ名の項目をまとめる見出し
const OTHER_GROUP_LABEL: &str = "その他の項目";

/// 項目の定義 1 件 (表示名・種類・選択肢)
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmCatalogProperty {
    /// HubSpot の内部名 (画面には出さない。値の取得と保存に使う)
    pub name: String,
    pub label: String,
    /// `string` / `number` / `date` / `datetime` / `enumeration` / `bool` 等 (HubSpot の `type`)
    pub property_type: String,
    /// `text` / `textarea` / `select` / `radio` / `checkbox` / `booleancheckbox` / `phonenumber` 等 (HubSpot の `fieldType`)
    pub field_type: String,
    /// 選択肢 (非表示の選択肢は除く)
    pub options: Vec<CrmPropertyOption>,
}

/// HubSpot のプロパティのグループ (画面の見出し) 1 つ
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPropertyGroup {
    /// グループの内部名 (画面には出さない)
    pub name: String,
    pub label: String,
    /// HubSpot の表示順
    pub properties: Vec<CrmCatalogProperty>,
}

/// 1 つの型 (`deals` / `contacts` / `companies`) の項目の一覧
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmCatalogObject {
    pub object_type: String,
    /// HubSpot の表示順。項目の無いグループは含めない
    pub groups: Vec<CrmPropertyGroup>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPropertyCatalogResponse {
    /// 案件 → 担当者 → 会社 の順
    pub objects: Vec<CrmCatalogObject>,
    /// 1 つの型で一度に表示できる項目の最大数
    pub max_selected_per_object: u32,
    pub fetched_at: String,
    pub cache_hit: bool,
}

/// 一覧と、許可リストとして引く名前の集合
pub struct CatalogEntry {
    pub response: CrmPropertyCatalogResponse,
    names: BTreeMap<&'static str, HashSet<String>>,
}

impl CatalogEntry {
    fn new(response: CrmPropertyCatalogResponse) -> Self {
        let mut names: BTreeMap<&'static str, HashSet<String>> = BTreeMap::new();
        for o in &response.objects {
            let key = match o.object_type.as_str() {
                "deals" => "deals",
                "contacts" => "contacts",
                "companies" => "companies",
                _ => continue,
            };
            let set = names.entry(key).or_default();
            for g in &o.groups {
                set.extend(g.properties.iter().map(|p| p.name.clone()));
            }
        }
        Self { response, names }
    }

    /// `object` (`deals` / `contacts` / `companies`) の一覧に無い名前
    pub fn unknown<'a>(&self, object: &str, wanted: &'a [String]) -> Vec<&'a str> {
        let set = self.names.get(object);
        wanted
            .iter()
            .filter(|n| !set.is_some_and(|s| s.contains(n.as_str())))
            .map(String::as_str)
            .collect()
    }
}

/// 定義の一覧のキャッシュ (プロセス内メモリのみ)
pub struct PropertyCatalogCache {
    slot: Mutex<Slot>,
    ttl: Duration,
    failure_ttl: Duration,
    /// 先読み (背景の優先度) が走っているか
    refreshing: super::call_queue::RefreshGate,
}

#[derive(Default)]
struct Slot {
    ok: Option<(Instant, Arc<CatalogEntry>)>,
    failed: Option<(Instant, HubSpotError)>,
}

impl Default for PropertyCatalogCache {
    fn default() -> Self {
        Self::with_ttl(CATALOG_TTL, FAILURE_TTL)
    }
}

impl PropertyCatalogCache {
    pub fn with_ttl(ttl: Duration, failure_ttl: Duration) -> Self {
        Self {
            slot: Mutex::new(Slot::default()),
            ttl,
            failure_ttl,
            refreshing: super::call_queue::RefreshGate::new(),
        }
    }

    /// 一覧を返す (2 つ目は キャッシュから返したか)。失敗は `failure_ttl` の間だけ同じ失敗を返す
    pub async fn get(
        &self,
        client: &HubSpotClient,
    ) -> Result<(Arc<CatalogEntry>, bool), HubSpotError> {
        let mut slot = self.slot.lock().await;
        if let Some((stored, entry)) = slot.ok.as_ref() {
            if stored.elapsed() < self.ttl {
                cache_hit("property_catalog");
                return Ok((entry.clone(), true));
            }
        }
        if let Some((at, e)) = slot.failed.as_ref() {
            if at.elapsed() < self.failure_ttl {
                return Err(e.clone());
            }
        }
        cache_miss("property_catalog");
        match self.fetch(client).await {
            Ok(entry) => {
                slot.ok = Some((Instant::now(), entry.clone()));
                slot.failed = None;
                Ok((entry, false))
            }
            Err(e) => {
                slot.failed = Some((Instant::now(), e.clone()));
                Err(e)
            }
        }
    }

    /// 有効期間の終わり近く (残り 20% 未満) なら true (先読みの対象)。取得中は false
    pub fn refresh_due(&self) -> bool {
        self.slot.try_lock().is_ok_and(|slot| {
            slot.ok
                .as_ref()
                .is_some_and(|(at, _)| super::call_queue::refresh_due(at.elapsed(), self.ttl))
        })
    }

    /// 一覧を読み直して置き換える (先読み。背景の優先度のクライアントを渡す)。読んでいる間はロックを持たない。
    /// 失敗しても今の一覧は残す
    pub async fn refresh(&self, client: &HubSpotClient) {
        if !self.refreshing.try_begin() {
            return;
        }
        match self.fetch(client).await {
            Ok(entry) => {
                let mut slot = self.slot.lock().await;
                slot.ok = Some((Instant::now(), entry));
                slot.failed = None;
            }
            Err(e) => tracing::warn!(
                error_kind = e.error_kind(),
                "crm property catalog refresh-ahead failed"
            ),
        }
        self.refreshing.end();
    }

    async fn fetch(&self, client: &HubSpotClient) -> Result<Arc<CatalogEntry>, HubSpotError> {
        let (dp, cp, op, dg, cg, og) = tokio::try_join!(
            client.property_definitions(RecordType::Deal),
            client.property_definitions(RecordType::Contact),
            client.property_definitions(RecordType::Company),
            client.property_groups(RecordType::Deal),
            client.property_groups(RecordType::Contact),
            client.property_groups(RecordType::Company),
        )
        .map_err(|e| match e {
            // 定義の取得で 404 は「レコードが無い」ではなく上流の不具合
            HubSpotError::NotFound => HubSpotError::Upstream { status: 404 },
            other => other,
        })?;
        let response = CrmPropertyCatalogResponse {
            objects: vec![
                build_object(RecordType::Deal, &dp, &dg)?,
                build_object(RecordType::Contact, &cp, &cg)?,
                build_object(RecordType::Company, &op, &og)?,
            ],
            max_selected_per_object: MAX_SELECTED_PER_OBJECT as u32,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            cache_hit: false,
        };
        Ok(Arc::new(CatalogEntry::new(response)))
    }
}

fn decode_error(what: &str) -> HubSpotError {
    HubSpotError::Decode(format!("{what} is not in the expected shape"))
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn order_of(v: &Value) -> i64 {
    // HubSpot は並び順の無い項目に -1 を返す。順のあるものより後ろに並べる
    match v.get("displayOrder").and_then(Value::as_i64) {
        Some(n) if n >= 0 => n,
        _ => i64::MAX,
    }
}

fn is_true(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// 組み立て途中のグループ (表示順, 表示名, (表示順, 項目))
type GroupAcc = (i64, String, Vec<(i64, CrmCatalogProperty)>);

/// プロパティ定義とグループ定義の応答から、1 つの型の一覧を組み立てる
pub(crate) fn build_object(
    object: RecordType,
    properties: &Value,
    groups: &Value,
) -> Result<CrmCatalogObject, HubSpotError> {
    let props = properties
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| decode_error("property definitions"))?;
    let group_defs = groups
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| decode_error("property groups"))?;
    // グループ: 内部名 → (表示順, 表示名, 項目)
    let mut by_group: BTreeMap<String, GroupAcc> = BTreeMap::new();
    for g in group_defs {
        let name = str_field(g, "name");
        if name.is_empty() || is_true(g, "archived") {
            continue;
        }
        let label = match str_field(g, "label").trim() {
            "" => name.to_string(),
            l => l.to_string(),
        };
        by_group.insert(name.to_string(), (order_of(g), label, Vec::new()));
    }
    for p in props {
        let name = str_field(p, "name");
        if name.is_empty()
            || !valid_property_name(name)
            || is_true(p, "hidden")
            || is_true(p, "archived")
        {
            continue;
        }
        // 機微情報の印が付いた項目は出さない (無い・non_sensitive だけ)
        if let Some(s) = p.get("dataSensitivity").and_then(Value::as_str) {
            if s != "non_sensitive" {
                continue;
            }
        }
        let options: Vec<CrmPropertyOption> = p
            .get("options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|o| !is_true(o, "hidden"))
                    .map(|o| CrmPropertyOption {
                        label: str_field(o, "label").to_string(),
                        value: str_field(o, "value").to_string(),
                        hidden: false,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let label = match str_field(p, "label").trim() {
            "" => name.to_string(),
            l => l.to_string(),
        };
        let prop = CrmCatalogProperty {
            name: name.to_string(),
            label,
            property_type: str_field(p, "type").to_string(),
            field_type: str_field(p, "fieldType").to_string(),
            options,
        };
        // 定義に無いグループ名の項目は「その他の項目」にまとめる (内部名を見出しにしない)
        let group = str_field(p, "groupName");
        let key = if by_group.contains_key(group) {
            group.to_string()
        } else {
            String::new()
        };
        by_group
            .entry(key)
            .or_insert_with(|| (i64::MAX, OTHER_GROUP_LABEL.to_string(), Vec::new()))
            .2
            .push((order_of(p), prop));
    }
    let mut groups_out: Vec<(i64, String, CrmPropertyGroup)> = by_group
        .into_iter()
        .filter(|(_, (_, _, ps))| !ps.is_empty())
        .map(|(name, (order, label, mut ps))| {
            ps.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
            let g = CrmPropertyGroup {
                name,
                label: label.clone(),
                properties: ps.into_iter().map(|(_, p)| p).collect(),
            };
            (order, label, g)
        })
        .collect();
    groups_out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(CrmCatalogObject {
        object_type: object.api_name().to_string(),
        groups: groups_out.into_iter().map(|(_, _, g)| g).collect(),
    })
}

/// HubSpot の内部名として受け付ける形 (英数字と `_`、1〜100 文字)
pub fn valid_property_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_PROPERTY_NAME_LEN
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `a,b,c` を名前の並びにする (空白を除き、空は捨て、重複は 1 つに)。
/// 形が不正・上限超えなら None (HubSpot を呼ばずに 400 にする)
pub fn parse_selected(raw: Option<&str>) -> Option<Vec<String>> {
    let Some(raw) = raw else {
        return Some(Vec::new());
    };
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',') {
        let n = part.trim();
        if n.is_empty() {
            continue;
        }
        if !valid_property_name(n) {
            return None;
        }
        if !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    }
    (out.len() <= MAX_SELECTED_PER_OBJECT).then_some(out)
}

/// `GET /api/crm/property-catalog`。CRM の利用者だけ (認可は HubSpot の設定有無より先)
pub(super) async fn get_property_catalog(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    if let Err(denied) = rbac::authorize(&session, &state, &ctx.access, None).await {
        return denied.into_response();
    }
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.catalog.get(&client)).await {
        Err(_elapsed) => {
            tracing::warn!(error_kind = "crm_timeout", "crm property catalog timed out");
            timeout_response()
        }
        Ok(Ok((entry, hit))) => {
            super::routes::refresh_ahead(&ctx, &client);
            let mut body = entry.response.clone();
            body.cache_hit = hit;
            Json(body).into_response()
        }
        Ok(Err(e)) => {
            tracing::warn!(
                error_kind = e.error_kind(),
                "crm property catalog read failed"
            );
            hubspot_error_response(&e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn グループの表示名と順で並べ_非表示_機微_アーカイブは除く() {
        let props = json!({"results": [
            {"name": "bpo_32", "label": "URL_求人検索", "type": "string", "fieldType": "text", "groupName": "dealinformation", "displayOrder": 2},
            {"name": "bpo_10", "label": "不通時チェック", "type": "enumeration", "fieldType": "radio", "groupName": "dealinformation", "displayOrder": 1,
             "options": [{"label": "受付拒否", "value": "a", "hidden": false}, {"label": "古い", "value": "z", "hidden": true}]},
            {"name": "secret", "label": "隠し", "type": "string", "fieldType": "text", "groupName": "dealinformation", "hidden": true},
            {"name": "sensitive", "label": "機微", "type": "string", "fieldType": "text", "groupName": "dealinformation", "dataSensitivity": "sensitive"},
            {"name": "old", "label": "古い項目", "type": "string", "fieldType": "text", "groupName": "dealinformation", "archived": true},
            {"name": "amount", "label": "金額", "type": "number", "fieldType": "number", "groupName": "deal_revenue", "displayOrder": -1},
            {"name": "orphan", "label": "迷子", "type": "string", "fieldType": "text", "groupName": "unknown_group"},
            {"name": "bad name", "label": "不正な名前", "type": "string", "fieldType": "text", "groupName": "deal_revenue"}
        ]});
        let groups = json!({"results": [
            {"name": "deal_revenue", "label": "Deal revenue", "displayOrder": 1},
            {"name": "dealinformation", "label": "Deal information", "displayOrder": 0},
            {"name": "empty", "label": "空のグループ", "displayOrder": 2}
        ]});
        let o = build_object(RecordType::Deal, &props, &groups).unwrap();
        assert_eq!(o.object_type, "deals");
        let shape: Vec<(String, Vec<String>)> = o
            .groups
            .iter()
            .map(|g| {
                (
                    g.label.clone(),
                    g.properties.iter().map(|p| p.name.clone()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    "Deal information".to_string(),
                    vec!["bpo_10".to_string(), "bpo_32".to_string()]
                ),
                ("Deal revenue".to_string(), vec!["amount".to_string()]),
                (OTHER_GROUP_LABEL.to_string(), vec!["orphan".to_string()]),
            ]
        );
        let bpo10 = &o.groups[0].properties[0];
        assert_eq!(bpo10.label, "不通時チェック");
        assert_eq!(bpo10.property_type, "enumeration");
        assert_eq!(bpo10.options.len(), 1, "非表示の選択肢は除く");
        assert_eq!(bpo10.options[0].label, "受付拒否");
    }

    #[test]
    fn 応答の形が違えば_decode_エラー() {
        assert!(build_object(RecordType::Deal, &json!({}), &json!({"results": []})).is_err());
        assert!(build_object(RecordType::Deal, &json!({"results": []}), &json!([])).is_err());
    }

    #[test]
    fn 選んだ項目の名前は英数字と下線だけ_100_件まで_重複は_1_つ() {
        assert_eq!(parse_selected(None), Some(vec![]));
        assert_eq!(parse_selected(Some("")), Some(vec![]));
        assert_eq!(
            parse_selected(Some(" bpo_10, bpo_32,,bpo_10 ")),
            Some(vec!["bpo_10".to_string(), "bpo_32".to_string()])
        );
        assert_eq!(parse_selected(Some("bpo-10")), None);
        assert_eq!(parse_selected(Some("a b")), None);
        assert_eq!(parse_selected(Some("../x")), None);
        assert_eq!(parse_selected(Some(&"a".repeat(101))), None);
        let hundred: Vec<String> = (0..100).map(|i| format!("p{i}")).collect();
        assert_eq!(parse_selected(Some(&hundred.join(","))).unwrap().len(), 100);
        let over: Vec<String> = (0..101).map(|i| format!("p{i}")).collect();
        assert_eq!(parse_selected(Some(&over.join(","))), None);
    }

    #[test]
    fn 一覧に無い名前を返す() {
        let props = json!({"results": [
            {"name": "bpo_10", "label": "不通時チェック", "type": "enumeration", "fieldType": "radio", "groupName": "g"}
        ]});
        let groups = json!({"results": [{"name": "g", "label": "G", "displayOrder": 0}]});
        let entry = CatalogEntry::new(CrmPropertyCatalogResponse {
            objects: vec![build_object(RecordType::Deal, &props, &groups).unwrap()],
            max_selected_per_object: 100,
            fetched_at: String::new(),
            cache_hit: false,
        });
        let wanted = vec!["bpo_10".to_string(), "nope".to_string()];
        assert_eq!(entry.unknown("deals", &wanted), vec!["nope"]);
        assert_eq!(entry.unknown("contacts", &wanted), vec!["bpo_10", "nope"]);
    }
}
