//! ②コンサルタント →「担当の交代」の、**交代の前後の接触**。
//!
//! 2026-09-24 藤巻さんの要望:「担当が変わったときに接触率が落ちたのか減ったのか、担当ごとに見たい」。
//! 交代ごとに、**前の担当がその拠点を持っていた期間の全体**と、**後の担当が持っている期間の全体**
//! （通期）の **30日あたりの接触回数** を比べ、増えた / 減った / 変わらない を出す。
//! 担当者ごとに、引き継いだ側（to）と引き継がれた側（from）で件数と変化の平均・中央値をまとめる。
//!
//! ------------------------------------------------------------------
//! 決めたこと（画面にも同じことを書いている）
//! ------------------------------------------------------------------
//! - 🔴 **交代が接触を減らした・増やした証拠ではない。** 危ない案件だから担当を替えた可能性もあり、
//!   向きは決まらない。過去に同じ問いを偽の比較対象（同じ取引の180日前に置いた偽の交代日）と並べたら、
//!   前の量をそろえた層で差が消え、層を上げると逆転した（引き継ぎ資料 06 の失敗9）。
//!   画面の頭にこの断りを出す。色も「減った＝赤」のように良し悪しで塗らない（接触は検知専用）。
//! - **接触の定義は作り直さない。** `contacts_by_deal`（MTG または60秒超の通話。メールは数えない。
//!   通話は日本時間）を、「担当者ごとの接触」と同じ `attach_contacts` で付け直したものを数える
//!   （関数を共用している。`main_spans` も同じもの）。
//! - **窓 ＝ それぞれの担当の期間の全体（通期）。** 2026-09-24 藤巻さんの判断
//!   （「平均か最頻、中央値で良い。通期のね」）で、前後60日から変えた。
//!   - 前 ＝ 同じ拠点の**一つ前の交代日**から、この交代日の前日まで。一つ前の交代が無ければ、
//!     その拠点で数えられる最初の日から（下の「窓に入れる日」で切り詰める）。
//!   - 後 ＝ この交代日から、同じ拠点の**次の交代日の前日**まで。次の交代が無ければ締め日の前日まで。
//!   - 交代日は `CS_担当交代` の `date`（MTG のホストが替わった日＝新しい担当の最初の MTG）で、後の側に入れる。
//!   - 🔴 **`CS_担当履歴`（HubSpot の担当者欄の履歴）では区切らない。** 依頼では「担当履歴から前の担当・
//!     後の担当の区間を取る」だったが、fixture（基準日 2026-09-18）で拠点の担当（その日に動いている本体案件の
//!     consultant）を引くと、241件の交代のうち 171件で交代日の前日・交代日・30日後が同じ人のまま。
//!     担当者欄が替わるのは交代日＋`record_gap_days`（322行中 216行でその日に変更の行がある。
//!     差は −116〜+203日、中央値 −5日）で、実際に替わった日とずれる。
//!     fixture の `from` / `to` は連番に伏せてあり、担当履歴の名前と突き合わせもできない。
//!     そこで区切りはすべて交代の記録（実際に替わった日）で取る。
//! - **窓に入れる日 ＝ その日に同じ拠点（`kyoten_key`）で契約期間の中にある本体案件が1件だけの日**
//!   （付け直しの決まりと同じ `Ctx::running`）。その日の接触は、その本体案件に付け直された接触を数える。
//!   拠点キーが空の取引は、その取引の契約期間の中の日だけ（交代もその取引の中で並べる）。
//! - 🔴 **同じ交代は1件と数える。** 交代 ＝ `(拠点キー, 交代日)`（拠点キーが空なら `(取引, 交代日)`）。
//!   `CS_担当交代` は同じ交代を拠点の取引ごとに1行ずつ書いてくる（前の契約・いまの契約・まだ始まって
//!   いない継続の契約、それぞれに同じ日の行がある）ので、行で数えると同じ交代を2〜9回数える
//!   （fixture で 357行が 241件の交代になる）。表の行には交代ごとの値をそのまま付ける。
//!   「一つ前・次の交代」も、この交代（拠点×交代日）の並びで決める。
//! - **通話の記録が始まる前の日は窓に入れない**（`call_from`。fixture で 2026-03-23）。
//!   入れると MTG しか数えられない日が混ざり、前の窓だけ少なく出る（「担当者ごとの接触」と同じ理由）。
//! - **データを取った日（`cutoff`）から先の日は窓に入れない。** 取った日そのものも途中までしか
//!   入っていないので、数えるのは `cutoff` の前日（`last`）まで。
//! - **後の担当がまだ担当中**（次の交代が無く、締め日にも同じ拠点で本体案件が動いている）の交代は、
//!   後の窓が締め日の前日で止まる。通期なので今も続くのは普通で、30日以上あれば比べる（`ongoing`。
//!   画面に「後の担当はまだ担当中（締め日までの通期）」と書く）。30日に届かないものだけ
//!   **途中**（`provisional`・未確定）として、まとめから外す。
//! - **どちらかの窓が30日未満なら「比べるには短い」**（`short`）。まとめの件数には入れず、
//!   理由ごとに分けて数える（交代どうしの間が短い / 通話の記録が始まる前にかかる /
//!   同じ拠点で本体案件が重なる / 前に動いていた案件が無い / 後に動いていた案件が無い）。
//!   - 窓そのもの（一つ前の交代〜前日、交代日〜次の交代の前日。締め日の前日で止める）が30日に
//!     届かないなら「交代どうしの間が短い」（`gap`）。
//!   - それ以外は、窓から**外した日を理由ごとに数え、いちばん多い理由**（同じ日数なら
//!     通話 → 重なり → 案件が無い の順）。外した日の理由は、その日に動いている本体案件が 0件なら
//!     「案件が無い」、2件以上なら「重なり」、1件で通話の記録の前なら「通話」。
//!   - 一つ前の交代が無い前の窓は、拠点の最初の契約の開始日と通話の記録の始まりの早いほうから見る
//!     （その間の日が、理由ごとの外した日になる）。
//!   - 重なりで外した日数も交代ごとに `lost.overlap` として出し、まとめで合計する（黙って外さない）。
//! - 増えた / 減った / 変わらない は、30日あたりの値を**分数のまま**比べる（小数の丸めで「変わらない」を作らない）。
//!   わずかな差でも増えた・減ったに入る（2026-09-24 藤巻さんの判断で今のまま）。大きさは変化（後−前、30日あたり）で見る。
//! - **まとめの数字は変化の平均と中央値。** 最頻値は出さない（30日あたりの回数は日数で割った値で、
//!   同じ値がほとんど重ならず、最頻値に意味が無いため）。
//! - **担当者ごとのまとめ**は比べられた交代（`ok`）だけで作る。同じ人の同じ交代は1件。
//!   母数が `MIN_PERSON_N` 件未満の人には印（`small`）。画面は図に出さず、表に残す。
//!   担当の表示名は交代の表と同じ `person_label`（メールアドレスを名前として出さない）。
//!   🔴 fixture の `from` / `to` は1行1人の連番に伏せてあるので、fixture では全員が1件になる。
//!   まとめ方の正しさは tests.rs の合成データで見ている（本番は実名なので人ごとにまとまる）。

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};

