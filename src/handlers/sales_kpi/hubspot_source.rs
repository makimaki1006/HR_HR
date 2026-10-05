//! 営業KPI: HubSpot から「軽い 6 ブロック」を直接組む (段階 1 の最初の PR、2026-10-05)
//!
//! 目的は「HubSpot の情報がリアルタイムに出る」こと。Python `sync_daily.py`
//! (Hubspot リポジトリ。`feat/sales-kpi-shoudanzokusei` 以降の版) が毎朝シートに書いていた
//! 次のブロックを、アプリが HubSpot から直接取って **同じ列名・同じ文字列の `SheetData`** にする。
//! `build_payload` と JSON の形は変えない。
//!
//! | ブロック | シート名 | Python の関数 |
//! |---|---|---|
//! | 商談 / アポ / Cヨミ | KPI営業_商談 / _アポ / _Cヨミ | `sync_shodan` の検索 + `deal_row` |
//! | 決定者(当日分) | KPI営業_決定者 | `kettei_rows` |
//! | メンバー | KPI営業_メンバー | `fetch_owners` + `member_rows` |
//! | 架電リスト(全社) | KPI営業_架電リスト | `default_pipeline_stages` + `kaden_list_rows` |
//!
//! **対象外(シートのまま)**: 架電リスト担当別・リスト在庫・架電日次(Zoom)・週次・決定者の過去日。
//!
//! 🔴 値の形は Python の写し。違いが出うる所は次のとおり(報告にも書く):
//! - 日時は `jst_text` と同じ規則で `yyyy-MM-dd HH:mm`(JST)。タイムゾーン無しの日時・日付だけの値は
//!   JST として読む(Python は端末のタイムゾーンで読む。ワークフローは `TZ=Asia/Tokyo`)
//! - 応答に `results` / `total` が無いときは、Python は 0 件として進むが、ここでは失敗にする
//!   (0 件に見せないため)
//! - 名簿(別スプレッドシート)は読めないので、前回のシート `KPI営業_メンバー` の「出どころ=名簿」の
//!   行から復元する(名簿の変更は次の Python 同期まで反映されない)
//!
//! 呼び出し回数は 1 回の更新で固定の式になる(`DirectBlocks::requests`):
//! 商談/アポ/Cヨミ/決定者それぞれ `max(1, ceil(件数/200))`(9,800 件で打ち切り)、
//! Owners は 在籍・退職それぞれ `max(1, ceil(人数/100))`、パイプライン 1、
//! 架電リストは `ステージ数 + 1 + 4`。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{json, Value};

use super::{ST_APO, ST_APO_BPO, ST_C};
use crate::handlers::call_quality::sheets::SheetData;
use crate::hubspot::client::parse_record;
use crate::hubspot::{HubSpotClient, HubSpotError, HubSpotRecord};

/// 検索 1 ページの件数と、打ち切り件数(HubSpot は `after` が 1 万件で 400 になる)。
const PAGE_LIMIT: u64 = 200;
const MAX_SEARCH_ROWS: usize = 9800;

/// 事前アンケートが返ってきたかの判定に使う項目 (Python `ANKETO_PROPS`)。どれか 1 つでも入っていれば「有」。
pub const ANKETO_PROPS: [&str; 15] = [
    "jizenanketo_tantou_namae",
    "jizenanketo_bosyu_ninzuu",
    "jizenanketo_bosyu_shokushu",
    "jizenanketo_juushi_point",
    "jizenanketo_nyuusha_jiki",
    "jizenanketo_saiyou_yosan",
    "jizenanketo_sentei_yakuwari1",
    "jizenanketo_kentouchuu_service",
    "jizenanketo_riyouchuu_service",
    "jizenanketo_gyoushu",
    "jizenanketo_tantou_yakushoku1",
    "jizenanketo_kaishamei",
    "jizenanketo_bosyu_haikei",
    "jizenanketo_kyoten",
    "jizenanketo_tantou_busho",
];

/// 商談 3 シートの見出し (Python `DEAL_HEADER`)。末尾に足す(位置で見る箇所があるため)。
pub const DEAL_HEADER: [&str; 16] = [
    "dealId",
    "取引名",
    "ownerId",
    "pipeline",
    "dealstage",
    "商談予定日時",
    "時間",
    "BPOアポ取得日",
    "事前アンケート",
    "アポ取得者",
    "アポ日確定を出た日",
    "BPOアポ日確定を出た日",
    "アポ日確定に入った日",
    "Cヨミに入った日",
    "商談種別",
    "商談属性",
];

