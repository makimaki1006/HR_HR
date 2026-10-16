//! 案件管理ホーム: 「まず見るもの」の件数と、事業所（顧客）ごとの表。
//! 全件索引（メモリ上）と、求人に付いた週ごとの実績だけから計算する。求人ごとの HubSpot 呼び出しはしない。
//! 事業所は索引完成後に先読みし、更新中は前回分を返す。読み直しは背景で 1 本だけ。
//! 見る基準はすべてこのファイルの定数。画面にも同じ数字を注記として出す（応募の増減は 2026-10-15 合意）。
use super::*;
use chrono::NaiveDate;
use std::time::Instant as StdInstant;

/// 応募の増減を数える差（件）。割合の基準と、どちらかを満たせば対象。
pub(super) const TREND_MIN_DIFF: i64 = 2;
/// 前の 4 週の応募に対する増減の割合（%）。
pub(super) const TREND_MIN_PERCENT: i64 = 20;
/// 「課金が高いまま応募が少ない」の下限。直近 4 週の課金がこの金額（円）以上であること。
pub(super) const COST_FLOOR_YEN: i64 = 10_000;

/// 画面の文言用の金額表記。1万円以上で万円で割り切れるときは「1万円」、それ以外は「9,500円」。
fn yen_label(value: i64) -> String {
    if value >= 10_000 && value % 10_000 == 0 {
        format!("{}万円", value / 10_000)
    } else {
        let digits = value.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        format!("{out}円")
    }
}
/// 直近 4 週の応募 1 件あたりが、応募のある求人の中央値のこの倍以上なら「高い」。応募 0 件ならその時点で対象。
pub(super) const COST_PER_APP_TIMES: i64 = 2;
/// 最新の週の終わりがこの日数より前なら「データが止まっている」。
pub(super) const STALE_DAYS: i64 = 14;
/// 1 つのカードで一覧に出す求人の上限（件数は全件を数える）。
pub(super) const LIST_CAP: usize = 100;
const COMPANY_TTL: Duration = Duration::from_secs(30 * 60);

/// 週ごとの実績から求人ごとに 1 度だけ作る要約。週ごとの JSON は索引に残さない。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct PerfDigest {
    pub(super) latest_end: NaiveDate,
    /// 最新の週を含む 4 週（週の開始日が最新の開始日の 21 日前まで）の応募と課金。
    pub(super) recent_apps: i64,
    pub(super) recent_cost: i64,
    /// その前の 4 週の応募。
    pub(super) prev_apps: i64,
}
fn day(value: &Value) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.as_str()?, "%Y-%m-%d").ok()
}
fn whole(value: &Value) -> i64 {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(0, |v| v.round() as i64)
}
impl PerfDigest {
    pub(super) fn from_json(perf: &Value) -> Option<Self> {
        let rows: Vec<(NaiveDate, NaiveDate, i64, i64)> = perf["rows"]
            .as_array()?
            .iter()
            .filter_map(|row| {
                Some((
                    day(&row["period_start"])?,
                    day(&row["period_end"])?,
                    whole(&row["cost_yen"]),
                    whole(&row["applications"]),
                ))
            })
            .collect();
        let (latest_start, latest_end, _, _) = rows.iter().max_by_key(|row| row.0).copied()?;
        let mut digest = Self {
            latest_end,
            recent_apps: 0,
            recent_cost: 0,
            prev_apps: 0,
        };
        for (start, _, cost, apps) in rows {
            let age = (latest_start - start).num_days();
            if (0..28).contains(&age) {
                digest.recent_apps += apps;
                digest.recent_cost += cost;
            } else if (28..56).contains(&age) {
                digest.prev_apps += apps;
            }
        }
        Some(digest)
    }
    pub(super) fn from_record(record: &Record, media: &str) -> Option<Self> {
        let perf = if media == "hrh" {
            listings::indeed_weekly_perf(record)
        } else {
            listings::airwork_weekly_perf(record)
        }?;
        Self::from_json(&perf)
    }
}

