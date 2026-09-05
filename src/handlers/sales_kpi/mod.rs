//! 営業KPI（現場版）
//!
//! 2026-09-05。営業の現場が毎朝見る画面。架電クオリティ（`/call-quality`）とは
//! 見る人も目的も違うので、同じアプリの中の**別ページ**として持つ。
//!
//! ------------------------------------------------------------------
//! データの出どころ
//! ------------------------------------------------------------------
//! GAS プロジェクト `sales_kpi_sync`（Hubspot リポジトリ
//! `scripts\gas\sales_kpi_sync\`）が毎朝スプレッドシートに書く7枚を読む。
//! 架電クオリティと同じスプレッドシートなので `SheetStore` を共有する。
//!
//!   KPI営業_商談      商談予定日が範囲内の取引（前月1日〜来週末）
//!   KPI営業_アポ      当月に「アポ日確定」へ入った取引
//!   KPI営業_Cヨミ     ステージが「Cヨミ」の取引
//!   KPI営業_架電日次  日 × 担当者の架電数（Zoom）
//!   KPI営業_架電リスト アポ前パイプラインの状態と、決定者・決裁者の入力状況
//!   KPI営業_メンバー  ownerId → 氏名・チーム
//!   KPI営業_取得条件  いつ・どの範囲で取ったか
//!
//! **仕分け（実施/未実施/未処理/予定）はシートに入っていない。ここで判定する。**
//! 現場ヒアリングで判定が変わる見込みがあり、変わるたびにシートを作り直したくないため。
//!
//! ------------------------------------------------------------------
//! 日付は文字列のまま比べる
//! ------------------------------------------------------------------
//! シートの日時は `yyyy-MM-dd HH:mm`（JST）の固定長。ゼロ埋めされているので
//! 辞書順の比較が時刻順の比較と一致する。パースを挟まないぶん、
//! タイムゾーンの取り違えが起きない。

pub mod routes;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- ステージ定義

/// アポ日確定。ここに留まったまま予定日を過ぎたものが「未処理」。
pub const ST_APO: &str = "52035886";
/// アポ日確定（BPO）
pub const ST_APO_BPO: &str = "1095457875";
/// Cヨミ
pub const ST_C: &str = "52035889";
/// BPO 商談実施
pub const ST_BPO_JISSHI: &str = "1325086466";

/// 商談後に移るステージ。ここに居れば予定日によらず「実施した」と見なす。
/// （2026-09-03 ユーザー確定: 商談した事実はステージ移動でも判断する）
const JISSHI: &[&str] = &[
    "52035887", "52035888", "52035889", "52035890", "52035891", "52017683",
];
/// 商談後に移るパイプライン。商談PLを出ていれば実施したということ。
const JISSHI_PIPELINES: &[&str] = &["62583420", "22417753", "21596025", "913508269"];

/// 結果が「やらなかった」で確定したステージと、その理由。
const MIJISSHI: &[(&str, &str)] = &[
    ("1422048803", "先方都合キャンセル"),
    ("1422048804", "当社都合キャンセル"),
    ("1330563334", "BPO商談未実施処理"),
    ("71794963", "日程再調整中"),
];

/// 止まっている取引をさかのぼる日数。
pub const STALE_DAYS: i64 = 60;
/// Cヨミが「置きっぱなし」と見なされる日数。
pub const CYOMI_STALE_DAYS: i64 = 30;

pub const SHEET_SHODAN: &str = "KPI営業_商談";
pub const SHEET_APO: &str = "KPI営業_アポ";
pub const SHEET_CYOMI: &str = "KPI営業_Cヨミ";
pub const SHEET_KADEN: &str = "KPI営業_架電日次";
pub const SHEET_KADEN_LIST: &str = "KPI営業_架電リスト";
pub const SHEET_MEMBER: &str = "KPI営業_メンバー";
pub const SHEET_META: &str = "KPI営業_取得条件";

// ---------------------------------------------------------------- 取引

/// シート1行ぶんの取引。列は名前で引く（位置で決め打ちしない）。
#[derive(Debug, Clone)]
pub struct Deal {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub pipeline: String,
    pub stage: String,
    /// 商談予定日時 `yyyy-MM-dd HH:mm`。空なら未設定。
    pub scheduled: String,
    pub jikan: String,
    pub bpo_appo: String,
    pub has_survey: bool,
    pub exited_apo: String,
    pub exited_apo_bpo: String,
    pub entered_c: String,
}

impl Deal {
    fn from_row(sheet: &SheetData, row: &[Arc<str>]) -> Self {
        let g = |name: &str| sheet.get(row, name).to_string();
        Self {
            id: g("dealId"),
            name: g("取引名"),
            owner: g("ownerId"),
            pipeline: g("pipeline"),
            stage: g("dealstage"),
            scheduled: g("商談予定日時"),
            jikan: g("時間"),
            bpo_appo: g("BPOアポ取得日"),
            has_survey: !g("事前アンケート").is_empty(),
            exited_apo: g("アポ日確定を出た日"),
            exited_apo_bpo: g("BPOアポ日確定を出た日"),
            entered_c: g("Cヨミに入った日"),
        }
    }

