//! `GET /api/crm/call-queue` (架電キュー。HubSpot からの読み取りだけ。書き込みはしない)。
//!
//! 設計: `claudedocs/CRM_CALL_QUEUE_DESIGN_2026-10-04.md`、確定条件: `claudedocs/REACT_HANDOVER_2026-10-05.md` §4.5。
//!
//! ## キューの定義 (確定条件)
//! - パイプライン `753186575` の Deal。**未済 (1095387442) は全部**。
//!   それ以外はアポ日確定・架電禁止・商談実施処理を除く全ステージを、次回架電日 `bpo_13` が今日 (JST) 以前のときだけ出す
//! - 架電禁止理由 `bpo_3`・ブロック理由 `bpo_4` が入っている Deal は外す。不通時チェック `bpo_10` は残して印を付ける
//! - 電話番号がどこにも無い Deal は外す (`bpo_29` → 担当者 phone → 担当者 mobilephone → 会社 phone)
//! - BPO は自分が担当の Deal だけ。管理者の既定は全員分 (`owner=all`)。担当なしは管理者だけ
//!
//! ## 取得の組み立て (行ごとに API を呼ばない)
//! 1 ページ = Search 1 + 関連 2 (`deal→contact` / `deal→company`) + 読み取り 2 (contact / company) = 5 回
//! (件数に依存しない。ステージ名のキャッシュが冷えているときだけ +1。BPO の owner 対応が未取得のときは +1)。
//! 1 Deal につき読む Contact は 1 人 (主 → なければ最初の 1 人) なので、読み取りは 50 件 (limit 上限) で 1 バッチに収まる。
//!
//! ## 並び (`sort`) と「段階」
//! HubSpot の Search は 1 回に 1 つのプロパティでしか並べられず、空の値の並び位置も保証されない。
//! そこで並びを複数の「段階」(それぞれ別の Search) に分け、段階を順に辿る。cursor は (段階, after) を署名して持つ。
//! 例: 既定 = ① 次回架電日が今日以前 (次回日の古い順) → ② 未済で未架電 (最終架電日なし) → ③ 未済で最終架電日の古い順。
//! 段階の境目でページが `limit` に満たなくても、`next_cursor` がある限り続きがある。
//!
//! ## 日付の範囲 (PR-2)
//! `next_from` / `next_to` (次回架電日 `bpo_13`)、`last_from` / `last_to` (最終架電日 `bpo_20`) は JST の日付 `YYYY-MM-DD`
//! (両端を含む。実在する日付で 2000〜2100 年。from > to は 400)。HubSpot の日付プロパティに合わせ、その日の UTC 0 時の ms で Search に渡す。
//! - 次回架電日の範囲を指定すると、範囲に入る Deal だけが対象 (日付なしの未済は出ない)。
//!   未済以外のステージは「今日以前」の条件と範囲の共通部分、未済は範囲そのまま (今日より後も出る)。
//! - 最終架電日の範囲を指定すると、最終架電日が空の段階は検索しない (範囲と矛盾するため)。
//! - `sort=next_call_asc` は `default` と同じ並びの明示値。
//!
//! ## Search の上限への収まり
//! HubSpot Search は OR グループ 5・グループあたり 6 フィルタ・全体 18 まで。各 Search は
//! OR グループ最大 2、グループあたり最大 6、全体最大 12。`pipeline` の絞り込みはグループあたり 6 に収めるため
//! Search には入れず、返ってきた Deal の `pipeline` を後段で確認する (ステージ ID は HubSpot 内で一意)。
//! 範囲の指定でグループが 6 に収まらないときは、架電禁止理由 `bpo_3`・ブロック理由 `bpo_4` の NOT_HAS_PROPERTY を
//! Search に入れず、返ってきた行を後段で外す (外した件数は `partial.excluded.stop_reason`。ページが `limit` に満たないことがある)。
//! 全組み合わせが上限内に収まることはテストで確認している。
//!
//! ## 認可と役割
//! `rbac::authorize` (Google OIDC + 許可リスト + 無効アカウント + 管理者かどうか) の後に読む。
//! 見られる範囲は全員同じ (全件)。違うのは `owner` 未指定のときの既定だけで、管理者 = 全員分、それ以外 = 自分 (`me`)。
//! `owner=all` / `unassigned` / owner id は全員が指定できる。本人のメール → HubSpot owner id は Owners API で引き、メモリにキャッシュする。
//! `me` (既定を含む) で owner を引けない人は全員分に倒さず 409 `owner_not_resolved` (画面が所有者の選択を促す)。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    extract::{Extension, RawQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower_sessions::Session;
use ts_rs::TS;

use super::rbac::{self, CrmRole};
use super::routes::{
    error_json, hubspot_error_response, is_valid_id, timeout_response, CrmCtx, CrmErrorResponse,
    CRM_REQUEST_DEADLINE,
};
use crate::hubspot::deep_link::{hubspot_portal_id, record_url};
use crate::hubspot::{
    AssociationRef, HubSpotClient, HubSpotError, HubSpotRecord, OwnerRef, RecordType,
};
use crate::AppState;

/// 架電キューのパイプライン (bpo_リクロジ)
pub const PIPELINE_ID: &str = "753186575";
/// 未済 (架電待ちの在庫)。キューに全部出す
pub const STAGE_UNPROCESSED: &str = "1095387442";
/// 次回架電日が今日以前のときだけ出すステージ (15 個)。アポ日確定 1095457875・架電禁止 1095457878・
/// 商談実施処理 1325086466 は含めない (ユーザー確認 2026-10-05)
pub const STAGES_WHEN_DUE: [&str; 15] = [
    "1095387443", // 不通
    "1095387444", // 受付ブロック
    "1095387445", // 不在
    "1274330477", // 番号検索依頼中
    "1095387446", // 担当者ブロック
    "1409897995", // 成果報酬のみ
    "1095387447", // ニーズなし/無料のみ
    "1325087323", // ニーズなし/有料あり
    "1325087324", // ニーズあり/無料のみ
    "1095387448", // ニーズあり/有料あり
    "1448079987", // SV依頼案件
    "1319310149", // 日程確保
    "1095457877", // 案件差戻
    "1330563334", // 商談未実施処理
    "1369739056", // リスト精査前
];

pub const DEFAULT_LIMIT: u32 = 25;
pub const MAX_LIMIT: u32 = 50;
pub const MAX_Q_CHARS: usize = 100;
/// HubSpot Search で after + limit が超えられない件数
pub const SEARCH_WINDOW: u64 = 10_000;
/// cursor の有効期間
pub const CURSOR_TTL_SECS: i64 = 30 * 60;
const OWNER_TTL: Duration = Duration::from_secs(10 * 60);
const OWNER_MISS_TTL: Duration = Duration::from_secs(60);
const LABELS_TTL: Duration = Duration::from_secs(5 * 60);

