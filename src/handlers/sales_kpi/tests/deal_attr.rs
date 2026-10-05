//! 営業KPI カード内訳の「商談属性」(HubSpot プロパティ `shoudanzokusei`、2026-10-05 に「商談種別」から置き換え)
//!
//! 値と表示名は同じ(変換なし): 決裁者商談 / 決定者商談 / 担当者商談。並びもこの順。
//! シート(商談 / アポ / Cヨミ)の末尾に列名「商談属性」で入る。列が無いシートでは行に値を付けない。
//!
//! 守ること:
//!   - 属性ごとの行数の合計 == カードの件数(全担当者 × 全カードのキー。BPO も)
//!   - 列が無いシートでは行に値を付けない(既存の JSON を変えない)
//!   - 空・定義外(旧「商談種別」の内部値を含む)が混ざっても、取引が落ちない / 割れない
//!   - 旧「商談種別」の入れ違い変換は残っていない(「代表者商談」は定義外として出る)

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::card_breakdown::{
    by_person_get, card_preds, card_rows, synthetic_day, synthetic_sheets, team_of, Scope,
};
use super::{build_payload, fixture_day, fixture_sheets, payload_of};
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::sales_kpi::{deal_attr_label, deal_attr_rank, Sheets, DEAL_ATTR_ORDER};

// ---------------------------------------------------------------- 変換(ユニット)

#[test]
fn 商談属性の既知の3値はそのまま通す() {
    assert_eq!(deal_attr_label("決裁者商談"), "決裁者商談");
    assert_eq!(deal_attr_label("決定者商談"), "決定者商談");
    assert_eq!(deal_attr_label("担当者商談"), "担当者商談");
}

/// 旧「商談種別」の内部値 → ラベルの入れ違い変換は廃止した。変換が残っていると
/// 「担当者商談」が「決裁者商談」に化けて件数が黙って動く。
#[test]
fn 旧商談種別の変換は残っていない() {
    assert_eq!(deal_attr_label("担当者商談"), "担当者商談");
    assert_eq!(deal_attr_label("代表者商談"), "代表者商談(定義外)");
    assert_eq!(deal_attr_label("非決裁者商談"), "非決裁者商談(定義外)");
}

#[test]
fn 商談属性が空なら未設定にする() {
    assert_eq!(deal_attr_label(""), "(未設定)");
    assert_eq!(deal_attr_label("   "), "(未設定)");
    assert_eq!(deal_attr_label("\u{3000}"), "(未設定)");
}

#[test]
fn 商談属性の表に無い値は値そのままに定義外を付ける() {
    assert_eq!(deal_attr_label("新属性"), "新属性(定義外)");
    assert_eq!(deal_attr_label("新属性;新属性"), "新属性(定義外)");
}

/// `;` 区切りの複数値は割らず 1 件として数える(割ると 1 取引が 2 回数えられる)。
/// 複数値は表に無い値なので、全体に `(定義外)` を 1 回だけ付ける。並びは入力のまま。
#[test]
fn 商談属性の複数値は割らず全体を定義外にする() {
    assert_eq!(
        deal_attr_label("決裁者商談;担当者商談"),
        "決裁者商談;担当者商談(定義外)"
    );
    assert_eq!(
        deal_attr_label("担当者商談;決裁者商談"),
        "担当者商談;決裁者商談(定義外)"
    );
    assert_eq!(
        deal_attr_label("決定者商談;新属性"),
        "決定者商談;新属性(定義外)"
    );
}

/// 部分の前後の空白は落とす。空の部分は無かったことにする。
/// 同じ部分の重複は 1 つにする。残りが 1 つなら、単独の値と同じ扱い。
#[test]
fn 商談属性の複数値は空白と空の部分と重複を整理する() {
    assert_eq!(
        deal_attr_label(" 決裁者商談 ; 担当者商談 "),
        "決裁者商談;担当者商談(定義外)"
    );
    assert_eq!(deal_attr_label("決定者商談;"), "決定者商談");
    assert_eq!(deal_attr_label(";決定者商談"), "決定者商談");
    assert_eq!(
        deal_attr_label("決裁者商談;;担当者商談"),
        "決裁者商談;担当者商談(定義外)"
    );
    assert_eq!(deal_attr_label(";"), "(未設定)");
    assert_eq!(deal_attr_label(" ; \u{3000};"), "(未設定)");
    assert_eq!(deal_attr_label("担当者商談;担当者商談"), "担当者商談");
}

#[test]
fn 商談属性は前後の空白を落としてから引く() {
    assert_eq!(deal_attr_label(" 決定者商談 "), "決定者商談");
    assert_eq!(deal_attr_label("\u{3000}担当者商談"), "担当者商談");
}

