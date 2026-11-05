//! 未連携のMTG。夜の更新が取引に結び付けられなかった録画の一覧（シート「CS_MTG結び付け待ち」）を、
//! コンサル本人が取引へ結び付けるための読み取り。結び付けの書き込みは CRM の取引の PATCH
//! （`mtg_manual_links` に 1 行足す）で行い、ここでは書かない。
use super::owner_todo::{OwnerQuery, Snapshot};
use super::*;
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::cs_dashboard as cs;
use unicode_normalization::UnicodeNormalization;

const SELECTED_TTL: Duration = Duration::from_secs(60);
/// 担当者 1 人あたり、選択欄に出す取引の上限。
const OWN_DEALS_MAX: usize = 300;
const SEARCH_PAGES_MAX: usize = 10;

#[derive(Deserialize)]
pub(super) struct UnlinkedQuery {
    #[serde(flatten)]
    owner: OwnerQuery,
    /// 1 なら選択済みの印のキャッシュを使わず読み直す（画面の保存・取り消しのあと）。
    fresh: Option<u8>,
}

/// 担当者名とホスト氏名の突き合わせ用。NFKC にして空白をすべて除く。
pub(super) fn norm_name(s: &str) -> String {
    s.nfkc().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Candidates {
    pub(super) deals: Vec<(String, String)>,
    pub(super) others: usize,
}
/// 「deal_id: 取引名 / deal_id: 取引名 / ほかN件」を分解する。
pub(super) fn parse_candidates(text: &str) -> Candidates {
    let mut deals: Vec<(String, String)> = Vec::new();
    let mut others = 0;
    for seg in text.split(" / ") {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if let Some((id, name)) = seg.split_once(':') {
            let id = id.trim();
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
                deals.push((id.to_string(), name.trim().to_string()));
                continue;
            }
        }
        if let Some(n) = seg
            .strip_prefix("ほか")
            .and_then(|r| r.strip_suffix('件'))
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            others = n;
            continue;
        }
        // 取引名の中に " / " があった場合は前の取引名の続きとして扱う。
        if let Some(last) = deals.last_mut() {
            last.1.push_str(" / ");
            last.1.push_str(seg);
        }
    }
    Candidates { deals, others }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Selected {
    pub(super) deal_id: String,
    pub(super) deal_name: String,
    pub(super) at: String,
    pub(super) by: String,
}
/// 取引の `mtg_manual_links`（1 行 = 録画UUID<TAB>日時<TAB>メール）を録画UUID ごとの選択に直す。
/// 同じ UUID が複数の取引にあれば、先に出たものを採る。
pub(super) fn parse_links(deals: &[(String, String, String)]) -> BTreeMap<String, Selected> {
    let mut out = BTreeMap::new();
    for (id, name, links) in deals {
        for line in links.lines() {
            let mut p = line.split('\t');
            let uuid = p.next().unwrap_or("").trim();
            if uuid.is_empty() {
                continue;
            }
            out.entry(uuid.to_string()).or_insert_with(|| Selected {
                deal_id: id.clone(),
                deal_name: name.clone(),
                at: p.next().unwrap_or("").trim().to_string(),
                by: p.next().unwrap_or("").trim().to_string(),
            });
        }
    }
    out
}

pub(super) struct Row {
    pub(super) uuid: String,
    pub(super) recording: String,
    pub(super) date: String,
    pub(super) subject: String,
    pub(super) host: String,
    pub(super) candidates: Candidates,
}
pub(super) fn parse_rows(sheet: &SheetData) -> Option<Vec<Row>> {
    if sheet.header != cs::routes::PENDING_RECORDING_HEADER {
        return None;
    }
    let mut rows: Vec<Row> = sheet
        .rows
        .iter()
        .filter(|r| !sheet.get(r, "録画UUID").trim().is_empty())
        .map(|r| Row {
            uuid: sheet.get(r, "録画UUID").trim().to_string(),
            recording: sheet.get(r, "録画").trim().to_string(),
            date: sheet.get(r, "開催日").trim().to_string(),
            subject: sheet.get(r, "件名").trim().to_string(),
            host: sheet.get(r, "ホスト氏名").trim().to_string(),
            candidates: parse_candidates(sheet.get(r, "候補の取引")),
        })
        .collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date));
    Some(rows)
}

/// 担当者の選択に合う行だけ残す。`owner_name` が None（全員）なら全部。
pub(super) fn filter_rows(rows: Vec<Row>, owner_name: Option<&str>) -> Vec<Row> {
    match owner_name {
        None => rows,
        Some(name) => {
            let want = norm_name(name);
            rows.into_iter()
                .filter(|r| !want.is_empty() && norm_name(&r.host) == want)
                .collect()
        }
    }
}

