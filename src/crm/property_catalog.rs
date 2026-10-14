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
//! **前に取れた一覧が残っているなら、期限が切れていても取り直しに失敗した間はそれを返す** (stale-while-error。
//! 保存の許可リストにも使うので、HubSpot の一時障害のたびに保存が 502 になるのを避ける)。
//! 一度も取れていないときだけ失敗を返す。
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

use super::clock::Clock;
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
/// 取得に失敗したとき、取り直さずに待つ時間 (短く: 障害が終わったらすぐ取り直す。
/// 一度も取れていないときは同じ失敗を、前の一覧があるときはそれを、この間返す)
pub const FAILURE_TTL: Duration = Duration::from_secs(5);
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
    /// 日本語に置き換えた項目の元の表示名 (英語)。検索で英語名でも見つけるために添える。訳していない項目には無い
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label_en: Option<String>,
    /// `string` / `number` / `date` / `datetime` / `enumeration` / `bool` 等 (HubSpot の `type`)
    pub property_type: String,
    /// `text` / `textarea` / `select` / `radio` / `checkbox` / `booleancheckbox` / `phonenumber` 等 (HubSpot の `fieldType`)
    pub field_type: String,
    /// 選択肢 (非表示の選択肢は除く)
    pub options: Vec<CrmPropertyOption>,
    /// 値が HubSpot の担当者 (owner) の ID である項目 (`referencedObjectType` が `OWNER` の選択式。選択肢は `options` に無い)。
    /// 画面は担当者の一覧から選ばせ、書き込みは実在する owner ID かを確かめる
    pub owner_ref: bool,
    /// HubSpot の API で書き込める項目か (読み取り専用・計算・システム項目などは false)。
    /// 書き込み API (`crm::write`) の許可リストに使う。応答 JSON には出さない
    #[serde(skip)]
    #[ts(skip)]
    pub writable: bool,
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
    defs: BTreeMap<&'static str, std::collections::HashMap<String, CrmCatalogProperty>>,
}

impl CatalogEntry {
    fn new(response: CrmPropertyCatalogResponse) -> Self {
        let mut names: BTreeMap<&'static str, HashSet<String>> = BTreeMap::new();
        let mut defs: BTreeMap<
            &'static str,
            std::collections::HashMap<String, CrmCatalogProperty>,
        > = BTreeMap::new();
        for o in &response.objects {
            let key = match o.object_type.as_str() {
                "deals" => "deals",
                "contacts" => "contacts",
                "companies" => "companies",
                _ => continue,
            };
            let set = names.entry(key).or_default();
            let d = defs.entry(key).or_default();
            for g in &o.groups {
                set.extend(g.properties.iter().map(|p| p.name.clone()));
                for p in &g.properties {
                    d.insert(p.name.clone(), p.clone());
                }
            }
        }
        Self {
            response,
            names,
            defs,
        }
    }

    /// `object` (`deals` / `contacts` / `companies`) の項目の定義
    pub fn property(&self, object: &str, name: &str) -> Option<&CrmCatalogProperty> {
        self.defs.get(object).and_then(|m| m.get(name))
    }

