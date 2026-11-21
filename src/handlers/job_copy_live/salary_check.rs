//! 求人の給与チェック。HRハッカーに掲載中の求人票 (HubSpot 0-420) を読み、給与の入力の誤りを一覧にする。
//! 判定は `crate::listing_salary_check`（純粋な関数）。ここは HubSpot の読み取り・60 分の保持・応答だけ。
//! 読み取りのみで、HubSpot には何も書かない。
//! 指摘のあった求人は HRハッカーの求人ページを開いて確かめ、掲載が終わっていたものは一覧から除く。
use super::*;
use crate::listing_salary_check::{self as judge, Category};
use ts_rs::TS;

/// 結果を全員で共有して持つ時間。
const TTL: Duration = Duration::from_secs(60 * 60);
/// 「最新にする」が続けて押されても、この時間内は読み直さない（同時に開いた人の分をまとめる）。
const FRESH_MIN_GAP: Duration = Duration::from_secs(30);
/// 100 件ずつ。掲載中は約 1,850 件 = 約 19 回の Search。
const PAGE: usize = 100;
const MAX_PAGES: usize = 100;
const PROPERTIES: [&str; 5] = [
    "id_hrhakkaa",
    "url_hrhakkaa",
    "hs_name",
    "todoufuken",
    "hrh_kyuujinhyou_honbun",
];

#[derive(Deserialize)]
pub(super) struct SalaryCheckQuery {
    /// 1 なら保持している結果を使わず読み直す（画面の「最新にする」）。
    fresh: Option<u8>,
}

/// `GET /api/listing-check/salary` の応答。
#[derive(Clone, Debug, Serialize, TS)]
pub struct SalaryCheckResponse {
    /// 集計した時刻（RFC 3339、UTC）。
    pub checked_at: String,
    /// 対象にした求人の数（HRハッカーで公開中の求人票）。
    pub total_listings: u32,
    /// 指摘が 1 つ以上ある求人の数（掲載が終わっていた求人は除く）。
    pub flagged_listings: u32,
    /// 指摘があったが、HRハッカーのページが無くなっていた（掲載が終わっていた）ため一覧から除いた求人の数。
    pub ended_listings: u32,
    /// 指摘があり一覧に出しているが、HRハッカーのページを開けず掲載中かを確かめられなかった求人の数。
    pub unconfirmed_listings: u32,
    /// 最低賃金の出典。
    pub min_wage_source: String,
    /// 区分ごとの指摘の数（0 件の区分は入れない）。
    pub counts: Vec<SalaryCheckCount>,
    pub findings: Vec<SalaryCheckFinding>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct SalaryCheckCount {
    pub category: String,
    pub label: String,
    /// `strong` / `medium` / `weak` / `unknown`
    pub severity: String,
    pub count: u32,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct SalaryCheckFinding {
    /// HubSpot の求人票の ID。
    pub listing_id: String,
    /// HRハッカーの求人ID。
    pub hrh_id: String,
    /// HRハッカーの求人ページ。
    pub hrh_url: Option<String>,
    /// HubSpot の求人票のページ。
    pub hubspot_url: String,
    /// 案件名。
    pub name: String,
    pub prefecture: String,
    pub category: String,
    pub category_label: String,
    /// `strong` / `medium` / `weak` / `unknown`
    pub severity: String,
    /// 説明文（額と比べた相手）。
    pub detail: String,
}

struct Listing {
    id: String,
    properties: BTreeMap<String, Option<String>>,
}

type Slot = tokio::sync::Mutex<Option<(Instant, Arc<SalaryCheckResponse>)>>;
fn slot() -> &'static Slot {
    static SLOT: std::sync::OnceLock<Slot> = std::sync::OnceLock::new();
    SLOT.get_or_init(Slot::default)
}

fn http_url(value: &str) -> Option<String> {
    let value = value.trim();
    (value.starts_with("https://") || value.starts_with("http://")).then(|| value.to_string())
}

fn build(listings: &[Listing], portal_id: &str, checked_at: String) -> SalaryCheckResponse {
    let mut findings = Vec::new();
    let mut flagged = 0;
    for listing in listings {
        let prop = |key: &str| {
            listing
                .properties
                .get(key)
                .and_then(|v| v.as_deref())
                .unwrap_or("")
        };
        let prefecture = prop("todoufuken");
        let found = judge::check(
            Some(prefecture).filter(|p| !p.is_empty()),
            prop("hrh_kyuujinhyou_honbun"),
        );
        if !found.is_empty() {
            flagged += 1;
        }
        for item in found {
            findings.push((
                item.category,
                SalaryCheckFinding {
                    listing_id: listing.id.clone(),
                    hrh_id: prop("id_hrhakkaa").to_string(),
                    hrh_url: http_url(prop("url_hrhakkaa")),
                    hubspot_url: format!(
                        "https://app.hubspot.com/contacts/{portal_id}/record/0-420/{}",
                        listing.id
                    ),
                    name: prop("hs_name").to_string(),
                    prefecture: prefecture.to_string(),
                    category: item.category.key().to_string(),
                    category_label: item.category.label().to_string(),
                    severity: item.category.severity().key().to_string(),
                    detail: item.detail,
                },
            ));
        }
    }
    // 強い指摘を先に。同じ区分の中は読んだ順のまま。
    findings.sort_by_key(|(category, _)| (category.severity(), *category));
    let findings: Vec<SalaryCheckFinding> = findings.into_iter().map(|(_, f)| f).collect();
    SalaryCheckResponse {
        checked_at,
        total_listings: listings.len() as u32,
        flagged_listings: flagged,
        ended_listings: 0,
        unconfirmed_listings: 0,
        min_wage_source: judge::MIN_WAGE_SOURCE.to_string(),
        counts: counts(&findings),
        findings,
    }
}

fn counts(findings: &[SalaryCheckFinding]) -> Vec<SalaryCheckCount> {
    Category::ALL
        .iter()
        .filter_map(|category| {
            let count = findings
                .iter()
                .filter(|f| f.category == category.key())
                .count();
            (count > 0).then(|| SalaryCheckCount {
                category: category.key().to_string(),
                label: category.label().to_string(),
                severity: category.severity().key().to_string(),
                count: count as u32,
            })
        })
        .collect()
}

/// HRハッカーの求人ページの状態。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    /// ページが開ける（掲載中）。
    Live,
    /// ページが無い（掲載が終わっている）。
    Ended,
    /// 開けず、どちらか分からない。
    Unknown,
}