use super::contact_trend::{attach_contacts, call_from, cutoff_of, main_spans, Attached};
use super::routes::{median_of, person_label};
use super::{contacts_by_deal, Deal, Sheets};

/// これより短い窓は「比べるには短い」。
pub const MIN_WINDOW_DAYS: i64 = 30;
/// 「30日あたり」の30。
pub const PER_DAYS: i64 = 30;
/// 担当者のまとめで、これより少ない件数には印を付ける（図に出さない）。
pub const MIN_PERSON_N: usize = 5;

/// 1つの窓で数えたもの。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    /// 窓に入れた日数
    pub days: i64,
    /// その日々の接触の回数
    pub contacts: usize,
    /// 窓に入れた最初の日と最後の日（日数0なら空）。間に外した日があれば、日数はこの間より短い
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    /// 窓そのものの長さ（交代どうし・締め日で区切った日数。外した日も含む）
    pub span: i64,
    /// 外した日: 本体案件は1件だが、通話の記録が始まる前
    pub lost_calls: i64,
    /// 外した日: 同じ拠点で契約期間の中の本体案件が2件以上（付け先を決められない）
    pub lost_overlap: i64,
    /// 外した日: 動いている本体案件が無い
    pub lost_none: i64,
}

impl Window {
    /// 30日あたりの接触。日数0は空（0 にしない）。
    pub fn per30(self) -> Option<f64> {
        (self.days > 0).then(|| self.contacts as f64 * PER_DAYS as f64 / self.days as f64)
    }
    fn json(self) -> Value {
        json!({
            "days": self.days,
            "contacts": self.contacts,
            "per30": self.per30(),
            // 窓に入れた最初の日・最後の日（`days` はこの間で数えた日の数）
            "start": self.start.map(|d| d.to_string()),
            "end": self.end.map(|d| d.to_string()),
            // 窓から外した日を理由ごとに
            "lost": {"calls": self.lost_calls, "overlap": self.lost_overlap, "none": self.lost_none},
        })
    }
    /// 短い窓の理由。窓の端が隣の交代で区切られ（`bounded`）、窓そのものが短ければ `ShortGap`。
    /// それ以外は外した日のいちばん多い理由（同じなら 通話 → 重なり → 案件が無い）。
    /// `none` は案件が無いときの理由（前の窓なら `ShortBefore`、後の窓なら `ShortAfter`）。
    fn short_reason(self, bounded: bool, none: Status) -> Status {
        if bounded && self.span < MIN_WINDOW_DAYS {
            return Status::ShortGap;
        }
        let mut best = (self.lost_calls, Status::ShortCalls);
        for c in [
            (self.lost_overlap, Status::ShortOverlap),
            (self.lost_none, none),
        ] {
            if c.0 > best.0 {
                best = c;
            }
        }
        best.1
    }
}

