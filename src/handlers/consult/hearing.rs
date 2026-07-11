//! ヒアリングシート + 回答保存 (フェーズC。計画書 §13 / §5.2-F / §22)
//!
//! - 印刷用シート (`hearing_sheet_html`): 面談に持っていける A4 の記入シート。
//!   navy スタイルを流用し「社内用」帯つき。記入欄は contenteditable + 印刷で罫線が出る。
//! - Web 入力フォーム (`hearing_form_html`): 15 必須項目 + 各項目に「不明」「データなし」トグル。
//!   §13.3 の動的質問 (応募数の回答帯に応じた追質問) は JS で表示切り替え。
//! - 保存 (`ensure_table` / `insert_result` / `latest_result`): ローカル SQLite の
//!   consult_hearing_results テーブルへ追記オンリー (§22 更新履歴)。UPDATE しない。
//!   最新 revision が現在値。Turso には一切書き込まない。
//!
//! 回答形式 (§13.2): 数値 / 単一選択 / 複数選択 / 自由記述。
//!   加えて「不明」と「データなし」を必ず区別する (別々のトグルとして保持)。
//!
//! V2ルール: 介護データ・HW由来データは一切参照しない。ローカル SQLite の
//! consult_hearing_results テーブルのみを読み書きする (HW求人・時系列・介護需要は不参照)。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::escape_html;

/// 回答形式 (§13.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKind {
    /// 数値
    Number,
    /// 単一選択
    Single,
    /// 複数選択
    Multi,
    /// 自由記述
    Text,
}

/// ヒアリング項目定義 (§13.1 の 15 必須項目)。
pub struct HearingItem {
    /// フォーム/JSON のキー (例: q01_hiring_count)
    pub key: &'static str,
    /// 表示ラベル
    pub label: &'static str,
    /// 回答形式
    pub kind: AnswerKind,
    /// 単位・補足 (数値項目の単位や記入ヒント。無ければ空)
    pub hint: &'static str,
    /// 選択肢 (Single / Multi のときのみ。他は空)
    pub options: &'static [&'static str],
}

/// §13.1 の 15 必須項目。順序は仕様書の並びに従う。
pub const HEARING_ITEMS: [HearingItem; 15] = [
    HearingItem {
        key: "q01_hiring_count",
        label: "採用人数",
        kind: AnswerKind::Number,
        hint: "名",
        options: &[],
    },
    HearingItem {
        key: "q02_deadline",
        label: "採用期限",
        kind: AnswerKind::Text,
        hint: "例: 2026年9月末 / 未定",
        options: &[],
    },
    HearingItem {
        key: "q03_reason",
        label: "採用理由",
        kind: AnswerKind::Single,
        hint: "増員・欠員・新拠点等",
        options: &["増員", "欠員補充", "新拠点・新事業", "その他"],
    },
    HearingItem {
        key: "q04_applications_monthly",
        label: "月間応募数",
        kind: AnswerKind::Single,
        hint: "直近の1か月あたり",
        options: &["0〜2件", "3件以上"],
    },
    HearingItem {
        key: "q05_contacts",
        label: "接触数",
        kind: AnswerKind::Number,
        hint: "件 / 月",
        options: &[],
    },
    HearingItem {
        key: "q06_interviews_set",
        label: "面接設定数",
        kind: AnswerKind::Number,
        hint: "件 / 月",
        options: &[],
    },
    HearingItem {
        key: "q07_interviews_done",
        label: "面接実施数",
        kind: AnswerKind::Number,
        hint: "件 / 月",
        options: &[],
    },
    HearingItem {
        key: "q08_offers",
        label: "内定数",
        kind: AnswerKind::Number,
        hint: "件 / 月",
        options: &[],
    },
    HearingItem {
        key: "q09_acceptances",
        label: "承諾数",
        kind: AnswerKind::Number,
        hint: "件 / 月",
        options: &[],
    },
    HearingItem {
        key: "q10_media",
        label: "使用媒体",
        kind: AnswerKind::Multi,
        hint: "複数選択可",
        options: &[
            "Indeed",
            "求人ボックス",
            "その他の求人サイト",
            "自社サイト",
            "人材紹介",
            "ハローワーク",
            "チラシ・紙媒体",
            "その他",
        ],
    },
    HearingItem {
        key: "q11_first_contact_time",
        label: "初回連絡までの時間",
        kind: AnswerKind::Single,
        hint: "応募〜最初の連絡",
        options: &["当日中", "翌日", "2日以降"],
    },
    HearingItem {
        key: "q12_decline_reasons",
        label: "辞退理由",
        kind: AnswerKind::Text,
        hint: "把握している範囲で",
        options: &[],
    },
    HearingItem {
        key: "q13_biggest_challenge",
        label: "現在の最大課題",
        kind: AnswerKind::Text,
        hint: "顧客が感じている最大の課題",
        options: &[],
    },
    HearingItem {
        key: "q14_changeable",
        label: "変更可能な条件",
        kind: AnswerKind::Text,
        hint: "給与・休日・時間・手当など",
        options: &[],
    },
    HearingItem {
        key: "q15_fixed",
        label: "変更不可能な条件",
        kind: AnswerKind::Text,
        hint: "動かせない前提条件",
        options: &[],
    },
];

/// 「商談を前に進める欄」(P1-7)。15の必須ヒアリング項目とは別枠で、
/// 商談を次の段階へ進めるための4項目を扱う。保存キーは b01_budget 等。
/// これらは採用の実務状況ではなく「意思決定・予算・次アクション」を確認するためのもの。
pub const BUSINESS_ITEMS: [HearingItem; 4] = [
    HearingItem {
        key: "b01_budget",
        label: "採用にかけられる予算感",
        kind: AnswerKind::Text,
        hint: "例: 1名あたり◯万円 / 媒体費 月◯万円 / 未定",
        options: &[],
    },
    HearingItem {
        key: "b02_decision_maker",
        label: "意思決定に関わる人",
        kind: AnswerKind::Text,
        hint: "決裁者・関与者 (役職・人数)",
        options: &[],
    },
    HearingItem {
        key: "b03_timing",
        label: "検討時期",
        kind: AnswerKind::Single,
        hint: "導入・発注を検討するタイミング",
        options: &["すぐに", "1〜3か月以内", "半年以内", "未定"],
    },
    HearingItem {
        key: "b04_next_action",
        label: "次回アクション (日時・内容)",
        kind: AnswerKind::Text,
        hint: "例: 2026年7月20日 提案書提出 / 次回打合せ日時",
        options: &[],
    },
];

