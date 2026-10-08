//! HubSpot CRM API の async クライアント (読み取りのみ)。
//!
//! - `Authorization: Bearer <token>`。トークンは Debug・エラー・ログに出さない
//! - HubSpot の応答本文はエラーに載せない (入力値が反映される場合があるため)
//! - 429 / 5xx / タイムアウトのみ `max_retries` 回まで retry (指数待ち。429 は Retry-After 優先)
//! - `X-HubSpot-RateLimit-*` を最後の値として記録し、残りが 10% 未満なら warn
//! - batch_read / search は POST だが読み取りのみ。retry しても副作用は無い (書き込み API はここに置かない)
//! - Search API は `search_min_interval` 以上の間隔で開始する (5 req/s/アカウント)
//! - object / id はパスに入るため、既知の api_name と ASCII 数字 (1〜20 桁) 以外は送らない

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, AUTHORIZATION, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

use super::types::{
    AssociationLabelDef, AssociationRef, EngagementType, HubSpotError, HubSpotRecord,
    RateLimitSnapshot, RecordType, TokenInfo,
};

pub const DEFAULT_BASE_URL: &str = "https://api.hubapi.com";

/// `access_token_info` の結果を使い回す時間
pub const TOKEN_INFO_TTL: Duration = Duration::from_secs(300);

/// batch read 1 回あたりの上限 (HubSpot の仕様)
const BATCH_READ_LIMIT: usize = 100;
/// Retry-After / 指数待ちの上限
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);

/// retry・タイムアウトの設定 (テストでは待ち時間を短くする)。
#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub timeout: Duration,
    /// 初回を除く retry 回数 (429 / 5xx / タイムアウトのみ)
    pub max_retries: u32,
    /// retry の待ち時間の基準 (指数的に増やす。429 の Retry-After があればそちらを優先し上限で切る)
    pub retry_base_delay: Duration,
    /// Search API の開始間隔。HubSpot の上限は 5 req/s/アカウントだが、
    /// 鍵を既存バッチと共有しているため既定は 1000ms (当アプリは 1 req/s まで) に抑える
    pub search_min_interval: Duration,
    /// 429 の retry の最低待ち時間。Retry-After が 0 や小さい値でも `max(Retry-After, これ)` 待つ。
    /// 鍵を既存バッチと共有しているため (100 req/10 秒の窓をアカウントで共有)、
    /// 1 秒未満で retry しても枠は空いておらず無意味。
    pub rate_limited_min_wait: Duration,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 2,
            retry_base_delay: Duration::from_millis(500),
            // 鍵を既存バッチと共有しているため (5 req/s の枠を残しておく)
            search_min_interval: Duration::from_millis(1000),
            rate_limited_min_wait: Duration::from_secs(1),
        }
    }
}

/// Debug にトークンを出さない。
pub struct HubSpotClient {
    http: reqwest::Client,
    /// Bearer トークン。Authorization ヘッダを組む以外に使わない
    token: String,
    base_url: String,
    opts: ClientOptions,
    rate_limit: Mutex<Option<RateLimitSnapshot>>,
    /// 直前の Search 開始時刻 (1 permit のロックとして使う)
    search_gate: tokio::sync::Mutex<Option<Instant>>,
    /// `access_token_info` の結果 (成功のみ。`TOKEN_INFO_TTL` の間使い回す。ロックは呼び出しの間持つ=同時に 1 本)
    token_info_cache: tokio::sync::Mutex<Option<(Instant, TokenInfo)>>,
}

impl std::fmt::Debug for HubSpotClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubSpotClient").finish_non_exhaustive()
    }
}

/// 1 回の試行の結果 (retry 判定用)
enum Attempt {
    Ok(Value),
    /// retry してよい失敗。`wait` は Retry-After 由来の待ち時間
    Retryable {
        err: HubSpotError,
        wait: Option<Duration>,
    },
    Fatal(HubSpotError),
}

fn is_known_object(object: &str) -> bool {
    RecordType::ALL.iter().any(|t| t.api_name() == object)
        || EngagementType::ALL.iter().any(|t| t.api_name() == object)
}

fn check_object(object: &str) -> Result<(), HubSpotError> {
    if is_known_object(object) {
        Ok(())
    } else {
        Err(HubSpotError::Decode("invalid object".into()))
    }
}

