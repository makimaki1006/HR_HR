//! `GET /api/crm/call-queue/stage-breakdown` (架電一覧の条件に合う取引の、ステージの内訳と日ごとの推移)。
//!
//! 目的: 作って配ったリスト (詳細条件で決まる取引の集まり) が消化されたかを、「未済」に残る数で見る。
//!
//! - 対象: 架電一覧と同じクエリ (パイプライン・詳細条件・担当者・検索語・日付の範囲)。ステージの絞り込みと並べ替えは使わず、
//!   選んだパイプラインの全ステージを対象にする (架電禁止・アポ日確定なども含めて、リストの全体を数える)。
//! - 今の内訳: 条件に合う取引の今のステージ。
//! - 推移: 同じ取引について、`dealstage` の変更履歴から各日 (JST) の終わりの時点のステージを組み立てる。
//!   取引の集まりは今の条件で決まる (後から条件を外れた取引は過去にも出ない)。
//!   HubSpot は履歴を新しい順に 20 件までしか返さないので、それより古い日は「履歴が足りない」に数える。
//!   作成前の日は「まだ無い」に数える。別のパイプラインにいた日は「ほかのパイプライン」に数える。
//! - 保存はしない (ADR-006)。結果はプロセス内に 10 分覚え、計算は背景で行う (応答は `loading` を返し、画面が取り直す)。
//! - HubSpot: Search 200 件ずつ (最大 50 回) + 履歴の batch read 50 件ずつ。3,000 件のリストで約 75 回。
//!   全部 Background の優先度 (画面の操作を先に通す)。1 万件を超える条件は数えない (`too_many`)。

use super::*;
use crate::crm::talk_review::scope::{parse_stage_history, stage_at};