/// 比べられたか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// 窓が短く、同じ拠点の交代どうしの間そのものが30日未満
    ShortGap,
    /// 窓が短く、外した日のいちばん多い理由が「通話の記録が始まる前」
    ShortCalls,
    /// 窓が短く、外した日のいちばん多い理由が「同じ拠点で本体案件が重なる」
    ShortOverlap,
    /// 前の窓が短い（前に動いていた本体案件が無い。初めての契約の直後など）
    ShortBefore,
    /// 後の窓が短い（後に動いていた本体案件が無い。満了・解約など）
    ShortAfter,
    /// 後の担当がまだ担当中で、後の窓がまだ30日に届かない（途中）
    Provisional,
}

impl Status {
    fn code(self) -> (&'static str, Option<&'static str>) {
        match self {
            Status::Ok => ("ok", None),
            Status::ShortGap => ("short", Some("gap")),
            Status::ShortCalls => ("short", Some("calls")),
            Status::ShortOverlap => ("short", Some("overlap")),
            Status::ShortBefore => ("short", Some("before")),
            Status::ShortAfter => ("short", Some("after")),
            Status::Provisional => ("provisional", None),
        }
    }
}

/// 交代1件の前後。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cmp {
    pub status: Status,
    pub before: Window,
    pub after: Window,
    /// 後の担当がまだ担当中（次の交代が無く、締め日にも本体案件が動いている）。後の窓は締め日の前日まで
    pub ongoing: bool,
}

