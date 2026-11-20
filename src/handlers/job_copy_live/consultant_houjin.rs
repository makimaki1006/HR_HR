//! 法人の集計はコンサルKPIを共用。会社名の類似判定はせず、共通の案件IDで結ぶ。
//!
//! 事業所と社名は HubSpot の会社で決める (2026-11-20 ユーザー決定)。
//! - 事業所 ＝ 取引に関連する会社。シートの拠点の鍵は取引名から切り出した値で、社名の書き方
//!   (法人格の有無・全角半角・旧社名) が違うと同じ事業所が分かれ、契約の系列で継続の番号が抜けて見えた。
//!   同じ会社に事業所が複数あっても 1 つにまとまる (了承済み)。
//! - 社名 ＝ 会社のいまの名前。シートの社名は取引名から作るので旧社名が残る。旧社名は検索用に `aliases` で返す。
//! - 会社は 5 分ごとの納品管理の読み取り (`Snapshot::companies`) を使う。HubSpot の呼び出しは増えない。
use super::owner_todo::{OwnerQuery, Snapshot};
use super::*;
use crate::handlers::cs_dashboard as cs;
use chrono::NaiveDate;
use std::collections::HashMap;

#[derive(Deserialize)]
pub(super) struct HoujinQuery {
    #[serde(flatten)]
    owner: OwnerQuery,
    houjin: Option<String>,
}

pub(super) fn customer(sheets: &cs::Sheets, houjin: Option<&str>, today: NaiveDate) -> Value {
    cs::routes::freshen(
        cs::routes::build_customer(sheets, houjin, today),
        sheets,
        today,
    )
}

type Company = (String, Option<String>);

/// 取引 → 会社。関連が無い取引は、同じ法人で取引名の書き方が同じ取引がすべて同じ 1 社に付いていればその会社にする。
/// それも無い取引は入れない (呼び出し側はシートの値のままにする)。
fn deal_companies<'a>(
    deals: &'a [cs::Deal],
    snapshot: &'a Snapshot,
) -> HashMap<&'a str, &'a Company> {
    let mut out: HashMap<&str, &Company> = deals
        .iter()
        .filter_map(|d| Some((d.id.as_str(), snapshot.companies.get(&d.id)?)))
        .collect();
    let mut by_name: HashMap<(&str, &str), Option<&Company>> = HashMap::new();
    for d in deals {
        let key = d.kyoten_key.trim();
        let Some(c) = out.get(d.id.as_str()).copied() else {
            continue;
        };
        if !key.is_empty() {
            by_name
                .entry((d.houjin_resolved.as_str(), key))
                .and_modify(|e| {
                    if e.is_some_and(|x| x.0 != c.0) {
                        *e = None;
                    }
                })
                .or_insert(Some(c));
        }
    }
    for d in deals {
        let key = (d.houjin_resolved.as_str(), d.kyoten_key.trim());
        if let (false, Some(Some(c))) = (out.contains_key(d.id.as_str()), by_name.get(&key)) {
            out.insert(d.id.as_str(), c);
        }
    }
    out
}

/// 法人 1 つぶんの、会社から決めた社名・事業所数・検索用の別名。
struct Named {
    /// 取引がいちばん多い会社のいまの名前 (同数なら契約開始が新しいほう)。会社が無ければ `None`
    name: Option<String>,
    sites: usize,
    /// シートの社名・取引名から切り出した名前・ほかの会社の名前。旧社名でも検索に当たるようにする
    aliases: Vec<String>,
}

