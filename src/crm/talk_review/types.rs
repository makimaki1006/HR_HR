//! トークの振り返り (`/app/admin?view=talk`) の API の型。React 側へ ts-rs で書き出す。
//! 画面と API の契約なので、項目を変えるときは両方を直す。

use serde::Serialize;
use ts_rs::TS;

/// 判定項目の定義 (checks.json から)
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkCheckDef {
    pub id: String,
    /// 場面 (受付 / 日付の前 / 断られた後)
    pub stage: String,
    pub title: String,
    /// 判定の決まり (画面の見出しの説明に出す)
    pub rule: String,
    /// 資料 07章 図5 の「全体でできていない割合」(%)。無ければ null
    pub baseline_pct: Option<f64>,
    /// その割合の分母 (例: 話せた電話のうち)
    pub baseline_basis: Option<String>,
}

/// 1 人 (またはチーム全体) の 1 項目の集計
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkItemRate {
    pub id: String,
    /// できていない本数
    pub not_done: u32,
    /// 該当した本数 (できた + できていない)
    pub applicable: u32,
    /// not_done ÷ applicable × 100 (applicable 0 は null)
    pub rate_pct: Option<f64>,
    /// 印の決まりに当たった
    pub flagged: bool,
}

/// 発信の内訳 (Zoom の通話記録から直接数えた本数。シート由来の発信数と数本ずれることがある)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, TS)]
pub struct DialBreakdown {
    /// つながった (録音が残った発信)
    pub connected: u32,
    /// 発信に失敗した (相手側の回線・番号の問題など。記録上は「つながった」と出るが録音が無いもの)
    pub failed: u32,
    /// こちらで切った
    pub cancelled: u32,
    /// モニタリング (他の人の通話を聞いた)
    pub monitor: u32,
    /// 社内の内線
    pub extension: u32,
    /// 上の合計
    pub total: u32,
}

/// 表の 1 行 (1 人、またはチーム全体)
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkPersonRow {
    /// HubSpot の担当者 ID。チーム全体の行は null
    pub owner_id: Option<String>,
    pub name: String,
    pub team: Option<String>,
    /// 発信数 (Zoom の発信すべて。相手が出る前に切ったもの・つながらなかったものを含む。営業KPI の「発信」)。dials と同じく読めなければ null
    pub outbound: Option<u32>,
    /// 発信の内訳 (発信数のホバー用)。集計中・Zoom から取れないときは null
    pub dial_breakdown: Option<DialBreakdown>,
    /// 架電数 (Zoom でつながった通話。営業KPI の「架電数」と同じ)。シートが読めなければ null
    pub dials: Option<u32>,
    pub appointments: u32,
    /// アポ数 ÷ 架電数 × 100 (架電数が無い・0 は null)
    pub appointment_rate_pct: Option<f64>,
    /// アポ率の印 (架電数が下限以上で、チーム全体のアポ率の一定割合未満)
    pub appointment_rate_flagged: bool,
    /// 接触数 (受付を越えて担当者・決裁者と話せた発信の本数)。判定の保存が読めなければ null
    pub contacts: Option<u32>,
    /// 接触数 ÷ 発信数 × 100 (発信数が無い・0 は null)
    pub contact_rate_pct: Option<f64>,
    /// 日程切り本数 (具体的な日時を 1 回以上出した発信の本数)
    pub date_proposal_calls: Option<u32>,
    /// 日程切り本数 ÷ 発信数 × 100 (発信数が無い・0 は null)
    pub date_proposal_rate_pct: Option<f64>,
    /// 1 接触あたりの日程切り回数 (接触した電話で日時を出した回数の合計 ÷ 接触数。接触 0 は null)
    pub proposals_per_contact: Option<f64>,
    /// 接触の判定がまだ済んでいない、つながった発信の本数
    pub contact_pending: u32,
    /// 判定の対象になった電話の本数 (70 秒以上。枠を超えて判定しないものも含む)
    pub eligible: u32,
    /// 判定できた電話の本数
    pub reviewed: u32,
    /// 判定待ち (文字起こし待ち・未判定) の本数
    pub waiting: u32,
    /// 録音・文字起こしが取れず判定できない本数
    pub unavailable: u32,
    /// checks の順に並ぶ
    pub items: Vec<TalkItemRate>,
}