impl Cmp {
    /// 後−前（30日あたり）。どちらかの日数が0なら空。
    pub fn change(&self) -> Option<f64> {
        Some(self.after.per30()? - self.before.per30()?)
    }
    /// 増えた / 減った / 変わらない。分数のまま比べる（丸めない）。
    pub fn dir(&self) -> Option<&'static str> {
        if self.before.days == 0 || self.after.days == 0 {
            return None;
        }
        let a = self.after.contacts as i64 * self.before.days;
        let b = self.before.contacts as i64 * self.after.days;
        Some(match a.cmp(&b) {
            std::cmp::Ordering::Greater => "up",
            std::cmp::Ordering::Less => "down",
            std::cmp::Ordering::Equal => "same",
        })
    }
    pub fn json(&self) -> Value {
        let (status, why) = self.status.code();
        json!({
            "status": status,
            "why": why,
            "before": self.before.json(),
            "after": self.after.json(),
            "ongoing": self.ongoing,
            "change": self.change(),
            "dir": self.dir(),
        })
    }
}

/// 前後を数えるための材料。1回の応答で1つ作る。
pub struct Ctx<'a> {
    /// 本体案件の契約期間
    own: HashMap<&'a str, (NaiveDate, NaiveDate)>,
    /// 本体案件の拠点キー（空は入れない）
    site: HashMap<&'a str, &'a str>,
    /// 拠点ごとの本体案件の契約期間
    by_site: HashMap<&'a str, Vec<(&'a str, NaiveDate, NaiveDate)>>,
    /// 付け直したあとの接触（`attach_contacts`）
    contacts: Attached,
    /// 交代の並び（`group_key` → 交代日の昇順・重複なし）。`set_events` で入れる
    events: HashMap<String, Vec<NaiveDate>>,
    /// 通話の記録が始まった日。これより前の日は数えない
    pub call_from: Option<NaiveDate>,
    /// 数える最後の日（データを取った日の前日）
    pub last: NaiveDate,
    /// 付け先を決められず数えなかった接触（`attach_contacts` の2つめ）
    pub n_ambiguous: usize,
}

