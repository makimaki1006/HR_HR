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

pub mod contact_trend;
pub mod deal_detail;
pub mod handover_contact;
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
/// 電話の要約（1行 = 1通話 × 取引）。Hubspot リポジトリの日次更新
/// （`scripts/consulting_call/summarize_calls.py`: Zoom Phone の文字起こし → MiniMax-M3）が書く。
///
/// 列: `call_id / deal_id / ts / duration_sec / summary / next_action / concern /
/// n_utterances / model / generated_at / source`。1通話が複数の取引に付くときは
/// `CS_通話明細` と同じく取引ごとに1行（call_id が重複してよい）。
/// 文字起こしが取れない・短すぎる通話は行が無い（画面は「要約なし」）。
///
/// 🔴 **読めなくても落とさない**（`CS_メタ` と同じ扱い）。2026-09-26 時点で本番にまだ無い。
/// 🔴 **`SHEETS`（先読み・全画面の読み込み）には入れない。** 読むのは案件の詳細だけ。
///    `SheetStore` は取れなかったことを覚えないので、全画面の `load` に入れると、
///    シートが無いあいだ**どの画面を開いても毎回 Sheets に取りに行って失敗する**（その分遅くなる）。
///    `load_call_summary` が失敗を少しのあいだ覚えて、取りに行く回数を抑える。
pub const SHEET_CALL_SUMMARY: &str = "CS_通話要約";

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

/// MTG が途絶えていると見なす日数。**GAS `no_mtg_alerter.gs` の閾値をそのまま使う。**
///
/// ```text
///   注意  30〜59 日   NMA_YELLOW_MIN / NMA_YELLOW_MAX
///   警告  60〜89 日   NMA_RED_MIN    / NMA_RED_MAX
///   重大  90 日〜     NMA_CRITICAL_MIN
/// ```
/// 🔴 **ここで引き直さない。** 毎日 05:00 に回っている GAS と画面で線引きが違うと、
/// 同じ取引が片方でだけ警告になる。
pub const MTG_GAP_YELLOW_DAYS: i64 = 30;
pub const MTG_GAP_RED_DAYS: i64 = 60;
pub const MTG_GAP_CRITICAL_DAYS: i64 = 90;

/// 立ち上がり期。契約開始からこの日数以内は帯を付けない。
///
/// GAS `NMA_ONBOARDING_GRACE_DAYS`。契約直後に MTG が無いのは普通なので、
/// ここを警告にすると毎朝の画面が始まったばかりの契約で埋まる。
///
/// 🔴 GAS は `createdate`（HubSpot に取引が作られた日）で数えているが、
/// **この画面は `contract_start_date` で数える**。満了日を `closedate` ではなく
/// `contract_expiration_date` で決めているのと同じ理由で、契約の実際の開始日が
/// 正本だから。日数の線引き（30日）は GAS と同じ。
pub const MTG_ONBOARDING_GRACE_DAYS: i64 = 30;

/// 契約終了間際の特別扱い。満了まで**この日数以内**で、
/// `MTG_PRE_TERMINATION_GAP_DAYS` 以上 MTG が途絶えていたら**強制的に重大**。
///
/// GAS `NMA_PRE_TERMINATION_DAYS` / `NMA_PRE_TERMINATION_MTG_GAP_DAYS`。
/// 更新の話をする時期に音沙汰が無いのは、経過日数が短くても重い。
pub const MTG_PRE_TERMINATION_DAYS: i64 = 90;
pub const MTG_PRE_TERMINATION_GAP_DAYS: i64 = 30;

/// 「今週始まった契約」と見なす日数。GAS `new_deal_detector.gs` の
/// `NDD_LOOKBACK_DAYS` と同じ。
///
/// 🔴 始まった日に気づけないと、立ち上がり期（開始30日以内は帯を付けない）が
/// ただの取りこぼしになる。始まりと見逃しは対で出す。
pub const NEW_DEAL_LOOKBACK_DAYS: i64 = 7;

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