pub(super) const DEAL_PROPERTIES: &[&str] = &[
    "dealname",
    "dealstage",
    "pipeline",
    "hubspot_owner_id",
    "bpo_13",
    "bpo_14",
    "bpo_20",
    "bpo_3",
    "bpo_4",
    "bpo_10",
    "bpo_29",
];
const CONTACT_PROPS: &[&str] = &["firstname", "lastname", "phone", "mobilephone", "jobtitle"];
const COMPANY_PROPS: &[&str] = &["name", "phone"];

fn is_allowed_stage(id: &str) -> bool {
    id == STAGE_UNPROCESSED || STAGES_WHEN_DUE.contains(&id)
}

// ---------------------------------------------------------------------------
// 状態 (owner / ステージ名のキャッシュ、cursor の署名鍵、時計)
// ---------------------------------------------------------------------------

pub struct CallQueueState {
    /// cursor の署名鍵。プロセスごとのランダム値 (再起動で既存の cursor は無効になる)
    key: [u8; 32],
    /// テストで時計を固定する
    fixed_now: Option<DateTime<Utc>>,
    /// メール (小文字) → (取得時刻, owner id)。見つからなかった結果も短く覚える
    owners: Mutex<HashMap<String, (Instant, Option<OwnerRef>)>>,
    /// ステージ ID → 表示名 (キューのパイプラインだけ)
    labels: tokio::sync::Mutex<Option<(Instant, HashMap<String, String>)>>,
    /// 管理者向けの担当者一覧 (`GET /api/crm/owners`)
    pub(super) owner_list: super::owners::OwnerListCache,
}

impl CallQueueState {
    pub fn new() -> Self {
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut key);
        Self::build(key, None)
    }

    #[cfg(test)]
    pub fn for_test(key: [u8; 32], now: DateTime<Utc>) -> Self {
        Self::build(key, Some(now))
    }

    fn build(key: [u8; 32], fixed_now: Option<DateTime<Utc>>) -> Self {
        Self {
            key,
            fixed_now,
            owners: Mutex::new(HashMap::new()),
            labels: tokio::sync::Mutex::new(None),
            owner_list: super::owners::OwnerListCache::new(),
        }
    }

    /// テストで担当者一覧の有効期間を差し替える
    #[cfg(test)]
    pub fn with_owner_list_ttl(mut self, ttl: Duration) -> Self {
        self.owner_list = super::owners::OwnerListCache::with_ttl(ttl);
        self
    }

    pub(super) fn now(&self) -> DateTime<Utc> {
        self.fixed_now.unwrap_or_else(Utc::now)
    }

    /// 本人のメール → HubSpot owner (id と所属チーム名)。引けた結果は 10 分、見つからなかった結果は 1 分キャッシュ。
    /// 失敗 (HubSpot のエラー) はキャッシュしない。所属チームの変更は最大 10 分で反映される。
    pub(super) async fn owner_info_for(
        &self,
        client: &HubSpotClient,
        email: &str,
    ) -> Result<Option<OwnerRef>, HubSpotError> {
        let key = email.trim().to_lowercase();
        if let Ok(g) = self.owners.lock() {
            if let Some((at, v)) = g.get(&key) {
                let ttl = if v.is_some() {
                    OWNER_TTL
                } else {
                    OWNER_MISS_TTL
                };
                if at.elapsed() < ttl {
                    return Ok(v.clone());
                }
            }
        }
        let found = client.owner_by_email(&key).await?;
        if let Ok(mut g) = self.owners.lock() {
            g.insert(key, (Instant::now(), found.clone()));
        }
        Ok(found)
    }

    /// 本人の owner id だけ ([`Self::owner_info_for`] のキャッシュを使う)
    pub(super) async fn owner_for(
        &self,
        client: &HubSpotClient,
        email: &str,
    ) -> Result<Option<String>, HubSpotError> {
        Ok(self.owner_info_for(client, email).await?.map(|o| o.id))
    }

    /// ステージ ID → 表示名 (5 分キャッシュ。同時に冷えた要求は 1 回の取得にまとめる)
    pub(super) async fn stage_labels(
        &self,
        client: &HubSpotClient,
    ) -> Result<HashMap<String, String>, HubSpotError> {
        let mut slot = self.labels.lock().await;
        if let Some((at, m)) = slot.as_ref() {
            if at.elapsed() < LABELS_TTL {
                return Ok(m.clone());
            }
        }
        let v = client.deal_pipelines().await?;
        let pipelines = crate::handlers::crm_metadata::parse_pipelines(&v)?;
        let map: HashMap<String, String> = pipelines
            .into_iter()
            .filter(|p| p.id == PIPELINE_ID)
            .flat_map(|p| p.stages)
            .map(|s| (s.id, s.label))
            .collect();
        *slot = Some((Instant::now(), map.clone()));
        Ok(map)
    }
}

