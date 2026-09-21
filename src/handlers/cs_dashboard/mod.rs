//! コンサルダッシュボード（納品管理PL 21596025 を見る画面）
//!
//! 2026-09-20。採用コンサルの継続・成果・接触を見る画面。架電クオリティ
//! （`/call-quality`）とも営業KPI（`/sales-kpi`）とも見る人が違うので、
//! 同じアプリの中の**別ページ**として持つ。
//!
//! モック（Hubspot リポジトリ
//! `claudedocs\consulting_dashboard_2026-09-18\コンサルダッシュボード_モック.html`）
//! が設計図。あちらは Python が 24MB の JSON を HTML に埋めているが、
//! ここでは**スプレッドシート経由で読んで集計は Rust でやる**。
//!
//! ------------------------------------------------------------------
//! データの出どころ
//! ------------------------------------------------------------------
//! Hubspot リポジトリ `scripts\consulting_dashboard\build_sheets.py` が
//! 平らな表に畳んだものを、架電クオリティと同じスプレッドシートに置く。
//! `SheetStore` は架電クオリティのものを借りる（別に持つとキャッシュが
//! 二重になって Sheets を無駄に2回叩く）。
//!
//!   CS_取引              3,656行 × 39列  全タブの土台
//!   CS_顧客              1,649行 × 18列  法人単位（取引をまたいで見るため）
//!   CS_MTG               2,500行 × 21列  MTG明細（長文は入れない）
//!   CS_担当履歴          4,136行 ×  6列
//!   CS_月次スナップショット 7,228行 ×  5列
//!   CS_通話明細         20,843行 × 11列  Call は Deal に多対多
//!
//! 🔴 **MTG の長文4フィールドはスプシに入れない**（Zoom要約(章立て) ほか）。
//!    モックの 24.1MB のうち 14.9MB がこれで、集計には一切使わない。
//!    顧客1件を開いたときだけ要るので、別経路で取る（設計書参照）。
//!
//! ------------------------------------------------------------------
//! 🔴 継続率の定義（2026-09-20 ユーザー確定。再導出しない）
//! ------------------------------------------------------------------
//! ```text
//! 継続率 = 継続済 ÷ (継続済 + 解約 + 充足)
//! ```
//! - 母数は **契約満了が該当月**、かつ **決着済み**（継続／解約／充足）
//! - **充足は分母に入れる。外さない。** 採用できて終わったのも
//!   「継続しなかった」結果
//! - **オプション契約は母数に入れない**（求人追加・AirWork広告運用・
//!   一次対応・エントリーフォーム・追加）。月ごとの偏りが大きく、
//!   含めると月次比較が歪む
//! - 結果待ち（上のどれでもないステージ）は分母に入れない
//! - 決着0件なら率は **空**（0% にしない）
//!
//! 🔴 **満了月は `contract_expiration_date` で決める。`closedate` ではない。**
//!    closedate で切ると 2026-06 の母数が 107→81 に減り、率も
//!    54.2%→53.1% とずれる（実測。テストで固定してある）。
//!
//! 定義の正本: `.claude\skills\call-quality-metrics\SKILL.md`

pub mod routes;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

pub const SHEET_DEAL: &str = "CS_取引";
pub const SHEET_CALL: &str = "CS_通話明細";
pub const SHEET_MTG: &str = "CS_MTG";
pub const SHEET_HISTORY: &str = "CS_プロパティ履歴";
pub const SHEET_CUSTOMER: &str = "CS_顧客";
/// メール由来の MTG 実施日。
///
/// 🔴 **これは推定であって事実ではない。** 実測の一致率は ±1日で 83.3%（母数636件）。
/// Zoom 録画（事実）と同じ確かさで並べない。画面では
/// 「実施した事実（推定）」と「中身が読める（事実）」を分けて出す。
pub const SHEET_MAIL_MTG: &str = "CS_MTG実施日_メール由来";
pub const SHEET_HANDOVER: &str = "CS_担当交代";
/// 担当の履歴。🔴 **担当者の正本は `consultant`**（`hubspot_owner_id` ではない）。
/// このシートの `owner` 欄が consultant で、取引ごとの最新行がいまの担当。
pub const SHEET_OWNER_HIST: &str = "CS_担当履歴";
/// このデータをいつ作ったか。
/// 🔴 **計算の基準日（今日）とは別物。** シートは手で作り直しているので、
///    基準日だけ今日になっていて中身は何日も前、ということが起きる。
pub const SHEET_META: &str = "CS_メタ";

