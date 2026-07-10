//! ヒアリング後の仮説更新 (計画書 §Phase5 / §24-7。フェーズD)
//!
//! - 面談前に生成した仮説一覧を、最新ヒアリング回答をもとに「支持 / 否定 / 保留」に更新する。
//! - 各仮説について、回答からルールで**自動判定の提案**を出す (`auto_suggest`)。
//!   判定ルールは config.rs の定数と本モジュールの純関数で構成し、テスト可能にする。
//! - コンサルが提案と異なる値へ**手動確定**でき、理由メモを付けられる (§Phase5)。
//! - 保存はローカル SQLite の consult_hypothesis_reviews テーブルへ追記オンリー。
//!   revision で修正前後の差分を追える (hearing.rs と同じ方式)。Turso には書き込まない。
//!
//! V2ルール: 介護データ・HW由来データは一切参照しない。ローカル SQLite のみ。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::escape_html;

use super::config;
use super::evidence_pack::ConsultAnalysis;
use super::hearing::AnswerValue;
use super::hypotheses::{Hypothesis, HypothesisCategory};

/// 仮説の判定 (§Phase5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// 支持 (ヒアリングで確からしいと確認)
    Support,
    /// 否定 (今回は当てはまらないと確認)
    Reject,
    /// 保留 (データ確認待ち等、まだ判断できない)
    Hold,
}

impl Decision {
    pub fn label_ja(&self) -> &'static str {
        match self {
            Decision::Support => "支持",
            Decision::Reject => "否定",
            Decision::Hold => "保留",
        }
    }
    pub fn as_form_value(&self) -> &'static str {
        match self {
            Decision::Support => "support",
            Decision::Reject => "reject",
            Decision::Hold => "hold",
        }
    }
    pub fn from_form_value(s: &str) -> Option<Decision> {
        match s {
            "support" => Some(Decision::Support),
            "reject" => Some(Decision::Reject),
            "hold" => Some(Decision::Hold),
            _ => None,
        }
    }
}

/// 1仮説の更新レコード (§Phase5。auto_suggestion と decision の差分で修正が追える)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisReview {
    pub hypothesis_id: String,
    /// 自動判定の提案 (回答からルールで算出)
    pub auto_suggestion: Decision,
    /// コンサルの確定 (提案と異なることがある)
    pub decision: Decision,
    /// 理由メモ (任意)
    #[serde(default)]
    pub note: String,
}

// =============================================================================
// 自動判定の提案ルール (config 定数 + 純関数。テスト可能)
// =============================================================================

/// 回答マップから数値を取り出す (不明/データなし/未記入は None)。
fn num(answers: &BTreeMap<String, AnswerValue>, key: &str) -> Option<f64> {
    match answers.get(key) {
        Some(av) if av.unknown || av.no_data => None,
        Some(av) => {
            let v = av.value.trim().replace(',', "");
            if v.is_empty() {
                None
            } else {
                v.parse::<f64>().ok()
            }
        }
        None => None,
    }
}

/// 単一選択などの文字列値を取り出す (不明/データなし/未記入は None)。
fn text(answers: &BTreeMap<String, AnswerValue>, key: &str) -> Option<String> {
    match answers.get(key) {
        Some(av) if av.unknown || av.no_data => None,
        Some(av) => {
            let v = av.value.trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        }
        None => None,
    }
}

