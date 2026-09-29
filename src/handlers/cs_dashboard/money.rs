//! 満了と継続（09 の 5、新しい画面）と、成果と継続の金額の札（09 の 7・8章 段B）
//!
//! 2026-09-29 画面の組み替え 段B。藤巻さんの判断（09 の 10章④）: 金額は**出す**。ただし
//! **会社全体だけ**（人ごとの金額は出さない）、**確度を掛けた見込み金額は出さない**。
//!
//! ------------------------------------------------------------------
//! 🔴 `amount` は契約総額（月額ではない）。2026-09-29 に実データで確かめた
//! ------------------------------------------------------------------
//! Hubspot リポジトリ `data\consulting_dashboard\sheets\CS_取引.tsv`（2026-09-28 13:38 取得、3,698行）で、
//! オプション契約を除いた 3,464件の `amount` を契約期間（`contract_period`）ごとに見ると、
//! 中央値が 3か月 45万（1,811件）・6か月 90万（1,171件）・12か月 180万（177件）で、
//! 期間に比例して伸び、月あたりにすると 3つとも 15万 にそろう。月額なら期間で伸びない。
//! → **`amount` は契約期間全体の額（契約総額）**。
//! 同じシートの `monthly_amount` は `amount ÷ contract_period` を Python 側
//! （`scripts\consulting_dataset\build_payload.py` 351行）で割って作った列で、HubSpot の値ではない
//! （だから `amount ÷ monthly_amount` が期間と一致するのは確かめたことにならない）。
//! 空の件数（金額 0 も空として扱う。`money()`）: 稼働中（オプション除く）609件のうち 1件、
//! オプションを除いた全取引 3,464件のうち 18件。
//!
//! 契約総額なので、期間の違う契約を足すと長い契約ほど重く効く（12か月契約は 3か月契約の約4倍）。
//! 画面には「契約期間全体の額（月額ではない）」と必ず書く。月額に割り直した値は出さない
//! （期間が空の取引が 278件あり、割れない分を黙って落とすことになるため）。
//!
//! ------------------------------------------------------------------
//! 決めたこと
//! ------------------------------------------------------------------
//! - 母集団は全画面と同じ `deals_of`（オプション契約を除く）。稼働中は `is_active`。
//! - 満了月は `contract_expiration_date` で決める（継続率と同じ。`closedate` ではない）。
//! - 「今月・来月・再来月」は**暦の月**（基準日の月から 3か月）。満了日を過ぎてもまだ稼働中の契約は、
//!   その満了月に入れる（今月のうちに過ぎたものは今月、先月以前のものは `overdue_before` に別に数える）。
//!   どの稼働中の契約も、3か月の各月・先月以前・再来月より先・満了日なしのどれか1つにだけ入る
//!   （見張り tests.rs `renewal_pipe_partitions_active`）。
//! - ステージは件数だけ（実数）。**確度を掛けない。** ステージ名に「50％」などが含まれるが、
//!   HubSpot のステージの名前で、金額には掛けていない。
//! - 金額の継続率は、件数の継続率（`monthly_retention`）と**同じ母数の取引**を金額で足したもの:
//!   継続済の金額 ÷（継続済＋解約＋充足の金額）。結果待ちは入れない。金額が空の取引は分子にも分母にも
//!   入れず、件数を返す。
//! - 人ごとの金額は作らない（担当で分けた合計を返さない）。表の行の金額は取引1件の金額。

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};
use serde_json::{json, Value};

use crate::handlers::call_quality::tabs::rate;

use super::routes::deal_rows;
use super::{date10, deals_of, outcome_of, Deal, Outcome, Sheets};

/// 満了と継続で見る月の数（今月・来月・再来月）。09 の 5「満了月（今月・来月・再来月）」
pub const PIPE_MONTHS: usize = 3;

/// 基準日の月から `n` か月の `yyyy-MM`
fn months_from(today: NaiveDate, n: usize) -> Vec<String> {
    let (mut y, mut m) = (today.year(), today.month());
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(format!("{y:04}-{m:02}"));
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    out
}

/// 金額の合計。空（`None`）は足さずに数える。0 件なら合計は `None`（0 円と書かない）
#[derive(Default)]
struct Sum {
    n: usize,
    amount: f64,
    with_amount: usize,
}