/// 定期NPS のプロパティ名。回ごとに別プロパティになっている。
pub const NPS_PROPS: &[&str] = &[
    "nps", "nps2", "nps3", "nps4", "nps5", "nps6", "nps7", "nps8", "nps9", "nps10", "nps11",
];

/// 手を打つ対象と見なす NPS。**これ以下**。
///
/// 🔴 NPS は**リスクの軸には使わない**（稼働中の半分にしか無く、構造的に発火しない）。
/// 独立した指標として、母数を必ず添えて出す。
pub const NPS_LOW: f64 = 4.0;

/// 接触率を**図に載せてよい**最小の分母（案件 × 経過月）。
///
/// 🔴 デザインの規律「n<30 は図に載せない」に合わせている。
/// 1案件・5か月の分母で 0% になった人が、33案件で 24.4% の人より「悪い」位置に
/// 並ぶと実態とずれる。**表からは外さない**（1案件でも接触ゼロなら拾いたい）。
pub const MIN_CONTACT_MONTHS: usize = 30;

/// 接触と見なす通話の長さ（秒）。**これより長いものだけ**を数える。
///
/// 実測で F1 0.671 が最良。`>300秒` にすると1人1時間あたり 80.4% が0件になり使えない。
/// `result=canceled` はほぼ全部0秒（3,104件中3,103件）なので、この閾値で自然に落ちる。
pub const CONTACT_SEC: f64 = 60.0;

// ---------------------------------------------------------------- ステージ

/// 継続済。継続率の分子。
pub const ST_KEEP: &str = "66848546";
/// 解約済（成果不足 / 会社方針・その他 / 架電禁止先）。
pub const ST_CANCEL: &[&str] = &["52016159", "52016158", "1016664339"];
/// 解約済（充足）。🔴 **分母に入れる**。採用できて終わったのも継続しなかった結果。
pub const ST_FILL: &str = "90598807";

/// 継続率の母数から外す契約種別。
///
/// 実測で月ごとの偏りが大きく（6月8件 vs 8月20件）、含めると月次比較が歪む。
/// 外すと結果待ちもほぼ消える（3ヶ月で 48件→5件）。満了してもステージが
/// 動かない取引の多くがオプションだったため。
pub const OPTION_KINDS: &[&str] = &[
    "求人追加",
    "AirWork広告運用",
    "一次対応",
    "エントリーフォーム",
    "追加",
];

/// 決着の3状態。ここに入らないものは「結果待ち」で、分母に入れない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Cancel,
    Fill,
    Pending,
}

pub fn outcome_of(stage: &str) -> Outcome {
    if stage == ST_KEEP {
        Outcome::Keep
    } else if ST_CANCEL.contains(&stage) {
        Outcome::Cancel
    } else if stage == ST_FILL {
        Outcome::Fill
    } else {
        Outcome::Pending
    }
}

// ---------------------------------------------------------------- 取引

/// `CS_取引` 1行ぶん。**列は名前で引く**（位置で決め打ちしない）。
#[derive(Debug, Clone)]
pub struct Deal {
    pub id: String,
    /// 取引名。**画面はこれを出す。** 取引IDは現場に意味が無い。
    pub name: String,
    pub stage: String,
    /// ステージの日本語名。🔴 **画面に内部ID（`52016155` など）を出さない。**
    pub stage_label: String,
    /// 契約種別。オプション判定に使う。
    pub contract_kind: String,
    /// 満了日 `yyyy-MM-dd`。**実データ。推定していない。**
    /// 空なら満了月が決まらない＝継続率の母数に入れない。
    pub contract_expiration_date: String,
    /// 契約開始日 `yyyy-MM-dd`。「契約後の接触」を切り出す基準。
    pub contract_start_date: String,
    /// 拠点キー。🔴 **決裁は事業所単位。** 採用単価はこの単位で見る
    /// （法人でまとめると拠点間のばらつきが「時間の悪化」に見える）。
    pub kyoten_key: String,
    pub houjin_resolved: String,
    /// 継続回数。初回が 0。
    pub renewal_no: Option<i64>,
    pub is_active: bool,
    /// 直近契約で結果が確定していないもの。除くと代表値が大きく動く
    /// （応募数の中央値が 3→11 と変わるほど効く）ので、画面で切り替えられるようにする。
    pub right_censored: bool,
    pub oubo: Option<f64>,
    pub mensetu: Option<f64>,
    pub syoudaku: Option<f64>,
    pub saiyomokuhyou: Option<f64>,
    pub keisaisu: Option<f64>,
    pub amount: Option<f64>,
    pub contract_period: Option<f64>,
}