/// 商談検索で取るプロパティ (Python `DEAL_PROPS`。並びも同じ)。
pub fn deal_props() -> Vec<String> {
    let mut v: Vec<String> = [
        "dealname",
        "hubspot_owner_id",
        "pipeline",
        "dealstage",
        "scheduled_business_meeting_date",
        "jikan",
        "bpo_appo_date",
        "aposyutokusya",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.extend(ANKETO_PROPS.iter().map(|s| s.to_string()));
    v.push(format!("hs_v2_date_exited_{ST_APO}"));
    v.push(format!("hs_v2_date_exited_{ST_APO_BPO}"));
    v.push(format!("hs_v2_date_entered_{ST_APO}"));
    v.push(format!("hs_v2_date_entered_{ST_C}"));
    v.push("negotiation_type".to_string());
    v.push("shoudanzokusei".to_string());
    v
}

/// 決定者・決裁者の項目 (プロパティ名, 列名)。並びがそのまま列の並び (Python `KETTEI_PROPS`)。
pub const KETTEI_PROPS: [(&str, &str); 4] = [
    ("ketteishamei", "決定者名"),
    ("ketteishanoyakushoku", "決定者の役職"),
    ("kessaishamei", "決裁者名"),
    ("kessaishanoyakushoku", "決裁者の役職"),
];
pub const KETTEI_HEADER: [&str; 7] = [
    "日付",
    "ownerId",
    "決定者名",
    "決定者の役職",
    "決裁者名",
    "決裁者の役職",
    "合計",
];

pub const MEMBER_HEADER: [&str; 8] = [
    "ownerId",
    "氏名",
    "チーム",
    "メール",
    "HubSpotチーム",
    "在籍",
    "出どころ",
    "集計対象",
];
pub const KADEN_LIST_HEADER: [&str; 4] = ["区分", "名前", "分類", "件数"];

/// 担当者が入っていない取引の ownerId (Python `OWNER_NONE`)。
const OWNER_NONE: &str = "";

/// アポ前パイプラインのステージ → 現場の言い方 (Python `KADEN_CLASS`)。ここに無いものは「対象外」。
fn kaden_class(stage_id: &str) -> &'static str {
    match stage_id {
        "appointmentscheduled" => "未架電",
        "presentationscheduled" | "decisionmakerboughtin" | "qualifiedtobuy" => "未接触",
        "closedwon" | "122445644" | "122445645" | "1366400580" | "1332175104" | "51997752" => {
            "接触済み"
        }
        _ => "対象外",
    }
}

// ---------------------------------------------------------------- エラー

/// 更新の失敗。**HubSpot の応答本文・トークンは含めない**(`HubSpotError` の Display は含まない)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    /// `hubspot_rate_limited` / `hubspot_timeout` / `hubspot_auth` など(`HubSpotError::error_kind`)
    pub kind: &'static str,
    pub detail: String,
}

impl From<HubSpotError> for FetchError {
    fn from(e: HubSpotError) -> Self {
        Self {
            kind: e.error_kind(),
            detail: e.to_string(),
        }
    }
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.detail, self.kind)
    }
}

impl std::error::Error for FetchError {}

fn decode(msg: &str) -> FetchError {
    HubSpotError::Decode(msg.to_string()).into()
}

// ---------------------------------------------------------------- 日付の窓

/// 商談検索の窓 (Python `sync_shodan` の先頭と同じ式)。すべて JST の日付。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Windows {
    pub today: NaiveDate,
    pub month_start: NaiveDate,
    pub month_end: NaiveDate,
    pub week_start: NaiveDate,
    /// 商談の範囲 [lo, hi)
    pub lo: NaiveDate,
    pub hi: NaiveDate,
}