/// §13.3 動的質問。トリガー項目の回答値に応じて表示する追質問。
pub struct DynamicQuestion {
    /// トリガーとなる項目キー (HEARING_ITEMS の key)
    pub trigger_key: &'static str,
    /// トリガーとなる回答値 (完全一致)
    pub trigger_value: &'static str,
    /// 追質問のキー
    pub key: &'static str,
    /// 追質問ラベル
    pub label: &'static str,
    /// 記入ヒント
    pub hint: &'static str,
}

/// §13.3 の分岐: 月間応募数の回答帯に応じた追質問。
/// - 0〜2件 → 表示数・クリック数・配信媒体を確認
/// - 3件以上 → 接触率・面接設定率を確認
/// - 不明 → 媒体管理画面の確認可否を確認
pub const DYNAMIC_QUESTIONS: [DynamicQuestion; 5] = [
    DynamicQuestion {
        trigger_key: "q04_applications_monthly",
        trigger_value: "0〜2件",
        key: "d04a_impressions_clicks",
        label: "表示数・クリック数",
        hint: "媒体管理画面の表示回数とクリック数",
    },
    DynamicQuestion {
        trigger_key: "q04_applications_monthly",
        trigger_value: "0〜2件",
        key: "d04b_delivery_media",
        label: "配信している媒体・地域",
        hint: "どの媒体にどの地域で配信しているか",
    },
    DynamicQuestion {
        trigger_key: "q04_applications_monthly",
        trigger_value: "3件以上",
        key: "d04c_contact_rate",
        label: "接触率",
        hint: "応募のうち連絡が取れた割合",
    },
    DynamicQuestion {
        trigger_key: "q04_applications_monthly",
        trigger_value: "3件以上",
        key: "d04d_interview_set_rate",
        label: "面接設定率",
        hint: "接触のうち面接設定に至った割合",
    },
    DynamicQuestion {
        trigger_key: "q04_applications_monthly",
        trigger_value: "__unknown__",
        key: "d04e_media_console_access",
        label: "媒体管理画面の確認可否",
        hint: "表示数などの数値を後で確認できるか",
    },
];

/// 1 項目の回答値。§13.2 に沿い「不明」と「データなし」を別フラグで保持する。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AnswerValue {
    /// 回答本体 (数値/自由記述は文字列、複数選択は「A, B」形式)。未記入なら空。
    #[serde(default)]
    pub value: String,
    /// 「不明」(顧客が把握していない)
    #[serde(default)]
    pub unknown: bool,
    /// 「データなし」(そもそも計測・記録が存在しない)
    #[serde(default)]
    pub no_data: bool,
}

impl AnswerValue {
    fn is_empty(&self) -> bool {
        self.value.trim().is_empty() && !self.unknown && !self.no_data
    }
}

/// フォーム POST の生データ (キー=値) から回答マップを構築する。
///
/// 各項目 `<key>` について次の入力を集約する:
/// - `<key>`               : 本体 (数値/自由記述)。複数選択は `<key>` が複数回来るため
///                           axum の HashMap では最後の1件になる。複数選択は
///                           `<key>__multi` にカンマ連結で送る運用にする。
/// - `<key>__multi`        : 複数選択の連結値 (任意)
/// - `<key>__unknown`      : "1" なら不明
/// - `<key>__nodata`       : "1" なら データなし
///
/// HEARING_ITEMS と DYNAMIC_QUESTIONS の全キーを走査する。空回答は省略する。
pub fn answers_from_form(form: &BTreeMap<String, String>) -> BTreeMap<String, AnswerValue> {
    let mut out: BTreeMap<String, AnswerValue> = BTreeMap::new();
    let mut collect = |key: &str| {
        let multi = form
            .get(&format!("{key}__multi"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let value = multi.unwrap_or_else(|| {
            form.get(key)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        });
        let unknown = form
            .get(&format!("{key}__unknown"))
            .map(|s| s == "1" || s == "on" || s == "true")
            .unwrap_or(false);
        let no_data = form
            .get(&format!("{key}__nodata"))
            .map(|s| s == "1" || s == "on" || s == "true")
            .unwrap_or(false);
        let av = AnswerValue {
            value,
            unknown,
            no_data,
        };
        if !av.is_empty() {
            out.insert(key.to_string(), av);
        }
    };
    for item in HEARING_ITEMS.iter() {
        collect(item.key);
    }
    for item in BUSINESS_ITEMS.iter() {
        collect(item.key);
    }
    for dq in DYNAMIC_QUESTIONS.iter() {
        collect(dq.key);
    }
    out
}

/// answers_json 文字列 → 回答マップ (プリフィル用)。壊れていれば空。
pub fn answers_from_json(json: &str) -> BTreeMap<String, AnswerValue> {
    serde_json::from_str::<BTreeMap<String, AnswerValue>>(json).unwrap_or_default()
}

/// 保存済み1件 (最新 revision 取得用)
#[derive(Debug, Clone)]
pub struct StoredResult {
    pub revision: i64,
    pub answers_json: String,
    pub created_at: String,
}

/// 過去 revision の履歴エントリ (日時のみ表示用)
#[derive(Debug, Clone)]
pub struct RevisionMeta {
    pub revision: i64,
    pub created_at: String,
}

/// テーブルを初回アクセス時に作成する (冪等)。
pub fn ensure_table(db: &LocalDb) -> Result<(), String> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS consult_hearing_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            answers_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        &[],
    )?;
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_hearing_session
            ON consult_hearing_results(session_id, revision)",
        &[],
    )?;
    Ok(())
}

/// 追記オンリーで1件保存する。revision は当該 session の最大+1 (初回は1)。
/// 返り値は付与した revision。
pub fn insert_result(
    db: &LocalDb,
    session_id: &str,
    answers_json: &str,
    created_at: &str,
) -> Result<i64, String> {
    ensure_table(db)?;
    // 現在の最大 revision を取得 (0件なら 0)
    let next: i64 = db
        .query_scalar::<i64>(
            "SELECT COALESCE(MAX(revision), 0) FROM consult_hearing_results WHERE session_id = ?",
            &[&session_id as &dyn rusqlite::types::ToSql],
        )
        .unwrap_or(0)
        + 1;
    db.execute(
        "INSERT INTO consult_hearing_results (session_id, revision, answers_json, created_at)
         VALUES (?, ?, ?, ?)",
        &[
            &session_id as &dyn rusqlite::types::ToSql,
            &next,
            &answers_json,
            &created_at,
        ],
    )?;
    Ok(next)
}