/// 仮説1件に対する自動判定の提案 (§12.6 の分岐の考え方をルール化)。
///
/// 方針 (顧客向けではなく提案なので断定的に見えるが、あくまで初期値。コンサルが確定する):
/// - 応募が少ない帯 (0〜2件) で、応募後や選考カテゴリの仮説 → データ不足のため **保留**
/// - 応募後対応カテゴリ: 初回連絡が「2日以降」→ **支持** 方向 / 「当日中」→ **否定** 方向
/// - 選考・内定カテゴリ: 面接実施はあるが承諾が0/少 → **支持** 方向
/// - 定着・離職カテゴリ: 採用理由が「欠員補充」→ **支持** 方向 / 「増員」→ **保留**
/// - 集客・媒体 / 求人訴求 / 市場構造 / 採用条件: 直接の反証データが無ければ **保留**
///   (関連する数値が「不明/データなし」の場合も保留=データ確認待ち)
/// - 上記に当てはまらない場合は **保留** (安全側。データ確認待ち)
pub fn auto_suggest(hypothesis: &Hypothesis, answers: &BTreeMap<String, AnswerValue>) -> Decision {
    let apps_band = text(answers, "q04_applications_monthly");
    let few_apps = apps_band.as_deref() == Some("0〜2件");
    let first_contact = text(answers, "q11_first_contact_time");
    let reason = text(answers, "q03_reason");

    match hypothesis.category {
        // 応募後対応: 初回連絡の速さで方向づけ
        HypothesisCategory::PostApplication => {
            if let Some(fc) = first_contact.as_deref() {
                if fc == config::FIRST_CONTACT_SLOW_VALUE {
                    return Decision::Support;
                }
                if fc == "当日中" {
                    // 初動が速い → 応募後対応が主因である可能性は下がる
                    // ただし応募自体が少ないと接触の母数が無いため断定せず保留
                    return if few_apps {
                        Decision::Hold
                    } else {
                        Decision::Reject
                    };
                }
            }
            Decision::Hold
        }
        // 選考・内定: 面接実施ありで承諾が細い → 支持方向
        HypothesisCategory::Selection => {
            let done = num(answers, "q07_interviews_done");
            let offers = num(answers, "q08_offers");
            let acceptances = num(answers, "q09_acceptances");
            match (done, offers, acceptances) {
                (Some(d), _, Some(acc)) if d > 0.0 && acc == 0.0 => Decision::Support,
                (_, Some(o), Some(acc)) if o > 0.0 && acc / o < 0.5 => Decision::Support,
                _ => Decision::Hold,
            }
        }
        // 定着・離職: 採用理由が欠員補充なら支持方向
        HypothesisCategory::Retention => match reason.as_deref() {
            Some("欠員補充") => Decision::Support,
            Some("増員") => Decision::Hold,
            _ => Decision::Hold,
        },
        // それ以外 (集客・媒体 / 求人訴求 / 市場構造 / 採用条件 / 採用目標設計):
        // 面談前の市場仮説。ヒアリングで直接反証が取れない限りデータ確認待ち=保留。
        // 応募が少なく表示数などが不明なケースも含め、この段階では常に「保留 (データ確認待ち)」。
        // コンサルが市場データと会話をもとに支持/否定へ確定する。
        _ => Decision::Hold,
    }
}

/// 全仮説について自動提案を並べる (画面初期値用)。
pub fn auto_suggest_all(
    analysis: &ConsultAnalysis,
    answers: &BTreeMap<String, AnswerValue>,
) -> Vec<HypothesisReview> {
    analysis
        .hypotheses
        .iter()
        .map(|h| {
            let s = auto_suggest(h, answers);
            HypothesisReview {
                hypothesis_id: h.hypothesis_id.clone(),
                auto_suggestion: s,
                decision: s,
                note: String::new(),
            }
        })
        .collect()
}

// =============================================================================
// 保存 (ローカル SQLite・追記オンリー・revision。hearing.rs と同方式)
// =============================================================================

/// 保存済み1件 (最新 revision 取得用)
#[derive(Debug, Clone)]
pub struct StoredReviews {
    pub revision: i64,
    pub reviews_json: String,
    pub created_at: String,
}

/// テーブルを初回アクセス時に作成する (冪等)。
pub fn ensure_table(db: &LocalDb) -> Result<(), String> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS consult_hypothesis_reviews (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            reviews_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        &[],
    )?;
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_hyp_review_session
            ON consult_hypothesis_reviews(session_id, revision)",
        &[],
    )?;
    Ok(())
}

