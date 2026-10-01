//! 営業KPI（現場版）
//!
//! 2026-09-05。営業の現場が毎朝見る画面。架電クオリティ（`/call-quality`）とは
//! 見る人も目的も違うので、同じアプリの中の**別ページ**として持つ。
//!
//! ------------------------------------------------------------------
//! データの出どころ
//! ------------------------------------------------------------------
//! Hubspot リポジトリの `scripts/sales_kpi/sync_daily.py` が、GitHub Actions
//! （`.github/workflows/sales_kpi_daily.yml`・月〜金 06:30）で毎朝スプレッドシートに
//! 書くシートを読む。架電クオリティと同じスプレッドシートなので `SheetStore` を共有する。
//!
//! 🔴 一度 GAS（`scripts/gas/call_quality_app/sales_kpi_sync.gs`）で書いていたが、
//!    Zoom の取得層も資格情報も既に Python 側（`scripts/call_quality_monitor/fetch_zoom.py`）に
//!    あり、同じものを2系統持つことになるので撤去した（2026-09-05）。
//!    **そのGASファイルはもう存在しない。**探しに行かないこと。
//!
//!   KPI営業_商談      商談予定日が範囲内の取引（前月1日〜来週末）
//!   KPI営業_アポ      当月に「アポ日確定」へ入った取引
//!   KPI営業_Cヨミ     ステージが「Cヨミ」の取引
//!   KPI営業_架電日次  日 × 担当者の架電数（Zoom）
//!   KPI営業_架電リスト アポ前パイプラインの状態と、決定者・決裁者の入力状況
//!   KPI営業_架電リスト_担当別 上と同じ内訳を担当者ごとに（チーム／個人の絞り込み用）
//!   KPI営業_メンバー  ownerId → 氏名・チーム
//!   KPI営業_取得条件  いつ・どの範囲で取ったか
//!   KPI営業_週次      週に1行の記録（唯一、集計済みの値を持つシート）
//!   KPI営業_決定者    決定者・決裁者の入力状況を、日 × 担当者で持つ
//!   KPI営業_リスト在庫 新規営業のリスト（リクロジ／大分）を、誰が持っているか × 企業人数で数えたもの
//!
//! **仕分け（実施/未実施/未処理/予定）はシートに入っていない。ここで判定する。**
//! 現場ヒアリングで判定が変わる見込みがあり、変わるたびにシートを作り直したくないため。
//!
//! 例外は `KPI営業_週次` だけ。「その週にどうだったか」は材料からは作り直せない
//! （HubSpot は今の状態しか返さない）ので、Python 側が集計してから書く。
//! そのぶん **Python の `classify()` はここの `classify()` の写し**になっている。
//! 片方だけ変えると週次だけ数字がずれる。
//!
//! ------------------------------------------------------------------
//! 日付は文字列のまま比べる
//! ------------------------------------------------------------------
//! シートの日時は `yyyy-MM-dd HH:mm`（JST）の固定長。ゼロ埋めされているので
//! 辞書順の比較が時刻順の比較と一致する。パースを挟まないぶん、
//! タイムゾーンの取り違えが起きない。

pub mod fixture;
pub mod payload;
pub mod routes;

#[cfg(test)]
mod tests;

