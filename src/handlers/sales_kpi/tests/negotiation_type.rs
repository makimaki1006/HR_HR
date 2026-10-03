//! 営業KPI カード内訳の「商談種別」(2026-10-02 追加・段階B: 実装前に落ちるテスト)
//!
//! 設計と逆証明の一覧: `claudedocs/SALES_KPI_NEGOTIATION_TYPE_2026-10-02.md`
//!
//! HubSpot の取引プロパティ `negotiation_type`(enumeration)は **内部値とラベルが入れ違っている**:
//!   内部値「代表者商談」→ ラベル「非決裁者商談」
//!   内部値「担当者商談」→ ラベル「決裁者商談」
//! シート(商談 / アポ / Cヨミ)には内部値が「商談種別」列で入る予定。画面にはラベルだけを出す。
//!
//! 守ること:
//!   - 種別ごとの行数の合計 == カードの件数(全担当者 × 全カードのキー。BPO も)
//!   - 列が無いシートでは行に値を付けない(既存の JSON を変えない)
//!   - 内部値・ラベル・空・定義外が混ざっても、1 つの種別が割れない / 取引が落ちない

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::card_breakdown::{
    by_person_get, card_preds, card_rows, synthetic_day, synthetic_sheets, team_of, Scope,
};
use super::{build_payload, fixture_day, fixture_sheets, payload_of};
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::sales_kpi::{
    negotiation_type_label, negotiation_type_rank, Sheets, NEGOTIATION_TYPE_ORDER,
};

// ---------------------------------------------------------------- 変換(ユニット)

/// 内部値 → ラベル は入れ違っている。ここを逆に書くと決裁者と非決裁者が丸ごと入れ替わる
/// (件数の合計は合うので画面を見ても気づけない)ので、具体値で固定する。
#[test]
fn 商談種別の内部値はラベルへ入れ違いを直して変換する() {
    assert_eq!(negotiation_type_label("代表者商談"), "非決裁者商談");
    assert_eq!(negotiation_type_label("担当者商談"), "決裁者商談");
}

#[test]
fn 商談種別はラベルが来たらそのまま通す() {
    assert_eq!(negotiation_type_label("決裁者商談"), "決裁者商談");
    assert_eq!(negotiation_type_label("非決裁者商談"), "非決裁者商談");
}

#[test]
fn 商談種別が空なら未設定にする() {
    assert_eq!(negotiation_type_label(""), "(未設定)");
    assert_eq!(negotiation_type_label("   "), "(未設定)");
    assert_eq!(negotiation_type_label("\u{3000}"), "(未設定)");
}

#[test]
fn 商談種別の表に無い値は値そのままに定義外を付ける() {
    assert_eq!(negotiation_type_label("新種別"), "新種別(定義外)");
    assert_eq!(negotiation_type_label("新種別;新種別"), "新種別(定義外)");
}

/// `;` 区切りの複数値は割らず 1 件として数える(割ると 1 取引が 2 回数えられる)。
/// ただし表示は各部分を同じ変換表でラベルに直す。内部値(代表者商談・担当者商談)は画面に出さない。
/// 複数値は表に無い値なので、全体に `(定義外)` を 1 回だけ付ける。
#[test]
fn 商談種別の複数値は各部分をラベルに直して全体を定義外にする() {
    assert_eq!(
        negotiation_type_label("担当者商談;代表者商談"),
        "決裁者商談;非決裁者商談(定義外)"
    );
    // 並びは入力のまま(並べ替えない)
    assert_eq!(
        negotiation_type_label("代表者商談;担当者商談"),
        "非決裁者商談;決裁者商談(定義外)"
    );
    // ラベル形と内部値形が混ざっても各部分を直す
    assert_eq!(
        negotiation_type_label("決裁者商談;代表者商談"),
        "決裁者商談;非決裁者商談(定義外)"
    );
    // 部分に定義外が混ざる: その部分は値そのまま。(定義外) は全体に 1 回だけ
    assert_eq!(
        negotiation_type_label("担当者商談;新種別"),
        "決裁者商談;新種別(定義外)"
    );
}