    pub fn date(&self) -> &str {
        self.scheduled.get(..10).unwrap_or("")
    }
}

/// 仕分けの結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 商談をやった
    Done,
    /// やらないことが決まった（キャンセル・再調整）
    NotDone,
    /// 予定日を過ぎたのにアポ日確定のまま。手が止まっている
    Stuck,
    /// これから
    Upcoming,
    /// どちらとも言えない
    Unknown,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Done => "実施",
            Kind::NotDone => "未実施",
            Kind::Stuck => "未処理",
            Kind::Upcoming => "これから",
            Kind::Unknown => "要判定",
        }
    }
}

/// 取引1件を仕分ける。`cutoff` は「ここより前は結果が出ているはず」の境目
/// （`yyyy-MM-dd HH:mm`。当日0時を渡す）。
///
/// 🔴 当日を含めると、まだ商談が終わっていないものが「未処理」に落ちて
/// 商談化率が下がる。実際に 9/03 の68件が混ざって 41.0% と出たことがある
/// （正しくは 65.2%）。境目は必ず当日0時にすること。
pub fn classify(deal: &Deal, cutoff: &str) -> (Kind, String) {
    let past = !deal.scheduled.is_empty() && deal.scheduled.as_str() < cutoff;
    let suffix = if past { "" } else { "（予定日は未来）" };

    // ① 商談後のステージ／パイプラインに移っている = やった事実。日付によらない
    if JISSHI.contains(&deal.stage.as_str())
        || JISSHI_PIPELINES.contains(&deal.pipeline.as_str())
        || deal.stage == ST_BPO_JISSHI
    {
        return (Kind::Done, format!("ステージ/PLで確定{suffix}"));
    }

    // ② やらないことが決まっている。これも日付によらない
    if let Some((_, reason)) = MIJISSHI.iter().find(|(id, _)| *id == deal.stage) {
        return (Kind::NotDone, format!("{reason}{suffix}"));
    }

    // ③ アポ日確定のまま
    if deal.stage == ST_APO || deal.stage == ST_APO_BPO {
        return if past {
            (Kind::Stuck, "アポ日確定のまま".into())
        } else {
            (Kind::Upcoming, "これから".into())
        };
    }

    // ④ 予定日がまだ来ていなければ、結果はこれから
    if !past {
        return (Kind::Upcoming, "これから".into());
    }

    // ⑤ アポ日確定を出た日と予定日を比べる。予定日を過ぎてから動かしていれば
    //    商談をやってから動かしたと見る
    let exited = if !deal.exited_apo.is_empty() {
        &deal.exited_apo
    } else {
        &deal.exited_apo_bpo
    };
    if !exited.is_empty() && !deal.scheduled.is_empty() {
        return if exited.as_str() >= deal.scheduled.as_str() {
            (Kind::Done, "予定日通過後に戻し".into())
        } else {
            (Kind::NotDone, "予定日前に戻し".into())
        };
    }

    (Kind::Unknown, format!("ステージ {}", deal.stage))
}

// ---------------------------------------------------------------- 読み込み

pub struct Sheets {
    pub shodan: Arc<SheetData>,
    pub apo: Arc<SheetData>,
    pub cyomi: Arc<SheetData>,
    pub kaden: Arc<SheetData>,
    pub kaden_list: Arc<SheetData>,
    pub member: Arc<SheetData>,
    pub meta: Arc<SheetData>,
    /// 全部キャッシュから返せたか（画面に鮮度を出すため）
    pub all_cached: bool,
}

pub async fn load(client: &SheetsClient, store: &SheetStore) -> Result<Sheets> {
    let mut cached = true;
    macro_rules! fetch {
        ($name:expr) => {{
            let (data, hit) = store
                .get(client, $name)
                .await
                .with_context(|| format!("シート「{}」が読めません", $name))?;
            cached &= hit;
            data
        }};
    }
    Ok(Sheets {
        shodan: fetch!(SHEET_SHODAN),
        apo: fetch!(SHEET_APO),
        cyomi: fetch!(SHEET_CYOMI),
        kaden: fetch!(SHEET_KADEN),
        kaden_list: fetch!(SHEET_KADEN_LIST),
        member: fetch!(SHEET_MEMBER),
        meta: fetch!(SHEET_META),
        all_cached: cached,
    })
}

pub fn deals_of(sheet: &SheetData) -> Vec<Deal> {
    sheet.rows.iter().map(|r| Deal::from_row(sheet, r)).collect()
}

// ---------------------------------------------------------------- メンバー

#[derive(Debug, Clone, Serialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub team: String,
}