/// 取引→事業所の対応と事業所名。索引の作成時刻ごとに作り直す。
#[derive(Clone, Default)]
pub(super) struct CompanyMap {
    built_for: String,
    at: Option<StdInstant>,
    pub(super) deal_company: BTreeMap<String, String>,
    pub(super) names: BTreeMap<String, Option<String>>,
    /// 取引 ID → 名前・ステージ・作成日（納品管理の取引だけ）。顧客表の「契約中」と最新の案件に使う。
    pub(super) deal_info: BTreeMap<String, DealInfo>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct DealInfo {
    pub(super) name: Option<String>,
    pub(super) stage_label: Option<String>,
    /// 完了していないステージか（`pipelines::stage_state` の定義）。
    pub(super) active: bool,
    pub(super) created_at: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
pub(super) struct HomeRow {
    pub(super) id: String,
    title: Option<String>,
    company: Option<String>,
    media: &'static str,
    recent_apps: Option<i64>,
    prev_apps: Option<i64>,
    recent_cost: Option<i64>,
    latest_week_end: Option<String>,
}
#[derive(Serialize, Debug, PartialEq)]
pub(super) struct Card {
    pub(super) key: &'static str,
    title: &'static str,
    rule: String,
    pub(super) count: usize,
    pub(super) listings: Vec<HomeRow>,
}
#[derive(Serialize, Debug, PartialEq)]
pub(super) struct Customer {
    pub(super) company_id: String,
    pub(super) name: Option<String>,
    /// 納品管理の案件の数（求人が結び付いたもの）。
    pub(super) deals: usize,
    /// そのうち契約中（完了のステージでない）の数。
    pub(super) active_deals: usize,
    /// 作成日がいちばん新しい案件。
    pub(super) latest_deal: Option<DealInfo>,
    pub(super) live_listings: usize,
    /// 掲載中・掲載終了などを合わせた、結び付いた求人の数。
    pub(super) total_listings: usize,
    pub(super) recent_apps: i64,
    pub(super) prev_apps: i64,
    pub(super) recent_cost: i64,
    /// 応募 1 件あたりの課金（円）。応募が 0 件なら null。
    pub(super) cost_per_app: Option<i64>,
}
#[derive(Serialize, Debug, PartialEq)]
pub(super) struct Home {
    pub(super) cards: Vec<Card>,
    pub(super) customers: Vec<Customer>,
    /// 取引に結び付いていない（または事業所が分からない）掲載中の求人の数。
    pub(super) live_without_company: usize,
    pub(super) median_cost_per_app: Option<i64>,
}

fn is_live(row: &Listing) -> bool {
    row.publication_status.as_deref() == Some("掲載中")
}
fn median(mut values: Vec<i64>) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2
    })
}
fn cost_per_app(perf: &PerfDigest) -> Option<i64> {
    (perf.recent_apps > 0).then(|| perf.recent_cost / perf.recent_apps)
}

/// 応募の増減の 5 分類は排他的。割合の掛け算は i128 に広げて桁あふれも防ぐ。
fn trend_key(perf: &PerfDigest) -> &'static str {
    let (prev, recent) = (perf.prev_apps, perf.recent_apps);
    let changed = |diff: i64| {
        diff >= TREND_MIN_DIFF
            || i128::from(diff) * 100 >= i128::from(TREND_MIN_PERCENT) * i128::from(prev)
    };
    if prev == 0 && recent == 0 {
        "zero_stay"
    } else if recent == 0 {
        "to_zero"
    } else if recent < prev && changed(prev - recent) {
        "less"
    } else if recent > prev && (prev == 0 || changed(recent - prev)) {
        "more"
    } else {
        "flat"
    }
}

