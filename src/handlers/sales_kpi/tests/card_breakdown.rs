//! 営業KPI「今月の成績」カードの内訳（2026-10-02 追加・段階A: 実装前に落ちるテスト）
//!
//! 設計書: `claudedocs/SALES_KPI_CARD_BREAKDOWN_DESIGN_2026-10-02.md`
//!
//! カード（① 取ったアポ / ③ 商談の予定 / ④ 日が過ぎた分 / ② やった商談 /
//! ⑥ 商談化率 / ⑤ アンケート回収率 / ⑨ 持っているCヨミ）を押すと、
//! チーム → 担当者 → 取引一覧 の内訳が開く。内訳の行はサーバが
//! `card_deals.{pool,apo,cyomi}` で返す（既存キーは変えず追加のみ）。
//!
//! 🔴 守ること: **カードの数字と内訳の合計が必ず一致する。**
//!    カードの件数（`by_person`）と内訳の行（`card_deals`）は、どちらも担当者ごとに
//!    持っている。画面の絞り込み（チーム・担当者・チェックで外した人）は
//!    **担当者の集合で切るだけ**なので、「担当者ごとに 件数 == 行数」が成り立てば、
//!    どの絞り込みでも一致する。ここではそれを
//!      1. 担当者ごと（全キー）
//!      2. チームの全組み合わせ × チーム選択、担当者選択（画面と同じ規則を Rust で再現）
//!    の両方で確かめる。
//!
//! 実装前は `card_deals` が無いので、合成入力の「現行の数え方」テスト以外は
//! すべて落ちる（コンパイルは通る。JSON のキーで見ているため）。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use chrono::NaiveDate;
use serde_json::Value;

use super::{fixture_sheets, payload, payload_of, sheet_from_tsv};
use crate::handlers::sales_kpi::Sheets;

// ---------------------------------------------------------------- カードの定義（期待値側）

/// 仕分けのラベル（`Kind::label()`）。④ 日が過ぎた分 = これから以外。
const PAST_KINDS: &[&str] = &["実施", "未実施", "未処理", "要判定"];
const ALL_KINDS: &[&str] = &["実施", "未実施", "未処理", "これから", "要判定"];

/// `by_person` のキー 1 つと、それを作る「行の述語」。
/// 件数側のキー `key` と BPO 側のキー `bpo_key` を、同じ行の集合から数える。
pub(super) struct CardPred {
    pub(super) key: String,
    pub(super) bpo_key: String,
    /// `card_deals` のどの配列から取るか
    pub(super) src: &'static str,
    pub(super) pred: Box<dyn Fn(&Value) -> bool>,
}

pub(super) fn kind_of(r: &Value) -> &str {
    r["kind"].as_str().unwrap_or("")
}

/// 件数のキーすべて。設計書 §3 の表と同じ。
pub(super) fn card_preds() -> Vec<CardPred> {
    let mut v: Vec<CardPred> = vec![
        CardPred {
            key: "apo".into(),
            bpo_key: "bpo_apo".into(),
            src: "apo",
            pred: Box::new(|_| true),
        },
        CardPred {
            key: "pool".into(),
            bpo_key: "bpo_pool".into(),
            src: "pool",
            pred: Box::new(|_| true),
        },
        CardPred {
            key: "cyomi".into(),
            bpo_key: "bpo_cyomi".into(),
            src: "cyomi",
            pred: Box::new(|_| true),
        },
        CardPred {
            key: "anq_den".into(),
            bpo_key: "bpo_anq_den".into(),
            src: "pool",
            pred: Box::new(|r| kind_of(r) != "これから"),
        },
        CardPred {
            key: "anq_num".into(),
            bpo_key: "bpo_anq_num".into(),
            src: "pool",
            pred: Box::new(|r| kind_of(r) != "これから" && r["anq"].as_bool() == Some(true)),
        },
    ];
    for k in ALL_KINDS {
        let kk = (*k).to_string();
        v.push(CardPred {
            key: kk.clone(),
            bpo_key: format!("bpo_{k}"),
            src: "pool",
            pred: Box::new(move |r| kind_of(r) == kk),
        });
    }
    v
}