impl Default for CallQueueState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 応答の型 (ts-rs で frontend/src/generated/ に書き出す)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueStop {
    /// 架電禁止理由 `bpo_3` (入っていれば本来キューに出ない)
    pub prohibited_reason: Option<String>,
    /// ブロック理由 `bpo_4` (同上)
    pub block_reason: Option<String>,
    /// 不通時チェック `bpo_10`。入っていても残して印を付ける
    pub unreachable_check: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueContact {
    pub id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    pub mobile: Option<String>,
    pub job_title: Option<String>,
    /// 主の担当者 (なければ最初の 1 人) のほかに紐づく担当者の人数
    pub extra_count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueCompany {
    pub id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueDeepLinks {
    pub deal: String,
    pub contact: Option<String>,
    pub company: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueItem {
    pub deal_id: String,
    pub deal_name: Option<String>,
    pub stage_id: String,
    /// パイプライン定義から引いた表示名。引けなければ null (ID を表示名にしない)
    pub stage_label: Option<String>,
    pub owner_id: Option<String>,
    pub next_call_date: Option<String>,
    pub next_call_time: Option<String>,
    pub last_call_date: Option<String>,
    pub stop: CallQueueStop,
    pub contact: Option<CallQueueContact>,
    pub company: Option<CallQueueCompany>,
    /// 架けるべき電話番号 (`phone_source` の優先順位で決めた 1 つ)
    pub phone: Option<String>,
    /// `deal` / `contact` / `mobile` / `company`。番号を判断できなかった行は null
    pub phone_source: Option<String>,
    pub deep_links: CallQueueDeepLinks,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueScope {
    /// `all` / `me` / `unassigned` / owner id
    pub owner: String,
    /// `admin` (管理者。既定が全員分) / `own` (それ以外の全員。既定が自分。どちらも全件を読める)
    pub role: String,
    /// ログインした人の HubSpot owner の所属チーム名 (参考情報だけ。見られる範囲の判定には使わない)。
    /// 管理者と、自分以外の所有者を指定したときは HubSpot を余分に呼ばないので空。自分の owner が見つからない人は既定 (me) が 409 `owner_not_resolved` になるのでここには来ない
    pub teams: Vec<String>,
    /// 実際に絞り込んだステージ ID (昇順)
    pub stages: Vec<String>,
    /// `all` / `today`
    pub due: String,
    pub sort: String,
    pub q: Option<String>,
    pub limit: u32,
    /// 次回架電日 `bpo_13` の範囲 (JST の日付 `YYYY-MM-DD`。指定がなければ null)
    pub next_from: Option<String>,
    pub next_to: Option<String>,
    /// 最終架電日 `bpo_20` の範囲 (同上)
    pub last_from: Option<String>,
    pub last_to: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct CallQueueExcluded {
    /// 電話番号がどこにも無い (関連を読めたときだけ判定)
    pub no_phone: u32,
    /// 架電禁止理由・ブロック理由が入っていた (Search でも外すが、後段でも確認)
    pub stop_reason: u32,
    /// アーカイブ済み・別パイプライン・許可外ステージ
    pub out_of_scope: u32,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct CallQueuePartial {
    /// 担当者が取れなかった行数
    pub missing_contacts: u32,
    /// 会社が取れなかった行数
    pub missing_companies: u32,
    /// 取得に失敗した部分 (`associations` / `contacts` / `companies` / `stage_labels`)。無ければ空
    pub failed: Vec<String>,
    pub excluded: CallQueueExcluded,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueResponse {
    pub items: Vec<CallQueueItem>,
    pub next_cursor: Option<String>,
    /// HubSpot Search の total (参考値。電話番号なし等の後段で外す前の件数)。
    /// 複数の段階にまたがる並びでは全体を数えていないので null
    pub total: Option<u32>,
    /// HubSpot Search の 1 万件上限に達して、これより先を取れないとき true
    pub truncated: bool,
    pub scope: CallQueueScope,
    pub partial: CallQueuePartial,
    pub generated_at: String,
}

// ---------------------------------------------------------------------------
// クエリ
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Due {
    All,
    Today,
}

impl Due {
    fn as_str(self) -> &'static str {
        match self {
            Due::All => "all",
            Due::Today => "today",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Default,
    /// 次回架電日の古い順 (`default` と同じ並び。画面の選択肢として明示できるようにした別名)
    NextCallAsc,
    NextCallDesc,
    LastCallAsc,
    LastCallDesc,
}

impl SortKey {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "default" => SortKey::Default,
            "next_call_asc" => SortKey::NextCallAsc,
            "next_call_desc" => SortKey::NextCallDesc,
            "last_call_asc" => SortKey::LastCallAsc,
            "last_call_desc" => SortKey::LastCallDesc,
            _ => return None,
        })
    }
    fn as_str(self) -> &'static str {
        match self {
            SortKey::Default => "default",
            SortKey::NextCallAsc => "next_call_asc",
            SortKey::NextCallDesc => "next_call_desc",
            SortKey::LastCallAsc => "last_call_asc",
            SortKey::LastCallDesc => "last_call_desc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OwnerParam {
    /// 指定なし (管理者 = all、BPO = me)
    Unspecified,
    Me,
    All,
    Unassigned,
    Id(String),
}

#[derive(Debug, Clone)]
struct Params {
    limit: u32,
    cursor: Option<String>,
    q: Option<String>,
    /// 昇順・重複なし。空 = 許可ステージ全部
    stages: Vec<String>,
    owner: OwnerParam,
    due: Due,
    sort: SortKey,
    next: DateRange,
    last: DateRange,
}

/// 日付 (JST) の範囲。両端を含む。`from <= to` は解析時に確認済み
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DateRange {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

impl DateRange {
    fn start_text(self) -> Option<String> {
        self.from.map(|d| d.format("%Y-%m-%d").to_string())
    }
    fn end_text(self) -> Option<String> {
        self.to.map(|d| d.format("%Y-%m-%d").to_string())
    }
}

/// `YYYY-MM-DD` (ゼロ詰め 10 文字、実在する日付、2000〜2100 年) だけ受け付ける
fn parse_date(v: &str) -> Option<NaiveDate> {
    let b = v.as_bytes();
    let shaped = b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !shaped {
        return None;
    }
    NaiveDate::parse_from_str(v, "%Y-%m-%d")
        .ok()
        .filter(|d| (2000..=2100).contains(&d.year()))
}

/// 不正なパラメータ名を `Err` で返す
fn parse_params(raw: &str) -> Result<Params, &'static str> {
    let url = reqwest::Url::parse(&format!("http://x/?{raw}")).map_err(|_| "query")?;
    let mut limit: Option<u32> = None;
    let mut cursor = None;
    let mut q = None;
    let mut stages: Vec<String> = Vec::new();
    let mut owner = None;
    let mut due = None;
    let mut sort = None;
    let mut dates: [Option<NaiveDate>; 4] = [None; 4];
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut once = |name: &'static str| -> Result<(), &'static str> {
        if seen.insert(name) {
            Ok(())
        } else {
            Err(name)
        }
    };
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "limit" => {
                once("limit")?;
                let n: u32 = v.parse().map_err(|_| "limit")?;
                if !(1..=MAX_LIMIT).contains(&n) {
                    return Err("limit");
                }
                limit = Some(n);
            }
            "cursor" => {
                once("cursor")?;
                if v.is_empty() {
                    return Err("cursor");
                }
                cursor = Some(v.into_owned());
            }
            "q" => {
                once("q")?;
                let t = v.trim();
                if t.chars().count() > MAX_Q_CHARS || t.chars().any(char::is_control) {
                    return Err("q");
                }
                if !t.is_empty() {
                    q = Some(t.to_string());
                }
            }
            "stage" => {
                if !is_allowed_stage(&v) {
                    return Err("stage");
                }
                stages.push(v.into_owned());
            }
            "owner" => {
                once("owner")?;
                owner = Some(match v.as_ref() {
                    "me" => OwnerParam::Me,
                    "all" => OwnerParam::All,
                    "unassigned" => OwnerParam::Unassigned,
                    id if is_valid_id(id) => OwnerParam::Id(id.to_string()),
                    _ => return Err("owner"),
                });
            }
            "due" => {
                once("due")?;
                due = Some(match v.as_ref() {
                    "all" => Due::All,
                    "today" => Due::Today,
                    _ => return Err("due"),
                });
            }
            "sort" => {
                once("sort")?;
                sort = Some(SortKey::parse(&v).ok_or("sort")?);
            }
            "next_from" | "next_to" | "last_from" | "last_to" => {
                let (name, slot): (&'static str, usize) = match k.as_ref() {
                    "next_from" => ("next_from", 0),
                    "next_to" => ("next_to", 1),
                    "last_from" => ("last_from", 2),
                    _ => ("last_to", 3),
                };
                once(name)?;
                dates[slot] = Some(parse_date(&v).ok_or(name)?);
            }
            _ => return Err("unknown"),
        }
    }
    // 範囲の逆転 (from > to) は不正
    if let (Some(a), Some(b)) = (dates[0], dates[1]) {
        if a > b {
            return Err("next_to");
        }
    }
    if let (Some(a), Some(b)) = (dates[2], dates[3]) {
        if a > b {
            return Err("last_to");
        }
    }
    stages.sort();
    stages.dedup();
    Ok(Params {
        limit: limit.unwrap_or(DEFAULT_LIMIT),
        cursor,
        q,
        stages,
        owner: owner.unwrap_or(OwnerParam::Unspecified),
        due: due.unwrap_or(Due::All),
        sort: sort.unwrap_or(SortKey::Default),
        next: DateRange {
            from: dates[0],
            to: dates[1],
        },
        last: DateRange {
            from: dates[2],
            to: dates[3],
        },
    })
}

/// 未指定のときの既定: 管理者 = 全員分、管理者以外の全員 = 自分。指定があればそのまま (全員が全員分・担当なし・他人を指定できる)
fn effective_owner(p: &OwnerParam, role: CrmRole) -> OwnerParam {
    match (p, role) {
        (OwnerParam::Unspecified, r) if r.is_admin() => OwnerParam::All,
        (OwnerParam::Unspecified, _) => OwnerParam::Me,
        (other, _) => other.clone(),
    }
}

fn owner_label(o: &OwnerParam) -> String {
    match o {
        OwnerParam::Unspecified | OwnerParam::All => "all".into(),
        OwnerParam::Me => "me".into(),
        OwnerParam::Unassigned => "unassigned".into(),
        OwnerParam::Id(id) => id.clone(),
    }
}

// ---------------------------------------------------------------------------
// 日付 (JST)
// ---------------------------------------------------------------------------

/// 今日 (JST) の日付を、HubSpot の日付プロパティが使う「その日の UTC 0 時のエポック ms」にする。
/// 例: JST 2026-10-05 23:59 (= UTC 14:59) → 2026-10-05T00:00:00Z、JST 10-06 00:00 → 2026-10-06T00:00:00Z。
pub fn jst_today_ms(now: DateTime<Utc>) -> i64 {
    let jst = now + chrono::Duration::hours(9);
    jst.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis()
}

/// 日付 (JST の暦日) → HubSpot の日付プロパティが使う「その日の UTC 0 時のエポック ms」
fn date_ms(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis()
}

const DAY_MS: i64 = 86_400_000;

/// 日付の範囲 (エポック ms)。次回架電日 `bpo_13`・最終架電日 `bpo_20`
#[derive(Debug, Clone, Copy, Default)]
struct Ranges {
    next_from: Option<i64>,
    next_to: Option<i64>,
    last_from: Option<i64>,
    last_to: Option<i64>,
}

impl Ranges {
    fn of(p: &Params) -> Self {
        Self {
            next_from: p.next.from.map(date_ms),
            next_to: p.next.to.map(date_ms),
            last_from: p.last.from.map(date_ms),
            last_to: p.last.to.map(date_ms),
        }
    }
    fn has_next(&self) -> bool {
        self.next_from.is_some() || self.next_to.is_some()
    }
    fn has_last(&self) -> bool {
        self.last_from.is_some() || self.last_to.is_some()
    }
}

/// `bpo_13` の GTE / LTE。範囲が空 (下端 > 上端) なら `None` (その OR グループは作らない)
fn next_filters(lo: Option<i64>, hi: Option<i64>) -> Option<Vec<Value>> {
    if let (Some(l), Some(h)) = (lo, hi) {
        if l > h {
            return None;
        }
    }
    let mut v = Vec::new();
    if let Some(l) = lo {
        v.push(f_op("bpo_13", "GTE", &l.to_string()));
    }
    if let Some(h) = hi {
        v.push(f_op("bpo_13", "LTE", &h.to_string()));
    }
    Some(v)
}

// ---------------------------------------------------------------------------
// 段階 (Search の組み立て)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// 許可ステージ (未済を含む) で次回架電日が今日以前
    Due,
    /// 未済で、次回架電日が無い or 明日以降 (= Due に入らない未済)
    NotDueUnprocessed,
    /// キュー全体 (未済 全部 + 許可ステージで次回架電日が今日以前)
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastCall {
    Any,
    Missing,
    Present,
}

#[derive(Debug, Clone, Copy)]
struct Phase {
    scope: Scope,
    last: LastCall,
    sort_prop: &'static str,
    desc: bool,
}

fn phases(sort: SortKey, due: Due, last_range: bool) -> Vec<Phase> {
    let p = |scope, last, sort_prop, desc| Phase {
        scope,
        last,
        sort_prop,
        desc,
    };
    let tail = |scope| {
        vec![
            p(scope, LastCall::Missing, "hs_object_id", false),
            p(scope, LastCall::Present, "bpo_20", false),
        ]
    };
    let all = match (sort, due) {
        (SortKey::Default | SortKey::NextCallAsc, Due::All) => {
            let mut v = vec![p(Scope::Due, LastCall::Any, "bpo_13", false)];
            v.extend(tail(Scope::NotDueUnprocessed));
            v
        }
        (SortKey::NextCallDesc, Due::All) => {
            let mut v = vec![p(Scope::Due, LastCall::Any, "bpo_13", true)];
            v.extend(tail(Scope::NotDueUnprocessed));
            v
        }
        (SortKey::Default | SortKey::NextCallAsc, Due::Today) => {
            vec![p(Scope::Due, LastCall::Any, "bpo_13", false)]
        }
        (SortKey::NextCallDesc, Due::Today) => vec![p(Scope::Due, LastCall::Any, "bpo_13", true)],
        (SortKey::LastCallAsc, Due::All) => tail(Scope::Queue),
        (SortKey::LastCallAsc, Due::Today) => tail(Scope::Due),
        (SortKey::LastCallDesc, due) => {
            let scope = if due == Due::All {
                Scope::Queue
            } else {
                Scope::Due
            };
            vec![
                p(scope, LastCall::Present, "bpo_20", true),
                p(scope, LastCall::Missing, "hs_object_id", false),
            ]
        }
    };
    // 最終架電日の範囲を指定したら、最終架電日が空の段階は範囲と矛盾するので出さない
    all.into_iter()
        .filter(|ph| !(last_range && ph.last == LastCall::Missing))
        .collect()
}

fn f_eq(prop: &str, v: &str) -> Value {
    json!({"propertyName": prop, "operator": "EQ", "value": v})
}
fn f_in(prop: &str, vs: &[&str]) -> Value {
    json!({"propertyName": prop, "operator": "IN", "values": vs})
}
fn f_op(prop: &str, op: &str, v: &str) -> Value {
    json!({"propertyName": prop, "operator": op, "value": v})
}
fn f_has(prop: &str, has: bool) -> Value {
    json!({"propertyName": prop, "operator": if has { "HAS_PROPERTY" } else { "NOT_HAS_PROPERTY" }})
}

/// 段階の OR グループ (各グループは AND)。該当するステージが無ければ空。
/// グループあたりのフィルタ数は最大 6 (HubSpot の上限)。範囲の指定でフィルタが増えて 6 に収まらないときは、
/// 架電禁止理由・ブロック理由の NOT_HAS_PROPERTY を Search に入れず、返ってきた行を後段で外す
/// (`execute` の後段確認。外した件数は `partial.excluded.stop_reason`)。
fn filter_groups(
    phase: Phase,
    stages: &[&str],
    owner: Option<&Value>,
    today_ms: i64,
    r: &Ranges,
) -> Vec<Value> {
    let today = today_ms.to_string();
    let has_unp = stages.contains(&STAGE_UNPROCESSED);
    let others: Vec<&str> = stages
        .iter()
        .copied()
        .filter(|s| *s != STAGE_UNPROCESSED)
        .collect();
    let finish = |mut fs: Vec<Value>| -> Value {
        if r.has_last() {
            // 範囲は「最終架電日が入っている」ことを含む
            if let Some(l) = r.last_from {
                fs.push(f_op("bpo_20", "GTE", &l.to_string()));
            }
            if let Some(h) = r.last_to {
                fs.push(f_op("bpo_20", "LTE", &h.to_string()));
            }
        } else {
            match phase.last {
                LastCall::Any => {}
                LastCall::Missing => fs.push(f_has("bpo_20", false)),
                LastCall::Present => fs.push(f_has("bpo_20", true)),
            }
        }
        if fs.len() + 2 + usize::from(owner.is_some()) <= 6 {
            fs.push(f_has("bpo_3", false));
            fs.push(f_has("bpo_4", false));
        }
        if let Some(o) = owner {
            fs.push(o.clone());
        }
        debug_assert!(fs.len() <= 6);
        json!({ "filters": fs })
    };
    // 次回架電日が今日以前 (範囲があればその中) のグループ
    let due_group = |stage: Value| -> Option<Value> {
        let hi = Some(r.next_to.map_or(today_ms, |t| t.min(today_ms)));
        let mut fs = vec![stage];
        fs.extend(next_filters(r.next_from, hi)?);
        Some(finish(fs))
    };
    let mut groups = Vec::new();
    match phase.scope {
        Scope::Due => {
            if !stages.is_empty() {
                groups.extend(due_group(f_in("dealstage", stages)));
            }
        }
        Scope::NotDueUnprocessed => {
            if has_unp {
                if r.has_next() {
                    // 範囲を指定すると次回日が入っていることが前提。明日以降の部分だけ残る
                    let lo = r
                        .next_from
                        .map_or(today_ms + DAY_MS, |f| f.max(today_ms + DAY_MS));
                    if let Some(nf) = next_filters(Some(lo), r.next_to) {
                        let mut fs = vec![f_eq("dealstage", STAGE_UNPROCESSED)];
                        fs.extend(nf);
                        groups.push(finish(fs));
                    }
                } else {
                    groups.push(finish(vec![
                        f_eq("dealstage", STAGE_UNPROCESSED),
                        f_has("bpo_13", false),
                    ]));
                    groups.push(finish(vec![
                        f_eq("dealstage", STAGE_UNPROCESSED),
                        f_op("bpo_13", "GT", &today),
                    ]));
                }
            }
        }
        Scope::Queue => {
            if has_unp {
                if let Some(nf) = next_filters(r.next_from, r.next_to) {
                    let mut fs = vec![f_eq("dealstage", STAGE_UNPROCESSED)];
                    fs.extend(nf);
                    groups.push(finish(fs));
                }
            }
            if !others.is_empty() {
                groups.extend(due_group(f_in("dealstage", &others)));
            }
        }
    }
    groups
}

fn search_body(
    groups: Vec<Value>,
    phase: Phase,
    limit: u32,
    after: Option<u64>,
    q: Option<&str>,
) -> Value {
    let mut body = json!({
        "filterGroups": groups,
        "sorts": [{
            "propertyName": phase.sort_prop,
            "direction": if phase.desc { "DESCENDING" } else { "ASCENDING" },
        }],
        "properties": DEAL_PROPERTIES,
        "limit": limit,
    });
    if let Some(a) = after {
        body["after"] = json!(a.to_string());
    }
    if let Some(q) = q {
        body["query"] = json!(q);
    }
    body
}

// ---------------------------------------------------------------------------
// cursor (署名付き。条件・本人・日付に束縛)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct CursorPayload {
    /// 条件のハッシュ
    h: String,
    /// 段階の番号
    p: u32,
    /// Search の after (段階の先頭なら null)
    a: Option<u64>,
    /// 発行時刻 (UNIX 秒)
    t: i64,
}

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 条件のハッシュ。本人・役割・条件・今日 (JST) が変われば別の値になり、cursor は使えなくなる。
fn condition_hash(
    email: &str,
    role: CrmRole,
    params: &Params,
    owner: &OwnerParam,
    today_ms: i64,
) -> String {
    let parts = [
        "v2".to_string(),
        email.trim().to_lowercase(),
        role.as_str().to_string(),
        params.limit.to_string(),
        params.q.clone().unwrap_or_default(),
        params.stages.join(","),
        owner_label(owner),
        params.due.as_str().to_string(),
        params.sort.as_str().to_string(),
        today_ms.to_string(),
        params.next.start_text().unwrap_or_default(),
        params.next.end_text().unwrap_or_default(),
        params.last.start_text().unwrap_or_default(),
        params.last.end_text().unwrap_or_default(),
    ];
    hex(&Sha256::digest(parts.join("\u{1f}").as_bytes()))
}

impl CallQueueState {
    fn sign_cursor(&self, hash: &str, phase: u32, after: Option<u64>) -> String {
        let payload = CursorPayload {
            h: hash.to_string(),
            p: phase,
            a: after,
            t: self.now().timestamp(),
        };
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("cursor json"));
        let mac = hmac_sha256(&self.key, body.as_bytes());
        format!("{body}.{}", URL_SAFE_NO_PAD.encode(mac))
    }

    /// 署名・条件・有効期限を確かめる。通らなければ `None` (400 `cursor_mismatch`)。
    fn verify_cursor(&self, cursor: &str, hash: &str) -> Option<(u32, Option<u64>)> {
        let (body, sig) = cursor.split_once('.')?;
        let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let want = hmac_sha256(&self.key, body.as_bytes());
        if !ct_eq(&sig, &want) {
            return None;
        }
        let payload: CursorPayload =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
        let age = self.now().timestamp() - payload.t;
        if payload.h != hash || !(-300..=CURSOR_TTL_SECS).contains(&age) {
            return None;
        }
        Some((payload.p, payload.a))
    }
}

// ---------------------------------------------------------------------------
// Search 応答 / 行の組み立て
// ---------------------------------------------------------------------------

struct SearchPage {
    deals: Vec<HubSpotRecord>,
    total: u64,
    next_after: Option<u64>,
}

fn prop_string(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn parse_search(v: &Value) -> Result<SearchPage, HubSpotError> {
    let results = v
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| HubSpotError::Decode("search without results".into()))?;
    let mut deals = Vec::with_capacity(results.len());
    for r in results {
        let id = match r.get("id") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => return Err(HubSpotError::Decode("deal without id".into())),
        };
        let properties: BTreeMap<String, Option<String>> = match r.get("properties") {
            Some(Value::Object(m)) => m.iter().map(|(k, v)| (k.clone(), prop_string(v))).collect(),
            _ => BTreeMap::new(),
        };
        deals.push(HubSpotRecord {
            id,
            properties,
            created_at: None,
            updated_at: None,
            archived: r.get("archived").and_then(Value::as_bool).unwrap_or(false),
        });
    }
    let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
    let next_after = match v.pointer("/paging/next/after") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(
            s.parse::<u64>()
                .map_err(|_| HubSpotError::Decode("search after".into()))?,
        ),
        Some(Value::Number(n)) => n.as_u64(),
        Some(_) => return Err(HubSpotError::Decode("search after".into())),
    };
    Ok(SearchPage {
        deals,
        total,
        next_after,
    })
}

/// 空白だけの値は「入力なし」として扱う
pub(super) fn nz(rec: &HubSpotRecord, key: &str) -> Option<String> {
    rec.properties
        .get(key)
        .and_then(|v| v.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(super) fn pick<'a>(
    refs: &'a [AssociationRef],
    primary_labels: &[&str],
) -> Option<&'a AssociationRef> {
    refs.iter()
        .find(|r| r.labels.iter().any(|l| primary_labels.contains(&l.trim())))
        .or_else(|| refs.first())
}

pub(super) fn contact_name(rec: &HubSpotRecord) -> Option<String> {
    let last = nz(rec, "lastname").unwrap_or_default();
    let first = nz(rec, "firstname").unwrap_or_default();
    let name = format!("{last} {first}").trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// 関連の取得状況
struct Related {
    contacts_known: bool,
    companies_known: bool,
    contact_refs: BTreeMap<String, Vec<AssociationRef>>,
    company_refs: BTreeMap<String, Vec<AssociationRef>>,
    contacts: HashMap<String, HubSpotRecord>,
    companies: HashMap<String, HubSpotRecord>,
}

async fn load_related(
    client: &HubSpotClient,
    deal_ids: &[String],
    failed: &mut Vec<String>,
) -> Related {
    let (c_assoc, co_assoc) = tokio::join!(
        client.batch_associations("deals", "contacts", deal_ids),
        client.batch_associations("deals", "companies", deal_ids)
    );
    let mut note = |name: &str, e: &HubSpotError| {
        tracing::warn!(
            error_kind = e.error_kind(),
            part = name,
            "call queue: partial failure"
        );
        if !failed.iter().any(|f| f == name) {
            failed.push(name.to_string());
        }
    };
    let (contact_refs, contacts_assoc_ok) = match c_assoc {
        Ok(m) => (m, true),
        Err(e) => {
            note("associations", &e);
            (BTreeMap::new(), false)
        }
    };
    let (company_refs, companies_assoc_ok) = match co_assoc {
        Ok(m) => (m, true),
        Err(e) => {
            note("associations", &e);
            (BTreeMap::new(), false)
        }
    };
    // 1 Deal につき読むのは 1 人 / 1 社 (主 → なければ最初)。重複する ID は 1 回だけ読む
    let uniq = |refs: &BTreeMap<String, Vec<AssociationRef>>, labels: &[&str]| -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for id in deal_ids {
            if let Some(r) = refs.get(id).and_then(|v| pick(v, labels)) {
                if seen.insert(r.id.clone()) {
                    out.push(r.id.clone());
                }
            }
        }
        out
    };
    let contact_ids = uniq(&contact_refs, &["主"]);
    let company_ids = uniq(&company_refs, &["主", "Primary"]);
    let read = |object: &'static str,
                ids: Vec<String>,
                props: &'static [&'static str],
                ok: bool| async move {
        if !ok || ids.is_empty() {
            return Ok(Vec::new());
        }
        client.batch_read(object, &ids, props).await
    };
    let (c_read, co_read) = tokio::join!(
        read("contacts", contact_ids, CONTACT_PROPS, contacts_assoc_ok),
        read("companies", company_ids, COMPANY_PROPS, companies_assoc_ok)
    );
    let mut contacts_known = contacts_assoc_ok;
    let mut companies_known = companies_assoc_ok;
    let contacts = match c_read {
        Ok(v) => v,
        Err(e) => {
            note("contacts", &e);
            contacts_known = false;
            Vec::new()
        }
    };
    let companies = match co_read {
        Ok(v) => v,
        Err(e) => {
            note("companies", &e);
            companies_known = false;
            Vec::new()
        }
    };
    Related {
        contacts_known,
        companies_known,
        contact_refs,
        company_refs,
        contacts: contacts
            .into_iter()
            .filter(|r| !r.archived)
            .map(|r| (r.id.clone(), r))
            .collect(),
        companies: companies
            .into_iter()
            .filter(|r| !r.archived)
            .map(|r| (r.id.clone(), r))
            .collect(),
    }
}

