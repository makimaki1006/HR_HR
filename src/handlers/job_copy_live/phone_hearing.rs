//! Read-only, masked phone hearing records. Raw applicant/note IDs never leave this module.
use super::*;
use applicant_reasons::{
    applicant_key, mask_personal_details, OptionLabels, MAX_ITEMS, MAX_TEXT_CHARS,
};
use axum::extract::Path as AxumPath;
use ts_rs::TS;

const FIELDS: &[(&[&str], &str)] = &[
    (&["hoyuushikaku_hiaringu"], "保有資格"),
    (&["kanrengyoumukeiken"], "関連業務経験"),
    (
        &["shuugyoujoukyou", "ichijitaiou_shuugyou_joukyou"],
        "就業状況",
    ),
    (&["genshokumaeshokukaranotenshokuriyuu"], "転職理由"),
    (&["oubodouki"], "応募動機"),
    (&["kinmujoukennoyuusenjuni"], "勤務条件の優先順位"),
    (&["oubonoheiganjoukyou"], "併願状況"),
    (&["kyuujinkensakujouken"], "求人の探し方（検索条件）"),
    (
        &["shigotosagashinohanikoutsuushudan"],
        "仕事探しの範囲・交通手段",
    ),
    (
        &["nyuushakiboujiki", "ichijitaiou_nyuusha_kibou_jiki"],
        "入社希望時期",
    ),
    (&["ichijitaiou_azukatta_shitsumon"], "預かった質問"),
    (&["ichijitaiou_jitai_riyuu"], "辞退理由"),
    (&["ichijitaiou_shokan_memo"], "所感メモ"),
    (&["tokukijikou"], "特記事項"),
    (&["bikou_hiaringu"], "備考"),
];
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct PhoneHearing {
    pub total_applicants: usize,
    pub items: Vec<PhoneHearingCard>,
    pub truncated: bool,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct PhoneHearingCard {
    pub applicant: String,
    pub application_date: Option<String>,
    pub media: Option<String>,
    pub multi_listing: bool,
    pub fields: Vec<PhoneHearingField>,
    pub notes: Vec<PhoneHearingNote>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct PhoneHearingField {
    pub label: String,
    pub text: String,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct PhoneHearingNote {
    pub time: Option<String>,
    pub text: String,
}
fn plain(raw: &str) -> Result<String, ReadError> {
    use lol_html::{element, text, RewriteStrSettings};
    use std::cell::RefCell;
    let cleaned = lol_html::rewrite_str(
        raw,
        RewriteStrSettings {
            element_content_handlers: vec![element!("script, style", |el| {
                el.remove();
                Ok(())
            })],
            ..RewriteStrSettings::default()
        },
    )
    .map_err(|_| fail("hearing_note_invalid"))?;
    let output = RefCell::new(String::new());
    lol_html::rewrite_str(
        &cleaned,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("br, p, div, li", |_el| {
                    output.borrow_mut().push('\n');
                    Ok(())
                }),
                text!("*", |chunk| {
                    output.borrow_mut().push_str(chunk.as_str());
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::default()
        },
    )
    .map_err(|_| fail("hearing_note_invalid"))?;
    let decoded = html_escape::decode_html_entities(&output.into_inner()).into_owned();
    Ok(decoded.trim().to_owned())
}
fn bounded(raw: &str, truncated: &mut bool) -> String {
    let masked = mask_personal_details(raw.trim());
    if masked.chars().count() > MAX_TEXT_CHARS {
        *truncated = true;
    }
    masked.chars().take(MAX_TEXT_CHARS).collect()
}
fn display(row: &Record, property: &str, labels: &OptionLabels) -> Option<String> {
    let raw = row.value(property)?.trim();
    Some(match labels.get(property) {
        Some(options) => raw
            .split(';')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| {
                options
                    .get(v)
                    .map(String::as_str)
                    .unwrap_or("選択内容を確認できません")
            })
            .collect::<Vec<_>>()
            .join("・"),
        _ => raw.to_owned(),
    })
}
#[allow(clippy::too_many_arguments)] // Pure assembly over separately batched, read-only inputs.
fn extract(
    listing: &str,
    rows: &[Record],
    notes: &[Record],
    note_links: &BTreeMap<String, Vec<String>>,
    multi: &BTreeSet<String>,
    labels: &OptionLabels,
    media_property: Option<&str>,
    total: usize,
) -> Result<PhoneHearing, ReadError> {
    let mut result = PhoneHearing {
        total_applicants: total,
        items: vec![],
        truncated: total > MAX_ITEMS,
    };
    let notes: BTreeMap<_, _> = notes.iter().map(|note| (note.id.as_str(), note)).collect();
    for row in rows.iter().take(MAX_ITEMS) {
        let mut fields = vec![];
        for (properties, label) in FIELDS {
            let mut values = vec![];
            for property in *properties {
                let Some(value) = display(row, property, labels) else {
                    continue;
                };
                if *property == "bikou_hiaringu" && value.trim_start().starts_with("求人未特定:")
                {
                    continue;
                }
                let value = bounded(&value, &mut result.truncated);
                if !value.is_empty() && !values.contains(&value) {
                    values.push(value);
                }
            }
            if !values.is_empty() {
                fields.push(PhoneHearingField {
                    label: (*label).into(),
                    text: bounded(&values.join("\n"), &mut result.truncated),
                });
            }
        }
        let mut kept_notes = vec![];
        for id in note_links.get(&row.id).into_iter().flatten() {
            let note = notes
                .get(id.as_str())
                .ok_or_else(|| fail("hearing_note_missing"))?;
            let text = plain(note.value("hs_note_body").unwrap_or(""))?;
            if text.is_empty()
                || text.contains("応募者対応メモテンプレート")
                || text.contains("【求人由来の暗黙知（自動コピー）")
            {
                continue;
            }
            kept_notes.push(PhoneHearingNote {
                time: note.value("hs_timestamp").and_then(timestamp),
                text: bounded(&text, &mut result.truncated),
            });
        }
        kept_notes.sort_by(|a, b| a.time.cmp(&b.time));
        if fields.is_empty() && kept_notes.is_empty() {
            continue;
        }
        result.items.push(PhoneHearingCard {
            applicant: applicant_key(listing, &row.id),
            application_date: row.value("yingmuri").and_then(super::deal_summary::jst_day),
            media: media_property
                .and_then(|p| display(row, p, labels))
                .map(|s| bounded(&s, &mut result.truncated)),
            multi_listing: multi.contains(&row.id),
            fields,
            notes: kept_notes,
        });
    }
    result.items.sort_by(|a, b| {
        b.application_date
            .cmp(&a.application_date)
            .then(a.applicant.cmp(&b.applicant))
    });
    Ok(result)
}
fn timestamp(raw: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.to_rfc3339())
        .or_else(|| {
            raw.parse::<i64>()
                .ok()
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|t| t.to_rfc3339())
        })
}
impl JobReadService {
    // Do not fall back to a GET per application. An incomplete association batch is a
    // failed read, never an empty hearing. This also prevents N+1 calls for empty lists.
    async fn hearing_links(
        &self,
        ids: &[String],
        to: &str,
    ) -> Result<BTreeMap<String, Vec<String>>, ReadError> {
        let mut output = BTreeMap::new();
        for chunk in ids.chunks(100) {
            let data = self.request(&format!("/crm/v4/associations/0-421/{to}/batch/read"), &[],
                Some(json!({"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}))).await?;
            if data["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty())
            {
                return Err(fail("hubspot_partial_read"));
            }
            let results = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            let expected: BTreeSet<_> = chunk.iter().map(String::as_str).collect();
            let mut seen = BTreeSet::new();
            for result in results {
                let id = result
                    .pointer("/from/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("hubspot_invalid_response"))?;
                if !expected.contains(id) || !seen.insert(id) || result.get("paging").is_some() {
                    return Err(fail("hubspot_partial_read"));
                }
                let mut targets = BTreeSet::new();
                for target in result["to"]
                    .as_array()
                    .ok_or_else(|| fail("hubspot_invalid_response"))?
                {
                    let id = target["toObjectId"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| target["toObjectId"].as_u64().map(|v| v.to_string()))
                        .ok_or_else(|| fail("hubspot_invalid_response"))?;
                    valid_id(&id).map_err(|_| fail("hubspot_invalid_response"))?;
                    targets.insert(id);
                }
                output.insert(id.to_owned(), targets.into_iter().collect());
            }
            if seen != expected {
                return Err(fail("hubspot_partial_read"));
            }
        }
        Ok(output)
    }
    async fn phone_hearing(&self, listing: &str) -> Result<PhoneHearing, ReadError> {
        let ids = self.all_associations("0-420", listing, "0-421").await?;
        if ids.is_empty() {
            return Ok(PhoneHearing {
                total_applicants: 0,
                items: vec![],
                truncated: false,
            });
        }
        // Resolve media by its definition label, rather than guessing its internal name.
        let definitions = self.request("/crm/v3/properties/0-421", &[], None).await?;
        let definitions = definitions["results"]
            .as_array()
            .ok_or_else(|| fail("hearing_definitions_invalid"))?;
        let media = definitions
            .iter()
            .find(|p| p["label"] == "応募媒体")
            .and_then(|p| p["name"].as_str());
        let mut labels = OptionLabels::new();
        for p in definitions {
            if let Some(name) = p["name"].as_str() {
                if let Some(options) = p["options"]
                    .as_array()
                    .filter(|v| p["type"] == "enumeration" || !v.is_empty())
                {
                    labels.insert(
                        name.to_owned(),
                        options
                            .iter()
                            .filter_map(|o| {
                                Some((
                                    o["value"].as_str()?.to_owned(),
                                    o["label"].as_str()?.to_owned(),
                                ))
                            })
                            .collect(),
                    );
                }
            }
        }
        let mut properties: Vec<&str> =
            FIELDS.iter().flat_map(|(p, _)| p.iter().copied()).collect();
        properties.push("yingmuri");
        if let Some(media) = media {
            properties.push(media);
        }
        // Only the latest 500 are read. Date-only batches let us choose them without reading
        // unlimited free text; the association count remains the denominator.
        let selected: Vec<_> = if ids.len() > MAX_ITEMS {
            let mut dated = self.batch("0-421", &ids, &["yingmuri"]).await?;
            dated.sort_by(|a, b| {
                b.value("yingmuri")
                    .and_then(super::deal_summary::jst_day)
                    .cmp(&a.value("yingmuri").and_then(super::deal_summary::jst_day))
                    .then(a.id.cmp(&b.id))
            });
            dated.into_iter().take(MAX_ITEMS).map(|r| r.id).collect()
        } else {
            ids.clone()
        };
        let rows = self.batch("0-421", &selected, &properties).await?;
        let note_links = self.hearing_links(&selected, "notes").await?;
        let note_ids: Vec<_> = note_links
            .values()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let notes = self
            .batch("notes", &note_ids, &["hs_note_body", "hs_timestamp"])
            .await?;
        let listing_links = self.hearing_links(&selected, "0-420").await?;
        let multi = listing_links
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(id, _)| id)
            .collect();
        extract(
            listing,
            &rows,
            &notes,
            &note_links,
            &multi,
            &labels,
            media,
            ids.len(),
        )
    }
}
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let result = listings::service(&access)?.phone_hearing(&id).await?;
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(id: &str, values: Value) -> Record {
        serde_json::from_value(json!({"id": id, "properties": values})).unwrap()
    }
    #[test]
    fn filters_automatic_content_labels_choices_and_omits_empty_fields() {
        let rows = vec![record(
            "101",
            json!({"yingmuri":"2026-10-03", "bikou_hiaringu":"求人未特定: 自動取込", "ichijitaiou_shuugyou_joukyou":"working", "oubodouki":"研修があるため", "hoyuushikaku_hiaringu":null, "tokukijikou":"  ", "media":"search"}),
        )];
        let notes = vec![
            record(
                "201",
                json!({"hs_note_body":"<p>応募者対応メモテンプレート</p>","hs_timestamp":"2026-10-03T01:00:00Z"}),
            ),
            record(
                "202",
                json!({"hs_note_body":"<div>【求人由来の暗黙知（自動コピー）】研修</div>"}),
            ),
            record(
                "203",
                json!({"hs_note_body":"<p>【架電】休日を優先<br>研修 &amp; 見学を希望</p>","hs_timestamp":"2026-10-03T03:00:00Z"}),
            ),
            record(
                "204",
                json!({"hs_note_body":"<p>【折り返し】来月から希望</p>","hs_timestamp":"2026-10-03T02:00:00Z"}),
            ),
        ];
        let links = BTreeMap::from([(
            "101".into(),
            vec!["201".into(), "202".into(), "203".into(), "204".into()],
        )]);
        let labels = BTreeMap::from([
            (
                "ichijitaiou_shuugyou_joukyou".into(),
                BTreeMap::from([("working".into(), "在職中".into())]),
            ),
            (
                "media".into(),
                BTreeMap::from([("search".into(), "求人検索サイト".into())]),
            ),
        ]);
        let out = extract(
            "30",
            &rows,
            &notes,
            &links,
            &BTreeSet::from(["101".into()]),
            &labels,
            Some("media"),
            1,
        )
        .unwrap();
        assert_eq!(out.total_applicants, 1);
        assert_eq!(out.items.len(), 1);
        let card = &out.items[0];
        assert_eq!(
            card.fields
                .iter()
                .map(|f| (f.label.as_str(), f.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("就業状況", "在職中"), ("応募動機", "研修があるため")]
        );
        assert_eq!(card.media.as_deref(), Some("求人検索サイト"));
        assert_eq!(card.application_date.as_deref(), Some("2026-10-03"));
        assert!(card.multi_listing);
        assert_eq!(card.notes.len(), 2);
        assert_eq!(card.notes[0].text, "【折り返し】来月から希望");
        assert_eq!(card.notes[1].text, "【架電】休日を優先\n研修 & 見学を希望");
        assert_eq!(
            card.notes[1].time.as_deref(),
            Some("2026-10-03T03:00:00+00:00")
        );
        assert_eq!(card.applicant, applicant_key("30", "101"));
        assert!(!out.truncated);
    }
    #[test]
    fn no_record_is_empty_and_long_unicode_text_is_explicitly_partial() {
        let empty = record(
            "101",
            json!({"bikou_hiaringu":"求人未特定: 自動取込", "oubodouki":null}),
        );
        let out = extract(
            "30",
            &[empty],
            &[],
            &BTreeMap::new(),
            &BTreeSet::new(),
            &OptionLabels::new(),
            None,
            1,
        )
        .unwrap();
        assert_eq!(out.items.len(), 0);
        assert_eq!(out.total_applicants, 1);
        let long = record("102", json!({"tokukijikou":"働".repeat(2001)}));
        let out = extract(
            "30",
            &[long],
            &[],
            &BTreeMap::new(),
            &BTreeSet::new(),
            &OptionLabels::new(),
            None,
            501,
        )
        .unwrap();
        assert_eq!(out.items[0].fields[0].text, "働".repeat(2000));
        assert!(out.truncated);
    }
    #[tokio::test]
    async fn batches_applications_and_notes_and_propagates_read_failures() {
        use axum::{http::Uri, routing::any};
        use std::sync::atomic::{AtomicBool, Ordering};
        let requests = Arc::new(std::sync::Mutex::new(Vec::<(String, Value)>::new()));
        let broken = Arc::new(AtomicBool::new(false));
        let (seen, error) = (requests.clone(), broken.clone());
        let app = Router::new().fallback(any(move |uri: Uri, body: axum::body::Bytes| {
            let (seen, error) = (seen.clone(), error.clone());
            async move {
                let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let path = uri.path().to_owned();
                seen.lock().unwrap().push((path.clone(), body));
                let reply = match path.as_str() {
                    "/crm/v4/objects/0-420/30/associations/0-421" => json!({"results":[{"toObjectId":101},{"toObjectId":102}]}),
                    "/crm/v3/properties/0-421" => json!({"results":[
                        {"name":"ichijitaiou_shuugyou_joukyou","type":"enumeration","options":[{"value":"working","label":"在職中"}]},
                        {"name":"synthetic_media","label":"応募媒体","type":"enumeration","options":[{"value":"search","label":"求人検索サイト"}]}
                    ]}),
                    "/crm/v3/objects/0-421/batch/read" => json!({"results":[
                        {"id":"101","properties":{"yingmuri":"2026-10-01","ichijitaiou_shuugyou_joukyou":"working","synthetic_media":"search","bikou_hiaringu":"求人未特定: 自動注記"}},
                        {"id":"102","properties":{"yingmuri":"2026-10-03","oubodouki":"研修を希望"}}
                    ]}),
                    "/crm/v4/associations/0-421/notes/batch/read" => json!({"results":[
                        {"from":{"id":"101"},"to":[{"toObjectId":201},{"toObjectId":202},{"toObjectId":203}]},
                        {"from":{"id":"102"},"to":[]}
                    ]}),
                    "/crm/v4/associations/0-421/0-420/batch/read" => json!({"results":[
                        {"from":{"id":"101"},"to":[{"toObjectId":30},{"toObjectId":31}]},
                        {"from":{"id":"102"},"to":[{"toObjectId":30}]}
                    ]}),
                    "/crm/v3/objects/notes/batch/read" if error.load(Ordering::SeqCst) => json!({"results":[]}),
                    "/crm/v3/objects/notes/batch/read" => json!({"results":[
                        {"id":"201","properties":{"hs_note_body":"応募者対応メモテンプレート"}},
                        {"id":"202","properties":{"hs_note_body":"【求人由来の暗黙知（自動コピー）】研修"}},
                        {"id":"203","properties":{"hs_note_body":"<p>【架電】見学を希望</p>","hs_timestamp":"2026-10-01T01:00:00Z"}}
                    ]}),
                    _ => panic!("unexpected fixture path: {path}"),
                };
                Json(reply)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let service = JobReadService::for_test(base);
        let result = service.phone_hearing("30").await.unwrap();
        assert_eq!(result.total_applicants, 2);
        assert_eq!(result.items.len(), 2);
        assert_eq!(
            result.items[0].application_date.as_deref(),
            Some("2026-10-03")
        );
        assert_eq!(result.items[1].fields[0].text, "在職中");
        assert_eq!(result.items[1].fields.len(), 1);
        assert_eq!(result.items[1].notes[0].text, "【架電】見学を希望");
        assert_eq!(result.items[1].notes.len(), 1);
        assert!(result.items[1].multi_listing);
        let seen = requests.lock().unwrap().clone();
        let reads: Vec<_> = seen
            .iter()
            .filter(|(path, _)| path.ends_with("/batch/read"))
            .collect();
        assert_eq!(reads.len(), 4);
        assert_eq!(
            reads
                .iter()
                .filter(|(p, _)| p.ends_with("objects/notes/batch/read"))
                .count(),
            1
        );
        assert!(reads
            .iter()
            .filter(|(p, _)| p.ends_with("objects/0-421/batch/read"))
            .all(|(_, b)| b["inputs"].as_array().unwrap().len() == 2));
        broken.store(true, Ordering::SeqCst);
        assert_eq!(
            service.phone_hearing("30").await.unwrap_err().code(),
            "hubspot_partial_read"
        );
        server.abort();
    }
    #[test]
    fn html_is_decoded_without_executable_content_and_unknown_choices_are_not_internal_codes() {
        assert_eq!(
            plain("<p>見学 &amp; 研修</p><script>隠す文</script><style>隠す様式</style>").unwrap(),
            "見学 & 研修"
        );
        let labels = BTreeMap::from([("ichijitaiou_shuugyou_joukyou".into(), BTreeMap::new())]);
        let row = record(
            "101",
            json!({"ichijitaiou_shuugyou_joukyou":"removed_choice"}),
        );
        assert_eq!(
            display(&row, "ichijitaiou_shuugyou_joukyou", &labels).as_deref(),
            Some("選択内容を確認できません")
        );
    }
}