fn check_id(id: &str) -> Result<(), HubSpotError> {
    if !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(HubSpotError::Decode("invalid id".into()))
    }
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    header_u64(headers, RETRY_AFTER.as_str()).map(|s| Duration::from_secs(s).min(MAX_RETRY_WAIT))
}

/// reqwest のエラーを URL 抜きで文字列化する
fn transport_message(e: reqwest::Error) -> String {
    let e = e.without_url();
    if e.is_connect() {
        "connect error".into()
    } else if e.is_request() {
        "request error".into()
    } else if e.is_body() || e.is_decode() {
        "body error".into()
    } else {
        e.to_string()
    }
}

/// JSON の値を文字列か null にそろえる (HubSpot は数値も文字列で返すが念のため)
fn value_to_opt_string(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// メールに対応する HubSpot owner (ID と所属チーム名。チーム名は画面の参考表示だけに使う)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRef {
    pub id: String,
    pub teams: Vec<String>,
}

fn id_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// batch 系の応答の `results`。全 ID が見つからないときは `results` が無く `errors` だけの
/// 207 が返りうる (HubSpot の multi-status)。その場合は 0 件。どちらも無ければ想定外の形。
fn batch_results<'a>(v: &'a Value, what: &str) -> Result<&'a [Value], HubSpotError> {
    match v.get("results").and_then(Value::as_array) {
        Some(results) => Ok(results),
        None if v.get("errors").is_some_and(Value::is_array) => Ok(&[]),
        None => Err(HubSpotError::Decode(format!("{what} without results"))),
    }
}

