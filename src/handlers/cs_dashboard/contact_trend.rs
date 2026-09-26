//! ②コンサルタント →「担当者ごとの接触」
//!
//! 2026-09-24 藤巻さんの要望:「各担当者の平均接触数を週か月で比較したい」。
//! 担当者ごとに **持ち案件1件あたりの接触回数** を期間（週 / 月）ごとに並べる。
//!
//! ------------------------------------------------------------------
//! 決めたこと（画面にも同じことを書いている）
//! ------------------------------------------------------------------
//! - **接触の定義は作り直さない。** `contacts_by_deal`（MTG または60秒超の通話。
//!   メールは数えない。通話は日本時間 `call_date_jst`）をそのまま使う。
//!   数えるのは**契約期間の中**の接触だけ（接触率と同じ範囲）。
//! - 🔴 **接触は「その日に動いていた同じ拠点の本体案件」に付け直す**（`attach_contacts`）。
//!   通話の `deal_id` は、その日に動いていた契約ではなく、**あとから作られた継続先の取引**に
//!   付いていることが多い（HubSpot 側でなぜそうなるかは確かめていない。実データの付き方から分かったこと）。付いている取引の
//!   契約期間だけで数えると、継続の前の月ほど通話が「契約開始前」として落ち、
//!   確定した月の値も、あとで継続の取引が作られると下がる。
//!   fixture（基準日 2026-09-18）の実測: 契約開始前として落ちていた接触のうち、同じ `kyoten_key` の
//!   別の本体案件の契約期間に入るものが 2026-04 で 697件、05 で 623件、06 で 599件、07 で 567件、
//!   08 で 283件。前の契約と継続の契約の組で見ると、継続の開始前の60秒超の通話は継続先に 2,318件・
//!   前の契約に 278件付いていた（MTG は前の契約に 432件・継続先に 1件。通話だけが付き直されている）。
//!   付け直さないと 2026-04 の全体は 1件あたり 0.85 回（514/608）、付け直すと 2.31 回（1,402/608。
//!   オプションの取引に付いていた通話を同じ拠点の本体案件へ付け直した分も入る）。
//!   付け直しの決まり: 付いている取引の契約期間の中ならそのまま。外なら（オプション契約に
//!   付いている接触も）、同じ `kyoten_key` の本体案件のうち**その日が契約期間に入るものが1件だけ**
//!   のときにその案件へ付け直す。0件（初めての契約の開始前・契約と契約のすき間）と、
//!   2件以上（どれか決められない）は数えない。拠点キーが空の取引の接触も付け直さない。
//!   `contacts_by_deal`（接触の定義）は作り直していない。付け先を決め直すだけ。
//!   付け直した接触の数は期間ごとに `moved` で出す。拠点をまたいで付いた通話は直せないので、
//!   確定した期間でもその分はあとで動きうる（画面に書く）。
//! - **分母 ＝ その期間に担当として持っていた本体案件の数。** `deals_of`（オプション除外）
//!   のうち、契約期間（開始日〜満了日）がその期間に**1日でも重なる**もの。
//!   期間の途中で始まった・終わった案件も1件と数える（日割りにしない）。
//!   🔴 `is_active`（いまの状態）では選ばない。過去の期間の「持っていた」は
//!   いまの状態では決まらない。実データでは、満了日前に「継続済」へ動いて
//!   `is_active = FALSE` になった案件が満了日まで動いている（fixture・基準日 2026-09-18 で、
//!   満了日前なのに `is_active = FALSE` の26件のうち23件が「継続済」）。
//!   開始日・満了日のどちらかが読めない案件は数えられないので外し、件数を出す。
//!   🔴 画面の頭の「稼働中 N 件」（`population`・いまの `is_active`）とは**別の集合**。
//!   fixture では 2026-09 の持ち案件が偶然同じ 604件になるが、9/18 時点で契約期間の中の本体案件は
//!   551件で、稼働中なのに契約期間の外が 79件、契約期間の中なのに `is_active = FALSE` が 26件ある。
//!   同じ集合に見えないよう、画面（`denom_rule`）に違いを書く。
//!   🔴 満了日より前に解約・充足のステージへ移った案件も、満了日までは持っていたと数える。
//!   ステージが書き換わった日は実際に手を離した日とは限らず、推測で終わりを早めないため。
//!   fixture で、2026-04 以降に満了する案件のうち 31件がこれに当たる（ほとんどは数日の差。
//!   長いものは解約 2026-08-07・満了 2027-01-30 など）。月に数件の影響（画面に書く）。
//! - **分母0は空（null）。** 0 にしない。
//! - **分母が小さい（`MIN_DEALS` 件未満）期間には印（`small_n`）。** 画面は図から外し、
//!   表には残す（規律「n が小さいものを確定値の顔で並べない」）。
//! - **担当の正本は consultant 欄**（`CS_担当履歴` の `owner`。hubspot_owner_id ではない）。
//!   日ごとに「その日に有効だった担当」を履歴から引く（`owner_at`）。
//!   同じ日に複数行あるときは**シートで後に来る行**（`consultant_of` と同じ約束）。
//! - **期間の途中で担当が替わった案件は、替わる前と後の両方の担当に1件ずつ数える。**
//!   接触は**その日の担当**に数える。どちらか一方に寄せると、替わる前の人の接触が
//!   替わった後の人のものになる（または逆）ため。両方に数えた案件の数は `shared` で出す
//!   （担当者の分母を足すと、全体の分母よりこの数だけ多い）。
//! - 🔴 **履歴で決められない日**（その案件の担当履歴の最初の行より前）は、
//!   **どの担当者にも数えない**。最初の担当にさかのぼって付ける（推測で埋める）ことはしない。
//!   その期間に担当が1日も決まらない案件と、担当が決まらない日の接触は `undetermined` に
//!   件数で出す（黙って消さない）。fixture（オプション込みの全取引）では、担当履歴の最初の行が
//!   契約開始より後の取引が 3,658件中 242件（うち31日を超えて後なのは 29件）、履歴が1行も無い取引が1件。
//! - **今の週・月は途中なので未確定（`provisional`）。** データの取得日が今日より古いときは、
//!   取得日を含む期間から先も未確定にする（まだ取れていない日がある）。
//!   画面は中空＋破線で描く（規律「未確定は中空＋破線。実線と混ぜない」）。
//! - 🔴 **通話の記録が始まる前の期間は比べられない。** `CS_通話明細` は fixture で 2026-03-23（日本時間）から
//!   しか無く（MTG は 2025-08 以前からある）、それより前の月は MTG だけで数えることになる。
//!   月で 12か月並べると 2025-10〜2026-02 が 1件あたり 0.3〜0.4 回、2026-04 以降が 2.3〜3.1 回に
//!   なり、「接触が増えた」ように見えるがデータの範囲が変わっただけ。
//!   通話の記録が始まった日（`call_from`）より前に始まる期間には `calls_missing` を立て、
//!   画面は図にも表にも出さない（出さない理由を書く）。
//! - 🔴 **接触は検知専用。** 多いほど良いという評価ではない（担当者の評価ではない）。
//!   もめている案件ほど電話が増えることもあり、向きはこのデータでは決まっていない
//!   （引き継ぎ資料 03「4. 接触の定義と実測」）。
//! - 母集団（`population`）は `freshen()` が載せる。ここでは数え直さない。

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};

