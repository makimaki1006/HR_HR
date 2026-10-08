//! `GET /api/crm/call-queue` (架電キュー。HubSpot からの読み取りだけ。書き込みはしない)。
//!
//! 設計: `claudedocs/CRM_CALL_QUEUE_DESIGN_2026-10-04.md`、確定条件: `claudedocs/REACT_HANDOVER_2026-10-05.md` §4.5。
//!
//! ## キューの定義 (確定条件)
//! - 選んだパイプライン (`pipeline`。既定は bpo_リクロジ `753186575`) の Deal。選べるパイプラインとステージごとの決まりは
//!   `queue_pipelines.rs` の表 1 箇所 (ユーザー決定 2026-10-08)。表に無いパイプラインは 400 `invalid_param`。
//!   決まりが `all` のステージ (未済など) は全部、`due` のステージは次回架電日 `bpo_13` が今日 (JST) 以前のときだけ、
//!   `exclude` (アポ日確定・架電禁止など) と表に無いステージは出さない。表に無いステージの数は `partial.unknown_stages`
//! - `stage` は選んだパイプラインの `all` / `due` のステージだけ受け付ける (それ以外は 400)。指定なし = その全部
//! - 架電禁止理由 `bpo_3`・ブロック理由 `bpo_4` が入っている Deal は外す。不通時チェック `bpo_10` は残して印を付ける
//! - 電話番号がどこにも無い Deal は外す (`bpo_29` → 担当者 phone → 担当者 mobilephone → 会社 phone)
//! - BPO は自分が担当の Deal だけ。管理者の既定は全員分 (`owner=all`)。担当なしは管理者だけ
//!
//! ## 取得の組み立て (行ごとに API を呼ばない)
//! 1 ページ = Search 1 + 関連 2 (`deal→contact` / `deal→company`) + 読み取り 2 (contact / company) = 5 回
//! (件数に依存しない。ステージ名のキャッシュが冷えているときだけ +1 (行が 0 件でも、表に無いステージを数えるために読む)。BPO の owner 対応が未取得のときは +1)。
//! 複数の段階にまたがる並びの先頭ページだけ、辿らなかった段階の件数を数える Search (limit 1) を段階ごとに足す (最大 +2。総数の表示用)。
//! 1 Deal につき読む Contact は 1 人 (主 → なければ最初の 1 人) なので、読み取りは 50 件 (limit 上限) で 1 バッチに収まる。
//!
//! ## 並び (`sort`) と「段階」
//! HubSpot の Search は 1 回に 1 つのプロパティでしか並べられず、空の値の並び位置も保証されない。
//! そこで並びを複数の「段階」(それぞれ別の Search) に分け、段階を順に辿る。cursor は (段階, after) を署名して持つ。
//! 例: 既定 = ① 次回架電日が今日以前 (次回日の古い順) → ② 未済 (`all` のステージ) で未架電 (最終架電日なし) → ③ 同じく最終架電日の古い順。
//! `all` のステージが無いパイプライン (商談済リードなど) は ① だけになる。
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
//! OR グループ最大 2、グループあたり最大 6、全体最大 12。ステージは IN (`all` のステージが 1 つだけのグループは EQ) の
//! 1 フィルタにまとめる (選んだステージの数でフィルタ数は変わらない)。`pipeline` の絞り込みはグループあたり 6 に収めるため
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