/// `card_deals.<src>`。無ければ「未実装」と分かる文で落とす。
pub(super) fn card_rows<'a>(body: &'a Value, src: &str) -> &'a Vec<Value> {
    body["card_deals"][src].as_array().unwrap_or_else(|| {
        panic!("payload に card_deals.{src} がありません（カード内訳の行が未実装）")
    })
}

pub(super) fn by_person_get(body: &Value, owner: &str, key: &str) -> i64 {
    body["by_person"][owner][key].as_i64().unwrap_or(0)
}

// ---------------------------------------------------------------- 画面の絞り込み（JS の写し）

/// `templates/tabs/sales_kpi.html` の `sumIf` / `sumScope` と同じ規則。
/// 担当者 id → チームは `D.people`（= payload の `people`）から引く（`TEAM_OF`）。
///   - `hidden` に入っている人は除く（チェックで外した人）
///   - 担当者を選んでいればその人だけ、そうでなければ チーム = 選んだチーム（すべてなら全員）
pub(super) struct Scope<'a> {
    pub(super) team: Option<&'a str>,
    pub(super) person: Option<&'a str>,
    pub(super) hidden: &'a HashSet<String>,
}

impl Scope<'_> {
    pub(super) fn has(&self, owner: &str, team_of: &HashMap<String, String>) -> bool {
        if self.hidden.contains(owner) {
            return false;
        }
        match (self.person, self.team) {
            (Some(p), _) => owner == p,
            (None, None) => true,
            (None, Some(t)) => team_of.get(owner).map(String::as_str) == Some(t),
        }
    }
}