pub fn members_of(sheet: &SheetData) -> HashMap<String, Person> {
    sheet
        .rows
        .iter()
        .map(|r| {
            let id = sheet.get(r, "ownerId").to_string();
            let name = sheet.get(r, "氏名").to_string();
            let team = sheet.get(r, "チーム").to_string();
            (
                id.clone(),
                Person {
                    id,
                    name,
                    team: if team.is_empty() { "チーム未設定".into() } else { team },
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------- 集計

/// チーム／個人ごとの数え上げ。キーは画面がそのまま出す日本語ラベル。
pub type Counts = BTreeMap<String, i64>;

/// BPO 経由かどうか。
///
/// 🔴 `bpo_appo_date` は「過去に一度でも BPO のアポで商談していると値が残り続ける」
/// （2026-09-04 ユーザー指摘）。値の有無で判定すると古い日付まで拾ってしまい、
/// 9月の商談200件中71件が2025年まで遡る日付だった。
/// **当月と前月の窓に入っているものだけ**を BPO 経由とする（ユーザー承認済み）。
pub fn is_bpo(deal: &Deal, prev_month_start: &str, month_end: &str) -> bool {
    !deal.bpo_appo.is_empty()
        && deal.bpo_appo.as_str() >= prev_month_start
        && deal.bpo_appo.as_str() < month_end
}

/// 画面に出す取引1件。
#[derive(Debug, Serialize)]
pub struct DealRow {
    pub id: String,
    pub name: String,
    pub date: String,
    pub time: String,
    pub owner: String,
    #[serde(rename = "ownerName")]
    pub owner_name: String,
    pub team: String,
    pub bpo: bool,
    pub kind: &'static str,
    pub why: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anq: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub past: Option<bool>,
}

pub fn deal_row(
    deal: &Deal,
    kind: Kind,
    why: String,
    members: &HashMap<String, Person>,
    bpo: bool,
) -> DealRow {
    let person = members.get(&deal.owner);
    DealRow {
        id: deal.id.clone(),
        name: if deal.name.is_empty() { "（取引名なし）".into() } else { deal.name.clone() },
        date: deal.date().to_string(),
        time: if deal.jikan.is_empty() {
            deal.scheduled.get(11..).unwrap_or("").to_string()
        } else {
            deal.jikan.clone()
        },
        owner: deal.owner.clone(),
        owner_name: person.map(|p| p.name.clone()).unwrap_or_else(|| {
            if deal.owner.is_empty() { "担当なし".into() } else { format!("owner_{}", deal.owner) }
        }),
        team: person.map(|p| p.team.clone()).unwrap_or_else(|| "チーム未設定".into()),
        bpo,
        kind: kind.label(),
        why,
        days: None,
        anq: None,
        past: None,
    }
}

/// 架電1行。
#[derive(Debug, Clone)]
pub struct KadenRow {
    pub date: String,
    pub owner: String,
    pub email: String,
    pub dept: String,
    pub calls: i64,
    pub connected: i64,
    pub long: i64,
}

pub fn kaden_of(sheet: &SheetData) -> Vec<KadenRow> {
    sheet
        .rows
        .iter()
        .map(|r| {
            let num = |name: &str| sheet.get(r, name).replace(',', "").parse::<i64>().unwrap_or(0);
            KadenRow {
                date: sheet.get(r, "日付").to_string(),
                owner: sheet.get(r, "ownerId").to_string(),
                email: sheet.get(r, "Zoomメール").to_string(),
                dept: sheet.get(r, "部署").to_string(),
                calls: num("発信"),
                connected: num("架電数"),
                long: num("5分超"),
            }
        })
        .collect()
}

/// 期間を切って、チーム別・個人別に足す。
#[derive(Debug, Default, Serialize)]
pub struct KadenPeriod {
    pub days: Vec<String>,
    pub total: Counts,
    pub matched: i64,
    pub by_team: BTreeMap<String, Counts>,
    pub by_person: BTreeMap<String, Counts>,
}

pub fn kaden_period(
    rows: &[KadenRow],
    days: &[String],
    members: &HashMap<String, Person>,
) -> KadenPeriod {
    let want: HashSet<&str> = days.iter().map(|s| s.as_str()).collect();
    let mut out = KadenPeriod {
        days: days.to_vec(),
        ..Default::default()
    };
    for row in rows.iter().filter(|r| want.contains(r.date.as_str())) {
        for (key, value) in [
            ("calls", row.calls),
            ("connected", row.connected),
            ("long", row.long),
        ] {
            *out.total.entry(key.to_string()).or_insert(0) += value;
            if row.owner.is_empty() {
                continue;
            }
            let team = members
                .get(&row.owner)
                .map(|p| p.team.clone())
                .unwrap_or_else(|| "チーム未設定".into());
            *out.by_team.entry(team).or_default().entry(key.to_string()).or_insert(0) += value;
            *out
                .by_person
                .entry(row.owner.clone())
                .or_default()
                .entry(key.to_string())
                .or_insert(0) += value;
        }
        if !row.owner.is_empty() {
            out.matched += row.calls;
        }
    }
    out
}