impl Sum {
    fn add(&mut self, d: &Deal) {
        self.n += 1;
        if let Some(a) = d.amount {
            self.amount += a;
            self.with_amount += 1;
        }
    }
    fn json(&self) -> Value {
        json!({
            "n": self.n,
            // 金額が入っている取引が 0 件なら null（0 円ではない）
            "amount": (self.with_amount > 0).then_some(self.amount),
            "amount_n": self.with_amount,
            "amount_missing": self.n - self.with_amount,
        })
    }
}

/// 金額の説明（画面にそのまま出す）。🔴 月額と読ませない
/// 実測の中央値（45万・90万・180万）は画面に書かない（データが替わると嘘になる。根拠はこのファイルの頭）
pub const AMOUNT_BASIS: &str =
    "金額は HubSpot の取引の金額（amount）で、契約期間全体の額です（月額ではありません。\
期間の長い契約ほど大きくなります）。ステージの確度は掛けていません。人ごとの金額は出していません";

/// 稼働中の契約の金額と、今月・来月・再来月に満了する金額（会社全体）。
/// 満了と継続（件数の内訳）と成果と継続（札）が同じ数を出すように、1か所で数える
fn active_money(act: &[&Deal], window: &[String]) -> (Sum, Vec<Sum>, Sum) {
    let mut total = Sum::default();
    let mut by: Vec<Sum> = window.iter().map(|_| Sum::default()).collect();
    let mut win = Sum::default();
    for d in act {
        total.add(d);
        if let Some(i) = d
            .manryou_month()
            .and_then(|m| window.iter().position(|w| w == m))
        {
            by[i].add(d);
            win.add(d);
        }
    }
    (total, by, win)
}

/// 「満了と継続」（`/api/consulting/renewal-pipe`）。
///
/// 今月・来月・再来月に満了する稼働中の契約を、月ごとの件数・金額・ステージ別の件数と、
/// 満了の近い順の一覧（満了前の働きかけ＝毎週の仕事）で返す。
/// 一覧の担当・名札は `deal_rows`（案件一覧・今日と同じ行）から引く（名札の定義を作り直さない）。
pub fn build_renewal_pipe(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let window = months_from(today, PIPE_MONTHS);
    let (total, by, win) = active_money(&act, &window);
    let first = window[0].as_str();

    // ステージ別の件数（月ごと）。🔴 確度は掛けない。名前が空のものは「ステージ名なし」（内部IDは出さない）
    let stage_of = |d: &Deal| -> String {
        let t = d.stage_label.trim();
        if t.is_empty() {
            "ステージ名なし".to_string()
        } else {
            t.to_string()
        }
    };
    let mut stage_n: Vec<BTreeMap<String, usize>> =
        window.iter().map(|_| BTreeMap::new()).collect();
    let mut stage_all: BTreeMap<String, usize> = BTreeMap::new();
    let mut overdue = Sum::default();
    let mut later = 0usize;
    let mut no_expiry = 0usize;
    let mut in_window: Vec<&Deal> = Vec::new();
    let mut overdue_deals: Vec<&Deal> = Vec::new();
    for d in &act {
        match d.manryou_month() {
            None => no_expiry += 1,
            Some(m) => {
                if let Some(i) = window.iter().position(|w| w == m) {
                    *stage_n[i].entry(stage_of(d)).or_insert(0) += 1;
                    *stage_all.entry(stage_of(d)).or_insert(0) += 1;
                    in_window.push(d);
                } else if m < first {
                    overdue.add(d);
                    overdue_deals.push(d);
                } else {
                    later += 1;
                }
            }
        }
    }
    // ステージの並びは 3か月の合計の多い順（同数なら名前順）。毎月同じ並びで比べられるように1つに決める
    let mut stages: Vec<(String, usize)> = stage_all.into_iter().collect();
    stages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let months: Vec<Value> = window
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mut v = by[i].json();
            v["month"] = json!(m);
            v["stages"] = json!(stages
                .iter()
                .map(|(s, _)| json!({"label": s, "n": stage_n[i].get(s).copied().unwrap_or(0)}))
                .collect::<Vec<_>>());
            v
        })
        .collect();

    // 一覧の行。担当・名札・ステージ名は deal_rows の行をそのまま使う
    let (rows, _) = deal_rows(sheets, today);
    let by_id: HashMap<&str, &Value> = rows
        .iter()
        .filter_map(|r| r["deal_id"].as_str().map(|id| (id, r)))
        .collect();
    let row_of = |d: &Deal| -> Value {
        let r = by_id.get(d.id.as_str());
        let g = |k: &str| r.map(|r| r[k].clone()).unwrap_or(Value::Null);
        json!({
            "deal_id": d.id,
            "name": d.name,
            "expiry": d.contract_expiration_date.get(..10).unwrap_or(&d.contract_expiration_date),
            "days_left": date10(&d.contract_expiration_date).map(|e| (e - today).num_days()),
            "stage": d.stage_label,
            "amount": d.amount,
            "consultant": g("consultant"),
            "retired": g("retired"),
            "flags": g("flags"),
            "n_flags": g("n_flags"),
        })
    };
    // 満了の近い順（同じ日なら取引名の順。毎回同じ並びにする）
    let by_expiry = |a: &&Deal, b: &&Deal| {
        a.contract_expiration_date
            .cmp(&b.contract_expiration_date)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    };
    in_window.sort_by(by_expiry);
    overdue_deals.sort_by(by_expiry);

    json!({
        "meta": {
            "today": today.to_string(),
            "all_cached": sheets.all_cached,
            "n_active": act.len(),
            "window": window,
            "amount_basis": AMOUNT_BASIS,
            "not_counted": "※ 予測ではありません。満了日と今のステージをそのまま数えています。ステージの確度を掛けた見込みの金額は出していません",
        },
        "months": months,
        "stages": stages.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
        "window_total": win.json(),
        "active_total": total.json(),
        // 先月以前に満了日を過ぎて、まだ稼働中（ステージが動いていない）。黙って落とさず別に出す
        "overdue_before": {
            "sum": overdue.json(),
            "rows": overdue_deals.iter().map(|d| row_of(d)).collect::<Vec<_>>(),
        },
        // 再来月より先に満了する稼働中の件数と、満了日が入っていない件数（母数の注記に使う）
        "later": later,
        "no_expiry": no_expiry,
        "rows": in_window.iter().map(|d| row_of(d)).collect::<Vec<_>>(),
    })
}

