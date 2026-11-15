//! Read-only: the application dates of every listing in one deal (案件の画面の週ごとの応募)。
//! Counts and dates only; no applicant attribute leaves the server. The relation listing -> deal
//! is checked against HubSpot for each listing, so a browser-provided id is not trusted.
use super::listings::{
    airwork_weekly_perf, indeed_weekly_perf, AIRWORK_WEEKLY_PERF, INDEED_WEEKLY_PERF,
};
use super::{authorized_user, valid_id, Access, ReadError, Record};
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tower_sessions::Session;

const MAX_LISTINGS: usize = 30;

#[derive(Deserialize)]
pub(super) struct DealSummaryQuery {
    deal: String,
    ids: String,
}

/// The calendar day (JST) of an application date: a plain `YYYY-MM-DD`, or a timestamp
/// (RFC 3339) converted to Japan time. None when unreadable.
pub(super) fn jst_day(value: &str) -> Option<String> {
    let value = value.trim();
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(date.format("%Y-%m-%d").to_string());
    }
    let jst = FixedOffset::east_opt(9 * 3600)?;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&jst).format("%Y-%m-%d").to_string())
}

/// HubSpot property read together with `yingmuri`: the day an application was created.
pub(super) const CREATED: &str = "hs_createdate";
/// The properties every application-date read asks for (the same batch read, no extra call).
pub(super) const DATE_PROPERTIES: [&str; 2] = ["yingmuri", CREATED];

/// The day (JST, `YYYY-MM-DD`) an application is counted on, and whether it came from the
/// creation time. The 応募日 (`yingmuri`) is used when it is readable; when it is empty the
/// creation time is used instead (HubSpot creates the record 0–2 days after the real 応募日).
/// None only when both are missing or unreadable.
pub(super) fn application_day_of(
    yingmuri: Option<&str>,
    created: Option<&str>,
) -> Option<(String, bool)> {
    fn blank(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    if let Some(day) = blank(yingmuri).and_then(jst_day) {
        return Some((day, false));
    }
    let created = blank(created)?;
    // HubSpot sends an ISO time; some paths hold epoch milliseconds.
    let day = jst_day(created).or_else(|| {
        let at = DateTime::<chrono::Utc>::from_timestamp_millis(created.parse::<i64>().ok()?)?;
        Some(
            at.with_timezone(&FixedOffset::east_opt(9 * 3600)?)
                .format("%Y-%m-%d")
                .to_string(),
        )
    })?;
    Some((day, true))
}

impl Record {
    /// See [`application_day_of`].
    pub(super) fn application_day(&self) -> Option<(String, bool)> {
        application_day_of(self.value("yingmuri"), self.value(CREATED))
    }
}

/// 同じ人の応募を見分けるための応募の項目（サーバーの中だけで使い、画面へは返さない）。
pub(super) const IDENTITY_PROPERTIES: [&str; 4] = [
    "ouboshashimei",
    "denwabangou",
    "meeruadoresu",
    "seinengappi",
];

/// 同じ人かを比べる鍵。電話は下 9 桁（CRM の目印と同じ）、メールは小文字、
/// 氏名だけ・生年月日だけでは同じ人にしない（同姓同名・同じ誕生日の別人がいるため、氏名と生年月日の組で見る）。
fn identity_keys(record: &Record) -> Vec<String> {
    let mut keys = vec![];
    if let Some(phone) = record.value("denwabangou") {
        let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
        if digits.len() >= 9 && !placeholder_phone(&digits) {
            keys.push(format!("tel:{}", &digits[digits.len() - 9..]));
        }
    }
    if let Some(mail) = record.value("meeruadoresu") {
        let mail = mail.trim().to_lowercase();
        if mail.contains('@') && !placeholder_mail(&mail) {
            keys.push(format!("mail:{mail}"));
        }
    }
    let name = record
        .value("ouboshashimei")
        .map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>());
    let birth = record
        .value("seinengappi")
        .and_then(|b| jst_day(b).or_else(|| b.trim().get(..10).map(str::to_owned)));
    if let (Some(name), Some(birth)) = (name.filter(|n| !n.is_empty()), birth) {
        keys.push(format!("person:{name}:{birth}"));
    }
    keys
}