/// 部分の前後の空白は落とす。空の部分は無かったことにする。
/// 同じ部分の重複は 1 つにする。残りが 1 つなら、単独の値と同じ扱い。
#[test]
fn 商談種別の複数値は空白と空の部分と重複を整理する() {
    assert_eq!(
        negotiation_type_label(" 担当者商談 ; 代表者商談 "),
        "決裁者商談;非決裁者商談(定義外)"
    );
    assert_eq!(negotiation_type_label("担当者商談;"), "決裁者商談");
    assert_eq!(negotiation_type_label(";代表者商談"), "非決裁者商談");
    assert_eq!(
        negotiation_type_label("担当者商談;;代表者商談"),
        "決裁者商談;非決裁者商談(定義外)"
    );
    assert_eq!(negotiation_type_label(";"), "(未設定)");
    assert_eq!(negotiation_type_label(" ; \u{3000};"), "(未設定)");
    assert_eq!(
        negotiation_type_label("担当者商談;担当者商談"),
        "決裁者商談"
    );
    assert_eq!(
        negotiation_type_label("担当者商談;決裁者商談"),
        "決裁者商談"
    );
}

#[test]
fn 商談種別は前後の空白を落としてから引く() {
    assert_eq!(negotiation_type_label(" 担当者商談 "), "決裁者商談");
    assert_eq!(negotiation_type_label("\u{3000}代表者商談"), "非決裁者商談");
}

#[test]
fn 商談種別の並び順は固定で決裁者から未設定そして定義外() {
    assert_eq!(
        NEGOTIATION_TYPE_ORDER,
        ["決裁者商談", "非決裁者商談", "(未設定)"]
    );
    let r = |l: &str| negotiation_type_rank(l);
    assert!(r("決裁者商談") < r("非決裁者商談"));
    assert!(r("非決裁者商談") < r("(未設定)"));
    assert!(r("(未設定)") < r("新種別(定義外)"));
    assert_eq!(
        r("新種別(定義外)"),
        r("別の値(定義外)"),
        "定義外は同じ順位(後ろで名前順)"
    );
}

// ---------------------------------------------------------------- 列を足す道具

/// シートの末尾に列を足す。`val` は (行) → セルの値。
fn add_col(
    sheet: &SheetData,
    name: &str,
    mut val: impl FnMut(&SheetData, &[Arc<str>]) -> String,
) -> Arc<SheetData> {
    let mut header = sheet.header.clone();
    header.push(name.to_string());
    let rows: Vec<Vec<Arc<str>>> = sheet
        .rows
        .iter()
        .map(|r| {
            let mut r2 = r.clone();
            r2.push(Arc::from(val(sheet, r).as_str()));
            r2
        })
        .collect();
    Arc::new(SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    })
}

/// 内部値・ラベル・空白つき・空・定義外・複数値を取り混ぜる(生の値, 期待ラベル)。
/// 期待ラベルはここに手で書く(実装の関数を使わない)。
const CYCLE: [(&str, &str); 8] = [
    ("代表者商談", "非決裁者商談"),
    ("担当者商談", "決裁者商談"),
    ("決裁者商談", "決裁者商談"),
    ("非決裁者商談", "非決裁者商談"),
    ("", "(未設定)"),
    (" 担当者商談 ", "決裁者商談"),
    ("新種別", "新種別(定義外)"),
    ("担当者商談;代表者商談", "決裁者商談;非決裁者商談(定義外)"),
];

/// 行の通し番号で CYCLE を回す。dealId → 期待ラベル も返す(同じ dealId が割れたら None)。
fn cycle_col(
    sheet: &SheetData,
    shift: usize,
) -> (Arc<SheetData>, HashMap<String, Option<&'static str>>) {
    let mut idx = shift;
    let mut expect: HashMap<String, Option<&'static str>> = HashMap::new();
    let out = add_col(sheet, "商談種別", |s, r| {
        let i = idx;
        idx += 1;
        let (raw, label) = CYCLE[i % CYCLE.len()];
        let id = s.get(r, "dealId").to_string();
        expect
            .entry(id)
            .and_modify(|e| {
                if *e != Some(label) {
                    *e = None;
                }
            })
            .or_insert(Some(label));
        raw.to_string()
    });
    (out, expect)
}

struct Cycled {
    sheets: Sheets,
    pool: HashMap<String, Option<&'static str>>,
    apo: HashMap<String, Option<&'static str>>,
    cyomi: HashMap<String, Option<&'static str>>,
}

fn fixture_with_types() -> Cycled {
    let base = fixture_sheets();
    let (shodan, pool) = cycle_col(&base.shodan, 0);
    let (apo, apo_m) = cycle_col(&base.apo, 3);
    let (cyomi, cyomi_m) = cycle_col(&base.cyomi, 5);
    Cycled {
        sheets: Sheets {
            shodan,
            apo,
            cyomi,
            ..base
        },
        pool,
        apo: apo_m,
        cyomi: cyomi_m,
    }
}