/// 印の決まり (画面の下に平易に書く)
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkAlertRules {
    /// 項目の印: 該当がこの本数以上
    pub item_min_applicable: u32,
    /// 項目の印: 本人のできていない割合がチーム全体よりこの % 以上多い
    pub item_excess_pct: f64,
    /// アポ率の印: 架電数がこの本数以上
    pub appointment_min_dials: u32,
    /// アポ率の印: チーム全体のアポ率のこの割合未満 (0.5 = 半分)
    pub appointment_rate_ratio: f64,
}

/// GET /api/crm/talk-review/summary
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkSummaryResponse {
    /// 期間 (JST、YYYY-MM-DD、両端を含む)
    pub from: String,
    pub to: String,
    /// この集計を作った時刻 (RFC 3339)
    pub as_of: String,
    /// 機能が有効か (無効なら people は空で、画面は案内を出す)
    pub enabled: bool,
    /// 絞り込み用のチーム名
    pub teams: Vec<String>,
    pub checks: Vec<TalkCheckDef>,
    pub rules: TalkAlertRules,
    pub people: Vec<TalkPersonRow>,
    /// 1 人 1 日あたりに判定する最大本数 (`CALL_REVIEW_PER_PERSON_DAY`)
    pub per_person_day: u32,
    /// アポをまだ数えている日 (YYYY-MM-DD、古い順)。その日のアポは表に入っていない
    pub counting_days: Vec<String>,
    /// 営業KPI のシートが読めず、架電数とアポ率を出せない
    pub dials_error: bool,
    /// 期間の最終日の架電数が、シートの途中経過 (同期の時点の数)
    pub dials_partial: bool,
    /// 途中経過の取得時刻 (`yyyy-MM-dd HH:mm`)。分からなければ null
    pub dials_fetched_at: Option<String>,
    /// 発信の内訳をまだ集計している (過去の日を Zoom から取り込み中。しばらくして取り直すと出る)
    pub dial_breakdown_pending: bool,
    /// チーム全体 (絞り込み後の people 全員の合計)
    pub total: TalkPersonRow,
}

/// 1 本の電話の 1 項目の判定 (一覧用)
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkItemMark {
    pub id: String,
    /// "done" | "not_done" | "na"
    pub status: String,
}

/// 電話の一覧の 1 行
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkCallRow {
    pub hubspot_call_id: String,
    /// 通話の時刻 (RFC 3339)
    pub call_at: String,
    pub duration_sec: u32,
    /// "judged" | "waiting" | "unavailable" | "failed"
    pub status: String,
    /// 日程が決まった / 決まらなかった / 担当者不在 (判定前は null)
    pub outcome: Option<String>,
    /// checks の順。判定前は空
    pub items: Vec<TalkItemMark>,
    /// 一番直すとよい点 (1 文)
    pub summary: Option<String>,
}

/// GET /api/crm/talk-review/calls?owner=&from=&to=
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkCallsResponse {
    pub owner_id: String,
    pub name: String,
    pub from: String,
    pub to: String,
    pub checks: Vec<TalkCheckDef>,
    /// 新しい順
    pub calls: Vec<TalkCallRow>,
}

/// 1 項目の判定の詳細
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkItemDetail {
    pub id: String,
    /// "done" | "not_done" | "na"
    pub status: String,
    /// 根拠の発言の番号 (文字起こしの n)
    pub evidence: Vec<u32>,
    pub quote: Option<String>,
    /// 次に言うとよいこと (できていないときだけ)
    pub advice: Option<String>,
    pub note: Option<String>,
}

/// 文字起こしの 1 発言
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkTurn {
    pub n: u32,
    /// "rep" (営業) | "customer" (先方)
    pub speaker: String,
    pub text: String,
}

/// GET /api/crm/talk-review/calls/{hubspot_call_id}
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TalkCallDetailResponse {
    pub call: TalkCallRow,
    pub owner_id: String,
    /// 営業の名前
    pub rep_name: String,
    pub checks: Vec<TalkCheckDef>,
    /// checks の順
    pub items: Vec<TalkItemDetail>,
    pub reached_person: Option<bool>,
    pub reception: Option<bool>,
    pub first_date_turn: Option<u32>,
    /// Zoom から取り直した文字起こし。取れなければ null
    pub transcript: Option<Vec<TalkTurn>>,
    /// 取れなかった理由 (例: 録音が残っていません)
    pub transcript_error: Option<String>,
    /// HubSpot の通話を開く URL
    pub hubspot_url: String,
    /// 判定に使ったモデルと checks の版
    pub model: Option<String>,
    pub checks_version: Option<String>,
}