/// 求人ページを開く先はこのサイトだけ（HubSpot の項目の値をそのまま開かない）。
const HRH_ORIGIN: &str = "https://hr-hacker.com/";
/// 同時に開くページの数。指摘のあった求人（数十件）だけを開く。
const PAGE_CONCURRENCY: usize = 4;
const PAGE_TIMEOUT: Duration = Duration::from_secs(20);

async fn page_state(http: &reqwest::Client, url: Option<&str>) -> Page {
    let Some(url) = url.filter(|u| u.starts_with(HRH_ORIGIN)) else {
        return Page::Unknown;
    };
    match http.get(url).send().await {
        Ok(res) if res.status().is_success() => Page::Live,
        Ok(res) if matches!(res.status().as_u16(), 404 | 410) => Page::Ended,
        _ => Page::Unknown,
    }
}

/// 指摘のあった求人のページを開き、掲載が終わっていた求人を一覧から除く。
/// HubSpot の「公開」は同期の時点の値で、掲載が終わったあとも残ることがあるため。
async fn keep_live_only(data: &mut SalaryCheckResponse) {
    use futures::StreamExt;
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    for f in &data.findings {
        if !targets.iter().any(|(id, _)| *id == f.listing_id) {
            targets.push((f.listing_id.clone(), f.hrh_url.clone()));
        }
    }
    let Ok(http) = reqwest::Client::builder()
        .timeout(PAGE_TIMEOUT)
        .user_agent("Mozilla/5.0 (compatible; listing-check)")
        .build()
    else {
        data.unconfirmed_listings = targets.len() as u32;
        return;
    };
    let states: Vec<(String, Page)> = futures::stream::iter(targets)
        .map(|(id, url)| {
            let http = http.clone();
            async move {
                let state = page_state(&http, url.as_deref()).await;
                (id, state)
            }
        })
        .buffer_unordered(PAGE_CONCURRENCY)
        .collect()
        .await;
    let ended: BTreeSet<&str> = states
        .iter()
        .filter(|(_, s)| *s == Page::Ended)
        .map(|(id, _)| id.as_str())
        .collect();
    data.findings
        .retain(|f| !ended.contains(f.listing_id.as_str()));
    data.ended_listings = ended.len() as u32;
    data.unconfirmed_listings = states.iter().filter(|(_, s)| *s == Page::Unknown).count() as u32;
    data.flagged_listings = data.flagged_listings.saturating_sub(data.ended_listings);
    data.counts = counts(&data.findings);
}

/// HRハッカーの求人ID があり、HRハッカーで公開中の求人票をすべて読む（背景の優先度）。
async fn load(service: &JobReadService) -> Result<Vec<Listing>, ReadError> {
    let hs = service.hs.background();
    let mut out = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut body = json!({
            "filterGroups": [{"filters": [
                {"propertyName": "id_hrhakkaa", "operator": "HAS_PROPERTY"},
                {"propertyName": "baitai_genjoukyou_hrhakkaa", "operator": "EQ", "value": "公開"}
            ]}],
            "properties": PROPERTIES,
            "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
            "limit": PAGE,
        });
        if let Some(after) = &after {
            body["after"] = json!(after);
        }
        let page = hs.search("0-420", body).await.map_err(client_error)?;
        let results = page["results"]
            .as_array()
            .ok_or_else(|| fail("hubspot_invalid_response"))?;
        for row in results {
            let id = row["id"]
                .as_str()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            let properties = row["properties"]
                .as_object()
                .map(|map| {
                    map.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().map(str::to_owned)))
                        .collect()
                })
                .unwrap_or_default();
            out.push(Listing {
                id: id.to_string(),
                properties,
            });
        }
        after = page["paging"]["next"]["after"].as_str().map(str::to_owned);
        if after.is_none() {
            return Ok(out);
        }
    }
    // 上限まで読んでも終わらないときは、途中までの結果を全件として出さない。
    Err(fail("hubspot_invalid_response"))
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<SalaryCheckQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    // 鍵を持ったまま読むので、同時に来た要求は 1 回の読み取りにまとまる。
    let mut held = slot().lock().await;
    let max_age = if query.fresh == Some(1) {
        FRESH_MIN_GAP
    } else {
        TTL
    };
    if let Some((at, data)) = held.as_ref() {
        if at.elapsed() < max_age {
            return Ok(([(header::CACHE_CONTROL, "no-store")], Json(data.clone())));
        }
    }
    let listings = load(service).await?;
    let mut data = build(
        &listings,
        &crate::hubspot::deep_link::hubspot_portal_id(),
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    );
    keep_live_only(&mut data).await;
    let data = Arc::new(data);
    *held = Some((Instant::now(), data.clone()));
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
}