impl Windows {
    pub fn of(today: NaiveDate) -> Self {
        let month_start = today.with_day(1).expect("1 日は必ずある");
        let prev_month_start = (month_start - Duration::days(1))
            .with_day(1)
            .expect("1 日は必ずある");
        let month_end = (month_start + Duration::days(40))
            .with_day(1)
            .expect("1 日は必ずある");
        let week_start = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
        let next_week_end = week_start + Duration::days(14);
        let lo = prev_month_start.min(today - Duration::days(super::STALE_DAYS));
        let hi = month_end.max(next_week_end);
        Self {
            today,
            month_start,
            month_end,
            week_start,
            lo,
            hi,
        }
    }
}

/// JST の 0 時のエポックミリ秒を、HubSpot の検索値(文字列)にする (Python `ms()`)。
pub fn jst_midnight_ms(d: NaiveDate) -> String {
    let utc_ms = d
        .and_hms_opt(0, 0, 0)
        .expect("0 時は必ずある")
        .and_utc()
        .timestamp_millis();
    (utc_ms - 9 * 3600 * 1000).to_string()
}

fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("JST")
}

fn fmt_jst(dt: DateTime<FixedOffset>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// HubSpot の日時を `yyyy-MM-dd HH:mm`(JST) に。空なら空文字 (Python `jst_text`)。
///
/// 数字(エポックミリ秒)ならそれ、そうでなければ ISO 8601。タイムゾーン無しは JST として読む。
pub fn jst_text(value: Option<&str>) -> String {
    let Some(v) = value else {
        return String::new();
    };
    if v.is_empty() || v == "null" {
        return String::new();
    }
    if let Ok(n) = v.trim().parse::<f64>() {
        if !n.is_finite() {
            return String::new();
        }
        let secs = (n / 1000.0).floor();
        if secs.abs() > 1.0e11 {
            return String::new();
        }
        return match DateTime::<Utc>::from_timestamp(secs as i64, 0) {
            Some(dt) => fmt_jst(dt.with_timezone(&jst())),
            None => String::new(),
        };
    }
    let iso = v.replace('Z', "+00:00");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&iso) {
        return fmt_jst(dt.with_timezone(&jst()));
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(n) = NaiveDateTime::parse_from_str(&iso, f) {
            return n.format("%Y-%m-%d %H:%M").to_string();
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(&iso, "%Y-%m-%d") {
        return format!("{d} 00:00");
    }
    String::new()
}

// ---------------------------------------------------------------- 行の組み立て (純粋関数)

fn prop<'a>(r: &'a HubSpotRecord, name: &str) -> Option<&'a str> {
    r.properties.get(name).and_then(|v| v.as_deref())
}

/// `str(p.get(name) or "")`
fn prop_s(r: &HubSpotRecord, name: &str) -> String {
    prop(r, name).unwrap_or("").to_string()
}

/// 商談 1 件を 16 列の行にする (Python `deal_row`)。
pub fn deal_row(r: &HubSpotRecord) -> Vec<String> {
    let has_survey = ANKETO_PROPS
        .iter()
        .any(|k| !prop(r, k).unwrap_or("").trim().is_empty());
    vec![
        r.id.clone(),
        prop_s(r, "dealname"),
        prop_s(r, "hubspot_owner_id"),
        prop_s(r, "pipeline"),
        prop_s(r, "dealstage"),
        jst_text(prop(r, "scheduled_business_meeting_date")),
        prop_s(r, "jikan"),
        jst_text(prop(r, "bpo_appo_date")),
        if has_survey {
            "有".into()
        } else {
            String::new()
        },
        prop_s(r, "aposyutokusya"),
        jst_text(prop(r, &format!("hs_v2_date_exited_{ST_APO}"))),
        jst_text(prop(r, &format!("hs_v2_date_exited_{ST_APO_BPO}"))),
        jst_text(prop(r, &format!("hs_v2_date_entered_{ST_APO}"))),
        jst_text(prop(r, &format!("hs_v2_date_entered_{ST_C}"))),
        prop_s(r, "negotiation_type"),
        prop_s(r, "shoudanzokusei"),
    ]
}

/// 商談シートの行 (予定日時の昇順。同じなら取得順 = Python の安定ソート)。
pub fn shodan_rows(deals: &[HubSpotRecord]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = deals.iter().map(deal_row).collect();
    rows.sort_by(|a, b| a[5].cmp(&b[5]));
    rows
}