impl<'a> Ctx<'a> {
    /// `deals` はオプション除外（`deals_of`）、`all` はオプション込み（`deals_all_of`）。
    pub fn new(sheets: &Sheets, today: NaiveDate, deals: &'a [Deal], all: &[Deal]) -> Self {
        let (spans, _) = main_spans(deals);
        let (raw, _, _) = contacts_by_deal(&sheets.call, &sheets.mtg);
        let (contacts, n_ambiguous) = attach_contacts(&raw, all, &spans);
        let site: HashMap<&'a str, &'a str> = deals
            .iter()
            .map(|d| (d.id.as_str(), d.kyoten_key.trim()))
            .filter(|(_, k)| !k.is_empty())
            .collect();
        let mut by_site: HashMap<&'a str, Vec<(&'a str, NaiveDate, NaiveDate)>> = HashMap::new();
        for &(id, s, e) in &spans {
            if let Some(k) = site.get(id) {
                by_site.entry(k).or_default().push((id, s, e));
            }
        }
        Ctx {
            own: spans.iter().map(|&(id, s, e)| (id, (s, e))).collect(),
            site,
            by_site,
            contacts,
            events: HashMap::new(),
            call_from: call_from(&sheets.call),
            last: cutoff_of(sheets, today) - Duration::days(1),
            n_ambiguous,
        }
    }

    /// 契約期間が読める本体案件か（読めなければ前後を数えられない）。
    pub fn has_span(&self, deal: &str) -> bool {
        self.own.contains_key(deal)
    }

    /// 交代を並べる単位。拠点キー、空なら取引。
    fn group_key(&self, deal: &str) -> String {
        match self.site.get(deal) {
            Some(k) => format!("site:{k}"),
            None => format!("deal:{deal}"),
        }
    }

    /// 交代を1件にまとめる鍵。`(拠点キー, 交代日)`、拠点キーが空なら `(取引, 交代日)`。
    pub fn event_key(&self, deal: &str, day: NaiveDate) -> String {
        format!("{}|{day}", self.group_key(deal))
    }

    /// 比べる交代をすべて入れる（`(取引, 交代日)`。`has_span` が真のものだけ渡す）。
    /// 一つ前・次の交代はこの並びで決める。同じ拠点・同じ日は1件。
    pub fn set_events<'b>(&mut self, it: impl IntoIterator<Item = (&'b str, NaiveDate)>) {
        let mut ev: HashMap<String, Vec<NaiveDate>> = HashMap::new();
        for (deal, day) in it {
            ev.entry(self.group_key(deal)).or_default().push(day);
        }
        for v in ev.values_mut() {
            v.sort();
            v.dedup();
        }
        self.events = ev;
    }

    /// 拠点（拠点キーが空なら取引）の最初の契約の開始日。
    fn first_start(&self, deal: &str) -> Option<NaiveDate> {
        match self.site.get(deal) {
            Some(k) => self
                .by_site
                .get(k)
                .into_iter()
                .flatten()
                .map(|&(_, s, _)| s)
                .min(),
            None => self.own.get(deal).map(|&(s, _)| s),
        }
    }

    /// `day` に動いている本体案件。同じ拠点で契約期間の中の本体案件が1件だけなら `One`
    /// （その日は窓に入れる）。0件・2件以上はその日を窓に入れない。
    fn running(&self, deal: &str, day: NaiveDate) -> Run<'_> {
        match self.site.get(deal) {
            Some(k) => {
                let mut hit = self
                    .by_site
                    .get(k)
                    .into_iter()
                    .flatten()
                    .filter(|&&(_, s, e)| s <= day && day <= e);
                match (hit.next(), hit.next()) {
                    (Some(&(id, _, _)), None) => Run::One(id),
                    (Some(_), Some(_)) => Run::Many,
                    _ => Run::Zero,
                }
            }
            None => self
                .own
                .get_key_value(deal)
                .filter(|(_, &(s, e))| s <= day && day <= e)
                .map_or(Run::Zero, |(&id, _)| Run::One(id)),
        }
    }

    /// `a`〜`b`（両端を含む。`b` は締め日の前日で止める）のうち窓に入れる日を数え、その日々の接触を足す。
    /// 入れなかった日は理由ごとに数える（`lost_*`）。
    fn count(&self, deal: &str, a: NaiveDate, b: NaiveDate) -> Window {
        let b = b.min(self.last);
        let mut w = Window {
            span: ((b - a).num_days() + 1).max(0),
            ..Window::default()
        };
        let mut d = a;
        while d <= b {
            match self.running(deal, d) {
                Run::Zero => w.lost_none += 1,
                Run::Many => w.lost_overlap += 1,
                // 通話の記録が1本も無い（call_from が無い）ときも、どの日も比べられない
                Run::One(_) if self.call_from.is_none_or(|f| d < f) => w.lost_calls += 1,
                Run::One(id) => {
                    w.days += 1;
                    w.start.get_or_insert(d);
                    w.end = Some(d);
                    if let Some(cs) = self.contacts.get(id) {
                        // cs は日の昇順
                        let lo = cs.partition_point(|x| x.0 < d);
                        let hi = cs.partition_point(|x| x.0 <= d);
                        w.contacts += hi - lo;
                    }
                }
            }
            d += Duration::days(1);
        }
        w
    }

    /// 交代1件（取引 `deal`・交代日 `day`）の前後。`deal` は `has_span` が真のものを渡す。
    /// 一つ前・次の交代は `set_events` で入れた並びから引く（入れていなければ、前後に交代は無いものとする）。
    pub fn compare(&self, deal: &str, day: NaiveDate) -> Cmp {
        let evs = self
            .events
            .get(&self.group_key(deal))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let prev = evs.iter().rev().find(|&&d| d < day).copied();
        let next = evs.iter().find(|&&d| d > day).copied();
        // 前: 一つ前の交代日から。無ければ、拠点の最初の契約の開始日と通話の記録の始まりの早いほうから
        let start = prev.unwrap_or_else(|| {
            let f = self.first_start(deal).unwrap_or(day);
            self.call_from.map_or(f, |c| f.min(c))
        });
        let before = self.count(deal, start, day - Duration::days(1));
        // 後: 次の交代日の前日まで。無ければ締め日の前日まで
        let after = self.count(deal, day, next.map_or(self.last, |n| n - Duration::days(1)));
        // 後の担当がまだ担当中: 次の交代が無く、締め日にも同じ拠点で本体案件が1件動いている
        let ongoing = next.is_none()
            && matches!(
                self.running(deal, self.last + Duration::days(1)),
                Run::One(_)
            );
        let status = if before.days < MIN_WINDOW_DAYS {
            before.short_reason(prev.is_some(), Status::ShortBefore)
        } else if after.days < MIN_WINDOW_DAYS {
            if ongoing {
                Status::Provisional
            } else {
                after.short_reason(next.is_some(), Status::ShortAfter)
            }
        } else {
            Status::Ok
        };
        Cmp {
            status,
            before,
            after,
            ongoing,
        }
    }
}