fn nt<'a>(r: &'a Value) -> Option<&'a str> {
    r["negotiation_type"].as_str()
}

// ---------------------------------------------------------------- 実データ(fixture + 種別の列)

/// 担当者ごと・全カードのキーで、種別ごとの行数の合計 == by_person の件数(BPO も)。
/// 全行がラベルを持つこと(空・定義外も数える)、ラベルが期待どおりであることも見る。
#[test]
fn 種別ごとの行数の合計がカードの件数と担当者ごとに一致する() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    assert_eq!(body["negotiation_type_available"].as_bool(), Some(true));
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
    let mut checked = 0usize;
    for o in &owners {
        for p in card_preds() {
            let mut by_type: BTreeMap<String, (i64, i64)> = BTreeMap::new();
            for r in card_rows(&body, p.src)
                .iter()
                .filter(|r| r["owner"].as_str() == Some(o.as_str()))
                .filter(|r| (p.pred)(r))
            {
                let Some(label) = nt(r) else {
                    bad.push(format!(
                        "{o} {}: 行 {} に negotiation_type が無い",
                        p.key, r["id"]
                    ));
                    continue;
                };
                let e = by_type.entry(label.to_string()).or_default();
                e.0 += 1;
                if r["bpo"].as_bool() == Some(true) {
                    e.1 += 1;
                }
            }
            let sum: i64 = by_type.values().map(|v| v.0).sum();
            let sum_bpo: i64 = by_type.values().map(|v| v.1).sum();
            if sum != by_person_get(&body, o, &p.key) {
                bad.push(format!(
                    "{o} {}: 種別の合計 {sum} ≠ カード {}",
                    p.key,
                    by_person_get(&body, o, &p.key)
                ));
            }
            if sum_bpo != by_person_get(&body, o, &p.bpo_key) {
                bad.push(format!(
                    "{o} {}: 種別の内BPO {sum_bpo} ≠ カード {}",
                    p.bpo_key,
                    by_person_get(&body, o, &p.bpo_key)
                ));
            }
            checked += 1;
        }
    }
    assert!(
        bad.is_empty(),
        "{} 件ずれ:\n{}",
        bad.len(),
        bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
    assert!(checked > 100, "検査が空振りしている");

    // ラベルが期待どおり(dealId → 期待ラベル)。同じ dealId が別ラベルに割れる行は除く
    let mut verified = 0usize;
    for (src, m) in [("pool", &c.pool), ("apo", &c.apo), ("cyomi", &c.cyomi)] {
        for r in card_rows(&body, src) {
            let id = r["id"].as_str().unwrap_or("");
            if let Some(Some(want)) = m.get(id) {
                assert_eq!(nt(r), Some(*want), "{src} の {id}");
                verified += 1;
            }
        }
    }
    assert!(verified > 800, "ラベルの突き合わせが空振り: {verified}");
    // 取り混ぜた 7 種類のラベルがすべて出ている(内部値の文字列「代表者商談」「担当者商談」は出ない)
    let seen: BTreeSet<&str> = card_rows(&body, "pool").iter().filter_map(nt).collect();
    let want: BTreeSet<&str> = [
        "決裁者商談",
        "非決裁者商談",
        "(未設定)",
        "新種別(定義外)",
        "決裁者商談;非決裁者商談(定義外)",
    ]
    .into_iter()
    .collect();
    assert_eq!(seen, want);
}

/// チーム全組み合わせ × チーム選択、担当者選択で、種別の合計 == カードの件数(JS の絞り込みの写し)。
#[test]
fn 種別の合計は絞り込みのどの組み合わせでもカードの件数と一致する() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    assert_type_scopes(&body);
}