/// 決定者・決裁者を担当者ごとに数えた行 (Python `kettei_rows`)。多い順(同数は取得順)。
pub fn kettei_rows(deals: &[HubSpotRecord], day: &str) -> Vec<Vec<String>> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, [u64; 4]> = BTreeMap::new();
    for d in deals {
        let oid = match prop(d, "hubspot_owner_id") {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => OWNER_NONE.to_string(),
        };
        if !counts.contains_key(&oid) {
            order.push(oid.clone());
        }
        let c = counts.entry(oid).or_insert([0; 4]);
        for (i, (p, _)) in KETTEI_PROPS.iter().enumerate() {
            if !prop(d, p).unwrap_or("").trim().is_empty() {
                c[i] += 1;
            }
        }
    }
    let mut rows: Vec<(u64, Vec<String>)> = order
        .into_iter()
        .map(|oid| {
            let c = counts[&oid];
            let sum: u64 = c.iter().sum();
            let mut row = vec![day.to_string(), oid];
            row.extend(c.iter().map(|n| n.to_string()));
            row.push(sum.to_string());
            (sum, row)
        })
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    rows.into_iter().map(|(_, r)| r).collect()
}

/// HubSpot の担当者 1 人 (Python `fetch_owners` の値)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub name: String,
    pub email: String,
    /// HubSpot 側のチーム(「新規営業」までの粒度)
    pub team: String,
    pub active: bool,
}

/// 名簿の 1 人。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterEntry {
    pub name: String,
    pub team: String,
}

/// 商談の集計から外す相手 (`KPI営業_集計除外`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exclusions {
    pub owner_ids: BTreeSet<String>,
    pub hubspot_teams: BTreeSet<String>,
}

fn id_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn owner_from_json(o: &Value, active: bool) -> Option<(String, Owner)> {
    let oid = o.get("id").and_then(id_text)?;
    let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("");
    let name = format!("{} {}", s("lastName"), s("firstName"))
        .trim()
        .to_string();
    let email = s("email").to_lowercase();
    let teams: &[Value] = o
        .get("teams")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let primary = teams
        .iter()
        .find(|t| t.get("primary").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| teams.first());
    let team = primary
        .and_then(|t| t.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let display = if !name.is_empty() {
        name
    } else if let Some(local) = email.split('@').next().filter(|l| !l.is_empty()) {
        local.to_string()
    } else {
        oid.clone()
    };
    Some((
        oid,
        Owner {
            name: display,
            email,
            team,
            active,
        },
    ))
}

/// ownerId → 氏名・チーム (Python `member_rows`)。名簿が氏名・チームの正。
/// 名簿の人(チームが付いている人)を先に、そのあと名前順。
pub fn member_rows(
    owners: &BTreeMap<String, Owner>,
    roster: &BTreeMap<String, RosterEntry>,
    excl: &Exclusions,
) -> Vec<Vec<String>> {
    let ids: BTreeSet<&String> = owners.keys().chain(roster.keys()).collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for oid in ids {
        let o = owners.get(oid);
        let r = roster.get(oid);
        let hs_team = o.map(|o| o.team.as_str()).unwrap_or("");
        let out = excl.owner_ids.contains(oid)
            || (!hs_team.is_empty() && excl.hubspot_teams.contains(hs_team));
        rows.push(vec![
            oid.clone(),
            r.map(|r| r.name.clone())
                .filter(|n| !n.is_empty())
                .or_else(|| o.map(|o| o.name.clone()).filter(|n| !n.is_empty()))
                .unwrap_or_else(|| oid.clone()),
            r.map(|r| r.team.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "チーム未設定".to_string()),
            o.map(|o| o.email.clone()).unwrap_or_default(),
            hs_team.to_string(),
            match o {
                None => String::new(),
                Some(o) if o.active => "在籍".to_string(),
                Some(_) => "退職".to_string(),
            },
            if r.is_some() {
                "名簿".to_string()
            } else if o.is_some() {
                "HubSpot".to_string()
            } else {
                "不明".to_string()
            },
            if out { "対象外" } else { "対象" }.to_string(),
        ]);
    }
    // (チーム未設定は後ろ, チーム, 氏名)。同じなら ownerId 順(BTreeSet 由来の順を保つ)
    rows.sort_by(|a, b| {
        ((a[2] == "チーム未設定"), &a[2], &a[1]).cmp(&((b[2] == "チーム未設定"), &b[2], &b[1]))
    });
    rows
}

// ---------------------------------------------------------------- 取得 (HubSpot)

/// 1 回の更新の結果。6 ブロックぶん。
#[derive(Debug)]
pub struct DirectBlocks {
    pub shodan: Arc<SheetData>,
    pub apo: Arc<SheetData>,
    pub cyomi: Arc<SheetData>,
    pub member: Arc<SheetData>,
    pub kaden_list: Arc<SheetData>,
    /// 決定者の当日分の日付 (`yyyy-MM-dd`) と行。過去日はシートから読む
    pub kettei_day: String,
    pub kettei_rows: Vec<Vec<String>>,
    pub windows: Windows,
    /// 取得を始めた時刻 `yyyy-MM-dd HH:mm`(JST)
    pub fetched_at: String,
    /// この更新に HubSpot へ投げた論理リクエスト数(内部 retry は数えない)
    pub requests: u32,
    pub took_secs: u64,
    /// 1 万件の上限(9,800 件)で打ち切ったブロック名
    pub truncated: Vec<&'static str>,
}

fn sheet(header: &[&str], rows: Vec<Vec<String>>) -> Arc<SheetData> {
    Arc::new(SheetData {
        header: header.iter().map(|s| s.to_string()).collect(),
        rows: rows
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|c| Arc::<str>::from(c.as_str()))
                    .collect()
            })
            .collect(),
        fetched_at: Instant::now(),
    })
}