/// その日に同じ拠点で契約期間の中にある本体案件（`Ctx::running`）。
enum Run<'a> {
    Zero,
    One(&'a str),
    Many,
}

/// 交代の表の1行ぶん（まとめに使うもの）。
pub struct Item<'r> {
    /// `Ctx::event_key`
    pub event: String,
    pub cmp: Cmp,
    /// シートの値そのまま（担当者一覧に無い人はメールアドレス）
    pub from: &'r str,
    pub to: &'r str,
}

/// 平均。1件も無ければ空（0 にしない）。
fn mean_of(vs: &[f64]) -> Option<f64> {
    (!vs.is_empty()).then(|| vs.iter().sum::<f64>() / vs.len() as f64)
}

/// 比べられた交代の件数と向き。
#[derive(Default)]
struct Tally {
    n_events: usize,
    n_ok: usize,
    n_up: usize,
    n_down: usize,
    n_same: usize,
    n_short: usize,
    n_provisional: usize,
    /// 比べられたうち、後の担当がまだ担当中のもの
    n_ongoing: usize,
    changes: Vec<f64>,
}

impl Tally {
    fn add(&mut self, c: &Cmp) {
        self.n_events += 1;
        match c.status {
            Status::Ok => {
                self.n_ok += 1;
                if c.ongoing {
                    self.n_ongoing += 1;
                }
                match c.dir() {
                    Some("up") => self.n_up += 1,
                    Some("down") => self.n_down += 1,
                    _ => self.n_same += 1,
                }
                if let Some(x) = c.change() {
                    self.changes.push(x);
                }
            }
            Status::Provisional => self.n_provisional += 1,
            _ => self.n_short += 1,
        }
    }
}