/// 空文字は `None`。**0 と「値が無い」を混ぜない**。
pub fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

/// 金額。🔴 **0 は「入っていない」として扱う。**
///
/// 0円の契約は存在しないので、0 は未入力の裏返し。`0万` と表示したり、
/// 採用単価の計算に 0 を入れると、集計が下に引っ張られる。
/// 実データで 3,659件中 2件（稼働中は1件）。入力側の問題なので、
/// **ここでは表示から外すだけ**にして、名指しの表は作らない。
fn money(s: &str) -> Option<f64> {
    match opt_num(s) {
        Some(v) if v > 0.0 => Some(v),
        _ => None,
    }
}

/// シートの真偽値。Python 側は `TRUE`/`FALSE` で書く。
pub fn flag_true(s: &str) -> bool {
    flag(s)
}

fn flag(s: &str) -> bool {
    matches!(s.trim(), "TRUE" | "true" | "True" | "1")
}

impl Deal {
    fn from_row(sheet: &SheetData, row: &[Arc<str>]) -> Self {
        let g = |name: &str| sheet.get(row, name);
        Self {
            id: g("deal_id").to_string(),
            name: g("dealname").to_string(),
            stage: g("dealstage").to_string(),
            stage_label: g("dealstage_label").to_string(),
            contract_kind: g("contract_kind").to_string(),
            contract_expiration_date: g("contract_expiration_date").to_string(),
            contract_start_date: g("contract_start_date").to_string(),
            kyoten_key: g("kyoten_key").to_string(),
            houjin_resolved: g("houjin_resolved").to_string(),
            renewal_no: opt_num(g("renewal_no")).map(|v| v as i64),
            is_active: flag(g("is_active")),
            right_censored: flag(g("right_censored")),
            oubo: opt_num(g("oubo")),
            mensetu: opt_num(g("mensetu")),
            syoudaku: opt_num(g("syoudaku")),
            saiyomokuhyou: opt_num(g("saiyomokuhyou")),
            keisaisu: opt_num(g("keisaisu")),
            amount: money(g("amount")),
            contract_period: opt_num(g("contract_period")),
        }
    }

    /// 継続率の母数から外す契約か。
    pub fn is_option(&self) -> bool {
        OPTION_KINDS.contains(&self.contract_kind.as_str())
    }

    /// 満了月 `yyyy-MM`。満了日が無ければ `None`。
    pub fn manryou_month(&self) -> Option<&str> {
        if self.contract_expiration_date.len() >= 7 {
            Some(&self.contract_expiration_date[..7])
        } else {
            None
        }
    }

    /// 求人票あたりの応募効率。掲載数が0/未入力なら `None`。
    ///
    /// 「失敗解約の最強識別子」として検証済み（per-posting 8.1 << 継続 14.5）。
    /// 🔴 欠測を 0 に落とさない。落とすと誤って低リスク側に寄る。
    pub fn oubo_per_posting(&self) -> Option<f64> {
        match (self.oubo, self.keisaisu) {
            (Some(o), Some(k)) if k > 0.0 => Some(o / k),
            _ => None,
        }
    }

    /// 採用目標に対する達成率。目標が0/未入力なら `None`。
    pub fn rate_tassei(&self) -> Option<f64> {
        match (self.syoudaku, self.saiyomokuhyou) {
            (Some(s), Some(t)) if t > 0.0 => Some(s / t),
            _ => None,
        }
    }
}