    /// `object` の書き込める項目 (名前順)
    pub fn writable(&self, object: &str) -> Vec<&CrmCatalogProperty> {
        let mut v: Vec<&CrmCatalogProperty> = self
            .defs
            .get(object)
            .map(|m| m.values().filter(|p| p.writable).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
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
    clock: Clock,
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
        Self::with_clock(ttl, failure_ttl, Clock::default())
    }

    /// 時計を指定して作る (テストで手で進める時計を渡す)
    pub fn with_clock(ttl: Duration, failure_ttl: Duration, clock: Clock) -> Self {
        Self {
            slot: Mutex::new(Slot::default()),
            ttl,
            failure_ttl,
            clock,
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
            if self.clock.since(*stored) < self.ttl {
                cache_hit("property_catalog");
                return Ok((entry.clone(), true));
            }
        }
        if let Some((at, e)) = slot.failed.as_ref() {
            if self.clock.since(*at) < self.failure_ttl {
                // 取り直しに失敗した直後: 前の一覧があればそれで続ける
                if let Some((_, entry)) = slot.ok.as_ref() {
                    cache_hit("property_catalog");
                    return Ok((entry.clone(), true));
                }
                return Err(e.clone());
            }
        }
        cache_miss("property_catalog");
        match self.fetch(client).await {
            Ok(entry) => {
                slot.ok = Some((self.clock.now(), entry.clone()));
                slot.failed = None;
                Ok((entry, false))
            }
            Err(e) => {
                slot.failed = Some((self.clock.now(), e.clone()));
                if let Some((_, entry)) = slot.ok.as_ref() {
                    tracing::warn!(
                        error_kind = e.error_kind(),
                        "crm property catalog refresh failed; serving the last good catalog"
                    );
                    return Ok((entry.clone(), true));
                }
                Err(e)
            }
        }
    }

    /// 有効期間の終わり近く (残り 20% 未満) なら true (先読みの対象)。取得中は false
    pub fn refresh_due(&self) -> bool {
        self.slot.try_lock().is_ok_and(|slot| {
            slot.ok.as_ref().is_some_and(|(at, _)| {
                super::call_queue::refresh_due(self.clock.since(*at), self.ttl)
            })
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
                slot.ok = Some((self.clock.now(), entry));
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
        // 全部の応答を待つ (`try_join!` は 1 つ失敗すると残りの送信を途中で取り下げる。
        // 取り下げは HubSpot に届くかどうかが時の運で、呼び出し回数の数え方も揺れる)
        let (dp, cp, op, dg, cg, og) = tokio::join!(
            client.property_definitions(RecordType::Deal),
            client.property_definitions(RecordType::Contact),
            client.property_definitions(RecordType::Company),
            client.property_groups(RecordType::Deal),
            client.property_groups(RecordType::Contact),
            client.property_groups(RecordType::Company),
        );
        let [dp, cp, op, dg, cg, og] = [dp, cp, op, dg, cg, og].map(|r| {
            r.map_err(|e| match e {
                // 定義の取得で 404 は「レコードが無い」ではなく上流の不具合
                HubSpotError::NotFound => HubSpotError::Upstream { status: 404 },
                other => other,
            })
        });
        let (dp, cp, op, dg, cg, og) = (dp?, cp?, op?, dg?, cg?, og?);
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
        let label = match (
            super::label_ja::group_label(object.api_name(), name),
            str_field(g, "label").trim(),
        ) {
            (Some(ja), _) => ja.to_string(),
            (None, "") => name.to_string(),
            (None, l) => l.to_string(),
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
        let original = match str_field(p, "label").trim() {
            "" => name.to_string(),
            l => l.to_string(),
        };
        let (label, label_en) = match super::label_ja::property_label(object.api_name(), name, p) {
            Some(ja) => (ja.to_string(), Some(original.clone())),
            None => (original.clone(), None),
        };
        let writable = is_writable(
            p,
            name,
            &original,
            str_field(p, "type"),
            str_field(p, "fieldType"),
        );
        let prop = CrmCatalogProperty {
            name: name.to_string(),
            label,
            label_en,
            property_type: str_field(p, "type").to_string(),
            field_type: str_field(p, "fieldType").to_string(),
            options,
            owner_ref: is_owner_ref(p, str_field(p, "type")),
            writable,
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

/// 担当者 (owner) の ID を値に持つ選択式の項目か (`referencedObjectType` が `OWNER` の enumeration)
fn is_owner_ref(p: &Value, ptype: &str) -> bool {
    ptype == "enumeration" && str_field(p, "referencedObjectType").eq_ignore_ascii_case("OWNER")
}

/// 書き込めない項目 (専用の経路がある・システムが管理する・外部システムの ID)
const NEVER_WRITABLE: &[&str] = &[
    // ステージ・パイプラインは `stage` で移す (必須項目の確認が要る)
    "dealstage",
    "pipeline",
    // BPO の取引 ID と HubSpot URL (外部システムが入れる値)
    "bpo_transactio_id",
    "bpo_hsurl",
];

/// 書き込める項目か。読み取り専用 (`modificationMetadata.readOnlyValue`)・計算・`hs_*` と `hubspot_*` のシステム項目・
/// ラベルに「※編集不可」を含む項目・選択肢が外部にある項目 (担当者を選ぶ型を除く)・書き込めない型は false
fn is_writable(p: &Value, name: &str, label: &str, ptype: &str, field_type: &str) -> bool {
    let read_only = p
        .pointer("/modificationMetadata/readOnlyValue")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if read_only
        || is_true(p, "calculated")
        || (is_true(p, "externalOptions") && !is_owner_ref(p, ptype))
        || name.starts_with("hs_")
        || name.starts_with("hubspot_")
        || label.contains("※編集不可")
        || NEVER_WRITABLE.contains(&name)
    {
        return false;
    }
    if field_type.starts_with("calculation") || field_type == "file" {
        return false;
    }
    matches!(
        ptype,
        "string" | "number" | "date" | "datetime" | "enumeration" | "bool" | "phone_number"
    )
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
    let Some(client) = state.crm_hubspot() else {
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
                    "取引情報".to_string(),
                    vec!["bpo_10".to_string(), "bpo_32".to_string()]
                ),
                ("取引の売上".to_string(), vec!["amount".to_string()]),
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
    fn 担当者を選ぶ型は書ける項目になり_ほかの外部選択肢は書けないまま() {
        let props = json!({"results": [
            {"name": "bpo_26", "label": "アポ獲得者", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "externalOptions": true, "referencedObjectType": "OWNER", "options": []},
            {"name": "other_ext", "label": "外部の選択肢", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "externalOptions": true, "referencedObjectType": "CONTACT", "options": []},
            {"name": "ext_no_ref", "label": "参照先なし", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "externalOptions": true, "options": []},
            {"name": "hubspot_owner_id", "label": "取引担当者", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "externalOptions": true, "referencedObjectType": "OWNER", "options": []},
            {"name": "owner_ro", "label": "担当(読み取り専用)", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "externalOptions": true, "referencedObjectType": "OWNER", "modificationMetadata": {"readOnlyValue": true}},
            {"name": "plain", "label": "ふつう", "type": "string", "fieldType": "text", "groupName": "g"}
        ]});
        let groups = json!({"results": [{"name": "g", "label": "G", "displayOrder": 0}]});
        let o = build_object(RecordType::Deal, &props, &groups).unwrap();
        let by = |n: &str| {
            o.groups
                .iter()
                .flat_map(|g| g.properties.iter())
                .find(|p| p.name == n)
                .unwrap()
        };
        assert!(by("bpo_26").writable && by("bpo_26").owner_ref);
        assert!(
            !by("other_ext").writable,
            "担当者以外の外部選択肢は書けない"
        );
        assert!(!by("other_ext").owner_ref);
        assert!(!by("ext_no_ref").writable);
        assert!(
            !by("hubspot_owner_id").writable,
            "取引の担当者は今までどおり対象外"
        );
        assert!(
            !by("owner_ro").writable,
            "読み取り専用は担当者型でも書けない"
        );
        assert!(by("plain").writable && !by("plain").owner_ref);
    }

    #[test]
    fn 標準項目と標準グループは日本語にし_会社の英語名と未登録の名前はそのまま() {
        let props = json!({"results": [
            {"name": "amount", "label": "Amount", "type": "number", "fieldType": "number", "groupName": "dealinformation", "displayOrder": 1, "hubspotDefined": true},
            {"name": "bpo_hsurl", "label": "BPO_HSURL", "type": "string", "fieldType": "text", "groupName": "dealinformation", "displayOrder": 2, "hubspotDefined": false},
            {"name": "ai_email_alert_test", "label": "AI Email Alert (Test)", "type": "string", "fieldType": "text", "groupName": "dealinformation", "displayOrder": 3},
            {"name": "hs_unlisted_example", "label": "Unlisted Example", "type": "string", "fieldType": "text", "groupName": "dealinformation", "displayOrder": 4},
            {"name": "createdate", "label": "Create Date", "type": "datetime", "fieldType": "date", "groupName": "dealinformation", "displayOrder": 5},
            {"name": "bpo_10", "label": "不通時チェック", "type": "string", "fieldType": "text", "groupName": "dealinformation", "displayOrder": 6},
            {"name": "x_custom", "label": "Custom Thing", "type": "string", "fieldType": "text", "groupName": "custom_group", "displayOrder": 7}
        ]});
        let groups = json!({"results": [
            {"name": "dealinformation", "label": "Deal information", "displayOrder": 0},
            {"name": "custom_group", "label": "Custom Group", "displayOrder": 1}
        ]});
        let o = build_object(RecordType::Deal, &props, &groups).unwrap();
        assert_eq!(o.groups[0].name, "dealinformation");
        assert_eq!(o.groups[0].label, "取引情報");
        assert_eq!(
            o.groups[1].label, "Custom Group",
            "未登録のグループはそのまま"
        );
        let by = |n: &str| {
            o.groups
                .iter()
                .flat_map(|g| &g.properties)
                .find(|p| p.name == n)
                .unwrap()
        };
        assert_eq!(by("amount").label, "金額");
        assert_eq!(
            by("amount").label_en.as_deref(),
            Some("Amount"),
            "元の英語名は検索用に残す"
        );
        assert_eq!(by("createdate").label, "作成日");
        assert_eq!(
            by("bpo_hsurl").label,
            "BPO_HSURL",
            "hubspotDefined=false は訳さない"
        );
        assert_eq!(by("ai_email_alert_test").label, "AI Email Alert (Test)");
        assert_eq!(by("ai_email_alert_test").label_en, None);
        assert_eq!(
            by("hs_unlisted_example").label,
            "Unlisted Example",
            "表に無い名前はそのまま"
        );
        assert_eq!(by("bpo_10").label, "不通時チェック");
        assert_eq!(by("x_custom").label, "Custom Thing");
        // 内部名は変えない (保存済みのビューや URL は内部名だけを持つ)
        assert_eq!(by("amount").name, "amount");
        // 訳した項目の書き込み可否は元の定義で決める (表示名を変えても変わらない)
        let serialized = serde_json::to_value(by("amount")).unwrap();
        assert_eq!(serialized["label_en"], "Amount");
        assert!(serde_json::to_value(by("bpo_10"))
            .unwrap()
            .get("label_en")
            .is_none());
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

    /// 架電結果の入力欄 (frontend/src/screens/crm/callResultModel.ts の RESULT_GROUPS) と、
    /// 左の欄の「架電結果」カードの項目は、どれも書ける項目であること。
    /// 定義は 2026-10-14 に HubSpot の取引プロパティ定義を読み取りで確かめた型・入力形式・読み取り専用の設定と同じ
    #[test]
    fn 架電結果の入力欄の項目はどれも書ける() {
        let rw = json!({"readOnlyDefinition": false, "readOnlyValue": false});
        let defs = [
            ("risuto_kadennbi", "架電日", "date", "date", json!(null)),
            (
                "risuto_saikadennbi",
                "再架電日",
                "date",
                "date",
                json!(null),
            ),
            (
                "saikadenn_zikan",
                "再架電時間",
                "number",
                "number",
                json!(null),
            ),
            (
                "risuto_bikou",
                "激アツメモ（決済・利用サービス・決算・職種など）",
                "string",
                "text",
                json!(null),
            ),
            ("bpo_13", "次回架電日", "date", "date", json!(null)),
            (
                "bpo_14",
                "次回架電時間",
                "enumeration",
                "radio",
                json!(null),
            ),
            ("bpo_16", "タスクメモ", "string", "textarea", json!(null)),
            ("bpo_51", "接触", "enumeration", "select", json!(null)),
            ("nitteigiri", "日程切り", "number", "number", json!(null)),
            // 新規営業の「TEL_アポ日確定」の必須項目 (担当者を選ぶ型)
            (
                "aposyutokusya",
                "アポ取得者",
                "enumeration",
                "select",
                json!("OWNER"),
            ),
        ];
        for (name, label, ptype, field_type, referenced) in defs {
            let mut p = json!({
                "name": name, "label": label, "type": ptype, "fieldType": field_type,
                "calculated": false, "modificationMetadata": rw.clone(),
            });
            if !referenced.is_null() {
                p["externalOptions"] = json!(true);
                p["referencedObjectType"] = referenced;
            }
            assert!(
                is_writable(&p, name, label, ptype, field_type),
                "{name} は書ける項目のはず"
            );
        }
        // 自動で付く項目・試作の項目は入力欄に入れないが、仮に読み取り専用なら書けないと判定される
        let ro = json!({"name": "bpo_5", "label": "x", "type": "string", "fieldType": "text",
            "modificationMetadata": {"readOnlyValue": true}});
        assert!(!is_writable(&ro, "bpo_5", "x", "string", "text"));
    }
}