use super::{
    call_date_jst, contacts_by_deal, data_as_of, date10, deals_all_of, deals_of, flag_true, Deal,
    Sheets,
};
use crate::handlers::call_quality::sheets::SheetData;

/// これより少ない持ち案件の期間は `small_n`（図から外す。表には残す）。
pub const MIN_DEALS: usize = 3;
/// 月で見るときに並べる月の数（今月を含む）。
pub const N_MONTHS: usize = 12;
/// 週で見るときに並べる週の数（今週を含む）。
pub const N_WEEKS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Week,
    Month,
}

/// 期間。`start` と `end` は**両端を含む**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

fn month_first(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("1日は必ずある")
}

/// 月の初日を k か月ずらす（k は負でもよい）。
fn add_months(first: NaiveDate, k: i32) -> NaiveDate {
    let m0 = first.year() * 12 + first.month0() as i32 + k;
    NaiveDate::from_ymd_opt(m0.div_euclid(12), m0.rem_euclid(12) as u32 + 1, 1)
        .expect("月の初日は必ずある")
}

/// 古い順に `n` 個。最後の期間が `today` を含む。
/// 週は**月曜はじまり**（月〜日）。月は暦の月。
pub fn periods(unit: Unit, today: NaiveDate, n: usize) -> Vec<Period> {
    let mut out = Vec::with_capacity(n);
    match unit {
        Unit::Week => {
            let mon = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            for k in (0..n as i64).rev() {
                let s = mon - Duration::weeks(k);
                out.push(Period {
                    start: s,
                    end: s + Duration::days(6),
                });
            }
        }
        Unit::Month => {
            let first = month_first(today);
            for k in (0..n as i32).rev() {
                let s = add_months(first, -k);
                out.push(Period {
                    start: s,
                    end: add_months(s, 1) - Duration::days(1),
                });
            }
        }
    }
    out
}