pub const DEFAULT_DAYS: u32 = 30;
pub const MAX_DAYS: u32 = 90;
const MAX_DEALS: u64 = 10_000;
const SEARCH_PAGE: u32 = 200;
const RESULT_TTL: Duration = Duration::from_secs(10 * 60);
const FAILED_TTL: Duration = Duration::from_secs(30);
/// 1 回の計算の上限 (関所の待ちを含む)
const JOB_DEADLINE: Duration = Duration::from_secs(5 * 60);
const STORE_MAX: usize = 100;
/// HubSpot が返す履歴の最大件数 (これに達した取引は、それより古い日のステージが分からない)
const HISTORY_CAP: usize = 20;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, TS)]
pub struct StageBreakdownStage {
    pub id: String,
    pub label: String,
    /// 表示名に「未済」を含む
    pub unconsumed: bool,
    /// 今この段にいる件数
    pub now: u32,
    /// `days` と同じ並びの、各日の終わりの件数
    pub by_day: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct StageBreakdownPipeline {
    pub id: String,
    pub label: String,
    pub stages: Vec<StageBreakdownStage>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct StageBreakdownResponse {
    /// `ready` / `loading` / `too_many` / `failed`
    pub status: String,
    /// 今条件に合う取引の数
    pub total: u32,
    pub pipelines: Vec<StageBreakdownPipeline>,
    /// 古い順の日付 (JST、`YYYY-MM-DD`)。最後が今日 (今日は今の時点)
    pub days: Vec<String>,
    /// 各日、選んだパイプラインの外のステージにいた件数
    pub other_pipeline: Vec<u32>,
    /// 各日、まだ作られていなかった件数
    pub not_yet: Vec<u32>,
    /// 各日、履歴が足りずステージが分からない件数
    pub unknown: Vec<u32>,
    pub generated_at: Option<String>,
    pub message: Option<String>,
}

impl StageBreakdownResponse {
    fn empty(status: &str, message: Option<&str>) -> Self {
        Self {
            status: status.to_string(),
            total: 0,
            pipelines: Vec::new(),
            days: Vec::new(),
            other_pipeline: Vec::new(),
            not_yet: Vec::new(),
            unknown: Vec::new(),
            generated_at: None,
            message: message.map(str::to_string),
        }
    }
}

enum Entry {
    Loading,
    Done(Instant, Arc<StageBreakdownResponse>),
}

/// 計算結果 (プロセス内。単一インスタンス前提)
pub struct BreakdownStore {
    entries: Mutex<HashMap<String, Entry>>,
    /// 同時に走らせる計算は 2 本まで
    jobs: Arc<tokio::sync::Semaphore>,
}

impl BreakdownStore {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            jobs: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
}

impl Default for BreakdownStore {
    fn default() -> Self {
        Self::new()
    }
}

/// クエリから `days` を外す (架電一覧の解析は知らない引数を拒むため)
fn split_days(raw: &str) -> Result<(String, u32), &'static str> {
    let url = reqwest::Url::parse(&format!("http://x/?{raw}")).map_err(|_| "query")?;
    let mut days = None;
    let mut rest = reqwest::Url::parse("http://x/").expect("static url");
    {
        let mut pairs = rest.query_pairs_mut();
        for (k, v) in url.query_pairs() {
            if k == "days" {
                if days.is_some() {
                    return Err("days");
                }
                let n: u32 = v.parse().map_err(|_| "days")?;
                if !(1..=MAX_DAYS).contains(&n) {
                    return Err("days");
                }
                days = Some(n);
            } else {
                pairs.append_pair(&k, &v);
            }
        }
    }
    Ok((
        rest.query().unwrap_or("").to_string(),
        days.unwrap_or(DEFAULT_DAYS),
    ))
}

pub(in crate::crm) async fn get_stage_breakdown(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    RawQuery(raw): RawQuery,
) -> Response {
    let (query, days) = match split_days(raw.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(name) => return bad_param(name),
    };
    let Prepared {
        params,
        role,
        owner,
        email,
        client,
        now,
    } = match prepare(&session, &state, &ctx, &query, true).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    let resolved = match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        resolve_owner(&client, &ctx, &owner, role, &email),
    )
    .await
    {
        Err(_) => return timeout_response(),
        Ok(Err(resp)) => return resp,
        Ok(Ok(r)) => r,
    };
    let today_ms = jst_today_ms(now);
    let key = format!(
        "{:?}|days={days}",
        filter_key(&params, &resolved.key, today_ms)
    );
    let store = &ctx.queue.breakdowns;
    {
        let mut entries = store.entries.lock().expect("breakdown store");
        match entries.get(&key) {
            Some(Entry::Loading) => {
                return Json(StageBreakdownResponse::empty("loading", None)).into_response()
            }
            Some(Entry::Done(at, r)) => {
                let ttl = if r.status == "ready" || r.status == "too_many" {
                    RESULT_TTL
                } else {
                    FAILED_TTL
                };
                if at.elapsed() < ttl && !params.fresh {
                    return Json(r.as_ref().clone()).into_response();
                }
            }
            None => {}
        }
        if entries.len() >= STORE_MAX {
            // 期限の切れたものから捨て、それでも多ければ一番古いものを捨てる
            entries.retain(|_, e| {
                matches!(e, Entry::Loading)
                    || matches!(e, Entry::Done(at, _) if at.elapsed() < RESULT_TTL)
            });
            if entries.len() >= STORE_MAX {
                if let Some(oldest) = entries
                    .iter()
                    .filter_map(|(k, e)| match e {
                        Entry::Done(at, _) => Some((k.clone(), *at)),
                        Entry::Loading => None,
                    })
                    .min_by_key(|(_, at)| *at)
                    .map(|(k, _)| k)
                {
                    entries.remove(&oldest);
                }
            }
        }
        entries.insert(key.clone(), Entry::Loading);
    }
    let ctx2 = ctx.clone();
    let bg = client.background();
    let owner_filter = resolved.filter;
    tokio::spawn(async move {
        let _permit = ctx2.queue.breakdowns.jobs.clone().acquire_owned().await;
        let result = tokio::time::timeout(
            JOB_DEADLINE,
            compute(&bg, &ctx2.queue, &params, owner_filter.as_ref(), days, now),
        )
        .await
        .unwrap_or_else(|_| {
            Ok(StageBreakdownResponse::empty(
                "failed",
                Some("時間内に数えられませんでした。少し待ってから最新にしてください。"),
            ))
        })
        .unwrap_or_else(|e| {
            tracing::warn!(error_kind = e.error_kind(), "stage breakdown failed");
            StageBreakdownResponse::empty(
                "failed",
                Some("HubSpot から読めませんでした。少し待ってから最新にしてください。"),
            )
        });
        ctx2.queue
            .breakdowns
            .entries
            .lock()
            .expect("breakdown store")
            .insert(key, Entry::Done(Instant::now(), Arc::new(result)));
    });
    Json(StageBreakdownResponse::empty("loading", None)).into_response()
}

