//! Indeed 採用市場データの読み出し。
//!
//! # ここが唯一の入口
//! 社内タブ（`/tab/indeed`）と顧客レポート（`/report/indeed`）は、
//! どちらもこのモジュールが返す [`Snapshot`] だけを見る。
//! 画面ごとに SQL を書くと、社内で話した数字と顧客に渡した数字が食い違う。
//!
//! # 元データ
//! `data/indeed_insights.db`（分析層・8 テーブル）。
//! 生データ `indeed_market_api.db`（125MB）はアプリに載せない。
//!
//! # 読み込みは 1 回だけ
//! DB は配布物で、動いている間は変わらない。起動後に一度読んで持ち続ける。
//! 毎リクエストで 51,787 行を読み直す理由がない。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64_opt, get_str};

/// 出どころと但し書き。顧客レポートに必ず出す。
#[derive(Debug, Clone, Default)]
pub struct Meta {
    /// 揃っている月。古い順
    pub months: Vec<String>,
    /// いちばん新しい月
    pub latest: String,
    /// 元データの名前
    pub source: String,
    /// 読むときの注意。これを省いて配らない
    pub caveat: String,
    /// 集計した日
    pub built_at: String,
}

/// 職種 1 つ。
#[derive(Debug, Clone)]
pub struct Title {
    pub name: String,
    /// 20 ある分類のどれか
    pub category: String,
    /// 全部の月に数字があるか。
    ///
    /// 途中から取り始めた職種を合計に混ぜると、母集団が月によって変わる。
    /// 実際、2026-08 から 21 職種を取り始めたときに、全国の先月比が
    /// -9.4% であるべきところ -2.5% に見えた（6.9 ポイントのずれ）。
    /// 合計はこれが true のものだけで出す。
    pub complete: bool,
}

/// 月ごとの並び。長さは [`Meta::months`] と必ず同じで、欠測は `None`。
///
/// 欠測を 0 で埋めない。0 件と「その月は取れていない」は別のことで、
/// 混ぜると傾向線が実際より急に見える。
#[derive(Debug, Clone, Default)]
pub struct Series {
    /// 求人数
    pub job: Vec<Option<f64>>,
    /// 求人を見た人数（クリック）。応募数ではない
    pub ctk: Vec<Option<f64>>,
    /// 募集している企業の数
    pub emp: Vec<Option<f64>>,
}

impl Series {
    /// 空の並びを作る。集計側から使う。
    pub fn blank_public(n: usize) -> Self {
        Self::blank(n)
    }

    fn blank(n: usize) -> Self {
        Self {
            job: vec![None; n],
            ctk: vec![None; n],
            emp: vec![None; n],
        }
    }

    /// 1 求人あたり何人が見たか。求人数が 0 の月は出さない。
    pub fn seekers_per_posting(&self) -> Vec<Option<f64>> {
        self.job
            .iter()
            .zip(self.ctk.iter())
            .map(|(j, c)| match (j, c) {
                (Some(j), Some(c)) if *j > 0.0 => Some(c / j),
                _ => None,
            })
            .collect()
    }

    /// 1 社あたり何件の求人を出しているか。
    pub fn postings_per_employer(&self) -> Vec<Option<f64>> {
        self.job
            .iter()
            .zip(self.emp.iter())
            .map(|(j, e)| match (j, e) {
                (Some(j), Some(e)) if *e > 0.0 => Some(j / e),
                _ => None,
            })
            .collect()
    }

    /// 全期間の合計。1 つも取れていなければ `None`。
    pub fn total(v: &[Option<f64>]) -> Option<f64> {
        let mut sum = 0.0;
        let mut any = false;
        for x in v.iter().flatten() {
            sum += *x;
            any = true;
        }
        if any {
            Some(sum)
        } else {
            None
        }
    }

    /// 足し合わせる。欠測どうしは欠測のまま残す。
    fn add_into(dst: &mut [Option<f64>], src: &[Option<f64>]) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            if let Some(s) = s {
                *d = Some(d.unwrap_or(0.0) + *s);
            }
        }
    }

    /// 足し合わせる。集計側から使う。
    pub fn merge_public(&mut self, other: &Series) {
        self.merge(other);
    }

    fn merge(&mut self, other: &Series) {
        Self::add_into(&mut self.job, &other.job);
        Self::add_into(&mut self.ctk, &other.ctk);
        Self::add_into(&mut self.emp, &other.emp);
    }
}

/// 都道府県 1 つ分の、ある職種の並び。
#[derive(Debug, Clone)]
pub struct PrefSeries {
    pub prefecture: String,
    pub title: String,
    pub series: Series,
}