/// 入力欄を埋めるための仮の電話番号（000000000・111111111 のような同じ数字だけ、0123456789 のような並び）。
/// 別の人どうしを同じ人にしないよう、鍵にしない。
fn placeholder_phone(digits: &str) -> bool {
    let tail = &digits[digits.len() - 9..];
    let same = tail.chars().all(|c| Some(c) == tail.chars().next());
    // 並びは番号全体で見る（本物の番号の下 9 桁がたまたま並ぶことがあるため）。
    let sequence = [
        "123456789",
        "1234567890",
        "0123456789",
        "987654321",
        "9876543210",
        "0987654321",
    ]
    .contains(&digits);
    same || sequence
}
/// 仮のメールアドレス（example.* などの例の名前・none@ / test@ のような中身の無い名前・同じ文字だけ）。
fn placeholder_mail(mail: &str) -> bool {
    let Some((local, domain)) = mail.split_once('@') else {
        return true;
    };
    let same = local.chars().all(|c| Some(c) == local.chars().next());
    same || [
        "none", "no", "test", "dummy", "sample", "nomail", "noemail", "xxx", "x",
    ]
    .contains(&local)
        || domain.starts_with("example.")
        || ["example", "test", "invalid", "localhost"].contains(&domain)
}

/// 同じ求人への同じ人の 2 件目以降の応募 ID（2026-11-15 ユーザー決定: 数えない）。
/// 同じ人 = 電話（下 9 桁）・メール・氏名と生年月日の組 のどれかが一致。応募日の早い順に見て最初の 1 件を残す。
/// `rows` は 1 つの求人の応募だけを渡す。
pub(super) fn repeat_applications(rows: &[Record]) -> BTreeSet<String> {
    applicant_groups(rows)
        .into_iter()
        .filter(|(id, kept)| id != kept)
        .map(|(id, _)| id)
        .collect()
}

/// 応募 ID → その人のまとまりで残す 1 件の応募 ID（`repeat_applications` と同じ判定）。
/// 残す 1 件は自分自身を指す。
pub(super) fn applicant_groups(rows: &[Record]) -> BTreeMap<String, String> {
    // 鍵（電話・メール・氏名と生年月日）を 1 つでも共有する応募を、並び順によらず 1 人にまとめる
    // （union-find）。電話だけ → メールだけ → 電話とメール のように後の応募で初めてつながる場合も 1 人。
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..rows.len()).collect();
    let mut owner_of_key: BTreeMap<String, usize> = BTreeMap::new();
    for (i, record) in rows.iter().enumerate() {
        for key in identity_keys(record) {
            match owner_of_key.get(&key) {
                Some(&j) => {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                }
                None => {
                    owner_of_key.insert(key, i);
                }
            }
        }
    }
    // 各まとまりで残すのは、応募日の最も早い 1 件（同じ日は作成時刻、次に ID の順。
    // 日付が分からない応募は日付のある応募より後）。
    let order = |r: &Record| {
        let day = r.application_day().map(|(day, _)| day);
        (
            day.is_none(),
            day,
            r.value(CREATED).map(str::to_owned),
            r.id.clone(),
        )
    };
    let mut keep: BTreeMap<usize, usize> = BTreeMap::new();
    for i in 0..rows.len() {
        let group = root(&mut parent, i);
        let best = keep.entry(group).or_insert(i);
        if order(&rows[i]) < order(&rows[*best]) {
            *best = i;
        }
    }
    (0..rows.len())
        .map(|i| {
            let best = keep[&root(&mut parent, i)];
            (rows[i].id.clone(), rows[best].id.clone())
        })
        .collect()
}

/// 応募の日付の項目。2 件目以降の値で残す 1 件を埋めない（残す 1 件の応募日で数えるため）。
const DATE_FIELDS: [&str; 2] = ["yingmuri", CREATED];

/// 同じ人の 2 件目以降を残す 1 件に寄せる。数える応募（残す 1 件）と、2 件目以降（寄せた先の ID 付き）を返す。
/// 残す 1 件の項目が空なら、2 件目以降の値（応募日が早い順）で埋める。電話ヒアリング・応募の理由・属性が
/// 2 件目にだけあっても消えず、同じ人なら 1 回だけ数える。
pub(super) fn merge_repeat_applications(rows: Vec<Record>) -> (Vec<Record>, Vec<(Record, String)>) {
    let groups = applicant_groups(&rows);
    let (repeats, mut kept): (Vec<Record>, Vec<Record>) =
        rows.into_iter().partition(|r| groups[&r.id] != r.id);
    let mut repeats: Vec<(Record, String)> = repeats
        .into_iter()
        .map(|r| {
            let to = groups[&r.id].clone();
            (r, to)
        })
        .collect();
    repeats.sort_by_key(|(r, _)| {
        (
            r.application_day().map(|(d, _)| d).is_none(),
            r.application_day().map(|(d, _)| d),
            r.id.clone(),
        )
    });
    let index: BTreeMap<String, usize> = kept
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.clone(), i))
        .collect();
    for (repeat, to) in &repeats {
        let Some(&i) = index.get(to) else { continue };
        for (key, value) in &repeat.properties {
            if DATE_FIELDS.contains(&key.as_str())
                || value.as_deref().is_none_or(|v| v.trim().is_empty())
            {
                continue;
            }
            let slot = kept[i].properties.entry(key.clone()).or_insert(None);
            if slot.as_deref().is_none_or(|v| v.trim().is_empty()) {
                *slot = value.clone();
            }
        }
    }
    (kept, repeats)
}