pub fn deals_of(sheet: &SheetData) -> Vec<Deal> {
    sheet.rows.iter().map(|r| Deal::from_row(sheet, r)).collect()
}

// ---------------------------------------------------------------- 読み込み

pub struct Sheets {
    pub deal: Arc<SheetData>,
    pub call: Arc<SheetData>,
    pub mtg: Arc<SheetData>,
    pub history: Arc<SheetData>,
    pub customer: Arc<SheetData>,
    pub mail_mtg: Arc<SheetData>,
    pub handover: Arc<SheetData>,
    pub owner_hist: Arc<SheetData>,
    /// 生成時刻。シートが無い環境もあるので、読めなくても画面は出す。
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
        deal: fetch!(SHEET_DEAL),
        call: fetch!(SHEET_CALL),
        mtg: fetch!(SHEET_MTG),
        history: fetch!(SHEET_HISTORY),
        customer: fetch!(SHEET_CUSTOMER),
        mail_mtg: fetch!(SHEET_MAIL_MTG),
        handover: fetch!(SHEET_HANDOVER),
        owner_hist: fetch!(SHEET_OWNER_HIST),
        // 🔴 これだけは**読めなくても落とさない**。鮮度が出ないだけで、
        //    画面そのものは開けるべきなので、失敗したら空のシートとして扱う。
        meta: match store.get(client, SHEET_META).await {
            Ok((data, hit)) => {
                cached &= hit;
                data
            }
            Err(e) => {
                tracing::warn!("シート「{}」が読めません（鮮度は出しません）: {e:#}", SHEET_META);
                Arc::new(SheetData {
                    header: vec!["key".into(), "value".into()],
                    rows: Vec::new(),
                    fetched_at: std::time::Instant::now(),
                })
            }
        },
        all_cached: cached,
    })
}

// ---------------------------------------------------------------- 接触

/// 取引ごとの接触日（昇順）。
///
/// 🔴 **接触 = MTG または60秒超の通話。メールは数えない。**
/// 既存の `deal_contact_rollup.json` は使わない。あれの `last_contact_at_post` は
/// Call + Email + MTG の最新で**メールが混ざり**、`last_call_at_post` は Call だけで
/// **MTG が抜ける**。どちらもこの定義と違う。
///
/// 🔴 **接触は検知にだけ使う。処方には使わない。** 中盤の接触急増は「火消し」で、
/// 結果が原因より後に来ている。「接触が減っているから増やせ」とはこの検証からは言えない。
pub fn contacts_by_deal(
    call: &SheetData,
    mtg: &SheetData,
) -> (HashMap<String, Vec<NaiveDate>>, usize, usize) {
    let mut by: HashMap<String, Vec<NaiveDate>> = HashMap::new();
    let mut n_call = 0usize;
    let mut n_mtg = 0usize;

    for row in &call.rows {
        let secs = opt_num(call.get(row, "duration_sec")).unwrap_or(0.0);
        if secs <= CONTACT_SEC {
            continue;
        }
        let deal = call.get(row, "deal_id");
        if deal.is_empty() {
            continue;
        }
        if let Some(d) = date10(call.get(row, "ts")) {
            by.entry(deal.to_string()).or_default().push(d);
            n_call += 1;
        }
    }

    for row in &mtg.rows {
        // 🔴 取引に紐づかない MTG は数えられない。実測で 2,500件中 376件（15.0%）。
        //    法人だけでは拠点の違う取引を分けられないので、ここでは捨てる。
        let deal = mtg.get(row, "deal_id");
        if deal.is_empty() {
            continue;
        }
        if let Some(d) = date10(mtg.get(row, "開催日")) {
            by.entry(deal.to_string()).or_default().push(d);
            n_mtg += 1;
        }
    }

    for v in by.values_mut() {
        v.sort();
    }
    (by, n_call, n_mtg)
}

/// `yyyy-MM-dd…` の先頭10文字だけ見る。時刻とタイムゾーンは落とす。
///
/// 通話の `ts` は UTC の ISO8601 だが、ここで使うのは「何日前か」だけなので
/// 日付で足りる。パースを増やすと取り違えが起きる。
pub fn date10(s: &str) -> Option<NaiveDate> {
    if s.len() < 10 {
        return None;
    }
    NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()
}