pub(super) fn team_of(body: &Value) -> HashMap<String, String> {
    body["people"]
        .as_array()
        .expect("people")
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap_or("").to_string(),
                p["team"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// カードの値を、件数側（`by_person` を絞り込みで足したもの）と内訳側（行を同じ絞り込みで
/// 数えたもの）の両方で作り、全部そろっているか比べる。ずれたキーを返す。
fn mismatches(body: &Value, scope: &Scope, label: &str) -> Vec<String> {
    let tof = team_of(body);
    let mut out = Vec::new();
    let by_person = body["by_person"].as_object().expect("by_person");
    for c in card_preds() {
        let card: i64 = by_person
            .iter()
            .filter(|(o, _)| scope.has(o, &tof))
            .map(|(_, v)| v[&c.key].as_i64().unwrap_or(0))
            .sum();
        let card_bpo: i64 = by_person
            .iter()
            .filter(|(o, _)| scope.has(o, &tof))
            .map(|(_, v)| v[&c.bpo_key].as_i64().unwrap_or(0))
            .sum();
        let rows: Vec<&Value> = card_rows(body, c.src)
            .iter()
            .filter(|r| scope.has(r["owner"].as_str().unwrap_or(""), &tof))
            .filter(|r| (c.pred)(r))
            .collect();
        let n = rows.len() as i64;
        let n_bpo = rows
            .iter()
            .filter(|r| r["bpo"].as_bool() == Some(true))
            .count() as i64;
        if card != n {
            out.push(format!("{label}: {} カード {card} ≠ 内訳 {n}", c.key));
        }
        if card_bpo != n_bpo {
            out.push(format!(
                "{label}: {} 内BPO カード {card_bpo} ≠ 内訳の bpo 行 {n_bpo}",
                c.key
            ));
        }
    }
    // ④（画面は 4 区分を足して作る）と ⑤ の分母（サーバの anq_den）が同じ行の集合であること
    let den_card: i64 = PAST_KINDS
        .iter()
        .map(|k| {
            by_person
                .iter()
                .filter(|(o, _)| scope.has(o, &tof))
                .map(|(_, v)| v[*k].as_i64().unwrap_or(0))
                .sum::<i64>()
        })
        .sum();
    let pool: Vec<&Value> = card_rows(body, "pool")
        .iter()
        .filter(|r| scope.has(r["owner"].as_str().unwrap_or(""), &tof))
        .collect();
    let den_rows = pool
        .iter()
        .filter(|r| PAST_KINDS.contains(&kind_of(r)))
        .count() as i64;
    // ⑥ の分母の内訳: 実施・未実施・未処理・要判定 の行数の和
    let six: i64 = PAST_KINDS
        .iter()
        .map(|k| pool.iter().filter(|r| kind_of(r) == *k).count() as i64)
        .sum();
    // ⑤ の分母の内訳: 回収済み・未回収 の行数の和
    let five_yes = pool
        .iter()
        .filter(|r| kind_of(r) != "これから" && r["anq"].as_bool() == Some(true))
        .count() as i64;
    let five_no = pool
        .iter()
        .filter(|r| kind_of(r) != "これから" && r["anq"].as_bool() == Some(false))
        .count() as i64;
    if den_card != den_rows || six != den_card || five_yes + five_no != den_card {
        out.push(format!(
            "{label}: ④ {den_card} / ④の行 {den_rows} / ⑥分母の内訳の和 {six} / ⑤ 回収済み {five_yes} + 未回収 {five_no}"
        ));
    }
    out
}

// ---------------------------------------------------------------- 実データ（fixture）

/// 担当者ごと・全キーで 件数 == 行数。どの絞り込みでも一致する根拠。
#[test]
fn カードの件数と内訳の行数が担当者ごとに一致する() {
    let body = payload();
    let mut owners: BTreeSet<String> = body["by_person"]
        .as_object()
        .expect("by_person")
        .keys()
        .cloned()
        .collect();
    for src in ["pool", "apo", "cyomi"] {
        for r in card_rows(&body, src) {
            owners.insert(r["owner"].as_str().unwrap_or("").to_string());
        }
    }
    let mut bad = Vec::new();
    for o in &owners {
        for c in card_preds() {
            let rows: Vec<&Value> = card_rows(&body, c.src)
                .iter()
                .filter(|r| r["owner"].as_str() == Some(o.as_str()))
                .filter(|r| (c.pred)(r))
                .collect();
            let n_bpo = rows
                .iter()
                .filter(|r| r["bpo"].as_bool() == Some(true))
                .count() as i64;
            if by_person_get(&body, o, &c.key) != rows.len() as i64 {
                bad.push(format!(
                    "owner {o} {}: カード {} ≠ 行 {}",
                    c.key,
                    by_person_get(&body, o, &c.key),
                    rows.len()
                ));
            }
            if by_person_get(&body, o, &c.bpo_key) != n_bpo {
                bad.push(format!(
                    "owner {o} {}: カード {} ≠ bpo 行 {n_bpo}",
                    c.bpo_key,
                    by_person_get(&body, o, &c.bpo_key)
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{} 件ずれ:\n{}", bad.len(), bad.join("\n"));
    // 空振りしていないこと（fixture の当月 537 件・アポ 245 件が行として出ている）
    assert_eq!(card_rows(&body, "pool").len(), 537);
    assert_eq!(
        card_rows(&body, "apo").len() as i64,
        super::team_sum(&body, "apo")
    );
}

/// 画面の絞り込みを全部試す: チームの全組み合わせを「チェックで外す」× チームの選択、
/// と 担当者 1 人ずつの選択。どれでもカードと内訳がそろう。
#[test]
fn チームの全組み合わせと担当者選択でカードと内訳が一致する() {
    let body = payload();
    assert_all_scopes(&body);
}

pub(super) fn assert_all_scopes(body: &Value) {
    let teams: Vec<String> = body["teams"]
        .as_array()
        .expect("teams")
        .iter()
        .map(|t| t.as_str().unwrap_or("").to_string())
        .collect();
    assert!(!teams.is_empty(), "teams が空でこの検査が効かない");
    assert!(teams.len() <= 12, "チームが多すぎて全組み合わせが重い");
    let tof = team_of(body);
    let mut bad = Vec::new();
    let mut tried = 0usize;
    for mask in 0u32..(1u32 << teams.len()) {
        let hidden_teams: Vec<&str> = teams
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, t)| t.as_str())
            .collect();
        let hidden: HashSet<String> = tof
            .iter()
            .filter(|(_, t)| hidden_teams.contains(&t.as_str()))
            .map(|(o, _)| o.clone())
            .collect();
        let selections: Vec<Option<&str>> = std::iter::once(None)
            .chain(teams.iter().map(|t| Some(t.as_str())))
            .collect();
        for sel in selections {
            let scope = Scope {
                team: sel,
                person: None,
                hidden: &hidden,
            };
            bad.extend(mismatches(
                body,
                &scope,
                &format!("外す={hidden_teams:?} 選択={sel:?}"),
            ));
            tried += 1;
        }
    }
    let none = HashSet::new();
    for owner in tof.keys() {
        let scope = Scope {
            team: None,
            person: Some(owner),
            hidden: &none,
        };
        bad.extend(mismatches(body, &scope, &format!("担当者={owner}")));
        tried += 1;
    }
    assert!(tried > teams.len(), "組み合わせを試していない");
    assert!(
        bad.is_empty(),
        "{} 件ずれ（先頭 20 件）:\n{}",
        bad.len(),
        bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// 内訳の行の担当者は、必ず `people` に居て、行の team が `people` の team と同じ。
/// 画面はチームの絞り込みを `TEAM_OF[owner]` で掛けるので、ここがずれると
/// カード（by_person）と内訳（行）で別のチームに入る。
#[test]
fn 内訳の行の担当者とチームは絞り込みの名簿と同じ() {
    let body = payload();
    let tof = team_of(&body);
    for src in ["pool", "apo", "cyomi"] {
        for r in card_rows(&body, src) {
            let o = r["owner"].as_str().unwrap_or("");
            let t = tof
                .get(o)
                .unwrap_or_else(|| panic!("{src} の行の担当者 {o} が people に居ない"));
            assert_eq!(r["team"].as_str(), Some(t.as_str()), "{src} {o}");
        }
    }
}

/// ① の内BPO は「当月に取った BPO アポ」だけ（`is_bpo(month_lo, month_hi)`）。
/// 他のカードの BPO（前月＋当月の窓 `bpo_of`）と同じ判定で行の bpo を作ると 1 件多くなる。
/// fixture の dealId 22722720314 は BPOアポ取得日 2026-08-27（前月）: ①では BPO ではない。
#[test]
fn 取ったアポの内bpoは当月の取得日だけ() {
    let body = payload();
    let row = card_rows(&body, "apo")
        .iter()
        .find(|r| r["id"].as_str() == Some("22722720314"))
        .expect("fixture のアポに 22722720314 が無い");
    assert_eq!(
        row["bpo"].as_bool(),
        Some(false),
        "前月取得の BPO アポを ① の内BPO に入れている"
    );
    let n = card_rows(&body, "apo")
        .iter()
        .filter(|r| r["bpo"].as_bool() == Some(true))
        .count() as i64;
    assert_eq!(n, super::team_sum(&body, "bpo_apo"));
    assert_eq!(n, 51, "fixture の ① 内BPO（当月の窓）は 51 件");
}

/// ⑨ の内BPO は前月＋当月の窓（`bpo_of`）。fixture では 14 件、当月だけなら 1 件。
#[test]
fn cヨミの内bpoは前月と当月の窓() {
    let body = payload();
    let n = card_rows(&body, "cyomi")
        .iter()
        .filter(|r| r["bpo"].as_bool() == Some(true))
        .count() as i64;
    assert_eq!(n, super::team_sum(&body, "bpo_cyomi"));
    assert_eq!(n, 14);
}

/// 集計除外（`KPI営業_集計除外` → メンバーの `集計対象=対象外`）の人の取引は内訳にも出ない。
/// fixture の 34084170002 は除外者の取引で、商談と Cヨミの両方のシートにある。
#[test]
fn 集計除外の取引は内訳にも出ない() {
    let sheets = fixture_sheets();
    let out = super::excluded_owners(&sheets);
    assert!(!out.is_empty());
    let body = payload();
    for src in ["pool", "apo", "cyomi"] {
        for r in card_rows(&body, src) {
            let o = r["owner"].as_str().unwrap_or("");
            assert!(!out.contains(o), "{src} に集計除外の {o} の取引が出ている");
            assert_ne!(r["id"].as_str(), Some("34084170002"));
        }
    }
}

/// ④ 日が過ぎた分は「日付が今日より前」ではなく「これから以外」。
/// 予定日は先でも、もう実施・未実施が決まった取引は ④ に入る（fixture では 49 件）。
/// 日付で切った内訳を作ると ④ と合わない。
#[test]
fn 日が過ぎた分には予定日が先でも結果が出た取引が入る() {
    let body = payload();
    let cutoff = "2026-09-04";
    let future_but_past_kind = card_rows(&body, "pool")
        .iter()
        .filter(|r| PAST_KINDS.contains(&kind_of(r)))
        .filter(|r| r["date"].as_str().unwrap_or("") >= cutoff)
        .count();
    assert_eq!(future_but_past_kind, 49);
}

/// 「HubSpotを開く」は object ID から作る（headless-crm-design §6）。
/// 形は コンサルKPI と同じ `https://app.hubspot.com/contacts/<portal>/record/0-3/<dealId>/`、
/// portal は `HUBSPOT_PORTAL_ID`（未設定・空白なら 23708633）。
///
/// 🔴 既存の ⑦⑤⑨・今週/来週の一覧も `itemEl()` が `r.url` を読むが、`DealRow` に url が
///    無く href="undefined" になっている（初版 a8a8596 から）。同じ構造体なので一緒に見る。
#[test]
fn 内訳と既存の一覧の行はhubspotの取引ページを開ける() {
    let portal = std::env::var("HUBSPOT_PORTAL_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "23708633".into());
    let body = payload();
    let mut lists: Vec<(&str, &Vec<Value>)> = ["pool", "apo", "cyomi"]
        .iter()
        .map(|s| (*s, card_rows(&body, s)))
        .collect();
    for k in [
        "stale",
        "week_deals",
        "next_week_deals",
        "cyomi_stale",
        "anq_missing",
    ] {
        lists.push((k, body[k].as_array().expect(k)));
    }
    for (name, rows) in lists {
        assert!(!rows.is_empty(), "{name} が空で検査が効かない");
        for r in rows {
            let id = r["id"].as_str().unwrap_or("");
            assert_eq!(
                r["url"].as_str(),
                Some(
                    format!("https://app.hubspot.com/contacts/{portal}/record/0-3/{id}/").as_str()
                ),
                "{name} の {id} の url"
            );
        }
    }
}

// ---------------------------------------------------------------- 合成入力（ずれを誘う形）

const DEAL_HEAD: &str = "dealId\townerId\tpipeline\tdealstage\t商談予定日時\t時間\tBPOアポ取得日\t事前アンケート\tアポ日確定を出た日\tBPOアポ日確定を出た日\tCヨミに入った日\t取引名";

/// (dealId, ownerId, stage, 予定日時, BPOアポ取得日, アンケート, アポ日確定を出た日)
type D<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

fn deal_sheet(rows: &[D]) -> Arc<crate::handlers::call_quality::sheets::SheetData> {
    let mut s = String::from(DEAL_HEAD);
    for (id, owner, stage, sched, bpo, anq, exited) in rows {
        s.push('\n');
        s.push_str(&format!(
            "{id}\t{owner}\t21724969\t{stage}\t{sched}\t\t{bpo}\t{anq}\t{exited}\t\t\t取引{id}"
        ));
    }
    Arc::new(sheet_from_tsv(&s))
}

/// today = 2026-09-04（金）。当月 9/1〜9/30、前月 8/1〜、cutoff は 9/04 00:00。
///
/// | id | 担当 | ねらい |
/// |----|------|--------|
/// | D001 | 1001 | 8/31 23:30。月またぎ（当月に入らない）|
/// | D002 | 1001 | 9/01 00:00 アポ日確定のまま → 未処理 |
/// | D003 | 1001 | 9/04 00:00 ちょうど（cutoff 当日）アポ日確定 → これから |
/// | D004 | 1002 | 実施ステージ・予定 9/20（先）・アンケートあり → 実施、④と⑤分子に入る。⑨にも同じ id |
/// | D005 | 1002 | キャンセル・予定 9/25（先）→ 未実施、④に入る |
/// | D006 | 2001 | 予定 9/02 を過ぎてから戻し → 実施 |
/// | D007 | 2001 | 予定 9/03・出た日なし → 要判定。BPO 8/15（前月）→ 内BPO |
/// | D008 | 9001 | 集計除外の人 |
/// | D009 | 7777 | 名簿にいない人・BPO 9/01 → 未処理・内BPO・チーム未設定 |
/// | D010 | 3001 | BPO 名簿（チーム空）・BPO 日付 2025-12-01（古い）→ 内BPO ではない |
/// | D011 | 1001 | 10/01 00:00。翌月（当月に入らない）|
/// | D012 | （空）| 担当なし → 実施 |
pub(super) fn synthetic_sheets() -> Sheets {
    const APO: &str = "52035886";
    const APO_BPO: &str = "1095457875";
    const DONE: &str = "52035887";
    const CANCEL: &str = "1422048803";
    const C: &str = "52035889";
    const OTHER: &str = "99999999";
    let shodan = deal_sheet(&[
        ("D001", "1001", APO, "2026-08-31 23:30", "", "", ""),
        ("D002", "1001", APO, "2026-09-01 00:00", "", "", ""),
        ("D003", "1001", APO, "2026-09-04 00:00", "", "", ""),
        ("D004", "1002", DONE, "2026-09-20 10:00", "", "済", ""),
        ("D005", "1002", CANCEL, "2026-09-25 10:00", "", "", ""),
        (
            "D006",
            "2001",
            OTHER,
            "2026-09-02 10:00",
            "",
            "",
            "2026-09-03 10:00",
        ),
        (
            "D007",
            "2001",
            OTHER,
            "2026-09-03 10:00",
            "2026-08-15 00:00",
            "",
            "",
        ),
        ("D008", "9001", DONE, "2026-09-02 10:00", "", "済", ""),
        (
            "D009",
            "7777",
            APO_BPO,
            "2026-09-02 10:00",
            "2026-09-01 00:00",
            "",
            "",
        ),
        (
            "D010",
            "3001",
            DONE,
            "2026-09-10 10:00",
            "2025-12-01 00:00",
            "",
            "",
        ),
        ("D011", "1001", APO, "2026-10-01 00:00", "", "", ""),
        ("D012", "", DONE, "2026-09-02 10:00", "", "", ""),
    ]);
    let apo = deal_sheet(&[
        // 前月取得の BPO アポ: ① の内BPO には入らない（他カードの窓なら入る）
        (
            "A001",
            "1001",
            APO,
            "2026-09-10 10:00",
            "2026-08-27 00:00",
            "",
            "",
        ),
        (
            "A002",
            "2001",
            APO,
            "2026-09-11 10:00",
            "2026-09-02 00:00",
            "",
            "",
        ),
        ("A003", "9001", APO, "2026-09-12 10:00", "", "", ""),
        // 予定日が翌月でも ① には入る（シートが「当月に確定した」もの）
        ("A004", "1002", APO, "2026-10-15 10:00", "", "", ""),
    ]);
    let cyomi = deal_sheet(&[
        // D004 と同じ取引。② と ⑨ の両方に出る
        (
            "D004",
            "1002",
            C,
            "2026-08-10 10:00",
            "2026-08-31 00:00",
            "",
            "",
        ),
        // 予定日が空の Cヨミ
        ("C002", "2001", C, "", "", "", ""),
        ("C003", "9001", C, "2026-08-20 10:00", "", "", ""),
    ]);
    let member = Arc::new(sheet_from_tsv(
        "ownerId\t氏名\tチーム\tHubSpotチーム\t集計対象\n\
         1001\t担当A1\tAチーム\t新規営業\t対象\n\
         1002\t担当A2\tAチーム\t新規営業\t対象\n\
         2001\t担当B1\tBチーム\t新規営業\t対象\n\
         9001\t除外者\t\tコンサル営業\t対象外\n\
         3001\tBPO1\t\tBPO_リクロジ\t対象",
    ));
    let e = super::super::empty_sheet;
    Sheets {
        shodan,
        apo,
        cyomi,
        kaden: e(),
        kaden_list: e(),
        kaden_by_owner: e(),
        member,
        meta: e(),
        weekly: e(),
        kettei: e(),
        list_stock: e(),
        all_cached: true,
    }
}

pub(super) fn synthetic_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()
}

/// 合成入力の「カードの数字」を、いまの数え方で具体値に固定する。
/// これは**実装前から通る**（現行の数え方の確認）。内訳の実装で数字が動いたら落ちる。
#[test]
fn 合成入力のカードの数字は現行の数え方どおり() {
    let body = payload_of(&synthetic_sheets(), synthetic_day());
    let sum = |k: &str| super::team_sum(&body, k);
    assert_eq!(sum("pool"), 9, "③ D002..D007,D009,D010,D012");
    assert_eq!(sum("未処理"), 2, "D002,D009");
    assert_eq!(
        sum("これから"),
        1,
        "D003（cutoff 当日 0:00 は過ぎていない）"
    );
    assert_eq!(sum("実施"), 4, "D004,D006,D010,D012");
    assert_eq!(sum("未実施"), 1, "D005");
    assert_eq!(sum("要判定"), 1, "D007");
    assert_eq!(sum("anq_den"), 8, "④ = これから以外");
    assert_eq!(sum("anq_num"), 1, "D004");
    assert_eq!(sum("bpo_pool"), 2, "D007（前月）,D009（当月）");
    assert_eq!(sum("apo"), 3, "A001,A002,A004");
    assert_eq!(sum("bpo_apo"), 1, "A002 だけ（A001 は前月取得）");
    assert_eq!(sum("cyomi"), 2, "D004,C002");
    assert_eq!(sum("bpo_cyomi"), 1, "D004（8/31 取得 = 前月窓）");
    assert_eq!(body["excluded"]["件数"].as_i64(), Some(3), "D008,A003,C003");
    let by_person = body["by_person"].as_object().unwrap();
    assert_eq!(by_person["7777"]["未処理"].as_i64(), Some(1));
    assert_eq!(by_person[""]["実施"].as_i64(), Some(1));
}

/// 合成入力で、内訳の行が具体的にどの取引かまで確かめる。
#[test]
fn 合成入力の内訳の行はカードの取引そのもの() {
    let body = payload_of(&synthetic_sheets(), synthetic_day());
    let ids = |src: &str| -> BTreeSet<String> {
        card_rows(&body, src)
            .iter()
            .map(|r| r["id"].as_str().unwrap_or("").to_string())
            .collect()
    };
    let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(
        ids("pool"),
        set(&["D002", "D003", "D004", "D005", "D006", "D007", "D009", "D010", "D012"])
    );
    assert_eq!(ids("apo"), set(&["A001", "A002", "A004"]));
    assert_eq!(ids("cyomi"), set(&["D004", "C002"]));

    let row = |src: &str, id: &str| -> Value {
        card_rows(&body, src)
            .iter()
            .find(|r| r["id"].as_str() == Some(id))
            .cloned()
            .unwrap_or_else(|| panic!("{src} に {id} が無い"))
    };
    let kinds: BTreeMap<&str, &str> = [
        ("D002", "未処理"),
        ("D003", "これから"),
        ("D004", "実施"),
        ("D005", "未実施"),
        ("D006", "実施"),
        ("D007", "要判定"),
        ("D009", "未処理"),
        ("D010", "実施"),
        ("D012", "実施"),
    ]
    .into_iter()
    .collect();
    for (id, k) in &kinds {
        assert_eq!(row("pool", id)["kind"].as_str(), Some(*k), "{id}");
    }
    // BPO の窓: カードごとに違う
    assert_eq!(row("pool", "D007")["bpo"].as_bool(), Some(true));
    assert_eq!(row("pool", "D010")["bpo"].as_bool(), Some(false));
    assert_eq!(row("apo", "A001")["bpo"].as_bool(), Some(false));
    assert_eq!(row("apo", "A002")["bpo"].as_bool(), Some(true));
    assert_eq!(row("cyomi", "D004")["bpo"].as_bool(), Some(true));
    // アンケート
    assert_eq!(row("pool", "D004")["anq"].as_bool(), Some(true));
    assert_eq!(row("pool", "D005")["anq"].as_bool(), Some(false));
    // 名簿にいない人・担当なし
    assert_eq!(row("pool", "D009")["team"].as_str(), Some("チーム未設定"));
    assert_eq!(
        row("pool", "D009")["ownerName"].as_str(),
        Some("owner_7777")
    );
    assert_eq!(row("pool", "D012")["ownerName"].as_str(), Some("担当なし"));
    // 予定日が空の Cヨミは date も空（「—」で出す）
    assert_eq!(row("cyomi", "C002")["date"].as_str(), Some(""));
    // 日付・取引名
    assert_eq!(row("pool", "D006")["date"].as_str(), Some("2026-09-02"));
    assert_eq!(row("pool", "D006")["name"].as_str(), Some("取引D006"));
}

/// 合成入力で、全絞り込みの組み合わせでもカードと内訳がそろう。
#[test]
fn 合成入力でもチームの全組み合わせと担当者選択でカードと内訳が一致する() {
    let body = payload_of(&synthetic_sheets(), synthetic_day());
    assert_all_scopes(&body);
}
