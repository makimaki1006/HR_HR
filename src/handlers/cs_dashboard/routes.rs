//! コンサルダッシュボード: ルーティングと集計の組み立て
//!
//! 🔴 既存の `handlers::consult`（商談準備レポート `/consult/brief`）とは**別物**。
//!    名前が似ているだけで、見る人もデータも違う。
//!
//! パスの規約は架電クオリティ・営業KPI に揃える（`/api/<領域>/<資源>`・
//! ハイフン区切り）。
//!   ページ `/consulting`
//!   API    `/api/consulting/renewal`
//!
//! サーバは画面の見た目（グラフの option）を組み立てない。返すのは
//! **集計済みの素の JSON** だけ。架電クオリティと同じ方針。
//!
//! ------------------------------------------------------------------
//! この画面が守る規律（モックから引き継ぐ）
//! ------------------------------------------------------------------
//! 1. **分母0の率は null。0% と書かない。** `tabs::rate()` を使う
//! 2. **母数(n)を全ての数字に添える。** 代表値は非空の値だけで作り、その n を返す
//! 3. **右側打ち切りを切り替えられる。** 直近契約は結果が確定しておらず低く出る
//!    （除くと応募数の中央値が 3→11 と変わる）
//! 4. **欠測の偏りを画面に出す。** うまくいかなかった契約ほど数字が
//!    記録されていない可能性があり、相関を押し上げる方向に効く。隠すと判断を誤る
//!
//! ------------------------------------------------------------------
//! 四分位のとり方
//! ------------------------------------------------------------------
//! モック（Python）と同じにしてある。`q(p) = 昇順[min(floor(n*p), n-1)]` の
//! nearest-rank、中央値だけ偶数個なら中2つの平均（`statistics.median`）。
//! 補間する方式に変えると**モックと数字が合わなくなる**ので、変えるときは
//! 両方そろえること。

use std::collections::{BTreeMap, HashMap, HashSet};

use askama::Template;
use axum::extract::Query;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_sessions::Session;

use crate::handlers::call_quality::routes::{cq_state, CqError};
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::call_quality::tabs::rate;
use crate::AppState;
use crate::SESSION_USER_KEY;

use super::{
    consultant_of, contacts_by_deal, cpa, customers_of, date10, deals_of, focus_of, latest_nps,
    load, opt_num, Deal, Outcome, Sheets,
};

pub fn router() -> Router<std::sync::Arc<AppState>> {
    Router::new()
        .route("/consulting", get(page))
        .route("/api/consulting/renewal", get(renewal))
        .route("/api/consulting/outcome", get(outcome))
        .route("/api/consulting/focus", get(focus))
        .route("/api/consulting/rampup", get(rampup))
        .route("/api/consulting/phone", get(phone))
        .route("/api/consulting/headquarters", get(headquarters))
        .route("/api/consulting/mtg-quality", get(mtg_quality))
        .route("/api/consulting/data-quality", get(data_quality))
        .route("/api/consulting/customer", get(customer_detail))
        .route("/api/consulting/consultants", get(consultants))
        .route("/api/consulting/handover", get(handover))
        .route("/api/consulting/deals", get(deal_board))
        .route("/api/consulting/today", get(today_board))
}

#[derive(Template)]
#[template(path = "tabs/cs_dashboard.html")]
struct ConsultingTemplate {
    user: String,
}

async fn page(session: Session) -> Result<Html<String>, CqError> {
    let user: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    ConsultingTemplate { user }.render().map(Html).map_err(|e| {
        CqError::from_anyhow("consulting", anyhow::anyhow!("画面の組み立てに失敗: {e}"))
    })
}

#[derive(Debug, Deserialize)]
struct RenewalQuery {
    /// `1` で右側打ち切り（結果が確定していない直近契約）を代表値から外す。
    exclude_right_censored: Option<String>,
    /// `1` でキャッシュを捨てて読み直す。
    refresh: Option<String>,
}

async fn renewal(Query(q): Query<RenewalQuery>, session: Session) -> Result<Response, CqError> {
    let _ = session;
    let state = cq_state()?;
    if q.refresh.as_deref() == Some("1") {
        state.store.invalidate(Some(super::SHEET_DEAL)).await;
    }
    let sheets = load(&state.client, &state.store)
        .await
        .map_err(|e| CqError::from_anyhow("consulting", e))?;
    let excl = q.exclude_right_censored.as_deref() == Some("1");
    Ok(Json(freshen(build_renewal(&sheets, excl), &sheets, today_jst())).into_response())
}

#[derive(Debug, Deserialize)]
struct OutcomeQuery {
    refresh: Option<String>,
}

async fn outcome(Query(q): Query<OutcomeQuery>, session: Session) -> Result<Response, CqError> {
    let _ = session;
    let state = cq_state()?;
    if q.refresh.as_deref() == Some("1") {
        for name in [super::SHEET_DEAL, super::SHEET_CALL, super::SHEET_MTG] {
            state.store.invalidate(Some(name)).await;
        }
    }
    let sheets = load(&state.client, &state.store)
        .await
        .map_err(|e| CqError::from_anyhow("consulting", e))?;
    Ok(Json(freshen(
        build_outcome(&sheets, today_jst()),
        &sheets,
        today_jst(),
    ))
    .into_response())
}

/// 日本時間の今日。サーバのタイムゾーン設定に依存させない。
fn today_jst() -> NaiveDate {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
        .date_naive()
}

#[derive(Debug, Deserialize)]
struct FocusQuery {
    refresh: Option<String>,
}

async fn focus(Query(q): Query<FocusQuery>, session: Session) -> Result<Response, CqError> {
    let _ = session;
    let state = cq_state()?;
    if q.refresh.as_deref() == Some("1") {
        for name in [
            super::SHEET_DEAL,
            super::SHEET_CALL,
            super::SHEET_MTG,
            super::SHEET_HISTORY,
            super::SHEET_CUSTOMER,
            super::SHEET_MAIL_MTG,
        ] {
            state.store.invalidate(Some(name)).await;
        }
    }
    let sheets = load(&state.client, &state.store)
        .await
        .map_err(|e| CqError::from_anyhow("consulting", e))?;
    Ok(Json(freshen(
        build_focus(&sheets, today_jst()),
        &sheets,
        today_jst(),
    ))
    .into_response())
}

macro_rules! simple_handler {
    ($name:ident, $build:ident) => {
        async fn $name(Query(q): Query<FocusQuery>, session: Session) -> Result<Response, CqError> {
            let _ = session;
            let state = cq_state()?;
            if q.refresh.as_deref() == Some("1") {
                // 🔴 `invalidate(None)` にしない。SheetStore は架電クオリティ・営業KPI と
                //    共有で、None だと**あちらのキャッシュまで丸ごと消える**。
                //    捨てるのはこの画面が読むシートだけ。
                for name in super::SHEETS {
                    state.store.invalidate(Some(name)).await;
                }
            }
            let sheets = load(&state.client, &state.store)
                .await
                .map_err(|e| CqError::from_anyhow("consulting", e))?;
            Ok(Json(freshen($build(&sheets, today_jst()), &sheets, today_jst())).into_response())
        }
    };
}

simple_handler!(rampup, build_rampup);
simple_handler!(phone, build_phone);
simple_handler!(headquarters, build_headquarters);
simple_handler!(mtg_quality, build_mtg_quality);
simple_handler!(data_quality, build_data_quality);
simple_handler!(consultants, build_consultants);
simple_handler!(handover, build_handover);
simple_handler!(deal_board, build_deal_board);
simple_handler!(today_board, build_today_board);

#[derive(Debug, Deserialize)]
struct CustomerQuery {
    /// 法人番号。**1社1つ。** 複数候補があるときは選ばない（突合情報が悪い）。
    houjin: Option<String>,
}

/// 顧客1件の縦串。法人を指定しなければ、対象になりうる法人の一覧だけ返す。
async fn customer_detail(
    Query(q): Query<CustomerQuery>,
    session: Session,
) -> Result<Response, CqError> {
    let _ = session;
    let state = cq_state()?;
    let sheets = load(&state.client, &state.store)
        .await
        .map_err(|e| CqError::from_anyhow("consulting", e))?;
    let v = build_customer(&sheets, q.houjin.as_deref(), today_jst());
    Ok(Json(freshen(v, &sheets, today_jst())).into_response())
}