// ---------------------------------------------------------------- プロパティ履歴

/// 月ごとの値。**持ち越したかどうかが分かる形**で返す。
///
/// 🔴 `carry = true` は「その月に書き換えが無く、前の値を引き継いだ」という意味。
/// 値としては読めるが**新しく書かれた数字ではない**ので、画面は
/// 右側打ち切りと同じ**中空・破線**で描く。実線で描くと「その月に更新があった」
/// ように見える（Python 側の `svgLine` も同じ描き分けをしている）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonthValue {
    pub month: String,
    pub v: f64,
    pub carry: bool,
}

/// `CS_プロパティ履歴` を (取引, プロパティ) ごとの系列にする。
///
/// シートには**値が変わった月の行だけ**入っている。全月を materialize すると
/// 行数が数倍になるため、畳む側（Python）が変化点だけを出している。
pub fn series_of(history: &SheetData) -> HashMap<(String, String), Vec<(String, f64)>> {
    let mut out: HashMap<(String, String), Vec<(String, f64)>> = HashMap::new();
    for row in &history.rows {
        let deal = history.get(row, "deal_id");
        let prop = history.get(row, "prop");
        let month = history.get(row, "month");
        if deal.is_empty() || prop.is_empty() || month.len() < 7 {
            continue;
        }
        let Some(v) = opt_num(history.get(row, "v")) else {
            continue;
        };
        out.entry((deal.to_string(), prop.to_string()))
            .or_default()
            .push((month[..7].to_string(), v));
    }
    for v in out.values_mut() {
        v.sort_by(|a, b| a.0.cmp(&b.0));
        // 同じ月が2行あることは無いはずだが、あったら後勝ち（畳む側と同じ約束）
        v.dedup_by(|a, b| a.0 == b.0);
    }
    out
}

/// `yyyy-MM` を1ヶ月進める。
fn next_month(m: &str) -> String {
    let (y, mo) = match (m.get(..4).and_then(|x| x.parse::<i32>().ok()),
                         m.get(5..7).and_then(|x| x.parse::<u32>().ok())) {
        (Some(y), Some(mo)) => (y, mo),
        _ => return m.to_string(),
    };
    if mo >= 12 {
        format!("{:04}-01", y + 1)
    } else {
        format!("{y:04}-{:02}", mo + 1)
    }
}

/// 変化点だけの系列を、月ごとに埋めて返す。
///
/// 約束は3つ:
/// 1. **飛んだ月は前の値を持ち越す**（`carry = true`）
/// 2. 🔴 **最初の点より前の月は出さない。** 「値が無い」のであって 0 ではない。
///    0 で埋めると、契約開始直後に応募が0件だったように見える
/// 3. 実測の月は `carry = false`。画面はここだけ実線・塗りつぶしで描く
///
/// `until` は「どこまで埋めるか」（例: 満了月、または今月）。
/// `until` が最初の点より前なら空を返す。
pub fn fill_forward(points: &[(String, f64)], until: &str) -> Vec<MonthValue> {
    let mut out = Vec::new();
    if points.is_empty() {
        return out;
    }
    let mut it = points.iter().peekable();
    // 🔴 開始は最初の実測の月。それより前は埋めない
    let mut cur = points[0].0.clone();
    let mut last = points[0].1;
    while cur.as_str() <= until {
        let measured = match it.peek() {
            Some((m, v)) if *m == cur => {
                last = *v;
                it.next();
                true
            }
            _ => false,
        };
        out.push(MonthValue {
            month: cur.clone(),
            v: last,
            carry: !measured,
        });
        cur = next_month(&cur);
    }
    out
}

// ---------------------------------------------------------------- 顧客

/// `CS_顧客` 1行ぶん。法人単位。
///
/// 🔴 **決裁は事業所単位。** 法人でまとめた1本の線にしない
/// （9拠点ぶんを1本につなぐと、拠点間のばらつきが「時間の悪化」に見える）。
/// ここで法人を持つのは、取引をまたいで履歴を追うため。
#[derive(Debug, Clone)]
pub struct Customer {
    pub houjin: String,
    pub name: String,
    pub deal_count: Option<f64>,
    pub kyoten_unique: Option<f64>,
    pub ltv: Option<f64>,
    pub active_deal_count: Option<f64>,
    pub max_renewal_no: Option<f64>,
    pub last_expiration: String,
    pub is_display_target: bool,
}