use super::queue_cache::{
    QueueCountKey, QueueFilterKey, QueuePageKey, TtlCache, QUEUE_COUNT_MAX, QUEUE_COUNT_TTL,
    QUEUE_PAGE_MAX, QUEUE_PAGE_TTL,
};
use super::queue_pipelines::{
    default_pipeline, find_pipeline, QueuePipeline, StageRule, QUEUE_PIPELINES,
};
use super::rbac::{self, CrmRole};
use super::routes::{
    error_json, hubspot_error_response, is_valid_id, timeout_response, CrmCtx, CrmErrorResponse,
    CRM_REQUEST_DEADLINE,
};
use crate::handlers::crm_metadata::CrmPipeline;
use crate::hubspot::deep_link::{hubspot_portal_id, record_url};
use crate::hubspot::gateway::{cache_hit, cache_miss};
use crate::hubspot::{
    AssociationRef, HubSpotClient, HubSpotError, HubSpotRecord, OwnerRef, RecordType,
};
use crate::AppState;

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
    /// HubSpot の Deal パイプライン定義 (全パイプライン。表示名と、表に無いステージの判定に使う)
    labels: tokio::sync::Mutex<Option<(Instant, Arc<Vec<CrmPipeline>>)>>,
    /// 管理者向けの担当者一覧 (`GET /api/crm/owners`)
    pub(super) owner_list: super::owners::OwnerListCache,
    /// 1 ページの中身 (30 秒。解決済みの担当者をキーに含む。`queue_cache.rs`)
    pages: TtlCache<QueuePageKey, Arc<QueueCore>>,
    /// 段階の件数 (60 秒)
    counts: TtlCache<QueueCountKey, u64>,
    /// パイプライン定義の先読み (背景の優先度) が走っているか
    labels_refreshing: RefreshGate,
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
            pages: TtlCache::new(QUEUE_PAGE_TTL, QUEUE_PAGE_MAX),
            counts: TtlCache::new(QUEUE_COUNT_TTL, QUEUE_COUNT_MAX),
            labels_refreshing: RefreshGate::new(),
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
                    cache_hit("owner_by_email");
                    return Ok(v.clone());
                }
            }
        }
        cache_miss("owner_by_email");
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

    /// HubSpot の Deal パイプライン定義 (5 分キャッシュ。同時に冷えた要求は 1 回の取得にまとめる)
    pub(super) async fn pipeline_defs(
        &self,
        client: &HubSpotClient,
    ) -> Result<Arc<Vec<CrmPipeline>>, HubSpotError> {
        let mut slot = self.labels.lock().await;
        if let Some((at, m)) = slot.as_ref() {
            if at.elapsed() < LABELS_TTL {
                cache_hit("pipelines");
                return Ok(m.clone());
            }
        }
        cache_miss("pipelines");
        let v = client.deal_pipelines().await?;
        let pipelines = Arc::new(crate::handlers::crm_metadata::parse_pipelines(&v)?);
        *slot = Some((Instant::now(), pipelines.clone()));
        Ok(pipelines)
    }

    /// パイプライン定義が有効期間の終わり近く (残り 20% 未満) なら true (先読みの対象)
    pub(super) fn pipeline_defs_refresh_due(&self) -> bool {
        match self.labels.try_lock() {
            Ok(slot) => slot
                .as_ref()
                .is_some_and(|(at, _)| refresh_due(at.elapsed(), LABELS_TTL)),
            // 取得中 (ロック中) は先読みしない
            Err(_) => false,
        }
    }

    /// パイプライン定義を読み直して置き換える (先読み。呼び出し側は背景の優先度のクライアントを渡す)。
    /// 読んでいる間もロックは持たない (画面の要求は今のキャッシュを使い続ける)。同時には 1 本だけ
    pub(super) async fn refresh_pipeline_defs(&self, client: &HubSpotClient) {
        if !self.labels_refreshing.try_begin() {
            return;
        }
        let read = async {
            let v = client.deal_pipelines().await?;
            crate::handlers::crm_metadata::parse_pipelines(&v)
        };
        match read.await {
            Ok(p) => *self.labels.lock().await = Some((Instant::now(), Arc::new(p))),
            Err(e) => tracing::warn!(
                error_kind = e.error_kind(),
                "call queue: pipeline definitions refresh-ahead failed"
            ),
        }
        self.labels_refreshing.end();
    }

    /// ステージ ID → 表示名 (全パイプライン。ステージ ID は HubSpot 内で一意)。[`Self::pipeline_defs`] のキャッシュを使う
    pub(super) async fn stage_labels(
        &self,
        client: &HubSpotClient,
    ) -> Result<HashMap<String, String>, HubSpotError> {
        Ok(stage_label_map(&self.pipeline_defs(client).await?))
    }
}

/// 先読みに失敗したあと、次の先読みを始めるまでの最短間隔 (失敗が続くときに要求ごとに読み直さない)
pub(super) const REFRESH_RETRY_GAP: Duration = Duration::from_secs(30);

/// 先読みの関門: 同時に 1 本だけ、前回の開始から [`REFRESH_RETRY_GAP`] 以上空ける
pub struct RefreshGate {
    running: std::sync::atomic::AtomicBool,
    last_start: Mutex<Option<Instant>>,
}