/// オプション契約の種別。**画面の母集団から外す。**
///
/// 実測で月ごとの偏りが大きく（6月8件 vs 8月20件）、含めると月次比較が歪む。
/// 外すと結果待ちもほぼ消える（3ヶ月で 48件→3件）。満了してもステージが
/// 動かない取引の多くがオプションだったため。
///
/// 🔴 `contract_kind` は**取引名の接頭辞から機械で作った値**
///    (`scripts/consulting_renewal/renewal_no.py` の `contract_kind`)。
///    接頭辞が想定と違うと別の値になる。実データ 3,659件で数えたところ、
///    「AirWork広告費用＿…」は `AirWork` という別の値になっていた（3件・全部稼働中）。
///    取引名に「AirWork」等を含むのに種別がオプション外だったのはこの3件だけで、
///    それ以外の取りこぼしは無い（AirWork 71件 / 広告運用 68件 / 求人追加 128件 /
///    追加 133件 / 一次対応 13件 / エントリーフォーム 6件 を名前で数えて確認）。
///    **取引名そのものでは判定しない**。本体契約の名前に「追加」等が混じったときに
///    売上のある本体を誤って外すため。判定に足すのは値（種別・ステージ）だけにする。
pub const OPTION_KINDS: &[&str] = &[
    "求人追加",
    "AirWork広告運用",
    // 「AirWork広告費用＿…」がここに落ちる。名前ではなく値で拾う
    "AirWork",
    "一次対応",
    "エントリーフォーム",
    "追加",
];

/// オプション契約だけが置かれるステージ。**種別と OR で判定する。**
///
/// - `1049738304` オプション（求人追加・一次対応）    173件中95件
/// - `1281526627` 満了済オプション（一次対応・追加）  同78件
///
/// 🔴 **種別だけでも、ステージだけでも取りこぼす。** 実測で
/// 「ステージはオプションなのに種別が `(新規)`／`サブスク継続`」が4件、
/// 逆に「種別はオプションなのにステージは継続済など」が54件ある。
/// 両方を見て、どちらかに当たればオプションとする。
pub const OPTION_STAGES: &[&str] = &["1049738304", "1281526627"];

/// 「マーケ関連」ステージ。コンサルの納品ではない取引（紹介料・マーケ施策の計上）が置かれる。
///
/// 実測（fixture）で稼働中 10件、全部が初回（継続回数0）・種別 `(新規)`・
/// 契約種別「その他」。「紹介料＿…」の取引もこのステージに置かれている
/// （Hubspot リポジトリの `consulting_pipeline_coverage.csv` で `紹介料＿…` → `1278456227` を確認）。接触も MTG も起きないのが普通なので、
/// 「接触が1本も無い」「MTG が結べていない初回契約」の**名指しの表**を埋めてしまう。
///
/// 🔴 **母集団からは外していない**（オプションと違い、外すかどうかはまだ決めていない）。
/// 外しているのは、上の2つの表の行と、その表の分母だけ。外した件数は表と一緒に返す。
/// 判定は取引名ではなくステージの値で行う（名前で外すと本体契約を巻き込む）。
pub const MARKETING_STAGES: &[&str] = &["1278456227"];

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
    /// 拠点の表示名。`kyoten_key` は照合用に正規化した値で、人が読む名前ではない。
    /// 画面に拠点を出すときはこちらを使う（空なら `kyoten_key` に戻す）。
    pub kyoten_name: String,
    pub houjin_resolved: String,
    /// 法人番号をどこから解決したか。データ品質の画面が数える。
    pub houjin_source: String,
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

/// `CS_取引` の列の位置。**シート1枚につき1回だけ**引く。
///
/// 🔴 `SheetData::get` は呼ぶたびに見出しを頭から探す。取引1行で20列を読むので、
/// 3,659行 × 20列 × 見出し43列ぶんの比較を、1回のリクエストで2回（集計と母集団）やっていた。
/// 列名で引く約束（位置で決め打ちしない）はそのまま、探すのを最初の1回にする。
struct DealCols([Option<usize>; DEAL_COLS.len()]);

const DEAL_COLS: [&str; 21] = [
    "deal_id",
    "dealname",
    "dealstage",
    "dealstage_label",
    "contract_kind",
    "contract_expiration_date",
    "contract_start_date",
    "kyoten_key",
    "kyoten_name",
    "houjin_resolved",
    "houjin_source",
    "renewal_no",
    "is_active",
    "right_censored",
    "oubo",
    "mensetu",
    "syoudaku",
    "saiyomokuhyou",
    "keisaisu",
    "amount",
    "contract_period",
];

impl DealCols {
    fn of(sheet: &SheetData) -> Self {
        let mut ix = [None; DEAL_COLS.len()];
        for (i, name) in DEAL_COLS.iter().enumerate() {
            ix[i] = sheet.col(name);
        }
        Self(ix)
    }