fn assert_type_scopes(body: &Value) {
    let teams: Vec<String> = body["teams"]
        .as_array()
        .expect("teams")
        .iter()
        .map(|t| t.as_str().unwrap_or("").to_string())
        .collect();
    let tof = team_of(body);
    let mut bad = Vec::new();
    let mut tried = 0usize;
    let mut check = |scope: &Scope, label: &str| {
        let by_person = body["by_person"].as_object().expect("by_person");
        for p in card_preds() {
            let card: i64 = by_person
                .iter()
                .filter(|(o, _)| scope.has(o, &tof))
                .map(|(_, v)| v[&p.key].as_i64().unwrap_or(0))
                .sum();
            let mut by_type: BTreeMap<&str, i64> = BTreeMap::new();
            for r in card_rows(body, p.src)
                .iter()
                .filter(|r| scope.has(r["owner"].as_str().unwrap_or(""), &tof))
                .filter(|r| (p.pred)(r))
            {
                *by_type.entry(nt(r).unwrap_or("<なし>")).or_default() += 1;
            }
            let sum: i64 = by_type.values().sum();
            if sum != card || by_type.contains_key("<なし>") {
                bad.push(format!(
                    "{label} {}: 種別の合計 {sum} ≠ カード {card} ({by_type:?})",
                    p.key
                ));
            }
        }
        tried += 1;
    };
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
        for sel in std::iter::once(None).chain(teams.iter().map(|t| Some(t.as_str()))) {
            let scope = Scope {
                team: sel,
                person: None,
                hidden: &hidden,
            };
            check(&scope, &format!("外す={hidden_teams:?} 選択={sel:?}"));
        }
    }
    let none = HashSet::new();
    for owner in tof.keys() {
        let scope = Scope {
            team: None,
            person: Some(owner),
            hidden: &none,
        };
        check(&scope, &format!("担当者={owner}"));
    }
    assert!(tried > teams.len());
    assert!(
        bad.is_empty(),
        "{} 件ずれ:\n{}",
        bad.len(),
        bad.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// 下段の一覧(⑦⑤⑨止まっている・今週・来週)の行にも種別が付く。
#[test]
fn 下段の一覧の行にも商談種別のラベルが付く() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    for k in [
        "stale",
        "week_deals",
        "next_week_deals",
        "cyomi_stale",
        "anq_missing",
    ] {
        let rows = body[k].as_array().expect(k);
        assert!(!rows.is_empty(), "{k} が空で検査が効かない");
        for r in rows {
            let l =
                nt(r).unwrap_or_else(|| panic!("{k} の行 {} に negotiation_type が無い", r["id"]));
            assert!(
                l != "代表者商談" && l != "担当者商談",
                "{k}: 内部値がそのまま出ている: {l}"
            );
        }
    }
}

// ---------------------------------------------------------------- 合成入力(取り混ぜた値を 1 件ずつ)

/// 合成入力(card_breakdown の `synthetic_sheets`)に、取引ごとに決めた生の値を入れる。
/// 同じ取引(D004)は商談シートではラベル形、Cヨミでは内部値で入れる(1 つの種別に束ねられること)。
fn synthetic_with_types() -> Sheets {
    let base = synthetic_sheets();
    let by_id = |m: &'static [(&'static str, &'static str)]| {
        move |s: &SheetData, r: &[Arc<str>]| {
            let id = s.get(r, "dealId");
            m.iter()
                .find(|(k, _)| *k == id)
                .map(|(_, v)| v.to_string())
                .unwrap_or_default()
        }
    };
    const POOL: &[(&str, &str)] = &[
        ("D001", "担当者商談"),
        ("D002", "代表者商談"),
        ("D003", "担当者商談"),
        ("D004", "決裁者商談"),
        ("D005", ""),
        ("D006", " 担当者商談 "),
        ("D007", "新種別"),
        ("D008", "担当者商談"),
        ("D009", "代表者商談"),
        ("D010", "担当者商談;代表者商談"),
        ("D011", "担当者商談"),
        ("D012", "担当者商談"),
    ];
    const APO: &[(&str, &str)] = &[
        ("A001", "代表者商談"),
        ("A002", "決裁者商談"),
        ("A003", "担当者商談"),
        ("A004", "不明"),
    ];
    const CYOMI: &[(&str, &str)] = &[("D004", "担当者商談"), ("C002", ""), ("C003", "担当者商談")];
    Sheets {
        shodan: add_col(&base.shodan, "商談種別", by_id(POOL)),
        apo: add_col(&base.apo, "商談種別", by_id(APO)),
        cyomi: add_col(&base.cyomi, "商談種別", by_id(CYOMI)),
        ..base
    }
}

fn labels(body: &Value, src: &str) -> BTreeMap<String, String> {
    card_rows(body, src)
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap_or("").to_string(),
                nt(r).unwrap_or("<なし>").to_string(),
            )
        })
        .collect()
}