pub(super) fn compute(records: &[Listing], map: &CompanyMap, today: NaiveDate) -> Home {
    let company_of = |row: &Listing| -> Option<String> {
        row.deal_ids
            .iter()
            .find_map(|deal| map.deal_company.get(deal))
            .and_then(|id| map.names.get(id).cloned().flatten())
    };
    let live: Vec<&Listing> = records.iter().filter(|row| is_live(row)).collect();
    let median_cpa = median(
        live.iter()
            .filter_map(|row| row.perf.as_ref())
            .filter(|perf| perf.recent_cost > 0)
            .filter_map(cost_per_app)
            .collect(),
    );
    let row_of = |row: &Listing| HomeRow {
        id: row.id.clone(),
        title: row.title.clone(),
        company: company_of(row),
        media: row.media,
        recent_apps: row.perf.as_ref().map(|p| p.recent_apps),
        prev_apps: row.perf.as_ref().map(|p| p.prev_apps),
        recent_cost: row.perf.as_ref().map(|p| p.recent_cost),
        latest_week_end: row.perf.as_ref().map(|p| p.latest_end.to_string()),
    };
    let card = |key, title, rule: String, picked: Vec<&Listing>| Card {
        key,
        title,
        rule,
        count: picked.len(),
        listings: picked
            .iter()
            .take(LIST_CAP)
            .map(|row| row_of(row))
            .collect(),
    };
    let pick = |key: &str| -> Vec<&Listing> {
        let mut picked: Vec<_> = live
            .iter()
            .copied()
            .filter(|row| row.perf.as_ref().is_some_and(|p| trend_key(p) == key))
            .collect();
        if key == "zero_stay" {
            picked.sort_by_key(|row| std::cmp::Reverse(row.perf.as_ref().map(|p| p.recent_cost)));
        }
        picked
    };
    let costly: Vec<&Listing> = live
        .iter()
        .copied()
        .filter(|row| {
            row.perf.as_ref().is_some_and(|p| {
                p.recent_cost >= COST_FLOOR_YEN
                    && match cost_per_app(p) {
                        None => true,
                        Some(value) => median_cpa.is_some_and(|m| value >= m * COST_PER_APP_TIMES),
                    }
            })
        })
        .collect();
    let stale: Vec<&Listing> = live
        .iter()
        .copied()
        .filter(|row| {
            row.perf
                .as_ref()
                .is_some_and(|p| (today - p.latest_end).num_days() > STALE_DAYS)
        })
        .collect();
    let unset: Vec<&Listing> = live
        .iter()
        .copied()
        .filter(|row| row.indeed_title_state == "unset")
        .collect();
    let cards = vec![
        card("zero_stay", "応募がゼロのままの求人", "掲載中で、前の4週も直近4週も応募が0件の求人。直近4週の課金が多い順に表示します。".to_owned(), pick("zero_stay")),
        card("to_zero", "応募がゼロになった求人", "掲載中で、前の4週に応募が1件以上あり、直近4週は0件になった求人。".to_owned(), pick("to_zero")),
        card("less", "直近4週の応募が、前の4週より少ない求人", format!("掲載中で、直近4週の応募が1件以上あり、前の4週より{TREND_MIN_DIFF}件以上、または{TREND_MIN_PERCENT}%以上少ない求人。"), pick("less")),
        card("more", "直近4週の応募が、前の4週より多い求人", format!("掲載中で、直近4週の応募が前の4週より{TREND_MIN_DIFF}件以上、または{TREND_MIN_PERCENT}%以上多い求人。前の4週が0件で応募があった求人も含みます。"), pick("more")),
        card("flat", "応募はあるが、前の4週とほぼ同じ求人", format!("掲載中で、直近4週に応募があり、前の4週と同じ件数、または差が{TREND_MIN_DIFF}件未満かつ{TREND_MIN_PERCENT}%未満の求人。"), pick("flat")),
        card("costly", "課金が高いまま応募が少ない求人", format!("掲載中で、直近4週の課金が{}以上あり、応募が0件、または応募1件あたりの課金が応募のある求人の中央値の{COST_PER_APP_TIMES}倍以上の求人。", yen_label(COST_FLOOR_YEN)), costly),
        card("stale", "データが止まっている求人", format!("掲載中で、週ごとの実績の最新の週が{STALE_DAYS}日より前に終わっている求人。この間の応募・課金は少なく見えます。"), stale),
        card("occupation", "市場の職種が未指定の求人", "掲載中で、市場と比べる職種をまだ指定していない求人。".to_owned(), unset),
    ];

    #[derive(Default)]
    struct Agg {
        deals: BTreeSet<String>,
        live: usize,
        total: usize,
        recent: i64,
        prev: i64,
        cost: i64,
    }
    let mut by_company: BTreeMap<String, Agg> = BTreeMap::new();
    let mut live_without_company = 0;
    for row in records {
        let companies: BTreeSet<&String> = row
            .deal_ids
            .iter()
            .filter_map(|deal| map.deal_company.get(deal))
            .collect();
        if companies.is_empty() && is_live(row) {
            live_without_company += 1;
        }
        for company in companies {
            let agg = by_company.entry(company.clone()).or_default();
            agg.deals.extend(
                row.deal_ids
                    .iter()
                    .filter(|deal| map.deal_company.get(*deal) == Some(company))
                    .cloned(),
            );
            agg.total += 1;
            if is_live(row) {
                agg.live += 1;
                if let Some(perf) = &row.perf {
                    agg.recent += perf.recent_apps;
                    agg.prev += perf.prev_apps;
                    agg.cost += perf.recent_cost;
                }
            }
        }
    }
    let mut customers: Vec<Customer> = by_company
        .into_iter()
        .map(|(id, agg)| {
            let infos: Vec<(&String, Option<&DealInfo>)> = agg
                .deals
                .iter()
                .map(|deal| (deal, map.deal_info.get(deal)))
                .collect();
            let active_deals = infos
                .iter()
                .filter(|(_, info)| info.is_none_or(|i| i.active))
                .count();
            // 今の取引: 契約中のうち作成日が最新。契約中が無ければ最新の取引（案件一覧の current と同じ定義）。
            let latest_deal = infos
                .iter()
                .filter_map(|(deal, info)| info.map(|i| (*deal, i)))
                .max_by(|a, b| {
                    a.1.active
                        .cmp(&b.1.active)
                        .then_with(|| a.1.created_at.cmp(&b.1.created_at))
                        .then_with(|| b.0.cmp(a.0))
                })
                .map(|(_, info)| info.clone());
            Customer {
                name: map.names.get(&id).cloned().flatten(),
                company_id: id,
                deals: agg.deals.len(),
                active_deals,
                latest_deal,
                live_listings: agg.live,
                total_listings: agg.total,
                recent_apps: agg.recent,
                prev_apps: agg.prev,
                recent_cost: agg.cost,
                cost_per_app: (agg.recent > 0).then(|| agg.cost / agg.recent),
            }
        })
        .collect();
    // 契約中の案件がある顧客を先に、同じなら最新の案件の作成日が新しい順。
    customers.sort_by(|a, b| {
        (b.active_deals > 0)
            .cmp(&(a.active_deals > 0))
            .then_with(|| {
                let created =
                    |c: &Customer| c.latest_deal.as_ref().and_then(|d| d.created_at.clone());
                created(b).cmp(&created(a))
            })
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.company_id.cmp(&b.company_id))
    });
    Home {
        cards,
        customers,
        live_without_company,
        median_cost_per_app: median_cpa,
    }
}