/// 同じ人の 2 件目以降を除いた応募。
pub(super) fn first_applications(rows: Vec<Record>) -> Vec<Record> {
    let repeats = repeat_applications(&rows);
    rows.into_iter()
        .filter(|r| !repeats.contains(&r.id))
        .collect()
}

/// Day per application (None when neither date exists) and how many of them came from the
/// creation time. The latter is inside the dated count, not added to it.
pub(super) fn days_of(rows: &[Record]) -> (Vec<Option<String>>, usize) {
    let found: Vec<_> = rows.iter().map(Record::application_day).collect();
    let created = found.iter().flatten().filter(|(_, c)| *c).count();
    (found.into_iter().map(|d| d.map(|d| d.0)).collect(), created)
}

/// Counts of dated applications by day, and the number without a usable date.
pub(super) fn count_by_date(days: &[Option<String>]) -> (BTreeMap<String, usize>, usize) {
    let mut by_date = BTreeMap::new();
    let mut missing = 0;
    for day in days {
        match day {
            Some(day) => *by_date.entry(day.clone()).or_insert(0) += 1,
            None => missing += 1,
        }
    }
    (by_date, missing)
}

/// Applications linked to more than one of the requested listings. Each is counted once per
/// listing in the listing's own series, so the deal total must subtract the extra copies:
/// (number of such applications, extra copies by day, extra copies without a date).
fn shared_overlap(
    per_listing: &[Vec<(String, Option<String>)>],
) -> (usize, BTreeMap<String, usize>, usize) {
    let mut seen = BTreeMap::<&str, (usize, Option<&str>)>::new();
    for listing in per_listing {
        for (id, day) in listing {
            let entry = seen.entry(id.as_str()).or_insert((0, day.as_deref()));
            entry.0 += 1;
        }
    }
    let mut by_day = BTreeMap::new();
    let mut undated = 0;
    let mut shared = 0;
    for (times, day) in seen.values().filter(|(times, _)| *times > 1) {
        shared += 1;
        match day {
            Some(day) => *by_day.entry((*day).to_owned()).or_insert(0) += times - 1,
            None => undated += times - 1,
        }
    }
    (shared, by_day, undated)
}

/// The weekly results (Indeed for HRハッカー, AirWork for AirWork) of each listing, read for all of
/// them in one batch call. A listing with no usable value is simply absent (the screen shows
/// "no billing data").
fn perf_by_listing(records: &[Record]) -> BTreeMap<String, (&'static str, Value)> {
    records
        .iter()
        .filter_map(|record| {
            if let Some(v) = indeed_weekly_perf(record) {
                return Some((record.id.clone(), ("indeed_weekly_perf", v)));
            }
            Some((
                record.id.clone(),
                ("airwork_weekly_perf", airwork_weekly_perf(record)?),
            ))
        })
        .collect()
}

/// The one company related to the deal (id, name). None when there are none or several.
async fn deal_company(
    service: &super::JobReadService,
    deal: &str,
) -> Result<Option<(String, String)>, ReadError> {
    let companies = service.all_associations("deals", deal, "companies").await?;
    let [company] = companies.as_slice() else {
        return Ok(None);
    };
    let records = service
        .batch("companies", std::slice::from_ref(company), &["name"])
        .await?;
    Ok(records
        .first()
        .and_then(|record| record.value("name"))
        .map(|name| (company.clone(), name.to_owned())))
}

/// 事業所の画面は求人 30 件ぶんをまとめて読む。この時間を超えた求人は「読めなかった」にする。
const DEAL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(40);