fn named(deals: &[&cs::Deal], companies: &HashMap<&str, &Company>, sheet_name: &str) -> Named {
    let mut sites = BTreeSet::new();
    let mut count: BTreeMap<&str, (usize, &str, Option<&str>)> = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    for d in deals {
        let company = companies.get(d.id.as_str());
        let key = company.map_or(d.kyoten_key.trim(), |c| c.0.as_str());
        if !key.is_empty() {
            sites.insert(key);
        }
        aliases.insert(d.kyoten_name.trim());
        if let Some(c) = company {
            let name = c.1.as_deref().map(str::trim).filter(|n| !n.is_empty());
            aliases.extend(name);
            let e = count.entry(c.0.as_str()).or_insert((0, "", name));
            e.0 += 1;
            e.1 = e.1.max(d.contract_start_date.as_str());
        }
    }
    let name = count
        .values()
        .filter(|c| c.2.is_some())
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)))
        .and_then(|c| c.2);
    aliases.insert(sheet_name.trim());
    aliases.remove("");
    aliases.remove(name.unwrap_or(sheet_name.trim()));
    Named {
        name: name.map(str::to_string),
        sites: sites.len(),
        aliases: aliases.into_iter().map(str::to_string).collect(),
    }
}

/// 1 社の明細の事業所と社名を会社のものに置き換える。
fn apply_companies(body: &mut Value, companies: &HashMap<&str, &Company>, named: &Named) {
    if body["customer"].is_object() {
        if let Some(name) = &named.name {
            body["customer"]["name"] = json!(name);
        }
        body["customer"]["sites"] = json!(named.sites);
    }
    let mut site_of: HashMap<String, String> = HashMap::new();
    for d in body["deals"].as_array_mut().into_iter().flatten() {
        let id = d["deal_id"].as_str().unwrap_or_default().to_string();
        if let Some(c) = companies.get(id.as_str()) {
            d["site"] = json!(c.0);
            if let Some(name) = c.1.as_deref().filter(|n| !n.trim().is_empty()) {
                d["site_name"] = json!(name);
            }
        }
        site_of.insert(id, d["site"].as_str().unwrap_or_default().to_string());
    }
    // 事業所ごとの採用単価も同じ事業所でまとめ直す
    let mut by_site: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for s in body["cpa_by_site"].as_array_mut().into_iter().flatten() {
        let old = s["site"].as_str().unwrap_or_default().to_string();
        for p in s["points"].as_array_mut().into_iter().flatten() {
            let site = site_of
                .get(p["deal_id"].as_str().unwrap_or_default())
                .filter(|k| !k.is_empty())
                .unwrap_or(&old);
            by_site.entry(site.clone()).or_default().push(p.take());
        }
    }
    for points in by_site.values_mut() {
        points.sort_by(|a, b| {
            a["start"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["start"].as_str().unwrap_or_default())
        });
    }
    body["cpa_by_site"] = by_site
        .into_iter()
        .map(|(site, points)| json!({"site": site, "points": points}))
        .collect();
}

