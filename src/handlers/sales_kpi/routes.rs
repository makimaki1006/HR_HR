//! 営業KPI: ルーティングと集計の組み立て
//!
//! パスの規約は架電クオリティに揃える（`/api/<領域>/<資源>`・ハイフン区切り）。
//!   ページ `/sales-kpi`
//!   API    `/api/sales-kpi/data`
//!
//! サーバは画面の見た目を組み立てない。返すのは集計済みの素の JSON だけで、
//! グラフや表の組み立てはクライアント側に置く（架電クオリティと同じ方針）。
//!
//! 常駐リソース（SheetsClient / SheetStore）は架電クオリティのものを borrow する。
//! **同じスプレッドシートなので、別に持つとキャッシュが二重になって
//! Sheets を無駄に2回叩く**。

use std::collections::{BTreeMap, HashMap, HashSet};

use askama::Template;
use axum::extract::Query;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tower_sessions::Session;

use crate::handlers::call_quality::routes::{cq_state, CqError};
use crate::AppState;
use crate::SESSION_USER_KEY;

use super::{
    classify, deal_row, deals_of, is_bpo, kaden_by_owner_of, kaden_of, kaden_period, load,
    members_of, person_of, snapshots_of, Counts, Deal, DealRow, Kind, Person, Sheets,
    KADEN_CLASSES, SHEET_META,
};

/// 日本時間。サーバのタイムゾーン設定に依存させない。
fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("JST")
}

fn today_jst() -> NaiveDate {
    Utc::now().with_timezone(&jst()).date_naive()
}