/// 追記オンリーで1件保存する。revision は当該 session の最大+1 (初回は1)。返り値は付与した revision。
pub fn insert_reviews(
    db: &LocalDb,
    session_id: &str,
    reviews_json: &str,
    created_at: &str,
) -> Result<i64, String> {
    ensure_table(db)?;
    let next: i64 = db
        .query_scalar::<i64>(
            "SELECT COALESCE(MAX(revision), 0) FROM consult_hypothesis_reviews WHERE session_id = ?",
            &[&session_id as &dyn rusqlite::types::ToSql],
        )
        .unwrap_or(0)
        + 1;
    db.execute(
        "INSERT INTO consult_hypothesis_reviews (session_id, revision, reviews_json, created_at)
         VALUES (?, ?, ?, ?)",
        &[
            &session_id as &dyn rusqlite::types::ToSql,
            &next,
            &reviews_json,
            &created_at,
        ],
    )?;
    Ok(next)
}

/// 当該 session の最新 revision を取得する。無ければ None。
pub fn latest_reviews(db: &LocalDb, session_id: &str) -> Option<StoredReviews> {
    ensure_table(db).ok()?;
    let rows = db
        .query(
            "SELECT revision, reviews_json, created_at
             FROM consult_hypothesis_reviews
             WHERE session_id = ?
             ORDER BY revision DESC
             LIMIT 1",
            &[&session_id as &dyn rusqlite::types::ToSql],
        )
        .ok()?;
    let row = rows.first()?;
    Some(StoredReviews {
        revision: row.get("revision").and_then(Value::as_i64).unwrap_or(0),
        reviews_json: row
            .get("reviews_json")
            .and_then(Value::as_str)
            .unwrap_or("[]")
            .to_string(),
        created_at: row
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// reviews_json 文字列 → レビュー配列 (プリフィル用)。壊れていれば空。
pub fn reviews_from_json(json: &str) -> Vec<HypothesisReview> {
    serde_json::from_str::<Vec<HypothesisReview>>(json).unwrap_or_default()
}

/// 過去 revision の日時一覧 (新しい順)。
pub fn revision_history(db: &LocalDb, session_id: &str) -> Vec<(i64, String)> {
    if ensure_table(db).is_err() {
        return Vec::new();
    }
    db.query(
        "SELECT revision, created_at
         FROM consult_hypothesis_reviews
         WHERE session_id = ?
         ORDER BY revision DESC",
        &[&session_id as &dyn rusqlite::types::ToSql],
    )
    .map(|rows| {
        rows.iter()
            .map(|r| {
                (
                    r.get("revision").and_then(Value::as_i64).unwrap_or(0),
                    r.get("created_at")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

/// フォーム POST から仮説更新を構築する。
///
/// 各仮説 `<hid>` について:
/// - `decision__<hid>`     : support / reject / hold
/// - `auto__<hid>`         : 元の自動提案 (差分保持のため hidden で送る)
/// - `note__<hid>`         : 理由メモ
///
/// analysis 内に存在する仮説IDのみ受理する (未知IDは無視)。
pub fn reviews_from_form(
    analysis: &ConsultAnalysis,
    answers: &BTreeMap<String, AnswerValue>,
    form: &BTreeMap<String, String>,
) -> Vec<HypothesisReview> {
    analysis
        .hypotheses
        .iter()
        .map(|h| {
            let hid = &h.hypothesis_id;
            // 自動提案: フォームの hidden 優先、無ければ再計算
            let auto = form
                .get(&format!("auto__{hid}"))
                .and_then(|s| Decision::from_form_value(s))
                .unwrap_or_else(|| auto_suggest(h, answers));
            let decision = form
                .get(&format!("decision__{hid}"))
                .and_then(|s| Decision::from_form_value(s))
                .unwrap_or(auto);
            let note = form
                .get(&format!("note__{hid}"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            HypothesisReview {
                hypothesis_id: hid.clone(),
                auto_suggestion: auto,
                decision,
                note,
            }
        })
        .collect()
}

// =============================================================================
// 画面 HTML (社内用。仮説一覧 + 自動提案 + 手動確定 + 理由メモ)
// =============================================================================

fn review_css() -> &'static str {
    r#"
* { box-sizing: border-box; }
body { font-family: "Noto Sans JP", system-ui, sans-serif; background: #0F172A; color: #E2E8F0; margin: 0; padding: 24px 12px; }
.wrap { max-width: 900px; margin: 0 auto; }
.band { background: #7F1D1D; border: 1px solid #B91C1C; color: #FECACA; font-weight: 700; font-size: 13px; padding: 6px 12px; border-radius: 6px; letter-spacing: .04em; }
h1 { font-size: 20px; margin: 16px 0 8px; }
h2 { font-size: 15px; margin: 20px 0 8px; color: #CBD5E1; }
.lead { font-size: 13px; color: #94A3B8; line-height: 1.7; }
.saved { background: #064E3B; border: 1px solid #059669; color: #A7F3D0; padding: 8px 12px; border-radius: 6px; margin: 12px 0; font-size: 13px; }
.empty { background: #1E293B; border: 1px solid #334155; color: #CBD5E1; padding: 12px; border-radius: 6px; font-size: 13px; }
.hyp { border: 1px solid #334155; border-left: 3px solid #38BDF8; border-radius: 6px; padding: 12px 14px; margin: 12px 0; }
.hyp .top { font-size: 13px; margin-bottom: 6px; }
.hyp .hid { color: #F87171; font-weight: 700; margin-right: 6px; }
.hyp .cat { font-size: 10px; font-weight: 700; border: 1px solid #64748B; border-radius: 3px; padding: 0 5px; margin-left: 6px; color: #94A3B8; }
.hyp .stmt { font-size: 14px; line-height: 1.6; margin: 4px 0 8px; }
.hyp .meta { font-size: 11px; color: #94A3B8; margin-bottom: 8px; }
.hyp .auto { font-size: 12px; color: #7DD3FC; margin-bottom: 6px; }
.hyp .auto b { color: #BAE6FD; }
.decisions { display: flex; gap: 14px; flex-wrap: wrap; margin-bottom: 8px; }
.decisions label { font-size: 13px; display: inline-flex; align-items: center; gap: 4px; padding: 4px 8px; border: 1px solid #475569; border-radius: 4px; cursor: pointer; min-height: 40px; }
.decisions label.d-support { color: #86EFAC; }
.decisions label.d-reject { color: #FCA5A5; }
.decisions label.d-hold { color: #FDE68A; }
.note-in { width: 100%; padding: 8px 10px; background: #1E293B; border: 1px solid #475569; border-radius: 4px; color: #fff; font-size: 13px; }
.note-label { font-size: 11px; color: #94A3B8; margin-bottom: 3px; }
.diff { font-size: 11px; color: #FBBF24; margin-top: 4px; }
.actions { margin: 20px 0; }
button { background: #B91C1C; color: #fff; border: none; padding: 12px 28px; border-radius: 6px; font-size: 15px; font-weight: 700; cursor: pointer; min-height: 44px; }
button:hover { background: #DC2626; }
.history ul { list-style: none; padding: 0; font-size: 12px; color: #94A3B8; }
.history li { padding: 4px 0; border-bottom: 1px solid #1E293B; }
.nav-links { margin: 12px 0; font-size: 13px; }
.nav-links a { color: #7DD3FC; margin-right: 16px; }
"#
}

/// 現在のレビュー状態をプリフィルした仮説更新画面 HTML。
///
/// `analysis` の全仮説を出す。`current` は既存の保存 (無ければ auto の初期値) を反映。
/// `has_hearing` が false のときは、ヒアリング未入力の案内を出す。
#[allow(clippy::too_many_arguments)]
pub fn hypothesis_review_html(
    session_id: &str,
    region: &str,
    analysis: &ConsultAnalysis,
    current: &[HypothesisReview],
    has_hearing: bool,
    saved: bool,
    saved_revision: Option<i64>,
    history: &[(i64, String)],
) -> String {
    let sid = escape_html(session_id);
    let mut html = String::with_capacity(48 * 1024);
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"default\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<meta name=\"robots\" content=\"noindex,nofollow\">\n");
    html.push_str("<title>仮説の確認・更新（社内用）</title>\n<style>\n");
    html.push_str(review_css());
    html.push_str("</style>\n</head>\n<body>\n");
    html.push_str("<div class=\"wrap\">\n");
    html.push_str(r#"<div class="band">🔒 社内用 — 顧客配布不可 / INTERNAL USE ONLY</div>"#);
    html.push_str("<h1>仮説の確認・更新</h1>\n");
    html.push_str(&format!(
        "<p class=\"lead\">面談前に整理した仮説を、ヒアリング回答をもとに支持・否定・保留へ更新します。対象: <strong>{}</strong>。自動提案は初期値です。異なる場合は選び直し、理由を残してください。保存すると追記され、最新が現在値になります。</p>\n",
        if region.trim().is_empty() { "（未特定）".to_string() } else { escape_html(region) }
    ));

    html.push_str(&format!(
        "<div class=\"nav-links\"><a href=\"/consult/hearing?session_id={sid}\">✍ ヒアリング入力</a><a href=\"/consult/action_memo?session_id={sid}\">📝 アクションメモ</a></div>\n"
    ));

    if !has_hearing {
        html.push_str(&format!(
            "<div class=\"empty\">ヒアリング回答がまだありません。自動提案はすべて「保留（データ確認待ち）」を初期値としています。<br>より精度の高い提案には <a href=\"/consult/hearing?session_id={sid}\" style=\"color:#7DD3FC\">ヒアリング入力</a> を先に行ってください。</div>\n"
        ));
    }

    if saved {
        let rev = saved_revision
            .map(|r| format!("（版 {r}）"))
            .unwrap_or_default();
        html.push_str(&format!(
            "<div class=\"saved\">保存しました{rev}。以下は最新の更新内容です。</div>\n"
        ));
    }

    if analysis.hypotheses.is_empty() {
        html.push_str("<div class=\"empty\">現在、更新対象の仮説がありません。市場データが十分に取得できると仮説が生成されます。</div>\n");
        html.push_str("</div>\n</body>\n</html>\n");
        return html;
    }

    // 仮説ID → 現在のレビュー
    let current_map: BTreeMap<&str, &HypothesisReview> = current
        .iter()
        .map(|r| (r.hypothesis_id.as_str(), r))
        .collect();

    html.push_str(&format!(
        "<form method=\"post\" action=\"/consult/hypothesis_review?session_id={sid}\">\n"
    ));

    // TOP5 を先頭表示するため、TOP群 → 残りの順に並べる
    let top_ids: Vec<&str> = analysis
        .top_hypotheses
        .iter()
        .map(|h| h.hypothesis_id.as_str())
        .collect();
    let mut ordered: Vec<&Hypothesis> = Vec::new();
    for h in &analysis.top_hypotheses {
        ordered.push(h);
    }
    for h in &analysis.hypotheses {
        if !top_ids.contains(&h.hypothesis_id.as_str()) {
            ordered.push(h);
        }
    }

    let mut shown_top_header = false;
    let mut shown_rest_header = false;
    for h in ordered {
        let is_top = top_ids.contains(&h.hypothesis_id.as_str());
        if is_top && !shown_top_header {
            html.push_str("<h2>優先仮説 TOP5</h2>\n");
            shown_top_header = true;
        }
        if !is_top && !shown_rest_header {
            html.push_str("<h2>その他の仮説</h2>\n");
            shown_rest_header = true;
        }
        let rv = current_map.get(h.hypothesis_id.as_str());
        let auto = rv
            .map(|r| r.auto_suggestion)
            .unwrap_or_else(|| Decision::Hold);
        let decision = rv.map(|r| r.decision).unwrap_or(auto);
        let note = rv.map(|r| r.note.as_str()).unwrap_or("");
        html.push_str(&render_hyp(h, auto, decision, note));
    }

    html.push_str(
        r#"<div class="actions"><button type="submit">更新を保存する</button></div>
</form>
"#,
    );

    if !history.is_empty() {
        html.push_str("<div class=\"history\">\n<h2>更新履歴</h2>\n<ul>\n");
        for (rev, at) in history {
            html.push_str(&format!("<li>版 {} — {}</li>\n", rev, escape_html(at)));
        }
        html.push_str("</ul>\n</div>\n");
    }

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

fn render_hyp(h: &Hypothesis, auto: Decision, decision: Decision, note: &str) -> String {
    let hid = escape_html(&h.hypothesis_id);
    let mut s = String::new();
    s.push_str("<div class=\"hyp\">\n");
    s.push_str(&format!(
        "<div class=\"top\"><span class=\"hid\">{hid}</span><span class=\"cat\">{cat}</span></div>\n",
        cat = escape_html(h.category.label_ja())
    ));
    s.push_str(&format!(
        "<div class=\"stmt\">{}</div>\n",
        escape_html(&h.statement)
    ));
    s.push_str(&format!(
        "<div class=\"meta\">信頼度: {conf} / 優先度: {prio} / 根拠: {refs}</div>\n",
        conf = h.confidence.label_ja(),
        prio = h.priority.label_ja(),
        refs = escape_html(&h.supporting_evidence_ids.join(", "))
    ));
    s.push_str(&format!(
        "<div class=\"auto\">自動判定の提案: <b>{}</b></div>\n",
        auto.label_ja()
    ));
    // hidden で自動提案を保持 (差分追跡)
    s.push_str(&format!(
        "<input type=\"hidden\" name=\"auto__{hid}\" value=\"{av}\">\n",
        av = auto.as_form_value()
    ));

    // 支持/否定/保留 のラジオ
    s.push_str("<div class=\"decisions\">\n");
    for (d, cls) in [
        (Decision::Support, "d-support"),
        (Decision::Reject, "d-reject"),
        (Decision::Hold, "d-hold"),
    ] {
        let checked = if d == decision { " checked" } else { "" };
        s.push_str(&format!(
            "<label class=\"{cls}\"><input type=\"radio\" name=\"decision__{hid}\" value=\"{dv}\"{checked}> {label}</label>\n",
            dv = d.as_form_value(),
            label = d.label_ja()
        ));
    }
    s.push_str("</div>\n");

    // 提案と確定が異なる場合の注記
    if decision != auto {
        s.push_str(&format!(
            "<div class=\"diff\">※ 自動提案（{}）から変更されています</div>\n",
            auto.label_ja()
        ));
    }

    // 理由メモ
    s.push_str("<div class=\"note-label\">理由メモ（任意）</div>\n");
    s.push_str(&format!(
        "<input type=\"text\" class=\"note-in\" name=\"note__{hid}\" value=\"{v}\" placeholder=\"確定した理由・確認した内容\">\n",
        v = escape_html(note)
    ));
    s.push_str("</div>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::consult::evidence_pack::analyze;
    use crate::handlers::consult::evidence_pack::tests::rich_input;
    use crate::handlers::consult::hypotheses::{Confidence, Priority};

    fn temp_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db = LocalDb::new(tmp.path().to_str().unwrap()).unwrap();
        (tmp, db)
    }

    fn av(value: &str) -> AnswerValue {
        AnswerValue {
            value: value.to_string(),
            unknown: false,
            no_data: false,
        }
    }
    fn av_unknown() -> AnswerValue {
        AnswerValue {
            value: String::new(),
            unknown: true,
            no_data: false,
        }
    }
    fn answers(pairs: &[(&str, AnswerValue)]) -> BTreeMap<String, AnswerValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn hyp(id: &str, category: HypothesisCategory) -> Hypothesis {
        Hypothesis {
            hypothesis_id: id.to_string(),
            category,
            statement: "テストの可能性がある".to_string(),
            supporting_evidence_ids: vec!["E-001".to_string()],
            counter_evidence_ids: vec![],
            missing_information: vec![],
            confidence: Confidence::Medium,
            priority: Priority::High,
            status: "unverified".to_string(),
        }
    }

    // ---- 自動判定ルール ----

    #[test]
    fn auto_post_application_slow_contact_supports() {
        let h = hyp("H-001", HypothesisCategory::PostApplication);
        let a = answers(&[("q11_first_contact_time", av("2日以降"))]);
        assert_eq!(auto_suggest(&h, &a), Decision::Support);
    }

    #[test]
    fn auto_post_application_same_day_rejects_when_apps_present() {
        let h = hyp("H-001", HypothesisCategory::PostApplication);
        let a = answers(&[
            ("q11_first_contact_time", av("当日中")),
            ("q04_applications_monthly", av("3件以上")),
        ]);
        assert_eq!(auto_suggest(&h, &a), Decision::Reject);
    }

    #[test]
    fn auto_post_application_same_day_holds_when_few_apps() {
        let h = hyp("H-001", HypothesisCategory::PostApplication);
        let a = answers(&[
            ("q11_first_contact_time", av("当日中")),
            ("q04_applications_monthly", av("0〜2件")),
        ]);
        assert_eq!(auto_suggest(&h, &a), Decision::Hold);
    }

    #[test]
    fn auto_sourcing_few_apps_unknown_impressions_holds() {
        // §12.6: 応募0〜2件 + 表示数不明 → 保留 (データ確認待ち)
        let h = hyp("H-001", HypothesisCategory::Sourcing);
        let a = answers(&[
            ("q04_applications_monthly", av("0〜2件")),
            ("d04a_impressions_clicks", av_unknown()),
        ]);
        assert_eq!(auto_suggest(&h, &a), Decision::Hold);
    }

    #[test]
    fn auto_selection_offers_no_acceptance_supports() {
        let h = hyp("H-001", HypothesisCategory::Selection);
        let a = answers(&[
            ("q07_interviews_done", av("3")),
            ("q08_offers", av("2")),
            ("q09_acceptances", av("0")),
        ]);
        assert_eq!(auto_suggest(&h, &a), Decision::Support);
    }

    #[test]
    fn auto_retention_vacancy_reason_supports() {
        let h = hyp("H-001", HypothesisCategory::Retention);
        let a = answers(&[("q03_reason", av("欠員補充"))]);
        assert_eq!(auto_suggest(&h, &a), Decision::Support);
    }

    #[test]
    fn auto_empty_answers_default_hold() {
        // ヒアリングなし → すべて保留
        let a: BTreeMap<String, AnswerValue> = BTreeMap::new();
        for cat in [
            HypothesisCategory::MarketStructure,
            HypothesisCategory::Conditions,
            HypothesisCategory::Appeal,
            HypothesisCategory::Sourcing,
            HypothesisCategory::PostApplication,
            HypothesisCategory::Selection,
            HypothesisCategory::Retention,
            HypothesisCategory::GoalDesign,
        ] {
            let h = hyp("H-001", cat);
            assert_eq!(auto_suggest(&h, &a), Decision::Hold, "{:?}", cat);
        }
    }

    // ---- 保存 / 取得 / 履歴 ----

    #[test]
    fn insert_and_latest_and_history() {
        let (_t, db) = temp_db();
        assert!(latest_reviews(&db, "S1").is_none());
        let r1 = insert_reviews(&db, "S1", "[]", "2026-07-11T10:00:00+09:00").unwrap();
        let r2 = insert_reviews(
            &db,
            "S1",
            "[{\"hypothesis_id\":\"H-001\",\"auto_suggestion\":\"hold\",\"decision\":\"support\",\"note\":\"x\"}]",
            "2026-07-11T11:00:00+09:00",
        )
        .unwrap();
        assert_eq!(r1, 1);
        assert_eq!(r2, 2);
        let latest = latest_reviews(&db, "S1").unwrap();
        assert_eq!(latest.revision, 2);
        assert!(latest.reviews_json.contains("support"));
        // 追記オンリー: 2行残る
        let count: i64 = db
            .query_scalar(
                "SELECT COUNT(*) FROM consult_hypothesis_reviews WHERE session_id = 'S1'",
                &[],
            )
            .unwrap();
        assert_eq!(count, 2);
        let hist = revision_history(&db, "S1");
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].0, 2);
    }

    #[test]
    fn reviews_json_round_trip() {
        let reviews = vec![HypothesisReview {
            hypothesis_id: "H-001".to_string(),
            auto_suggestion: Decision::Hold,
            decision: Decision::Reject,
            note: "確認済み".to_string(),
        }];
        let json = serde_json::to_string(&reviews).unwrap();
        let back = reviews_from_json(&json);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].decision, Decision::Reject);
        assert_eq!(back[0].auto_suggestion, Decision::Hold);
    }

    #[test]
    fn reviews_from_form_captures_manual_override_and_diff() {
        let analysis = analyze(&rich_input());
        let hid = analysis.hypotheses[0].hypothesis_id.clone();
        let a: BTreeMap<String, AnswerValue> = BTreeMap::new();
        let mut form: BTreeMap<String, String> = BTreeMap::new();
        form.insert(format!("auto__{hid}"), "hold".to_string());
        form.insert(format!("decision__{hid}"), "support".to_string());
        form.insert(format!("note__{hid}"), "面談で確認".to_string());
        let reviews = reviews_from_form(&analysis, &a, &form);
        let r = reviews.iter().find(|r| r.hypothesis_id == hid).unwrap();
        assert_eq!(r.auto_suggestion, Decision::Hold);
        assert_eq!(r.decision, Decision::Support, "手動確定が反映される");
        assert_eq!(r.note, "面談で確認");
        // 全仮説分が返る
        assert_eq!(reviews.len(), analysis.hypotheses.len());
    }

    // ---- 画面 HTML ----

    #[test]
    fn review_html_lists_hypotheses_and_marks_override() {
        let analysis = analyze(&rich_input());
        let a = answers(&[("q11_first_contact_time", av("2日以降"))]);
        let current = auto_suggest_all(&analysis, &a);
        // 1件を手動で否定に変える
        let mut current = current;
        let hid = current[0].hypothesis_id.clone();
        current[0].decision = Decision::Reject;
        let html = hypothesis_review_html(
            "SID1",
            "群馬県 高崎市",
            &analysis,
            &current,
            true,
            false,
            None,
            &[],
        );
        assert!(html.contains("仮説の確認・更新"));
        assert!(html.contains("社内用"));
        assert!(html.contains(&hid));
        assert!(html.contains("自動判定の提案"));
        // 変更マーク
        assert!(html.contains("変更されています"));
        // 3択が出る
        assert!(html.contains("支持"));
        assert!(html.contains("否定"));
        assert!(html.contains("保留"));
    }

    #[test]
    fn review_html_shows_no_hearing_notice() {
        let analysis = analyze(&rich_input());
        let a: BTreeMap<String, AnswerValue> = BTreeMap::new();
        let current = auto_suggest_all(&analysis, &a);
        let html = hypothesis_review_html(
            "SID1",
            "群馬県",
            &analysis,
            &current,
            false,
            false,
            None,
            &[],
        );
        assert!(html.contains("ヒアリング回答がまだありません"));
    }
}