impl RefreshGate {
    pub fn new() -> Self {
        Self {
            running: std::sync::atomic::AtomicBool::new(false),
            last_start: Mutex::new(None),
        }
    }

    /// 始めてよければ true (呼び出し側は終わったら [`Self::end`] を呼ぶ)
    pub fn try_begin(&self) -> bool {
        use std::sync::atomic::Ordering;
        if self.running.swap(true, Ordering::AcqRel) {
            return false;
        }
        let Ok(mut last) = self.last_start.lock() else {
            self.running.store(false, Ordering::Release);
            return false;
        };
        if last.is_some_and(|t| t.elapsed() < REFRESH_RETRY_GAP) {
            self.running.store(false, Ordering::Release);
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    pub fn end(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

impl Default for RefreshGate {
    fn default() -> Self {
        Self::new()
    }
}

/// 先読みの対象か: 有効期間の残りが 20% 未満 (まだ有効なうちに背景で読み直す)
pub(super) fn refresh_due(age: Duration, ttl: Duration) -> bool {
    age < ttl && age.saturating_mul(5) >= ttl.saturating_mul(4)
}

fn stage_label_map(defs: &[CrmPipeline]) -> HashMap<String, String> {
    defs.iter()
        .flat_map(|p| p.stages.iter())
        .map(|s| (s.id.clone(), s.label.clone()))
        .collect()
}

/// HubSpot のパイプラインにあって、表 (`queue_pipelines.rs`) に無いステージ (表示順)
fn unknown_stages<'a>(
    defs: &'a [CrmPipeline],
    pipeline: &QueuePipeline,
) -> Vec<&'a crate::handlers::crm_metadata::CrmStage> {
    defs.iter()
        .filter(|p| p.id == pipeline.id)
        .flat_map(|p| p.stages.iter())
        .filter(|s| pipeline.rule(&s.id).is_none())
        .collect()
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
    /// 実際に使ったパイプライン ID (指定なしは既定の bpo_リクロジ)
    pub pipeline: String,
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
    /// 選んだパイプラインにあって、架電キューの表に無いステージの数 (後から HubSpot に追加されたもの)。
    /// 架電対象外として扱い、検索しない。ステージ名を読めなかったときは 0
    pub unknown_stages: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueResponse {
    pub items: Vec<CallQueueItem>,
    pub next_cursor: Option<String>,
    /// HubSpot Search の total (参考値。電話番号なし等の後段で外す前の件数)。
    /// 複数の段階にまたがる並びでは、先頭ページでだけ残りの段階を数えて全体を出す。
    /// 2 ページ目以降と、数えられなかったときは null
    pub total: Option<u32>,
    /// HubSpot Search の 1 万件上限に達して、これより先を取れないとき true
    pub truncated: bool,
    pub scope: CallQueueScope,
    pub partial: CallQueuePartial,
    pub generated_at: String,
}

/// `GET /api/crm/call-queue/pipelines` のステージ 1 つ
#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueueStageOption {
    pub id: String,
    /// HubSpot の表示名。読めなければ null
    pub label: Option<String>,
    /// `all` (常に出す) / `due` (次回架電日が今日以前のときだけ) / `exclude` (出さない。表に無いステージも)
    pub rule: String,
}

/// `GET /api/crm/call-queue/pipelines` のパイプライン 1 つ
#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueuePipelineOption {
    pub id: String,
    /// HubSpot の表示名。読めなければ null (画面は表の呼び名を使う)
    pub label: Option<String>,
    /// 表にあるステージ (表の順)
    pub stages: Vec<CallQueueStageOption>,
    /// HubSpot にあって表に無いステージ (架電対象外。rule は `exclude`)
    pub unknown_stages: Vec<CallQueueStageOption>,
}

/// `GET /api/crm/call-queue/pipelines` (架電キューで選べるパイプラインとステージ名)
#[derive(Debug, Clone, Serialize, TS)]
pub struct CallQueuePipelinesResponse {
    /// 既定のパイプライン ID
    pub default_pipeline: String,
    pub pipelines: Vec<CallQueuePipelineOption>,
    /// HubSpot からステージ名を読めたか。false なら label はすべて null で、表に無いステージも分からない
    pub labels_available: bool,
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
    pipeline: &'static QueuePipeline,
    limit: u32,
    cursor: Option<String>,
    q: Option<String>,
    /// 昇順・重複なし。選んだパイプラインの `all` / `due` のステージだけ。空 = その全部
    stages: Vec<String>,
    owner: OwnerParam,
    due: Due,
    sort: SortKey,
    next: DateRange,
    last: DateRange,
    /// `fresh=1`: サーバの短いキャッシュを使わずに HubSpot から読み直す (cursor の条件には入れない)
    fresh: bool,
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
    let mut pipeline: Option<&'static QueuePipeline> = None;
    let mut limit: Option<u32> = None;
    let mut cursor = None;
    let mut q = None;
    let mut stages: Vec<String> = Vec::new();
    let mut owner = None;
    let mut due = None;
    let mut sort = None;
    let mut dates: [Option<NaiveDate>; 4] = [None; 4];
    let mut fresh = false;
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
            "pipeline" => {
                once("pipeline")?;
                pipeline = Some(find_pipeline(&v).ok_or("pipeline")?);
            }
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
            // 選んだパイプラインのステージかは、全部読んでから確かめる (`pipeline` が後ろにあってもよい)
            "stage" => stages.push(v.into_owned()),
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
            "fresh" => {
                once("fresh")?;
                fresh = match v.as_ref() {
                    "1" | "true" => true,
                    "0" | "false" => false,
                    _ => return Err("fresh"),
                };
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
    let pipeline = pipeline.unwrap_or_else(default_pipeline);
    if stages.iter().any(|s| !pipeline.is_eligible(s)) {
        return Err("stage");
    }
    stages.sort();
    stages.dedup();
    Ok(Params {
        pipeline,
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
        fresh,
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
    /// 選んだステージ (`all` を含む) で次回架電日が今日以前
    Due,
    /// `all` のステージ (未済など) で、次回架電日が無い or 明日以降 (= Due に入らないもの)
    NotDueAlways,
    /// キュー全体 (`all` のステージは全部 + `due` のステージで次回架電日が今日以前)
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
            v.extend(tail(Scope::NotDueAlways));
            v
        }
        (SortKey::NextCallDesc, Due::All) => {
            let mut v = vec![p(Scope::Due, LastCall::Any, "bpo_13", true)];
            v.extend(tail(Scope::NotDueAlways));
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
/// `all` のステージの絞り込み (1 つなら EQ、複数なら IN。どちらもフィルタ 1 つ)。
/// 次回日の条件が付くグループは従来どおり常に IN
fn f_stages(vs: &[&str]) -> Value {
    match vs {
        [one] => f_eq("dealstage", one),
        _ => f_in("dealstage", vs),
    }
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
    pipeline: &QueuePipeline,
    stages: &[&str],
    owner: Option<&Value>,
    today_ms: i64,
    r: &Ranges,
) -> Vec<Value> {
    let today = today_ms.to_string();
    // 選んだステージを決まりで分ける (表に無い・`exclude` は parse_params で弾いている)
    let always: Vec<&str> = stages
        .iter()
        .copied()
        .filter(|s| pipeline.rule(s) == Some(StageRule::All))
        .collect();
    let when_due: Vec<&str> = stages
        .iter()
        .copied()
        .filter(|s| pipeline.rule(s) == Some(StageRule::Due))
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
            let both: Vec<&str> = always.iter().chain(&when_due).copied().collect();
            if !both.is_empty() {
                groups.extend(due_group(f_in("dealstage", &both)));
            }
        }
        Scope::NotDueAlways => {
            if !always.is_empty() {
                if r.has_next() {
                    // 範囲を指定すると次回日が入っていることが前提。明日以降の部分だけ残る
                    let lo = r
                        .next_from
                        .map_or(today_ms + DAY_MS, |f| f.max(today_ms + DAY_MS));
                    if let Some(nf) = next_filters(Some(lo), r.next_to) {
                        let mut fs = vec![f_stages(&always)];
                        fs.extend(nf);
                        groups.push(finish(fs));
                    }
                } else {
                    groups.push(finish(vec![f_stages(&always), f_has("bpo_13", false)]));
                    groups.push(finish(vec![
                        f_stages(&always),
                        f_op("bpo_13", "GT", &today),
                    ]));
                }
            }
        }
        Scope::Queue => {
            if !always.is_empty() {
                if let Some(nf) = next_filters(r.next_from, r.next_to) {
                    let mut fs = vec![f_stages(&always)];
                    fs.extend(nf);
                    groups.push(finish(fs));
                }
            }
            if !when_due.is_empty() {
                groups.extend(due_group(f_in("dealstage", &when_due)));
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

/// 段階の件数だけを数える Search (行は 1 件だけ・ID だけ読む。並びは件数に関係しないので付けない)
fn count_body(groups: Vec<Value>, q: Option<&str>) -> Value {
    let mut body = json!({
        "filterGroups": groups,
        "properties": COUNT_PROPERTIES,
        "limit": 1,
    });
    if let Some(q) = q {
        body["query"] = json!(q);
    }
    body
}

/// 総数を数える Search 全体の上限。超えたら総数は null にして一覧は返す (総数のために一覧を遅らせない)
const COUNT_DEADLINE: Duration = Duration::from_secs(5);

/// 件数を数える Search で読むプロパティ (行の中身は使わない)
pub const COUNT_PROPERTIES: [&str; 1] = ["hs_object_id"];

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
        "v3".to_string(),
        params.pipeline.id.to_string(),
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
/// 検索 (`filter_groups`) と同じ条件を 1 件の Deal に当てる: 架電キューで選べるパイプライン (どれでもよい)・担当者・
/// アーカイブでない・架電禁止理由 `bpo_3` / ブロック理由 `bpo_4` が空・ステージ (決まりが `all` なら常に、
/// `due` なら次回架電日が今日以前。`exclude` と表に無いステージは外す)。
/// **電話番号の有無は見ない** (Contact / Company の追加読み取りが要るため。電話番号が無い自分の担当 Deal は
/// キューには出ないが、個別取得はできる)。
pub(super) fn deal_in_queue(deal: &HubSpotRecord, owner_id: &str, today_ms: i64) -> bool {
    if deal.archived || owner_id.trim().is_empty() {
        return false;
    }
    let Some(pipeline) = nz(deal, "pipeline").and_then(|p| find_pipeline(&p)) else {
        return false;
    };
    if nz(deal, "hubspot_owner_id").as_deref() != Some(owner_id.trim()) {
        return false;
    }
    if nz(deal, "bpo_3").is_some() || nz(deal, "bpo_4").is_some() {
        return false;
    }
    let Some(stage) = nz(deal, "dealstage") else {
        return false;
    };
    match pipeline.rule(&stage) {
        Some(StageRule::All) => return true,
        Some(StageRule::Due) => {}
        Some(StageRule::Exclude) | None => return false,
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
    // 1) 認可 (HubSpot の設定有無より先。キャッシュから返すときも毎回ここを通す)
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
    // 6) 担当者を解決する (`me` → 本人の owner ID。10 分キャッシュ)。全体の締め切りはここから数える
    let started = Instant::now();
    let resolved = match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        resolve_owner(&client, &ctx, &owner, role, &email),
    )
    .await
    {
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "call queue owner timed out");
            return timeout_response();
        }
        Ok(Err(resp)) => return resp,
        Ok(Ok(r)) => r,
    };
    // 7) 1 ページのキャッシュ (30 秒)。キーは解決済みの担当者を含む条件とページの位置。
    //    cursor (本人に束縛) と scope (役割・所属チーム) はこの人の分を作り直す
    let filter = filter_key(&params, &resolved.key, today_ms);
    let page_key = QueuePageKey {
        filter: filter.clone(),
        limit: params.limit,
        start,
    };
    if !params.fresh {
        if let Some(core) = ctx.queue.pages.get(&page_key) {
            cache_hit("call_queue_page");
            super::routes::refresh_ahead(&ctx, &client);
            let body = assemble(
                &ctx.queue,
                &core,
                &params,
                role,
                &owner,
                &hash,
                resolved.teams,
            );
            return Json(body).into_response();
        }
    }
    cache_miss("call_queue_page");
    // 8) 同時実行の枠 (待ちきれなければ 503 hubspot_busy) + 全体の締め切り
    let _slot = match super::routes::acquire_queue_slot(&ctx, &client).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let remaining = CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed());
    let run = execute(
        &client,
        &ctx,
        &params,
        resolved.filter.as_ref(),
        &filter,
        start,
        today_ms,
        now,
    );
    let resp = match tokio::time::timeout(remaining, run).await {
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "call queue timed out");
            timeout_response()
        }
        Ok(Ok((core, cacheable))) => {
            let core = Arc::new(core);
            if cacheable {
                ctx.queue.pages.insert(page_key, core.clone());
            }
            let body = assemble(
                &ctx.queue,
                &core,
                &params,
                role,
                &owner,
                &hash,
                resolved.teams,
            );
            Json(body).into_response()
        }
        Ok(Err(resp)) => resp,
    };
    super::routes::refresh_ahead(&ctx, &client);
    resp
}

/// 解決済みの担当者の絞り込み
struct ResolvedOwner {
    /// キャッシュのキー (`all` / `unassigned` / `id:<owner id>`)
    key: String,
    /// Search の担当者の条件 (全員分なら None)
    filter: Option<Value>,
    /// 本人の所属チーム名 (参考表示だけ。管理者・`me` 以外は空)
    teams: Vec<String>,
}

async fn resolve_owner(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    owner: &OwnerParam,
    role: CrmRole,
    email: &str,
) -> Result<ResolvedOwner, Response> {
    let fail = |e: HubSpotError| -> Response {
        tracing::warn!(error_kind = e.error_kind(), "call queue read failed");
        hubspot_error_response(&e)
    };
    let (key, filter) = match owner {
        OwnerParam::All | OwnerParam::Unspecified => ("all".to_string(), None),
        OwnerParam::Unassigned => (
            "unassigned".to_string(),
            Some(f_has("hubspot_owner_id", false)),
        ),
        OwnerParam::Id(id) => (format!("id:{id}"), Some(f_eq("hubspot_owner_id", id))),
        OwnerParam::Me => match ctx.queue.owner_for(client, email).await.map_err(fail)? {
            Some(id) => (format!("id:{id}"), Some(f_eq("hubspot_owner_id", &id))),
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
            .map_err(fail)?
            .map(|o| o.teams)
            .unwrap_or_default()
    };
    Ok(ResolvedOwner { key, filter, teams })
}

/// キャッシュのキーにする絞り込み (利用者によらない部分だけ)
fn filter_key(params: &Params, owner_key: &str, today_ms: i64) -> QueueFilterKey {
    QueueFilterKey {
        pipeline: params.pipeline.id.to_string(),
        q: params.q.clone(),
        stages: params.stages.clone(),
        owner: owner_key.to_string(),
        due: params.due.as_str().to_string(),
        sort: params.sort.as_str().to_string(),
        today_ms,
        next_from: params.next.start_text(),
        next_to: params.next.end_text(),
        last_from: params.last.start_text(),
        last_to: params.last.end_text(),
    }
}

/// 1 ページの中身のうち利用者によらない部分 (キャッシュに入れる)
#[derive(Debug)]
pub(super) struct QueueCore {
    items: Vec<CallQueueItem>,
    /// 次のページの位置 (段階, after)。cursor には要求ごとに本人の条件で署名する
    next: Option<(u32, Option<u64>)>,
    total: Option<u32>,
    truncated: bool,
    partial: CallQueuePartial,
    generated_at: String,
}

/// キャッシュした中身 (または読んだばかりの中身) から、この人への応答を作る
fn assemble(
    queue: &CallQueueState,
    core: &QueueCore,
    params: &Params,
    role: CrmRole,
    owner: &OwnerParam,
    hash: &str,
    teams: Vec<String>,
) -> CallQueueResponse {
    let selected: Vec<String> = if params.stages.is_empty() {
        params
            .pipeline
            .eligible_stages()
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        params.stages.clone()
    };
    CallQueueResponse {
        items: core.items.clone(),
        next_cursor: core
            .next
            .map(|(phase, after)| queue.sign_cursor(hash, phase, after)),
        total: core.total,
        truncated: core.truncated,
        scope: CallQueueScope {
            pipeline: params.pipeline.id.to_string(),
            owner: owner_label(owner),
            role: if role.is_admin() { "admin" } else { "own" }.to_string(),
            teams,
            stages: {
                let mut s = selected;
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
        partial: core.partial.clone(),
        generated_at: core.generated_at.clone(),
    }
}

/// HubSpot から 1 ページを読む。戻りの bool はキャッシュに入れてよいか
/// (関連の読み取りに欠けが無く、数えるべき総数を数えられた)。
#[allow(clippy::too_many_arguments)]
async fn execute(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    params: &Params,
    owner_filter: Option<&Value>,
    filter: &QueueFilterKey,
    start: (u32, Option<u64>),
    today_ms: i64,
    now: DateTime<Utc>,
) -> Result<(QueueCore, bool), Response> {
    let fail = |e: HubSpotError| -> Response {
        tracing::warn!(error_kind = e.error_kind(), "call queue read failed");
        hubspot_error_response(&e)
    };
    // 段階 (ステージの絞り込み後に検索するものがあるものだけ)
    let pipeline = params.pipeline;
    let selected: Vec<&str> = if params.stages.is_empty() {
        pipeline.eligible_stages()
    } else {
        params.stages.iter().map(String::as_str).collect()
    };
    let ranges = Ranges::of(params);
    let active: Vec<(Phase, Vec<Value>)> = phases(params.sort, params.due, ranges.has_last())
        .into_iter()
        .map(|p| {
            (
                p,
                filter_groups(p, pipeline, &selected, owner_filter, today_ms, &ranges),
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
    let mut next: Option<(u32, Option<u64>)> = None;
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
            next = Some((phase_idx as u32, Some(n)));
            break;
        }
        // この段階は終わり。次の段階へ
        phase_idx += 1;
        after = None;
        // 空の段階は同じ要求の中で次へ進む (空ページを返さない)
        if phase_idx < active.len() && got_any {
            next = Some((phase_idx as u32, None));
            break;
        }
    }

    // 総数は、段階が 1 つ or 先頭から全段階を数えたときにそのまま出せる。
    // 先頭ページで辿らなかった段階が残るときは、その段階の件数だけを数える Search を足して全体を出す
    // (一部の段階の件数を全体と偽らない。数えられなければ null)。2 ページ目以降は数えない (画面は先頭ページの総数を使う)
    let known_all =
        active.len() == 1 || (start_phase == 0 && start_after.is_none() && visited == active.len());
    let first_page = start_phase == 0 && start_after.is_none();
    let rest: &[(Phase, Vec<Value>)] = if !known_all && first_page {
        &active[visited..]
    } else {
        &[]
    };
    let counted_so_far: u64 = totals.iter().sum();
    let count_inner = async {
        if known_all {
            return Some(counted_so_far);
        }
        if rest.is_empty() {
            return None;
        }
        let mut sum = counted_so_far;
        for (i, (_, groups)) in rest.iter().enumerate() {
            // 段階の件数は 60 秒キャッシュする (`fresh` なら読み直して置き換える)
            let key = QueueCountKey {
                filter: filter.clone(),
                phase: (visited + i) as u32,
            };
            if !params.fresh {
                if let Some(n) = ctx.queue.counts.get(&key) {
                    cache_hit("call_queue_count");
                    sum += n;
                    continue;
                }
            }
            cache_miss("call_queue_count");
            match client
                .search("deals", count_body(groups.clone(), params.q.as_deref()))
                .await
            {
                Ok(v) => match v.get("total").and_then(Value::as_u64) {
                    Some(n) => {
                        ctx.queue.counts.insert(key, n);
                        sum += n
                    }
                    None => return None,
                },
                Err(e) => {
                    tracing::warn!(
                        error_kind = e.error_kind(),
                        "call queue: total count unavailable"
                    );
                    return None;
                }
            }
        }
        Some(sum)
    };
    let count_rest = async {
        let counted = tokio::time::timeout(COUNT_DEADLINE, count_inner).await;
        counted.unwrap_or_else(|_| {
            tracing::warn!("call queue: total count timed out");
            None
        })
    };

    // 後段の確認: アーカイブ・別パイプライン・許可外ステージ・停止系
    let mut partial = CallQueuePartial::default();
    let mut deals: Vec<HubSpotRecord> = Vec::new();
    for d in page_deals {
        let in_scope = !d.archived
            && nz(&d, "pipeline").as_deref() == Some(pipeline.id)
            && nz(&d, "dealstage").is_some_and(|s| pipeline.is_eligible(&s));
        if !in_scope {
            partial.excluded.out_of_scope += 1;
        } else if nz(&d, "bpo_3").is_some() || nz(&d, "bpo_4").is_some() {
            partial.excluded.stop_reason += 1;
        } else {
            deals.push(d);
        }
    }

    let mut items = Vec::with_capacity(deals.len());
    // 残りの段階の件数は、関連の読み取りと並べて数える (Search の間隔待ちを読み取りの時間に重ねる)
    // パイプライン定義 (ステージ名と、表に無いステージの数) は行が無いときも読む (5 分キャッシュ)
    let total_all: Option<u64>;
    if deals.is_empty() {
        let (counted, defs) = tokio::join!(count_rest, ctx.queue.pipeline_defs(client));
        total_all = counted;
        // 行が無いときは失敗を partial に出さない (ステージ名を出す行が無い)。表に無いステージの数だけ分からない
        if let Ok(defs) = defs {
            partial.unknown_stages = unknown_stages(&defs, pipeline).len() as u32;
        }
    } else {
        let deal_ids: Vec<String> = deals.iter().map(|d| d.id.clone()).collect();
        let mut failed: Vec<String> = Vec::new();
        let (rel, defs, counted) = tokio::join!(
            load_related(client, &deal_ids, &mut failed),
            ctx.queue.pipeline_defs(client),
            count_rest
        );
        total_all = counted;
        let labels = match defs {
            Ok(defs) => {
                partial.unknown_stages = unknown_stages(&defs, pipeline).len() as u32;
                stage_label_map(&defs)
            }
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

    let total = total_all.map(|n| n.min(u64::from(u32::MAX)) as u32);
    // 欠けがあった応答・数えるべき総数を数えられなかった応答はキャッシュに入れない (次の要求で読み直す)
    let cacheable = partial.failed.is_empty() && (rest.is_empty() || total.is_some());

    Ok((
        QueueCore {
            items,
            next,
            total,
            truncated,
            partial,
            generated_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
        },
        cacheable,
    ))
}

// ---------------------------------------------------------------------------
// GET /api/crm/call-queue/pipelines (選べるパイプラインとステージ名)
// ---------------------------------------------------------------------------

/// 表 (`queue_pipelines.rs`) と HubSpot のパイプライン定義を合わせる。`defs` が無ければ名前は null
fn pipeline_options(defs: Option<&[CrmPipeline]>) -> Vec<CallQueuePipelineOption> {
    QUEUE_PIPELINES
        .iter()
        .map(|p| {
            let hub = defs.and_then(|d| d.iter().find(|x| x.id == p.id));
            let label_of = |id: &str| {
                hub.and_then(|h| h.stages.iter().find(|s| s.id == id))
                    .map(|s| s.label.clone())
            };
            CallQueuePipelineOption {
                id: p.id.to_string(),
                label: hub.map(|h| h.label.clone()),
                stages: p
                    .stages
                    .iter()
                    .map(|(id, rule)| CallQueueStageOption {
                        id: id.to_string(),
                        label: label_of(id),
                        rule: rule.as_str().to_string(),
                    })
                    .collect(),
                unknown_stages: defs
                    .map(|d| unknown_stages(d, p))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| CallQueueStageOption {
                        id: s.id.clone(),
                        label: Some(s.label.clone()),
                        rule: StageRule::Exclude.as_str().to_string(),
                    })
                    .collect(),
            }
        })
        .collect()
}

/// 架電キューで選べるパイプラインと、ステージの名前・決まり。
/// HubSpot から名前を読めないとき (未設定・失敗・時間切れ) も 200 で返し、`labels_available: false` にする
/// (画面は表の呼び名で選択肢を出し、キューの取得は続けられる)。
pub(super) async fn get_call_queue_pipelines(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    RawQuery(raw): RawQuery,
) -> Response {
    if let Err(denied) =
        rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await
    {
        return denied.into_response();
    }
    if raw.as_deref().is_some_and(|q| !q.is_empty()) {
        return bad_param("unknown");
    }
    let defs = match state.hubspot.clone() {
        None => None,
        Some(client) => {
            match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.queue.pipeline_defs(&client)).await
            {
                Ok(Ok(d)) => Some(d),
                Ok(Err(e)) => {
                    tracing::warn!(
                        error_kind = e.error_kind(),
                        "call queue pipelines: labels unavailable"
                    );
                    None
                }
                Err(_) => {
                    tracing::warn!(
                        error_kind = "crm_timeout",
                        "call queue pipelines: labels timed out"
                    );
                    None
                }
            }
        }
    };
    Json(CallQueuePipelinesResponse {
        default_pipeline: default_pipeline().id.to_string(),
        pipelines: pipeline_options(defs.as_deref().map(Vec::as_slice)),
        labels_available: defs.is_some(),
    })
    .into_response()
}