/// 一度読んだら変えない、データのかたまり。
#[derive(Debug, Default)]
pub struct Snapshot {
    pub meta: Meta,
    /// 104 職種
    pub titles: Vec<Title>,
    /// 職種名 → 全国の並び
    pub by_title: HashMap<String, Series>,
    /// 分類名 → その分類に属する職種名
    pub category_titles: HashMap<String, Vec<String>>,
    /// 分類名 → 全国の並び
    pub by_category: HashMap<String, Series>,
    /// 全職種を足した全国の並び
    pub nation: Series,
    /// 都道府県 × 職種
    pub by_pref: Vec<PrefSeries>,
}

impl Snapshot {
    /// 月の数。
    pub fn n_months(&self) -> usize {
        self.meta.months.len()
    }

    /// 合計に入っている職種の数（全期間そろっているもの）。
    pub fn complete_titles(&self) -> usize {
        self.titles.iter().filter(|t| t.complete).count()
    }

    /// 最新月の値を取り出す。
    pub fn last_of(v: &[Option<f64>]) -> Option<f64> {
        v.last().copied().flatten()
    }

    /// 分類名を、最新月の求人数が多い順に返す。
    pub fn categories_by_size(&self) -> Vec<String> {
        let mut v: Vec<(String, f64)> = self
            .by_category
            .iter()
            .map(|(k, s)| (k.clone(), Self::last_of(&s.job).unwrap_or(0.0)))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.into_iter().map(|(k, _)| k).collect()
    }

    /// 職種を、最新月の求人数が多い順に返す。
    pub fn titles_by_size(&self) -> Vec<&Title> {
        let mut v: Vec<&Title> = self.titles.iter().collect();
        v.sort_by(|a, b| {
            let ja = self
                .by_title
                .get(&a.name)
                .and_then(|s| Self::last_of(&s.job))
                .unwrap_or(0.0);
            let jb = self
                .by_title
                .get(&b.name)
                .and_then(|s| Self::last_of(&s.job))
                .unwrap_or(0.0);
            jb.total_cmp(&ja).then_with(|| a.name.cmp(&b.name))
        });
        v
    }