/// 交代ごと（同じ交代は1件）と、担当者ごと（引き継いだ側 / 引き継がれた側）のまとめ。
/// `unresolved_no` は氏名の分からない担当の番号（routes.rs `unresolved_numbers`。交代の表と同じ番号）。
pub fn summarize(items: &[Item], unresolved_no: &HashMap<&str, usize>) -> Value {
    // 交代ごと。同じ交代の行は同じ値なので、最初の1行で数える
    let mut events: BTreeMap<&str, &Cmp> = BTreeMap::new();
    for it in items {
        events.entry(it.event.as_str()).or_insert(&it.cmp);
    }
    let mut all = Tally::default();
    let mut why = [0usize; 5];
    // 同じ拠点で本体案件が重なり、窓から外した日（交代ごとに1回、前後の合計）
    let mut overlap_days = 0i64;
    let mut overlap_events = 0usize;
    for c in events.values() {
        all.add(c);
        match c.status {
            Status::ShortCalls => why[0] += 1,
            Status::ShortBefore => why[1] += 1,
            Status::ShortAfter => why[2] += 1,
            Status::ShortOverlap => why[3] += 1,
            Status::ShortGap => why[4] += 1,
            _ => {}
        }
        let o = c.before.lost_overlap + c.after.lost_overlap;
        if o > 0 {
            overlap_days += o;
            overlap_events += 1;
        }
    }

    json!({
        "n_events": all.n_events,
        "n_ok": all.n_ok,
        "n_up": all.n_up,
        "n_down": all.n_down,
        "n_same": all.n_same,
        "n_short": all.n_short,
        "n_provisional": all.n_provisional,
        // 比べられたうち、後の担当がまだ担当中（後の窓は締め日の前日まで）
        "n_ongoing": all.n_ongoing,
        // 短い理由ごとの件数（calls / before / after / overlap / gap）
        "short_why": {"calls": why[0], "before": why[1], "after": why[2], "overlap": why[3], "gap": why[4]},
        // 重なりで窓から外した日数と、その日がある交代の件数（比べられた交代も含む）
        "overlap_days": overlap_days,
        "overlap_events": overlap_events,
        // 変化（後−前、30日あたり）の平均と中央値。最頻値は出さない（モジュールの頭）
        "mean_change": mean_of(&all.changes),
        "median_change": median_of(all.changes),
        "by_to": side(items, |it| it.to, unresolved_no),
        "by_from": side(items, |it| it.from, unresolved_no),
    })
}

/// 担当者ごと（`pick` で引き継いだ側か引き継がれた側かを選ぶ）。
fn side<'r>(
    items: &[Item<'r>],
    pick: fn(&Item<'r>) -> &'r str,
    unresolved_no: &HashMap<&str, usize>,
) -> Vec<Value> {
    // 人ごと・交代ごとに1件（同じ人の同じ交代を行の数だけ数えない）
    let mut seen: BTreeMap<(&str, &str), &Cmp> = BTreeMap::new();
    for it in items {
        let p = pick(it).trim();
        if p.is_empty() {
            continue; // 担当が空。誰にも数えない
        }
        seen.entry((p, it.event.as_str())).or_insert(&it.cmp);
    }
    let mut by: BTreeMap<&str, Tally> = BTreeMap::new();
    for ((p, _), c) in &seen {
        by.entry(p).or_default().add(c);
    }
    let mut rows: Vec<(&str, Tally)> = by.into_iter().collect();
    // 並びは比べられた件数の多い順（成績の順ではない）
    rows.sort_by(|x, y| {
        y.1.n_ok
            .cmp(&x.1.n_ok)
            .then(y.1.n_events.cmp(&x.1.n_events))
            .then(x.0.cmp(y.0))
    });
    rows.into_iter()
        .map(|(p, t)| {
            let unresolved = super::routes::is_mail(p);
            json!({
                "label": person_label(p),
                "unresolved": unresolved,
                // 氏名の分からない担当が何人かいるとき、画面で見分ける番号（1から）。
                // 🔴 この側の並びで振らない。交代の表・もう一方の側と同じ番号（`unresolved_no`）
                "unresolved_no": if unresolved { unresolved_no.get(p).copied() } else { None },
                "n_events": t.n_events,
                "n_ok": t.n_ok,
                "n_up": t.n_up,
                "n_down": t.n_down,
                "n_same": t.n_same,
                "n_short": t.n_short,
                "n_provisional": t.n_provisional,
                "n_ongoing": t.n_ongoing,
                "mean_change": mean_of(&t.changes),
                "median_change": median_of(t.changes),
                "small": t.n_ok < MIN_PERSON_N,
            })
        })
        .collect()
}