/// 担当履歴の1行。`(その日から有効, 担当, 退職者か)`。
pub type OwnerEntry = (NaiveDate, String, bool);

/// 取引ごとの担当の移り変わり（日付の昇順。同じ日はシートの並び順のまま）。
///
/// 🔴 `consultant_of` と同じ約束で読む: 担当が空の行は読み飛ばす・同じ日なら
///    シートで後の行が勝つ。日付として読めない行は時間軸に置けないので外す
///    （fixture では 5,640行すべて `yyyy-MM-dd`）。
pub fn owner_timeline(owner_hist: &SheetData) -> HashMap<String, Vec<OwnerEntry>> {
    let mut out: HashMap<String, Vec<OwnerEntry>> = HashMap::new();
    for row in &owner_hist.rows {
        let deal = owner_hist.get(row, "deal_id");
        let owner = owner_hist.get(row, "owner").trim();
        if deal.is_empty() || owner.is_empty() {
            continue;
        }
        let Some(d) = date10(owner_hist.get(row, "date")) else {
            continue;
        };
        let retired = flag_true(owner_hist.get(row, "retired"));
        out.entry(deal.to_string())
            .or_default()
            .push((d, owner.to_string(), retired));
    }
    for v in out.values_mut() {
        // 安定ソート。同じ日の行はシートの並び順のまま（後の行が勝つ）
        v.sort_by_key(|e| e.0);
        // 🔴 同じ日の行は最後の1行だけ残す。先の行の担当は1日も持っていない
        //    （その日のうちに後の行に書き換わる）ので、「期間中に担当だった人」に入れない。
        //    残すと、同じ日に2行ある案件が「担当が替わった案件」として両方の担当に数えられる
        //    （fixture で月の `shared` が最大3件ずつ多く出た）。
        let mut keep: Vec<OwnerEntry> = Vec::with_capacity(v.len());
        for e in v.drain(..) {
            match keep.last_mut() {
                Some(last) if last.0 == e.0 => *last = e,
                _ => keep.push(e),
            }
        }
        *v = keep;
    }
    out
}

/// `day` に有効だった担当。履歴の最初の行より前なら `None`（**推測で埋めない**）。
pub fn owner_at(tl: &[OwnerEntry], day: NaiveDate) -> Option<&OwnerEntry> {
    let i = tl.partition_point(|e| e.0 <= day);
    (i > 0).then(|| &tl[i - 1])
}

/// `a`〜`b`（両端を含む）のどこかで担当だった人（出てきた順・重複なし）。
fn owners_during(tl: &[OwnerEntry], a: NaiveDate, b: NaiveDate) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let first = owner_at(tl, a).into_iter();
    let later = tl.iter().filter(|e| e.0 > a && e.0 <= b);
    for e in first.chain(later) {
        if !out.contains(&e.1.as_str()) {
            out.push(&e.1);
        }
    }
    out
}

/// 付け直したあとの接触。`(日, 付け直したか)`。日の昇順。
pub type Attached = HashMap<String, Vec<(NaiveDate, bool)>>;