pub(crate) fn parse_record(v: &Value) -> Result<HubSpotRecord, HubSpotError> {
    let id = v
        .get("id")
        .and_then(id_string)
        .ok_or_else(|| HubSpotError::Decode("record without id".into()))?;
    let properties = match v.get("properties") {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), value_to_opt_string(v)))
            .collect(),
        None | Some(Value::Null) => BTreeMap::new(),
        Some(_) => return Err(HubSpotError::Decode("properties is not an object".into())),
    };
    let text = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
    Ok(HubSpotRecord {
        id,
        properties,
        created_at: text("createdAt"),
        updated_at: text("updatedAt"),
        archived: v.get("archived").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn parse_associations(v: &Value) -> Result<(Vec<AssociationRef>, bool), HubSpotError> {
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| HubSpotError::Decode("associations without results".into()))?;
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        // 関連 1 件の形が崩れていても全体は失敗させない (その 1 件だけ飛ばす)
        let Some(id) = r.get("toObjectId").and_then(id_string) else {
            continue;
        };
        let labels = r
            .get("associationTypes")
            .and_then(Value::as_array)
            .map(|types| {
                types
                    .iter()
                    .filter_map(|t| t.get("label").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push(AssociationRef {
            id,
            labels,
            type_names: Vec::new(),
        });
    }
    let has_more = v
        .pointer("/paging/next/after")
        .is_some_and(|a| !a.is_null());
    Ok((out, has_more))
}

impl HubSpotClient {
    /// `access_token` が空白だけなら `Err(NotConfigured)`。
    pub fn new(
        access_token: String,
        base_url: &str,
        opts: ClientOptions,
    ) -> Result<Self, HubSpotError> {
        let token = access_token.trim().to_string();
        if token.is_empty() {
            return Err(HubSpotError::NotConfigured);
        }
        let http = reqwest::Client::builder()
            .timeout(opts.timeout)
            .connect_timeout(opts.timeout.min(Duration::from_secs(5)))
            // 固定の api.hubapi.com 以外へ Bearer を持ち出さない (リダイレクトは辿らない)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| HubSpotError::Transport(transport_message(e)))?;
        Ok(Self {
            http,
            token,
            base_url: base_url.trim_end_matches('/').to_string(),
            opts,
            rate_limit: Mutex::new(None),
            search_gate: tokio::sync::Mutex::new(None),
            token_info_cache: tokio::sync::Mutex::new(None),
        })
    }

    /// 鍵の scope とポータル ID を HubSpot に問い合わせる (`POST /oauth/v2/private-apps/get/access-token-info`)。
    /// 戻りの bool は「キャッシュから返したか」。
    ///
    /// - HubSpot への呼び出しは 1 回だけ (retry しない)。`timeout` はこの 1 回に掛ける
    /// - 成功だけを 5 分キャッシュする。失敗は次回また問い合わせる
    /// - 返すのは scope・ポータル ID・確認時刻だけ。鍵そのものは (本文に入れて送る以外) 外に出さない
    pub async fn access_token_info(
        &self,
        timeout: Duration,
    ) -> Result<(TokenInfo, bool), HubSpotError> {
        let mut cache = self.token_info_cache.lock().await;
        if let Some((at, info)) = cache.as_ref() {
            if at.elapsed() < TOKEN_INFO_TTL {
                return Ok((info.clone(), true));
            }
        }
        let url = format!(
            "{}/oauth/v2/private-apps/get/access-token-info",
            self.base_url
        );
        let resp = self
            .http
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .timeout(timeout)
            .json(&json!({ "tokenKey": self.token }))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    HubSpotError::Timeout
                } else {
                    HubSpotError::Transport(transport_message(e))
                }
            })?;
        let status = resp.status();
        if !status.is_success() {
            let code = status.as_u16();
            return Err(match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    HubSpotError::Auth { status: code }
                }
                StatusCode::NOT_FOUND => HubSpotError::NotFound,
                StatusCode::TOO_MANY_REQUESTS => HubSpotError::RateLimited,
                _ => HubSpotError::Upstream { status: code },
            });
        }
        let bytes = resp.bytes().await.map_err(|e| {
            if e.is_timeout() {
                HubSpotError::Timeout
            } else {
                HubSpotError::Transport(transport_message(e))
            }
        })?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| HubSpotError::Decode("invalid json".into()))?;
        let scopes = v
            .get("scopes")
            .and_then(Value::as_array)
            .ok_or_else(|| HubSpotError::Decode("token info without scopes".into()))?
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        let info = TokenInfo {
            portal_id: v.get("hubId").and_then(id_string),
            scopes,
            checked_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };
        *cache = Some((Instant::now(), info.clone()));
        Ok((info, false))
    }

    /// `GET /crm/v3/properties/{object}` (プロパティ定義の一覧。Contact / Company / Deal のみ)。
    /// 応答の `results` をそのまま返す。値の解釈は呼び出し側 (`handlers::crm_metadata`)。
    pub async fn property_definitions(&self, object: RecordType) -> Result<Value, HubSpotError> {
        let path = format!("/crm/v3/properties/{}", object.api_name());
        self.send(Method::GET, &path, &[], None).await
    }

    /// `GET /crm/v3/properties/{object}/groups` (プロパティのグループ = HubSpot の画面の見出し。Contact / Company / Deal のみ)。
    /// 応答の JSON をそのまま返す。値の解釈は呼び出し側 (`crm::property_catalog`)。
    pub async fn property_groups(&self, object: RecordType) -> Result<Value, HubSpotError> {
        let path = format!("/crm/v3/properties/{}/groups", object.api_name());
        self.send(Method::GET, &path, &[], None).await
    }

    /// `GET /crm/v3/pipelines/deals` (Deal のパイプラインとステージ定義)。
    pub async fn deal_pipelines(&self) -> Result<Value, HubSpotError> {
        self.send(Method::GET, "/crm/v3/pipelines/deals", &[], None)
            .await
    }

    /// `GET /crm/v3/owners?limit=100&archived={archived}[&after=..]` の 1 ページ (応答の JSON をそのまま返す)。
    /// 退職者 (archived=true) は別の呼び出しでしか返ってこない。ページを追うのは呼び出し側。
    /// Search ではないので `search_min_interval` の対象外 (呼び出し回数は呼び出し側が数える)。
    pub async fn owners_page(
        &self,
        archived: bool,
        after: Option<&str>,
    ) -> Result<Value, HubSpotError> {
        let mut query: Vec<(&str, String)> = vec![
            ("limit", "100".to_string()),
            ("archived", archived.to_string()),
        ];
        if let Some(a) = after {
            query.push(("after", a.to_string()));
        }
        self.send(Method::GET, "/crm/v3/owners", &query, None).await
    }

    /// `GET /crm/v3/objects/{object}/{id}?properties=...`
    /// `object` は `RecordType::api_name()` / `EngagementType::api_name()`。
    pub async fn get_object(
        &self,
        object: &str,
        id: &str,
        properties: &[&str],
    ) -> Result<HubSpotRecord, HubSpotError> {
        check_object(object)?;
        check_id(id)?;
        let path = format!("/crm/v3/objects/{object}/{id}");
        let query: Vec<(&str, String)> = if properties.is_empty() {
            Vec::new()
        } else {
            vec![("properties", properties.join(","))]
        };
        let v = self.send(Method::GET, &path, &query, None).await?;
        parse_record(&v)
    }

    /// `POST /crm/v3/objects/{object}/batch/read` (読み取り。最大 100 件ずつに分割)。
    /// 見つからない ID は結果に含まれないだけでエラーにしない。
    pub async fn batch_read(
        &self,
        object: &str,
        ids: &[String],
        properties: &[&str],
    ) -> Result<Vec<HubSpotRecord>, HubSpotError> {
        check_object(object)?;
        for id in ids {
            check_id(id)?;
        }
        let path = format!("/crm/v3/objects/{object}/batch/read");
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(BATCH_READ_LIMIT) {
            let body = json!({
                "properties": properties,
                "inputs": chunk.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
            });
            let v = self.send(Method::POST, &path, &[], Some(&body)).await?;
            for r in batch_results(&v, "batch read")? {
                out.push(parse_record(r)?);
            }
        }
        Ok(out)
    }

    /// `GET /crm/v3/objects/{object}/{id}?properties=..&associations=a,b,..`
    /// 1 回の呼び出しで本体と関連 ID を取る (呼び出し回数を抑えるため)。
    ///
    /// 応答の `associations.{type}.results[].id` (文字列) を関連 ID として返す。`{type}` は
    /// 要求した型名 (複数形: `contacts`, `calls` ...)。次ページがあれば
    /// `associations.{type}.paging.next` が付く → truncated = true。
    /// v3 のこの形では関連ラベルの名前は取れないので `labels` は常に空 Vec。代わりに `type_names` に
    /// `results[].type` (例 `deal_to_company`) を入れる (ラベルは [`Self::association_labels`] の定義から引く)。
    /// 戻り値には要求した型すべてのキーが入る (関連が無ければ空 Vec・false)。
    ///
    /// [推測] 応答のキー名・paging の位置は HubSpot v3 の公開仕様からの想定で、
    /// 実データでの確認はユーザー承認後。偽 HubSpot (テスト) も同じ形を使う。
    pub async fn get_object_with_associations(
        &self,
        object: &str,
        id: &str,
        properties: &[&str],
        to_types: &[&str],
    ) -> Result<(HubSpotRecord, BTreeMap<String, (Vec<AssociationRef>, bool)>), HubSpotError> {
        check_object(object)?;
        check_id(id)?;
        for t in to_types {
            check_object(t)?;
        }
        let path = format!("/crm/v3/objects/{object}/{id}");
        let mut query: Vec<(&str, String)> = Vec::new();
        if !properties.is_empty() {
            query.push(("properties", properties.join(",")));
        }
        if !to_types.is_empty() {
            query.push(("associations", to_types.join(",")));
        }
        let v = self.send(Method::GET, &path, &query, None).await?;
        let record = parse_record(&v)?;
        let mut out: BTreeMap<String, (Vec<AssociationRef>, bool)> = to_types
            .iter()
            .map(|t| (t.to_string(), (Vec::new(), false)))
            .collect();
        for t in to_types {
            let Some(entry) = v.pointer(&format!("/associations/{t}")) else {
                continue;
            };
            // `results` が無い / null (形が崩れた型) は関連 0 件として扱い、本体は返す
            let results = entry
                .get("results")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut refs: Vec<AssociationRef> = Vec::with_capacity(results.len());
            for r in results {
                // id の無い 1 件は飛ばす
                let Some(rid) = r.get("id").and_then(id_string) else {
                    continue;
                };
                let ty = r.get("type").and_then(Value::as_str).map(str::to_string);
                // 関連タイプ違いで同じ id が繰り返されることがあるので 1 件にする (型名はまとめて持つ)
                match refs.iter_mut().find(|x| x.id == rid) {
                    Some(existing) => {
                        if let Some(t) = ty {
                            if !existing.type_names.contains(&t) {
                                existing.type_names.push(t);
                            }
                        }
                    }
                    None => refs.push(AssociationRef {
                        id: rid,
                        labels: Vec::new(),
                        type_names: ty.into_iter().collect(),
                    }),
                }
            }
            let more = entry.pointer("/paging/next").is_some_and(|n| !n.is_null());
            out.insert(t.to_string(), (refs, more));
        }
        Ok((record, out))
    }

    /// `POST /crm/v4/associations/{from}/{to}/batch/read` (読み取り。100 件ずつに分割)。
    /// 戻り値は from の ID → 関連 (ラベル付き)。関連の無い from は空 Vec か、キー自体が無い。
    /// 1 from あたりの関連の次ページは追わない (1 ページ分のみ)。
    pub async fn batch_associations(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> Result<BTreeMap<String, Vec<AssociationRef>>, HubSpotError> {
        check_object(from)?;
        check_object(to)?;
        for id in ids {
            check_id(id)?;
        }
        let path = format!("/crm/v4/associations/{from}/{to}/batch/read");
        let mut out: BTreeMap<String, Vec<AssociationRef>> = BTreeMap::new();
        for chunk in ids.chunks(BATCH_READ_LIMIT) {
            let body = json!({
                "inputs": chunk.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
            });
            let v = self.send(Method::POST, &path, &[], Some(&body)).await?;
            for r in batch_results(&v, "batch associations")? {
                let Some(from_id) = r.pointer("/from/id").and_then(id_string) else {
                    continue;
                };
                let to_list = r
                    .get("to")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let (refs, _) = parse_associations(&json!({ "results": to_list }))?;
                out.entry(from_id).or_default().extend(refs);
            }
        }
        Ok(out)
    }

    /// `GET /crm/v4/associations/{from}/{to}/labels` (関連ラベルの定義。読み取り)。
    /// `results[]` の `category` / `typeId` / `label` (null は無ラベル)。typeId の無い 1 件は飛ばす。
    pub async fn association_labels(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<AssociationLabelDef>, HubSpotError> {
        check_object(from)?;
        check_object(to)?;
        let path = format!("/crm/v4/associations/{from}/{to}/labels");
        let v = self.send(Method::GET, &path, &[], None).await?;
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| HubSpotError::Decode("association labels without results".into()))?;
        Ok(results
            .iter()
            .filter_map(|r| {
                let type_id = r.get("typeId").and_then(Value::as_u64)?;
                Some(AssociationLabelDef {
                    category: r
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    type_id,
                    label: r
                        .get("label")
                        .and_then(Value::as_str)
                        .filter(|l| !l.trim().is_empty())
                        .map(str::to_string),
                })
            })
            .collect())
    }

    /// `GET /crm/v4/objects/{from}/{id}/associations/{to}?limit=500`。
    /// 多対多をそのまま返す (1 件に潰さない)。戻り値の bool は次ページが残っていたか。
    pub async fn list_associations(
        &self,
        from: &str,
        id: &str,
        to: &str,
    ) -> Result<(Vec<AssociationRef>, bool), HubSpotError> {
        check_object(from)?;
        check_object(to)?;
        check_id(id)?;
        let path = format!("/crm/v4/objects/{from}/{id}/associations/{to}");
        let v = self
            .send(Method::GET, &path, &[("limit", "500".to_string())], None)
            .await?;
        parse_associations(&v)
    }

    /// `POST /crm/v3/objects/{object}/search` (読み取り)。5 req/s 以下に絞る。
    pub async fn search(&self, object: &str, body: Value) -> Result<Value, HubSpotError> {
        check_object(object)?;
        {
            // ロックを持ったまま待つので、同時に来た Search は 1 本ずつ間隔を空けて開始する
            let mut last = self.search_gate.lock().await;
            if let Some(prev) = *last {
                let next = prev + self.opts.search_min_interval;
                let now = Instant::now();
                if next > now {
                    tokio::time::sleep(next - now).await;
                }
            }
            *last = Some(Instant::now());
        }
        let path = format!("/crm/v3/objects/{object}/search");
        self.send(Method::POST, &path, &[], Some(&body)).await
    }

    /// `GET /crm/v3/owners?email=..` (読み取り)。メールに対応する HubSpot owner の ID と所属チーム名を返す。
    /// 応答の `email` が要求と (大文字小文字を除いて) 一致する有効な (archived でない) ものだけ採用する (曖昧一致を避ける)。
    /// 見つからなければ `Ok(None)`。`teams` は応答の `teams[].name` (空・欠落は空の一覧)。
    pub async fn owner_by_email(&self, email: &str) -> Result<Option<OwnerRef>, HubSpotError> {
        let email = email.trim();
        if email.is_empty() || email.len() > 320 {
            return Ok(None);
        }
        let v = self
            .send(
                Method::GET,
                "/crm/v3/owners",
                &[("email", email.to_string()), ("limit", "10".to_string())],
                None,
            )
            .await?;
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| HubSpotError::Decode("owners without results".into()))?;
        Ok(results.iter().find_map(|r| {
            let same = r
                .get("email")
                .and_then(Value::as_str)
                .is_some_and(|e| e.trim().eq_ignore_ascii_case(email));
            let archived = r.get("archived").and_then(Value::as_bool).unwrap_or(false);
            if !same || archived {
                return None;
            }
            let id = r.get("id").and_then(id_string)?;
            let teams = r
                .get("teams")
                .and_then(Value::as_array)
                .map(|ts| {
                    ts.iter()
                        .filter_map(|t| t.get("name").and_then(Value::as_str))
                        .map(|n| n.trim().to_string())
                        .filter(|n| !n.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            Some(OwnerRef { id, teams })
        }))
    }

    /// 最後に観測したレート制限ヘッダ
    pub fn last_rate_limit(&self) -> Option<RateLimitSnapshot> {
        self.rate_limit.lock().ok().and_then(|g| g.clone())
    }

    /// retry 込みで 1 リクエストを送り、JSON を返す。
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, HubSpotError> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            match self.try_once(&method, &url, query, body).await {
                Attempt::Ok(v) => return Ok(v),
                Attempt::Fatal(e) => return Err(e),
                Attempt::Retryable { err, wait } => {
                    if attempt >= self.opts.max_retries {
                        return Err(err);
                    }
                    let exp = self
                        .opts
                        .retry_base_delay
                        .saturating_mul(2u32.saturating_pow(attempt));
                    let mut backoff = wait.unwrap_or(exp);
                    if err == HubSpotError::RateLimited {
                        // Retry-After が 0 や小さい値でも最低待ち時間は守る
                        backoff = backoff.max(self.opts.rate_limited_min_wait);
                    }
                    let backoff = backoff.min(MAX_RETRY_WAIT);
                    tracing::warn!(
                        error_kind = err.error_kind(),
                        attempt = attempt + 1,
                        wait_ms = backoff.as_millis() as u64,
                        "HubSpot API を retry します"
                    );
                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn try_once(
        &self,
        method: &Method,
        url: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Attempt {
        let mut req = self
            .http
            .request(method.clone(), url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token));
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) if e.is_timeout() => {
                return Attempt::Retryable {
                    err: HubSpotError::Timeout,
                    wait: None,
                }
            }
            Err(e) => return Attempt::Fatal(HubSpotError::Transport(transport_message(e))),
        };
        self.record_rate_limit(resp.headers());
        let status = resp.status();
        if status.is_success() {
            return match resp.bytes().await {
                Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(v) => Attempt::Ok(v),
                    Err(_) => Attempt::Fatal(HubSpotError::Decode("invalid json".into())),
                },
                Err(e) if e.is_timeout() => Attempt::Retryable {
                    err: HubSpotError::Timeout,
                    wait: None,
                },
                Err(e) => Attempt::Fatal(HubSpotError::Transport(transport_message(e))),
            };
        }
        let code = status.as_u16();
        match status {
            StatusCode::TOO_MANY_REQUESTS => Attempt::Retryable {
                err: HubSpotError::RateLimited,
                wait: retry_after(resp.headers()),
            },
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Attempt::Fatal(HubSpotError::Auth { status: code })
            }
            StatusCode::NOT_FOUND => Attempt::Fatal(HubSpotError::NotFound),
            s if s.is_server_error() => Attempt::Retryable {
                err: HubSpotError::Upstream { status: code },
                wait: None,
            },
            _ => Attempt::Fatal(HubSpotError::Upstream { status: code }),
        }
    }

    fn record_rate_limit(&self, headers: &HeaderMap) {
        let snap = RateLimitSnapshot {
            max: header_u64(headers, "x-hubspot-ratelimit-max"),
            remaining: header_u64(headers, "x-hubspot-ratelimit-remaining"),
            daily_remaining: header_u64(headers, "x-hubspot-ratelimit-daily-remaining"),
        };
        if snap.max.is_none() && snap.remaining.is_none() && snap.daily_remaining.is_none() {
            // Search の応答などヘッダが無い場合は前回値を残す
            return;
        }
        if let (Some(max), Some(remaining)) = (snap.max, snap.remaining) {
            if max > 0 && remaining.saturating_mul(10) < max {
                tracing::warn!(
                    max,
                    remaining,
                    daily_remaining = snap.daily_remaining,
                    "HubSpot API のレート制限の残りが 10% 未満です"
                );
            }
        }
        if let Ok(mut g) = self.rate_limit.lock() {
            *g = Some(snap);
        }
    }
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod client_tests;