pub use payload::*;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;
use ts_rs::TS;

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
/// 架電リストの内訳を担当者ごとに持つ（`ownerId / 分類 / 件数`）。
/// **無いことがある**（2026-09-07 に足したので、日次同期が1度も走っていない環境では
/// シート自体が存在しない）。週次と同じく、無ければ空で通す。
pub const SHEET_KADEN_BY_OWNER: &str = "KPI営業_架電リスト_担当別";
pub const SHEET_MEMBER: &str = "KPI営業_メンバー";
pub const SHEET_META: &str = "KPI営業_取得条件";
pub const SHEET_WEEKLY: &str = "KPI営業_週次";
/// 決定者・決裁者の入力状況を、日 × 担当者で持つ（`日付 / ownerId / 決定者名 / …`）。
/// **無いことがある**（2026-09-11 に足したので、日次同期が新しい版で1度も走っていない
/// 環境ではシート自体が存在しない）。週次と同じく、無ければ空で通す。
pub const SHEET_KETTEI: &str = "KPI営業_決定者";
/// 新規営業のリストの在庫（`リスト / 区分 / 内訳 / 企業人数 / 件数`）。
/// **無いことがある**（2026-09-29 に足したので、日次同期が新しい版で1度も走っていない
/// 環境ではシート自体が存在しない）。週次と同じく、無ければ空で通す。
pub const SHEET_LIST_STOCK: &str = "KPI営業_リスト在庫";

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
    /// 架電リストの担当者別。**まだ1度も書かれていないことがある**ので、無ければ空。
    pub kaden_by_owner: Arc<SheetData>,
    pub member: Arc<SheetData>,
    pub meta: Arc<SheetData>,
    /// 週次の記録。**まだ1度も書かれていないことがある**ので、無ければ空。
    pub weekly: Arc<SheetData>,
    /// 決定者・決裁者の入力状況。**まだ1度も書かれていないことがある**ので、無ければ空。
    pub kettei: Arc<SheetData>,
    /// リストの在庫。**まだ1度も書かれていないことがある**ので、無ければ空。
    pub list_stock: Arc<SheetData>,
    /// 全部キャッシュから返せたか（画面に鮮度を出すため）
    pub all_cached: bool,
}

/// 見出しも行も無いシート。まだ作られていない週次シートの代わりに使う。
pub fn empty_sheet() -> Arc<SheetData> {
    Arc::new(SheetData {
        header: Vec::new(),
        rows: Vec::new(),
        fetched_at: std::time::Instant::now(),
    })
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
    // 週次と担当者別だけは無くても通す。あとから足したシートなので、日次同期が
    // まだ新しい版で1度も走っていない環境ではシート自体が存在せず、
    // ここで落とすと画面ごと出なくなる。
    macro_rules! optional {
        ($name:expr, $what:expr) => {
            match store.get(client, $name).await {
                Ok((data, hit)) => {
                    cached &= hit;
                    data
                }
                Err(e) => {
                    tracing::warn!(
                        "シート「{}」が読めないので{}は空で出します: {e:#}",
                        $name,
                        $what
                    );
                    empty_sheet()
                }
            }
        };
    }
    let weekly = optional!(SHEET_WEEKLY, "週次");
    let kaden_by_owner = optional!(SHEET_KADEN_BY_OWNER, "架電リストの担当者別");
    let kettei = optional!(SHEET_KETTEI, "決定者・決裁者");
    let list_stock = optional!(SHEET_LIST_STOCK, "リストの在庫");
    Ok(Sheets {
        shodan: fetch!(SHEET_SHODAN),
        apo: fetch!(SHEET_APO),
        cyomi: fetch!(SHEET_CYOMI),
        kaden: fetch!(SHEET_KADEN),
        kaden_list: fetch!(SHEET_KADEN_LIST),
        kaden_by_owner,
        member: fetch!(SHEET_MEMBER),
        meta: fetch!(SHEET_META),
        weekly,
        kettei,
        list_stock,
        all_cached: cached,
    })
}

// ---------------------------------------------------------------- 週次の記録

fn cell_num(text: &str) -> Option<i64> {
    let t = text.replace(',', "");
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<i64>().ok()
}