async fn read_listing(
    service: &super::JobReadService,
    deal: &str,
    id: &str,
) -> Result<(Value, Vec<(String, Option<String>)>), ReadError> {
    let deals = service.all_associations("0-420", id, "deals").await?;
    if !deals.iter().any(|d| d == deal) {
        return Err(ReadError(StatusCode::FORBIDDEN, "listing_not_in_deal"));
    }
    let applications = service.all_associations("0-420", id, "0-421").await?;
    let properties: Vec<_> = DATE_PROPERTIES
        .into_iter()
        .chain(IDENTITY_PROPERTIES)
        .collect();
    let rows = first_applications(service.batch("0-421", &applications, &properties).await?);
    let (days, from_created) = days_of(&rows);
    let (by_date, missing) = count_by_date(&days);
    let pairs = rows.iter().map(|r| r.id.clone()).zip(days).collect();
    Ok((
        json!({"listing_id":id,"total":rows.len(),"missing_date":missing,"created_date_count":from_created,"by_date":by_date,
            "fetched_at":chrono::Utc::now().to_rfc3339()}),
        pairs,
    ))
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<DealSummaryQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    valid_id(&query.deal)?;
    let ids: Vec<String> = query
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() || ids.len() > MAX_LISTINGS {
        return Err(ReadError(StatusCode::BAD_REQUEST, "invalid_listing_ids"));
    }
    for id in &ids {
        valid_id(id)?;
    }
    // 求人ごとの読み取りは同時に 4 本まで進め、全体の締め切り (`DEAL_DEADLINE`) を超えた求人は
    // 「読めなかった」として返す（画面は「—」にし、残りの求人の数字は出す）。
    let deadline = tokio::time::Instant::now() + DEAL_DEADLINE;
    let limit = Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, id) in ids.iter().enumerate() {
        let (service, limit, id, deal) = (
            Arc::clone(service),
            Arc::clone(&limit),
            id.clone(),
            query.deal.clone(),
        );
        tasks.spawn(async move {
            let read = tokio::time::timeout_at(deadline, async {
                let _permit = limit.acquire().await.ok();
                read_listing(&service, &deal, &id).await
            })
            .await
            .unwrap_or(Err(ReadError(
                StatusCode::GATEWAY_TIMEOUT,
                "applications_timeout",
            )));
            (index, id, read)
        });
    }
    let mut results = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        if let Ok(item) = joined {
            results.push(item);
        }
    }
    results.sort_by_key(|(index, ..)| *index);
    let mut listings = Vec::new();
    let mut linked = Vec::new();
    for (_, id, read) in results {
        match read {
            Ok((value, pairs)) => {
                listings.push(value);
                linked.push(pairs);
            }
            // The deal relation is a permission check: fail the whole request.
            Err(error) if error.0 == StatusCode::FORBIDDEN => return Err(error),
            Err(error) => listings.push(json!({"listing_id":id,"error":error.1})),
        }
    }
    let (shared, shared_by_date, shared_missing_date) = shared_overlap(&linked);
    // One batch call for every listing's weekly results; a failure only hides billing.
    let perf = match service
        .batch(
            "0-420",
            &ids,
            &[INDEED_WEEKLY_PERF, AIRWORK_WEEKLY_PERF, "id_airwork"],
        )
        .await
    {
        Ok(records) => perf_by_listing(&records),
        Err(error) => {
            tracing::warn!(code = error.1, "job-copy deal billing read failed");
            BTreeMap::new()
        }
    };
    for entry in &mut listings {
        let found = entry["listing_id"].as_str().and_then(|id| perf.get(id));
        entry["indeed_weekly_perf"] = Value::Null;
        entry["airwork_weekly_perf"] = Value::Null;
        if let Some((key, value)) = found {
            entry[*key] = value.clone();
        }
    }
    let company = deal_company(service, &query.deal).await.ok().flatten();
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "deal_id": query.deal,
            "company": company.map(|(id, name)| json!({"id": id, "name": name})),
            "fetched_at": chrono::Utc::now().to_rfc3339(),
            "listings": listings,
            "shared_applications": shared,
            "shared_by_date": shared_by_date,
            "shared_missing_date": shared_missing_date,
        })),
    ))
}

#[cfg(test)]
mod tests {