pub fn customers_of(sheet: &SheetData) -> Vec<Customer> {
    sheet
        .rows
        .iter()
        .map(|row| {
            let g = |name: &str| sheet.get(row, name);
            Customer {
                houjin: g("houjin").to_string(),
                name: g("代表社名").to_string(),
                deal_count: opt_num(g("deal_count")),
                kyoten_unique: opt_num(g("kyoten_unique")),
                ltv: opt_num(g("ltv")),
                active_deal_count: opt_num(g("active_deal_count")),
                max_renewal_no: opt_num(g("max_renewal_no")),
                last_expiration: g("last_expiration").to_string(),
                is_display_target: flag(g("is_display_target")),
            }
        })
        .collect()
}

/// 取引ごとの「最新の定期NPS」。
///
/// 回ごとに別プロパティ（`nps` / `nps2` … `nps11`）なので、**月が新しいほう**を採る。
/// 同じ月なら回が後のほうを採る。
///
/// 🔴 返すのは入っている取引だけ。入っていない取引を 0 にしない
/// （`customer_health_score` が稼働中で実質NPS単独になっていたのと同じ罠）。
pub fn latest_nps(history: &SheetData) -> HashMap<String, (String, f64)> {
    let mut out: HashMap<String, (String, f64)> = HashMap::new();
    for row in &history.rows {
        let prop = history.get(row, "prop");
        if !NPS_PROPS.contains(&prop) {
            continue;
        }
        let deal = history.get(row, "deal_id");
        let month = history.get(row, "month");
        if deal.is_empty() || month.len() < 7 {
            continue;
        }
        let Some(v) = opt_num(history.get(row, "v")) else {
            continue;
        };
        let e = out.entry(deal.to_string()).or_insert((month.to_string(), v));
        if month >= e.0.as_str() {
            *e = (month.to_string(), v);
        }
    }
    out
}

/// 採用単価 = 契約総額 ÷ 採用数。
///
/// 🔴 顧客が見ている数字はこれひとつ。応募数でも面接数でもない。
/// 🔴 **分母0・欠損では点を作らない**（0 で割った値を「単価が高い」と読ませない）。
pub fn cpa(d: &Deal) -> Option<f64> {
    match (d.amount, d.syoudaku) {
        (Some(a), Some(s)) if s > 0.0 => Some(a / s),
        _ => None,
    }
}

// ---------------------------------------------------------------- 起動時の先読み

/// この画面が読むシート。先読みと、データ品質タブの一覧に使う。
pub const SHEETS: &[&str] = &[
    SHEET_DEAL,
    SHEET_CUSTOMER,
    SHEET_MTG,
    SHEET_CALL,
    SHEET_HISTORY,
    SHEET_MAIL_MTG,
    SHEET_HANDOVER,
    SHEET_OWNER_HIST,
    SHEET_META,
];