    fn get<'a>(&self, row: &'a [Arc<str>], name: &str) -> &'a str {
        DEAL_COLS
            .iter()
            .position(|c| *c == name)
            .and_then(|i| self.0[i])
            .and_then(|i| row.get(i))
            .map(|s| s.as_ref())
            .unwrap_or("")
    }
}

impl Deal {
    fn from_row(cols: &DealCols, row: &[Arc<str>]) -> Self {
        let g = |name: &str| cols.get(row, name);
        Self {
            id: g("deal_id").to_string(),
            name: g("dealname").to_string(),
            stage: g("dealstage").to_string(),
            stage_label: g("dealstage_label").to_string(),
            contract_kind: g("contract_kind").to_string(),
            contract_expiration_date: g("contract_expiration_date").to_string(),
            contract_start_date: g("contract_start_date").to_string(),
            kyoten_key: g("kyoten_key").to_string(),
            kyoten_name: g("kyoten_name").to_string(),
            houjin_resolved: g("houjin_resolved").to_string(),
            houjin_source: g("houjin_source").to_string(),
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

    /// 拠点の表示名。表示名が空なら `None`。
    ///
    /// 🔴 **照合用の `kyoten_key` で埋めない。** キーは突き合わせ用に正規化した値で、
    /// 人が読む名前ではない（V24「内部の値を出さない」）。以前はここでキーに戻していた。
    /// fixture では表示名が空の行は 0件だが、本番に空があるかは確かめていない。
    /// 空のときの見せ方（「拠点名なし」など）は画面が決める。
    pub fn site_name(&self) -> Option<&str> {
        let t = self.kyoten_name.trim();
        (!t.is_empty()).then_some(t)
    }

    /// オプション契約か。**画面の母集団から外すもの。**
    ///
    /// 種別（取引名の接頭辞由来）とステージの **OR**。片方だけだと取りこぼす。
    pub fn is_option(&self) -> bool {
        OPTION_KINDS.contains(&self.contract_kind.as_str())
            || OPTION_STAGES.contains(&self.stage.as_str())
    }

    /// 満了月 `yyyy-MM`。満了日が無ければ `None`。
    pub fn manryou_month(&self) -> Option<&str> {
        // 🔴 バイト位置で切らない。7バイト目が文字の途中だと panic して全タブが落ちる
        //    （書き出し側は今 ISO 形式だが、手で「2026年9月…」と入ると起きる）
        self.contract_expiration_date.get(..7)
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

/// 画面が見る取引。**オプション契約は入っていない。**
///
/// 🔴 ここで外すのは、外し忘れる画面を作らないため。実データで
/// 稼働中703件のうち99件がオプション（求人追加・AirWork広告運用ほか）で、
/// 案件一覧・コンサルタント一覧・集計に混ざると読めない画面になる。
/// **外した件数は黙って消さず、`population_of` が数えて画面に出す。**
pub fn deals_of(sheet: &SheetData) -> Vec<Deal> {
    deals_all_of(sheet)
        .into_iter()
        .filter(|d| !d.is_option())
        .collect()
}

/// オプション契約も含む全取引。**母集団の注記を作るときだけ使う。**
pub fn deals_all_of(sheet: &SheetData) -> Vec<Deal> {
    let cols = DealCols::of(sheet);
    sheet
        .rows
        .iter()
        .map(|r| Deal::from_row(&cols, r))
        .collect()
}

/// 法人の母数。**「全 N 法人」はここで1回だけ数える。**
///
/// 🔴 以前は法人番号で見る画面に「全 1,649 法人」（注力の注記、`CS_顧客` の行数）と
///    「全 1,646 法人」（本部アプローチ、オプション契約を外した取引の法人）が並んでいた。
///    差の3法人は**オプション契約しか持たない法人**（fixture 2026-09-23 実測:
///    AirWork広告運用だけ 2社、オプションのステージだけ 1社）。
///    画面の母集団はオプション契約を外す約束（`deals_of`）なので、法人もそれに揃える。
///    外した法人の数は黙って消さず `option_only` で返す。
#[derive(Debug, Clone, PartialEq)]
pub struct HoujinPopulation {
    /// オプション以外の取引を1件以上持つ法人（法人番号が空の取引は数えない）
    pub main: HashSet<String>,
    /// 取引はあるが、全部オプション契約の法人の数
    pub option_only: usize,
}

pub fn houjin_population(sheet: &SheetData) -> HoujinPopulation {
    let mut main = HashSet::new();
    let mut any = HashSet::new();
    for d in deals_all_of(sheet) {
        if d.houjin_resolved.is_empty() {
            continue;
        }
        if !d.is_option() {
            main.insert(d.houjin_resolved.clone());
        }
        any.insert(d.houjin_resolved);
    }
    let option_only = any.len() - main.len();
    HoujinPopulation { main, option_only }
}

/// 画面に出す母集団。**どの画面でも同じ数字を出すために1か所で作る。**
///
/// 🔴 タブによって母集団が違うと、同じ画面の中で数が合わなくなる。
/// 除いた件数と金額も持つ（AirWork広告運用は実在する売上なので、
/// 「無かったこと」にはしない）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Population {
    /// 全取引（オプション込み）
    pub deals_all: usize,
    /// そのうちオプション契約
    pub deals_option: usize,
    /// 全取引（オプション契約を除く）
    pub deals: usize,
    /// 稼働中（オプション込み）
    pub active_all: usize,
    /// そのうちオプション契約
    pub active_option: usize,
    /// 稼働中（オプション契約を除く）。**これが画面の母集団**
    pub active: usize,
    /// 外した稼働中オプションの金額合計。入っていなければ `None`
    pub option_amount: Option<f64>,
    /// 画面に出す一文
    pub note: String,
}

pub fn population_of(sheet: &SheetData) -> Population {
    let all = deals_all_of(sheet);
    let deals_all = all.len();
    let deals_option = all.iter().filter(|d| d.is_option()).count();
    let active: Vec<&Deal> = all.iter().filter(|d| d.is_active).collect();
    let active_all = active.len();
    let opt_active: Vec<&&Deal> = active.iter().filter(|d| d.is_option()).collect();
    let active_option = opt_active.len();
    let amt: f64 = opt_active.iter().filter_map(|d| d.amount).sum();
    Population {
        deals_all,
        deals_option,
        deals: deals_all - deals_option,
        active_all,
        active_option,
        active: active_all - active_option,
        option_amount: if amt > 0.0 { Some(amt) } else { None },
        note: format!(
            "稼働中{active_all}件からオプション契約{active_option}件を除いた{}件で見ています。オプション（求人追加・AirWork広告運用・一次対応・エントリーフォーム・追加）は本体契約と一緒に数えると案件も担当者も二重に見えるため外しています。外した契約が無くなったわけではありません",
            active_all - active_option
        ),
    }
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
                tracing::warn!(
                    "シート「{}」が読めません（鮮度は出しません）: {e:#}",
                    SHEET_META
                );
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

/// 取れなかったとき、次に取りに行くまで待つ時間。
const CALL_SUMMARY_RETRY: std::time::Duration = std::time::Duration::from_secs(600);

/// `CS_通話要約` を読む。**読めなければ `None`**（画面は「電話の要約はまだありません」と出す）。
///
/// 🔴 エラーにしない（`CS_メタ` と同じ）。要約が無くても、案件の詳細の残り（MTG・電話の記録・交代）は出す。
/// 🔴 失敗したら `CALL_SUMMARY_RETRY` のあいだは取りに行かない（無いシートを毎回取りに行って遅くしない）。
///    ただし `force`（画面の「読み直す」）のときは待たずに取りに行く
///    （本番にシートを作った直後に読み直しても、最大10分「まだありません」が出続けないように）。
/// 🔴 `store.get` は使わない。`get` はキャッシュ切れのとき**書き込みロックを持ったまま**取りに行くので、
///    そのあいだ同じストアを読む他の画面（架電クオリティ・営業KPI・コンサル）が待たされる。
///    生きたキャッシュを読み取りロックだけで見て、無ければ `refresh`（取得中にロックを持たない）で取る。
pub async fn load_call_summary(
    client: &SheetsClient,
    store: &SheetStore,
    force: bool,
) -> Option<Arc<SheetData>> {
    static LAST_MISS: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    if !force {
        if let Some(d) = store.fresh(SHEET_CALL_SUMMARY).await {
            return Some(d);
        }
        let recently_missed = LAST_MISS
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|t| t.elapsed() < CALL_SUMMARY_RETRY);
        if recently_missed {
            return None;
        }
    }
    match store.refresh(client, SHEET_CALL_SUMMARY).await {
        Ok(data) => {
            if let Ok(mut g) = LAST_MISS.lock() {
                *g = None;
            }
            Some(data)
        }
        Err(e) => {
            tracing::warn!(
                "シート「{}」が読めません（電話の要約は出しません）: {e:#}",
                SHEET_CALL_SUMMARY
            );
            if let Ok(mut g) = LAST_MISS.lock() {
                *g = Some(std::time::Instant::now());
            }
            None
        }
    }
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
        if let Some(d) = call_date_jst(call.get(row, "ts")) {
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
/// 🔴 **通話の `ts`（UTC）には使わない。** `call_date_jst` を使う。
/// 🔴 バイト位置で切らない（`&s[..10]`）。10バイト目が文字の途中だと panic して
///    全タブが落ちる。`get` なら日付でないものは `None` になるだけ。
pub fn date10(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// 通話の `ts` を**日本時間の日付**にする。
///
/// 🔴 `ts` は UTC の ISO8601（`2026-03-27T02:52:44Z`）。先頭10文字をそのまま日付にすると、
/// 日本時間 0〜9時の通話が前日（月初なら前月）に入る。fixture 20,843行のうち
/// UTC 15時以降（＝日本時間では翌日）が 153行、うち接触（60秒超）が 55行、
/// 月までずれるのが 6行（2026-09-23 実測）。
/// タイムゾーンが付いていない値は、いままでどおり先頭10文字で読む（推測で時差を足さない）。
pub fn call_date_jst(ts: &str) -> Option<NaiveDate> {
    match chrono::DateTime::parse_from_rfc3339(ts.trim()) {
        Ok(t) => Some(
            t.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).expect("JST"))
                .date_naive(),
        ),
        Err(_) => date10(ts),
    }
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
        // 🔴 `yyyy-MM` として読めないものは捨てる。`fill_forward` は月を1つずつ
        //    進めるので、月として読めない値が混ざると進めなくなる
        let Some(month) = history
            .get(row, "month")
            .get(..7)
            .filter(|m| ym_of(m).is_some())
        else {
            continue;
        };
        if deal.is_empty() || prop.is_empty() {
            continue;
        }
        let Some(v) = opt_num(history.get(row, "v")) else {
            continue;
        };
        out.entry((deal.to_string(), prop.to_string()))
            .or_default()
            .push((month.to_string(), v));
    }
    for v in out.values_mut() {
        v.sort_by(|a, b| a.0.cmp(&b.0));
        // 同じ月が2行あることは無いはずだが、あったら後勝ち（畳む側と同じ約束）
        v.dedup_by(|a, b| a.0 == b.0);
    }
    out
}

/// `yyyy-MM` を (年, 月) にする。月が 1〜12 でなければ `None`。
pub fn ym_of(m: &str) -> Option<(i32, u32)> {
    let y = m.get(..4)?.parse::<i32>().ok()?;
    let mo = m.get(5..7)?.parse::<u32>().ok()?;
    (1..=12).contains(&mo).then_some((y, mo))
}

/// `yyyy-MM` を1ヶ月進める。月として読めなければ `None`。
///
/// 🔴 以前は読めないときに**同じ値をそのまま返していた**。`fill_forward` の
/// `while cur <= until` がそれで一歩も進まず、無限に行を積んでメモリを食い尽くす。
fn next_month(m: &str) -> Option<String> {
    let (y, mo) = ym_of(m)?;
    Some(if mo >= 12 {
        format!("{:04}-01", y + 1)
    } else {
        format!("{y:04}-{:02}", mo + 1)
    })
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
        // 🔴 月として読めない値なら、そこで打ち切る（同じ月を積み続けない）
        match next_month(&cur) {
            Some(n) if n > cur => cur = n,
            _ => break,
        }
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
    // (月, 回の番号, 値)。🔴 同じ月に2回ぶん入っているときは**回が後のほう**を採る。
    //    以前は「シートで後に来た行」を採っていて、上の約束と違っていた
    //    （行の並びは畳む側の都合で、回の順とは限らない）。
    let mut best: HashMap<String, (String, usize, f64)> = HashMap::new();
    for row in &history.rows {
        let prop = history.get(row, "prop");
        let Some(round) = NPS_PROPS.iter().position(|p| *p == prop) else {
            continue;
        };
        let deal = history.get(row, "deal_id");
        let month = history.get(row, "month");
        if deal.is_empty() || month.len() < 7 {
            continue;
        }
        let Some(v) = opt_num(history.get(row, "v")) else {
            continue;
        };
        let e = best
            .entry(deal.to_string())
            .or_insert((month.to_string(), round, v));
        if (month, round) >= (e.0.as_str(), e.1) {
            *e = (month.to_string(), round, v);
        }
    }
    best.into_iter().map(|(k, (m, _, v))| (k, (m, v))).collect()
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
/// 🔴 **戻ってこない。** 最初の先読みのあと、TTL が切れる前に取り直し続ける。
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
                    if from_cache {
                        "（キャッシュ）"
                    } else {
                        ""
                    }
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

    // 🔴 **TTL が切れる前に取り直し続ける。** 起動時の1回だけだと、1時間後に
    //    最初に開いた人が9枚の直列取得を待つ（初回は実測 18.7秒・2026-09-23 本番）。
    //    しかも `SheetStore::get` は取得中ずっと書き込みロックを持つので、
    //    そのあいだ架電クオリティ・営業KPI の画面まで止まる。
    //    `refresh` はロックを持たずに取ってから差し替えるので、誰も待たない。
    let every = prefetch_interval();
    loop {
        tokio::time::sleep(every).await;
        let started = std::time::Instant::now();
        let mut ng = 0usize;
        for name in SHEETS {
            if let Err(e) = state.store.refresh(&state.client, name).await {
                ng += 1;
                // 取れなかった枚は古いまま残る。TTL が切れたら `get` が取りに行く
                tracing::warn!("コンサル定期更新: {name} が読めない: {e:#}");
            }
        }
        tracing::info!(
            "コンサル定期更新: {}枚 / 失敗 {ng}枚 / {:.1}秒",
            SHEETS.len(),
            started.elapsed().as_secs_f64()
        );
    }
}

/// 定期更新の間隔。**TTL より必ず短くする**（切れてから取ると、開いた人が待つ）。
///
/// TTL の 3/4（60分なら45分）。取得そのものに 20秒前後かかるので、
/// ぎりぎりにすると間に合わない。
pub fn prefetch_interval() -> std::time::Duration {
    crate::handlers::call_quality::sheets::CACHE_TTL * 3 / 4
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
    // 取引 → (最新日, その日の最初の行の中身, 同じ日に中身の違う行があったか)
    //
    // 🔴 「同じ日に複数行」だけでは割れていない。同じ日・同じ担当の行が重なっていても、
    //    どちらを採っても担当は変わらないので数えない。以前はそれも数えていた
    //    （中身を比べる式が、同じ行から作った文字列どうしを比べていて常に一致していた）。
    // 🔴 担当が空の行は `consultant_of` が読み飛ばすので、ここでも数えない。
    let mut top: HashMap<&str, (String, (String, bool), bool)> = HashMap::new();
    for row in &owner_hist.rows {
        let deal = owner_hist.get(row, "deal_id");
        if !active.contains(deal) {
            continue;
        }
        let owner = owner_hist.get(row, "owner").trim();
        if owner.is_empty() {
            continue;
        }
        let date = owner_hist.get(row, "date").to_string();
        let who = (owner.to_string(), flag(owner_hist.get(row, "retired")));
        match top.get_mut(deal) {
            Some(e) if e.0 == date => {
                if e.1 != who {
                    e.2 = true;
                }
            }
            Some(e) if date > e.0 => {
                *e = (date, who, false);
            }
            Some(_) => {}
            None => {
                top.insert(deal, (date, who, false));
            }
        }
    }
    top.values().filter(|(_, _, split)| *split).count()
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

/// 注力の条件。**法人単位**の性質で、日々は変わらない（大きさの話）。
///
/// 🔴 **MTG途絶の帯（状態・取引単位・日々変わる）とは別物。** 混ぜない。
///
/// 値は `focus_flags` 列に入っているものをそのまま読む。**ここで定義を作り直さない。**
/// 3つの条件は fixture 1,649法人で境目を数えて確かめた（2026-09-23）:
///
/// ```text
///   monthly_over_300k  月額30万**以上**   true の最小 300,000 / false の最大 280,000
///   enterprise         従業員1,000名以上  true の最小 1,000   / false の最大 991
///   multi_site         拠点3つ以上        true の最小 3       / false の最大 2
/// ```
/// （以前ここに「月額30万**超** / 拠点が**複数**」と書いてあったが、実データと合わない）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct FocusFlags {
    /// 月額30万以上
    pub monthly_over_300k: bool,
    /// 従業員1,000名以上
    pub enterprise: bool,
    /// 拠点3つ以上
    pub multi_site: bool,
    /// いずれかに当たる
    pub any: bool,
}

impl FocusFlags {
    /// 当たっている条件の名前。画面が「なぜ注力なのか」を出すために使う。
    pub fn reasons(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.monthly_over_300k {
            v.push("月額30万以上");
        }
        if self.enterprise {
            v.push("従業員1,000名以上");
        }
        if self.multi_site {
            v.push("拠点3つ以上");
        }
        v
    }
}

pub fn focus_flags_of(customer: &SheetData) -> HashMap<String, FocusFlags> {
    customer
        .rows
        .iter()
        .filter_map(|row| {
            let h = customer.get(row, "houjin");
            if h.is_empty() {
                return None;
            }
            let v: serde_json::Value = serde_json::from_str(customer.get(row, "focus_flags"))
                .unwrap_or(serde_json::Value::Null);
            let b = |k: &str| v.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
            Some((
                h.to_string(),
                FocusFlags {
                    monthly_over_300k: b("monthly_over_300k"),
                    enterprise: b("enterprise"),
                    multi_site: b("multi_site"),
                    any: b("any"),
                },
            ))
        })
        .collect()
}

/// 法人ごとの「注力かどうか」。
pub fn focus_of(customer: &SheetData) -> HashMap<String, bool> {
    focus_flags_of(customer)
        .into_iter()
        .map(|(k, f)| (k, f.any))
        .collect()
}

// ---------------------------------------------------------------- MTG が途絶えているか

/// MTG途絶の帯。**取引単位・日々変わる状態**。
///
/// 🔴 注力（法人単位・大きさ・不変）とは別物。GAS の Layer1/2/3 でいう
/// 「状態」に当たるのはこちら。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgBand {
    /// 90日以上
    Critical,
    /// 60〜89日
    Red,
    /// 30〜59日
    Yellow,
    /// 30日未満。直近にMTGがある
    Recent,
    /// 録画でもメールでも記録が見つからない。
    /// 🔴 **「MTGをしていない」ではない。** 取引に結べていないぶんを含む
    NoRecord,
    /// 契約開始30日以内。帯を付けない
    Onboarding,
}

impl MtgBand {
    pub fn label(self) -> &'static str {
        match self {
            MtgBand::Critical => "MTGが90日以上途絶",
            MtgBand::Red => "MTGが60〜89日途絶",
            MtgBand::Yellow => "MTGが30〜59日途絶",
            MtgBand::Recent => "直近30日にMTGあり",
            MtgBand::NoRecord => "MTGの記録が無い",
            MtgBand::Onboarding => "立ち上がり期（契約開始30日以内）",
        }
    }

    /// 毎朝の画面で名札を立てる帯か。
    ///
    /// 🔴 `NoRecord` は立てない。**ここだけ GAS と違う。**
    /// GAS は記録が無いものを `daysElapsed = 9999` として重大に落とすが、
    /// 実データでは「MTGをしていない」ではなく「録画が取引に結べていない」が
    /// 混ざる（録画だけだと稼働中の45.4%にしか記録が無い）。
    /// 赤にすると本当に途絶えている先が埋もれるので、件数だけ出して名札は立てない
    /// （2026-09-23 ユーザー確定）。
    /// `Recent` / `Onboarding` も、悪いことが起きていないので立てない。
    pub fn is_alert(self) -> bool {
        matches!(self, MtgBand::Critical | MtgBand::Red | MtgBand::Yellow)
    }
}

/// 最終MTG日を決めた出どころ。**行ごとに画面へ出す。**
///
/// 🔴 録画は事実、メール由来は推定（±1日で83.3%）。同じ確かさで並べない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgSource {
    /// Zoom録画。事実
    Recording,
    /// メールから起こした実施日。推定
    Mail,
    /// 両方が同じ日を指している
    Both,
    /// どちらにも記録が無い
    None,
}