/// 週次シートを、画面がそのまま使える配列にする。
///
/// 週が空の行は捨てる（シートの下に空行が残っていることがある）。
/// 画面は古い順に並んでいる前提で末尾8週を出すので、ここで週の昇順に揃える。
/// `2026-W07` のようにゼロ埋めしてあるので辞書順で週順になる。
///
/// 🔴 シートの見出し（`num("母集団")` 等の左側）は Python 側（Hubspot リポジトリ
/// `scripts/sales_kpi/sync_daily.py` の `WEEKLY_HEADER`）と対で決まっている。
/// 片方だけ変えると値が 0 で並ぶ。
///
/// `totals` は**当月**に商談予定日があるもの（月内は積み上がり、月初に入れ替わる）。
/// `week_totals` は**その週（月〜日）**に商談予定日があるもの（2026-09-07 追加）。
/// 週次表に当月ぶんだけを並べると、月初の行で 1,064 → 537 と半減して見える
/// （8月と9月を比べているだけ）。両方を持って、画面で選べるようにしている
/// （2026-09-07 ユーザー判断）。
pub fn snapshots_of(sheet: &SheetData) -> Vec<Snapshot> {
    let mut out: Vec<Snapshot> = Vec::new();
    for row in &sheet.rows {
        let week = sheet.get(row, "週").trim().to_string();
        if week.is_empty() {
            continue;
        }
        let num = |col: &str| cell_num(sheet.get(row, col)).unwrap_or(0);
        let totals = SnapshotTotals {
            pool: num("母集団"),
            done: num("実施"),
            not_done: num("未実施"),
            stuck: num("未処理"),
            upcoming: num("これから"),
            unknown: num("要判定"),
            apo: num("取ったアポ"),
            cyomi: num("Cヨミ"),
            bpo_pool: num("BPO母集団"),
        };
        // その週に予定された商談だけを数えた列（2026-09-07 追加）。
        // 🔴 それ以前に書かれた行にはこの列が無い。`SheetData::get()` は
        //    列が無ければ "" を返すので落ちはしないが、0 を入れると画面が
        //    「その週は0件だった」と嘘をつく。母集団が読めない行は None にして
        //    画面に「—」を出させる。
        let week_totals = cell_num(sheet.get(row, "週_母集団")).map(|_| SnapshotWeekTotals {
            pool: num("週_母集団"),
            done: num("週_実施"),
            not_done: num("週_未実施"),
            stuck: num("週_未処理"),
            upcoming: num("週_これから"),
            unknown: num("週_要判定"),
        });
        out.push(Snapshot {
            week,
            taken_at: sheet.get(row, "記録日").to_string(),
            week_start: sheet.get(row, "週はじまり").to_string(),
            totals,
            stale: num("止まっている"),
            anq_missing: num("アンケート未回収"),
            cyomi_stale: num("Cヨミ置きっぱなし"),
            kaden_called: num("架電リスト手をつけた"),
            kaden_base: num("架電リスト母数"),
            // 架電数だけは「まだ無い」と「0件」を分ける。画面は null を「—」で出す。
            zoom_called: cell_num(sheet.get(row, "Zoom架電数")),
            zoom_days: num("Zoom日数"),
            zoom_partial: sheet.get(row, "Zoom集計中") == "集計中",
            week_totals,
            week_partial: sheet.get(row, "週_集計中") == "集計中",
            list_stock: weekly_list_stock(sheet, row),
        });
    }
    out.sort_by(|a, b| a.week.cmp(&b.week));
    out
}