/// 起動時にシートを常駐キャッシュへ載せておく。
///
/// 入れた理由: 先読みが無いと**最初に開いた人だけが 24.4秒待つ**（9枚を一度に取るため。
/// 2026-09-21 実測。2回目以降は 0.2〜0.7秒）。現場は「重い画面」と一度思うと
/// 二度目を開かない。
///
/// 🔴 **失敗してもサーバの起動を止めない。** シートが読めない状態でも画面は開いて、
/// そのタブだけ「データが取れていません」と出るほうがよい（起動できないほうが困る）。
/// なので戻り値は `()` で、失敗はログに出すだけにしてある。
///
/// 🔴 **`tokio::spawn` で呼ぶこと（`main.rs` 参照）。** `SheetStore::get` は
/// 取得のあいだ書き込みロックを持つので、待って（await して）から listen すると
/// 起動が 24秒遅れ、Render のヘルスチェックが落ちる。
pub async fn prefetch() {
    let state = match crate::handlers::call_quality::routes::cq_state() {
        Ok(s) => s,
        Err(_) => {
            // GOOGLE_SA_KEY_B64 / SPREADSHEET_ID が無い。init_from_env が
            // 既に warn を出しているので、ここでは二重に騒がない
            tracing::warn!("コンサルダッシュボード: Sheets が未設定のため先読みしない");
            return;
        }
    };
    let started = std::time::Instant::now();
    let mut ok = 0usize;
    let mut ng = 0usize;
    for name in SHEETS {
        match state.store.get(&state.client, name).await {
            Ok((data, from_cache)) => {
                ok += 1;
                tracing::info!(
                    "コンサル先読み: {name} {}行{}",
                    data.rows.len(),
                    if from_cache { "（キャッシュ）" } else { "" }
                );
            }
            Err(e) => {
                ng += 1;
                // 1枚読めなくても残りは続ける。どの枚が落ちたかを残す
                tracing::warn!("コンサル先読み: {name} が読めない: {e:#}");
            }
        }
    }
    tracing::info!(
        "コンサル先読み: 完了 {ok}枚 / 失敗 {ng}枚 / {:.1}秒",
        started.elapsed().as_secs_f64()
    );
}

// ---------------------------------------------------------------- 担当者

/// 取引ごとの「いまの担当」。
///
/// 🔴 **正本は `consultant`。** `hubspot_owner_id` ではない。
/// `CS_担当履歴` の `owner` 欄が consultant で、日付が最新の行がいまの担当。
/// 退職者のまま残っている取引があるので、`retired` も一緒に返す。
/// 🔴 **同じ日に複数行あると、どちらを採るかで答えが変わる。**
/// 稼働中で最新日が割れているのは 38件（2026-09-21 実測）。
/// ここでは**シートの並び順で後に来る行**を採る（追記順＝新しい出来事）。
/// 割れている件数は `consultant_ties` で数えて画面に出す。黙って選ばない。
pub fn consultant_ties(owner_hist: &SheetData, active: &HashSet<&str>) -> usize {
    let mut top: HashMap<&str, (String, usize, bool)> = HashMap::new();
    for row in &owner_hist.rows {
        let deal = owner_hist.get(row, "deal_id");
        if !active.contains(deal) {
            continue;
        }
        let date = owner_hist.get(row, "date").to_string();
        let owner = owner_hist.get(row, "owner").to_string();
        let retired = flag(owner_hist.get(row, "retired"));
        let key = format!("{owner}|{retired}");
        match top.get_mut(deal) {
            Some(e) if e.0 == date => {
                e.1 += 1;
                if format!("{owner}|{retired}") != key {
                    // 同日で中身が違う
                }
                e.2 = true;
            }
            Some(e) if date > e.0 => {
                *e = (date, 1, false);
            }
            Some(_) => {}
            None => {
                top.insert(deal, (date, 1, false));
            }
        }
    }
    top.values().filter(|(_, n, _)| *n > 1).count()
}

pub fn consultant_of(owner_hist: &SheetData) -> HashMap<String, (String, bool)> {
    let mut latest: HashMap<String, (String, String, bool)> = HashMap::new();
    for row in &owner_hist.rows {
        let deal = owner_hist.get(row, "deal_id");
        let owner = owner_hist.get(row, "owner").trim();
        let date = owner_hist.get(row, "date");
        if deal.is_empty() || owner.is_empty() {
            continue;
        }
        let retired = flag(owner_hist.get(row, "retired"));
        let e = latest.entry(deal.to_string());
        match e {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                if date >= o.get().0.as_str() {
                    o.insert((date.to_string(), owner.to_string(), retired));
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert((date.to_string(), owner.to_string(), retired));
            }
        }
    }
    latest
        .into_iter()
        .map(|(k, (_, o, r))| (k, (o, r)))
        .collect()
}

/// 法人ごとの「注力かどうか」。
///
/// 既存の定義をそのまま使う（`focus_flags`: 月額30万超 / 拠点が複数 / 従業員規模）。
/// **ここで定義を作り直さない。**
pub fn focus_of(customer: &SheetData) -> HashMap<String, bool> {
    customer
        .rows
        .iter()
        .filter_map(|row| {
            let h = customer.get(row, "houjin");
            if h.is_empty() {
                return None;
            }
            let raw = customer.get(row, "focus_flags");
            let any = serde_json::from_str::<serde_json::Value>(raw)
                .ok()
                .and_then(|v| v.get("any").and_then(|x| x.as_bool()))
                .unwrap_or(false);
            Some((h.to_string(), any))
        })
        .collect()
}