/// 求人は複数の取引のどれかが一致すれば含める。顧客集計には一致した結び付けだけを残す。
fn scoped(
    records: &[Listing],
    map: &CompanyMap,
    snapshot: &owner_todo::Snapshot,
    field: &str,
    owner: Option<&str>,
) -> (Vec<Listing>, CompanyMap) {
    let mut map = map.clone();
    map.deal_company
        .retain(|deal, _| snapshot.matches(deal, field, owner));
    map.deal_info
        .retain(|deal, _| snapshot.matches(deal, field, owner));
    let records = records
        .iter()
        .filter_map(|row| {
            let mut row = row.clone();
            row.deal_ids
                .retain(|deal| snapshot.matches(deal, field, owner));
            (owner.is_none() || !row.deal_ids.is_empty()).then_some(row)
        })
        .collect();
    (records, map)
}

/// Short locks only: HubSpot I/O never holds the company-state lock.
#[derive(Default)]
pub(super) struct CompanyState {
    map: Option<Arc<CompanyMap>>,
    refreshing: bool,
    failed_at: Option<StdInstant>,
}
#[derive(Default)]
pub(super) struct Cache {
    companies: tokio::sync::Mutex<CompanyState>,
    changed: tokio::sync::Notify,
    response: tokio::sync::Mutex<Option<CachedHome>>,
}
struct CachedHome {
    // Holding this Arc also makes local index edits copy-on-write, invalidating this key.
    index: Arc<Index>,
    company_at: Option<StdInstant>,
    today: NaiveDate,
    body: axum::body::Bytes,
}
const COMPANY_RETRY: Duration = Duration::from_secs(10);