impl MtgSource {
    pub fn label(self) -> &'static str {
        match self {
            MtgSource::Recording => "録画（事実）",
            MtgSource::Mail => "メール由来（推定・±1日で83.3%）",
            MtgSource::Both => "録画とメールの両方（事実）",
            MtgSource::None => "記録なし",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MtgGap {
    pub band: MtgBand,
    pub source: MtgSource,
    /// 最終MTG日。記録が無ければ `None`。**0日にしない**
    pub last: Option<NaiveDate>,
    /// 途絶日数。記録が無ければ `None`。**9999 のような番兵を入れない**
    pub days: Option<i64>,
    /// 満了90日前の途絶で、帯を強制的に重大へ上げたか
    pub forced_by_expiry: bool,
}

/// 取引ごとの最終MTG日を、録画（事実）とメール（推定）の両方から集める。
///
/// 🔴 **両方を見る。** 録画だけだと稼働中604件のうち274件（45.4%）にしか
/// 記録が無く、記録が無いものが191件出る。これをそのまま赤にすると、
/// 「MTGをしていない」ではなく「録画が取引に結べていない」を警告することになる。
/// メール由来を足すと記録が無いものは44件まで落ちる（2026-09-23 実測）。
///
/// メール側は **`kind` が「実施」の行だけ**採る（予定・候補・取り下げは実施ではない）。
pub fn last_mtg_by_deal(
    mtg: &SheetData,
    mail: &SheetData,
) -> HashMap<String, (Option<NaiveDate>, Option<NaiveDate>)> {
    let mut by: HashMap<String, (Option<NaiveDate>, Option<NaiveDate>)> = HashMap::new();
    for row in &mtg.rows {
        let deal = mtg.get(row, "deal_id");
        if deal.is_empty() {
            continue;
        }
        if let Some(d) = date10(mtg.get(row, "開催日")) {
            let e = by.entry(deal.to_string()).or_default();
            if e.0.is_none_or(|x| d > x) {
                e.0 = Some(d);
            }
        }
    }
    for row in &mail.rows {
        let deal = mail.get(row, "deal_id");
        if deal.is_empty() || mail.get(row, "kind") != "実施" {
            continue;
        }
        if let Some(d) = date10(mail.get(row, "date")) {
            let e = by.entry(deal.to_string()).or_default();
            if e.1.is_none_or(|x| d > x) {
                e.1 = Some(d);
            }
        }
    }
    by
}

/// 取引1件の MTG途絶の帯。閾値は GAS `no_mtg_alerter.gs` と同じ。
///
/// 順番は GAS と合わせてある:
///   1. 立ち上がり期（契約開始30日以内）なら、そこで打ち切る
///   2. 録画とメールの新しい方を最終MTG日にする
///   3. 記録が無ければ `NoRecord`（**日数を 9999 にしない**）
///   4. 経過日数で帯を決める
///   5. 満了90日前で30日以上途絶なら、帯に関わらず**重大へ上げる**
pub fn mtg_gap_of(
    d: &Deal,
    last: &HashMap<String, (Option<NaiveDate>, Option<NaiveDate>)>,
    today: NaiveDate,
) -> MtgGap {
    let none = MtgGap {
        band: MtgBand::NoRecord,
        source: MtgSource::None,
        last: None,
        days: None,
        forced_by_expiry: false,
    };

    // 1. 立ち上がり期
    if let Some(st) = date10(&d.contract_start_date) {
        if (today - st).num_days() < MTG_ONBOARDING_GRACE_DAYS {
            return MtgGap {
                band: MtgBand::Onboarding,
                ..none
            };
        }
    }

    // 2〜3. 最終MTG日と出どころ
    let (rec, mail) = last.get(&d.id).copied().unwrap_or((None, None));
    let (day, source) = match (rec, mail) {
        (None, None) => return none,
        (Some(r), None) => (r, MtgSource::Recording),
        (None, Some(m)) => (m, MtgSource::Mail),
        (Some(r), Some(m)) => match r.cmp(&m) {
            std::cmp::Ordering::Equal => (r, MtgSource::Both),
            std::cmp::Ordering::Greater => (r, MtgSource::Recording),
            std::cmp::Ordering::Less => (m, MtgSource::Mail),
        },
    };

    // 4. 経過日数で帯
    let days = (today - day).num_days();
    let mut band = if days >= MTG_GAP_CRITICAL_DAYS {
        MtgBand::Critical
    } else if days >= MTG_GAP_RED_DAYS {
        MtgBand::Red
    } else if days >= MTG_GAP_YELLOW_DAYS {
        MtgBand::Yellow
    } else {
        MtgBand::Recent
    };

    // 5. 満了90日前の途絶は強制的に重大
    let mut forced = false;
    if band != MtgBand::Critical && days >= MTG_PRE_TERMINATION_GAP_DAYS {
        if let Some(exp) = date10(&d.contract_expiration_date) {
            let to_end = (exp - today).num_days();
            if (0..=MTG_PRE_TERMINATION_DAYS).contains(&to_end) {
                band = MtgBand::Critical;
                forced = true;
            }
        }
    }

    MtgGap {
        band,
        source,
        last: Some(day),
        days: Some(days),
        forced_by_expiry: forced,
    }
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
    let hit: HashSet<String> = dates
        .iter()
        .map(|x| x.format("%Y-%m").to_string())
        .collect();
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