struct Ctx<'a> {
    client: &'a HubSpotClient,
    requests: u32,
}

impl Ctx<'_> {
    async fn search(&mut self, body: Value) -> Result<Value, FetchError> {
        self.requests += 1;
        Ok(self.client.search("deals", body).await?)
    }

    /// filterGroups 同士は OR、中の filters は AND (Python `search_deals`)。
    /// 戻り値の bool は「9,800 件で打ち切った」。
    async fn search_all(
        &mut self,
        filter_groups: Value,
        props: &[String],
        sorts: Option<Value>,
    ) -> Result<(Vec<HubSpotRecord>, bool), FetchError> {
        let mut out: Vec<HubSpotRecord> = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut body = json!({
                "filterGroups": filter_groups,
                "properties": props,
                "limit": PAGE_LIMIT,
            });
            if let Some(s) = &sorts {
                body["sorts"] = s.clone();
            }
            if let Some(a) = &after {
                body["after"] = json!(a);
            }
            let j = self.search(body).await?;
            let results = j
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| decode("search without results"))?;
            for r in results {
                out.push(parse_record(r)?);
            }
            let next = j.pointer("/paging/next/after").and_then(id_text);
            match next {
                None => return Ok((out, false)),
                Some(a) => {
                    if results.is_empty() {
                        // 次ページがあると言いながら 0 件。無限に回さない
                        return Err(decode("empty page with next cursor"));
                    }
                    if out.len() >= MAX_SEARCH_ROWS {
                        return Ok((out, true));
                    }
                    after = Some(a);
                }
            }
        }
    }

    /// `limit=1` で `total` だけ読む (Python `count_deals` / `_count_one`)。
    async fn count(&mut self, filters: Value) -> Result<u64, FetchError> {
        let body = json!({ "filterGroups": [{ "filters": filters }], "limit": 1 });
        let j = self.search(body).await?;
        j.get("total")
            .and_then(Value::as_u64)
            .ok_or_else(|| decode("count without total"))
    }

    /// HubSpot の全担当者。退職者は `archived=true` でしか返らないので両方見る。在籍が勝つ。
    async fn owners(&mut self) -> Result<BTreeMap<String, Owner>, FetchError> {
        let mut out: BTreeMap<String, Owner> = BTreeMap::new();
        for archived in [false, true] {
            let mut after: Option<String> = None;
            loop {
                self.requests += 1;
                let j = self.client.owners_page(archived, after.as_deref()).await?;
                let results = j
                    .get("results")
                    .and_then(Value::as_array)
                    .ok_or_else(|| decode("owners without results"))?;
                for o in results {
                    if let Some((id, owner)) = owner_from_json(o, !archived) {
                        out.entry(id).or_insert(owner);
                    }
                }
                match j.pointer("/paging/next/after").and_then(id_text) {
                    None => break,
                    Some(a) => {
                        if results.is_empty() {
                            return Err(decode("empty owners page with next cursor"));
                        }
                        after = Some(a);
                    }
                }
            }
        }
        Ok(out)
    }

    /// アポ前パイプライン (default) のステージを表示順で `(id, label)` にする。
    async fn default_stages(&mut self) -> Result<Vec<(String, String)>, FetchError> {
        self.requests += 1;
        let j = self.client.deal_pipelines().await?;
        let pipe = j
            .get("results")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .find(|p| p.get("id").and_then(id_text).as_deref() == Some("default"))
            })
            .ok_or_else(|| decode("default pipeline not found"))?;
        let stages = pipe
            .get("stages")
            .and_then(Value::as_array)
            .ok_or_else(|| decode("pipeline without stages"))?;
        let mut v: Vec<(i64, String, String)> = Vec::new();
        for s in stages {
            let id = s
                .get("id")
                .and_then(id_text)
                .ok_or_else(|| decode("stage without id"))?;
            let label = s
                .get("label")
                .and_then(Value::as_str)
                .ok_or_else(|| decode("stage without label"))?
                .to_string();
            let order = s
                .get("displayOrder")
                .and_then(|o| {
                    o.as_i64()
                        .or_else(|| o.as_str().and_then(|t| t.parse().ok()))
                })
                .unwrap_or(0);
            v.push((order, id, label));
        }
        v.sort_by_key(|x| x.0);
        Ok(v.into_iter().map(|(_, id, l)| (id, l)).collect())
    }
}