/// 接触1件の付け先（モジュールの頭の「付け直し」の決まり）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// 付いている本体案件の契約期間の中。そのまま
    Own,
    /// 同じ拠点で、その日が契約期間に入る本体案件が1件だけあった。そこへ付け直す
    Moved(&'a str),
    /// 同じ拠点で、その日が契約期間に入る本体案件が2件以上。決められないので数えない
    Ambiguous,
    /// 拠点が分からない・その日に動いている本体案件が無い。数えない
    Dropped,
}

/// 付け直しの索引。**付け先の決まりはここ1か所だけに置く**
/// （「担当者ごとの接触」「担当の交代」「案件の詳細」が同じ決まりで付け直す）。
///
/// - `all`: オプションも含む全取引（付いている取引の拠点を引くため）
/// - `spans`: 本体案件の契約期間（`main_spans`）
pub struct AttachIndex<'a> {
    own: HashMap<&'a str, (NaiveDate, NaiveDate)>,
    site: HashMap<&'a str, &'a str>,
    by_site: HashMap<&'a str, Vec<(&'a str, NaiveDate, NaiveDate)>>,
}

impl<'a> AttachIndex<'a> {
    pub fn new(all: &'a [Deal], spans: &[(&'a str, NaiveDate, NaiveDate)]) -> Self {
        let own: HashMap<&str, (NaiveDate, NaiveDate)> =
            spans.iter().map(|&(id, s, e)| (id, (s, e))).collect();
        let site: HashMap<&str, &str> = all
            .iter()
            .map(|d| (d.id.as_str(), d.kyoten_key.trim()))
            .collect();
        let mut by_site: HashMap<&str, Vec<(&str, NaiveDate, NaiveDate)>> = HashMap::new();
        for &(id, s, e) in spans {
            if let Some(k) = site.get(id).filter(|k| !k.is_empty()) {
                by_site.entry(k).or_default().push((id, s, e));
            }
        }
        Self { own, site, by_site }
    }

    /// 取引の拠点キー（空なら `None`）。
    pub fn site_of(&self, id: &str) -> Option<&'a str> {
        self.site.get(id).copied().filter(|k| !k.is_empty())
    }

    /// `id` に付いている `d` の日の接触を、どこに付けるか。
    pub fn target(&self, id: &str, d: NaiveDate) -> Target<'a> {
        // 付いている本体案件の契約期間の中なら、そのまま
        if self.own.get(id).is_some_and(|&(s, e)| s <= d && d <= e) {
            return Target::Own;
        }
        let Some(k) = self.site_of(id) else {
            return Target::Dropped; // 拠点が分からない。付け直さない
        };
        let mut hit = self
            .by_site
            .get(k)
            .into_iter()
            .flatten()
            .filter(|&&(_, s, e)| s <= d && d <= e);
        match (hit.next(), hit.next()) {
            (Some(&(to, _, _)), None) => Target::Moved(to),
            // 🔴 どれに付けるか決められない。推測で選ばない
            (Some(_), Some(_)) => Target::Ambiguous,
            // 初めての契約の開始前・契約と契約のすき間。持っている案件が無い
            (None, _) => Target::Dropped,
        }
    }
}