/// 週次シートの「リスト_<リスト名>_<全体|アクティブ|保管>」列を
/// `{リスト名: {全体, アクティブ, 保管}}` にする。
///
/// 🔴 リスト名はここに書かない。見出しから拾う（Python 側 `LIST_PIPELINES` が
/// 列を作るので、リストが増えてもここは直さずに済む）。並びは見出しの順（`OrderedMap`）。
/// 2026-09-29 より前に書かれた行にはこの列が無い（値が空）。0 にすると画面が
/// 「在庫が0だった」と嘘をつくので、そのリストの「全体」が読めなければ入れない。
/// 1つも無ければ `None`。
fn weekly_list_stock(sheet: &SheetData, row: &[Arc<str>]) -> Option<OrderedMap<StockTrendList>> {
    let mut out: OrderedMap<StockTrendList> = OrderedMap::new();
    for col in &sheet.header {
        let Some(rest) = col.strip_prefix("リスト_") else {
            continue;
        };
        let Some(name) = rest.strip_suffix("_全体") else {
            continue;
        };
        let Some(whole) = cell_num(sheet.get(row, col)) else {
            continue;
        };
        let num =
            |kind: &str| cell_num(sheet.get(row, &format!("リスト_{name}_{kind}"))).unwrap_or(0);
        out.insert(
            name.to_string(),
            StockTrendList {
                whole,
                active: num("アクティブ"),
                stored: num("保管"),
            },
        );
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub fn deals_of(sheet: &SheetData) -> Vec<Deal> {
    sheet
        .rows
        .iter()
        .map(|r| Deal::from_row(sheet, r))
        .collect()
}

// ---------------------------------------------------------------- メンバー

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiPerson")]
pub struct Person {
    pub id: String,
    pub name: String,
    pub team: String,
    /// HubSpot 側のチーム名（`BPO_リクロジ` など）。営業の名簿に載っていない人が
    /// 誰なのかを示す手掛かり。名簿のチームより粒度が粗いので、絞り込みには使わない。
    /// 空なら JSON にキーごと出さない（TS では `hsTeam?: string`。`default` は ts-rs にそれを伝えるためのもの）。
    #[serde(rename = "hsTeam", default, skip_serializing_if = "String::is_empty")]
    pub hs_team: String,
    /// 商談（①③②⑥⑨）の集計に入れるか。
    ///
    /// 🔴 **誰を外すかはここには書かない。** `KPI営業_メンバー` の `集計対象` 列を
    /// そのまま読むだけで、条件は運用シート `KPI営業_集計除外` にしかない
    /// （2026-09-08。現場が触れる場所を1つにするため）。列が無い古いシートでは
    /// 全員 `true` になる＝これまでどおり全員数える。
    ///
    /// 🔴 外すのは商談だけ。架電と架電リストは外さない（コンサル営業も架電はしている）。
    #[serde(skip)]
    pub counted: bool,
}

/// 名簿にも HubSpot にも居ない ownerId のときの表示。
pub fn unknown_person(owner: &str) -> Person {
    Person {
        id: owner.to_string(),
        name: if owner.is_empty() {
            "担当なし".into()
        } else {
            format!("owner_{owner}")
        },
        team: TEAM_NONE.into(),
        hs_team: String::new(),
        counted: true,
    }
}

/// `members` から引く。無ければ `unknown_person`。
pub fn person_of(members: &HashMap<String, Person>, owner: &str) -> Person {
    members
        .get(owner)
        .cloned()
        .unwrap_or_else(|| unknown_person(owner))
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
                    // 氏名が空の行は ownerId のままにする（名前を作らない）。
                    name: if name.is_empty() {
                        unknown_person(&id).name
                    } else {
                        name
                    },
                    id,
                    team: if team.is_empty() {
                        TEAM_NONE.into()
                    } else {
                        team
                    },
                    // 2026-09-07 追加。それ以前のシートにはこの列が無いので空になる。
                    hs_team: sheet.get(r, "HubSpotチーム").to_string(),
                    // 2026-09-08 追加。列が無い古いシートでは全員 true（＝全員数える）。
                    counted: sheet.get(r, "集計対象") != "対象外",
                },
            )
        })
        .collect()
}

// ------------------------------------------------- 架電リストの担当者別

/// 架電リストの分類。画面に出す順で並べてある。
pub const KADEN_CLASSES: &[&str] = &["未架電", "未接触", "接触済み"];

/// 名簿にチームが無い人に付くチーム名。
///
/// 🔴 **「営業のチームかどうか」はこれと比べて判定する。チーム名を列挙しない。**
/// 名簿（`②メンバー配置_入力`）に載っているチームだけが `KPI営業_メンバー` の
/// チーム列に入り、載っていない人は全員これになる。チームが増えても、
/// 名簿に足すだけで営業として数えられる。
pub const TEAM_NONE: &str = "チーム未設定";

/// 名簿にチームが入っているか（＝営業のチームか）。
pub fn is_sales_team(team: &str) -> bool {
    !team.is_empty() && team != TEAM_NONE
}