    async fn mock(
        extra_deal: &'static str,
    ) -> (super::super::JobReadService, tokio::task::JoinHandle<()>) {
        let app = axum::Router::new().fallback(axum::routing::any(
            move |uri: axum::http::Uri, _body: axum::body::Bytes| async move {
                let path = uri.path().to_owned();
                axum::Json(if path.ends_with("/associations/deals") {
                    json!({"results":[{"toObjectId":extra_deal.parse::<u64>().unwrap()}]})
                } else if path.ends_with("/associations/0-421") {
                    json!({"results":[{"toObjectId":501},{"toObjectId":502},{"toObjectId":503}]})
                } else if path == "/crm/v3/objects/0-421/batch/read" {
                    // 503 は 501 と同じ人（電話の書き方だけ違う）。
                    json!({"results":[
                        {"id":"501","properties":{"yingmuri":"2026-10-01","ouboshashimei":"架空 一郎","denwabangou":"090-0000-1111","meeruadoresu":"ichiro@example.test","seinengappi":"1990-04-01"}},
                        {"id":"502","properties":{"yingmuri":"2026-10-01","ouboshashimei":"架空 二郎","denwabangou":"080-0000-2222"}},
                        {"id":"503","properties":{"yingmuri":"2026-10-02","ouboshashimei":"架空一郎","denwabangou":"09000001111"}}]})
                } else {
                    panic!("unexpected {path}")
                })
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = super::super::JobReadService::for_test(format!(
            "http://{}",
            listener.local_addr().unwrap()
        ));
        (
            service,
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }),
        )
    }

    #[tokio::test]
    async fn one_listing_counts_its_applications_by_day() {
        let (service, server) = mock("10").await;
        let (value, pairs) = read_listing(&service, "10", "7").await.unwrap();
        // 同じ人の 2 件目（503）は数えない。
        assert_eq!(value["total"], 2);
        assert_eq!(value["by_date"], json!({"2026-10-01":2}));
        assert_eq!(pairs.len(), 2);
        let text = value.to_string();
        for secret in [
            "架空",
            "090-0000-1111",
            "09000001111",
            "080-0000-2222",
            "ichiro",
            "example.test",
            "1990-04-01",
        ] {
            assert!(
                !text.contains(secret),
                "{secret} must not be in the response"
            );
        }
        server.abort();
    }

    fn applicant(id: &str, day: &str, props: &[(&str, &str)]) -> Record {
        let mut properties: BTreeMap<String, Option<String>> = props
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Some((*v).to_owned())))
            .collect();
        properties.insert("yingmuri".into(), Some(day.into()));
        Record {
            id: id.into(),
            properties,
        }
    }

    #[test]
    fn the_same_person_applying_again_to_one_listing_counts_once() {
        // 本番の形: 11/02 と 11/05 が同じ人、ほかに別の人が 1 件。3 件ではなく 2 件。
        let rows = vec![
            applicant(
                "3",
                "2026-11-05",
                &[
                    ("ouboshashimei", "架空 太郎"),
                    ("denwabangou", "090-0000-1111"),
                ],
            ),
            applicant(
                "1",
                "2026-11-02",
                &[
                    ("ouboshashimei", "架空太郎"),
                    ("denwabangou", "+81 90 0000 1111"),
                ],
            ),
            applicant(
                "2",
                "2026-11-03",
                &[
                    ("ouboshashimei", "架空 花子"),
                    ("denwabangou", "080-0000-2222"),
                ],
            ),
        ];
        assert_eq!(repeat_applications(&rows), BTreeSet::from(["3".to_owned()]));
        let kept = first_applications(rows);
        let (days, _) = days_of(&kept);
        let (by_date, missing) = count_by_date(&days);
        assert_eq!(kept.len(), 2);
        assert_eq!(missing, 0);
        assert_eq!(
            by_date,
            BTreeMap::from([("2026-11-02".to_owned(), 1), ("2026-11-03".to_owned(), 1)])
        );
    }

    #[test]
    fn a_chain_found_only_by_a_later_application_is_still_one_person() {
        // 電話だけ（11/01）→ メールだけ（11/02）→ 電話とメール（11/03）。3 件目で初めてつながる。
        let rows = [
            applicant("1", "2026-11-01", &[("denwabangou", "090-3333-4444")]),
            applicant("2", "2026-11-02", &[("meeruadoresu", "ben@kakuu.jp")]),
            applicant(
                "3",
                "2026-11-03",
                &[
                    ("denwabangou", "09033334444"),
                    ("meeruadoresu", "ben@kakuu.jp"),
                ],
            ),
        ];
        assert_eq!(
            repeat_applications(&rows),
            BTreeSet::from(["2".to_owned(), "3".to_owned()])
        );
        // 並びを逆にしても、残すのは応募日の最も早い 1 件
        let reversed: Vec<Record> = rows.iter().rev().cloned().collect();
        assert_eq!(
            repeat_applications(&reversed),
            BTreeSet::from(["2".to_owned(), "3".to_owned()])
        );
    }

    #[test]
    fn same_day_keeps_the_earliest_created_then_the_smaller_id_and_undated_comes_last() {
        let with_created = |id: &str, day: &str, created: &str| {
            let mut r = applicant(id, day, &[("denwabangou", "090-5555-6666")]);
            r.properties.insert(CREATED.into(), Some(created.into()));
            r
        };
        let same_day = [
            with_created("9", "2026-11-02", "2026-11-02T05:00:00Z"),
            with_created("8", "2026-11-02", "2026-11-02T01:00:00Z"),
        ];
        assert_eq!(
            repeat_applications(&same_day),
            BTreeSet::from(["9".to_owned()])
        );
        let same_time = [
            with_created("9", "2026-11-02", "2026-11-02T01:00:00Z"),
            with_created("8", "2026-11-02", "2026-11-02T01:00:00Z"),
        ];
        assert_eq!(
            repeat_applications(&same_time),
            BTreeSet::from(["9".to_owned()])
        );
        // 応募日も作成日も無い応募は、日付のある応募より後に回す
        let mut undated = applicant("1", "", &[("denwabangou", "090-5555-6666")]);
        undated.properties.remove("yingmuri");
        let rows = [
            undated,
            applicant("2", "2026-11-05", &[("denwabangou", "090-5555-6666")]),
        ];
        assert_eq!(repeat_applications(&rows), BTreeSet::from(["1".to_owned()]));
    }

    #[test]
    fn placeholder_phone_and_mail_never_join_different_people() {
        for phone in [
            "000-0000-0000",
            "111111111",
            "0999999999",
            "0123456789",
            "123-456-789",
        ] {
            let rows = [
                applicant("1", "2026-11-01", &[("denwabangou", phone)]),
                applicant("2", "2026-11-02", &[("denwabangou", phone)]),
            ];
            assert!(repeat_applications(&rows).is_empty(), "{phone}");
        }
        for mail in [
            "none@example.test",
            "test@kakuu.jp",
            "aaa@kakuu.jp",
            "taro@example.com",
            "x@kakuu.jp",
        ] {
            let rows = [
                applicant("1", "2026-11-01", &[("meeruadoresu", mail)]),
                applicant("2", "2026-11-02", &[("meeruadoresu", mail)]),
            ];
            assert!(repeat_applications(&rows).is_empty(), "{mail}");
        }
        // 本物らしい番号・アドレスは今までどおり同じ人
        let rows = [
            applicant(
                "1",
                "2026-11-01",
                &[
                    ("denwabangou", "090-1234-5678"),
                    ("meeruadoresu", "taro@kakuu.jp"),
                ],
            ),
            applicant("2", "2026-11-02", &[("denwabangou", "09012345678")]),
        ];
        assert_eq!(repeat_applications(&rows).len(), 1);
    }

    #[test]
    fn repeats_are_merged_into_the_first_one_without_overwriting_it() {
        let rows = vec![
            applicant(
                "2",
                "2026-11-05",
                &[
                    ("denwabangou", "090-1234-5678"),
                    ("seibetsu", "男性"),
                    ("ouboriyuu_hiaringu", "通勤が近い"),
                    ("nenrei", "40"),
                ],
            ),
            applicant(
                "1",
                "2026-11-02",
                &[("denwabangou", "09012345678"), ("nenrei", "35")],
            ),
            applicant("3", "2026-11-03", &[("denwabangou", "080-2222-3333")]),
        ];
        let groups = applicant_groups(&rows);
        assert_eq!(groups["2"], "1");
        assert_eq!(groups["1"], "1");
        assert_eq!(groups["3"], "3");
        let (kept, repeats) = merge_repeat_applications(rows);
        assert_eq!(
            kept.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["1", "3"]
        );
        let first = &kept[0];
        // 空の項目だけ 2 件目で埋め、入っている値（年齢 35）と応募日は変えない
        assert_eq!(first.value("seibetsu"), Some("男性"));
        assert_eq!(first.value("ouboriyuu_hiaringu"), Some("通勤が近い"));
        assert_eq!(first.value("nenrei"), Some("35"));
        assert_eq!(first.value("yingmuri"), Some("2026-11-02"));
        assert_eq!(
            repeats
                .iter()
                .map(|(r, to)| (r.id.as_str(), to.as_str()))
                .collect::<Vec<_>>(),
            [("2", "1")]
        );
    }

    #[test]
    fn same_person_keys_are_phone_mail_or_name_with_birthday() {
        let base = |id: &str, props: &[(&str, &str)]| applicant(id, "2026-11-01", props);
        // メール（大文字小文字・前後の空白の違いは同じ）
        let mail = [
            base("1", &[("meeruadoresu", "Kakuu@kakuu.jp")]),
            base("2", &[("meeruadoresu", "kakuu@kakuu.jp ")]),
        ];
        assert_eq!(repeat_applications(&mail).len(), 1);
        // 氏名と生年月日の組（全角の空白も同じ）
        let person = [
            base(
                "1",
                &[
                    ("ouboshashimei", "架空 次郎"),
                    ("seinengappi", "1990-04-01"),
                ],
            ),
            base(
                "2",
                &[
                    ("ouboshashimei", "架空\u{3000}次郎"),
                    ("seinengappi", "1990-04-01"),
                ],
            ),
        ];
        assert_eq!(repeat_applications(&person).len(), 1);
        // 同姓同名でも生年月日が違えば別の人。氏名だけ・生年月日だけ・短い番号では同じ人にしない
        let others = [
            base(
                "1",
                &[
                    ("ouboshashimei", "架空 次郎"),
                    ("seinengappi", "1990-04-01"),
                ],
            ),
            base(
                "2",
                &[
                    ("ouboshashimei", "架空 次郎"),
                    ("seinengappi", "1985-01-01"),
                ],
            ),
            base("3", &[("ouboshashimei", "架空 次郎")]),
            base("4", &[("seinengappi", "1990-04-01")]),
            base("5", &[("denwabangou", "1234")]),
            base("6", &[("denwabangou", "1234")]),
        ];
        assert!(repeat_applications(&others).is_empty());
        // つながりをたどる: 1 と 2 は電話、2 と 3 はメールで同じ人 → 2 と 3 を数えない
        let chain = [
            applicant("1", "2026-11-01", &[("denwabangou", "090-1111-2222")]),
            applicant(
                "2",
                "2026-11-02",
                &[
                    ("denwabangou", "09011112222"),
                    ("meeruadoresu", "aoi@kakuu.jp"),
                ],
            ),
            applicant("3", "2026-11-03", &[("meeruadoresu", "aoi@kakuu.jp")]),
        ];
        assert_eq!(
            repeat_applications(&chain),
            BTreeSet::from(["2".to_owned(), "3".to_owned()])
        );
    }

    #[tokio::test]
    async fn a_listing_of_another_deal_is_refused_not_counted() {
        let (service, server) = mock("99").await;
        let error = read_listing(&service, "10", "7").await.unwrap_err();
        assert_eq!(
            (error.0, error.1),
            (StatusCode::FORBIDDEN, "listing_not_in_deal")
        );
        server.abort();
    }
    use super::*;

    #[test]
    fn airwork_rows_are_read_with_their_own_id() {
        let aw = json!({"schema_version":1,"source":"airwork","job_id":"555","captured_at":"2026-10-05T00:00:00Z",
            "rows":[{"period_start":"2026-09-28","period_end":"2026-10-04","impressions":100,"clicks":5,"cost_yen":8000,"applications":2}]})
        .to_string();
        let rec = |id: &str, own: &str| -> Record {
            serde_json::from_value(
                json!({"id":id,"properties":{"id_airwork":own,"airwork_weekly_perf":aw}}),
            )
            .unwrap()
        };
        let map = perf_by_listing(&[rec("7", "555"), rec("8", "556")]);
        assert_eq!(map.len(), 1);
        assert_eq!(map["7"].0, "airwork_weekly_perf");
        assert_eq!(map["7"].1["rows"][0]["cost_yen"], 8000);
    }

    #[test]
    fn perf_is_taken_from_valid_json_and_bad_values_are_ignored() {
        let rec = |id: &str, v: Value| -> Record {
            serde_json::from_value(json!({"id":id,"properties":{"indeed_weekly_perf":v}})).unwrap()
        };
        let good = json!({"schema_version":1,"source":"hrhacker","job_id":"1234567","captured_at":"2026-10-05T00:00:00Z",
            "rows":[{"period_start":"2026-09-28","period_end":"2026-10-04","impressions":100,"clicks":5,"cost_yen":12345.5,"applications":2}]})
        .to_string();
        let records = vec![
            rec("1", json!(good)),
            rec("2", json!("{not json")),
            rec("3", json!(null)),
            rec(
                "4",
                json!(r#"{"schema_version":2,"source":"hrhacker","rows":[]}"#),
            ),
        ];
        let map = perf_by_listing(&records);
        assert_eq!(map.len(), 1);
        assert_eq!(map["1"].0, "indeed_weekly_perf");
        assert_eq!(map["1"].1["rows"][0]["cost_yen"], 12345.5);
        assert!(!map.contains_key("2") && !map.contains_key("3") && !map.contains_key("4"));
    }

    fn rec(id: &str, yingmuri: Option<&str>, created: Option<&str>) -> Record {
        Record {
            id: id.into(),
            properties: [("yingmuri", yingmuri), ("hs_createdate", created)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.map(str::to_owned)))
                .collect(),
        }
    }

    #[test]
    fn application_day_prefers_the_application_date_then_the_creation_day() {
        let day = |y, c| application_day_of(y, c);
        // 応募日あり: 作成日は見ない。
        assert_eq!(
            day(Some("2026-10-03"), Some("2026-10-05T00:00:00Z")),
            Some(("2026-10-03".into(), false))
        );
        // 応募日が空・空白・読めない: 作成日（JST の日付）。UTC 15:00 以降は翌日になる。
        for blank in [None, Some(""), Some("  "), Some("bad")] {
            assert_eq!(
                day(blank, Some("2026-10-05T14:59:59.999Z")),
                Some(("2026-10-05".into(), true))
            );
            assert_eq!(
                day(blank, Some("2026-10-05T15:00:00.000Z")),
                Some(("2026-10-06".into(), true))
            );
        }
        // epoch ミリ秒でも同じ（2026-10-05T15:00:00Z）。
        assert_eq!(
            day(None, Some("1791212400000")),
            Some(("2026-10-06".into(), true))
        );
        // 両方ない・作成日も読めない: 数えない。
        assert_eq!(day(None, None), None);
        assert_eq!(day(Some(" "), Some("")), None);
        assert_eq!(day(None, Some("not a time")), None);
    }

    #[test]
    fn days_of_counts_creation_fallbacks_inside_the_dated_count() {
        let rows = [
            rec("1", Some("2026-10-03"), Some("2026-10-05T00:00:00Z")),
            rec("2", None, Some("2026-10-04T01:00:00Z")),
            rec("3", Some(""), Some("2026-10-04T20:00:00Z")),
            rec("4", None, None),
        ];
        let (days, from_created) = days_of(&rows);
        assert_eq!(
            days,
            vec![
                Some("2026-10-03".into()),
                Some("2026-10-04".into()),
                Some("2026-10-05".into()),
                None
            ]
        );
        assert_eq!(from_created, 2);
        let (by_date, missing) = count_by_date(&days);
        assert_eq!(by_date.values().sum::<usize>() + missing, rows.len());
        assert_eq!(missing, 1);
        assert_eq!(by_date["2026-10-05"], 1);
    }

    #[test]
    fn counts_dated_and_undated_applications() {
        let days: Vec<Option<String>> = [
            Some("2026-10-01"),
            Some("2026-10-01"),
            Some("2026-10-03"),
            None,
            Some("bad"),
        ]
        .iter()
        .map(|v| v.and_then(jst_day))
        .collect();
        let (by_date, missing) = count_by_date(&days);
        assert_eq!(by_date.get("2026-10-01"), Some(&2));
        assert_eq!(by_date.get("2026-10-03"), Some(&1));
        assert_eq!(missing, 2);
    }

    #[test]
    fn timestamps_are_counted_on_their_japan_time_day() {
        // 2026-09-30 16:30 UTC is 2026-10-01 01:30 in Japan.
        assert_eq!(
            jst_day("2026-09-30T16:30:00Z").as_deref(),
            Some("2026-10-01")
        );
        assert_eq!(
            jst_day("2026-10-01T08:00:00+09:00").as_deref(),
            Some("2026-10-01")
        );
        assert_eq!(jst_day("2026-10-01").as_deref(), Some("2026-10-01"));
        assert_eq!(jst_day("not a date"), None);
    }

    #[test]
    fn shared_applications_are_subtracted_by_day() {
        let app = |id: &str, day: Option<&str>| (id.to_owned(), day.map(str::to_owned));
        let a = vec![
            app("1", Some("2026-10-01")),
            app("2", Some("2026-10-02")),
            app("3", None),
        ];
        let b = vec![app("3", None), app("4", Some("2026-10-01"))];
        let c = vec![app("2", Some("2026-10-02")), app("3", None)];
        let (shared, by_day, undated) = shared_overlap(&[a, b, c]);
        // 2 is in a and c (1 extra on 10-02); 3 is in a, b and c (2 extra, undated).
        assert_eq!(shared, 2);
        assert_eq!(by_day.get("2026-10-02"), Some(&1));
        assert_eq!(by_day.len(), 1);
        assert_eq!(undated, 2);
    }
}