#[test]
fn 合成入力の商談種別は取引ごとに期待したラベルになる() {
    let body = payload_of(&synthetic_with_types(), synthetic_day());
    let pool = labels(&body, "pool");
    let want_pool: BTreeMap<String, String> = [
        ("D002", "非決裁者商談"),
        ("D003", "決裁者商談"),
        ("D004", "決裁者商談"),
        ("D005", "(未設定)"),
        ("D006", "決裁者商談"),
        ("D007", "新種別(定義外)"),
        ("D009", "非決裁者商談"),
        ("D010", "決裁者商談;非決裁者商談(定義外)"),
        ("D012", "決裁者商談"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(pool, want_pool);
    let apo = labels(&body, "apo");
    assert_eq!(apo["A001"], "非決裁者商談");
    assert_eq!(apo["A002"], "決裁者商談");
    assert_eq!(apo["A004"], "不明(定義外)");
    let cy = labels(&body, "cyomi");
    assert_eq!(
        cy["D004"], "決裁者商談",
        "内部値で入った同じ取引も決裁者商談に束ねる"
    );
    assert_eq!(cy["C002"], "(未設定)");
    // 決裁者商談は内部値・ラベル・空白つきを 1 つに束ねて 4 件(D003, D004, D006, D012)
    assert_eq!(pool.values().filter(|v| *v == "決裁者商談").count(), 4);
    // カードの数字は種別を足す前と同じ(③ 9 件、④ 8 件)
    assert_eq!(super::team_sum(&body, "pool"), 9);
    assert_eq!(super::team_sum(&body, "anq_den"), 8);
}

#[test]
fn 合成入力でも種別の合計はどの絞り込みでもカードの件数と一致する() {
    let body = payload_of(&synthetic_with_types(), synthetic_day());
    assert_type_scopes(&body);
}

// ---------------------------------------------------------------- 列が無い / 一部だけある

/// 既存の fixture には「商談種別」列が無い。行に値を付けず、無いことを payload で伝える。
#[test]
fn 列が無いシートでは行に商談種別を付けず未取得を伝える() {
    let body = payload_of(&fixture_sheets(), fixture_day());
    assert_eq!(body["negotiation_type_available"].as_bool(), Some(false));
    for s in ["pool", "apo", "cyomi"] {
        assert_eq!(
            body["negotiation_type_sheets"][s].as_bool(),
            Some(false),
            "{s}"
        );
        assert!(!card_rows(&body, s).is_empty());
        for r in card_rows(&body, s) {
            assert!(
                r.get("negotiation_type").is_none(),
                "{s} の行に値が付いている"
            );
        }
    }
    for k in [
        "stale",
        "week_deals",
        "next_week_deals",
        "cyomi_stale",
        "anq_missing",
    ] {
        for r in body[k].as_array().expect(k) {
            assert!(
                r.get("negotiation_type").is_none(),
                "{k} の行に値が付いている"
            );
        }
    }
    assert_eq!(
        body["negotiation_type_order"],
        serde_json::json!(["決裁者商談", "非決裁者商談", "(未設定)"])
    );
}

/// 書き出しの反映がシートごとにずれる期間: 商談シートにだけ列がある。
#[test]
fn 商談シートだけに列があるときはシートごとの有無を分けて伝える() {
    let base = fixture_sheets();
    let (shodan, _) = cycle_col(&base.shodan, 0);
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(body["negotiation_type_available"].as_bool(), Some(true));
    assert_eq!(
        body["negotiation_type_sheets"]["pool"].as_bool(),
        Some(true)
    );
    assert_eq!(
        body["negotiation_type_sheets"]["apo"].as_bool(),
        Some(false)
    );
    assert_eq!(
        body["negotiation_type_sheets"]["cyomi"].as_bool(),
        Some(false)
    );
    assert!(card_rows(&body, "pool").iter().all(|r| nt(r).is_some()));
    assert!(card_rows(&body, "apo").iter().all(|r| nt(r).is_none()));
    assert!(card_rows(&body, "cyomi").iter().all(|r| nt(r).is_none()));
}

/// 列はあるが全部空 = 「未取得」ではなく「(未設定)が全件」。区別できること。
#[test]
fn 列はあるが値が全部空なら未設定として全件数える() {
    let base = fixture_sheets();
    let shodan = add_col(&base.shodan, "商談種別", |_, _| String::new());
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(
        body["negotiation_type_sheets"]["pool"].as_bool(),
        Some(true)
    );
    let pool = card_rows(&body, "pool");
    assert_eq!(pool.len(), 537);
    assert!(pool.iter().all(|r| nt(r) == Some("(未設定)")));
}

// ---------------------------------------------------------------- 並びの決め方(Rust の 1 か所)

/// 並びは Rust(`negotiation_type_rank`)が決めて payload の `negotiation_type_order` で渡す。
/// 画面(JS)はそれに従うだけ。順は 決裁者商談 → 非決裁者商談 → (未設定) → 定義外(名前順)。
/// 定義外は、その月のシートに実際に出てきたものだけを後ろへ足す。
#[test]
fn 並びは_payload_が全ラベルを順に渡す() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    let order: Vec<&str> = body["negotiation_type_order"]
        .as_array()
        .expect("negotiation_type_order")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "決裁者商談",
            "非決裁者商談",
            "(未設定)",
            "新種別(定義外)",
            "決裁者商談;非決裁者商談(定義外)"
        ]
    );
    // 画面に出るラベルはすべて order にある(JS が並べられない行が出ない)
    for src in ["pool", "apo", "cyomi"] {
        for r in card_rows(&body, src) {
            let l = nt(r).unwrap();
            assert!(order.contains(&l), "{src}: {l} が order に無い");
        }
    }
    // 必ず並べる(0 件でも出す)2 つ
    assert_eq!(
        body["negotiation_type_fixed"],
        serde_json::json!(["決裁者商談", "非決裁者商談"])
    );
}