    /// 都道府県の一覧。重複なし、名前順。
    pub fn prefectures(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .by_pref
            .iter()
            .map(|p| p.prefecture.clone())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// 読み込み済みのデータ。プロセスの間ずっと使い回す。
static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();

/// 読む。2 回目以降は最初に読んだものをそのまま返す。
pub fn snapshot(db: &LocalDb) -> Result<&'static Snapshot, String> {
    if let Some(s) = SNAPSHOT.get() {
        return Ok(s);
    }
    let loaded = load(db)?;
    // 競走で先を越されても、勝った方を使えばよい（中身は同じ）
    let _ = SNAPSHOT.set(loaded);
    SNAPSHOT
        .get()
        .ok_or_else(|| "Indeed データの取り置きに失敗しました".to_string())
}

/// 取り置きを見ずに読む。テスト用。
pub fn load(db: &LocalDb) -> Result<Snapshot, String> {
    let meta = load_meta(db)?;
    let n = meta.months.len();
    if n == 0 {
        return Err("insight_meta に full_months がありません".to_string());
    }
    // 月 → 並びの何番目か
    let idx: HashMap<String, usize> = meta
        .months
        .iter()
        .enumerate()
        .map(|(i, m)| (m.clone(), i))
        .collect();

    // 1. 職種と、その分類
    let title_rows = db.query(
        "SELECT norm_title, display_category FROM insight_title ORDER BY norm_title",
        &[],
    )?;
    let mut titles = Vec::with_capacity(title_rows.len());
    for r in &title_rows {
        let name = get_str(r, "norm_title");
        let raw = get_str(r, "display_category");
        let category = if raw.is_empty() {
            "その他".to_string()
        } else {
            raw
        };
        titles.push(Title {
            name,
            category,
            complete: false, // 並びを読んだあとで判定する
        });
    }

    // 2. 全国 = 都道府県を足したもの。
    //    insight_title は全国の合計を持つが、月ごとの並びは持っていない。
    //    月次は insight_title_pref を足して作る。
    let agg = db.query(
        "SELECT norm_title, report_month, \
           SUM(job_count) AS job, SUM(ctk_count) AS ctk, SUM(employer_count) AS emp \
         FROM insight_title_pref GROUP BY norm_title, report_month",
        &[],
    )?;
    let mut by_title: HashMap<String, Series> = HashMap::new();
    for r in &agg {
        let t = get_str(r, "norm_title");
        let m = get_str(r, "report_month");
        let Some(&i) = idx.get(&m) else {
            continue; // 月の一覧にない月は捨てる（並びの長さを崩さないため）
        };
        let e = by_title.entry(t).or_insert_with(|| Series::blank(n));
        e.job[i] = get_f64_opt(r, "job");
        e.ctk[i] = get_f64_opt(r, "ctk");
        e.emp[i] = get_f64_opt(r, "emp");
    }

    // 3. 名簿を、実際に数字を持っている職種に合わせる。
    //
    //    insight_title は「最新月にデータがある職種」の名簿で、
    //    insight_title_pref は「どこかの月にデータがある職種」を持つ。
    //    前者だけを回して集計すると、最新月に落ちた職種の過去の数字が
    //    全国と分類の合計から静かに抜ける。実データで「ライン作業」が該当し、
    //    2025-11 の全国求人数が 58 件少なく出ていた。
    //    数字を持っている方に名簿を合わせる。分類が引けないものは「その他」。
    let cat_of: HashMap<String, String> = titles
        .iter()
        .map(|t| (t.name.clone(), t.category.clone()))
        .collect();
    let mut extra: Vec<Title> = by_title
        .keys()
        .filter(|k| !cat_of.contains_key(*k))
        .map(|k| Title {
            name: k.clone(),
            category: "その他".to_string(),
            complete: false,
        })
        .collect();
    extra.sort_by(|a, b| a.name.cmp(&b.name));
    titles.extend(extra);
    titles.sort_by(|a, b| a.name.cmp(&b.name));

    // 全部の月に数字があるかを判定する。合計はこれが true のものだけで作る
    for t in titles.iter_mut() {
        t.complete = by_title
            .get(&t.name)
            .map(|s| s.job.iter().all(|v| v.is_some()))
            .unwrap_or(false);
    }

    let mut by_category: HashMap<String, Series> = HashMap::new();
    let mut category_titles: HashMap<String, Vec<String>> = HashMap::new();
    let mut nation = Series::blank(n);
    for t in &titles {
        // 一覧には全部出す。合計に入れるかどうかだけを分ける
        category_titles
            .entry(t.category.clone())
            .or_default()
            .push(t.name.clone());
        if !t.complete {
            continue;
        }
        if let Some(s) = by_title.get(&t.name) {
            by_category
                .entry(t.category.clone())
                .or_insert_with(|| Series::blank(n))
                .merge(s);
            nation.merge(s);
        }
    }

    // 4. 都道府県 × 職種
    let pref_rows = db.query(
        "SELECT prefecture, norm_title, report_month, job_count AS job, \
           ctk_count AS ctk, employer_count AS emp FROM insight_title_pref",
        &[],
    )?;
    let mut pref_map: HashMap<(String, String), Series> = HashMap::new();
    for r in &pref_rows {
        let p = get_str(r, "prefecture");
        let t = get_str(r, "norm_title");
        let m = get_str(r, "report_month");
        let Some(&i) = idx.get(&m) else {
            continue;
        };
        let e = pref_map.entry((p, t)).or_insert_with(|| Series::blank(n));
        e.job[i] = get_f64_opt(r, "job");
        e.ctk[i] = get_f64_opt(r, "ctk");
        e.emp[i] = get_f64_opt(r, "emp");
    }
    let mut by_pref: Vec<PrefSeries> = pref_map
        .into_iter()
        .map(|((prefecture, title), series)| PrefSeries {
            prefecture,
            title,
            series,
        })
        .collect();
    by_pref.sort_by(|a, b| {
        a.prefecture
            .cmp(&b.prefecture)
            .then_with(|| a.title.cmp(&b.title))
    });

    Ok(Snapshot {
        meta,
        titles,
        by_title,
        category_titles,
        by_category,
        nation,
        by_pref,
    })
}

fn load_meta(db: &LocalDb) -> Result<Meta, String> {
    let rows = db.query("SELECT key, value, built_at FROM insight_meta", &[])?;
    let mut kv: HashMap<String, String> = HashMap::new();
    let mut built_at = String::new();
    for r in &rows {
        kv.insert(get_str(r, "key"), get_str(r, "value"));
        if built_at.is_empty() {
            built_at = get_str(r, "built_at");
        }
    }
    let months: Vec<String> = kv
        .get("full_months")
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Ok(Meta {
        latest: kv
            .get("latest_month")
            .cloned()
            .or_else(|| months.last().cloned())
            .unwrap_or_default(),
        months,
        source: kv.get("source").cloned().unwrap_or_default(),
        caveat: kv.get("caveat").cloned().unwrap_or_default(),
        built_at,
    })
}