/// Used by both the index worker and requests; reserves the single refresh before spawning.
async fn companies_or_refresh(
    service: &JobReadService,
    index: &Arc<Index>,
) -> Option<Arc<CompanyMap>> {
    let cache = &service.listing_index.home;
    let mut state = cache.companies.lock().await;
    let fresh = state.map.as_ref().is_some_and(|map| {
        map.built_for == index.built_at && map.at.is_some_and(|at| at.elapsed() < COMPANY_TTL)
    });
    if !fresh
        && !state.refreshing
        && !state
            .failed_at
            .is_some_and(|at| at.elapsed() < COMPANY_RETRY)
    {
        state.refreshing = true;
        state.failed_at = None;
        let mut service = service.clone();
        service.hs = service.hs.background();
        let index = index.clone();
        tokio::spawn(async move {
            let result = fetch_companies(&service, &index).await;
            let cache = &service.listing_index.home;
            let mut state = cache.companies.lock().await;
            match result {
                Ok(map) => {
                    state.map = Some(Arc::new(map));
                    state.failed_at = None;
                }
                Err(_) => {
                    state.failed_at = Some(StdInstant::now());
                    tracing::warn!("job-copy home company refresh failed");
                }
            }
            state.refreshing = false;
            drop(state);
            cache.changed.notify_waiters();
        });
    }
    state.map.clone()
}

pub(super) async fn prefetch(service: &JobReadService, index: &Arc<Index>) {
    companies_or_refresh(service, index).await;
}

pub(super) async fn load_companies(
    service: &JobReadService,
    index: &Arc<Index>,
) -> Result<Arc<CompanyMap>, ReadError> {
    loop {
        // Register before inspecting state so a fast completion cannot be missed.
        let changed = service.listing_index.home.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if let Some(map) = companies_or_refresh(service, index).await {
            return Ok(map);
        }
        if service
            .listing_index
            .home
            .companies
            .lock()
            .await
            .failed_at
            .is_some()
        {
            return Err(fail("home_companies_unavailable"));
        }
        changed.await;
    }
}

async fn fetch_companies(service: &JobReadService, index: &Index) -> Result<CompanyMap, ReadError> {
    let started = StdInstant::now();
    let stage = StdInstant::now();
    let defs = super::pipelines::load_defs(service).await;
    tracing::info!(
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home pipeline definitions"
    );
    let deal_info: BTreeMap<String, DealInfo> = index
        .deals
        .iter()
        .map(|deal| {
            let (stage_label, active) = super::pipelines::stage_state(
                &defs,
                deal.pipeline.as_deref(),
                deal.stage.as_deref(),
            );
            (
                deal.id.clone(),
                DealInfo {
                    name: deal.name.clone(),
                    stage_label,
                    active,
                    created_at: deal.created_at.clone(),
                },
            )
        })
        .collect();
    let deal_ids: Vec<String> = index.deals.iter().map(|d| d.id.clone()).collect();
    let stage = StdInstant::now();
    let related = service
        .association_map("deals", &deal_ids, "companies")
        .await?;
    tracing::info!(
        count = deal_ids.len(),
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home company associations"
    );
    // 事業所が 2 つ以上ある取引は、結び付けを決められないので事業所なしとして扱う。
    let deal_company: BTreeMap<String, String> = related
        .into_iter()
        .filter_map(|(deal, companies)| match companies.as_slice() {
            [only] => Some((deal, only.clone())),
            _ => None,
        })
        .collect();
    let wanted: Vec<String> = deal_company
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let stage = StdInstant::now();
    let names = service
        .batch("companies", &wanted, &["name"])
        .await?
        .into_iter()
        .map(|record| {
            let name = record.value("name").map(str::to_owned);
            (record.id, name)
        })
        .collect();
    tracing::info!(
        count = wanted.len(),
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home company names"
    );
    let map = CompanyMap {
        built_for: index.built_at.clone(),
        at: Some(StdInstant::now()),
        deal_company,
        names,
        deal_info,
    };
    tracing::info!(
        count = map.names.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy home company refresh"
    );
    Ok(map)
}