fn pipe_filter() -> Value {
    json!({"propertyName": "pipeline", "operator": "EQ", "value": "default"})
}

/// 6 ブロックを HubSpot から取る。**すべて取れたときだけ `Ok`**(取れたぶんだけ返すと時点が混ざる)。
///
/// 呼び出しの順は Python `sync_shodan` と同じ: 商談 → アポ → Cヨミ → 決定者 → Owners →
/// パイプライン → 架電リストの件数。
pub async fn fetch_blocks(
    client: &HubSpotClient,
    now: DateTime<FixedOffset>,
    roster: &BTreeMap<String, RosterEntry>,
    excl: &Exclusions,
) -> Result<DirectBlocks, FetchError> {
    let started = Instant::now();
    let now = now.with_timezone(&jst());
    let w = Windows::of(now.date_naive());
    let mut ctx = Ctx {
        client,
        requests: 0,
    };
    let mut truncated: Vec<&'static str> = Vec::new();
    let props = deal_props();

    let ge_lt = |name: &str, lo: NaiveDate, hi: NaiveDate| {
        json!([
            {"propertyName": name, "operator": "GTE", "value": jst_midnight_ms(lo)},
            {"propertyName": name, "operator": "LT", "value": jst_midnight_ms(hi)},
        ])
    };
    let apo_entered = format!("hs_v2_date_entered_{ST_APO}");
    let bpo_entered = format!("hs_v2_date_entered_{ST_APO_BPO}");

    let (shodan, t1) = ctx
        .search_all(
            json!([{ "filters": ge_lt("scheduled_business_meeting_date", w.lo, w.hi) }]),
            &props,
            None,
        )
        .await?;
    let (apo, t2) = ctx
        .search_all(
            json!([
                { "filters": ge_lt(&apo_entered, w.month_start, w.month_end) },
                { "filters": ge_lt(&bpo_entered, w.month_start, w.month_end) },
            ]),
            &props,
            None,
        )
        .await?;
    let (cyomi, t3) = ctx
        .search_all(
            json!([{ "filters": [
                {"propertyName": "dealstage", "operator": "EQ", "value": ST_C}
            ] }]),
            &props,
            None,
        )
        .await?;
    for (t, name) in [(t1, "商談"), (t2, "アポ"), (t3, "Cヨミ")] {
        if t {
            truncated.push(name);
        }
    }

    // 決定者・決裁者: 4 項目のどれか 1 つでも入っている取引を全件(hs_object_id 昇順)取って数える
    let kettei_groups: Vec<Value> = KETTEI_PROPS
        .iter()
        .map(|(p, _)| {
            json!({ "filters": [
                pipe_filter(),
                {"propertyName": p, "operator": "HAS_PROPERTY"},
            ] })
        })
        .collect();
    let mut kettei_props: Vec<String> = KETTEI_PROPS.iter().map(|(p, _)| p.to_string()).collect();
    kettei_props.push("hubspot_owner_id".to_string());
    let (kettei_deals, t4) = ctx
        .search_all(
            Value::Array(kettei_groups),
            &kettei_props,
            Some(json!([{ "propertyName": "hs_object_id", "direction": "ASCENDING" }])),
        )
        .await?;
    if t4 {
        truncated.push("決定者");
    }
    let day = w.today.format("%Y-%m-%d").to_string();
    let kettei = kettei_rows(&kettei_deals, &day);

    // メンバー
    let owners = ctx.owners().await?;
    let members = member_rows(&owners, roster, excl);

    // 架電リスト(全社): ステージ別 + 全体 + 充足 4
    let stages = ctx.default_stages().await?;
    let mut list_rows: Vec<Vec<String>> = Vec::new();
    for (id, label) in &stages {
        let n = ctx
            .count(json!([
                pipe_filter(),
                {"propertyName": "dealstage", "operator": "EQ", "value": id},
            ]))
            .await?;
        list_rows.push(vec![
            "ステージ".into(),
            label.clone(),
            kaden_class(id).into(),
            n.to_string(),
        ]);
    }
    let all = ctx.count(json!([pipe_filter()])).await?;
    list_rows.push(vec![
        "合計".into(),
        "アポ前パイプライン全体".into(),
        String::new(),
        all.to_string(),
    ]);
    for (prop_name, label) in KETTEI_PROPS {
        let n = ctx
            .count(json!([
                pipe_filter(),
                {"propertyName": prop_name, "operator": "HAS_PROPERTY"},
            ]))
            .await?;
        list_rows.push(vec![
            "充足".into(),
            label.into(),
            String::new(),
            n.to_string(),
        ]);
    }

    Ok(DirectBlocks {
        shodan: sheet(&DEAL_HEADER, shodan_rows(&shodan)),
        apo: sheet(
            &DEAL_HEADER,
            apo.iter().map(deal_row).collect::<Vec<Vec<String>>>(),
        ),
        cyomi: sheet(
            &DEAL_HEADER,
            cyomi.iter().map(deal_row).collect::<Vec<Vec<String>>>(),
        ),
        member: sheet(&MEMBER_HEADER, members),
        kaden_list: sheet(&KADEN_LIST_HEADER, list_rows),
        kettei_day: day,
        kettei_rows: kettei,
        windows: w,
        fetched_at: fmt_jst(now),
        requests: ctx.requests,
        took_secs: started.elapsed().as_secs(),
        truncated,
    })
}