/// 接触を、その日に動いていた本体案件に付ける（モジュールの頭の「付け直し」）。
///
/// - `raw`: `contacts_by_deal` の結果（接触の定義はそのまま）
/// - `all`: オプションも含む全取引（付いている取引の拠点を引くため）
/// - `spans`: 本体案件の契約期間（数える側と同じもの）
///
/// 返り値の2つめは、同じ拠点で契約期間の重なる本体案件が2件以上あって付け先を決められず、
/// 数えなかった接触の数。
pub fn attach_contacts(
    raw: &HashMap<String, Vec<NaiveDate>>,
    all: &[Deal],
    spans: &[(&str, NaiveDate, NaiveDate)],
) -> (Attached, usize) {
    let ix = AttachIndex::new(all, spans);
    let mut out: Attached = HashMap::new();
    let mut ambiguous = 0usize;
    for (id, days) in raw {
        for &d in days {
            match ix.target(id, d) {
                Target::Own => out.entry(id.clone()).or_default().push((d, false)),
                Target::Moved(to) => out.entry(to.to_string()).or_default().push((d, true)),
                Target::Ambiguous => ambiguous += 1,
                Target::Dropped => {}
            }
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    (out, ambiguous)
}

/// 1人1期間の数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Cell {
    deals: usize,
    contacts: usize,
}

impl Cell {
    fn json(self) -> Value {
        json!({
            "deals": self.deals,
            "contacts": self.contacts,
            // 🔴 分母0は空。0 にしない
            "avg": (self.deals > 0).then(|| self.contacts as f64 / self.deals as f64),
            "small_n": self.deals > 0 && self.deals < MIN_DEALS,
        })
    }
}

/// 本体案件の契約期間 `(取引ID, 開始日, 満了日)`。`deals` はオプション除外（`deals_of`）を渡す。
/// 開始日・満了日のどちらかが読めない（または開始日が満了日より後の）案件は入れず、その数を2つめに返す。
///
/// 🔴 「担当者ごとの接触」と「担当の交代」の前後比較は、**同じこの期間で**接触を付け直す
///    （`attach_contacts`）。画面ごとに契約期間の読み方を作り直さない。
pub fn main_spans(deals: &[Deal]) -> (Vec<(&str, NaiveDate, NaiveDate)>, usize) {
    let mut no_span = 0usize;
    let spans = deals
        .iter()
        .filter_map(|d| {
            match (
                date10(&d.contract_start_date),
                date10(&d.contract_expiration_date),
            ) {
                (Some(s), Some(e)) if s <= e => Some((d.id.as_str(), s, e)),
                _ => {
                    no_span += 1;
                    None
                }
            }
        })
        .collect();
    (spans, no_span)
}

/// 未確定にし始める日。今日と、データを取った日の早いほう。
pub(super) fn cutoff_of(sheets: &Sheets, today: NaiveDate) -> NaiveDate {
    data_as_of(&sheets.meta)
        .and_then(|s| date10(&s))
        .map(|d| d.min(today))
        .unwrap_or(today)
}

/// 通話の記録が始まった日（日本時間）。長さを問わず、いちばん古い通話の日。
/// 1本も無ければ `None`（どの期間も通話が数えられない）。
pub fn call_from(call: &SheetData) -> Option<NaiveDate> {
    call.rows
        .iter()
        .filter_map(|r| call_date_jst(call.get(r, "ts")))
        .min()
}

fn period_json(unit: Unit, p: &Period, cutoff: NaiveDate, calls: Option<NaiveDate>) -> Value {
    let (key, label) = match unit {
        Unit::Month => {
            let k = p.start.format("%Y-%m").to_string();
            (k.clone(), k)
        }
        Unit::Week => (
            p.start.to_string(),
            format!(
                "{}/{}〜{}/{}",
                p.start.month(),
                p.start.day(),
                p.end.month(),
                p.end.day()
            ),
        ),
    };
    json!({
        "key": key,
        "label": label,
        "start": p.start.to_string(),
        "end": p.end.to_string(),
        // 🔴 いまの期間（途中）と、データを取った日から先を含む期間は未確定
        "provisional": p.end >= cutoff,
        // 🔴 通話の記録が始まる前に始まる期間。MTG しか数えられないので、ほかの期間と比べられない
        "calls_missing": calls.is_none_or(|f| p.start < f),
    })
}

/// 期間の単位ひとつぶん。
fn unit_json(
    unit: Unit,
    spans: &[(&str, NaiveDate, NaiveDate)],
    contacts: &Attached,
    tl: &HashMap<String, Vec<OwnerEntry>>,
    today: NaiveDate,
    cutoff: NaiveDate,
    calls: Option<NaiveDate>,
) -> Value {
    let n = match unit {
        Unit::Week => N_WEEKS,
        Unit::Month => N_MONTHS,
    };
    let ps = periods(unit, today, n);
    let mut by: BTreeMap<&str, Vec<Cell>> = BTreeMap::new();
    let mut retired: HashMap<&str, bool> = HashMap::new();
    let mut team = vec![Cell::default(); n];
    let mut und_deals = vec![0usize; n];
    let mut und_contacts = vec![0usize; n];
    let mut shared = vec![0usize; n];
    let mut moved = vec![0usize; n];
    let empty: Vec<OwnerEntry> = Vec::new();
    let no_contact: Vec<(NaiveDate, bool)> = Vec::new();

    for &(id, s, e) in spans {
        let t = tl.get(id).unwrap_or(&empty);
        let cs = contacts.get(id).unwrap_or(&no_contact);
        for (i, p) in ps.iter().enumerate() {
            // 契約期間と期間の重なり。今日より先はまだ来ていない
            let a = p.start.max(s);
            let b = p.end.min(e).min(today);
            if a > b {
                continue;
            }
            // 重なりの中の接触（cs は昇順）
            let lo = cs.partition_point(|x| x.0 < a);
            let hi = cs.partition_point(|x| x.0 <= b);
            let inside = &cs[lo..hi];

            let owners = owners_during(t, a, b);
            if owners.is_empty() {
                // 🔴 履歴で担当が決められない。どの担当者にも数えない
                und_deals[i] += 1;
                und_contacts[i] += inside.len();
                continue;
            }
            team[i].deals += 1;
            if owners.len() > 1 {
                shared[i] += 1;
            }
            for o in &owners {
                by.entry(o).or_insert_with(|| vec![Cell::default(); n])[i].deals += 1;
            }
            for &(d, mv) in inside {
                match owner_at(t, d) {
                    Some(o) => {
                        if let Some(c) = by.get_mut(o.1.as_str()) {
                            c[i].contacts += 1;
                        }
                        team[i].contacts += 1;
                        if mv {
                            moved[i] += 1;
                        }
                    }
                    // 期間の途中で履歴が始まった案件の、始まる前の接触
                    None => und_contacts[i] += 1,
                }
            }
        }
        for e in t {
            *retired.entry(e.1.as_str()).or_insert(false) |= e.2;
        }
    }

    let pjs: Vec<Value> = ps
        .iter()
        .map(|p| period_json(unit, p, cutoff, calls))
        .collect();
    // 並びは「直近の確定した期間の持ち案件が多い順」。成績の順ではない
    let last_fixed = ps.iter().rposition(|p| p.end < cutoff);
    let mut names: Vec<&&str> = by.keys().collect();
    names.sort_by(|x, y| {
        let k = |nm: &str| last_fixed.map(|i| by[nm][i].deals).unwrap_or(0);
        k(y).cmp(&k(x)).then_with(|| x.cmp(y))
    });
    let rows: Vec<Value> = names
        .iter()
        .map(|nm| {
            let cells = &by[**nm];
            json!({
                "consultant": nm,
                "retired": retired.get(**nm).copied().unwrap_or(false),
                "cells": cells.iter().map(|c| c.json()).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "periods": pjs,
        "rows": rows,
        "team": team.iter().map(|c| c.json()).collect::<Vec<_>>(),
        "undetermined": (0..n).map(|i| json!({
            "deals": und_deals[i],
            "contacts": und_contacts[i],
        })).collect::<Vec<_>>(),
        "shared": shared,
        // 付いていた取引から、その日に動いていた同じ拠点の本体案件へ付け直して数えた接触
        //（担当が決まった分。上の team.contacts の内数）
        "moved": moved,
    })
}

/// ②コンサルタント →「担当者ごとの接触」。月と週の両方を1回で返す（画面の切り替えで取り直さない）。
pub fn build_contact_trend(sheets: &Sheets, today: NaiveDate) -> Value {
    let deals = deals_of(&sheets.deal);
    let (raw, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
    let all = deals_all_of(&sheets.deal);
    let tl = owner_timeline(&sheets.owner_hist);
    let cutoff = cutoff_of(sheets, today);
    let calls = call_from(&sheets.call);

    let (spans, no_span) = main_spans(&deals);
    let (contacts, n_ambiguous) = attach_contacts(&raw, &all, &spans);
    let no_history = spans
        .iter()
        .filter(|(id, _, _)| !tl.contains_key(*id))
        .count();

    json!({
        "meta": {
            "today": today.to_string(),
            "all_cached": sheets.all_cached,
            "cutoff": cutoff.to_string(),
            // 通話の記録が始まった日。これより前に始まる期間は出さない
            "call_from": calls.map(|d| d.to_string()),
            "min_deals": MIN_DEALS,
            // 契約の開始日・満了日が読めず、どの期間にも数えられない本体案件
            "n_no_span": no_span,
            // 担当履歴が1行も無い本体案件（どの期間でも担当が決められない）
            "n_no_history": no_history,
            // 同じ拠点で契約期間の重なる本体案件が2件以上あり、付け先を決められず数えなかった接触
            "n_ambiguous": n_ambiguous,
            "not_counted": "※ 接触は検知専用です。多いほど良いという評価ではありません（担当者の評価ではありません）。\
    もめている案件ほど電話が増えることもあり、接触の多い少ないが良い悪いのどちらに向くかは、このデータでは決まっていません",
        },
        "contact_rule": "接触 ＝ MTG または60秒超の通話（メールは数えない）。通話の日付は日本時間。\
    数えるのは契約期間の中の接触だけです",
        "attach_rule": "通話は、その日に動いていた契約ではなく、あとから作られた継続の取引（まだ始まっていない契約）や\
    オプションの取引に付いていることがよくあります。付いている取引の契約期間だけで数えると、継続の前の月ほど\
    通話が「契約の前」として落ち、昔の月ほど低く出て、確定した月もあとで下がります。\
    そこでこの画面では、付いている取引の契約期間の外の接触を、その日に契約期間の中にある同じ拠点の本体案件が\
    1件だけあれば、その案件に付け直して数えています（担当者の一覧の接触率は付け直していないので、数が違います）。\
    同じ拠点に動いている案件が無い日（初めての契約の前・契約と契約のすき間）と、2件以上あって決められない日の接触は数えていません。\
    別の拠点の取引に付いた通話は直せないので、確定した期間でも、その分はあとで少し動くことがあります",
        "denom_rule": format!(
            "1件あたりの接触 ＝ その期間の接触の回数 ÷ その期間に担当として持っていた案件の数。\
    持っていた案件は、オプション契約を除く本体の案件のうち、契約期間（開始日〜満了日）がその期間に1日でも重なるものです。\
    期間の途中で始まった・終わった案件も1件と数えます（日割りにしていません）。\
    画面の頭の「稼働中」の件数（いまの稼働の印で数えたもの）とは別の数え方です。\
    過去の期間に持っていたかはいまの稼働の印では決まらないので、契約期間で数えています\
    （いま稼働中でも契約期間の外の案件や、契約期間の中でも稼働の印が外れた案件があり、件数がそろうとは限りません）。\
    満了日より前に解約・充足へ移った案件も、満了日までは持っていたと数えています（ステージが書き換わった日は、手を離した日とは限らないため）。\
    持ち案件が0件の期間は空欄です（0 ではありません）。持ち案件が {MIN_DEALS} 件未満の期間は印を付け、図には点を打っていません"
        ),
        "owner_rule": "担当は consultant が正本です（hubspot_owner_id ではありません）。\
    担当履歴から、その日に有効だった担当を引いています（同じ日に複数行あれば、シートで後に来る行）。\
    期間の途中で担当が替わった案件は、替わる前と後の両方の担当に1件ずつ数え、接触はその日の担当に数えています。\
    担当の替わり目は担当欄が書き換えられた日で、実際の引き継ぎ日とはずれることがあります",
        "undetermined_rule": "担当履歴の最初の行より前の日は、担当が決められません。\
    最初の担当にさかのぼって付けることはせず、どの担当者にも数えていません（件数は別に出しています）",
        "provisional_rule": "いまの週・月は途中なので未確定です（途中までの接触しか入っていないので低く出ます）。\
    データを取った日がそれより前なら、その日を含む期間から先も未確定にしています",
        "calls_missing_rule": "通話の記録が始まる前の期間は、MTG しか数えられないので出していません。\
    出すと、データの範囲が変わっただけなのに接触が増えたように見えるためです",
        "month": unit_json(Unit::Month, &spans, &contacts, &tl, today, cutoff, calls),
        "week": unit_json(Unit::Week, &spans, &contacts, &tl, today, cutoff, calls),
    })
}