/// 成果と継続の金額の札（09 の 7 の札のうち金額の3つ）。会社全体だけ。
///
/// - `active_total`: 稼働中の契約の金額の合計
/// - `window`: 今月・来月・再来月に満了する稼働中の契約の金額（満了と継続と同じ数）
/// - `retention`: 満了月ごとの金額の継続率。件数の継続率（`monthly_retention`）と同じ取引を金額で足す。
///   画面は件数の札と**同じ満了月**を選んで、並べて出す
pub fn build_money(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let act: Vec<&Deal> = deals.iter().filter(|d| d.is_active).collect();
    let window = months_from(today, PIPE_MONTHS);
    let (total, _, win) = active_money(&act, &window);

    #[derive(Default)]
    struct M {
        keep: f64,
        cancel: f64,
        fill: f64,
        pending: f64,
        settled_n: usize,
        settled_missing: usize,
    }
    let mut by_month: BTreeMap<String, M> = BTreeMap::new();
    for d in &deals {
        let Some(month) = d.manryou_month() else {
            continue;
        };
        let e = by_month.entry(month.to_string()).or_default();
        let o = outcome_of(&d.stage);
        if o != Outcome::Pending {
            e.settled_n += 1;
        }
        let Some(a) = d.amount else {
            if o != Outcome::Pending {
                e.settled_missing += 1;
            }
            continue;
        };
        match o {
            Outcome::Keep => e.keep += a,
            Outcome::Cancel => e.cancel += a,
            Outcome::Fill => e.fill += a,
            Outcome::Pending => e.pending += a,
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
                // 満了した金額（継続＋解約＋充足）。金額の継続率の分母
                "denom": denom,
                "pending": m.pending,
                // 決着した取引の件数と、そのうち金額が空で足していない件数
                "settled_n": m.settled_n,
                "settled_missing": m.settled_missing,
                // 決着した金額が 0 なら空。0% にしない
                "rate": rate(m.keep, denom),
            })
        })
        .collect();

    json!({
        "active_total": total.json(),
        "window": { "months": window, "sum": win.json() },
        "retention": {
            "rows": rows,
            "rule": "金額で見た継続率 ＝ 継続済の金額 ÷（継続済 ＋ 解約 ＋ 充足の金額）。取引は件数の継続率と同じ（満了月が該当月で決着済み、オプション契約は除く）。金額が空の取引は分子にも分母にも入れていません",
        },
        "amount_basis": AMOUNT_BASIS,
    })
}