type SelectedCache = tokio::sync::Mutex<Option<(Instant, Arc<BTreeMap<String, Selected>>)>>;
static SELECTED: std::sync::OnceLock<SelectedCache> = std::sync::OnceLock::new();

/// `mtg_manual_links` を持つ取引を Search（HAS_PROPERTY）で読み、録画UUID ごとの選択にする。60 秒キャッシュ。
async fn selected_links(
    service: &JobReadService,
    fresh: bool,
) -> Result<Arc<BTreeMap<String, Selected>>, ReadError> {
    let cache = SELECTED.get_or_init(Default::default);
    let mut guard = cache.lock().await;
    if !fresh {
        if let Some((at, v)) = guard.as_ref().filter(|(at, _)| at.elapsed() < SELECTED_TTL) {
            let _ = at;
            return Ok(v.clone());
        }
    }
    let mut deals = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..SEARCH_PAGES_MAX {
        let mut body = json!({"filterGroups":[{"filters":[{"propertyName":"mtg_manual_links","operator":"HAS_PROPERTY"}]}],
            "properties":["dealname","mtg_manual_links"],"limit":200});
        if let Some(a) = &after {
            body["after"] = json!(a);
        }
        let data = service
            .hs
            .search("deals", body)
            .await
            .map_err(client_error)?;
        for r in data["results"].as_array().into_iter().flatten() {
            deals.push((
                r["id"].as_str().unwrap_or_default().to_string(),
                r["properties"]["dealname"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                r["properties"]["mtg_manual_links"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
        after = data["paging"]["next"]["after"].as_str().map(str::to_string);
        if after.is_none() {
            break;
        }
    }
    let map = Arc::new(parse_links(&deals));
    *guard = Some((Instant::now(), map.clone()));
    Ok(map)
}

/// 行のホスト氏名に合う担当者の取引（終了していないもの）。選択欄の候補が無い・合わない行のために出す。
fn own_deals(
    snapshot: &Snapshot,
    field: &str,
    rows: &[Row],
) -> (BTreeMap<String, Value>, BTreeMap<String, String>) {
    let mut by_owner: BTreeMap<String, Value> = BTreeMap::new();
    let mut host_owner: BTreeMap<String, String> = BTreeMap::new();
    for host in rows
        .iter()
        .map(|r| r.host.as_str())
        .collect::<BTreeSet<_>>()
    {
        let want = norm_name(host);
        let Some(owner) = snapshot
            .owners
            .iter()
            .find(|o| !want.is_empty() && norm_name(&o.name) == want)
        else {
            continue;
        };
        host_owner.insert(host.to_string(), owner.id.clone());
        if by_owner.contains_key(&owner.id) {
            continue;
        }
        let mut list: Vec<(String, String)> = snapshot
            .deals
            .iter()
            .filter(|(id, d)| {
                snapshot.matches(id, field, Some(&owner.id))
                    && !d
                        .record
                        .value("dealstage")
                        .is_some_and(|s| owner_todo::ENDED_STAGES.contains(&s))
            })
            .map(|(id, d)| {
                (
                    id.clone(),
                    d.record
                        .value("dealname")
                        .unwrap_or("（名称なし）")
                        .to_string(),
                )
            })
            .collect();
        list.sort_by(|a, b| a.1.cmp(&b.1));
        list.truncate(OWN_DEALS_MAX);
        by_owner.insert(
            owner.id.clone(),
            json!(list
                .into_iter()
                .map(|(id, name)| json!({"id":id,"name":name}))
                .collect::<Vec<_>>()),
        );
    }
    (by_owner, host_owner)
}

pub(super) fn build(
    sheet: &SheetData,
    snapshot: &Snapshot,
    query: &OwnerQuery,
    email: &str,
    selected: &BTreeMap<String, Selected>,
    selected_ok: bool,
) -> Value {
    let Some(rows) = parse_rows(sheet) else {
        return json!({"status":"unavailable"});
    };
    let (owner, _) = snapshot.selection(query, email);
    let owner_name = owner.as_deref().map(|id| {
        snapshot
            .owners
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.name.as_str())
            .unwrap_or("")
    });
    let rows = filter_rows(rows, owner_name);
    let (own, host_owner) = own_deals(snapshot, query.field(), &rows);
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({"uuid":r.uuid,"recording":r.recording,"date":r.date,"subject":r.subject,"host":r.host,
                "host_owner_id":host_owner.get(&r.host),
                "candidates":r.candidates.deals.iter().map(|(id,name)| json!({"id":id,"name":name})).collect::<Vec<_>>(),
                "other_candidates":r.candidates.others,
                "selected":selected.get(&r.uuid).map(|s| json!({"deal_id":s.deal_id,"deal_name":s.deal_name,"at":s.at,"by":s.by}))})
        })
        .collect();
    json!({"status":"ready","me_email":email,"selected_known":selected_ok,"rows":out,"own_deals":own})
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<UnlinkedQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.owner.validate()?;
    let service = listings::service(&access)?;
    let view = owner_todo::work_view(service).await;
    let Some(snapshot) = view.snapshot.filter(|s| s.complete) else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"status":if view.failed {"failed"} else {"preparing"}})),
        ));
    };
    let cq = crate::handlers::call_quality::routes::cq_state()
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
    let sheets = cs::load(&cq.client, &cq.store)
        .await
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
    let Some(sheet) = sheets.pending_recordings.clone() else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"status":"unavailable"})),
        ));
    };
    // 選択済みの印が読めなくても一覧は出す（印だけ付かない）。
    let (selected, ok) = match selected_links(service, query.fresh == Some(1)).await {
        Ok(m) => (m, true),
        Err(_) => (Arc::new(BTreeMap::new()), false),
    };
    let body = build(&sheet, &snapshot, &query.owner, &email, &selected, ok);
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sheet(rows: &[[&str; 7]]) -> SheetData {
        SheetData {
            header: cs::routes::PENDING_RECORDING_HEADER
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| Arc::from(*s)).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }
    #[test]
    fn 候補の取引を分解する() {
        let c = parse_candidates(
            "101: 架空の案件A / 102: 架空の案件B / 103: 名前に / を含む / ほか3件",
        );
        assert_eq!(c.deals[0], ("101".into(), "架空の案件A".into()));
        assert_eq!(c.deals[1], ("102".into(), "架空の案件B".into()));
        assert_eq!(c.deals.len(), 3);
        assert_eq!(c.deals[2].1, "名前に / を含む");
        assert_eq!(c.others, 3);
        assert_eq!(
            parse_candidates(""),
            Candidates {
                deals: vec![],
                others: 0
            }
        );
    }
    #[test]
    fn 氏名の突き合わせは全角半角と空白を無視する() {
        assert_eq!(norm_name("山田　太郎"), norm_name("山田太郎"));
        assert_eq!(norm_name("ﾔﾏﾀﾞ 太郎"), norm_name("ヤマダ太郎"));
        assert_ne!(norm_name("山田太郎"), norm_name("山田次郎"));
    }
    #[test]
    fn 行の分解と担当者での絞り込み() {
        let s = sheet(&[
            [
                "u1",
                "https://example.test/r1",
                "2026-10-01",
                "架空MTG1",
                "山田 太郎",
                "101: 架空A",
                "x",
            ],
            [
                "u2",
                "https://example.test/r2",
                "2026-10-05",
                "架空MTG2",
                "鈴木花子",
                "",
                "x",
            ],
            ["", "", "", "", "", "", ""],
        ]);
        let rows = parse_rows(&s).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].uuid, "u2"); // 新しい順
        let only = filter_rows(parse_rows(&s).unwrap(), Some("山田太郎"));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].uuid, "u1");
        assert_eq!(filter_rows(parse_rows(&s).unwrap(), None).len(), 2);
        assert!(filter_rows(parse_rows(&s).unwrap(), Some("")).is_empty());
    }
    #[test]
    fn 見出しが違うシートは使わない() {
        let mut s = sheet(&[]);
        s.header[0] = "違う".into();
        assert!(parse_rows(&s).is_none());
    }
    #[test]
    fn 選択済みの印は録画uuidごとに付く() {
        let m = parse_links(&[
            (
                "101".into(),
                "架空A".into(),
                "u1\t2026-10-02 10:00\ta@example.test\nu2\t2026-10-03 09:00\tb@example.test".into(),
            ),
            (
                "102".into(),
                "架空B".into(),
                "u1\t2026-10-04 10:00\tc@example.test".into(),
            ),
        ]);
        assert_eq!(m["u1"].deal_id, "101");
        assert_eq!(m["u2"].by, "b@example.test");
        assert_eq!(m["u2"].at, "2026-10-03 09:00");
        assert_eq!(m.len(), 2);
    }
}