/// 前回の `KPI営業_メンバー` から名簿(出どころ=名簿)を復元する。
/// 名簿は別スプレッドシートで読めないため。名簿の変更は次の Python 同期まで反映されない。
pub fn roster_from_member_sheet(s: &SheetData) -> BTreeMap<String, RosterEntry> {
    let mut out = BTreeMap::new();
    for row in &s.rows {
        if s.get(row, "出どころ") != "名簿" {
            continue;
        }
        let id = s.get(row, "ownerId").trim();
        if id.is_empty() {
            continue;
        }
        out.insert(
            id.to_string(),
            RosterEntry {
                name: s.get(row, "氏名").to_string(),
                team: s.get(row, "チーム").to_string(),
            },
        );
    }
    out
}

/// `KPI営業_集計除外`(種別 / 値 / 理由) を読む。種別は `HubSpotチーム` と `ownerId` だけ。
pub fn exclusions_from_sheet(s: &SheetData) -> Exclusions {
    let mut out = Exclusions::default();
    for row in &s.rows {
        let kind = s.get(row, "種別").trim();
        let value = s.get(row, "値").trim();
        if value.is_empty() {
            continue;
        }
        match kind {
            "ownerId" => {
                out.owner_ids.insert(value.to_string());
            }
            "HubSpotチーム" => {
                out.hubspot_teams.insert(value.to_string());
            }
            _ => {}
        }
    }
    out
}