/// Cache the complete encoded answer: neither scanning nor JSON encoding repeats on a hit.
async fn answer(
    service: &JobReadService,
    index: Arc<Index>,
    today: NaiveDate,
) -> axum::body::Bytes {
    let started = StdInstant::now();
    let (map, companies_ok) = match load_companies(service, &index).await {
        Ok(map) => (map, true),
        Err(_) => (Arc::new(CompanyMap::default()), false),
    };
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy home company wait"
    );
    let mut slot = service.listing_index.home.response.lock().await;
    if let Some(cached) = slot.as_ref() {
        if Arc::ptr_eq(&cached.index, &index)
            && cached.company_at == map.at
            && cached.today == today
        {
            return cached.body.clone();
        }
    }
    let stage = StdInstant::now();
    let home = compute(&index.records, &map, today);
    tracing::info!(
        count = index.records.len(),
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home compute"
    );
    let stage = StdInstant::now();
    let body = axum::body::Bytes::from(serde_json::to_vec(&json!({
        "status":"ready", "index_built_at":index.built_at, "today":today.to_string(), "companies_ok":companies_ok,
        "rules": {"trend_min_diff":TREND_MIN_DIFF, "trend_min_percent":TREND_MIN_PERCENT, "cost_floor_yen":COST_FLOOR_YEN, "cost_per_app_times":COST_PER_APP_TIMES, "stale_days":STALE_DAYS, "list_cap":LIST_CAP, "delivery_pipelines":super::pipelines::DELIVERY_PIPELINES.iter().map(|(id, label)| json!({"id":id,"label":label})).collect::<Vec<_>>()},
        "home": home,
    })).expect("home response contains only JSON-safe values"));
    tracing::info!(
        count = body.len(),
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home JSON bytes"
    );
    if companies_ok {
        *slot = Some(CachedHome {
            index,
            company_at: map.at,
            today,
            body: body.clone(),
        });
    }
    body
}

pub(in crate::handlers::job_copy_live) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<owner_todo::OwnerQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let started = StdInstant::now();
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let service = service(&access)?;
    let stage = StdInstant::now();
    // Refreshing/failed builds retain the last complete snapshot.
    let cached = service.listing_index.state.read().await;
    let snapshot = cached.snapshot.clone();
    let progress = cached.progress();
    drop(cached);
    tracing::info!(
        elapsed_ms = stage.elapsed().as_millis() as u64,
        "job-copy home index wait"
    );
    let body = match snapshot {
        Some(index) => {
            let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
            let work = owner_todo::view(service).await;
            let base = answer(service, index.clone(), today).await;
            let mut data: Value = serde_json::from_slice(&base).expect("home JSON");
            if let Some(work_snapshot) = work.snapshot {
                let (owner, _) = work_snapshot.selection(&query, &email);
                let map = load_companies(service, &index).await.unwrap_or_default();
                let (records, map) = scoped(
                    &index.records,
                    &map,
                    &work_snapshot,
                    query.field(),
                    owner.as_deref(),
                );
                data["home"] =
                    serde_json::to_value(compute(&records, &map, today)).expect("home values");
                data["work"] = work_snapshot.report(&query, &email, today, work.failed);
                if work.partial {
                    data["work"]["todos_status"] =
                        json!(if work.failed { "failed" } else { "preparing" });
                }
            } else {
                data["work"] = json!({"status":if work.failed {"failed"} else {"preparing"}});
            }
            axum::body::Bytes::from(serde_json::to_vec(&data).expect("home JSON"))
        }
        None => {
            axum::body::Bytes::from(json!({"status":"preparing", "progress":progress}).to_string())
        }
    };
    tracing::info!(
        count = body.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy home request"
    );
    Ok((
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::CONTENT_TYPE, "application/json"),
        ],
        body,
    ))
}

#[cfg(test)]
#[path = "listing_home_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "listing_home_loading_tests.rs"]
mod loading_tests;