/// `KPI営業_架電リスト_担当別` を `ownerId → {分類: 件数}` にする。
///
/// 「対象外」の行も入っているが、母数（`base`）には足さない。母数は全社の
/// `KPI営業_架電リスト` と同じ「未架電＋未接触＋接触済み」で揃える。
pub fn kaden_by_owner_of(sheet: &SheetData) -> BTreeMap<String, Counts> {
    let mut out: BTreeMap<String, Counts> = BTreeMap::new();
    for row in &sheet.rows {
        let owner = sheet.get(row, "ownerId").to_string();
        let class = sheet.get(row, "分類");
        if class.is_empty() {
            continue;
        }
        let n = sheet
            .get(row, "件数")
            .replace(',', "")
            .parse::<i64>()
            .unwrap_or(0);
        let per = out.entry(owner).or_default();
        *per.entry(class.to_string()).or_insert(0) += n;
        if KADEN_CLASSES.contains(&class) {
            *per.entry("base".into()).or_insert(0) += n;
        }
    }
    out
}

// ------------------------------------------------- リストの在庫

/// 企業人数で絞らない数を表す帯の名前。Python 側 `STOCK_ALL_BANDS` と対。
pub const STOCK_ALL_BANDS: &str = "すべて";

/// `KPI営業_リスト在庫` を、画面がそのまま使える形にする。
///
/// シートは `リスト / 区分 / 内訳 / 企業人数 / 件数 / 担当者名あり` の縦持ち。区分は
/// `合計`（リスト全体）・`アクティブ`・`保管` のどれか。
///
/// 🔴 **「その他」はシートに無い。ここで 合計 − 内訳の和 として出す。**
/// どの内訳にも当てはまらない担当者（区分シートに書かれていない人）と、
/// 担当者が入っていない取引。黙って落とすと、内訳の和が全体に届かない理由が
/// 画面から読めなくなる。
///
/// 🔴 リスト名・内訳名・帯の名前はここに書かない。全部シートの並びのまま返す
/// （内訳は運用シート `KPI営業_リスト区分` で決まり、ここは読むだけ）。
///
/// `band_gap` は「企業人数=すべて」と帯の合計の差。どの帯にも入らない値
/// （マイナスなど）があれば 0 にならない。画面はこれを出して、帯で絞ったときに
/// 足りなくなる理由を隠さない。
pub fn list_stock_of(sheet: &SheetData) -> ListStock {
    /// 1つの升目の数え上げ。`n` が件数、`named` がそのうち担当者名に人の名前が入っている件数。
    #[derive(Default, Clone)]
    struct Cell {
        n: Counts,
        named: Counts,
    }
    #[derive(Default)]
    struct List {
        total: Cell,
        groups: Vec<(String, String, Cell)>,
    }
    // 「担当者名あり」列は 2026-09-29 の2版目で足した。無い版のシートでは画面に
    // 絞り込みを出さない（0 と読むと「名前が1件も無い」と嘘をつく）。
    let has_named = sheet.header.iter().any(|h| h == "担当者名あり");
    let mut bands: Vec<String> = Vec::new();
    let mut lists: Vec<(String, List)> = Vec::new();
    for row in &sheet.rows {
        let name = sheet.get(row, "リスト").trim();
        let kind = sheet.get(row, "区分").trim();
        let label = sheet.get(row, "内訳").trim();
        let band = sheet.get(row, "企業人数").trim();
        if name.is_empty() || kind.is_empty() || band.is_empty() {
            continue;
        }
        let n = cell_num(sheet.get(row, "件数")).unwrap_or(0);
        let named = cell_num(sheet.get(row, "担当者名あり")).unwrap_or(0);
        if band != STOCK_ALL_BANDS && !bands.iter().any(|b| b == band) {
            bands.push(band.to_string());
        }
        let list = match lists.iter().position(|(n, _)| n == name) {
            Some(i) => &mut lists[i].1,
            None => {
                lists.push((name.to_string(), List::default()));
                &mut lists.last_mut().expect("直前に足した").1
            }
        };
        let cell = if kind == "合計" {
            &mut list.total
        } else {
            match list
                .groups
                .iter()
                .position(|(k, l, _)| k == kind && l == label)
            {
                Some(i) => &mut list.groups[i].2,
                None => {
                    list.groups
                        .push((kind.to_string(), label.to_string(), Cell::default()));
                    &mut list.groups.last_mut().expect("直前に足した").2
                }
            }
        };
        *cell.n.entry(band.to_string()).or_insert(0) += n;
        *cell.named.entry(band.to_string()).or_insert(0) += named;
    }

    let mut all_bands: Vec<String> = vec![STOCK_ALL_BANDS.to_string()];
    all_bands.extend(bands.iter().cloned());
    let get = |c: &Counts, b: &str| c.get(b).copied().unwrap_or(0);
    let out: Vec<StockList> = lists
        .into_iter()
        .map(|(name, list)| {
            // 合計 − 内訳の和。件数と担当者名ありの両方で出す。
            let rest = |pick: fn(&Cell) -> &Counts| -> Counts {
                all_bands
                    .iter()
                    .map(|b| {
                        let parts: i64 = list.groups.iter().map(|(_, _, c)| get(pick(c), b)).sum();
                        (b.clone(), get(pick(&list.total), b) - parts)
                    })
                    .collect()
            };
            let gap = |c: &Counts| -> i64 {
                get(c, STOCK_ALL_BANDS) - bands.iter().map(|b| get(c, b)).sum::<i64>()
            };
            let groups: Vec<StockGroup> = list
                .groups
                .iter()
                .map(|(kind, label, cell)| StockGroup {
                    kind: kind.clone(),
                    name: label.clone(),
                    counts: cell.n.clone(),
                    named: cell.named.clone(),
                })
                .collect();
            let other = rest(|c| &c.n);
            let other_named = rest(|c| &c.named);
            let band_gap = gap(&list.total.n);
            let band_gap_named = gap(&list.total.named);
            StockList {
                name,
                total: list.total.n,
                total_named: list.total.named,
                groups,
                other,
                other_named,
                band_gap,
                band_gap_named,
            }
        })
        .collect();
    ListStock {
        all_band: STOCK_ALL_BANDS,
        bands,
        has_named,
        lists: out,
        // 前の週の記録は `routes::list_stock_block` が後から入れる。
        trend: None,
    }
}