#[test]
fn 商談属性の並び順は決裁者決定者担当者の順で未設定そして定義外() {
    assert_eq!(
        DEAL_ATTR_ORDER,
        ["決裁者商談", "決定者商談", "担当者商談", "(未設定)"]
    );
    let r = |l: &str| deal_attr_rank(l);
    assert!(r("決裁者商談") < r("決定者商談"));
    assert!(r("決定者商談") < r("担当者商談"));
    assert!(r("担当者商談") < r("(未設定)"));
    assert!(r("(未設定)") < r("新属性(定義外)"));
    assert_eq!(
        r("新属性(定義外)"),
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

/// 取り混ぜた生の値と期待ラベル。旧「商談種別」の内部値(代表者商談)は定義外として出る。
/// 期待ラベルはここに手で書く(実装の関数を使わない)。
const CYCLE: [(&str, &str); 8] = [
    ("決裁者商談", "決裁者商談"),
    ("決定者商談", "決定者商談"),
    ("担当者商談", "担当者商談"),
    ("", "(未設定)"),
    (" 決定者商談 ", "決定者商談"),
    ("新属性", "新属性(定義外)"),
    ("決裁者商談;担当者商談", "決裁者商談;担当者商談(定義外)"),
    ("代表者商談", "代表者商談(定義外)"),
];

/// 行の通し番号で CYCLE を回す。dealId → 期待ラベル も返す(同じ dealId が割れたら None)。
fn cycle_col(
    sheet: &SheetData,
    shift: usize,
) -> (Arc<SheetData>, HashMap<String, Option<&'static str>>) {
    let mut idx = shift;
    let mut expect: HashMap<String, Option<&'static str>> = HashMap::new();
    let out = add_col(sheet, "商談属性", |s, r| {
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

fn nt(r: &Value) -> Option<&str> {
    r["deal_attr"].as_str()
}

// ---------------------------------------------------------------- 実データ(fixture + 種別の列)

/// 担当者ごと・全カードのキーで、種別ごとの行数の合計 == by_person の件数(BPO も)。
/// 全行がラベルを持つこと(空・定義外も数える)、ラベルが期待どおりであることも見る。
#[test]
fn 種別ごとの行数の合計がカードの件数と担当者ごとに一致する() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    assert_eq!(body["deal_attr_available"].as_bool(), Some(true));
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
                    bad.push(format!("{o} {}: 行 {} に deal_attr が無い", p.key, r["id"]));
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
    // 取り混ぜたラベルがすべて出ている(旧「商談種別」の内部値「代表者商談」は定義外として出る)
    let seen: BTreeSet<&str> = card_rows(&body, "pool").iter().filter_map(nt).collect();
    let want: BTreeSet<&str> = [
        "決裁者商談",
        "決定者商談",
        "担当者商談",
        "(未設定)",
        "新属性(定義外)",
        "決裁者商談;担当者商談(定義外)",
        "代表者商談(定義外)",
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
fn 下段の一覧の行にも商談属性のラベルが付く() {
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
            let l = nt(r).unwrap_or_else(|| panic!("{k} の行 {} に deal_attr が無い", r["id"]));
            assert!(!l.is_empty(), "{k}: 空のラベルが出ている");
        }
    }
}

// ---------------------------------------------------------------- 合成入力(取り混ぜた値を 1 件ずつ)

/// 合成入力(card_breakdown の `synthetic_sheets`)に、取引ごとに決めた生の値を入れる。
/// 同じ取引(D004)は商談シートでは素の値、Cヨミでは前後に空白つきで入れる(1 つの属性に束ねられること)。
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
        ("D002", "決定者商談"),
        ("D003", "決裁者商談"),
        ("D004", "担当者商談"),
        ("D005", ""),
        ("D006", " 決裁者商談 "),
        ("D007", "新属性"),
        ("D008", "担当者商談"),
        ("D009", "決定者商談"),
        ("D010", "決裁者商談;担当者商談"),
        ("D011", "担当者商談"),
        ("D012", "代表者商談"),
    ];
    const APO: &[(&str, &str)] = &[
        ("A001", "決定者商談"),
        ("A002", "決裁者商談"),
        ("A003", "担当者商談"),
        ("A004", "不明"),
    ];
    const CYOMI: &[(&str, &str)] = &[
        ("D004", " 担当者商談 "),
        ("C002", ""),
        ("C003", "決定者商談"),
    ];
    Sheets {
        shodan: add_col(&base.shodan, "商談属性", by_id(POOL)),
        apo: add_col(&base.apo, "商談属性", by_id(APO)),
        cyomi: add_col(&base.cyomi, "商談属性", by_id(CYOMI)),
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
fn 合成入力の商談属性は取引ごとに期待したラベルになる() {
    let body = payload_of(&synthetic_with_types(), synthetic_day());
    let pool = labels(&body, "pool");
    let want_pool: BTreeMap<String, String> = [
        ("D002", "決定者商談"),
        ("D003", "決裁者商談"),
        ("D004", "担当者商談"),
        ("D005", "(未設定)"),
        ("D006", "決裁者商談"),
        ("D007", "新属性(定義外)"),
        ("D009", "決定者商談"),
        ("D010", "決裁者商談;担当者商談(定義外)"),
        ("D012", "代表者商談(定義外)"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(pool, want_pool);
    let apo = labels(&body, "apo");
    assert_eq!(apo["A001"], "決定者商談");
    assert_eq!(apo["A002"], "決裁者商談");
    assert_eq!(apo["A004"], "不明(定義外)");
    let cy = labels(&body, "cyomi");
    assert_eq!(
        cy["D004"], "担当者商談",
        "前後に空白がついた同じ取引も 1 つの属性に束ねる"
    );
    assert_eq!(cy["C002"], "(未設定)");
    // 決裁者商談は素の値・空白つきを 1 つに束ねて 2 件(D003, D006)
    assert_eq!(pool.values().filter(|v| *v == "決裁者商談").count(), 2);
    // カードの数字は属性を足す前と同じ(③ 9 件、④ 8 件)
    assert_eq!(super::team_sum(&body, "pool"), 9);
    assert_eq!(super::team_sum(&body, "anq_den"), 8);
}

#[test]
fn 合成入力でも属性の合計はどの絞り込みでもカードの件数と一致する() {
    let body = payload_of(&synthetic_with_types(), synthetic_day());
    assert_type_scopes(&body);
}

// ---------------------------------------------------------------- 列が無い / 一部だけある

/// 既存の fixture には「商談属性」列が無い。行に値を付けず、無いことを payload で伝える。
#[test]
fn 列が無いシートでは行に商談属性を付けず未取得を伝える() {
    let body = payload_of(&fixture_sheets(), fixture_day());
    assert_eq!(body["deal_attr_available"].as_bool(), Some(false));
    for s in ["pool", "apo", "cyomi"] {
        assert_eq!(body["deal_attr_sheets"][s].as_bool(), Some(false), "{s}");
        assert!(!card_rows(&body, s).is_empty());
        for r in card_rows(&body, s) {
            assert!(r.get("deal_attr").is_none(), "{s} の行に値が付いている");
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
            assert!(r.get("deal_attr").is_none(), "{k} の行に値が付いている");
        }
    }
    assert_eq!(
        body["deal_attr_order"],
        serde_json::json!(["決裁者商談", "決定者商談", "担当者商談", "(未設定)"])
    );
}

/// 書き出しの反映がシートごとにずれる期間: 商談シートにだけ列がある。
#[test]
fn 商談シートだけに列があるときはシートごとの有無を分けて伝える() {
    let base = fixture_sheets();
    let (shodan, _) = cycle_col(&base.shodan, 0);
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(body["deal_attr_available"].as_bool(), Some(true));
    assert_eq!(body["deal_attr_sheets"]["pool"].as_bool(), Some(true));
    assert_eq!(body["deal_attr_sheets"]["apo"].as_bool(), Some(false));
    assert_eq!(body["deal_attr_sheets"]["cyomi"].as_bool(), Some(false));
    assert!(card_rows(&body, "pool").iter().all(|r| nt(r).is_some()));
    assert!(card_rows(&body, "apo").iter().all(|r| nt(r).is_none()));
    assert!(card_rows(&body, "cyomi").iter().all(|r| nt(r).is_none()));
}

/// 列はあるが全部空 = 「未取得」ではなく「(未設定)が全件」。区別できること。
#[test]
fn 列はあるが値が全部空なら未設定として全件数える() {
    let base = fixture_sheets();
    let shodan = add_col(&base.shodan, "商談属性", |_, _| String::new());
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(body["deal_attr_sheets"]["pool"].as_bool(), Some(true));
    let pool = card_rows(&body, "pool");
    assert_eq!(pool.len(), 537);
    assert!(pool.iter().all(|r| nt(r) == Some("(未設定)")));
}

// ---------------------------------------------------------------- 並びの決め方(Rust の 1 か所)

/// 並びは Rust(`deal_attr_rank`)が決めて payload の `deal_attr_order` で渡す。
/// 画面(JS)はそれに従うだけ。順は 決裁者商談 → 決定者商談 → 担当者商談 → (未設定) → 定義外(名前順)。
/// 定義外は、その月のシートに実際に出てきたものだけを後ろへ足す。
#[test]
fn 並びは_payload_が全ラベルを順に渡す() {
    let c = fixture_with_types();
    let body = payload_of(&c.sheets, fixture_day());
    let order: Vec<&str> = body["deal_attr_order"]
        .as_array()
        .expect("deal_attr_order")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "決裁者商談",
            "決定者商談",
            "担当者商談",
            "(未設定)",
            // 定義外はコードポイントの名前順(代 < 新 < 決)
            "代表者商談(定義外)",
            "新属性(定義外)",
            "決裁者商談;担当者商談(定義外)"
        ]
    );
    // 画面に出るラベルはすべて order にある(JS が並べられない行が出ない)
    for src in ["pool", "apo", "cyomi"] {
        for r in card_rows(&body, src) {
            let l = nt(r).unwrap();
            assert!(order.contains(&l), "{src}: {l} が order に無い");
        }
    }
    // 必ず並べる(0 件でも出す)3 つ
    assert_eq!(
        body["deal_attr_fixed"],
        serde_json::json!(["決裁者商談", "決定者商談", "担当者商談"])
    );
}

/// 列があっても定義外が無ければ、固定の 4 つだけ。
#[test]
fn 並びは定義外が無ければ固定の_4_つだけ() {
    let base = fixture_sheets();
    let shodan = add_col(&base.shodan, "商談属性", |_, _| {
        "担当者商談".to_string()
    });
    let body = payload_of(&Sheets { shodan, ..base }, fixture_day());
    assert_eq!(
        body["deal_attr_order"],
        serde_json::json!(["決裁者商談", "決定者商談", "担当者商談", "(未設定)"])
    );
}

/// 並びを決めるのは Rust だけ。テンプレートに種別のラベルの直書きが無いこと。
#[test]
fn テンプレートに並びや未設定の直書きが無い() {
    let tpl = include_str!("../../../../templates/tabs/sales_kpi.html");
    assert!(
        !tpl.contains("(未設定)"),
        "テンプレートに '(未設定)' の直書きがある。並びは payload の deal_attr_order に従うこと"
    );
    assert!(tpl.contains("deal_attr_order"));
    assert!(tpl.contains("deal_attr_fixed"));
    // 種別の並べ替え(order の定義から表の組み立てまで)に名前順の比較を持ち込まない
    let from = tpl.find("const order=D.deal_attr_order").expect("order");
    let to = from + tpl[from..].find("className='ntt'").expect("ntt");
    assert!(
        !tpl[from..to].contains("localeCompare"),
        "種別の並べ替えを JS で決めている"
    );
}

/// 分子でない区分を選んだとき、種別表の分子の列は 0 になる。列は隠さず、注釈で理由を示す。
#[test]
fn 分子に当たらない区分を選んだとき注釈を出す() {
    let tpl = include_str!("../../../../templates/tabs/sales_kpi.html");
    assert!(tpl.contains("panel1-num-note"));
    assert!(tpl.contains("numSeg&&cardSeg&&cardSeg!==conf.num"));
    assert!(tpl.contains("は分子に当たらないため、分子の列は計算できません（0 と表示しています）"));
}

// ---------------------------------------------------------------- JSON のスナップショット

/// `examples/dump_sales_kpi --attr` が足す「商談属性」の生の値（同じ並び・同じ回し方）。
const SNAPSHOT_CYCLE: [&str; 9] = [
    "決裁者商談",
    "決定者商談",
    "担当者商談",
    "",
    " 決定者商談 ",
    "新属性",
    "決裁者商談;担当者商談",
    "決裁者商談;",
    "代表者商談",
];

/// 🔴 商談属性の列があるときの JSON が、書き出したスナップショットと 1 バイトも変わらないことの証明。
/// 期待値は `dump_sales_kpi -- out.json 2026-09-04 --attr` の出力
/// （tests/fixtures/sales_kpi/payload_2026-09-04_attr.json。取り直したら差分を目で確かめる）。
#[test]
fn 商談属性の列があるjsonはスナップショットと一致する() {
    let base = fixture_sheets();
    let with = |sheet: &SheetData, shift: usize| {
        let mut i = shift;
        add_col(sheet, "商談属性", |_, _| {
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
        .join("tests/fixtures/sales_kpi/payload_2026-09-04_attr.json");
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("スナップショットが読めません {}: {e}", path.display()));
    assert!(
        actual == expected,
        "商談属性つきの JSON がスナップショットと違う（actual {} / expected {} バイト）",
        actual.len(),
        expected.len()
    );
}