/// 1 Deal 分の行を作る。電話番号がどこにも無く、かつ関連を読めていたなら `None` (外す)。
fn build_item(
    deal: &HubSpotRecord,
    rel: &Related,
    labels: &HashMap<String, String>,
    portal: &str,
    no_phone: &mut u32,
) -> Option<CallQueueItem> {
    let contact_refs = rel
        .contact_refs
        .get(&deal.id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let contact_rec = pick(contact_refs, &["主"]).and_then(|r| rel.contacts.get(&r.id));
    let company_refs = rel
        .company_refs
        .get(&deal.id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let company_rec = pick(company_refs, &["主", "Primary"]).and_then(|r| rel.companies.get(&r.id));

    let candidates = [
        ("deal", nz(deal, "bpo_29")),
        ("contact", contact_rec.and_then(|c| nz(c, "phone"))),
        ("mobile", contact_rec.and_then(|c| nz(c, "mobilephone"))),
        ("company", company_rec.and_then(|c| nz(c, "phone"))),
    ];
    let found = candidates
        .into_iter()
        .find_map(|(src, v)| v.map(|v| (src, v)));
    if found.is_none() && rel.contacts_known && rel.companies_known {
        *no_phone += 1;
        return None;
    }
    let (phone_source, phone) = match found {
        Some((s, v)) => (Some(s.to_string()), Some(v)),
        None => (None, None),
    };
    let stage_id = nz(deal, "dealstage").unwrap_or_default();
    let contact = contact_rec.map(|c| CallQueueContact {
        id: c.id.clone(),
        name: contact_name(c),
        phone: nz(c, "phone"),
        mobile: nz(c, "mobilephone"),
        job_title: nz(c, "jobtitle"),
        extra_count: contact_refs.len().saturating_sub(1) as u32,
    });
    let company = company_rec.map(|c| CallQueueCompany {
        id: c.id.clone(),
        name: nz(c, "name"),
        phone: nz(c, "phone"),
    });
    Some(CallQueueItem {
        deal_id: deal.id.clone(),
        deal_name: nz(deal, "dealname"),
        stage_label: labels.get(&stage_id).cloned(),
        stage_id,
        owner_id: nz(deal, "hubspot_owner_id"),
        next_call_date: nz(deal, "bpo_13"),
        next_call_time: nz(deal, "bpo_14"),
        last_call_date: nz(deal, "bpo_20"),
        stop: CallQueueStop {
            prohibited_reason: nz(deal, "bpo_3"),
            block_reason: nz(deal, "bpo_4"),
            unreachable_check: nz(deal, "bpo_10"),
        },
        deep_links: CallQueueDeepLinks {
            deal: record_url(portal, RecordType::Deal, &deal.id),
            contact: contact
                .as_ref()
                .map(|c| record_url(portal, RecordType::Contact, &c.id)),
            company: company
                .as_ref()
                .map(|c| record_url(portal, RecordType::Company, &c.id)),
        },
        contact,
        company,
        phone,
        phone_source,
    })
}

// ---------------------------------------------------------------------------
// 1 件の Deal がキューの条件に合うか (BPO のレコード単位の制限用。`record_gate.rs`)
// ---------------------------------------------------------------------------

/// 日付プロパティ (`bpo_13` 等) → エポック ms。`YYYY-MM-DD` (その日の UTC 0 時)・RFC 3339・数字 (ms) を受ける。
/// 解釈できなければ `None` (呼び出し側は「条件に合わない」側に倒す)。
fn date_prop_ms(v: &str) -> Option<i64> {
    let t = v.trim();
    if t.is_empty() {
        return None;
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        return t.parse::<i64>().ok();
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some(date_ms(d));
    }
    DateTime::parse_from_rfc3339(t)
        .ok()
        .map(|d| d.timestamp_millis())
}

/// この Deal が `owner_id` (HubSpot owner) の架電キューに出る条件を満たすか。
///
/// 検索 (`filter_groups`) と同じ条件を 1 件の Deal に当てる: パイプライン・担当者・アーカイブでない・
/// 架電禁止理由 `bpo_3` / ブロック理由 `bpo_4` が空・ステージ (未済は常に、他の許可ステージは次回架電日が今日以前)。
/// **電話番号の有無は見ない** (Contact / Company の追加読み取りが要るため。電話番号が無い自分の担当 Deal は
/// キューには出ないが、個別取得はできる)。
pub(super) fn deal_in_queue(deal: &HubSpotRecord, owner_id: &str, today_ms: i64) -> bool {
    if deal.archived || owner_id.trim().is_empty() {
        return false;
    }
    if nz(deal, "pipeline").as_deref() != Some(PIPELINE_ID) {
        return false;
    }
    if nz(deal, "hubspot_owner_id").as_deref() != Some(owner_id.trim()) {
        return false;
    }
    if nz(deal, "bpo_3").is_some() || nz(deal, "bpo_4").is_some() {
        return false;
    }
    let Some(stage) = nz(deal, "dealstage") else {
        return false;
    };
    if stage == STAGE_UNPROCESSED {
        return true;
    }
    if !STAGES_WHEN_DUE.contains(&stage.as_str()) {
        return false;
    }
    nz(deal, "bpo_13")
        .as_deref()
        .and_then(date_prop_ms)
        .is_some_and(|ms| ms <= today_ms)
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn bad_param(name: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(CrmErrorResponse {
            error_kind: "invalid_param".to_string(),
            message: Some(format!("クエリの {name} が不正です")),
        }),
    )
        .into_response()
}

pub(super) async fn get_call_queue(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    RawQuery(raw): RawQuery,
) -> Response {
    // 1) 認可 (HubSpot の設定有無より先)
    let principal =
        match rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await {
            Ok(p) => p,
            Err(denied) => return denied.into_response(),
        };
    // 2) クエリ
    let params = match parse_params(raw.as_deref().unwrap_or("")) {
        Ok(p) => p,
        Err(name) => return bad_param(name),
    };
    // 3) 役割。担当者の指定は誰でも自由 (全員分・担当なし・他人も可)。未指定のときの既定だけ役割で変わる
    let role = rbac::resolve_role(&principal);
    let owner = effective_owner(&params.owner, role);
    let email = principal.email.clone().unwrap_or_default();
    // 4) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 5) cursor (署名・条件・有効期限。HubSpot は呼ばない)
    let now = ctx.queue.now();
    let today_ms = jst_today_ms(now);
    let hash = condition_hash(&email, role, &params, &owner, today_ms);
    let start = match params.cursor.as_deref() {
        None => (0u32, None),
        Some(c) => match ctx.queue.verify_cursor(c, &hash) {
            Some(s) => s,
            None => return error_json(StatusCode::BAD_REQUEST, "cursor_mismatch"),
        },
    };
    // 6) 同時実行の枠 + 全体の締め切り
    let started = Instant::now();
    let _slot = match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.read_slots.acquire()).await {
        Ok(Ok(p)) => p,
        _ => return timeout_response(),
    };
    let remaining = CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed());
    let run = execute(
        &client, &ctx, &params, role, &owner, &email, &hash, start, today_ms, now,
    );
    match tokio::time::timeout(remaining, run).await {
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "call queue timed out");
            timeout_response()
        }
        Ok(Ok(resp)) => Json(resp).into_response(),
        Ok(Err(resp)) => resp,
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    params: &Params,
    role: CrmRole,
    owner: &OwnerParam,
    email: &str,
    hash: &str,
    start: (u32, Option<u64>),
    today_ms: i64,
    now: DateTime<Utc>,
) -> Result<CallQueueResponse, Response> {
    let fail = |e: HubSpotError| -> Response {
        tracing::warn!(error_kind = e.error_kind(), "call queue read failed");
        hubspot_error_response(&e)
    };
    // 担当者の絞り込み
    let owner_filter: Option<Value> = match owner {
        OwnerParam::All | OwnerParam::Unspecified => None,
        OwnerParam::Unassigned => Some(f_has("hubspot_owner_id", false)),
        OwnerParam::Id(id) => Some(f_eq("hubspot_owner_id", id)),
        OwnerParam::Me => match ctx.queue.owner_for(client, email).await.map_err(&fail)? {
            Some(id) => Some(f_eq("hubspot_owner_id", &id)),
            // 引けない人を全員分に倒さない。画面は所有者の選択を促す (403 ではない)
            None => return Err(error_json(StatusCode::CONFLICT, "owner_not_resolved")),
        },
    };
    // 所属チーム名 (参考表示だけ。owner のキャッシュを使うので通常は HubSpot を呼ばない)
    let teams: Vec<String> = if role.is_admin() || !matches!(owner, OwnerParam::Me) {
        Vec::new()
    } else {
        ctx.queue
            .owner_info_for(client, email)
            .await
            .map_err(&fail)?
            .map(|o| o.teams)
            .unwrap_or_default()
    };
    // 段階 (ステージの絞り込み後に検索するものがあるものだけ)
    let selected: Vec<&str> = if params.stages.is_empty() {
        std::iter::once(STAGE_UNPROCESSED)
            .chain(STAGES_WHEN_DUE)
            .collect()
    } else {
        params.stages.iter().map(String::as_str).collect()
    };
    let ranges = Ranges::of(params);
    let active: Vec<(Phase, Vec<Value>)> = phases(params.sort, params.due, ranges.has_last())
        .into_iter()
        .map(|p| {
            (
                p,
                filter_groups(p, &selected, owner_filter.as_ref(), today_ms, &ranges),
            )
        })
        .filter(|(_, g)| !g.is_empty())
        .collect();
    let (start_phase, start_after) = start;
    // 発行した cursor は必ず存在する段階を指す。段階の数が合わないものは使えない
    if params.cursor.is_some() && start_phase as usize >= active.len() {
        return Err(error_json(StatusCode::BAD_REQUEST, "cursor_mismatch"));
    }

    let mut phase_idx = start_phase as usize;
    let mut after = start_after;
    let mut totals: Vec<u64> = Vec::new();
    let mut visited = 0usize;
    let mut truncated = false;
    let mut next_cursor: Option<String> = None;
    let mut page_deals: Vec<HubSpotRecord> = Vec::new();

    while phase_idx < active.len() {
        let (phase, groups) = &active[phase_idx];
        let body = search_body(
            groups.clone(),
            *phase,
            params.limit,
            after,
            params.q.as_deref(),
        );
        let v = client.search("deals", body).await.map_err(&fail)?;
        let page = parse_search(&v).map_err(&fail)?;
        visited += 1;
        totals.push(page.total);
        // 続きがあり、次の取得が Search の窓 (after + limit ≤ 1 万) に収まるときだけ同じ段階を続ける
        let more = match page.next_after {
            Some(n) if n + u64::from(params.limit) <= SEARCH_WINDOW => Some(n),
            Some(_) => {
                truncated = true;
                None
            }
            None => None,
        };
        let got_any = !page.deals.is_empty();
        page_deals = page.deals;
        if let Some(n) = more {
            next_cursor = Some(ctx.queue.sign_cursor(hash, phase_idx as u32, Some(n)));
            break;
        }
        // この段階は終わり。次の段階へ
        phase_idx += 1;
        after = None;
        // 空の段階は同じ要求の中で次へ進む (空ページを返さない)
        if phase_idx < active.len() && got_any {
            next_cursor = Some(ctx.queue.sign_cursor(hash, phase_idx as u32, None));
            break;
        }
    }

    // 総数は、段階が 1 つ or 先頭から全段階を数えたときだけ出す (一部の段階の件数を全体と偽らない)
    let known_all =
        active.len() == 1 || (start_phase == 0 && start_after.is_none() && visited == active.len());
    let total = known_all.then(|| totals.iter().sum::<u64>().min(u64::from(u32::MAX)) as u32);

    // 後段の確認: アーカイブ・別パイプライン・許可外ステージ・停止系
    let mut partial = CallQueuePartial::default();
    let mut deals: Vec<HubSpotRecord> = Vec::new();
    for d in page_deals {
        let in_scope = !d.archived
            && nz(&d, "pipeline").as_deref() == Some(PIPELINE_ID)
            && nz(&d, "dealstage").is_some_and(|s| is_allowed_stage(&s));
        if !in_scope {
            partial.excluded.out_of_scope += 1;
        } else if nz(&d, "bpo_3").is_some() || nz(&d, "bpo_4").is_some() {
            partial.excluded.stop_reason += 1;
        } else {
            deals.push(d);
        }
    }

    let mut items = Vec::with_capacity(deals.len());
    if !deals.is_empty() {
        let deal_ids: Vec<String> = deals.iter().map(|d| d.id.clone()).collect();
        let mut failed: Vec<String> = Vec::new();
        let (rel, labels) = tokio::join!(
            load_related(client, &deal_ids, &mut failed),
            ctx.queue.stage_labels(client)
        );
        let labels = match labels {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    error_kind = e.error_kind(),
                    "call queue: stage labels unavailable"
                );
                failed.push("stage_labels".to_string());
                HashMap::new()
            }
        };
        let portal = hubspot_portal_id();
        for d in &deals {
            if let Some(item) =
                build_item(d, &rel, &labels, &portal, &mut partial.excluded.no_phone)
            {
                items.push(item);
            }
        }
        partial.missing_contacts = items.iter().filter(|i| i.contact.is_none()).count() as u32;
        partial.missing_companies = items.iter().filter(|i| i.company.is_none()).count() as u32;
        partial.failed = failed;
    }

    Ok(CallQueueResponse {
        items,
        next_cursor,
        total,
        truncated,
        scope: CallQueueScope {
            owner: owner_label(owner),
            role: if role.is_admin() { "admin" } else { "own" }.to_string(),
            teams,
            stages: {
                let mut s: Vec<String> = selected.iter().map(|s| s.to_string()).collect();
                s.sort();
                s
            },
            due: params.due.as_str().to_string(),
            sort: params.sort.as_str().to_string(),
            q: params.q.clone(),
            limit: params.limit,
            next_from: params.next.start_text(),
            next_to: params.next.end_text(),
            last_from: params.last.start_text(),
            last_to: params.last.end_text(),
        },
        partial,
        generated_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}