// ------------------------------------------------- 決定者・決裁者

/// 決定者・決裁者の入力状況の列。左がシートの見出し、右が画面に出す短い名前。
///
/// 🔴 左側は Python 側（Hubspot リポジトリ `scripts/sales_kpi/sync_daily.py`）と
/// 対で決まっている。片方だけ変えると全員 0 で並ぶ。
///
/// 見出しの「決定者の役職」に `の` が入っているのは、既に `KPI営業_架電リスト` の
/// 充足行がその表記だから（プロパティのラベルをそのまま使っている）。
/// 画面の表は列が7つ並んで横に長くなるので、ここで短い名前に読み替える。
pub const KETTEI_COLS: &[(&str, &str)] = &[
    ("決定者名", "決定者名"),
    ("決定者の役職", "決定者役職"),
    ("決裁者名", "決裁者名"),
    ("決裁者の役職", "決裁者役職"),
];

/// 決定者・決裁者シートを「いちばん新しい日」と「その1つ前の日」に絞ったもの。
///
/// シートは日付が違う行が積み上がる（キー = 日付 + ownerId）ので、
/// 全部を足すと同じ取引を日数ぶん数えることになる。**必ず1日を切り出して使う。**
#[derive(Debug, Default)]
pub struct KetteiDays {
    /// いちばん新しい日。行が1つも無ければ空。
    pub date: String,
    /// その1つ前の日。日が1つしか無ければ空。
    pub prev_date: String,
    /// `ownerId → {列: 件数}`。最新日ぶん。
    pub latest: BTreeMap<String, Counts>,
    /// 同じく前日ぶん。増加を出すためだけに使う。
    pub prev: BTreeMap<String, Counts>,
}