/// 列があっても定義外が無ければ、固定の 3 つだけ。
#[test]
fn 並びは定義外が無ければ固定の_3_つだけ() {
    let base = fixture_sheets();
    let shodan = add_col(&base.shodan, "商談種別", |_, _| {
        "担当者商談".to_string()
    });
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(
        body["negotiation_type_order"],
        serde_json::json!(["決裁者商談", "非決裁者商談", "(未設定)"])
    );
}

/// 並びを決めるのは Rust だけ。テンプレートに種別のラベルの直書きが無いこと。
#[test]
fn テンプレートに並びや未設定の直書きが無い() {
    let tpl = include_str!("../../../../templates/tabs/sales_kpi.html");
    assert!(
        !tpl.contains("(未設定)"),
        "テンプレートに '(未設定)' の直書きがある。並びは payload の negotiation_type_order に従うこと"
    );
    assert!(tpl.contains("negotiation_type_order"));
    assert!(tpl.contains("negotiation_type_fixed"));
    // 種別の並べ替え(order の定義から表の組み立てまで)に名前順の比較を持ち込まない
    let from = tpl
        .find("const order=D.negotiation_type_order")
        .expect("order");
    let to = from + tpl[from..].find("className='ntt'").expect("ntt");
    assert!(
        !tpl[from..to].contains("localeCompare"),
        "種別の並べ替えを JS で決めている"
    );
}

// ---------------------------------------------------------------- JSON のスナップショット

/// `examples/dump_sales_kpi --negtype` が足す「商談種別」の生の値（同じ並び・同じ回し方）。
const SNAPSHOT_CYCLE: [&str; 9] = [
    "代表者商談",
    "担当者商談",
    "決裁者商談",
    "非決裁者商談",
    "",
    " 担当者商談 ",
    "新種別",
    "担当者商談;代表者商談",
    "担当者商談;",
];

/// 🔴 商談種別の列があるときの JSON を、`Value` 実装（b78bd8e = PR #49）の出力と 1 バイトも変えない
/// ことの証明。期待値は b78bd8e の `dump_sales_kpi -- out.json 2026-09-04 --negtype` の出力
/// （tests/fixtures/sales_kpi/payload_2026-09-04_negtype.json）。
#[test]
fn 商談種別の列があるjsonはvalue実装のスナップショットと一致する() {
    let base = fixture_sheets();
    let with = |sheet: &SheetData, shift: usize| {
        let mut i = shift;
        add_col(sheet, "商談種別", |_, _| {
            let v = SNAPSHOT_CYCLE[i % SNAPSHOT_CYCLE.len()];
            i += 1;
            v.to_string()
        })
    };
    let sheets = Sheets {
        shodan: with(&base.shodan, 0),
        apo: with(&base.apo, 3),
        cyomi: with(&base.cyomi, 5),
        ..base
    };
    let actual = serde_json::to_string(&build_payload(&sheets, fixture_day())).expect("JSON 化");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sales_kpi/payload_2026-09-04_negtype.json");
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("スナップショットが読めません {}: {e}", path.display()));
    assert!(
        actual == expected,
        "商談種別つきの JSON が b78bd8e と違う（actual {} / expected {} バイト）",
        actual.len(),
        expected.len()
    );
}