/// 条件に合う取引を全部読み、今のステージと日ごとのステージを数える
async fn compute(
    client: &HubSpotClient,
    queue: &CallQueueState,
    params: &Params,
    owner_filter: Option<&Value>,
    days: u32,
    now: DateTime<Utc>,
) -> Result<StageBreakdownResponse, HubSpotError> {
    let defs = queue.pipeline_defs(client).await?;
    // Search の条件 (架電一覧の「すべての取引」と同じ組み立て)
    let ranges = Ranges::of(params);
    let ids: Vec<&str> = params.all_stages.iter().map(String::as_str).collect();
    let mut fs = vec![f_in("dealstage", &ids)];
    fs.extend(next_filters(ranges.next_from, ranges.next_to).unwrap_or_default());
    if let Some(v) = ranges.last_from {
        fs.push(f_op("bpo_20", "GTE", &v.to_string()));
    }
    if let Some(v) = ranges.last_to {
        fs.push(f_op("bpo_20", "LTE", &v.to_string()));
    }
    if let Some(o) = owner_filter {
        fs.push(o.clone());
    }
    let groups = match super::super::queue_property_filter::append_groups(
        vec![json!({ "filters": fs })],
        &params.property_filter_groups,
    ) {
        Ok(g) => g,
        Err(m) => return Ok(StageBreakdownResponse::empty("failed", Some(m))),
    };

    // 1) 取引の ID を全部読む (ID 順で 200 件ずつ)
    let mut deal_ids: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let mut body = json!({
            "filterGroups": groups,
            "sorts": [{ "propertyName": "hs_object_id", "direction": "ASCENDING" }],
            "properties": ["dealstage", "pipeline"],
            "limit": SEARCH_PAGE,
        });
        if let Some(a) = &after {
            body["after"] = json!(a);
        }
        if let Some(q) = &params.q {
            body["query"] = json!(q);
        }
        let v = client.search("deals", body).await?;
        let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
        if total > MAX_DEALS {
            let mut r = StageBreakdownResponse::empty(
                "too_many",
                Some("条件に合う取引が 1 万件を超えるため数えられません。条件を絞ってください。"),
            );
            r.total = total.min(u64::from(u32::MAX)) as u32;
            return Ok(r);
        }
        for d in v
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if d.get("archived").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            if let Some(id) = d.get("id").and_then(Value::as_str) {
                deal_ids.push(id.to_string());
            }
        }
        after = v
            .pointer("/paging/next/after")
            .and_then(Value::as_str)
            .map(str::to_string);
        if after.is_none() {
            break;
        }
    }
    deal_ids.sort();
    deal_ids.dedup();

    // 2) ステージの履歴 (50 件ずつ)
    let records = if deal_ids.is_empty() {
        Vec::new()
    } else {
        client
            .batch_read_with_history(
                "deals",
                &deal_ids,
                &["dealstage", "pipeline", "createdate"],
                &["dealstage"],
            )
            .await?
    };
    Ok(tally(&defs, params, &records, days, now))
}

/// 履歴から数える (HubSpot を呼ばない)
fn tally(
    defs: &[CrmPipeline],
    params: &Params,
    records: &[Value],
    days: u32,
    now: DateTime<Utc>,
) -> StageBreakdownResponse {
    // 日の終わり (JST)。今日だけは今の時点
    let today_ms = jst_today_ms(now);
    let jst_today_start = today_ms - 9 * 60 * 60 * 1000;
    let mut day_labels = Vec::new();
    let mut day_ends = Vec::new();
    for i in (0..days).rev() {
        let start = jst_today_start - i64::from(i) * DAY_MS;
        let label = DateTime::<Utc>::from_timestamp_millis(today_ms - i64::from(i) * DAY_MS)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        day_labels.push(label);
        day_ends.push(if i == 0 {
            now.timestamp_millis()
        } else {
            start + DAY_MS - 1
        });
    }
    let n = day_ends.len();

    // 選んだパイプラインの段 (HubSpot の並び順)
    let mut pipelines: Vec<StageBreakdownPipeline> = Vec::new();
    let mut index: HashMap<String, (usize, usize)> = HashMap::new();
    for p in &params.pipelines {
        let Some(def) = defs.iter().find(|d| d.id == p.id) else {
            continue;
        };
        let pi = pipelines.len();
        let stages = def
            .stages
            .iter()
            .enumerate()
            .map(|(si, s)| {
                index.insert(s.id.clone(), (pi, si));
                StageBreakdownStage {
                    id: s.id.clone(),
                    label: s.label.clone(),
                    unconsumed: s.label.contains("未済"),
                    now: 0,
                    by_day: vec![0; n],
                }
            })
            .collect();
        pipelines.push(StageBreakdownPipeline {
            id: def.id.clone(),
            label: def.label.clone(),
            stages,
        });
    }

    let mut other = vec![0u32; n];
    let mut not_yet = vec![0u32; n];
    let mut unknown = vec![0u32; n];
    let mut total = 0u32;
    for r in records {
        let current = r.pointer("/properties/dealstage").and_then(Value::as_str);
        let Some(&(cp, cs)) = current.and_then(|c| index.get(c)) else {
            // 読んでいる間に別のパイプラインへ移った取引は数えない
            continue;
        };
        total += 1;
        pipelines[cp].stages[cs].now += 1;
        let hist = parse_stage_history(r);
        let created_ms = r
            .pointer("/properties/createdate")
            .and_then(Value::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp_millis());
        let capped = hist.len() >= HISTORY_CAP;
        for (i, &end) in day_ends.iter().enumerate() {
            if created_ms.is_some_and(|c| c > end) {
                not_yet[i] += 1;
                continue;
            }
            match stage_at(&hist, end) {
                Some(s) => match index.get(s) {
                    Some(&(p, st)) => pipelines[p].stages[st].by_day[i] += 1,
                    None => other[i] += 1,
                },
                None if capped => unknown[i] += 1,
                None => not_yet[i] += 1,
            }
        }
    }
    StageBreakdownResponse {
        status: "ready".to_string(),
        total,
        pipelines,
        days: day_labels,
        other_pipeline: other,
        not_yet,
        unknown,
        generated_at: Some(now.to_rfc3339_opts(SecondsFormat::Secs, true)),
        message: None,
    }
}