/// 1行から `{列: 件数}` を作る。
///
/// `合計` はシートの列をそのまま読む（シートが正）。列が無い・読めないときだけ
/// 4つを足して埋める。足し算で上書きしないのは、シート側が別の定義で合計を
/// 出すようになったときに、画面が黙って違う数を出さないようにするため。
fn kettei_counts(sheet: &SheetData, row: &[Arc<str>]) -> Counts {
    let num = |name: &str| -> i64 {
        sheet
            .get(row, name)
            .replace(',', "")
            .trim()
            .parse::<i64>()
            .unwrap_or(0)
    };
    let mut c = Counts::new();
    let mut sum = 0i64;
    for (col, key) in KETTEI_COLS {
        let n = num(col);
        c.insert((*key).to_string(), n);
        sum += n;
    }
    let total = sheet
        .get(row, "合計")
        .replace(',', "")
        .trim()
        .parse::<i64>()
        .unwrap_or(sum);
    c.insert("合計".into(), total);
    c
}

/// `KPI営業_決定者` から最新日と前日を切り出す。行が無ければ全部空。
///
/// 同じ日・同じ担当者の行が2つあれば足す（日次同期が二重に書いた場合の保険。
/// 落とすより足したほうが、取りこぼしに気づける）。
pub fn kettei_days_of(sheet: &SheetData) -> KetteiDays {
    let mut dates: Vec<&str> = sheet
        .rows
        .iter()
        .map(|r| sheet.get(r, "日付").trim())
        .filter(|d| !d.is_empty())
        .collect();
    dates.sort_unstable();
    dates.dedup();
    // 日付は `yyyy-MM-dd` のゼロ埋めなので辞書順が日付順になる。
    let date = dates.last().copied().unwrap_or("").to_string();
    let prev_date = if dates.len() >= 2 {
        dates[dates.len() - 2].to_string()
    } else {
        String::new()
    };

    let mut out = KetteiDays {
        date: date.clone(),
        prev_date: prev_date.clone(),
        ..Default::default()
    };
    for row in &sheet.rows {
        let d = sheet.get(row, "日付").trim();
        let bucket = if !date.is_empty() && d == date {
            &mut out.latest
        } else if !prev_date.is_empty() && d == prev_date {
            &mut out.prev
        } else {
            continue;
        };
        let owner = sheet.get(row, "ownerId").trim().to_string();
        let per = bucket.entry(owner).or_default();
        for (key, value) in kettei_counts(sheet, row) {
            *per.entry(key).or_insert(0) += value;
        }
    }
    out
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

/// 画面に出す取引1件。`days` / `anq` / `past` は無いときキーごと出さない（TS では `?`）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiDealRow")]
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
    #[ts(optional)]
    pub days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub anq: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub past: Option<bool>,
    /// HubSpot の取引ページ（object ID から作る。headless-crm-design §6）
    pub url: String,
}

/// HubSpot の取引ページ。コンサルKPI と同じ形で、portal は呼び出し側が 1 回だけ読む。
pub fn hubspot_deal_url(portal: &str, deal_id: &str) -> String {
    format!("https://app.hubspot.com/contacts/{portal}/record/0-3/{deal_id}/")
}