/// 契約開始から今日（または満了）までの「経過月」を並べる。
///
/// 接触率の分母。🔴 **件数ではなく率で見るため**に要る
/// （件数だと持ち案件が多い人ほど大きく出て、手が回っているかが分からない）。
pub fn elapsed_months(d: &Deal, today: NaiveDate) -> Vec<String> {
    let Some(st) = date10(&d.contract_start_date) else {
        return Vec::new();
    };
    let end = date10(&d.contract_expiration_date)
        .map(|e| if e < today { e } else { today })
        .unwrap_or(today);
    if end < st {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut y, mut m) = (st.year(), st.month());
    let (ey, em) = (end.year(), end.month());
    while (y, m) <= (ey, em) {
        out.push(format!("{y:04}-{m:02}"));
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
        // 暴走よけ。契約が壊れていても画面は出す
        if out.len() > 120 {
            break;
        }
    }
    out
}

/// 案件1件の接触率。**②コンサルタント一覧と同じ定義・同じ計算**。
///
/// 🔴 接触 ＝ MTG または60秒超の通話。**メールは数えない。**
/// 接触率 ＝ 接触があった月 ÷ 経過月。**率だけ出さず、分子と分母を一緒に返す。**
/// 経過月が0なら `None`（0% と書かない）。
pub fn contact_rate_of(
    d: &Deal,
    contacts: &HashMap<String, Vec<NaiveDate>>,
    today: NaiveDate,
) -> (usize, usize) {
    let ms = elapsed_months(d, today);
    let Some(dates) = contacts.get(&d.id) else {
        return (0, ms.len());
    };
    let hit: HashSet<String> = dates.iter().map(|x| x.format("%Y-%m").to_string()).collect();
    (ms.iter().filter(|m| hit.contains(*m)).count(), ms.len())
}

// ---------------------------------------------------------------- データの鮮度

/// メタシートのキーを引く。
fn meta_value(meta: &SheetData, key: &str) -> Option<String> {
    meta.rows.iter().find_map(|row| {
        if meta.get(row, "key").trim() == key {
            let v = meta.get(row, "value").trim().to_string();
            (!v.is_empty()).then_some(v)
        } else {
            None
        }
    })
}

/// シートを作り直した日時（JST, `yyyy-MM-dd HH:mm:ss`）。
///
/// 🔴 **計算の基準日（今日）とは別物。** この画面は毎朝見るものなので、
/// 「古いデータを新しいものと誤認させない」のは画面の責任。
/// 取れないときは `None`。**推測で埋めない**（埋めると嘘になる）。
pub fn generated_at(meta: &SheetData) -> Option<String> {
    meta_value(meta, "生成時刻(JST)")
}

/// 生成時刻から基準日まで何日たったか。日付部分（先頭10文字）だけで数える。
pub fn generated_age_days(meta: &SheetData, today: NaiveDate) -> Option<i64> {
    let at = generated_at(meta)?;
    date10(&at).map(|d| (today - d).num_days())
}

/// 元データを HubSpot / Zoom から落とした時刻（いちばん古いもの）。
///
/// 🔴 **シートを作り直した時刻（`generated_at`）とは別物。**
/// 古い JSON を詰め直してもシートの生成時刻だけ新しくなり、中身は古いまま。
/// 「このデータは何日前のものか」はこちらで数える。
/// 元データが複数あるときは**いちばん古いもの**を代表にしてある
/// （画面はいちばん古いところまでしか遡れないため）。
pub fn data_as_of(meta: &SheetData) -> Option<String> {
    meta_value(meta, "データ取得時刻(JST)")
}

/// 元データの取得から基準日まで何日たったか。
pub fn data_age_days(meta: &SheetData, today: NaiveDate) -> Option<i64> {
    let at = data_as_of(meta)?;
    date10(&at).map(|d| (today - d).num_days())
}
