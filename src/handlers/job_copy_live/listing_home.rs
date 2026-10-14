//! 案件管理ホーム: 「まず見るもの」の件数と、事業所（顧客）ごとの表。
//! 全件索引（メモリ上）と、求人に付いた週ごとの実績だけから計算する。求人ごとの HubSpot 呼び出しはしない。
//! 事業所は 取引→事業所 の関連を一度だけ読み、索引を作り直すか 30 分たつまで使い回す。
//! 見る基準はすべてこのファイルの定数。画面にも同じ数字を注記として出す（基準は合意前の仮置き）。
use super::*;
use chrono::NaiveDate;
use std::time::Instant as StdInstant;

/// 「前の 4 週より多い／少ない」と言うために必要な応募の差（件）。差がこれ未満なら「ほぼ同じ」。
pub(super) const TREND_MIN_DIFF: i64 = 2;
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
#[derive(Clone, Debug, PartialEq)]
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
    pub(super) deals: usize,
    pub(super) live_listings: usize,
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
    let pick = |test: &dyn Fn(i64) -> bool| -> Vec<&Listing> {
        live.iter()
            .copied()
            .filter(|row| {
                row.perf
                    .as_ref()
                    .is_some_and(|p| test(p.recent_apps - p.prev_apps))
            })
            .collect()
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
        card("more", "直近4週の応募が、前の4週より多い求人", format!("掲載中で、直近4週の応募が前の4週より{TREND_MIN_DIFF}件以上多い求人。週ごとの実績がある求人だけを数えます。"), pick(&|d| d >= TREND_MIN_DIFF)),
        card("same", "直近4週の応募が、前の4週とほぼ同じ求人", format!("掲載中で、直近4週と前の4週の応募の差が{TREND_MIN_DIFF}件未満の求人。"), pick(&|d| d.abs() < TREND_MIN_DIFF)),
        card("less", "直近4週の応募が、前の4週より少ない求人", format!("掲載中で、直近4週の応募が前の4週より{TREND_MIN_DIFF}件以上少ない求人。"), pick(&|d| d <= -TREND_MIN_DIFF)),
        card("costly", "課金が高いまま応募が少ない求人", format!("掲載中で、直近4週の課金が{}以上あり、応募が0件、または応募1件あたりの課金が応募のある求人の中央値の{COST_PER_APP_TIMES}倍以上の求人。", yen_label(COST_FLOOR_YEN)), costly),
        card("stale", "データが止まっている求人", format!("掲載中で、週ごとの実績の最新の週が{STALE_DAYS}日より前に終わっている求人。この間の応募・課金は少なく見えます。"), stale),
        card("occupation", "市場の職種が未指定の求人", "掲載中で、市場と比べる職種をまだ指定していない求人。".to_owned(), unset),
    ];

    #[derive(Default)]
    struct Agg {
        deals: BTreeSet<String>,
        live: usize,
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
        .map(|(id, agg)| Customer {
            name: map.names.get(&id).cloned().flatten(),
            company_id: id,
            deals: agg.deals.len(),
            live_listings: agg.live,
            recent_apps: agg.recent,
            prev_apps: agg.prev,
            recent_cost: agg.cost,
            cost_per_app: (agg.recent > 0).then(|| agg.cost / agg.recent),
        })
        .collect();
    customers.sort_by(|a, b| {
        b.recent_apps
            .cmp(&a.recent_apps)
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

async fn load_companies(service: &JobReadService, index: &Index) -> Result<CompanyMap, ReadError> {
    let mut slot = service.listing_index.home_companies.lock().await;
    if let Some(map) = slot.as_ref() {
        if map.built_for == index.built_at && map.at.is_some_and(|at| at.elapsed() < COMPANY_TTL) {
            return Ok(map.clone());
        }
    }
    let deal_ids: Vec<String> = index.deals.iter().map(|d| d.id.clone()).collect();
    let related = service
        .association_map("deals", &deal_ids, "companies")
        .await?;
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
    let names = service
        .batch("companies", &wanted, &["name"])
        .await?
        .into_iter()
        .map(|record| {
            let name = record.value("name").map(str::to_owned);
            (record.id, name)
        })
        .collect();
    let map = CompanyMap {
        built_for: index.built_at.clone(),
        at: Some(StdInstant::now()),
        deal_company,
        names,
    };
    *slot = Some(map.clone());
    Ok(map)
}

pub(in crate::handlers::job_copy_live) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = service(&access)?;
    let snapshot = service.listing_index.state.read().await.snapshot.clone();
    let Some(index) = snapshot else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"status":"preparing"})),
        ));
    };
    let today = (chrono::Utc::now() + chrono::Duration::hours(9)).date_naive();
    // 事業所が読めなくてもカードは返す。表は空にして、理由を伝える。
    let (map, companies_ok) = match load_companies(service, &index).await {
        Ok(map) => (map, true),
        Err(_) => (CompanyMap::default(), false),
    };
    let home = compute(&index.records, &map, today);
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "status":"ready", "index_built_at":index.built_at, "today":today.to_string(), "companies_ok":companies_ok,
            "rules": {"trend_min_diff":TREND_MIN_DIFF, "cost_floor_yen":COST_FLOOR_YEN, "cost_per_app_times":COST_PER_APP_TIMES, "stale_days":STALE_DAYS, "list_cap":LIST_CAP},
            "home": home,
        })),
    ))
}

#[cfg(test)]
#[path = "listing_home_tests.rs"]
mod tests;