/// 当該 session の最新 revision (現在値) を取得する。無ければ None。
pub fn latest_result(db: &LocalDb, session_id: &str) -> Option<StoredResult> {
    ensure_table(db).ok()?;
    let rows = db
        .query(
            "SELECT revision, answers_json, created_at
             FROM consult_hearing_results
             WHERE session_id = ?
             ORDER BY revision DESC
             LIMIT 1",
            &[&session_id as &dyn rusqlite::types::ToSql],
        )
        .ok()?;
    let row = rows.first()?;
    Some(StoredResult {
        revision: row.get("revision").and_then(Value::as_i64).unwrap_or(0),
        answers_json: row
            .get("answers_json")
            .and_then(Value::as_str)
            .unwrap_or("{}")
            .to_string(),
        created_at: row
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// 当該 session の全 revision の日時一覧 (新しい順)。
pub fn revision_history(db: &LocalDb, session_id: &str) -> Vec<RevisionMeta> {
    if ensure_table(db).is_err() {
        return Vec::new();
    }
    db.query(
        "SELECT revision, created_at
         FROM consult_hearing_results
         WHERE session_id = ?
         ORDER BY revision DESC",
        &[&session_id as &dyn rusqlite::types::ToSql],
    )
    .map(|rows| {
        rows.iter()
            .map(|r| RevisionMeta {
                revision: r.get("revision").and_then(Value::as_i64).unwrap_or(0),
                created_at: r
                    .get("created_at")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// 最新回答を evidence_pack 用の `hearing` JSON に変換する。
/// 回答が無ければ None (evidence_pack 側で省略する)。
pub fn hearing_json_for_pack(db: &LocalDb, session_id: &str) -> Option<Value> {
    let stored = latest_result(db, session_id)?;
    let answers = answers_from_json(&stored.answers_json);
    if answers.is_empty() {
        return None;
    }
    // 項目キー → { label, value, unknown, no_data } の配列 (仕様書の項目順)
    let mut items: Vec<Value> = Vec::new();
    let label_of = |key: &str| -> String {
        if let Some(it) = HEARING_ITEMS.iter().find(|i| i.key == key) {
            return it.label.to_string();
        }
        if let Some(it) = BUSINESS_ITEMS.iter().find(|i| i.key == key) {
            return it.label.to_string();
        }
        if let Some(dq) = DYNAMIC_QUESTIONS.iter().find(|d| d.key == key) {
            return dq.label.to_string();
        }
        key.to_string()
    };
    // HEARING_ITEMS → BUSINESS_ITEMS → DYNAMIC_QUESTIONS の順で出力 (決定的)
    let ordered_keys: Vec<&str> = HEARING_ITEMS
        .iter()
        .map(|i| i.key)
        .chain(BUSINESS_ITEMS.iter().map(|i| i.key))
        .chain(DYNAMIC_QUESTIONS.iter().map(|d| d.key))
        .collect();
    for key in ordered_keys {
        if let Some(av) = answers.get(key) {
            items.push(serde_json::json!({
                "key": key,
                "label": label_of(key),
                "value": av.value,
                "unknown": av.unknown,
                "no_data": av.no_data,
            }));
        }
    }
    Some(serde_json::json!({
        "revision": stored.revision,
        "recorded_at": stored.created_at,
        "items": items,
    }))
}

// =============================================================================
// 印刷用シート HTML (§13。面談に持っていく A4 記入シート)
// =============================================================================

fn internal_band() -> &'static str {
    r#"<div class="consult-internal-band" role="note">&#128274; 社内用 &#8212; 顧客配布不可 / INTERNAL USE ONLY</div>"#
}

/// ヒアリングシート専用の追加CSS (navy CSS の後に読み込む)
fn hearing_css() -> &'static str {
    r#"
/* ==== ヒアリングシート (社内用) 追加スタイル ==== */
@page {
  @bottom-left {
    content: "FOR A-CAREER  /  ヒアリングシート [社内用 - 顧客配布不可]";
    font-family: "Noto Sans JP", sans-serif;
    font-size: 8pt;
    color: #A8331F;
    letter-spacing: 0.04em;
  }
}
body.theme-navy .consult-internal-band {
  display: flex; align-items: center; gap: 8px;
  background: #FBEAE6; border: 1.5px solid #A8331F; color: #A8331F;
  font-size: 9.5pt; font-weight: 700; letter-spacing: 0.06em;
  padding: 5px 12px; margin-bottom: 3mm;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .hs-item {
  border: 1px solid var(--rule, #D8D2C4);
  border-left: 3px solid #1F2D4D;
  padding: 2mm 3mm; margin-bottom: 2.5mm;
  break-inside: avoid; page-break-inside: avoid;
}
body.theme-navy .hs-item .hs-label {
  font-weight: 700; font-size: 10pt; color: #1F2D4D;
}
body.theme-navy .hs-item .hs-no { color: #A8331F; margin-right: 4px; }
body.theme-navy .hs-item .hs-hint { font-size: 8pt; color: #6A6E7A; margin-left: 6px; }
body.theme-navy .hs-item .hs-kind {
  display: inline-block; font-size: 7.5pt; font-weight: 700; color: #1F2D4D;
  border: 1px solid #1F2D4D; border-radius: 3px; padding: 0 4px; margin-left: 6px;
}
/* 記入欄: 印刷でも罫線が出る。contenteditable で画面上も書ける */
body.theme-navy .hs-fill {
  min-height: 8mm; margin-top: 1.5mm;
  border-bottom: 1px solid #9A9486;
  background-image: repeating-linear-gradient(
    transparent, transparent 7mm, #D8D2C4 7mm, #D8D2C4 7.2mm);
  font-size: 9.5pt; line-height: 8mm; padding: 0 2mm;
  -webkit-print-color-adjust: exact; print-color-adjust: exact;
}
body.theme-navy .hs-options { font-size: 9pt; margin-top: 1mm; }
body.theme-navy .hs-options .hs-opt {
  display: inline-block; margin-right: 10px; white-space: nowrap;
}
body.theme-navy .hs-options .hs-box {
  display: inline-block; width: 3.5mm; height: 3.5mm;
  border: 1px solid #1F2D4D; margin-right: 2px; vertical-align: -0.5mm;
}
/* 「不明 / データなし」区別欄 (§13.2) */
body.theme-navy .hs-flags {
  font-size: 8.5pt; color: #6A6E7A; margin-top: 1.5mm;
}
body.theme-navy .hs-flags .hs-box { border-color: #A8331F; }
body.theme-navy .hs-branch {
  margin: 1mm 0 0 4mm; font-size: 8.5pt; color: #1F2D4D;
}
body.theme-navy .hs-branch .hs-branch-q { margin-bottom: 0.5mm; }
/* 商談を前に進める欄 (P1-7) */
body.theme-navy .hs-business-head {
  margin-top: 6mm; font-weight: 700; font-size: 12pt; color: #A8331F;
  border-bottom: 1.5px solid #A8331F; padding-bottom: 1mm;
}
body.theme-navy .hs-business-note { font-size: 8.5pt; color: #6A6E7A; margin: 1mm 0 2mm; }
body.theme-navy .hs-item.hs-business { border-left-color: #A8331F; }
"#
}

fn kind_label(kind: AnswerKind) -> &'static str {
    match kind {
        AnswerKind::Number => "数値",
        AnswerKind::Single => "単一選択",
        AnswerKind::Multi => "複数選択",
        AnswerKind::Text => "自由記述",
    }
}

/// 「不明 / データなし」の区別チェック欄 (印刷用)。
fn print_flags() -> String {
    r#"<div class="hs-flags">
  <span class="hs-opt"><span class="hs-box"></span>不明（顧客が把握していない）</span>
  <span class="hs-opt"><span class="hs-box"></span>データなし（計測・記録が存在しない）</span>
</div>"#
        .to_string()
}

/// 印刷用ヒアリングシート HTML を生成する。
/// `region`/`as_of` はヘッダ表示用 (空でも可)。
pub fn hearing_sheet_html(region: &str, as_of: &str) -> String {
    let mut html = String::with_capacity(32 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>ヒアリングシート（社内用）</title>\n<style>\n");
    html.push_str(&crate::handlers::survey::report_html::navy_css_bundle());
    html.push_str(hearing_css());
    html.push_str("</style>\n</head>\n<body class=\"theme-navy\">\n");

    html.push_str("<div class=\"page-navy\">\n");
    html.push_str(internal_band());
    html.push_str(
        r#"<div class="page-head">
  <div class="ph-sec">ヒアリングシート</div>
  <div class="ph-title">採用状況の確認シート</div>
  <div class="ph-sub">面談で確認する項目です。「不明」と「データなし」は必ず区別して記入します。</div>
  <div class="ph-rule"></div>
</div>"#,
    );

    // 対象・基準日
    html.push_str("<table class=\"table-navy\"><tbody>\n");
    let region_disp = if region.trim().is_empty() {
        "（　　　　　　　　　）".to_string()
    } else {
        escape_html(region)
    };
    html.push_str(&format!(
        "<tr><th style=\"width:28mm\">対象</th><td>{}</td><th style=\"width:24mm\">記入日</th><td>{}</td></tr>\n",
        region_disp,
        if as_of.trim().is_empty() { "　　　年　　月　　日".to_string() } else { escape_html(as_of) }
    ));
    html.push_str("</tbody></table>\n");

    // 15 項目
    for (i, item) in HEARING_ITEMS.iter().enumerate() {
        html.push_str("<div class=\"hs-item\">\n");
        html.push_str(&format!(
            "<div><span class=\"hs-no\">{:02}</span><span class=\"hs-label\">{}</span><span class=\"hs-kind\">{}</span>",
            i + 1,
            escape_html(item.label),
            kind_label(item.kind)
        ));
        if !item.hint.is_empty() {
            html.push_str(&format!(
                "<span class=\"hs-hint\">{}</span>",
                escape_html(item.hint)
            ));
        }
        html.push_str("</div>\n");

        match item.kind {
            AnswerKind::Single | AnswerKind::Multi => {
                html.push_str("<div class=\"hs-options\">");
                for opt in item.options {
                    html.push_str(&format!(
                        "<span class=\"hs-opt\"><span class=\"hs-box\"></span>{}</span>",
                        escape_html(opt)
                    ));
                }
                html.push_str("</div>\n");
                // 自由補足の記入欄
                html.push_str("<div class=\"hs-fill\" contenteditable=\"true\"></div>\n");
            }
            AnswerKind::Number | AnswerKind::Text => {
                html.push_str("<div class=\"hs-fill\" contenteditable=\"true\"></div>\n");
            }
        }

        html.push_str(&print_flags());

        // §13.3 分岐ヒント (この項目がトリガーのもの)
        let branches: Vec<&DynamicQuestion> = DYNAMIC_QUESTIONS
            .iter()
            .filter(|d| d.trigger_key == item.key)
            .collect();
        if !branches.is_empty() {
            html.push_str("<div class=\"hs-branch\">\n");
            html.push_str("<div class=\"hs-branch-q\"><strong>回答に応じて確認：</strong></div>\n");
            // trigger_value ごとにまとめて表示
            let mut shown: Vec<&str> = Vec::new();
            for b in &branches {
                if shown.contains(&b.trigger_value) {
                    continue;
                }
                shown.push(b.trigger_value);
                let cond = branch_condition_label(b.trigger_value);
                let follow: Vec<String> = branches
                    .iter()
                    .filter(|x| x.trigger_value == b.trigger_value)
                    .map(|x| escape_html(x.label))
                    .collect();
                html.push_str(&format!(
                    "<div>・{} → {}</div>\n",
                    escape_html(&cond),
                    follow.join(" / ")
                ));
            }
            html.push_str("</div>\n");
        }

        html.push_str("</div>\n");
    }

    // 商談を前に進める欄 (P1-7)。15項目とは別枠で見出しをつけて配置する。
    html.push_str(
        r#"<div class="hs-business-head">商談を前に進める欄（4項目）</div>
<p class="hs-business-note">採用の実務状況とは別に、商談を次に進めるための確認事項です。</p>"#,
    );
    for (i, item) in BUSINESS_ITEMS.iter().enumerate() {
        html.push_str("<div class=\"hs-item hs-business\">\n");
        html.push_str(&format!(
            "<div><span class=\"hs-no\">B{:02}</span><span class=\"hs-label\">{}</span><span class=\"hs-kind\">{}</span>",
            i + 1,
            escape_html(item.label),
            kind_label(item.kind)
        ));
        if !item.hint.is_empty() {
            html.push_str(&format!(
                "<span class=\"hs-hint\">{}</span>",
                escape_html(item.hint)
            ));
        }
        html.push_str("</div>\n");
        if let AnswerKind::Single = item.kind {
            html.push_str("<div class=\"hs-options\">");
            for opt in item.options {
                html.push_str(&format!(
                    "<span class=\"hs-opt\"><span class=\"hs-box\"></span>{}</span>",
                    escape_html(opt)
                ));
            }
            html.push_str("</div>\n");
        }
        html.push_str("<div class=\"hs-fill\" contenteditable=\"true\"></div>\n");
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

/// 分岐条件の表示ラベル。__unknown__ は「不明の場合」に読み替える。
fn branch_condition_label(trigger_value: &str) -> String {
    match trigger_value {
        "__unknown__" => "不明の場合".to_string(),
        other => format!("「{other}」の場合"),
    }
}

// =============================================================================
// Web 入力フォーム HTML (§13.2 + §13.3 動的質問)
// =============================================================================

/// 現在の回答をプリフィルした入力フォーム HTML を生成する。
/// `saved` が true なら保存完了メッセージを表示する。
/// `history` は過去 revision の日時一覧 (新しい順)。
pub fn hearing_form_html(
    session_id: &str,
    region: &str,
    answers: &BTreeMap<String, AnswerValue>,
    saved: bool,
    saved_revision: Option<i64>,
    history: &[RevisionMeta],
) -> String {
    let sid = escape_html(session_id);
    let mut html = String::with_capacity(48 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>ヒアリング入力（社内用）</title>\n");
    html.push_str("<style>\n");
    html.push_str(form_css());
    html.push_str("</style>\n</head>\n<body>\n");

    html.push_str("<div class=\"wrap\">\n");
    html.push_str(r#"<div class="band">🔒 社内用 — 顧客配布不可 / INTERNAL USE ONLY</div>"#);
    html.push_str("<h1>ヒアリング入力</h1>\n");
    html.push_str(&format!(
        "<p class=\"lead\">面談で確認した採用状況を記録します。対象: <strong>{}</strong>。「不明」と「データなし」は分けて記録してください。保存すると追記され、最新が現在値になります。</p>\n",
        if region.trim().is_empty() { "（未特定）".to_string() } else { escape_html(region) }
    ));

    if saved {
        let rev = saved_revision
            .map(|r| format!("（版 {r}）"))
            .unwrap_or_default();
        html.push_str(&format!(
            "<div class=\"saved\">保存しました{rev}。以下は最新の回答です。</div>\n"
        ));
    }

    html.push_str(&format!(
        "<form method=\"post\" action=\"/consult/hearing?session_id={sid}\">\n"
    ));

    for (i, item) in HEARING_ITEMS.iter().enumerate() {
        let cur = answers.get(item.key);
        html.push_str(&render_form_item(i + 1, item, cur));
        // §13.3 動的追質問 (JS で表示切替する容器)
        render_dynamic_block(&mut html, item, answers);
    }

    // 商談を前に進める欄 (P1-7)。15項目とは別枠。
    html.push_str(
        "<h2 class=\"biz-head\">商談を前に進める欄</h2>\n<p class=\"biz-note\">採用の実務状況とは別に、商談を次に進めるための確認事項です。</p>\n",
    );
    for (i, item) in BUSINESS_ITEMS.iter().enumerate() {
        let cur = answers.get(item.key);
        html.push_str(&render_form_item_labeled(
            &format!("B{:02}", i + 1),
            item,
            cur,
        ));
    }

    html.push_str(
        r#"<div class="actions"><button type="submit">保存する</button></div>
</form>
"#,
    );

    // 履歴一覧 (日時のみ)
    if !history.is_empty() {
        html.push_str("<div class=\"history\">\n<h2>更新履歴</h2>\n<ul>\n");
        for h in history {
            html.push_str(&format!(
                "<li>版 {} — {}</li>\n",
                h.revision,
                escape_html(&h.created_at)
            ));
        }
        html.push_str("</ul>\n</div>\n");
    }

    html.push_str("</div>\n");
    html.push_str(&form_js());
    html.push_str("</body>\n</html>\n");
    html
}

/// 1 項目の入力欄 (本体 + 不明/データなしトグル)。番号は 2 桁ゼロ埋め。
fn render_form_item(no: usize, item: &HearingItem, cur: Option<&AnswerValue>) -> String {
    render_form_item_labeled(&format!("{no:02}"), item, cur)
}

/// 番号ラベル (「01」「B01」等) を指定して入力欄を描画する。
fn render_form_item_labeled(
    no_label: &str,
    item: &HearingItem,
    cur: Option<&AnswerValue>,
) -> String {
    let val = cur.map(|a| a.value.as_str()).unwrap_or("");
    let unknown = cur.map(|a| a.unknown).unwrap_or(false);
    let no_data = cur.map(|a| a.no_data).unwrap_or(false);
    let key = item.key;
    let mut s = String::new();
    s.push_str(&format!(
        "<fieldset class=\"item\"><legend><span class=\"no\">{no}</span> {label} <span class=\"kind\">{kind}</span></legend>\n",
        no = escape_html(no_label),
        label = escape_html(item.label),
        kind = kind_label(item.kind)
    ));
    if !item.hint.is_empty() {
        s.push_str(&format!(
            "<p class=\"hint\">{}</p>\n",
            escape_html(item.hint)
        ));
    }

    match item.kind {
        AnswerKind::Number => {
            s.push_str(&format!(
                "<input type=\"number\" name=\"{key}\" value=\"{v}\" data-hkey=\"{key}\" class=\"in\">\n",
                v = escape_html(val)
            ));
        }
        AnswerKind::Text => {
            s.push_str(&format!(
                "<input type=\"text\" name=\"{key}\" value=\"{v}\" data-hkey=\"{key}\" class=\"in\">\n",
                v = escape_html(val)
            ));
        }
        AnswerKind::Single => {
            s.push_str(&format!(
                "<select name=\"{key}\" data-hkey=\"{key}\" class=\"in\">\n"
            ));
            s.push_str("<option value=\"\">選択してください</option>\n");
            for opt in item.options {
                let sel = if val == *opt { " selected" } else { "" };
                s.push_str(&format!(
                    "<option value=\"{o}\"{sel}>{o}</option>\n",
                    o = escape_html(opt)
                ));
            }
            s.push_str("</select>\n");
        }
        AnswerKind::Multi => {
            // 複数選択: チェックボックス群 + 隠しフィールド (__multi) に連結して送る
            let selected: Vec<&str> = val.split(',').map(|s| s.trim()).collect();
            s.push_str(&format!("<div class=\"multi\" data-hkey=\"{key}\">\n"));
            for opt in item.options {
                let checked = if selected.contains(opt) {
                    " checked"
                } else {
                    ""
                };
                s.push_str(&format!(
                    "<label class=\"chk\"><input type=\"checkbox\" class=\"multi-opt\" data-key=\"{key}\" value=\"{o}\"{checked}> {o}</label>\n",
                    o = escape_html(opt)
                ));
            }
            s.push_str(&format!(
                "<input type=\"hidden\" name=\"{key}__multi\" value=\"{v}\">\n",
                v = escape_html(val)
            ));
            s.push_str("</div>\n");
        }
    }

    // 不明 / データなし トグル (§13.2)
    s.push_str("<div class=\"flags\">\n");
    s.push_str(&format!(
        "<label class=\"flag\"><input type=\"checkbox\" name=\"{key}__unknown\" value=\"1\"{u}> 不明</label>\n",
        u = if unknown { " checked" } else { "" }
    ));
    s.push_str(&format!(
        "<label class=\"flag\"><input type=\"checkbox\" name=\"{key}__nodata\" value=\"1\"{n}> データなし</label>\n",
        n = if no_data { " checked" } else { "" }
    ));
    s.push_str("</div>\n");
    s.push_str("</fieldset>\n");
    s
}

/// §13.3 動的追質問ブロック (トリガー項目の直後に配置し、JS で表示切替)。
fn render_dynamic_block(
    html: &mut String,
    item: &HearingItem,
    answers: &BTreeMap<String, AnswerValue>,
) {
    let branches: Vec<&DynamicQuestion> = DYNAMIC_QUESTIONS
        .iter()
        .filter(|d| d.trigger_key == item.key)
        .collect();
    if branches.is_empty() {
        return;
    }
    // trigger_value ごとにグループ化して表示
    let mut shown: Vec<&str> = Vec::new();
    for b in &branches {
        if shown.contains(&b.trigger_value) {
            continue;
        }
        shown.push(b.trigger_value);
        let group: Vec<&&DynamicQuestion> = branches
            .iter()
            .filter(|x| x.trigger_value == b.trigger_value)
            .collect();
        html.push_str(&format!(
            "<div class=\"dyn\" data-trigger-key=\"{tk}\" data-trigger-value=\"{tv}\">\n",
            tk = escape_html(item.key),
            tv = escape_html(b.trigger_value)
        ));
        html.push_str(&format!(
            "<div class=\"dyn-head\">追加確認（{}）</div>\n",
            escape_html(&branch_condition_label(b.trigger_value))
        ));
        for dq in group {
            let cur = answers.get(dq.key);
            let val = cur.map(|a| a.value.as_str()).unwrap_or("");
            let unknown = cur.map(|a| a.unknown).unwrap_or(false);
            let no_data = cur.map(|a| a.no_data).unwrap_or(false);
            html.push_str(&format!(
                "<div class=\"dyn-item\"><label>{label}</label>",
                label = escape_html(dq.label)
            ));
            if !dq.hint.is_empty() {
                html.push_str(&format!(
                    "<span class=\"hint\">{}</span>",
                    escape_html(dq.hint)
                ));
            }
            html.push_str(&format!(
                "<input type=\"text\" name=\"{k}\" value=\"{v}\" class=\"in\">\n",
                k = dq.key,
                v = escape_html(val)
            ));
            html.push_str("<div class=\"flags\">\n");
            html.push_str(&format!(
                "<label class=\"flag\"><input type=\"checkbox\" name=\"{k}__unknown\" value=\"1\"{u}> 不明</label>\n",
                k = dq.key, u = if unknown { " checked" } else { "" }
            ));
            html.push_str(&format!(
                "<label class=\"flag\"><input type=\"checkbox\" name=\"{k}__nodata\" value=\"1\"{n}> データなし</label>\n",
                k = dq.key, n = if no_data { " checked" } else { "" }
            ));
            html.push_str("</div>\n</div>\n");
        }
        html.push_str("</div>\n");
    }
}

fn form_css() -> &'static str {
    r#"
* { box-sizing: border-box; }
body { font-family: "Noto Sans JP", system-ui, sans-serif; background: #0F172A; color: #E2E8F0; margin: 0; padding: 24px 12px; }
.wrap { max-width: 760px; margin: 0 auto; }
.band { background: #7F1D1D; border: 1px solid #B91C1C; color: #FECACA; font-weight: 700; font-size: 13px; padding: 6px 12px; border-radius: 6px; letter-spacing: .04em; }
h1 { font-size: 20px; margin: 16px 0 8px; }
h2 { font-size: 15px; margin: 20px 0 8px; color: #CBD5E1; }
.lead { font-size: 13px; color: #94A3B8; line-height: 1.7; }
.saved { background: #064E3B; border: 1px solid #059669; color: #A7F3D0; padding: 8px 12px; border-radius: 6px; margin: 12px 0; font-size: 13px; }
fieldset.item { border: 1px solid #334155; border-left: 3px solid #38BDF8; border-radius: 6px; padding: 10px 12px; margin: 12px 0; }
legend { font-weight: 700; font-size: 14px; padding: 0 6px; }
legend .no { color: #F87171; margin-right: 4px; }
legend .kind { font-size: 10px; font-weight: 700; border: 1px solid #64748B; border-radius: 3px; padding: 0 5px; margin-left: 6px; color: #94A3B8; }
.hint { font-size: 11px; color: #94A3B8; margin: 2px 0 6px; }
.in { width: 100%; padding: 8px 10px; background: #1E293B; border: 1px solid #475569; border-radius: 4px; color: #fff; font-size: 14px; }
.multi { display: flex; flex-wrap: wrap; gap: 6px 14px; }
.chk { font-size: 13px; display: inline-flex; align-items: center; gap: 4px; }
.flags { margin-top: 8px; display: flex; gap: 16px; }
.flag { font-size: 12px; color: #FCA5A5; display: inline-flex; align-items: center; gap: 4px; }
.dyn { border: 1px dashed #475569; border-radius: 6px; padding: 8px 12px; margin: 0 0 12px 12px; background: #111C33; }
.dyn-head { font-size: 12px; font-weight: 700; color: #7DD3FC; margin-bottom: 6px; }
.dyn-item { margin-bottom: 8px; }
.dyn-item label { font-size: 12px; color: #CBD5E1; margin-right: 6px; }
.actions { margin: 20px 0; }
button { background: #B91C1C; color: #fff; border: none; padding: 12px 28px; border-radius: 6px; font-size: 15px; font-weight: 700; cursor: pointer; min-height: 44px; }
button:hover { background: #DC2626; }
.history ul { list-style: none; padding: 0; font-size: 12px; color: #94A3B8; }
.history li { padding: 4px 0; border-bottom: 1px solid #1E293B; }
.biz-head { color: #FCA5A5; border-bottom: 1px solid #7F1D1D; padding-bottom: 4px; }
.biz-note { font-size: 12px; color: #94A3B8; margin: 4px 0 8px; }
"#
}

fn form_js() -> String {
    // 複数選択チェックボックス → __multi 隠しフィールドへ連結。
    // 動的追質問ブロックの表示切替 (トリガー項目の現在値に応じて)。
    r#"<script>
(function() {
  function syncMulti(key) {
    var boxes = document.querySelectorAll('.multi-opt[data-key="' + key + '"]');
    var vals = [];
    boxes.forEach(function(b) { if (b.checked) vals.push(b.value); });
    var hidden = document.querySelector('input[name="' + key + '__multi"]');
    if (hidden) hidden.value = vals.join(', ');
  }
  document.querySelectorAll('.multi-opt').forEach(function(b) {
    b.addEventListener('change', function() { syncMulti(b.getAttribute('data-key')); });
  });
  // 動的質問の表示切替
  function currentValue(key) {
    var el = document.querySelector('[data-hkey="' + key + '"]');
    if (!el) return '';
    if (el.tagName === 'SELECT' || el.tagName === 'INPUT') return (el.value || '').trim();
    return '';
  }
  function unknownChecked(key) {
    var u = document.querySelector('input[name="' + key + '__unknown"]');
    return !!(u && u.checked);
  }
  function refreshDynamic() {
    document.querySelectorAll('.dyn').forEach(function(d) {
      var tk = d.getAttribute('data-trigger-key');
      var tv = d.getAttribute('data-trigger-value');
      var show;
      if (tv === '__unknown__') {
        show = unknownChecked(tk);
      } else {
        show = currentValue(tk) === tv;
      }
      d.style.display = show ? '' : 'none';
    });
  }
  document.querySelectorAll('[data-hkey]').forEach(function(el) {
    el.addEventListener('change', refreshDynamic);
    el.addEventListener('input', refreshDynamic);
  });
  document.querySelectorAll('input[type="checkbox"][name$="__unknown"]').forEach(function(el) {
    el.addEventListener('change', refreshDynamic);
  });
  refreshDynamic();
})();
</script>
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空の tempfile DB から LocalDb を作る (local_sqlite の既存テストと同方式)。
    fn temp_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = LocalDb::new(tmp.path().to_str().unwrap()).unwrap();
        (tmp, db)
    }

    fn form_kv(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn ensure_table_is_idempotent() {
        let (_t, db) = temp_db();
        assert!(ensure_table(&db).is_ok());
        // 2回目でも失敗しない
        assert!(ensure_table(&db).is_ok());
        let count: i64 = db
            .query_scalar("SELECT COUNT(*) FROM consult_hearing_results", &[])
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn insert_assigns_incrementing_revisions_per_session() {
        let (_t, db) = temp_db();
        let r1 = insert_result(&db, "S1", "{\"a\":1}", "2026-07-11T10:00:00+09:00").unwrap();
        let r2 = insert_result(&db, "S1", "{\"a\":2}", "2026-07-11T11:00:00+09:00").unwrap();
        // 別セッションは独立に 1 から
        let other = insert_result(&db, "S2", "{\"b\":1}", "2026-07-11T12:00:00+09:00").unwrap();
        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
        assert_eq!(other, 1, "セッションごとに revision は独立");
    }

    #[test]
    fn latest_result_returns_highest_revision() {
        let (_t, db) = temp_db();
        insert_result(&db, "S1", "{\"v\":\"first\"}", "2026-07-11T10:00:00+09:00").unwrap();
        insert_result(&db, "S1", "{\"v\":\"second\"}", "2026-07-11T11:00:00+09:00").unwrap();
        let latest = latest_result(&db, "S1").unwrap();
        assert_eq!(latest.revision, 2);
        assert!(latest.answers_json.contains("second"));
        // 追記オンリー: 過去 revision は残る (2行)
        let count: i64 = db
            .query_scalar(
                "SELECT COUNT(*) FROM consult_hearing_results WHERE session_id = 'S1'",
                &[],
            )
            .unwrap();
        assert_eq!(count, 2, "UPDATE ではなく追記されている");
    }

    #[test]
    fn latest_result_none_when_empty() {
        let (_t, db) = temp_db();
        assert!(latest_result(&db, "NOPE").is_none());
    }

    #[test]
    fn revision_history_is_descending() {
        let (_t, db) = temp_db();
        insert_result(&db, "S1", "{}", "2026-07-11T10:00:00+09:00").unwrap();
        insert_result(&db, "S1", "{}", "2026-07-11T11:00:00+09:00").unwrap();
        insert_result(&db, "S1", "{}", "2026-07-11T12:00:00+09:00").unwrap();
        let hist = revision_history(&db, "S1");
        assert_eq!(hist.len(), 3);
        assert_eq!(hist[0].revision, 3);
        assert_eq!(hist[2].revision, 1);
    }

    #[test]
    fn answers_from_form_distinguishes_unknown_and_nodata() {
        let form = form_kv(&[
            ("q01_hiring_count", "3"),
            ("q13_biggest_challenge", "応募が来ない"),
            ("q05_contacts__unknown", "1"),
            ("q08_offers__nodata", "1"),
            // 空値 + フラグ無し → 省略される
            ("q02_deadline", "  "),
        ]);
        let answers = answers_from_form(&form);
        assert_eq!(answers.get("q01_hiring_count").unwrap().value, "3");
        assert!(answers.get("q05_contacts").unwrap().unknown);
        assert!(!answers.get("q05_contacts").unwrap().no_data);
        assert!(answers.get("q08_offers").unwrap().no_data);
        assert!(!answers.get("q08_offers").unwrap().unknown);
        assert!(answers.get("q02_deadline").is_none(), "空回答は省略される");
    }

    #[test]
    fn answers_multi_uses_multi_field() {
        let form = form_kv(&[("q10_media__multi", "Indeed, 自社サイト")]);
        let answers = answers_from_form(&form);
        assert_eq!(
            answers.get("q10_media").unwrap().value,
            "Indeed, 自社サイト"
        );
    }

    #[test]
    fn form_html_has_15_items_and_flags() {
        let answers = BTreeMap::new();
        let html = hearing_form_html("SID1", "群馬県 高崎市", &answers, false, None, &[]);
        // 15 項目のラベルが全て含まれる
        for item in HEARING_ITEMS.iter() {
            assert!(
                html.contains(item.label),
                "項目ラベル {} がフォームにない",
                item.label
            );
        }
        // 「不明」「データなし」の区別トグルが各項目にある (15 + 商談4 + 動的質問)
        assert_eq!(
            html.matches("__unknown\" value=\"1\"").count(),
            HEARING_ITEMS.len() + BUSINESS_ITEMS.len() + DYNAMIC_QUESTIONS.len(),
            "不明トグルが全項目 + 商談欄 + 動的質問にある"
        );
        assert_eq!(
            html.matches("__nodata\" value=\"1\"").count(),
            HEARING_ITEMS.len() + BUSINESS_ITEMS.len() + DYNAMIC_QUESTIONS.len(),
            "データなしトグルが全項目 + 商談欄 + 動的質問にある"
        );
        // 商談を前に進める欄の4項目 (P1-7)
        for item in BUSINESS_ITEMS.iter() {
            assert!(
                html.contains(item.label),
                "商談欄 {} がフォームにない",
                item.label
            );
        }
        assert!(html.contains("商談を前に進める欄"));
        // 社内用帯 + 顧客配布不可
        assert!(html.contains("社内用"));
        assert!(html.contains("顧客配布不可"));
    }

    #[test]
    fn form_html_prefills_and_shows_saved_and_history() {
        let mut answers = BTreeMap::new();
        answers.insert(
            "q13_biggest_challenge".to_string(),
            AnswerValue {
                value: "母集団形成".to_string(),
                unknown: false,
                no_data: false,
            },
        );
        let history = vec![
            RevisionMeta {
                revision: 2,
                created_at: "2026-07-11T11:00:00+09:00".to_string(),
            },
            RevisionMeta {
                revision: 1,
                created_at: "2026-07-11T10:00:00+09:00".to_string(),
            },
        ];
        let html = hearing_form_html("SID1", "群馬県", &answers, true, Some(2), &history);
        assert!(html.contains("母集団形成"), "既存回答がプリフィルされる");
        assert!(html.contains("保存しました"), "保存済みメッセージ");
        assert!(html.contains("更新履歴"));
        assert!(html.contains("版 2"));
        assert!(html.contains("版 1"));
    }

    #[test]
    fn sheet_html_has_structure_and_branch_hints() {
        let html = hearing_sheet_html("群馬県 高崎市", "2026-07-11");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("theme-navy"));
        assert!(html.contains("社内用"));
        // 15 項目
        for item in HEARING_ITEMS.iter() {
            assert!(
                html.contains(item.label),
                "シートに項目 {} がない",
                item.label
            );
        }
        // 「不明」「データなし」の区別欄 (§13.2)
        assert!(html.contains("不明（顧客が把握していない）"));
        assert!(html.contains("データなし（計測・記録が存在しない）"));
        // §13.3 分岐ヒント (応募数の帯に応じた追質問)
        assert!(html.contains("回答に応じて確認"));
        assert!(html.contains("表示数・クリック数"));
        assert!(html.contains("接触率"));
        assert!(html.contains("媒体管理画面の確認可否"));
        // contenteditable の記入欄
        assert!(html.contains("contenteditable=\"true\""));
    }

    #[test]
    fn hearing_json_for_pack_reflects_latest_answers() {
        let (_t, db) = temp_db();
        // 回答なし → None
        assert!(hearing_json_for_pack(&db, "S1").is_none());
        // 保存後 → items に反映
        let mut answers = BTreeMap::new();
        answers.insert(
            "q01_hiring_count".to_string(),
            AnswerValue {
                value: "3".to_string(),
                unknown: false,
                no_data: false,
            },
        );
        answers.insert(
            "q05_contacts".to_string(),
            AnswerValue {
                value: String::new(),
                unknown: true,
                no_data: false,
            },
        );
        let json = serde_json::to_string(&answers).unwrap();
        insert_result(&db, "S1", &json, "2026-07-11T10:00:00+09:00").unwrap();

        let pack = hearing_json_for_pack(&db, "S1").unwrap();
        assert_eq!(pack["revision"].as_i64(), Some(1));
        let items = pack["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        // 項目順 (HEARING_ITEMS 順): q01 が先
        assert_eq!(items[0]["key"].as_str(), Some("q01_hiring_count"));
        assert_eq!(items[0]["label"].as_str(), Some("採用人数"));
        assert_eq!(items[0]["value"].as_str(), Some("3"));
        assert_eq!(items[1]["key"].as_str(), Some("q05_contacts"));
        assert_eq!(items[1]["unknown"].as_bool(), Some(true));
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<&str> = HEARING_ITEMS
            .iter()
            .map(|i| i.key)
            .chain(BUSINESS_ITEMS.iter().map(|i| i.key))
            .chain(DYNAMIC_QUESTIONS.iter().map(|d| d.key))
            .collect();
        let n = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n, "項目/商談欄/動的質問のキーが重複している");
    }

    #[test]
    fn business_items_persist_and_appear_in_sheet() {
        // P1-7: 商談欄 (b01_budget 等) が保存キーとして保持される
        let form = form_kv(&[
            ("b01_budget", "1名30万円"),
            ("b03_timing", "1〜3か月以内"),
            ("b04_next_action", "7月20日 提案"),
        ]);
        let answers = answers_from_form(&form);
        assert_eq!(answers.get("b01_budget").unwrap().value, "1名30万円");
        assert_eq!(answers.get("b03_timing").unwrap().value, "1〜3か月以内");
        // 印刷シートにも商談欄が出る
        let sheet = hearing_sheet_html("群馬県", "2026-07-11");
        assert!(sheet.contains("商談を前に進める欄"));
        for item in BUSINESS_ITEMS.iter() {
            assert!(sheet.contains(item.label), "シートに {} がない", item.label);
        }
    }

    /// 視覚確認用フィクスチャ出力 (P1-7 の商談欄を含むヒアリングシート/フォーム)。
    /// CONSULT_HEARING_FIXTURE_OUT にディレクトリを指定して実行すると HTML を書き出す。
    #[test]
    fn write_hearing_fixture_when_env_set() {
        let Ok(dir) = std::env::var("CONSULT_HEARING_FIXTURE_OUT") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("hearing_sheet_v3.html"),
            hearing_sheet_html("群馬県 高崎市", "2026-07-11"),
        )
        .unwrap();
        let answers = BTreeMap::new();
        std::fs::write(
            dir.join("hearing_form_v3.html"),
            hearing_form_html("SID1", "群馬県 高崎市", &answers, false, None, &[]),
        )
        .unwrap();
    }

    #[test]
    fn round_trip_json_serialization() {
        let form = form_kv(&[
            ("q01_hiring_count", "5"),
            ("q10_media__multi", "自社サイト, 人材紹介"),
            ("q05_contacts__unknown", "1"),
        ]);
        let answers = answers_from_form(&form);
        let json = serde_json::to_string(&answers).unwrap();
        let back = answers_from_json(&json);
        assert_eq!(answers, back);
    }
}