/// `yyyy-MM-dd 00:00`。シートの日時と辞書順で比べるための形。
fn at_midnight(date: NaiveDate) -> String {
    format!("{date} 00:00")
}

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn month_first(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

fn next_month_first(date: NaiveDate) -> NaiveDate {
    let (y, m) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(date)
}

fn prev_month_first(date: NaiveDate) -> NaiveDate {
    let first = month_first(date);
    month_first(first - Duration::days(1))
}

/// 月曜はじまりの週頭。
fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

fn days_between(from: NaiveDate, to: NaiveDate) -> Vec<String> {
    let mut out = Vec::new();
    let mut d = from;
    while d < to {
        out.push(ymd(d));
        d += Duration::days(1);
    }
    out
}

pub fn router() -> Router<std::sync::Arc<AppState>> {
    Router::new()
        .route("/sales-kpi", get(page))
        .route("/api/sales-kpi/data", get(data))
}

#[derive(Template)]
#[template(path = "tabs/sales_kpi.html")]
struct SalesKpiTemplate {
    user: String,
}

async fn page(session: Session) -> Result<Html<String>, CqError> {
    let user: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    SalesKpiTemplate { user }.render().map(Html).map_err(|e| {
        CqError::from_anyhow("sales-kpi", anyhow::anyhow!("画面の組み立てに失敗: {e}"))
    })
}

#[derive(Debug, Deserialize)]
struct DataQuery {
    /// `1` を渡すとキャッシュを捨てて読み直す。
    refresh: Option<String>,
}

/// 画面がそのまま使える形の JSON を1本で返す。
///
/// 分けて何本も叩かせないのは、どの数字も同じ日の同じシートから作らないと
/// 画面の中で食い違うため（「取ったアポ」と「やった商談」が別時点になる）。
async fn data(Query(q): Query<DataQuery>, session: Session) -> Result<Response, CqError> {
    let _ = session;
    let state = cq_state()?;
    if q.refresh.as_deref() == Some("1") {
        for name in [
            super::SHEET_SHODAN,
            super::SHEET_APO,
            super::SHEET_CYOMI,
            super::SHEET_KADEN,
            super::SHEET_KADEN_LIST,
            super::SHEET_KADEN_BY_OWNER,
            super::SHEET_MEMBER,
            SHEET_META,
            super::SHEET_WEEKLY,
        ] {
            state.store.invalidate(Some(name)).await;
        }
    }
    let sheets = load(&state.client, &state.store)
        .await
        .map_err(|e| CqError::from_anyhow("sales-kpi", e))?;

    Ok(Json(build_payload(&sheets, today_jst())).into_response())
}

/// シートと「今日」から、画面がそのまま使える JSON を作る。
///
/// `data()` から切り出してあるのは、**実データのシートを読み込んで
/// Python 版（これまでの画面）と突き合わせるテストを書くため**。
/// 今日を引数で受けるので、過去の日付でも同じ結果を再現できる。
pub fn build_payload(sheets: &Sheets, today: NaiveDate) -> Value {
    let members = members_of(&sheets.member);
    let cutoff = at_midnight(today);
    let month_lo = at_midnight(month_first(today));
    let month_hi = at_midnight(next_month_first(today));
    let prev_month_lo = at_midnight(prev_month_first(today));
    let wk = week_start(today);
    let week_lo = at_midnight(wk);
    let week_hi = at_midnight(wk + Duration::days(7));
    let next_hi = at_midnight(wk + Duration::days(14));
    let stale_from = at_midnight(today - Duration::days(super::STALE_DAYS));

    // 商談の集計から外す相手。**条件はここに書かない。**
    // `KPI営業_メンバー` の `集計対象` 列（＝運用シート `KPI営業_集計除外` の写し）
    // だけを見る。列が無い古いシートでは全員 true ＝ これまでどおり全員数える。
    //
    // 🔴 落とすのは**入口で1回だけ**。①③②⑥⑨ のカードだけでなく、
    //    ⑦止まっている・⑤アンケート未回収・今週/来週の一覧も同じ材料から作るので、
    //    ここで落とさないと画面の中で数え方が2つになる。週次シートを書く
    //    Python 側（`weekly_cells()`）も同じ入口で落としている。
    // 🔴 外すのは商談だけ。架電と架電リストは外さない（コンサル営業も架電している）。
    let mut dropped: Counts = Counts::new();
    let mut keep_counted = |deals: Vec<Deal>| -> Vec<Deal> {
        deals
            .into_iter()
            .filter(|d| {
                let p = person_of(&members, &d.owner);
                if p.counted {
                    return true;
                }
                *dropped.entry("件数".into()).or_insert(0) += 1;
                let label = if p.hs_team.is_empty() {
                    "（所属なし）".to_string()
                } else {
                    p.hs_team.clone()
                };
                *dropped.entry(label).or_insert(0) += 1;
                false
            })
            .collect()
    };
    let all = keep_counted(deals_of(&sheets.shodan));
    let apo_deals = keep_counted(deals_of(&sheets.apo));
    let cyomi_deals = keep_counted(deals_of(&sheets.cyomi));
    let bpo_of = |d: &Deal| is_bpo(d, &prev_month_lo, &month_hi);

    // ---- 当月の母集団を仕分ける ----------------------------------------
    let mut by_team: BTreeMap<String, Counts> = BTreeMap::new();
    let mut by_person: BTreeMap<String, Counts> = BTreeMap::new();
    let mut bpo_total: Counts = Counts::new();
    let mut people: HashMap<String, Person> = HashMap::new();

    let mut add = |team: &str, owner: &str, key: &str| {
        *by_team
            .entry(team.to_string())
            .or_default()
            .entry(key.to_string())
            .or_insert(0) += 1;
        *by_person
            .entry(owner.to_string())
            .or_default()
            .entry(key.to_string())
            .or_insert(0) += 1;
    };

    let month: Vec<&Deal> = all
        .iter()
        .filter(|d| {
            d.scheduled.as_str() >= month_lo.as_str() && d.scheduled.as_str() < month_hi.as_str()
        })
        .collect();

    for deal in &month {
        let team = note(&mut people, &members, &deal.owner);

        let (kind, _) = classify(deal, &cutoff);
        let bpo = bpo_of(deal);
        add(&team, &deal.owner, "pool");
        add(&team, &deal.owner, kind.label());
        if bpo {
            add(&team, &deal.owner, "bpo_pool");
            add(&team, &deal.owner, &format!("bpo_{}", kind.label()));
            *bpo_total.entry("pool".into()).or_insert(0) += 1;
            *bpo_total.entry(kind.label().into()).or_insert(0) += 1;
        }

        // ⑤ アンケートの分母は **④「日が過ぎた分」と同じ**にする（2026-09-10 ユーザー指示）。
        //    ＝ これから以外（実施・未実施・未処理・要判定）。
        //
        //    以前は「商談予定日時 < 今日」という**日付**で切っていた。だが日付で切ると、
        //    **予定日はまだ先なのに、もう実施した／やらないと決まったもの**が分母から漏れる。
        //    2026-09-10 実測で48件（実施32・未実施16、予定日 9/10〜9/30、うち20件は回収済み）。
        //    これらはもう回収する時間が無いので、分母に入れるのが正しい。
        //
        //    🔴 「これから」を分母に入れてはいけない。まだ回収する時間があるものまで
        //       「未回収」に見えてしまう（2026-09-04 ユーザー指示。こちらは今も有効）。
        if kind != Kind::Upcoming {
            add(&team, &deal.owner, "anq_den");
            if bpo {
                add(&team, &deal.owner, "bpo_anq_den");
            }
            if deal.has_survey {
                add(&team, &deal.owner, "anq_num");
                if bpo {
                    add(&team, &deal.owner, "bpo_anq_num");
                }
            }
        }
    }

    // ---- ① 取ったアポ --------------------------------------------------
    for deal in apo_deals {
        let team = note(&mut people, &members, &deal.owner);
        add(&team, &deal.owner, "apo");
        // ① は当月に確定したアポなので、BPO 判定も当月の取得日に限る
        if is_bpo(&deal, &month_lo, &month_hi) {
            add(&team, &deal.owner, "bpo_apo");
        }
    }

    // ---- ⑨ Cヨミ --------------------------------------------------------
    let mut cyomi_stale: Vec<DealRow> = Vec::new();
    for deal in cyomi_deals {
        let team = note(&mut people, &members, &deal.owner);
        add(&team, &deal.owner, "cyomi");
        if bpo_of(&deal) {
            add(&team, &deal.owner, "bpo_cyomi");
        }
        if let Some(days) = days_since(&deal.entered_c, today) {
            if days >= super::CYOMI_STALE_DAYS {
                add(&team, &deal.owner, "cyomi_stale");
                let mut row = deal_row(
                    &deal,
                    Kind::Unknown,
                    "Cヨミのまま".into(),
                    &members,
                    bpo_of(&deal),
                );
                row.days = Some(days);
                cyomi_stale.push(row);
            }
        }
    }
    cyomi_stale.sort_by(|a, b| b.days.cmp(&a.days));

    // ---- 止まっている取引（予定日を過ぎてアポ日確定のまま）--------------
    let mut stale: Vec<DealRow> = all
        .iter()
        .filter(|d| {
            (d.stage == super::ST_APO || d.stage == super::ST_APO_BPO)
                && d.scheduled.as_str() >= stale_from.as_str()
                && d.scheduled.as_str() < cutoff.as_str()
        })
        .map(|d| {
            deal_row(
                d,
                Kind::Stuck,
                "アポ日確定のまま".into(),
                &members,
                bpo_of(d),
            )
        })
        .collect();
    stale.sort_by(|a, b| a.date.cmp(&b.date));

    // ---- 今週・来週 ------------------------------------------------------
    let week_rows = |lo: &str, hi: &str| -> Vec<DealRow> {
        let mut rows: Vec<DealRow> = all
            .iter()
            .filter(|d| d.scheduled.as_str() >= lo && d.scheduled.as_str() < hi)
            .map(|d| {
                let (kind, why) = classify(d, &cutoff);
                let mut row = deal_row(d, kind, why, &members, bpo_of(d));
                row.past = Some(d.scheduled.as_str() < cutoff.as_str());
                row.anq = Some(d.has_survey);
                row
            })
            .collect();
        rows.sort_by(|a, b| {
            (a.date.as_str(), a.time.as_str()).cmp(&(b.date.as_str(), b.time.as_str()))
        });
        rows
    };
    let this_week = week_rows(&week_lo, &week_hi);
    let next_week = week_rows(&week_hi, &next_hi);

    // アンケート未回収は「これから商談があるのに、まだアンケートが無い」もの
    let anq_missing: Vec<Value> = this_week
        .iter()
        .chain(next_week.iter())
        .filter(|r| r.past == Some(false) && r.anq == Some(false))
        .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        .collect();

    // ---- 架電リストの状態 -------------------------------------------------
    let (mut kaden_block, kaden_base) =
        kaden_list_block(&sheets.kaden_list, &sheets.kaden_by_owner, &members);
    // 母数がどれだけ動いたか。週次シートは読むだけ（書くのは Python 側）。
    if let Some(obj) = kaden_block.as_object_mut() {
        obj.insert(
            "base_trend".into(),
            kaden_base_trend(&sheets.weekly, kaden_base, &ymd(wk)),
        );
    }

    // 架電リストだけに出てくる担当者も個人プルダウンに載せる。
    // 🔴 載せないと「そのチームの合計は出るのに、中の誰も選べない」ことが起きる。
    // 実際 65名中27名が商談・アポ・Cヨミのどれにも出てこない（BPO が中心。2026-09-07 実測）。
    // 今月の商談が無い人なので、成績のカードは 0 で並ぶ。それは事実なのでそのまま出す。
    for row in &sheets.kaden_by_owner.rows {
        let owner = sheets.kaden_by_owner.get(row, "ownerId");
        if !owner.is_empty() {
            note(&mut people, &members, owner);
        }
    }

    // ---- 取得条件 ---------------------------------------------------------
    // 架電より先に読む。「架電の最終日が途中かどうか」は取得条件に入っている。
    let mut meta: BTreeMap<String, String> = BTreeMap::new();
    for row in &sheets.meta.rows {
        meta.insert(
            sheets.meta.get(row, "項目").to_string(),
            sheets.meta.get(row, "値").to_string(),
        );
    }

    // ---- Zoom の架電 ------------------------------------------------------
    //
    // 🔴 週・月の区切りは **実際の today** で決める。シートの最終日で決めてはいけない。
    //    日次同期は前日ぶんを朝に書くので、月曜の朝はシートの最終日が日曜（＝先週）
    //    になる。そこを週頭にすると「今週」として先週が丸ごと出る。
    //    実際に 2026-09-07（月）の本番で、今週として 8/31〜9/06 が出ていた。
    //    その週にまだ行が無ければ 0件と正直に出す。先週を今週と偽らない。
    let kaden_rows = kaden_of(&sheets.kaden);
    // Zoom で架電した人も控える。`people` に入れておかないと、人別の架電表が
    // 名前を引けずに `owner_96437217` と出る（2026-09-07 現場指摘の直し残り）。
    // ここまでで `people` は「この画面に数字が出る人 = 商談 ∪ 架電リスト ∪ Zoom架電」になる。
    for row in &kaden_rows {
        if !row.owner.is_empty() {
            note(&mut people, &members, &row.owner);
        }
    }
    let have_days: HashSet<&str> = kaden_rows.iter().map(|r| r.date.as_str()).collect();
    let last_day = kaden_rows
        .iter()
        .map(|r| r.date.as_str())
        .max()
        .unwrap_or("")
        .to_string();
    // 最終日がまだ途中か。日次同期が「架電の最終日／その日は途中か」を取得条件に
    // 書くので、それが同じ日について言っているならそれを使う。
    // 無ければ「最終日＝今日なら途中」と見なす（今日はまだ終わっていない）。
    let kaden_partial = match meta.get("架電の最終日") {
        Some(d) if *d == last_day => meta.get("架電の最終日は途中").map(|v| v == "はい"),
        _ => None,
    }
    .unwrap_or(!last_day.is_empty() && last_day == ymd(today));

    // 商談の「今週」と同じ週頭を使う（wk = week_start(today)）。
    // 商談と架電で週がずれていると、画面の中で今週の意味が2つになる。
    let this_week_days: Vec<String> = days_between(wk, today + Duration::days(1))
        .into_iter()
        .filter(|d| have_days.contains(d.as_str()))
        .collect();
    let prev_week_days: Vec<String> = days_between(wk - Duration::days(7), wk);
    // 先週の「同じところまで」。件数で頭から取るのではなく、今週ぶんの各日を
    // そのまま7日ずらす。今週の途中に行が無い日があっても曜日がずれない。
    let prev_same: Vec<String> = this_week_days
        .iter()
        .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .map(|d| ymd(d - Duration::days(7)))
        .collect();
    let month_days: Vec<String> = days_between(month_first(today), today + Duration::days(1))
        .into_iter()
        .filter(|d| have_days.contains(d.as_str()))
        .collect();

    let mut daily: Vec<Value> = Vec::new();
    let mut per_day: BTreeMap<&str, (i64, i64, i64)> = BTreeMap::new();
    for row in &kaden_rows {
        let e = per_day.entry(row.date.as_str()).or_insert((0, 0, 0));
        e.0 += row.calls;
        e.1 += row.connected;
        e.2 += row.long;
    }
    for (date, (calls, connected, long)) in &per_day {
        daily.push(json!({"date": date, "calls": calls, "connected": connected, "long": long}));
    }

    let mut unmatched_by_dept: BTreeMap<String, i64> = BTreeMap::new();
    for row in kaden_rows.iter().filter(|r| r.owner.is_empty()) {
        *unmatched_by_dept
            .entry(if row.dept.is_empty() {
                "(不明)".into()
            } else {
                row.dept.clone()
            })
            .or_insert(0) += row.calls;
    }
    let mut unmatched: Vec<(String, i64)> = unmatched_by_dept.into_iter().collect();
    unmatched.sort_by(|a, b| b.1.cmp(&a.1));

    let calls = json!({
        "generated_at": last_day.clone(),
        // シートに入っている最後の日と、その日がまだ途中かどうか。
        // 画面はこれを見て「集計中」と出せる。
        "last_day": last_day.clone(),
        "last_day_partial": kaden_partial,
        // いつ Zoom から取ったか。当日は次の同期まで動かないので、
        // 「何時時点の数か」を出さないと、夕方に見た人が朝の数を今の数だと思う。
        // 2026-09-08 に実際そうなっていた（画面 22件・実数 10,410件）。
        "fetched_at": meta.get("架電の取得時刻").cloned().unwrap_or_default(),
        "rule": {
            "calls": "Zoomの通話ログのうち direction=outbound を1件と数える",
            "connected": "result が Auto Recorded のもの。現場が「架電数」と呼んでいるのはこの数",
            "long": "通話 300 秒超。過去分析でアポ獲得との相関が高かった指標",
            "join": "call_logs に caller_email が無いため Zoomユーザーのメール → HubSpot担当者のメールで紐づけ",
        },
        "periods": {
            // 🔴 today はシートの最終日ではなく実際の今日。今日の行がまだ無ければ
            //    空（0件）で返す。無い日を「今日」として出すと画面が嘘をつく。
            "today": kaden_period(&kaden_rows, &[ymd(today)], &members),
            "yesterday": kaden_period(&kaden_rows, &[ymd(today - Duration::days(1))], &members),
            "this_week": kaden_period(&kaden_rows, &this_week_days, &members),
            "prev_week_same": kaden_period(&kaden_rows, &prev_same, &members),
            "prev_week": kaden_period(&kaden_rows, &prev_week_days, &members),
            "this_month": kaden_period(&kaden_rows, &month_days, &members),
        },
        "daily": daily,
        "people": people_list(&people),
        "unmatched_by_dept": unmatched.into_iter().collect::<BTreeMap<_, _>>(),
    });

    let teams: Vec<String> = by_team.keys().cloned().collect();
    let body = json!({
        "generated_at": meta.get("取得時刻").cloned().unwrap_or_else(|| ymd(today)),
        "week": {"start": ymd(wk), "end": ymd(wk + Duration::days(6))},
        "next_week": {"start": ymd(wk + Duration::days(7)), "end": ymd(wk + Duration::days(13))},
        "stale_days": super::STALE_DAYS,
        "bpo_rule": "BPOアポ取得日が当月または前月にあるものをBPO経由とする。このプロパティは過去のBPOアポの日付が残り続けるため、値の有無では判定できない（現場指摘）",
        "teams": teams,
        "by_team": by_team,
        "by_person": by_person,
        "people": people_list(&people),
        "bpo_total": bpo_total,
        "stale": stale,
        "week_deals": this_week,
        "next_week_deals": next_week,
        "anq_missing": anq_missing,
        "cyomi_stale": cyomi_stale,
        // 商談の集計から外した件数。内訳は HubSpotチーム 別。
        // 🔴 チーム名はシート（KPI営業_集計除外）由来で、ここには書かれていない。
        "excluded": dropped,
        "kaden": kaden_block,
        "kaden_base": kaden_base,
        "calls": calls,
        // 週に1行の記録。Python の日次同期（Hubspot リポジトリ
        // `scripts/sales_kpi/sync_daily.py` の `sync_weekly()`）が
        // KPI営業_週次 へその週の行を上書きする。まだ1度も書かれていなければ空配列。
        "snapshots": snapshots_of(&sheets.weekly),
        "meta": meta,
        "from_cache": sheets.all_cached,
    });

    body
}

/// 取引に出てきた担当者を控えて、その人のチーム名を返す。
/// 名簿にも HubSpot にも居なければ `owner_<id>` のまま「チーム未設定」。
fn note(
    people: &mut HashMap<String, Person>,
    members: &HashMap<String, Person>,
    owner: &str,
) -> String {
    people
        .entry(owner.to_string())
        .or_insert_with(|| person_of(members, owner))
        .team
        .clone()
}

fn people_list(people: &HashMap<String, Person>) -> Vec<&Person> {
    let mut v: Vec<&Person> = people.values().collect();
    v.sort_by(|a, b| (a.team.as_str(), a.name.as_str()).cmp(&(b.team.as_str(), b.name.as_str())));
    v
}

/// `yyyy-MM-dd HH:mm` から今日までの日数。空・壊れていれば None。
fn days_since(text: &str, today: NaiveDate) -> Option<i64> {
    let date = NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()?;
    Some((today - date).num_days())
}

/// 架電リストの母数が、前の週の記録からどれだけ動いたか。無ければ `Null`。
///
/// 🔴 **母数は毎月大きく動く。異常ではなくリストマネジメントの正常な運用**
/// （2026-09-07 ユーザー確認）。アポ前リストと BPO リストの間でまとまった件数が
/// 行き来している。実測では 09-01 に BPO→アポ前 7,360件、09-02 に アポ前→BPO
/// 6,663件、09-03 に新ステージへ 1,825件。母数は 136,518 → 129,790 と動いた。
/// これを画面に出さないと、「手をつけた割合」が動いたのを見た人が
/// 「先週より進んだ／戻った」と読む。実際には母数の入れ替えで動いただけ、
/// ということが起きる。
///
/// 比べる相手は「今週ではない、いちばん新しい記録」。今週の行は今日と同じ材料から
/// 書かれているので、それと比べても 0 にしかならない。
///
/// 🔴 増減の**理由**までは出さない。この材料（週ごとの母数）だけでは
/// 「BPO へ払い出したから減った」のか「ステージ構成が変わったから」なのかを
/// 判定できない。画面には動いた事実だけを出して、断定しない。
fn kaden_base_trend(
    weekly: &crate::handlers::call_quality::sheets::SheetData,
    base: i64,
    this_week_start: &str,
) -> Value {
    let prev = snapshots_of(weekly)
        .into_iter()
        .filter(|s| {
            s["kaden_base"].as_i64().unwrap_or(0) > 0
                && s["week_start"].as_str().unwrap_or("") != this_week_start
        })
        // snapshots_of は週の昇順。最後が「今週ではない、いちばん新しい記録」。
        .next_back();
    match prev {
        Some(s) => json!({
            "week": s["week"],
            "week_start": s["week_start"],
            "base": s["kaden_base"],
            "diff": base - s["kaden_base"].as_i64().unwrap_or(0),
        }),
        None => Value::Null,
    }
}

/// 架電リストの状態（未架電／未接触／接触済み）と入力状況をまとめる。
///
/// 全社の数字は `KPI営業_架電リスト` から、チーム別・個人別は
/// `KPI営業_架電リスト_担当別` から作る。担当者別のシートが無ければ
/// `by_person` / `by_team` は空で返し、画面は全社の数字だけを出す。
///
/// 🔴 **チームの合計は全社の合計にならない**。担当者が入っていない取引が
/// 7,337件あり（2026-09-07 実測。12万件の 5.7%）、どのチームにも属さないため。
/// そのぶんは `no_owner` に出して、画面が黙って落とさないようにする。
fn kaden_list_block(
    sheet: &crate::handlers::call_quality::sheets::SheetData,
    by_owner_sheet: &crate::handlers::call_quality::sheets::SheetData,
    members: &HashMap<String, Person>,
) -> (Value, i64) {
    let mut composition = Vec::new();
    let mut cls: BTreeMap<String, i64> = BTreeMap::new();
    let mut fill: BTreeMap<String, i64> = BTreeMap::new();
    let mut total = 0i64;
    for row in &sheet.rows {
        let kind = sheet.get(row, "区分");
        let name = sheet.get(row, "名前");
        let group = sheet.get(row, "分類");
        let count = sheet
            .get(row, "件数")
            .replace(',', "")
            .parse::<i64>()
            .unwrap_or(0);
        match kind {
            "ステージ" => {
                composition.push(json!({"stage": name, "cls": group, "count": count}));
                *cls.entry(group.to_string()).or_insert(0) += count;
            }
            "合計" => total = count,
            "充足" => {
                fill.insert(name.to_string(), count);
            }
            _ => {}
        }
    }
    let base: i64 = KADEN_CLASSES
        .iter()
        .map(|k| cls.get(*k).copied().unwrap_or(0))
        .sum();

    // ---- 担当者別 ----
    let by_person = kaden_by_owner_of(by_owner_sheet);
    let mut by_team: BTreeMap<String, Counts> = BTreeMap::new();
    let mut no_owner: Counts = Counts::new();
    let mut counted: i64 = 0;
    for (owner, counts) in &by_person {
        counted += counts.get("base").copied().unwrap_or(0);
        let bucket = if owner.is_empty() {
            &mut no_owner
        } else {
            by_team
                .entry(person_of(members, owner).team)
                .or_default()
        };
        for (key, value) in counts {
            *bucket.entry(key.clone()).or_insert(0) += value;
        }
    }
    // 担当なしは by_person からも外す。画面の個人プルダウンに空の項目を出さない。
    let by_person: BTreeMap<String, Counts> = by_person
        .into_iter()
        .filter(|(owner, _)| !owner.is_empty())
        .collect();

    // ---- 「全社」＝ 営業チームの合計にする ------------------------------
    //
    // 🔴 2026-09-08 ユーザー判断。アポ前パイプライン全体で数えると
    // 「74.5% が未着手」になるが、これは名簿に載っていない人が持っている在庫
    // （永田さん 70,176件 ほか）に引きずられた数字だった。営業5チームだけで
    // 数えると 72.8% が着手済みになる。現場が見たいのは後者。
    //
    // 🔴 チーム名は列挙しない。**名簿にチームが入っているか**だけで判定する
    // （`is_sales_team`）。チームが増えても名簿に足すだけで数えられる。
    let mut sales_cls: Counts = Counts::new();
    let mut unassigned_cls: Counts = Counts::new();
    for (team, counts) in &by_team {
        let bucket = if super::is_sales_team(team) {
            &mut sales_cls
        } else {
            &mut unassigned_cls
        };
        for (key, value) in counts {
            *bucket.entry(key.clone()).or_insert(0) += value;
        }
    }
    // 担当者が入っていない取引も「まだ配られていない」側。
    for (key, value) in &no_owner {
        *unassigned_cls.entry(key.clone()).or_insert(0) += value;
    }
    let sum_of = |c: &Counts| -> i64 {
        KADEN_CLASSES
            .iter()
            .map(|k| c.get(*k).copied().unwrap_or(0))
            .sum()
    };
    let sales_base = sum_of(&sales_cls);
    let unassigned_base = sum_of(&unassigned_cls);

    // まだ配られていない分を、誰が持っているかまで出す。配る判断に使うため。
    let mut stock: Vec<Value> = by_person
        .iter()
        .filter(|(owner, _)| !super::is_sales_team(&person_of(members, owner).team))
        .map(|(owner, counts)| {
            let p = person_of(members, owner);
            json!({
                "id": owner, "name": p.name, "team": p.team, "hsTeam": p.hs_team,
                "base": counts.get("base").copied().unwrap_or(0),
                "未架電": counts.get("未架電").copied().unwrap_or(0),
                "未接触": counts.get("未接触").copied().unwrap_or(0),
                "接触済み": counts.get("接触済み").copied().unwrap_or(0),
            })
        })
        .filter(|v| v["base"].as_i64().unwrap_or(0) > 0)
        .collect();
    stock.sort_by_key(|v| -v["base"].as_i64().unwrap_or(0));

    // 担当者別シートがまだ無い環境では分けようがない。従来どおり全体を出す。
    let have = !by_owner_sheet.rows.is_empty();

    (
        json!({
            "composition": composition,
            // 画面のカードが使う「全社」。営業チームの合計。
            "base": if have { sales_base } else { base },
            "cls": if have { sales_cls } else { cls.clone() },
            "total": total,
            "fill": fill,
            // アポ前パイプライン全体（従来の「全社」）。注記と母数の推移に使う。
            // 🔴 週次シートの `kaden_base` はこちらの数え方なので、
            //    前の週との比較はこちらと突き合わせないと桁が合わない。
            "all": {"cls": cls, "base": base},
            // まだ営業チームに配られていない分。合計と、誰が持っているか。
            "unassigned": {
                "cls": unassigned_cls,
                "base": unassigned_base,
                "no_owner": no_owner.get("base").copied().unwrap_or(0),
                "people": stock,
            },
            // どちらも担当なしを含まない。合計は必ず一致する。
            "by_person": by_person,
            "by_team": by_team,
            // 担当者が入っていない取引。どのチームにも個人にも入らない。
            "no_owner": no_owner,
            // 数えられた母数の合計（= by_person の合計 ＋ no_owner）。
            // 全社の `base` との差は「今回数えていない担当者ぶん」か
            // 「数えている間にステージが動いたぶん」。画面はこの差を出す。
            "counted_base": counted,
            "has_by_owner": have,
        }),
        base,
    )
}