/// 「今月の成績」カードの件数キー 1 つ。**件数と内訳の行を同じ述語で作る**ための表（1 か所）。
/// `src` は行の出どころ（どのシート由来の配列か）、`pred` はその行がこのカードに入るか。
pub struct CardKey {
    pub key: &'static str,
    pub bpo_key: &'static str,
    pub src: CardSrc,
    pub pred: fn(&DealRow) -> bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardSrc {
    /// ③ 当月の母集団
    Pool,
    /// ① アポシートの全行
    Apo,
    /// ⑨ Cヨミシートの全行
    Cyomi,
}

fn p_all(_: &DealRow) -> bool {
    true
}
fn p_past(r: &DealRow) -> bool {
    r.kind != Kind::Upcoming.label()
}
fn p_past_anq(r: &DealRow) -> bool {
    p_past(r) && r.anq == Some(true)
}
fn p_done(r: &DealRow) -> bool {
    r.kind == Kind::Done.label()
}
fn p_not_done(r: &DealRow) -> bool {
    r.kind == Kind::NotDone.label()
}
fn p_stuck(r: &DealRow) -> bool {
    r.kind == Kind::Stuck.label()
}
fn p_upcoming(r: &DealRow) -> bool {
    r.kind == Kind::Upcoming.label()
}
fn p_unknown(r: &DealRow) -> bool {
    r.kind == Kind::Unknown.label()
}

/// カードの件数キーの表。④ = これから以外（日付では切らない）、⑤ の分子 = ④ のうちアンケートあり。
pub const CARD_KEYS: &[CardKey] = &[
    CardKey {
        key: "apo",
        bpo_key: "bpo_apo",
        src: CardSrc::Apo,
        pred: p_all,
    },
    CardKey {
        key: "pool",
        bpo_key: "bpo_pool",
        src: CardSrc::Pool,
        pred: p_all,
    },
    CardKey {
        key: "cyomi",
        bpo_key: "bpo_cyomi",
        src: CardSrc::Cyomi,
        pred: p_all,
    },
    CardKey {
        key: "実施",
        bpo_key: "bpo_実施",
        src: CardSrc::Pool,
        pred: p_done,
    },
    CardKey {
        key: "未実施",
        bpo_key: "bpo_未実施",
        src: CardSrc::Pool,
        pred: p_not_done,
    },
    CardKey {
        key: "未処理",
        bpo_key: "bpo_未処理",
        src: CardSrc::Pool,
        pred: p_stuck,
    },
    CardKey {
        key: "これから",
        bpo_key: "bpo_これから",
        src: CardSrc::Pool,
        pred: p_upcoming,
    },
    CardKey {
        key: "要判定",
        bpo_key: "bpo_要判定",
        src: CardSrc::Pool,
        pred: p_unknown,
    },
    CardKey {
        key: "anq_den",
        bpo_key: "bpo_anq_den",
        src: CardSrc::Pool,
        pred: p_past,
    },
    CardKey {
        key: "anq_num",
        bpo_key: "bpo_anq_num",
        src: CardSrc::Pool,
        pred: p_past_anq,
    },
];

pub fn deal_row(
    deal: &Deal,
    kind: Kind,
    why: String,
    members: &HashMap<String, Person>,
    bpo: bool,
    portal: &str,
) -> DealRow {
    let person = person_of(members, &deal.owner);
    DealRow {
        id: deal.id.clone(),
        name: if deal.name.is_empty() {
            "（取引名なし）".into()
        } else {
            deal.name.clone()
        },
        date: deal.date().to_string(),
        time: if deal.jikan.is_empty() {
            deal.scheduled.get(11..).unwrap_or("").to_string()
        } else {
            deal.jikan.clone()
        },
        owner: deal.owner.clone(),
        owner_name: person.name,
        team: person.team,
        bpo,
        kind: kind.label(),
        why,
        days: None,
        anq: None,
        past: None,
        url: hubspot_deal_url(portal, &deal.id),
    }
}

/// 架電1行。
#[derive(Debug, Clone)]
pub struct KadenRow {
    pub date: String,
    pub owner: String,
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
            let num = |name: &str| {
                sheet
                    .get(r, name)
                    .replace(',', "")
                    .parse::<i64>()
                    .unwrap_or(0)
            };
            KadenRow {
                date: sheet.get(r, "日付").to_string(),
                owner: sheet.get(r, "ownerId").to_string(),
                dept: sheet.get(r, "部署").to_string(),
                calls: num("発信"),
                connected: num("架電数"),
                long: num("5分超"),
            }
        })
        .collect()
}

/// 期間を切って、チーム別・個人別に足す。
#[derive(Debug, Default, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKadenPeriod")]
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
            let team = person_of(members, &row.owner).team;
            *out.by_team
                .entry(team)
                .or_default()
                .entry(key.to_string())
                .or_insert(0) += value;
            *out.by_person
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