/// 返す JSON の `meta` に「このデータをいつ作ったか」を足す。
///
/// 🔴 この画面は毎朝見るもの。**古いデータを新しいものと誤認させないのは画面の責任**。
/// `meta.today` は計算に使った基準日で、**シートを作り直した日時とは別物**。
/// シートは手で作り直しているので、基準日だけ今日になっていて中身は何日も前、
/// ということが実際に起きる。だから両方を返す。
///
/// 取れないときは `null` を返す。**推測で埋めない**（埋めると嘘になる）。
/// 集計側（`build_*`）ではなくここで足しているのは、集計は fixture でも
/// 同じ値を返してほしいのに対し、鮮度は取ってきたシートの性質だから。
///
/// 🔴 **母集団（`population`）もここで足す。** 画面ごとに数えると、タブによって
/// 母集団が違う画面に戻ってしまう（実測で②593件・③703件と食い違っていた）。
pub(super) fn freshen(mut v: Value, sheets: &Sheets, today: NaiveDate) -> Value {
    // 🔴 母集団は**ここで1回だけ**作って全部の画面に載せる。
    //    画面ごとに数えると、タブによって数が違う画面に戻る。
    if let Some(o) = v.as_object_mut() {
        o.insert(
            "population".into(),
            serde_json::to_value(super::population_of(&sheets.deal)).unwrap_or(Value::Null),
        );
    }
    let at = super::generated_at(&sheets.meta);
    let age = super::generated_age_days(&sheets.meta, today);
    if let Some(m) = v.get_mut("meta").and_then(Value::as_object_mut) {
        m.insert(
            "generated_at".into(),
            at.map(Value::String).unwrap_or(Value::Null),
        );
        m.insert(
            "generated_age_days".into(),
            age.map(Value::from).unwrap_or(Value::Null),
        );
        // 🔴 元データを取った時刻。シートを作り直した時刻とは**別物**。
        //    古い JSON を詰め直すと generated_at だけ新しくなるので、
        //    「何日前のデータか」はこちらで数える。
        m.insert(
            "source_as_of".into(),
            super::data_as_of(&sheets.meta)
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        m.insert(
            "source_age_days".into(),
            super::data_age_days(&sheets.meta, today)
                .map(Value::from)
                .unwrap_or(Value::Null),
        );
    }
    v
}

// ================================================================ 集計

/// 代表値ひとそろい。**n を必ず持つ**（母数を添えずに中央値だけ出さない）。
#[derive(Debug, Serialize)]
pub struct Box5 {
    pub n: usize,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
    pub mean: f64,
}

/// 中央値。**偶数個なら中2つの平均**（`box5` と同じ約束。モックの `statistics.median`）。
///
/// 🔴 `v[v.len() / 2]` と書かない。偶数個のとき上側の値になり、`box5` の中央値と
/// 食い違う（担当の交代・採用単価の進捗帯で実際にそうなっていた）。
/// 並べ替えてから渡すこと。空なら `None`。
fn median_sorted(vs: &[f64]) -> Option<f64> {
    let n = vs.len();
    if n == 0 {
        None
    } else if n.is_multiple_of(2) {
        Some((vs[n / 2 - 1] + vs[n / 2]) / 2.0)
    } else {
        Some(vs[n / 2])
    }
}

/// 並べ替えてから中央値を取る。
pub(super) fn median_of(mut vs: Vec<f64>) -> Option<f64> {
    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median_sorted(&vs)
}

/// 非空の値だけから代表値を作る。1件も無ければ `None`（0 を返さない）。
fn box5(mut vs: Vec<f64>) -> Option<Box5> {
    if vs.is_empty() {
        return None;
    }
    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = vs.len();
    // モック(Python)と同じ nearest-rank。補間しない。
    let q = |p: f64| vs[(((n as f64) * p) as usize).min(n - 1)];
    Some(Box5 {
        n,
        min: vs[0],
        q1: q(0.25),
        median: median_sorted(&vs)?,
        q3: q(0.75),
        max: vs[n - 1],
        mean: vs.iter().sum::<f64>() / n as f64,
    })
}

/// 満了月ごとの継続率。
///
/// 定義は `call-quality-metrics` スキルで 2026-09-20 に確定済み。再導出しない。
///   - 母数 = 満了月が該当月 かつ 決着済み（継続／解約／充足）
///   - 充足は**分母に入れる**
///   - オプション契約は母数に入れない
///   - 結果待ちは分母に入れない（件数だけ別に返す）
fn monthly_retention(deals: &[Deal]) -> Value {
    #[derive(Default)]
    struct M {
        keep: usize,
        cancel: usize,
        fill: usize,
        pending: usize,
    }
    let mut by_month: BTreeMap<String, M> = BTreeMap::new();
    let mut no_expiry = 0usize;

    for d in deals {
        // オプションは `deals_of` の時点で落ちている。ここは保険。
        debug_assert!(!d.is_option(), "オプションが母数に残っている: {}", d.name);
        let Some(month) = d.manryou_month() else {
            no_expiry += 1;
            continue;
        };
        let e = by_month.entry(month.to_string()).or_default();
        match super::outcome_of(&d.stage) {
            Outcome::Keep => e.keep += 1,
            Outcome::Cancel => e.cancel += 1,
            Outcome::Fill => e.fill += 1,
            Outcome::Pending => e.pending += 1,
        }
    }

    let rows: Vec<Value> = by_month
        .iter()
        .map(|(month, m)| {
            let denom = m.keep + m.cancel + m.fill;
            json!({
                "month": month,
                "keep": m.keep,
                "cancel": m.cancel,
                "fill": m.fill,
                "denom": denom,
                "pending": m.pending,
                // 決着0件なら空。0% にしない。
                "rate": rate(m.keep as f64, denom as f64),
            })
        })
        .collect();

    json!({
        "rows": rows,
        "no_expiry": no_expiry,
        // 外したオプションの件数は `population`（全画面共通）に入っている
        "denominator_label": "満了月が該当月で決着済み（継続＋解約＋充足）。オプション契約は除く",
    })
}

/// 継続回数ごとの解約率と代表値。
///
/// 解約率の分子は **解約 + 充足**、分母は **決着済み（継続＋解約＋充足）**。
/// 充足を外さないのが確定した定義（外すと初回は 58.1%→46.9% に見える）。
fn by_renewal(deals: &[Deal], exclude_right_censored: bool) -> Vec<Value> {
    let mut groups: BTreeMap<i64, Vec<&Deal>> = BTreeMap::new();
    for d in deals {
        if let Some(r) = d.renewal_no {
            groups.entry(r).or_default().push(d);
        }
    }

    groups
        .iter()
        .map(|(no, ds)| {
            let n = ds.len();
            let n_active = ds.iter().filter(|d| d.is_active).count();
            let count = |o: Outcome| {
                ds.iter()
                    .filter(|d| super::outcome_of(&d.stage) == o)
                    .count()
            };
            let keep = count(Outcome::Keep);
            let cancel = count(Outcome::Cancel);
            let fill = count(Outcome::Fill);
            let pending = count(Outcome::Pending);
            // 🔴 解約率の分母は**決着済み（継続＋解約＋充足）**。結果待ちは入れない。
            //    以前は `n`（その継続回数の全取引）で割っていて、稼働中が分母に入っていた。
            //    実測（fixture・オプション除外後）で初回は n=1,914 のうち結果待ち 264件、
            //    解約＋充足 959件 → 50.1% と出ていたが、決着済み 1,650件で割ると 58.1%。
            let denom = keep + cancel + fill;

            // 代表値だけ右側打ち切りを外せる。解約率の母数は外さない
            // （解約したかどうかは打ち切りに関係なく確定している）。
            let stat_src: Vec<&Deal> = ds
                .iter()
                .copied()
                .filter(|d| !exclude_right_censored || !d.right_censored)
                .collect();
            let pick = |f: &dyn Fn(&Deal) -> Option<f64>| {
                box5(stat_src.iter().filter_map(|d| f(d)).collect())
            };

            json!({
                "renewal_no": no,
                "n": n,
                "n_active": n_active,
                "keep": keep,
                "cancel": cancel,
                "fill": fill,
                // 結果待ち。解約率の分母には入れない（件数だけ返す）
                "pending": pending,
                "denom": denom,
                // 分子は解約＋充足。母数は決着済み（継続＋解約＋充足）。
                "cancel_rate": rate((cancel + fill) as f64, denom as f64),
                // 充足を外した値。**画面の主値にはしない**（比較のためだけに返す）。
                // 分母は主値と同じ決着済み（分子だけが違う形にして、並べたときに比べられるように）
                "cancel_rate_excl_fill": rate(cancel as f64, denom as f64),
                "n_stats": stat_src.len(),
                "oubo": pick(&|d| d.oubo),
                "mensetu": pick(&|d| d.mensetu),
                "syoudaku": pick(&|d| d.syoudaku),
                "amount": pick(&|d| d.amount),
                "contract_period": pick(&|d| d.contract_period),
                "oubo_per_posting": pick(&|d| d.oubo_per_posting()),
                "rate_tassei": pick(&|d| d.rate_tassei()),
            })
        })
        .collect()
}

/// 欠測の偏り。
///
/// **これを隠すと判断を誤る。** うまくいかなかった契約ほど成果が記録されて
/// いない可能性があり、「継続するほど成果が良い」という相関を押し上げる方向に効く。
/// 結果（継続／解約／充足／結果待ち）ごとに記入率を出して、偏りを目で見えるようにする。
fn missingness(deals: &[Deal]) -> Vec<Value> {
    let fields: &[(&str, &dyn Fn(&Deal) -> Option<f64>)] = &[
        ("oubo", &|d: &Deal| d.oubo),
        ("mensetu", &|d: &Deal| d.mensetu),
        ("syoudaku", &|d: &Deal| d.syoudaku),
        ("saiyomokuhyou", &|d: &Deal| d.saiyomokuhyou),
        ("keisaisu", &|d: &Deal| d.keisaisu),
    ];
    let groups: &[(&str, Outcome)] = &[
        ("継続済", Outcome::Keep),
        ("解約", Outcome::Cancel),
        ("充足", Outcome::Fill),
        ("結果待ち", Outcome::Pending),
    ];

    let mut out = Vec::new();
    for (glabel, g) in groups {
        let ds: Vec<&Deal> = deals
            .iter()
            .filter(|d| super::outcome_of(&d.stage) == *g)
            .collect();
        for (flabel, f) in fields {
            let filled = ds.iter().filter(|d| f(d).is_some()).count();
            out.push(json!({
                "group": glabel,
                "field": flabel,
                "n": ds.len(),
                "filled": filled,
                "fill_rate": rate(filled as f64, ds.len() as f64),
            }));
        }
    }
    out
}

/// タブ②「継続回数 × 成果」の中身をまるごと作る。
///
/// `renewal()` から切り出してあるのは、**実データのシートを読んで
/// モックの数字と突き合わせるテストを書くため**。
pub fn build_renewal(sheets: &Sheets, exclude_right_censored: bool) -> Value {
    let deals = deals_of(&sheets.deal);
    let right_censored_n = deals.iter().filter(|d| d.right_censored).count();

    json!({
        "meta": {
            "n_deals": deals.len(),
            "right_censored_n": right_censored_n,
            "exclude_right_censored": exclude_right_censored,
            "all_cached": sheets.all_cached,
            // 何を数えていないかを画面に書くための一文。既存 p11 の書きぶりを借りる。
            "not_counted": "※ この画面は取引（契約）を数えています。顧客数でも商談数でもありません",
        },
        "monthly_retention": monthly_retention(&deals),
        "by_renewal": by_renewal(&deals, exclude_right_censored),
        "missingness": missingness(&deals),
    })
}

// ================================================================ タブ8 成果

/// 目標に対する進捗。
///
/// 🔴 **目標が入っていない取引は「達成率0%」ではない。母数に入れない。**
/// 稼働中で `saiyomokuhyou` が入っているのは半分ほどしかない。
/// 出せる分だけ出し、出していない分の件数を必ず添える。
fn goal_of(pop: &[&Deal]) -> Value {
    let has: Vec<&&Deal> = pop
        .iter()
        .filter(|d| matches!(d.saiyomokuhyou, Some(v) if v > 0.0))
        .collect();
    let both: Vec<&&&Deal> = has.iter().filter(|d| d.syoudaku.is_some()).collect();
    let mut vs: Vec<f64> = both.iter().filter_map(|d| d.rate_tassei()).collect();
    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let (mut b100, mut b50, mut b1, mut b0) = (0usize, 0usize, 0usize, 0usize);
    for v in &vs {
        if *v >= 1.0 {
            b100 += 1;
        } else if *v >= 0.5 {
            b50 += 1;
        } else if *v > 0.0 {
            b1 += 1;
        } else {
            b0 += 1;
        }
    }
    let median = median_sorted(&vs);

    // 🔴 目標はあるが採用数が空の取引。以前はどの帯にも入らず、帯を足しても
    //    見出しの母数（pop）にならなかった。「0%（実績ゼロ）」には落とさない
    //    （空は 0 ではない）。未記入として別の帯に置く。
    let no_result = has.len() - both.len();
    json!({
        "pop": pop.len(),
        "has_goal": has.len(),
        "both": both.len(),
        "no_result": no_result,
        "median": median,
        "fill_rate": rate(has.len() as f64, pop.len() as f64),
        // 帯の合計は必ず pop になる（tests.rs で固定）
        "bands": [
            {"label": "100%以上", "n": b100},
            {"label": "50〜100%", "n": b50},
            {"label": "1〜50%", "n": b1},
            {"label": "0%（実績ゼロ）", "n": b0},
            // 🔴 目標はあるが承諾数が空の取引。以前はどの帯にも入らず、帯の合計が pop に
            // 足りなかった（fixture 稼働中: 帯 345 + 未記入 253 = 598 / pop 604。6 件が消えていた）。
            // 達成率は出せないので 0% には入れず、別の帯で件数だけ出す。
            // rust / view の両チームが同じ帯を足していたので1本にまとめた。ラベルは画面
            // （goalBandColor / 注記の件数）が前方一致で読む「承諾数が空」を使う。
            // 件数は no_result（= has − both。rate_tassei は両方あれば必ず Some なので has − vs と同じ）
            {"label": "承諾数が空（目標はある）", "n": no_result},
            {"label": "未記入（目標が無い）", "n": pop.len() - has.len()},
        ],
    })
}

/// 求人票あたりの応募効率（`oubo / keisaisu`）を、結果の3群で比べる。
///
/// 🔴 **掲載数が空の取引はこの図に入れない。** 0件として扱うと、未入力が
/// 「効率が悪い」に化ける。
///
/// 🔴 資料の「失敗解約の最強識別子（per-posting 8.1 が 継続 14.5 を大きく下回る）」について。
/// 以前は「継続した / 稼働中」に結果待ちを混ぜていて、中央値の差がほとんど無く
/// 「再現しない」と書いていた。**結果待ちを外して決着の3群で比べ直すと差が出る**
/// （fixture: 継続 n=742 平均12.9 中央値7.5 ／ 解約 n=529 平均8.4 中央値5.0 ／
///   充足 n=142 平均16.5 中央値10.0。外した結果待ち 540件は平均6.7 中央値3.0、2026-09-23）。
/// 向きも大きさも資料に近いが、平均は外れ値に引かれる（中央値は 7.5 対 5.0）。
/// **単独で解約を見分けられるかは、ここからは言えない**（群の代表値を比べただけ）。
fn efficiency(deals: &[Deal], act: &[&Deal]) -> Value {
    let mut keizoku = Vec::new();
    let mut kaiyaku = Vec::new();
    let mut juusoku = Vec::new();
    // 🔴 結果待ちを「継続した」に混ぜない。以前は `_ => 継続` で、稼働中など
    //    結果がまだ出ていない取引が「継続した / 稼働中」の群に入っていた。
    //    決着の3群で比べ、結果待ちは件数だけ返す（未確定の点で比べない）。
    let mut pending = 0usize;
    for d in deals {
        let Some(v) = d.oubo_per_posting() else {
            continue;
        };
        match super::outcome_of(&d.stage) {
            Outcome::Keep => keizoku.push(v),
            Outcome::Fill => juusoku.push(v),
            Outcome::Cancel => kaiyaku.push(v),
            Outcome::Pending => pending += 1,
        }
    }
    json!({
        "groups": [
            {"label": "継続した", "box": box5(keizoku)},
            {"label": "解約した", "box": box5(kaiyaku)},
            {"label": "充足（採れて終わった）", "box": box5(juusoku)},
        ],
        "n_pending_excluded": pending,
        "has_keisaisu_all":
            deals.iter().filter(|d| matches!(d.keisaisu, Some(v) if v > 0.0)).count(),
        "has_keisaisu_act":
            act.iter().filter(|d| matches!(d.keisaisu, Some(v) if v > 0.0)).count(),
        "n_all": deals.len(),
        "n_act": act.len(),
        "caveat": "掲載数が空の取引は入っていません（0件として扱うと、未入力が「効率が悪い」に化けます）",
    })
}

/// リスクは2軸だけ。
///
/// 4軸のうち2つを**意図的に外している**（手抜きではない）:
///   1. 関係性（NPS）… 中身は「最新NPS 5以下」単独。NPS が稼働中の 46.4% にしか
///      無く、半分の顧客では構造的に発火しない。常に非赤の軸は「問題ない」と読まれる
///   2. モデル（churn予測）… 本番が StratifiedKFold のままで、同じ顧客が継続契約で
///      複数の取引を持つため AUC が上振れする。説明できない順位は現場に出せない
///
/// 🔴 **3軸目の「無い」を一括で扱わない。** 2つの状態を分ける:
///   - 接触の記録が1つも無い … 真の未測定 → **赤にしない**
///   - 記録はあるが契約後がゼロ … **赤のまま**（いちばん拾うべきもの）
fn risk(act: &[&Deal], contacts: &HashMap<String, Vec<NaiveDate>>, today: NaiveDate) -> Value {
    let mut rows = Vec::new();
    // (白, 赤, 未測定)
    let mut a3 = (0usize, 0usize, 0usize);
    let mut a4 = (0usize, 0usize, 0usize);
    let mut band = [0usize; 3];

    for d in act {
        let start = date10(&d.contract_start_date);
        let all = contacts.get(&d.id);
        let n_contact = all.map(|v| v.len()).unwrap_or(0);
        let last_post = match (all, start) {
            (Some(v), Some(st)) => v.iter().filter(|x| **x >= st).max().copied(),
            _ => None,
        };

        let (ax3, ax3w) = if n_contact == 0 {
            a3.2 += 1;
            ("未測定", "接触の記録が1つも無い".to_string())
        } else if last_post.is_none() {
            a3.1 += 1;
            ("赤", "契約後に一度も接触していない".to_string())
        } else {
            let dd = (today - last_post.expect("直前で None を除いている")).num_days();
            if dd > 30 {
                a3.1 += 1;
                ("赤", format!("契約後の最終接触から {dd}日"))
            } else {
                a3.0 += 1;
                ("白", format!("契約後の最終接触から {dd}日"))
            }
        };

        let expiry = date10(&d.contract_expiration_date);
        let (ax4, days_to_expiry) = match (expiry, d.amount) {
            (Some(ed), Some(amt)) => {
                let dte = (ed - today).num_days();
                // 満了60日以内 かつ 50万円以上。必要プロパティは100%入っている
                if (0..=60).contains(&dte) && amt >= 500_000.0 {
                    a4.1 += 1;
                    ("赤", Some(dte))
                } else {
                    a4.0 += 1;
                    ("白", Some(dte))
                }
            }
            _ => {
                a4.2 += 1;
                ("未測定", None)
            }
        };

        let b = usize::from(ax3 == "赤") + usize::from(ax4 == "赤");
        band[b] += 1;
        if b == 2 {
            rows.push(json!({
                "deal_id": d.id,
                "name": d.name,
                "stage": d.stage_label,
                "amount": d.amount,
                "days_to_expiry": days_to_expiry,
                "ax3w": ax3w,
                "n_contact": n_contact,
                // 契約後ゼロは別に立てる。いちばん拾うべきもの
                "never_after_start": last_post.is_none() && n_contact > 0,
            }));
        }
    }

    // 金額が大きい順。**機械が付けた順であって、優先順位そのものではない**
    rows.sort_by(|x, y| {
        y["amount"]
            .as_f64()
            .unwrap_or(-1.0)
            .partial_cmp(&x["amount"].as_f64().unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    json!({
        "n_act": act.len(),
        "ax3": {
            "白": a3.0, "赤": a3.1, "未測定": a3.2,
            "rule": "契約後の最終接触から30日超。接触＝MTG または60秒超の通話（メールは数えない）",
        },
        "ax4": {
            "白": a4.0, "赤": a4.1, "未測定": a4.2,
            "rule": "満了60日以内 かつ 50万円以上",
        },
        "bands": [
            {"label": "0＝安定", "n": band[0]},
            {"label": "1＝要注意", "n": band[1]},
            {"label": "2＝最優先", "n": band[2]},
        ],
        "top": rows,
        "order_note": "この並びは機械が付けた順（金額順）。手を打つかどうかは中身を読んで決めること",
    })
}

/// タブ8「成果」の中身。
///
/// `today` を引数で受けるのは、**同じ日で何度でも再現できるようにする**ため
/// （リスクの2軸はどちらも今日からの日数で決まる）。
pub fn build_outcome(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let all: Vec<&Deal> = deals.iter().collect();
    let (contacts, n_call, n_mtg) = contacts_by_deal(&sheets.call, &sheets.mtg);

    json!({
        "meta": {
            "today": today.to_string(),
            "n_deals": deals.len(),
            "n_active": act.len(),
            "all_cached": sheets.all_cached,
            "not_counted": "※ 成約率・継続率・MTG件数・メール件数ではありません。接触＝MTG または60秒超の通話で、メールは数えていません。接触は検知にだけ使い、処方には使いません",
        },
        "contact_source": {
            "calls_over_threshold": n_call,
            "mtgs_linked": n_mtg,
            "deals_with_contact": contacts.len(),
            "threshold_sec": super::CONTACT_SEC,
        },
        "goal_all": goal_of(&all),
        "goal_act": goal_of(&act),
        "efficiency": efficiency(&deals, &act),
        "risk": risk(&act, &contacts, today),
    })
}

// ================================================================ タブ1 いま見るべき顧客

/// 顧客ぜんたいの形。**母数を必ず添える**。
fn shape(cust: &[super::Customer]) -> Value {
    let disp: Vec<&super::Customer> = cust.iter().filter(|c| c.is_display_target).collect();
    let mut ltv: Vec<f64> = disp.iter().filter_map(|c| c.ltv).collect();
    ltv.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let multi = disp
        .iter()
        .filter(|c| matches!(c.kyoten_unique, Some(k) if k >= 2.0))
        .count();
    json!({
        "n_all": cust.len(),
        "n_display": disp.len(),
        "display_label": "稼働中の取引を持つ法人",
        "ltv": box5(ltv),
        "multi_site": multi,
        "multi_site_note": "拠点が2つ以上ある法人。決裁は事業所単位なので、1本の線にまとめない",
    })
}

/// 定期NPSが低い顧客を名指しする。
///
/// 🔴 **リスクの軸には入れない。** NPS は稼働中の半分にしか無く、
/// 半分の顧客では構造的に発火しないため、軸に混ぜると「問題ない」と誤読される。
/// 独立した指標として、**母数（何件に入っているか）を必ず添えて**出す。
fn nps_low(
    act: &[&Deal],
    nps: &HashMap<String, (String, f64)>,
    contacts: &HashMap<String, Vec<NaiveDate>>,
    today: NaiveDate,
) -> Value {
    let mut rows = Vec::new();
    let mut have = 0usize;
    let mut dist: BTreeMap<i64, usize> = BTreeMap::new();
    for d in act {
        let Some((month, v)) = nps.get(&d.id) else {
            continue;
        };
        have += 1;
        dist.entry(*v as i64).and_modify(|x| *x += 1).or_insert(1);
        if *v > super::NPS_LOW {
            continue;
        }
        let n_contact = contacts.get(&d.id).map(|v| v.len()).unwrap_or(0);
        let days_to_expiry =
            super::date10(&d.contract_expiration_date).map(|ed| (ed - today).num_days());
        rows.push(json!({
            "deal_id": d.id,
            "name": d.name,
                "stage": d.stage_label,
            "nps": v,
            "nps_month": month,
            "amount": d.amount,
            "days_to_expiry": days_to_expiry,
            "n_contact": n_contact,
            // 接触の記録が1つも無いものは別に立てる（赤ではなく未測定）
            "no_contact_record": n_contact == 0,
        }));
    }
    rows.sort_by(|a, b| {
        a["nps"]
            .as_f64()
            .unwrap_or(99.0)
            .partial_cmp(&b["nps"].as_f64().unwrap_or(99.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b["amount"]
                    .as_f64()
                    .unwrap_or(-1.0)
                    .partial_cmp(&a["amount"].as_f64().unwrap_or(-1.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    json!({
        "threshold": super::NPS_LOW,
        "n": rows.len(),
        "n_have_nps": have,
        "n_act": act.len(),
        // 🔴 率だけ出さない。何件に入っているかを必ず添える
        "coverage": rate(have as f64, act.len() as f64),
        "dist": dist.iter().map(|(v, n)| json!({"v": v, "n": n})).collect::<Vec<_>>(),
        "rows": rows,
        "note": "NPS はリスクの軸に入れていません。稼働中の半分にしか無く、半分の顧客では構造的に発火しないためです。独立した指標として出しています",
    })
}

/// 採用単価の悪化。
///
/// 🔴 **拠点ごとに見る。** 決裁は事業所単位で、9拠点ぶんを1本につなぐと
/// 拠点間のばらつきが「時間の悪化」に見えてしまう。
/// 🔴 **未確定（右側打ち切り）の点で悪化を判定しない。** 稼働中の契約は
/// 金額が丸ごと乗っているのに採用数がまだ伸びていないので、必ず高く出る。
fn cpa_worsening(deals: &[Deal]) -> Value {
    // 拠点キー → 契約開始順の採用単価
    let mut by_site: BTreeMap<String, Vec<(String, f64, bool)>> = BTreeMap::new();
    // 🔴 拠点キーが空の取引は判定に使わない（件数だけ返す）。以前は
    //    「(拠点不明) 法人番号」で1本にまとめていて、同じ法人の別々の拠点を
    //    1本の線で比べていた（決裁は事業所単位、の規律に反する）。
    //    法人番号まで空だと、全法人の拠点不明が1本になっていた。
    let mut no_site = 0usize;
    // 拠点キー → 表示名（照合用のキーは人が読む名前ではない）。
    // 表示名が空の拠点は入れない（キーで埋めない。行の `site_name` は null になる）
    let mut site_name: HashMap<String, String> = HashMap::new();
    for d in deals {
        let Some(v) = cpa(d) else { continue };
        if d.kyoten_key.is_empty() {
            no_site += 1;
            continue;
        }
        if let Some(nm) = d.site_name() {
            site_name
                .entry(d.kyoten_key.clone())
                .or_insert_with(|| nm.to_string());
        }
        by_site.entry(d.kyoten_key.clone()).or_default().push((
            d.contract_start_date.clone(),
            v,
            d.right_censored,
        ));
    }

    let mut worse = 0usize;
    let mut judged = 0usize;
    let mut skipped_censored = 0usize;
    let mut rows = Vec::new();
    for (site, mut pts) in by_site {
        pts.sort_by(|a, b| a.0.cmp(&b.0));
        // 判定に使うのは確定した点だけ
        let fixed: Vec<&(String, f64, bool)> = pts.iter().filter(|p| !p.2).collect();
        if fixed.len() < 2 {
            if pts.len() >= 2 {
                skipped_censored += 1;
            }
            continue;
        }
        judged += 1;
        let a = fixed[fixed.len() - 2].1;
        let b = fixed[fixed.len() - 1].1;
        if b > a {
            worse += 1;
            rows.push(json!({
                "site": site,
                "site_name": site_name.get(&site),
                "prev": a,
                "last": b,
                "ratio": if a > 0.0 { Some(b / a) } else { None },
                "n_points": fixed.len(),
            }));
        }
    }
    rows.sort_by(|x, y| {
        y["ratio"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&x["ratio"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(60);
    json!({
        "judged": judged,
        "worse": worse,
        "worse_rate": rate(worse as f64, judged as f64),
        "skipped_censored": skipped_censored,
        "skipped_no_site": no_site,
        "truncated": worse > rows.len(),
        "rows": rows,
        "note": "採用単価 ＝ 契約総額 ÷ 採用数。拠点ごとに、確定した直近2点で比べています。未確定（稼働中）の点は判定に使っていません（金額は丸ごと乗るのに採用数がまだ伸びず、必ず高く出るため）",
    })
}

/// MTG が「実施された事実」を2層に分けて出す。
///
/// 🔴 **同じ確かさで並べない。**
///   - Zoom 録画がある … **事実**。中身も読める
///   - メールから起こした日付 … **推定**（±1日で 83.3%・母数636件）
/// 1つの率にまとめると、推定が事実の顔をする。
fn mtg_layers(act: &[&Deal], mtg: &SheetData, mail: &SheetData) -> Value {
    let mut with_rec: HashSet<&str> = HashSet::new();
    for row in &mtg.rows {
        let d = mtg.get(row, "deal_id");
        if !d.is_empty() {
            with_rec.insert(d);
        }
    }
    let mut with_mail: HashSet<&str> = HashSet::new();
    let mut n_mail_rows = 0usize;
    for row in &mail.rows {
        let d = mail.get(row, "deal_id");
        // 「実施」だけを事実の層に寄せる。予定・候補は実施ではない
        if !d.is_empty() && mail.get(row, "kind") == "実施" {
            with_mail.insert(d);
            n_mail_rows += 1;
        }
    }
    let both = act
        .iter()
        .filter(|d| with_rec.contains(d.id.as_str()) && with_mail.contains(d.id.as_str()))
        .count();
    let only_rec = act
        .iter()
        .filter(|d| with_rec.contains(d.id.as_str()) && !with_mail.contains(d.id.as_str()))
        .count();
    let only_mail = act
        .iter()
        .filter(|d| !with_rec.contains(d.id.as_str()) && with_mail.contains(d.id.as_str()))
        .count();
    let neither = act.len() - both - only_rec - only_mail;
    json!({
        "n_act": act.len(),
        "fact_recording": {
            "label": "Zoom録画がある（事実・中身も読める）",
            "n": both + only_rec,
            "rate": rate((both + only_rec) as f64, act.len() as f64),
        },
        "estimated_mail": {
            "label": "メールから起こした実施日（推定・±1日で83.3%）",
            "n": both + only_mail,
            "rate": rate((both + only_mail) as f64, act.len() as f64),
            "rows": n_mail_rows,
        },
        "both": both,
        "only_recording": only_rec,
        "only_mail": only_mail,
        "neither": neither,
        "note": "2つを1つの率にまとめていません。録画は事実、メール由来は推定です",
    })
}

/// タブ1「いま見るべき顧客」の中身。
pub fn build_focus(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let cust = customers_of(&sheets.customer);
    let (contacts, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
    let nps = latest_nps(&sheets.history);

    json!({
        "meta": {
            "today": today.to_string(),
            "n_deals": deals.len(),
            "n_active": act.len(),
            "all_cached": sheets.all_cached,
            "not_counted": "※ 売上でも担当者の評価でもありません。手を打つ先を絞るための画面です。良し悪しの判断は人がします",
        },
        "shape": shape(&cust),
        "nps_low": nps_low(&act, &nps, &contacts, today),
        "cpa": cpa_worsening(&deals),
        "mtg_layers": mtg_layers(&act, &sheets.mtg, &sheets.mail_mtg),
    })
}

// ================================================================ タブ7 立ち上がり

/// 契約のどこにいるか。
///
/// 🔴 **経過月数ではなく、契約長に対する割合で見る。**
/// 同じ「3ヶ月目」でも、3ヶ月契約なら満了、12ヶ月契約なら序盤。
/// しきい値は検証済み: < 0.34 序盤 / < 0.67 中盤 / <= 1.05 終盤 / 超 満了超過。
/// 1.05 は満了直後の猶予。
fn phase_of(act: &[&Deal], today: NaiveDate) -> Value {
    // 🔴 並びは**時間の順**で返す。BTreeMap の文字コード順だと
    //    「中盤 → 出せない → 序盤 → 満了超過 → 終盤」になり、帯の図の並びと
    //    色の対応に意味が無くなっていた（画面は受け取った順に描く）。
    const ORDER: [&str; 5] = ["序盤", "中盤", "終盤", "満了超過", "出せない"];
    let mut c: HashMap<&str, usize> = ORDER.iter().map(|l| (*l, 0)).collect();
    for d in act {
        let start = date10(&d.contract_start_date);
        let label = match (start, d.contract_period) {
            (Some(st), Some(p)) if p > 0.0 => {
                let elapsed = (today - st).num_days() as f64 / 30.4;
                let pct = elapsed / p;
                if pct < 0.34 {
                    "序盤"
                } else if pct < 0.67 {
                    "中盤"
                } else if pct <= 1.05 {
                    "終盤"
                } else {
                    "満了超過"
                }
            }
            // 契約期間が空。**0 として「満了超過」に落とさない**
            _ => "出せない",
        };
        *c.get_mut(label).expect("label") += 1;
    }
    json!({
        "total": act.len(),
        "rows": ORDER.iter().map(|k| json!({"label": k, "n": c[k]})).collect::<Vec<_>>(),
        "rule": "契約長に対する割合。< 0.34 序盤 / < 0.67 中盤 / <= 1.05 終盤 / 超 満了超過。同じ3ヶ月目でも、3ヶ月契約なら満了・12ヶ月契約なら序盤",
    })
}

/// 契約開始から初回MTGまで。
///
/// 🔴 **この画面の初回MTGは MTG だけ。電話は入れていない**
/// （MTGと電話を束ねた「最終接触」はタブ8にある）。
/// 🔴 **単調ではない。** 相関であって因果ではない。
/// 🔴 解約率の分母を決着済みに直したら（2026-09-23）、以前の「遅い群が悪い」は読めなくなった。
///    fixture（基準日 2026-09-18）で 14日以内 49.7%（178/358）・15〜30日 36.7%（40/109）・
///    31〜60日 42.9%（63/147）・61日超 45.7%（91/199）。稼働中（結果待ち）を分母に入れていたとき
///    （32.7% / 26.5% / 36.6% / 42.3%）は、14日以内に若い契約が多く集まっていて低く見えていただけ。
fn first_mtg(deals: &[Deal], mtg_first: &HashMap<String, NaiveDate>) -> Value {
    let mut gaps: Vec<i64> = Vec::new();
    let mut pre = 0usize;
    // (n, 解約＋充足, 決着済み)
    // 🔴 解約率の分母は**決着済み（継続＋解約＋充足）**。以前は帯の全件（n）で割っていて、
    //    稼働中（結果待ち）が分母に入っていた。n は件数として別に返す。
    let mut buckets: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for label in ["14日以内", "15〜30日", "31〜60日", "61日超"] {
        buckets.insert(label, (0, 0, 0));
    }
    for d in deals {
        let (Some(st), Some(m)) = (date10(&d.contract_start_date), mtg_first.get(&d.id)) else {
            continue;
        };
        let g = (*m - st).num_days();
        if g < 0 {
            // 契約前のMTG。立ち上がりの速さには数えない（別に件数だけ出す）
            pre += 1;
            continue;
        }
        gaps.push(g);
        let label = if g <= 14 {
            "14日以内"
        } else if g <= 30 {
            "15〜30日"
        } else if g <= 60 {
            "31〜60日"
        } else {
            "61日超"
        };
        let e = buckets.get_mut(label).expect("label");
        e.0 += 1;
        match super::outcome_of(&d.stage) {
            Outcome::Cancel | Outcome::Fill => {
                e.1 += 1;
                e.2 += 1;
            }
            Outcome::Keep => e.2 += 1,
            Outcome::Pending => {}
        }
    }
    let stats = box5(gaps.iter().map(|x| *x as f64).collect());
    json!({
        "n": stats.as_ref().map(|s| s.n).unwrap_or(0),
        "pre_contract": pre,
        "stats": stats,
        "buckets": buckets.iter().map(|(k, (n, c, dn))| json!({
            "label": k, "n": n, "cancel": c,
            // 解約率の分母（決着済み）。結果待ち ＝ n − denom
            "denom": dn,
            // 決着0件なら空。0% にしない
            "cancel_rate": rate(*c as f64, *dn as f64),
        })).collect::<Vec<_>>(),
        "note": "MTG だけで、電話は入れていません。解約率は決着済み（継続＋解約＋充足）を分母にしています。単調ではないので「早ければ良い」とも「遅ければ悪い」とも言えません（相関であって因果ではない）",
    })
}

/// 稼働中の初回契約で、まだMTGをしていないもの。
fn no_mtg(act: &[&Deal], mtg_first: &HashMap<String, NaiveDate>, today: NaiveDate) -> Value {
    // 🔴 「マーケ関連」（紹介料・マーケ施策の計上）は MTG をしないのが普通なので、
    //    この表と、その分母（稼働中の初回契約）から外す。件数は返す（黙って消さない）
    let (marketing, first_time): (Vec<&&Deal>, Vec<&&Deal>) = act
        .iter()
        .filter(|d| d.renewal_no == Some(0))
        .partition(|d| super::MARKETING_STAGES.contains(&d.stage.as_str()));
    let mut rows = Vec::new();
    for d in &first_time {
        if mtg_first.contains_key(&d.id) {
            continue;
        }
        let days = date10(&d.contract_start_date).map(|st| (today - st).num_days());
        rows.push(json!({
            "deal_id": d.id,
            "name": d.name,
                "stage": d.stage_label,
            "amount": d.amount,
            "days_since_start": days,
        }));
    }
    rows.sort_by(|a, b| {
        b["days_since_start"]
            .as_i64()
            .unwrap_or(-1)
            .cmp(&a["days_since_start"].as_i64().unwrap_or(-1))
    });
    json!({
        "first_active": first_time.len(),
        "excluded_marketing": marketing.len(),
        "n": rows.len(),
        "rate": rate(rows.len() as f64, first_time.len() as f64),
        "rows": rows,
        "note": "稼働中の初回契約のうち、MTG の記録が1件も無いもの。記録が無いことと、やっていないことは別です",
    })
}

/// 取引ごとの初回MTG日（Zoom録画がある実データだけ）。
fn first_mtg_by_deal(mtg: &SheetData) -> HashMap<String, NaiveDate> {
    let mut out: HashMap<String, NaiveDate> = HashMap::new();
    for row in &mtg.rows {
        let deal = mtg.get(row, "deal_id");
        if deal.is_empty() {
            continue;
        }
        if let Some(d) = date10(mtg.get(row, "開催日")) {
            out.entry(deal.to_string())
                .and_modify(|x| {
                    if d < *x {
                        *x = d;
                    }
                })
                .or_insert(d);
        }
    }
    out
}

/// タブ7「立ち上がり」の中身。
///
/// 継続率と継続回数ごとの解約率は `build_renewal` が持っている
/// （画面側で同じ payload から描く。**同じ数字の作り方を2つ持たない**）。
pub fn build_rampup(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let mf = first_mtg_by_deal(&sheets.mtg);

    json!({
        "meta": {
            "today": today.to_string(),
            "n_deals": deals.len(),
            "n_active": act.len(),
            "all_cached": sheets.all_cached,
            "not_counted": "※ 成約率・売上・担当者の評価ではありません。契約が始まってから最初の MTG までに何日かかったかだけを見ています",
        },
        "phase": phase_of(&act, today),
        "first_mtg": first_mtg(&deals, &mf),
        "no_mtg": no_mtg(&act, &mf, today),
    })
}

// ================================================================ タブ6 電話

/// タブ6「電話」の中身。
///
/// 🔴 **接触は60秒超。** 実測で F1 0.671 が最良。`>300秒` は1人1時間あたり
/// 80.4% が0件になり使えない。`result=canceled` はほぼ全部0秒なのでこの閾値で落ちる。
pub fn build_phone(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let call = &sheets.call;
    // 🔴 通話はオプション契約に付いたものを数えない。画面の母集団（`deals_of`）と揃える。
    //    以前は通話明細の全行を数えていて、行数・月次・文字起こしの率に
    //    オプション契約の通話（fixture で 20,843行中 2,218行）が混ざっていた。
    let option_ids: HashSet<String> = super::deals_all_of(&sheets.deal)
        .into_iter()
        .filter(|d| d.is_option())
        .map(|d| d.id)
        .collect();

    // 取引ごとの通話（全部 / 接触）と、文字起こしの有無
    let mut all_by: HashMap<&str, usize> = HashMap::new();
    let mut hit_by: HashMap<&str, Vec<NaiveDate>> = HashMap::new();
    let mut monthly: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut n_transcript = 0usize;
    let mut n_rows = 0usize;
    let mut n_option_rows = 0usize;
    for row in &call.rows {
        let deal = call.get(row, "deal_id");
        if deal.is_empty() {
            continue;
        }
        if option_ids.contains(deal) {
            n_option_rows += 1;
            continue;
        }
        n_rows += 1;
        *all_by.entry(deal).or_insert(0) += 1;
        let secs = opt_num(call.get(row, "duration_sec")).unwrap_or(0.0);
        // 🔴 ts は UTC。日本時間の日付にしてから月・日を取る（0〜9時の通話が前日に入らないように）
        let day = super::call_date_jst(call.get(row, "ts"));
        if let Some(d) = day {
            let e = monthly
                .entry(d.format("%Y-%m").to_string())
                .or_insert((0, 0));
            e.0 += 1;
            if secs > super::CONTACT_SEC {
                e.1 += 1;
            }
        }
        if super::flag_true(call.get(row, "has_transcript")) {
            n_transcript += 1;
        }
        if secs > super::CONTACT_SEC {
            if let Some(d) = day {
                hit_by.entry(deal).or_default().push(d);
            }
        }
    }

    // 稼働中の取引ごとに「最後に話してから何日」
    let mut silent = Vec::new();
    let mut days: Vec<f64> = Vec::new();
    let mut no_call = 0usize;
    let mut no_contact = 0usize;
    // 名指しの表から外した「マーケ関連」の件数（KPI の件数からは外していない）
    let mut silent_marketing = 0usize;
    for d in &act {
        let n_all = all_by.get(d.id.as_str()).copied().unwrap_or(0);
        let hits = hit_by.get(d.id.as_str());
        if n_all == 0 {
            no_call += 1;
        }
        let last = hits.and_then(|v| v.iter().max().copied());
        let dd = last.map(|l| (today - l).num_days());
        match dd {
            Some(x) => days.push(x as f64),
            None => no_contact += 1,
        }
        // 接触が1本も無い、または最後の接触から90日超
        if !matches!(dd, None | Some(91..)) {
            continue;
        }
        // 🔴 「マーケ関連」（紹介料・マーケ施策の計上）は電話をしないのが普通。
        //    名指しの表に並べると、本当に音沙汰の無い契約が埋もれる。
        //    KPI（no_call / no_contact / 経過日数）からは外さず、表からだけ外して件数を返す
        if super::MARKETING_STAGES.contains(&d.stage.as_str()) {
            silent_marketing += 1;
            continue;
        }
        silent.push(json!({
            "deal_id": d.id, "name": d.name,
            "stage": d.stage_label, "amount": d.amount,
            "n_calls": n_all,
            "n_contact": hits.map(|v| v.len()).unwrap_or(0),
            // 🔴 「接触が1本も無い」は日数が出せない。0日として混ぜない
            "last_contact": last.map(|l| l.to_string()),
            "days_since": dd,
        }));
    }
    silent.sort_by(|a, b| {
        // 日数が無いもの（接触ゼロ）を先に出す。いちばん拾うべきもの
        let ka = a["days_since"].as_i64().unwrap_or(i64::MAX);
        let kb = b["days_since"].as_i64().unwrap_or(i64::MAX);
        kb.cmp(&ka)
    });

    json!({
        "meta": {
            "today": today.to_string(),
            "n_active": act.len(),
            "all_cached": sheets.all_cached,
            "threshold_sec": super::CONTACT_SEC,
            "not_counted": "※ 接触 ＝ 60秒超の通話。メールは数えていません。接触は検知にだけ使い、処方には使いません",
        },
        "reach": {
            "rows": n_rows,
            // 数えなかったオプション契約の通話の行数（黙って消さない）
            "option_rows_excluded": n_option_rows,
            "deals_with_call": all_by.len(),
            "no_call": no_call,
            "no_contact": no_contact,
            "no_call_rate": rate(no_call as f64, act.len() as f64),
            "no_contact_rate": rate(no_contact as f64, act.len() as f64),
            "note": "Call は Deal に多対多。同じ通話が複数の取引に付くので、行数はユニークな通話数より多くなります",
        },
        "days_since": box5(days),
        "silent": {
            "n": silent.len(),
            "rule": "接触が1本も無い、または最後の接触から90日超",
            // 表から外した「マーケ関連」の件数。黙って消さない
            "excluded_marketing": silent_marketing,
            "rows": silent,
        },
        "transcript": {
            "n": n_transcript,
            "rows": n_rows,
            "rate": rate(n_transcript as f64, n_rows as f64),
            "note": "電話の中身はまだ読めていません。文字起こしを取れているのはごく一部です",
        },
        "monthly": monthly.iter().map(|(m, (a, h))| json!({
            "month": m, "calls": a, "contacts": h,
            "contact_rate": rate(*h as f64, *a as f64),
        })).collect::<Vec<_>>(),
    })
}

// ================================================================ タブ4 本部アプローチ

/// 拠点が複数ある法人で、事業所ごとの成果を並べる。
///
/// 🔴 **親法人へロールアップしない。** 決裁は事業所単位で、
/// 9拠点ぶんを1つの数字にまとめると、どの事業所に行けばよいかが消える。
/// 本部に持っていくのは「事業所どうしの差」そのもの。
pub fn build_headquarters(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let hp = super::houjin_population(&sheets.deal);
    let mut by_houjin: HashMap<&str, Vec<&Deal>> = HashMap::new();
    for d in &deals {
        if !d.houjin_resolved.is_empty() {
            by_houjin
                .entry(d.houjin_resolved.as_str())
                .or_default()
                .push(d);
        }
    }

    let cust = customers_of(&sheets.customer);
    let names: HashMap<&str, &str> = cust
        .iter()
        .filter(|c| !c.name.trim().is_empty())
        .map(|c| (c.houjin.as_str(), c.name.as_str()))
        .collect();

    let mut rows = Vec::new();
    let mut multi = 0usize;
    for (houjin, ds) in &by_houjin {
        let sites: BTreeMap<&str, Vec<&&Deal>> = ds.iter().fold(BTreeMap::new(), |mut acc, d| {
            let k = if d.kyoten_key.is_empty() {
                "(拠点不明)"
            } else {
                d.kyoten_key.as_str()
            };
            acc.entry(k).or_default().push(d);
            acc
        });
        if sites.len() < 2 {
            continue;
        }
        multi += 1;
        let site_rows: Vec<Value> = sites
            .iter()
            .map(|(k, ds)| {
                let active = ds.iter().filter(|d| d.is_active).count();
                let out = |o: Outcome| {
                    ds.iter()
                        .filter(|d| super::outcome_of(&d.stage) == o)
                        .count()
                };
                let (keep, cancel, fill) =
                    (out(Outcome::Keep), out(Outcome::Cancel), out(Outcome::Fill));
                // 🔴 解約率は継続率と同じ定義: 分子 解約＋充足 ／ 分母 決着済み（継続＋解約＋充足）。
                //    以前は分子に充足が入らず、分母に稼働中（結果待ち）が入っていた。
                let denom = keep + cancel + fill;
                let amount: f64 = ds.iter().filter_map(|d| d.amount).sum();
                let syoudaku: f64 = ds.iter().filter_map(|d| d.syoudaku).sum();
                // 🔴 採用単価は**金額と採用数の両方が入っている取引だけ**で作る。
                //    以前は「金額がある取引の合計 ÷ 採用数がある取引の合計」で、
                //    割る側と割られる側の取引の集合が違っていた。
                //    （`amount` / `syoudaku` の列は表示用に今までどおり全件の合計を返す）
                // 🔴 さらに**稼働中（未確定）の取引は入れない**（未確定の点で比べない、の規律）。
                //    稼働中は金額が丸ごと乗っているのに採用がまだ伸びていないので単価が高く出て、
                //    法人の並び順（`spread` ＝ 拠点間の最大÷最小）をそのまま動かす。
                //    実測（fixture・オプション除外、2拠点以上の法人194社・548拠点）で、
                //    稼働中が採用単価に入っていた拠点が167、外すと単価が20%超動く拠点が61、
                //    稼働中でしか採用が無く単価が出なくなる拠点が33（2026-09-23）。
                //    外した件数は `cpa_n_active` で返す（黙って消さない）。
                let (cpa_amt, cpa_sy, cpa_n) = ds
                    .iter()
                    .filter(|d| !d.is_active)
                    .filter_map(|d| d.amount.zip(d.syoudaku))
                    .fold((0.0, 0.0, 0usize), |acc, (a, s)| {
                        (acc.0 + a, acc.1 + s, acc.2 + 1)
                    });
                let cpa_n_active = ds
                    .iter()
                    .filter(|d| d.is_active && d.amount.is_some() && d.syoudaku.is_some())
                    .count();
                // 表示名。同じ拠点キーで名前が違うときは最初の1つ。
                // 🔴 空なら null。照合用のキーで埋めない（人が読む名前ではない）
                let name = ds.iter().find_map(|d| d.site_name());
                json!({
                    "site": k,
                    "site_name": name,
                    "deals": ds.len(),
                    "active": active,
                    "keep": keep,
                    "cancel": cancel,
                    "fill": fill,
                    "denom": denom,
                    "cancel_rate": rate((cancel + fill) as f64, denom as f64),
                    "amount": amount,
                    "syoudaku": syoudaku,
                    // 採用単価の元になった取引の件数（決着済みで、金額と採用数が両方ある）
                    "cpa_n": cpa_n,
                    // 稼働中なので採用単価から外した取引の件数（金額と採用数は両方ある）
                    "cpa_n_active": cpa_n_active,
                    // 採用単価。**採用0では出さない**（0で割った値を単価にしない）
                    "cpa": if cpa_sy > 0.0 { Some(cpa_amt / cpa_sy) } else { None },
                })
            })
            .collect();
        // 拠点間の差がいちばん大きい法人を上に出す
        let cpas: Vec<f64> = site_rows.iter().filter_map(|r| r["cpa"].as_f64()).collect();
        let spread = match (
            cpas.iter().cloned().fold(f64::NAN, f64::min),
            cpas.iter().cloned().fold(f64::NAN, f64::max),
        ) {
            (lo, hi) if lo.is_finite() && hi.is_finite() && lo > 0.0 => Some(hi / lo),
            _ => None,
        };
        rows.push(json!({
            "houjin": houjin,
            // 会社名。🔴 13桁の法人番号だけでは現場は誰のことか分からない。
            //    `CS_顧客` の代表社名。見つからなければ null（番号で埋めない）
            "name": names.get(houjin).copied(),
            "sites": sites.len(),
            "deals": ds.len(),
            "active": ds.iter().filter(|d| d.is_active).count(),
            "spread": spread,
            "rows": site_rows,
        }));
    }
    rows.sort_by(|a, b| {
        b["spread"]
            .as_f64()
            .unwrap_or(-1.0)
            .partial_cmp(&a["spread"].as_f64().unwrap_or(-1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let truncated = rows.len() > 80;
    rows.truncate(80);

    json!({
        "meta": {
            "today": today.to_string(),
            // 🔴 法人番号で見る画面の注力の注記と同じ母数（`houjin_population`）。
            //    `by_houjin` もオプション契約を外した取引から作るので同じ数になる（テストで見張る）
            "n_houjin": hp.main.len(),
            "n_houjin_option_only": hp.option_only,
            "all_cached": sheets.all_cached,
            "not_counted": "※ 親法人の合計ではありません。事業所ごとに出しています。決裁は事業所単位なので、まとめると行き先が消えます",
            // 画面の注記に使う。拠点の行の数字どうしで検算できるように、定義を1か所で持つ
            "cpa_rule": "採用単価は決着済みの取引のうち、金額と採用数が両方入っているものだけで出しています。稼働中は採用がまだ伸びきっておらず高く出るので外しています（外した件数は拠点ごとに出しています）",
            "cancel_rule": "解約率 ＝（解約＋充足）÷ 決着済み（継続＋解約＋充足）。稼働中は分母に入れていません",
        },
        "multi_site": multi,
        "truncated": truncated,
        "rows": rows,
    })
}

// ================================================================ タブ5 MTGの品質

/// MTG の記録がどれだけ埋まっているか。
///
/// 🔴 **埋まっていないのは「悪い」ではなく「まだ抽出していない」。**
/// 抽出は Gemini の日次枠で止まっており、全 MTG のうち一部しか通っていない。
/// 率だけ出すと「記録していない現場が悪い」に読めるので、必ず理由を添える。
pub fn build_mtg_quality(sheets: &Sheets, today: NaiveDate) -> Value {
    let mtg = &sheets.mtg;
    // 🔴 オプション契約に結ばれた MTG は数えない。画面の母集団（`deals_of`）と揃える。
    //    取引に結べていない MTG は、オプションかどうか決められないので残す
    //    （「取引に結べた率」の分母に要る）。fixture では 3,133行中 9行が外れる。
    let option_ids: HashSet<String> = super::deals_all_of(&sheets.deal)
        .into_iter()
        .filter(|d| d.is_option())
        .map(|d| d.id)
        .collect();
    let rows: Vec<&Vec<std::sync::Arc<str>>> = mtg
        .rows
        .iter()
        .filter(|r| !option_ids.contains(mtg.get(r, "deal_id")))
        .collect();
    let total = rows.len();

    // 分析項目の埋まり具合
    let fields = [
        "議題",
        "決定事項",
        "やること",
        "顧客の懸念",
        "前向きシグナル",
        "次回予定",
        "リスク判定",
        "リスク理由",
        "商談フェーズ",
    ];
    let filled: Vec<Value> = fields
        .iter()
        .map(|f| {
            let n = rows
                .iter()
                .filter(|r| !mtg.get(r, f).trim().is_empty())
                .count();
            json!({"field": f, "n": n, "rate": rate(n as f64, total as f64)})
        })
        .collect();

    // リスク判定の分布
    let mut risk: BTreeMap<String, usize> = BTreeMap::new();
    let mut host: BTreeMap<String, usize> = BTreeMap::new();
    let mut linked = 0usize;
    let mut monthly: BTreeMap<String, usize> = BTreeMap::new();
    for r in &rows {
        let v = mtg.get(r, "リスク判定").trim();
        let key = if v.is_empty() {
            "（未判定）".to_string()
        } else {
            v.to_string()
        };
        *risk.entry(key).or_insert(0) += 1;
        let h = mtg.get(r, "ホスト氏名").trim();
        if !h.is_empty() {
            *host.entry(h.to_string()).or_insert(0) += 1;
        }
        if !mtg.get(r, "deal_id").is_empty() {
            linked += 1;
        }
        // バイト位置で切らない（日付でない値で panic させない）
        if let Some(m) = mtg.get(r, "開催日").get(..7) {
            *monthly.entry(m.to_string()).or_insert(0) += 1;
        }
    }
    // 🔴 リスク判定は**意味の順**（高 → 中 → 低 → 判定不可 → 未判定）で返す。
    //    BTreeMap の文字コード順だと「中 → 低 → 判定不可 → 高」になり、
    //    帯の図の並びに意味が無くなっていた。想定外の値は後ろに文字コード順で付ける。
    const RISK_ORDER: [&str; 5] = ["高", "中", "低", "判定不可", "（未判定）"];
    let rank = |k: &str| {
        RISK_ORDER
            .iter()
            .position(|x| *x == k)
            .unwrap_or(RISK_ORDER.len())
    };
    let mut risk_rows: Vec<(&String, &usize)> = risk.iter().collect();
    risk_rows.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| a.0.cmp(b.0)));
    let mut hosts: Vec<Value> = host
        .iter()
        .map(|(k, v)| json!({"host": k, "n": v}))
        .collect();
    hosts.sort_by(|a, b| {
        b["n"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["n"].as_i64().unwrap_or(0))
    });

    json!({
        "meta": {
            "today": today.to_string(),
            "n_mtg": total,
            // 数えなかったオプション契約の MTG の行数
            "option_rows_excluded": mtg.rows.len() - total,
            "all_cached": sheets.all_cached,
            "not_counted": "※ MTG の良し悪しを採点していません。何が記録されているかだけを出しています",
        },
        "linked": {
            "n": linked,
            "rate": rate(linked as f64, total as f64),
            "note": "取引に結べた MTG。結べないものは接触の計算にも入りません",
        },
        "filled": filled,
        "filled_note": "埋まっていないのは「記録していない」ではなく「まだ抽出を通していない」です。抽出は日次の枠で止まっており、全件には届いていません",
        "risk_dist": risk_rows.iter().map(|(k, v)| json!({"label": k, "n": v})).collect::<Vec<_>>(),
        "hosts": hosts,
        "monthly": monthly.iter().map(|(m, n)| json!({"month": m, "n": n})).collect::<Vec<_>>(),
    })
}

// ================================================================ タブ9 データ品質

/// この画面の数字をどこまで信じてよいか。
pub fn build_data_quality(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();

    // 法人番号の出どころ。
    // 🔴 生の行ではなく `deals` を数える。生の行だとオプション契約が混ざり、
    //    同じ画面の中で母数（n_deals）と内訳の合計が食い違う。
    let mut src: BTreeMap<String, usize> = BTreeMap::new();
    for d in &deals {
        let v = d.houjin_source.trim();
        let k = if v.is_empty() {
            "（無し）".to_string()
        } else {
            v.to_string()
        };
        *src.entry(k).or_insert(0) += 1;
    }

    // 成果系の記入率を、結果の区分ごとに
    let groups: &[(&str, Outcome)] = &[
        ("継続済", Outcome::Keep),
        ("解約", Outcome::Cancel),
        ("充足", Outcome::Fill),
        ("結果待ち", Outcome::Pending),
    ];
    let fields: &[(&str, &dyn Fn(&Deal) -> Option<f64>)] = &[
        ("oubo", &|d: &Deal| d.oubo),
        ("mensetu", &|d: &Deal| d.mensetu),
        ("syoudaku", &|d: &Deal| d.syoudaku),
        ("saiyomokuhyou", &|d: &Deal| d.saiyomokuhyou),
        ("keisaisu", &|d: &Deal| d.keisaisu),
    ];
    let mut bias = Vec::new();
    for (gl, g) in groups {
        let ds: Vec<&Deal> = deals
            .iter()
            .filter(|d| super::outcome_of(&d.stage) == *g)
            .collect();
        for (fl, f) in fields {
            let n = ds.iter().filter(|d| f(d).is_some()).count();
            bias.push(json!({"group": gl, "field": fl, "n": ds.len(), "filled": n,
                             "fill_rate": rate(n as f64, ds.len() as f64)}));
        }
    }

    let censored = deals.iter().filter(|d| d.right_censored).count();
    let no_expiry = deals
        .iter()
        .filter(|d| d.contract_expiration_date.is_empty())
        .count();
    let no_period = deals.iter().filter(|d| d.contract_period.is_none()).count();
    let no_start = deals
        .iter()
        .filter(|d| d.contract_start_date.is_empty())
        .count();

    json!({
        "meta": {
            "today": today.to_string(),
            "n_deals": deals.len(),
            "n_active": act.len(),
            "all_cached": sheets.all_cached,
        },
        "houjin_source": {
            "rows": src.iter().map(|(k, v)| json!({"label": k, "n": v})).collect::<Vec<_>>(),
            "note": "法人番号は1社1つ。候補が複数あるときは選んでいません（突合情報が悪いということなので、最小値を採って別法人に解決した事故があります）。法人番号は就業場所を表しません。事業所の特定には使えません",
        },
        "outcome_bias": {
            "rows": bias,
            "note": "うまくいかなかった契約ほど数字が記録されていない可能性があります。「継続するほど成果が良い」という見え方を押し上げる方向に効きます",
        },
        "missing": [
            {"label": "右側打ち切り（結果が確定していない直近の契約）", "n": censored,
             "rate": rate(censored as f64, deals.len() as f64),
             "note": "外すと応募数の代表値が動きます。画面で切り替えられます"},
            {"label": "満了日が空", "n": no_expiry,
             "rate": rate(no_expiry as f64, deals.len() as f64),
             "note": "継続率の母数に入りません"},
            {"label": "契約期間が空", "n": no_period,
             "rate": rate(no_period as f64, deals.len() as f64),
             "note": "フェーズを出せません。0 として満了超過に落としていません"},
            {"label": "契約開始日が空", "n": no_start,
             "rate": rate(no_start as f64, deals.len() as f64),
             "note": "「契約後の接触」を切り出せません"},
        ],
        // 先読み(`cs_dashboard::SHEETS`)と同じ順・同じ顔ぶれ（9枚）。
        // 片方だけ増えると「先読みしていないシートがある」状態に気づけない。
        // 🔴 以前は7枚で、担当履歴とメタが抜けていた（tests.rs で SHEETS と突き合わせる）
        "sheets": [
            {"name": super::SHEET_DEAL, "rows": sheets.deal.rows.len()},
            {"name": super::SHEET_CUSTOMER, "rows": sheets.customer.rows.len()},
            {"name": super::SHEET_MTG, "rows": sheets.mtg.rows.len()},
            {"name": super::SHEET_CALL, "rows": sheets.call.rows.len()},
            {"name": super::SHEET_HISTORY, "rows": sheets.history.rows.len()},
            {"name": super::SHEET_MAIL_MTG, "rows": sheets.mail_mtg.rows.len()},
            {"name": super::SHEET_HANDOVER, "rows": sheets.handover.rows.len()},
            {"name": super::SHEET_OWNER_HIST, "rows": sheets.owner_hist.rows.len()},
            {"name": super::SHEET_META, "rows": sheets.meta.rows.len()},
        ],
    })
}

// ================================================================ 担当の交代

/// 担当が替わった記録の一覧。
///
/// 🔴 **新しい指標を作らない。** `CS_担当交代` に既に入っている列を並べるだけ。
/// あの列は `scripts/consulting_owner/analyze_handover_reality.py` が作っていて、
/// 「MTG のホストが替わった日」を実際の交代日としている。
/// HubSpot の担当者欄が直された日は交代日ではない（遅れの中央値 73日）ので、
/// **そのずれ（`record_gap_days`）も並べて見えるようにする**。
///
/// 🔴 **誰から誰へ、でまとめない。** `from` / `to` は取引名と同じ「読めば分かる列」で、
/// テスト用データでは1行1人の連番に伏せてある。ここでまとめると、拠点キー・担当者・
/// ホスト氏名で踏んだのと同じ穴（1行1グループ）にはまる。**表示だけにとどめる。**
pub fn build_handover(sheets: &Sheets, today: NaiveDate) -> Value {
    // 🔴 引き当ては**オプション込みの全取引**で行い、オプション契約の行は外す。
    //    以前は `deals_of`（オプション除外）で引いていたので、オプション契約の交代が
    //    「取引名なし・決着済」として表に残っていた（fixture 385行中 27行）。
    //    外すのは画面の母集団（オプション除外）と揃えるため。外した件数は meta に出す。
    let all = super::deals_all_of(&sheets.deal);
    let by_id: HashMap<&str, &Deal> = all.iter().map(|d| (d.id.as_str(), d)).collect();
    let who = consultant_of(&sheets.owner_hist);
    let hv = &sheets.handover;

    let mut rows: Vec<Value> = Vec::new();
    let mut reflected: BTreeMap<String, usize> = BTreeMap::new();
    let mut to_retired = 0usize;
    let mut gaps: Vec<f64> = Vec::new();
    let mut active = 0usize;
    let mut option_excluded = 0usize;
    let mut unknown_deal = 0usize;

    for r in &hv.rows {
        let deal_id = hv.get(r, "deal_id");
        let d = by_id.get(deal_id);
        if d.is_some_and(|x| x.is_option()) {
            option_excluded += 1;
            continue;
        }
        if d.is_none() {
            unknown_deal += 1;
        }
        let is_active = d.map(|x| x.is_active).unwrap_or(false);
        if is_active {
            active += 1;
        }
        let rf = hv.get(r, "reflected").trim();
        let rf = if rf.is_empty() {
            "（記録なし）"
        } else {
            rf
        };
        *reflected.entry(rf.to_string()).or_insert(0) += 1;

        // 🔴 文字列の "TRUE"/"FALSE"。真偽値として入っていない
        let retired = hv.get(r, "to_retired").trim().eq_ignore_ascii_case("true");
        if retired {
            to_retired += 1;
        }
        let gap = opt_num(hv.get(r, "record_gap_days"));
        if let Some(g) = gap {
            gaps.push(g);
        }
        let (from, to) = (hv.get(r, "from"), hv.get(r, "to"));
        rows.push(json!({
            "deal_id": deal_id,
            "name": d.map(|x| x.name.as_str()).unwrap_or(""),
            "date": hv.get(r, "date"),
            // 🔴 シートの値そのまま。担当者一覧に無い人はメールアドレスが入っている
            "from": from,
            "to": to,
            // 画面に出す名前。メールアドレスは人の名前ではないので出さない（`person_label`）
            "from_label": person_label(from),
            "to_label": person_label(to),
            "from_unresolved": is_mail(from),
            "to_unresolved": is_mail(to),
            "to_retired": retired,
            "reflected": rf,
            "record_gap_days": gap,
            "is_active": is_active,
            // 状態の言葉。取引が見つからないものを「決着済」と書かないため
            "state_label": match d {
                None => "取引が見つからない",
                Some(x) if x.is_active => "稼働中",
                Some(_) => "決着済",
            },
            // いまの担当。交代の相手と食い違っていたら、その後さらに動いている
            "consultant": who.get(deal_id).map(|(n, _)| n.as_str()).unwrap_or(""),
        }));
    }

    // 新しい順。日付が無い行は末尾（0 として並べない）
    rows.sort_by(|a, b| {
        let k = |v: &Value| v["date"].as_str().unwrap_or("").to_string();
        k(b).cmp(&k(a))
    });

    // 偶数個なら中2つの平均（`box5` と同じ約束）
    let median_gap = median_of(gaps.clone());

    json!({
        "meta": {
            "today": today.to_string(),
            "all_cached": sheets.all_cached,
            "n": rows.len(),
            "n_active": active,
            // 表から外したオプション契約の交代。黙って消さない
            "n_option_excluded": option_excluded,
            // 取引のシートに見つからなかった交代（表には残す）
            "n_unknown_deal": unknown_deal,
            "not_counted": "※ 交代が良かったか悪かったかは判定していません。記録を並べているだけです。オプション契約の交代は、ほかの画面と同じく外しています",
        },
        "rows": rows,
        "reflected_dist": reflected.iter()
            .map(|(k, v)| json!({"label": k, "n": v})).collect::<Vec<_>>(),
        "to_retired": to_retired,
        "median_gap_days": median_gap,
        "n_gap": gaps.len(),
        "gap_rule": "「記録の遅れ」は、実際に替わった日（MTG のホストが替わった日）と HubSpot の担当者欄が直された日の差です。プラスなら HubSpot のほうが後。マイナスは、先に欄だけ直して実務の引き継ぎが後になったことを表します",
        "source_rule": "交代日は MTG のホストが替わった日で取っています。HubSpot の担当者欄が直された日は交代日ではありません",
    })
}

/// メールアドレスか。担当の交代シートは、HubSpot の担当者一覧に無い人
/// （退職してアーカイブされた担当など）を**メールアドレスのまま**書いてくる
/// （Hubspot リポジトリ `scripts/consulting_owner/analyze_handover_reality.py` の `who()`）。
fn is_mail(s: &str) -> bool {
    let t = s.trim();
    t.contains('@') && !t.contains(char::is_whitespace)
}

/// 画面に出す担当の名前。
///
/// 🔴 **メールアドレスを人の名前として出さない。** 氏名に引き直す表がこの画面の
/// シートには無い（担当履歴は consultant 名と owner_id だけ、MTG はホスト氏名だけ）ので、
/// 推測で名前を作らず「担当者一覧に無い人」と出す。元の値は `from` / `to` に残してある。
/// 本当に直すなら、書き出し側（上の `who()`）がアーカイブ済みの担当も引けるようにする。
pub(super) fn person_label(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else if is_mail(t) {
        Some("氏名が分からない担当（HubSpotの担当者一覧に無い）".to_string())
    } else {
        Some(t.to_string())
    }
}

// ================================================================ タブ3 顧客詳細

/// 顧客1件の縦串。
///
/// 🔴 **契約は継続のたびに別の取引レコードになる。** 取引をまたいで並べないと
/// 履歴が分断される。ここは法人でまとめるが、**採用単価だけは拠点ごとに分ける**
/// （決裁が事業所単位なので、1本の線にすると拠点差が時間の悪化に見える）。

/// 契約のどこまで来たか（契約長に対する割合）。
///
/// 🔴 経過月数ではなく**割合**で見る。同じ「3ヶ月目」でも、3ヶ月契約なら満了、
/// 12ヶ月契約なら序盤。契約期間が無ければ `None`（0 にしない）。
fn progress(d: &Deal, today: NaiveDate) -> Option<f64> {
    let st = date10(&d.contract_start_date)?;
    let p = d.contract_period?;
    if p <= 0.0 {
        return None;
    }
    let elapsed = (today - st).num_days() as f64 / 30.4;
    Some(elapsed / p)
}

/// 採用単価を比べる進捗帯。**モック（build_mock.py の `CPA_BANDS`）と同じ区切り。**
///
/// ```text
///   0  契約の前半（0〜50%）       進捗 <= 0.5
///   1  後半にさしかかり（50〜75%） 0.5 < 進捗 <= 0.75
///   2  終盤（75〜100%）           0.75 < 進捗 <= 1.01
/// ```
/// 🔴 1.01 を超えたもの（満了超過）はどの帯にも入れない。
/// 🔴 画面の「序盤／中盤／終盤」（`phase_of`・0.34 / 0.67）とは**別の区切り**。
///    あちらはいまどのフェーズにいるかを見るためのもの、こちらは採用単価を
///    比べる相手をそろえるためのもの。0〜25% はモックの実測で n=10 と少なく、
///    0〜50% にまとめてある。
const CPA_BANDS: [(&str, f64, f64); 3] = [
    ("契約の前半（0〜50%）", 0.0, 0.5),
    ("後半にさしかかり（50〜75%）", 0.5, 0.75),
    ("終盤（75〜100%）", 0.75, 1.01),
];

/// 採用単価の中央値を帯に載せてよい最小の件数（n<30 は比べる相手にしない）。
const CPA_BAND_MIN_N: usize = 30;

/// 🔴 契約開始が今日より先（進捗が負）の取引は「契約の前半」に入る（`lo == 0` の帯に下限が無い）。
/// これは**モックと同じ帯の割り当て**。モックは経過月数を `max(1, …)` で1か月に切り上げるので、
/// 開始前でも進捗は 1/契約期間になり、契約期間が2か月以上なら同じく前半に入る
/// （1か月契約だけはモックが終盤、こちらが前半で食い違う）。
/// fixture（オプション込みで数えた）で開始前・稼働中・採用数ありの取引は 5件で、契約期間は 3〜12か月（2026-09-18 時点）。
fn cpa_band_of(d: &Deal, today: NaiveDate) -> Option<usize> {
    let p = progress(d, today)?;
    CPA_BANDS
        .iter()
        .position(|(_, lo, hi)| (*lo == 0.0 || p > *lo) && p <= *hi)
}

/// 進捗帯ごとの採用単価の中央値と件数。
///
/// 🔴 **稼働中の本体契約だけ**で作る（モックと同じ）。以前は決着済みを含む全取引から
/// 作っていて、決着済みは進捗が 1 を超えて全部「終盤」に入っていた。
/// 採れる人数が出そろった決着済みの単価と、まだ採用が伸びていない稼働中の単価を
/// 同じ帯で比べると、稼働中が必ず高く見える（未確定の点で悪化を判定しない、の規律）。
/// 件数が `CPA_BAND_MIN_N` に満たない帯は中央値を出さない（名札も立たない）。
fn cpa_band_medians(deals: &[Deal], today: NaiveDate) -> [(Option<f64>, usize); 3] {
    let mut vals: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for d in deals.iter().filter(|d| d.is_active) {
        if let (Some(b), Some(v)) = (cpa_band_of(d, today), cpa(d)) {
            vals[b].push(v);
        }
    }
    vals.map(|v| {
        let n = v.len();
        let med = if n >= CPA_BAND_MIN_N {
            median_of(v)
        } else {
            None
        };
        (med, n)
    })
}

/// 契約開始からの「何ヶ月目か」。🔴 **始月を1ヶ月目と数える**（暦の月の差 + 1）。
///
/// 契約開始 7/23 なら、7月が1ヶ月目・8月が2ヶ月目・9月が3ヶ月目（日付は見ない）。
/// モックの `monthsBetween(start, t) + 1` と同じで、画面の `monthAdd(start, m)` も
/// m=1 を始月として読んでいる。
/// （以前ここに「始月は含めない（7/23→9/21 なら2ヶ月目）」と書いてあったが、
///   実装は上のとおり始月を含めていて、コメントのほうが誤りだった）
/// 数え方で率が振れるので1か所に閉じる。
/// 🔴 これは**暦の月**の番号。顧客詳細の月次推移（変更履歴が月単位）の横軸に使う。
///    案件一覧の「N / 期間 か月目」には使わない（`contract_month` を使う。理由はそちら）。
pub(super) fn month_index(start: &str, month: &str) -> Option<i64> {
    let (sy, sm) = super::ym_of(start)?;
    let (my, mm) = super::ym_of(month)?;
    Some((my as i64 - sy as i64) * 12 + (mm as i64 - sm as i64) + 1)
}

/// 契約の「何ヶ月目か」を**契約の日付で**数える（開始日から1か月ごとに区切る）。
///
/// 開始 3/19 なら 3/19〜4/18 が1ヶ月目、8/19〜9/18 が6ヶ月目。
/// 満了日（HubSpot では「開始 + 期間 − 1日」）の日までは期間を超えない。
///
/// 🔴 `month_index`（暦の月の差 + 1）を案件一覧の「N / 期間 か月目」に使っていたので、
///    **月の途中に始まった契約は最後の月に「7 / 6」と期間を1つ超えていた**。
///    6ヶ月契約 3/19〜9/18 は暦では 3月〜9月の7か月にまたがるため。
///    同じ理由で、まだ1か月残っている 10/20 開始の12ヶ月契約が 9/18 に「12 / 12」と出ていた。
///    実測（fixture・案件一覧の稼働中604件、2026-09-23）: 満了日が「開始 + 期間 − 1日」と
///    一致 583件（うち開始が1日なのは49件だけ、534件は月の途中の開始）、±3日 19件、
///    それ以外 1件、期間か日付が空 1件。
///    基準日 2026-09-18 で、暦の数え方だと期間を超える案件が62件（ほかに「6 / 6」なのに
///    実際はまだ5ヶ月目、と1か月先に出ていたものが59件）。この数え方だと期間を超えるのは
///    20件で、その20件は全部、満了日を過ぎてもまだ稼働中のもの。
///    つまり満了日が後ろにずれている取引ではなく、**暦の月と契約の月を取り違えた1ずれ**。
///    残る「満了を過ぎて稼働中」は画面が「満了後」と出す（`past_expiry`）。
/// 🔴 以前（N18）は顧客詳細の月次推移と数え方を揃えるために `month_index` にしていた。
///    推移の横軸は変更履歴が月単位なので暦の月でしか並べられない。
///    そちらは横軸を「暦の月」と書いて出し、「何ヶ月目」と名乗らせない。
///
/// 開始日が読めない、または開始が `today` より後なら `None`。
pub(super) fn contract_month(start: &str, today: NaiveDate) -> Option<i64> {
    use chrono::Datelike;
    let st = date10(start)?;
    if st > today {
        return None;
    }
    // 暦の月の差から始めて、今月の「開始日と同じ日」がまだ来ていなければ1つ戻す。
    // 月末の開始（1/31 など）は chrono が月末に寄せる（2/28）
    let mut k = (today.year() - st.year()) * 12 + (today.month() as i32 - st.month() as i32);
    while k > 0 {
        match st.checked_add_months(chrono::Months::new(k as u32)) {
            Some(d) if d > today => k -= 1,
            _ => break,
        }
    }
    Some(k as i64 + 1)
}

/// 取引ごとの月次推移。🔴 **現在値を並べない。** プロパティの変更履歴を使う。
///
/// 実測で応募数の変更の 30.4% / 採用数の 35.2% が決着日より後に入っている。
/// 現在値を時系列に並べると「契約中に伸びた」ように見える。
///
/// 値が変わらなかった月はシートに行が無いので、**最後の値を持ち越す**。
/// 持ち越した月は `carry` を立てて、画面が中空・破線で描けるようにする。
fn monthly_of(
    series: &HashMap<(String, String), Vec<(String, f64)>>,
    deal: &Deal,
    props: &[&str],
    until: &str,
) -> Value {
    let mut out = serde_json::Map::new();
    for prop in props {
        let key = (deal.id.clone(), (*prop).to_string());
        let pts = match series.get(&key) {
            Some(v) if !v.is_empty() => v,
            // 🔴 記録が無い系列は 0 で描かない。空で返して「記録がありません」と出させる
            _ => {
                out.insert((*prop).to_string(), json!([]));
                continue;
            }
        };
        let filled = super::fill_forward(pts, until);
        let rows: Vec<Value> = filled
            .iter()
            .filter_map(|mv| {
                month_index(&deal.contract_start_date, &mv.month)
                    .map(|mi| json!({"m": mi, "month": mv.month, "v": mv.v, "carry": mv.carry}))
            })
            .filter(|r| r["m"].as_i64().unwrap_or(0) >= 1)
            .collect();
        out.insert((*prop).to_string(), Value::Array(rows));
    }
    Value::Object(out)
}

/// 注力の内訳。**法人単位**（大きさの話）。
///
/// 🔴 **MTG途絶の帯とは別物。** あちらは取引単位で日々変わる状態、こちらは
/// 法人の大きさで、日々は変わらない。GAS の Layer に当たるのは帯のほう。
///
/// 🔴 定義はシートの `focus_flags` をそのまま読んでいる。**ここで作り直さない。**
/// 条件は重なる（1社が2つ3つに当たる）ので、内訳の合計は注力の社数と一致しない。
fn focus_shape(
    cust: &[super::Customer],
    flags: &HashMap<String, super::FocusFlags>,
    hp: &super::HoujinPopulation,
) -> Value {
    let disp: Vec<&super::Customer> = cust.iter().filter(|c| c.is_display_target).collect();
    let f = |c: &super::Customer| flags.get(&c.houjin).copied().unwrap_or_default();
    let n = |pop: &[&super::Customer], g: fn(&super::FocusFlags) -> bool| -> usize {
        pop.iter().filter(|c| g(&f(c))).count()
    };
    // 🔴 「全 N 法人まで広げると」の母数は本部アプローチの「全 N 法人」と同じもの
    //    （`houjin_population`・オプション契約しか持たない法人を外す）。
    //    以前は `CS_顧客` の全行（1,649）で、同じ画面の本部アプローチ（1,646）と合わなかった
    let all: Vec<&super::Customer> = cust
        .iter()
        .filter(|c| hp.main.contains(&c.houjin))
        .collect();
    json!({
        // `CS_顧客` の行数そのもの（データの点検で使う。画面の「全 N 法人」には使わない）
        "n_all": cust.len(),
        // 画面の「全 N 法人」。オプション契約しか持たない法人を外した数
        "n_houjin": all.len(),
        "n_houjin_option_only": hp.option_only,
        "n_display": disp.len(),
        "display_label": "稼働中の取引を持つ法人",
        // 図は表示対象（稼働中の取引を持つ法人）で描く。全法人だと点が多すぎて数えられない
        "n_focus": n(&disp, |x| x.any),
        "monthly_over_300k": n(&disp, |x| x.monthly_over_300k),
        "enterprise": n(&disp, |x| x.enterprise),
        "multi_site": n(&disp, |x| x.multi_site),
        // 全法人での件数も出す（表示対象だけを見ていると思われないように）
        "n_focus_all": n(&all, |x| x.any),
        "rule": "月額30万以上 ／ 従業員1,000名以上 ／ 拠点3つ以上 のいずれかに当たる法人です。\
    条件は重なるので、内訳を足しても注力の社数にはなりません。\
    この線引きはシートに入っている値をそのまま読んでいて、画面で決めていません",
        "not_layer": "これは法人の大きさの話で、法人ごとに決まります。日々は変わりません。\
    日々変わる状態は「今日動く先」の MTG途絶の帯のほうです。混ぜて読まないでください",
    })
}

pub fn build_customer(sheets: &Sheets, houjin: Option<&str>, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let cust = customers_of(&sheets.customer);
    let fflags = super::focus_flags_of(&sheets.customer);
    let focus = focus_shape(&cust, &fflags, &super::houjin_population(&sheets.deal));

    let Some(h) = houjin.filter(|x| !x.is_empty()) else {
        // 法人の指定が無ければ一覧だけ返す（全部の明細を返すと巨大になる）
        let mut list: Vec<&super::Customer> = cust.iter().filter(|c| c.is_display_target).collect();
        list.sort_by(|a, b| {
            b.ltv
                .unwrap_or(0.0)
                .partial_cmp(&a.ltv.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // 🔴 開いた瞬間に空の画面を出さない。**取引がいちばん多い法人**を既定にする。
        //    ①今日動く先の1件目にしなかったのは、あちらが日によって変わるので
        //    「昨日と同じ顧客を続けて見る」ができなくなるため。
        //    取引数が多い法人は履歴が長く、この画面（1顧客を深く見る）の値打ちが出る。
        let default_houjin = list
            .iter()
            .max_by(|a, b| {
                a.deal_count
                    .unwrap_or(0.0)
                    .partial_cmp(&b.deal_count.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.houjin.clone());
        return json!({
            "meta": {"today": today.to_string(), "all_cached": sheets.all_cached},
            "default_houjin": default_houjin,
            "default_reason": "取引がいちばん多い法人を既定で開いています。\
        ①今日動く先の1件目にしていないのは、あちらが日によって変わるので\
        「昨日と同じ顧客を続けて見る」ができなくなるためです",
            "index": list.iter().map(|c| {
                let f = fflags.get(&c.houjin).copied().unwrap_or_default();
                json!({
                    "houjin": c.houjin, "name": c.name, "ltv": c.ltv,
                    "deals": c.deal_count, "sites": c.kyoten_unique,
                    "active": c.active_deal_count, "last_expiration": c.last_expiration,
                    // 注力かどうかと、**なぜ注力なのか**。理由を出さないと絞り込めない
                    "focus": f.any, "focus_why": f.reasons(),
                })
            }).collect::<Vec<_>>(),
            "focus": focus,
            "note": "houjin を付けると1社の明細を返します",
        });
    };

    let c = cust.iter().find(|x| x.houjin == h);
    let mut ds: Vec<&Deal> = deals.iter().filter(|d| d.houjin_resolved == h).collect();
    ds.sort_by(|a, b| a.contract_start_date.cmp(&b.contract_start_date));

    // MTG の履歴
    let mut mtgs = Vec::new();
    let ids: HashSet<&str> = ds.iter().map(|d| d.id.as_str()).collect();
    for r in &sheets.mtg.rows {
        let did = sheets.mtg.get(r, "deal_id");
        if !ids.contains(did) {
            continue;
        }
        mtgs.push(json!({
            "deal_id": did,
            "date": sheets.mtg.get(r, "開催日"),
            "risk": sheets.mtg.get(r, "リスク判定"),
            "todo": sheets.mtg.get(r, "やること"),
            "concern": sheets.mtg.get(r, "顧客の懸念"),
            "positive": sheets.mtg.get(r, "前向きシグナル"),
            "next": sheets.mtg.get(r, "次回予定"),
            // 🔴 中身が空なのは「まだ抽出していない」。記録が無いのとは違う
            "extracted": !sheets.mtg.get(r, "やること").trim().is_empty(),
        }));
    }
    mtgs.sort_by(|a, b| {
        a["date"]
            .as_str()
            .unwrap_or("")
            .cmp(b["date"].as_str().unwrap_or(""))
    });

    /* ---- 採用単価を3つの出し方で ----
    🔴 1つの数字に見せない。出し方で値が変わることを画面に出す。
      (1) 総額 ÷ 採用数      … いちばん素直。ただし稼働中は金額が丸ごと乗る
      (2) 月割り             … 金額 ÷ 契約期間 × 経過月数 ÷ 採用数
      (3) 同じ進捗帯の中央値 … 進捗が近い契約どうしで比べる */
    // 同じ進捗帯（稼働中の本体契約だけ）の採用単価の中央値。`cpa_band_medians` 参照
    let band_med = cpa_band_medians(&deals, today);

    let cpa3: Vec<Value> = ds
        .iter()
        .map(|d| {
            let total = cpa(d);
            let monthly_cpa = match (d.amount, d.contract_period, d.syoudaku, progress(d, today)) {
                (Some(a), Some(p), Some(sy), Some(pg)) if p > 0.0 && sy > 0.0 => {
                    Some(a / p * (p * pg.min(1.0)).max(1.0) / sy)
                }
                _ => None,
            };
            json!({
                "deal_id": d.id, "name": d.name, "start": d.contract_start_date,
                "total": total, "monthly": monthly_cpa,
                // 🔴 帯と中央値は**稼働中の契約にだけ**付ける。決着済みの単価は
                //    稼働中の中央値と比べる相手ではない（採用数が出そろっている）
                "band": cpa_band_of(d, today).filter(|_| d.is_active).map(|b| CPA_BANDS[b].0),
                "band_median": cpa_band_of(d, today)
                    .filter(|_| d.is_active)
                    .and_then(|b| band_med[b].0),
                "syoudaku": d.syoudaku,
                "censored": d.right_censored, "active": d.is_active,
            })
        })
        .collect();

    /* ---- 月次推移（プロパティ履歴から）---- */
    let series = super::series_of(&sheets.history);
    let this_month = today.format("%Y-%m").to_string();
    let monthly: Vec<Value> = ds
        .iter()
        .map(|d| {
            // 🔴 埋めるのは**満了月まで**（今月のほうが早ければ今月まで）。
            //    以前は全取引を今月まで埋めていて、何年も前に満了した契約の線が
            //    今月まで平らに伸びていた（満了後に契約中の値が続いたように見える）。
            //    満了日が空なら今月まで（どこで終わったか分からないので切らない）。
            let until = match d.manryou_month() {
                Some(m) if m < this_month.as_str() => m.to_string(),
                _ => this_month.clone(),
            };
            json!({
                "deal_id": d.id, "name": d.name, "start": d.contract_start_date,
                "period": d.contract_period,
                // 🔴 横軸は**暦の月**（`month_index`、始月を1）。変更履歴が月単位なので。
                //    月の途中に始まった契約は、契約 6ヶ月でも暦では7か月にまたがる
                //    （案件一覧の稼働中604件のうち553件がこれ。`contract_month` の実測）。画面は横軸を
                //    「Nヶ月」ではなく暦の月で出し、期間とまたがる月数の違いを書く。
                "expiration": d.contract_expiration_date,
                // 開始の月から満了の月まで、暦で何か月にまたがるか。満了日が空・読めなければ null
                "span_months": d
                    .manryou_month()
                    .and_then(|m| month_index(&d.contract_start_date, m)),
                "until": until,
                "series": monthly_of(&series, d, &["oubo", "mensetu", "syoudaku"], &until),
                "nps": monthly_of(&series, d, super::NPS_PROPS, &until),
            })
        })
        .collect();

    /* ---- 担当交代と接触（時系列に載せる）---- */
    let hv = &sheets.handover;
    let handover: Vec<Value> = hv
        .rows
        .iter()
        .filter(|r| ids.contains(hv.get(r, "deal_id")))
        .map(|r| {
            json!({
                "deal_id": hv.get(r, "deal_id"), "date": hv.get(r, "date"),
                "from": hv.get(r, "from"), "to": hv.get(r, "to"),
                "reflected": hv.get(r, "reflected"),
            })
        })
        .collect();

    let (cmap, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
    let contacts: Vec<Value> = ds
        .iter()
        .map(|d| {
            json!({
                "deal_id": d.id,
                "dates": cmap.get(&d.id).map(|v| v.iter().map(|x| x.to_string())
                    .collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect();

    // 拠点ごとの採用単価
    let mut by_site: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for d in &ds {
        let Some(v) = cpa(d) else { continue };
        let k = if d.kyoten_key.is_empty() {
            "(拠点不明)"
        } else {
            d.kyoten_key.as_str()
        };
        by_site.entry(k).or_default().push(json!({
            "deal_id": d.id, "start": d.contract_start_date, "cpa": v,
            "syoudaku": d.syoudaku, "amount": d.amount,
            "renewal_no": d.renewal_no,
            // 未確定の点。実線でつながない
            "censored": d.right_censored, "active": d.is_active,
        }));
    }

    json!({
        "meta": {
            "today": today.to_string(),
            "houjin": h,
            "found": c.is_some(),
            "all_cached": sheets.all_cached,
            "not_counted": "※ 採用単価は拠点ごとに分けています。1本にまとめると拠点間のばらつきが時間の悪化に見えます",
        },
        "customer": c.map(|c| {
            let f = fflags.get(&c.houjin).copied().unwrap_or_default();
            json!({
                "name": c.name, "ltv": c.ltv, "deals": c.deal_count,
                "sites": c.kyoten_unique, "active": c.active_deal_count,
                "max_renewal_no": c.max_renewal_no, "last_expiration": c.last_expiration,
                "focus": f.any, "focus_why": f.reasons(),
            })
        }),
        // 注力の内訳。🔴 明細を開いていても出す（一覧に戻らないと分からない、を作らない）
        "focus": focus,
        "deals": ds.iter().map(|d| json!({
            "deal_id": d.id, "name": d.name,
                "stage": d.stage_label, "kind": d.contract_kind,
            "start": d.contract_start_date, "expiration": d.contract_expiration_date,
            "renewal_no": d.renewal_no, "amount": d.amount,
            "oubo": d.oubo, "mensetu": d.mensetu, "syoudaku": d.syoudaku,
            "is_active": d.is_active, "right_censored": d.right_censored,
            "site": d.kyoten_key,
        })).collect::<Vec<_>>(),
        "mtgs": mtgs,
        "cpa_by_site": by_site.iter().map(|(k, v)| json!({"site": k, "points": v}))
            .collect::<Vec<_>>(),
        // 応募 → 面接 → 採用 の落ち方。**0 と欠損を分ける**
        "funnel": {
            "oubo": sum_opt(&ds, |d| d.oubo),
            "mensetu": sum_opt(&ds, |d| d.mensetu),
            "syoudaku": sum_opt(&ds, |d| d.syoudaku),
        },
        // 採用単価を3つの出し方で。1つの数字に見せない
        "cpa3": cpa3,
        // 契約ごとの月次推移（プロパティ履歴から。現在値ではない）
        "monthly": monthly,
        // 担当の交代。時系列に載せる
        "handover": handover,
        "contacts": contacts,
    })
}

/// 非空だけ足す。1件も無ければ `None`（0 と「値が無い」を分ける）。
fn sum_opt(ds: &[&Deal], f: impl Fn(&Deal) -> Option<f64>) -> Option<f64> {
    let vs: Vec<f64> = ds.iter().filter_map(|d| f(d)).collect();
    if vs.is_empty() {
        None
    } else {
        Some(vs.iter().sum())
    }
}

// ================================================================ コンサルタント一覧

/// 1行1担当者。**毎朝これを見て、手が回っていない場所を探す画面**。
///
/// 🔴 **担当者の評価ではない。** 手が足りていない場所を見つけるためのもの。
/// 🔴 **接触率で見る。件数では見ない。** 件数だと持ち案件が多い人ほど大きく出て、
///    手が回っているかが分からなくなる。
///    接触率 ＝ 接触があった月 ÷ （案件 × 経過月）。**分母を必ず一緒に返す**。
/// 🔴 接触 ＝ MTG または60秒超の通話。**メールは数えない。**
pub fn build_consultants(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let who = consultant_of(&sheets.owner_hist);
    let focus = focus_of(&sheets.customer);
    let (contacts, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
    let nps = latest_nps(&sheets.history);

    struct Agg {
        n: usize,
        months: usize,
        touched: usize,
        atv_max: Option<f64>,
        focus: usize,
        expiring: usize,
        nps_low: usize,
        no_contact: usize,
        retired: bool,
    }
    let mut by: BTreeMap<String, Agg> = BTreeMap::new();
    let mut unknown = 0usize;
    let mut retired_deals = 0usize;
    let active_ids: HashSet<&str> = act.iter().map(|d| d.id.as_str()).collect();
    let ties = super::consultant_ties(&sheets.owner_hist, &active_ids);

    for d in &act {
        let Some((name, retired)) = who.get(&d.id) else {
            // 担当が取れない取引。**「その他」に混ぜない**。件数だけ出す
            unknown += 1;
            continue;
        };
        let e = by.entry(name.clone()).or_insert(Agg {
            n: 0,
            months: 0,
            touched: 0,
            atv_max: None,
            focus: 0,
            expiring: 0,
            nps_low: 0,
            no_contact: 0,
            retired: false,
        });
        e.n += 1;
        e.retired |= *retired;
        if *retired {
            retired_deals += 1;
        }

        // 接触率の分母と分子。🔴 ③案件の立ち位置と同じ関数を使う
        let (touched, months) = super::contact_rate_of(d, &contacts, today);
        e.months += months;
        e.touched += touched;
        if !contacts.contains_key(&d.id) {
            e.no_contact += 1;
        }

        if let Some(a) = d.amount {
            e.atv_max = Some(e.atv_max.map_or(a, |x: f64| x.max(a)));
        }
        if *focus.get(&d.houjin_resolved).unwrap_or(&false) {
            e.focus += 1;
        }
        if let Some(ed) = date10(&d.contract_expiration_date) {
            let dte = (ed - today).num_days();
            if (0..=60).contains(&dte) {
                e.expiring += 1;
            }
        }
        if matches!(nps.get(&d.id), Some((_, v)) if *v <= super::NPS_LOW) {
            e.nps_low += 1;
        }
    }

    let rows: Vec<Value> = by
        .iter()
        .map(|(name, a)| {
            json!({
                "consultant": name,
                "n_active": a.n,
                // 🔴 率だけ出さない。分子と分母を必ず添える
                "contact_touched": a.touched,
                "contact_months": a.months,
                "contact_rate": rate(a.touched as f64, a.months as f64),
                "atv_max": a.atv_max,
                "focus": a.focus,
                "expiring": a.expiring,
                "nps_low": a.nps_low,
                "no_contact": a.no_contact,
                "retired": a.retired,
                // 🔴 母数（案件×経過月）が小さい担当者は、図に載せると誤読を生む。
                //    1案件・5か月で 0% の人が、33案件で 24.4% の人より「悪い」位置に並ぶ。
                //    画面はこの印を見て**図からだけ外す**。表には残す（接触ゼロは拾いたい）。
                "small_n": a.months < super::MIN_CONTACT_MONTHS,
            })
        })
        .collect();

    json!({
        "meta": {
            "today": today.to_string(),
            "n_active": act.len(),
            "n_consultant": rows.len(),
            "unknown_owner": unknown,
            // 🔴 案件の数と担当の人数を両方出す。片方だけだと画面が食い違って見える
            "retired_deals": retired_deals,
            "retired_people": by.values().filter(|a| a.retired).count(),
            // 同じ日に複数行あって、どちらを採るかで担当が変わる取引
            "owner_ties": ties,
            "all_cached": sheets.all_cached,
            "not_counted": "※ 担当者の評価ではありません。手が足りていない場所を見つけるための画面です。\
    順位を付けていますが、良し悪しの判断は人がします",
        },
        "contact_rule": "接触 ＝ MTG または60秒超の通話（メールは数えない）。\
    接触率 ＝ 接触があった月 ÷（案件 × 経過月）。件数ではなく率で見るのは、\
    件数だと持ち案件が多い人ほど大きく出て、手が回っているかが分からなくなるため",
        "small_n_rule": format!(
            "接触率の図には、分母（案件 × 経過月）が {} か月未満の担当者を載せていません。\
    1案件・数か月の分母で 0% になった人が、何十案件も抱えて 20% 台の人より「悪い」位置に\
    並ぶと、実態とずれて読まれるためです。表には残しています（1案件でも接触ゼロなら拾いたいので）。",
            super::MIN_CONTACT_MONTHS),
        // 🔴 定義は `FocusFlags`（mod.rs）の実測どおり。以前ここだけ訂正前の
        //    「月額30万超 / 拠点が複数」が残っていた。画面の文字列に Markdown の ** を入れない
        "focus_rule": "注力は、月額30万以上 ／ 従業員1,000名以上 ／ 拠点3つ以上 のいずれかに当たる法人です。\
    シートに入っている値をそのまま読んでいて、ここで作り直していません",
        "owner_rule": "担当は consultant が正本です（hubspot_owner_id ではありません）。\
    取引ごとに、担当履歴のいちばん新しい行を採っています。\
    同じ日に複数行ある取引では、シートで後に来る行（＝追記順で新しい方）を採っています。\
    採り方を変えると担当が変わる取引があるので、その件数を出しています",
        "rows": rows,
    })
}

// ================================================================ 案件の立ち位置 / 今日動く先

/// 案件1件の「いまの立ち位置」。②と③で同じものを使う。
///
/// 🔴 **スコアや確率を出さない。** 契約開始時点の AUC は 0.583 で、順位付けの
/// 根拠にならない。代わりに**名札**（NPS4以下・満了が近い・接触が空いている等）を
/// 立てて、その**本数**で並べる。何で上に来たかが画面で説明できる形にする。
fn deal_rows(sheets: &Sheets, today: NaiveDate) -> (Vec<Value>, Value) {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let who = consultant_of(&sheets.owner_hist);
    let focus = focus_of(&sheets.customer);
    let (contacts, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
    let nps = latest_nps(&sheets.history);
    let series = super::series_of(&sheets.history);
    let until = today.format("%Y-%m").to_string();
    // 🔴 MTG は**録画（事実）とメール由来（推定）の両方**を見る。
    //    録画だけだと記録が無いものが191件出るが、その多くは「していない」ではなく
    //    「取引に結べていない」。それをそのまま赤にしない（2026-09-23）。
    let last_mtg = super::last_mtg_by_deal(&sheets.mtg, &sheets.mail_mtg);

    // 表示用のフェーズ（序盤／中盤／終盤）。`phase_of` と同じ区切り
    let band_of = |d: &Deal| -> Option<usize> {
        let p = progress(d, today)?;
        Some(if p < 0.34 {
            0
        } else if p < 0.67 {
            1
        } else {
            2
        })
    };
    // 同じ進捗帯の採用単価の中央値。比べる相手をそろえる。
    // 🔴 稼働中の本体契約だけで作り、区切りはモックと同じ 0.5 / 0.75（`cpa_band_medians`）
    let band_med = cpa_band_medians(&deals, today);

    // 履歴の最新値。🔴 現在値ではない
    let hist_last = |d: &Deal, prop: &str| -> Option<(String, f64, bool)> {
        let pts = series.get(&(d.id.clone(), prop.to_string()))?;
        let f = super::fill_forward(pts, &until);
        f.last().map(|m| (m.month.clone(), m.v, m.carry))
    };

    let mut rows = Vec::new();
    let mut flag_count: BTreeMap<&str, usize> = BTreeMap::new();
    // MTG途絶の帯ごとの件数。帯・出どころ・強制引き上げを別々に数える
    let mut band_count: BTreeMap<&str, usize> = BTreeMap::new();
    let mut src_count: BTreeMap<&str, usize> = BTreeMap::new();
    let mut forced_n = 0usize;
    let mut cover_rec = 0usize;
    let mut cover_mail = 0usize;
    let mut cover_any = 0usize;
    for d in &act {
        let (owner, retired) = who
            .get(&d.id)
            .map(|(o, r)| (o.clone(), *r))
            .unwrap_or_default();
        // 🔴 契約開始がまだ先の案件がある（実測115件）。
        //    そのまま計算すると「-1 / 6 か月目」と出て読めない。
        //    **開始前は経過月を出さない**（0ヶ月目でもない）。
        // 🔴 開始日が**空**のものは「開始前」ではない（分からない、であって先ではない）。
        //    以前は空も `started = false` に落ちて「開始がまだ先の契約」に数えられていた。
        let start = date10(&d.contract_start_date);
        let not_started = start.is_some_and(|st| st > today);
        let started = start.is_some_and(|st| st <= today);
        // 「契約の N ヶ月目」。🔴 契約の日付で数える（`contract_month`）。
        //    暦の月（`month_index`）で数えていたので、月の途中に始まった契約が
        //    最後の月に「7 / 6 か月目」と期間を超えていた（理由と実測は `contract_month`）。
        let months = if started {
            contract_month(&d.contract_start_date, today)
        } else {
            None
        };
        let days_left = date10(&d.contract_expiration_date).map(|e| (e - today).num_days());
        // 満了日を過ぎてもまだ稼働中。🔴 画面は「何ヶ月目」の代わりに「満了後」と出す。
        //    「15 / 6 か月目」のように期間を超えた数字は、数え間違いに見えて読めない。
        //    実測（fixture・基準日 2026-09-18）で案件一覧の稼働中604件のうち20件
        let past_expiry = matches!(days_left, Some(x) if x < 0);
        let last_contact = contacts.get(&d.id).and_then(|v| v.iter().max().copied());
        let days_since = last_contact.map(|l| (today - l).num_days());
        let n_contact = contacts.get(&d.id).map(|v| v.len()).unwrap_or(0);
        // 接触率。🔴 ②コンサルタント一覧と同じ関数。計算を2つ持たない
        let (touched_m, elapsed_m) = super::contact_rate_of(d, &contacts, today);
        let np = nps.get(&d.id);
        // MTG途絶の帯。閾値は GAS `no_mtg_alerter.gs` と同じ
        let gap = super::mtg_gap_of(d, &last_mtg, today);
        *band_count.entry(gap.band.label()).or_insert(0) += 1;
        if gap.band != super::MtgBand::Onboarding {
            let (r, m) = last_mtg.get(&d.id).copied().unwrap_or((None, None));
            if r.is_some() {
                cover_rec += 1;
            }
            if m.is_some() {
                cover_mail += 1;
            }
            if r.is_some() || m.is_some() {
                cover_any += 1;
            }
        }
        if gap.source != super::MtgSource::None {
            *src_count.entry(gap.source.label()).or_insert(0) += 1;
        }
        if gap.forced_by_expiry {
            forced_n += 1;
        }
        let band = band_of(d);
        let cband = cpa_band_of(d, today);
        let mycpa = cpa(d);
        let bmed = cband.and_then(|b| band_med[b].0);
        let vs_band = match (mycpa, bmed) {
            (Some(a), Some(b)) if b > 0.0 => Some(a / b),
            _ => None,
        };

        // ---- 名札。🔴 これの本数で並べる。スコアにしない ----
        let mut flags: Vec<&str> = Vec::new();
        if matches!(np, Some((_, v)) if *v <= super::NPS_LOW) {
            flags.push("NPSが4以下");
        }
        if matches!(days_left, Some(x) if (0..=60).contains(&x)) {
            flags.push("満了まで60日以内");
        }
        // 🔴 契約開始がまだ先なら「接触が無い」は当たり前。名札にしない
        // 🔴 接触の記録が1つも無いものには名札を立てる（いまの仕様のまま）。
        //    ④成果とリスクの3軸目はこれを「未測定」として赤にしないので、画面によって扱いが逆。
        //    ただし電話の「沈黙している取引」は接触ゼロを先頭（いちばん拾うべきもの）に出し、
        //    担当者一覧も「1案件でも接触ゼロなら拾いたい」としている。どちらに揃えるかは
        //    藤巻さんの判断待ち。一度ここで名札を外したが、方向を決めないまま
        //    今日動く先（名札2本以上）から案件が落ちるので戻した
        //    （fixture で開始済み・接触ゼロの稼働中は61件）。
        //    開始日が空のものは「開始前」とは分からないので、開始済みと同じに扱う（N6）
        if not_started {
            // 開始前。ここでは接触の名札を立てない
        } else if n_contact == 0 {
            flags.push("接触の記録が無い");
        } else if matches!(days_since, Some(x) if x > 30) {
            flags.push("接触が30日以上空いている");
        }
        if matches!(vs_band, Some(x) if x >= 1.5) {
            flags.push("採用単価が同じ進捗帯の1.5倍以上");
        }
        if retired {
            flags.push("担当が退職者のまま");
        }
        // 🔴 MTG途絶の名札。**記録が無いものには立てない**（そのまま赤にしない）。
        //    接触の名札（MTG＋60秒超の通話）とは別に立つので、同じ案件に2本つくことがある。
        //    向いている先が違う（あちらは電話も含む「連絡そのもの」、こちらは MTG だけ）。
        if gap.forced_by_expiry {
            flags.push("満了90日前でMTGが30日以上途絶");
        } else if gap.band.is_alert() {
            flags.push(gap.band.label());
        }
        if matches!(d.saiyomokuhyou, Some(t) if t > 0.0)
            && matches!(d.rate_tassei(), Some(r) if r < 0.5)
        {
            flags.push("採用目標の半分に届いていない");
        }
        for f in &flags {
            *flag_count.entry(f).or_insert(0) += 1;
        }

        let oubo = hist_last(d, "oubo");
        let mensetu = hist_last(d, "mensetu");
        let syoudaku = hist_last(d, "syoudaku");

        rows.push(json!({
            "deal_id": d.id, "name": d.name, "stage": d.stage_label,
            "consultant": owner, "retired": retired,
            "amount": d.amount, "focus": focus.get(&d.houjin_resolved).copied().unwrap_or(false),
            // 契約の N ヶ月目（開始日から1か月ごと。`contract_month`）。開始前・開始日が空なら null
            "months": months,
            // 満了日を過ぎてもまだ稼働中。画面は「何ヶ月目」の代わりに「満了後」と出す
            "past_expiry": past_expiry,
            // 開始前かどうか。画面は「何ヶ月目」の代わりに「開始前」と出す
            "not_started": not_started,
            // 開始日が空。開始前ではない（分からない）
            "start_unknown": start.is_none(),
            "start": d.contract_start_date,
            "period": d.contract_period,
            "days_left": days_left,
            "progress": progress(d, today),
            "band": band.map(|b| ["序盤", "中盤", "終盤"][b]),
            "nps": np.map(|(_, v)| *v),
            "nps_month": np.map(|(m, _)| m.clone()),
            "days_since_contact": days_since,
            "n_contact": n_contact,
            // 🔴 現在値ではなくプロパティ履歴の最新。carry は「その月に書き換えが無い」
            "oubo": oubo.as_ref().map(|x| x.1), "oubo_carry": oubo.as_ref().map(|x| x.2),
            "mensetu": mensetu.as_ref().map(|x| x.1),
            "mensetu_carry": mensetu.as_ref().map(|x| x.2),
            "syoudaku": syoudaku.as_ref().map(|x| x.1),
            "syoudaku_carry": syoudaku.as_ref().map(|x| x.2),
            // 接触率。🔴 ②と同じ定義・同じ関数。分子と分母を必ず一緒に返す
            "contact_touched": touched_m,
            "contact_months": elapsed_m,
            "contact_rate": rate(touched_m as f64, elapsed_m as f64),
            // MTG途絶。🔴 **出どころを行ごとに出す**（録画は事実、メールは推定）
            "mtg_band": gap.band,
            "mtg_band_label": gap.band.label(),
            "mtg_days": gap.days,
            "mtg_last": gap.last.map(|x| x.to_string()),
            "mtg_source": gap.source,
            "mtg_source_label": gap.source.label(),
            "mtg_forced_by_expiry": gap.forced_by_expiry,
            "saiyomokuhyou": d.saiyomokuhyou,
            "rate_tassei": d.rate_tassei(),
            "cpa": mycpa, "cpa_band_median": bmed, "cpa_vs_band": vs_band,
            // 採用単価を比べた進捗帯（`band` の序盤／中盤／終盤とは区切りが違う）
            "cpa_band": cband.map(|b| CPA_BANDS[b].0),
            "flags": flags,
            "n_flags": flags.len(),
        }));
    }

    // 名札の本数が多い順。同数なら金額の大きい順
    rows.sort_by(|a, b| {
        b["n_flags"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["n_flags"].as_u64().unwrap_or(0))
            .then_with(|| {
                b["amount"]
                    .as_f64()
                    .unwrap_or(-1.0)
                    .partial_cmp(&a["amount"].as_f64().unwrap_or(-1.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // 帯は決まった順に並べる（件数順にすると毎日入れ替わって読めない）
    let band_order = [
        super::MtgBand::Critical,
        super::MtgBand::Red,
        super::MtgBand::Yellow,
        super::MtgBand::Recent,
        super::MtgBand::NoRecord,
        super::MtgBand::Onboarding,
    ];
    let n_judged = act.len()
        - band_count
            .get(super::MtgBand::Onboarding.label())
            .copied()
            .unwrap_or(0);
    let meta = json!({
        "today": today.to_string(),
        "n_active": act.len(),
        "all_cached": sheets.all_cached,
        "flag_counts": flag_count.iter().map(|(k, v)| json!({"label": k, "n": v}))
            .collect::<Vec<_>>(),
        "mtg_gap": {
            "bands": band_order.iter().map(|b| json!({
                "band": b, "label": b.label(),
                "n": band_count.get(b.label()).copied().unwrap_or(0),
                "alert": b.is_alert(),
            })).collect::<Vec<_>>(),
            "forced_by_expiry": forced_n,
            "sources": src_count.iter().map(|(k, v)| json!({"label": k, "n": v}))
                .collect::<Vec<_>>(),
            // 帯を付けた母数（立ち上がり期を除いた稼働中）に対する被覆
            "n_judged": n_judged,
            "coverage": {
                "recording": cover_rec, "recording_rate": rate(cover_rec as f64, n_judged as f64),
                "mail": cover_mail, "mail_rate": rate(cover_mail as f64, n_judged as f64),
                "either": cover_any, "either_rate": rate(cover_any as f64, n_judged as f64),
            },
            "rule": format!(
                "最終MTGからの経過日数で分けています。注意 {}〜{}日 ／ 警告 {}〜{}日 ／ 重大 {}日以上。\
    契約開始から{}日以内は立ち上がり期として帯を付けていません。\
    満了まで{}日以内で{}日以上途絶しているものは、経過日数に関わらず重大にしています。\
    線引きは毎朝動いている GAS（no_mtg_alerter）と同じです",
                super::MTG_GAP_YELLOW_DAYS, super::MTG_GAP_RED_DAYS - 1,
                super::MTG_GAP_RED_DAYS, super::MTG_GAP_CRITICAL_DAYS - 1,
                super::MTG_GAP_CRITICAL_DAYS, super::MTG_ONBOARDING_GRACE_DAYS,
                super::MTG_PRE_TERMINATION_DAYS, super::MTG_PRE_TERMINATION_GAP_DAYS),
            "no_record_note": "「記録が無い」は「MTGをしていない」という意味ではありません。\
    録画が取引に結べていないぶんを含みます。だから名札は立てていません。\
    録画（事実）とメール由来の実施日（推定・±1日で83.3%）の両方を見たうえで、\
    それでも見つからなかったものだけがここに入ります",
            "source_note": "行ごとに、その日付をどちらから取ったかを出しています。\
    録画は事実、メール由来は推定です。1つの数字にまとめていません",
        },
        // 「採用単価が同じ進捗帯の1.5倍以上」の比べる相手。帯ごとの件数と中央値
        "cpa_bands": CPA_BANDS.iter().zip(band_med.iter()).map(|((label, lo, hi), (med, n))| json!({
            "label": label, "lo": lo, "hi": hi, "n": n, "median": med,
        })).collect::<Vec<_>>(),
        "cpa_band_rule": format!(
            "採用単価は、稼働中の契約どうしを契約の進み具合（前半 0〜50% ／ 50〜75% ／ 75〜100%）で\
    そろえて比べています。決着済みの契約とは比べていません（採用数が出そろっていて、必ず安く見えるため）。\
    件数が {CPA_BAND_MIN_N} 件に満たない帯は中央値を出さず、名札も立てていません"),
        "order_rule": "既定の並びは「名札の本数が多い順、同じなら金額の大きい順」です。\
    スコアや確率は出していません。契約開始時点の当たり具合（AUC 0.583）では順位付けの\
    根拠になりません。何で上に来たかは、その行の名札を見れば分かります",
        "not_counted": "※ 予測ではありません。既にあるデータに名札を付けて並べただけです。\
    手を打つかどうかは中身を読んで決めてください",
    });
    (rows, meta)
}

/// ②案件の立ち位置。稼働中の全件を返す（画面で並び替える）。
pub fn build_deal_board(sheets: &Sheets, today: NaiveDate) -> Value {
    let (rows, meta) = deal_rows(sheets, today);
    json!({"meta": meta, "rows": rows})
}

/// ③今日動く先。
///
/// 🔴 **件数が多すぎると使われない。** 上から順に潰せる長さに絞る。
/// 絞った条件は画面に出す。
pub fn build_today_board(sheets: &Sheets, today: NaiveDate) -> Value {
    let (rows, mut meta) = deal_rows(sheets, today);

    // 名札が2本以上。そのうえで `deal_rows` の並び（名札の本数が多い順、
    // 同じ本数なら金額の大きい順）の上から 24件
    const KEEP: usize = 24;
    const MIN_FLAGS: u64 = 2;
    let picked: Vec<Value> = rows
        .iter()
        .filter(|r| r["n_flags"].as_u64().unwrap_or(0) >= MIN_FLAGS)
        .cloned()
        .collect();
    let n_hit = picked.len();
    let mut top = picked;
    top.truncate(KEEP);

    // 今週満了するもの（名札の本数に関わらず落とさない）
    let soon: Vec<Value> = rows
        .iter()
        .filter(|r| matches!(r["days_left"].as_i64(), Some(x) if (0..=7).contains(&x)))
        .cloned()
        .collect();

    // 今週始まった契約。🔴 **始まった日に気づけないと、立ち上がり期
    //    （開始30日以内は帯を付けない）がただの取りこぼしになる。**
    //    日数は GAS `new_deal_detector.gs` の 7日と同じ。
    //    GAS は `createdate` で数えているが、ここは契約の開始日で数える
    //    （立ち上がり期と同じ時計でないと、対にならない）。
    let started: Vec<Value> = rows
        .iter()
        .filter(|r| {
            r["start"]
                .as_str()
                .and_then(date10)
                .map(|st| {
                    let age = (today - st).num_days();
                    (0..=super::NEW_DEAL_LOOKBACK_DAYS).contains(&age)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    // 開始がまだ先のもの。**「始まっていない」を「途絶」と読ませない**ために別に出す
    let not_started: Vec<Value> = rows
        .iter()
        .filter(|r| r["not_started"] == true)
        .cloned()
        .collect();

    if let Some(m) = meta.as_object_mut() {
        m.insert("filter_rule".into(), json!(format!(
            "名札が {MIN_FLAGS} 本以上ついた {n_hit} 件から、名札の本数が多い順（同じ本数なら金額の大きい順）に {KEEP} 件を出しています。\
毎朝ここだけ見れば動ける長さに絞るためで、{MIN_FLAGS} 本という線引きは取り決めです。\
全件は「案件」の中の「案件の立ち位置」で見られます"
        )));
        m.insert("n_hit".into(), json!(n_hit));
        m.insert("n_shown".into(), json!(top.len()));
        m.insert("n_started_this_week".into(), json!(started.len()));
        m.insert("n_not_started".into(), json!(not_started.len()));
        m.insert(
            "new_deal_rule".into(),
            json!(format!(
                "契約開始日が{}日以内のものを「今週始まった契約」として出しています。\
名札の本数に関わらず落としていません。始まったことに気づけないと、\
立ち上がり期（開始から{}日以内は MTG途絶の帯を付けない）が\
ただの取りこぼしになるためです。開始がまだ先の契約は別に数えています",
                super::NEW_DEAL_LOOKBACK_DAYS,
                super::MTG_ONBOARDING_GRACE_DAYS
            )),
        );
    }
    json!({
        "meta": meta,
        "rows": top,
        "expiring_this_week": soon,
        "started_this_week": started,
        "not_started": not_started,
    })
}