fn report(
    sheets: &cs::Sheets,
    snapshot: &Snapshot,
    query: &HoujinQuery,
    email: &str,
    today: NaiveDate,
) -> Value {
    let (owner, _) = snapshot.selection(&query.owner, email);
    let deals = cs::deals_all_of(&sheets.deal);
    let eligible: BTreeSet<_> = deals
        .iter()
        .filter(|d| snapshot.matches(&d.id, query.owner.field(), owner.as_deref()))
        .map(|d| d.houjin_resolved.as_str())
        .collect();
    let mut index = customer(sheets, None, today);
    let rows = index["index"].as_array_mut().expect("customer index");
    if owner.is_some() {
        rows.retain(|r| r["houjin"].as_str().is_some_and(|h| eligible.contains(h)));
    }
    let companies = deal_companies(&deals, snapshot);
    let mut by_houjin: HashMap<&str, Vec<&cs::Deal>> = HashMap::new();
    for d in &deals {
        by_houjin
            .entry(d.houjin_resolved.as_str())
            .or_default()
            .push(d);
    }
    let mut names: HashMap<String, Named> = HashMap::new();
    for r in rows.iter_mut() {
        let h = r["houjin"].as_str().unwrap_or_default().to_string();
        let own = by_houjin.get(h.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let n = named(own, &companies, r["name"].as_str().unwrap_or_default());
        if let Some(name) = &n.name {
            r["name"] = json!(name);
        }
        if !own.is_empty() {
            r["sites"] = json!(n.sites);
        }
        r["aliases"] = json!(n.aliases);
        names.insert(h, n);
    }
    let links: BTreeMap<String, BTreeMap<String, Option<String>>> = rows
        .iter()
        .map(|r| {
            let h = r["houjin"].as_str().unwrap_or_default();
            let companies = deals
                .iter()
                .filter(|d| d.houjin_resolved == h)
                .filter_map(|d| snapshot.companies.get(&d.id))
                .cloned()
                .collect();
            (h.to_string(), companies)
        })
        .collect();
    let unmatched = links.values().filter(|v| v.is_empty()).count();
    let default = rows
        .iter()
        .max_by(|a, b| {
            a["deals"]
                .as_f64()
                .unwrap_or_default()
                .total_cmp(&b["deals"].as_f64().unwrap_or_default())
        })
        .and_then(|r| r["houjin"].as_str())
        .map(str::to_string);
    let selected = query
        .houjin
        .as_deref()
        .filter(|h| rows.iter().any(|r| r["houjin"] == *h))
        .or(default.as_deref());
    let detail = selected.map(|h| {
        let mut body = customer(sheets, Some(h), today);
        if let Some(n) = names.get(h) {
            apply_companies(&mut body, &companies, n);
        }
        body
    });
    json!({"status":"ready", "index":index["index"], "default_houjin":default,
        "selected_houjin":selected, "customer":detail,
        "companies":selected.and_then(|h| links.get(h)).map(|m| m.iter().map(|(id,name)| json!({"id":id,"name":name})).collect::<Vec<_>>()).unwrap_or_default(),
        "linkage":{"total":links.len(),"unmatched":unmatched}, "scope":snapshot.scope(&query.owner,email)})
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<HoujinQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.owner.validate()?;
    let view = owner_todo::work_view(listings::service(&access)?).await;
    let body = if let Some(snapshot) = view.snapshot.filter(|s| s.complete) {
        let cq = crate::handlers::call_quality::routes::cq_state()
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        // コンサルKPI と同じシートを読むので「見ている」に数える (古ければ取り直す。画面は 120 秒待つので上限付きで待つ)
        cs::note_view(Some(cs::VIEW_WAIT)).await;
        let sheets = cs::load(&cq.client, &cq.store)
            .await
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        tokio::task::spawn_blocking(move || {
            report(
                &sheets,
                &snapshot,
                &query,
                &email,
                owner_todo::today(chrono::Utc::now()),
            )
        })
        .await
        .map_err(|_| ReadError(StatusCode::INTERNAL_SERVER_ERROR, "consulting_unavailable"))?
    } else {
        json!({"status":if view.failed {"failed"} else {"preparing"}})
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::super::owner_todo::{Assignment, WorkDeal};
    use super::*;
    use crate::handlers::call_quality::sheets::SheetData;
    fn sheet(header: &[&str], rows: &[&[&str]]) -> Arc<SheetData> {
        Arc::new(SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|v| Arc::from(*v)).collect())
                .collect(),
            fetched_at: Instant::now(),
        })
    }
    fn fixture() -> cs::Sheets {
        let empty = sheet(&[], &[]);
        cs::Sheets {
            pending_recordings: None,
            mail_history: cs::empty_history_sheet(),
            zoom_mtg: cs::empty_history_sheet(),
            all_cached: true,
            deal: sheet(
                &[
                    "deal_id",
                    "dealname",
                    "houjin_resolved",
                    "is_active",
                    "contract_kind",
                    "amount",
                    "syoudaku",
                    "kyoten_key",
                    "contract_start_date",
                ],
                &[
                    &[
                        "101",
                        "架空の案件A",
                        "法人A",
                        "true",
                        "本体契約",
                        "600000",
                        "3",
                        "拠点A",
                        "2026-08-01",
                    ],
                    &[
                        "102",
                        "架空の案件B",
                        "法人B",
                        "true",
                        "本体契約",
                        "200000",
                        "1",
                        "拠点B",
                        "2026-09-01",
                    ],
                    &[
                        "103",
                        "架空の案件C",
                        "法人C",
                        "true",
                        "本体契約",
                        "100000",
                        "0",
                        "拠点C",
                        "2026-09-01",
                    ],
                ],
            ),
            customer: sheet(
                &[
                    "houjin",
                    "name",
                    "is_display_target",
                    "deal_count",
                    "ltv",
                    "kyoten_unique",
                ],
                &[
                    &["法人A", "架空の法人A", "true", "1", "600000", "1"],
                    &["法人B", "架空の法人B", "true", "1", "200000", "1"],
                    &["法人C", "架空の法人C", "true", "1", "100000", "1"],
                ],
            ),
            call: empty.clone(),
            mtg: empty.clone(),
            history: empty.clone(),
            mail_mtg: empty.clone(),
            handover: empty.clone(),
            owner_hist: empty.clone(),
            meta: empty,
        }
    }
    fn snapshot() -> Snapshot {
        let mut snapshot = Snapshot::for_index(
            [("101", "11"), ("102", "22"), ("103", "11")]
                .into_iter()
                .map(|(id, owner)| {
                    (
                        id.to_string(),
                        WorkDeal {
                            record: Record {
                                id: id.into(),
                                properties: BTreeMap::new(),
                            },
                            assignment: Assignment {
                                consultant: Some(owner.into()),
                                ..Default::default()
                            },
                        },
                    )
                })
                .collect(),
            vec![],
        );
        // 会社名はシートと異なっていても案件IDの関連で結び付く。
        snapshot
            .companies
            .insert("101".into(), ("501".into(), Some("架空の別表記A".into())));
        snapshot
            .companies
            .insert("102".into(), ("502".into(), Some("架空の別表記B".into())));
        snapshot
    }
    #[test]
    fn same_customer_payload_as_consulting_and_owner_scoped_index() {
        let sheets = fixture();
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let snapshot = snapshot();
        for h in [None, Some("法人A"), Some("法人B")] {
            assert_eq!(
                customer(&sheets, h, today),
                cs::routes::freshen(
                    cs::routes::build_customer(&sheets, h, today),
                    &sheets,
                    today
                )
            );
        }
        let q = |owner: &str, houjin: Option<&str>| HoujinQuery {
            owner: OwnerQuery {
                owner: Some(owner.into()),
                owner_field: None,
            },
            houjin: houjin.map(str::to_string),
        };
        let all = report(&sheets, &snapshot, &q("all", Some("法人A")), "", today);
        assert_eq!(all["index"].as_array().unwrap().len(), 3);
        assert_eq!(all["linkage"], json!({"total":3,"unmatched":1}));
        // 事業所と社名は会社で決める (取引名から切り出したシートの値ではない)
        assert_eq!(all["customer"]["deals"][0]["site"], "501");
        assert_eq!(all["customer"]["deals"][0]["site_name"], "架空の別表記A");
        assert_eq!(all["customer"]["cpa_by_site"][0]["site"], "501");
        assert_eq!(all["index"][0]["name"], "架空の別表記A");
        assert_eq!(all["customer"]["deals"][0]["amount"], 600000.0);
        assert_eq!(all["companies"][0]["id"], "501");
        let scoped = report(&sheets, &snapshot, &q("22", Some("法人A")), "", today);
        assert_eq!(scoped["index"].as_array().unwrap().len(), 1);
        assert_eq!(scoped["selected_houjin"], "法人B");
        assert_eq!(scoped["customer"]["meta"]["houjin"], "法人B");
        let none = report(&sheets, &snapshot, &q("33", None), "", today);
        assert!(none["customer"].is_null());
    }
    #[test]
    fn option_only_customer_still_matches_owner_and_company() {
        let mut sheets = fixture();
        sheets.deal = sheet(
            &["deal_id", "dealname", "houjin_resolved", "contract_kind"],
            &[&["101", "求人追加_架空の案件", "法人A", "求人追加"]],
        );
        let query = HoujinQuery {
            owner: OwnerQuery {
                owner: Some("11".into()),
                owner_field: None,
            },
            houjin: Some("法人A".into()),
        };
        let result = report(
            &sheets,
            &snapshot(),
            &query,
            "",
            NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
        );
        assert_eq!(result["index"].as_array().unwrap().len(), 1);
        assert_eq!(result["companies"][0]["id"], "501");
        assert!(result["customer"]["deals"].as_array().unwrap().is_empty());
    }
}
