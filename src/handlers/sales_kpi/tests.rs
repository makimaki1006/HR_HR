//! 営業KPI: 実データでの突き合わせ
//!
//! `tests/fixtures/sales_kpi/` にある TSV は、本番のスプレッドシートから
//! そのまま落としたもの。ただし **このリポジトリは public** なので、
//! 集計に使っていない列は落としてある（取引名＝顧客の会社名、
//! Zoomメール／メール＝社員のメールアドレス、在籍／出どころ）。
//! 落とす列は Hubspot リポジトリ `scripts/sales_kpi/make_fixture.py` の
//! `DROP_COLUMNS` にある。
//!
//! なぜ自作のテストデータを使わないか:
//!   架電クオリティを Rust へ移したとき、テストデータを自作していたせいで
//!   「存在しない列名を読んで常に 0」というバグを検出できなかった。
//!   列名の取り違えは実データでしか見つからない。
//!
//! 期待値は Python 版（これまで現場に見せていた画面 `genba_data.json`）の実測値。
//! 2026-09-04 時点のもので、シートも同じ日の内容。

use std::sync::Arc;
use std::time::Instant;

use chrono::NaiveDate;
use serde_json::Value;

use super::routes::build_payload;
use super::Sheets;
use crate::handlers::call_quality::sheets::SheetData;

/// Python 版を動かした日。ここを変えると期待値も変わる。
///
/// 🔴 **fixture を取り直しても、ここは 2026-09-04 に固定したままにする**
/// （期待値が動かない方が安定するため。2026-09-07 ユーザー判断）。
/// そのぶん、取り直した fixture は判定日より新しい行を含むことがある。
/// 実際に 2026-09-06 まで架電が入った fixture で「9/04 時点」を判定した
/// ことがある。**判定日と fixture の最終日が一致する前提で書かないこと。**
/// 日付や件数は直書きせず、シートから引いて期待値にする。
fn fixture_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()
}

/// シートの列を数字として足す。fixture の取り直しで動く値を、
/// 直書きせずシートから作るために使う。
fn sum_col(sheet: &SheetData, col: &str, keep: impl Fn(&[Arc<str>]) -> bool) -> i64 {
    sheet
        .rows
        .iter()
        .filter(|r| keep(r))
        .map(|r| {
            sheet
                .get(r, col)
                .replace(',', "")
                .parse::<i64>()
                .unwrap_or(0)
        })
        .sum()
}

fn load_tsv(name: &str) -> Arc<SheetData> {
    let path = format!(
        "{}/tests/fixtures/sales_kpi/{name}.tsv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("テストデータが読めません {path}: {e}"));
    Arc::new(sheet_from_tsv(&text))
}

/// タブ区切りの文字列を1枚のシートにする。
fn sheet_from_tsv(text: &str) -> SheetData {
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .expect("見出し行がありません")
        .split('\t')
        .map(|s| s.trim_start_matches('\u{feff}').to_string())
        .collect();
    let rows: Vec<Vec<Arc<str>>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut cells: Vec<Arc<str>> = l.split('\t').map(Arc::from).collect();
            cells.resize(header.len(), Arc::from(""));
            cells
        })
        .collect();
    SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    }
}

fn fixture_sheets() -> Sheets {
    Sheets {
        shodan: load_tsv("KPI営業_商談"),
        apo: load_tsv("KPI営業_アポ"),
        cyomi: load_tsv("KPI営業_Cヨミ"),
        kaden: load_tsv("KPI営業_架電日次"),
        kaden_list: load_tsv("KPI営業_架電リスト"),
        kaden_by_owner: load_tsv("KPI営業_架電リスト_担当別"),
        member: load_tsv("KPI営業_メンバー"),
        meta: load_tsv("KPI営業_取得条件"),
        weekly: load_tsv("KPI営業_週次"),
        all_cached: true,
    }
}

/// `集計対象` 列を落としたメンバーシート。列を足す前の環境の再現で、
/// 「誰も外さない」ときの数字を作るのにも使う。
fn fixture_sheets_counting_everyone() -> Sheets {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/sales_kpi/KPI営業_メンバー.tsv",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("メンバーの fixture が読めません");
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("見出し").split('\t').collect();
    let keep: Vec<usize> = (0..head.len()).filter(|i| head[*i] != "集計対象").collect();
    let pick = |line: &str| {
        let cells: Vec<&str> = line.split('\t').collect();
        keep.iter()
            .map(|i| cells.get(*i).copied().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\t")
    };
    let mut out = vec![keep.iter().map(|i| head[*i]).collect::<Vec<_>>().join("\t")];
    out.extend(lines.filter(|l| !l.trim().is_empty()).map(pick));
    Sheets {
        member: Arc::new(sheet_from_tsv(&out.join("\n"))),
        ..fixture_sheets()
    }
}

/// 週次シートがまだ無いとき（初回）。落ちずに空配列を返せることを見る。
fn fixture_sheets_without_weekly() -> Sheets {
    Sheets {
        weekly: super::empty_sheet(),
        ..fixture_sheets()
    }
}

fn payload() -> Value {
    build_payload(&fixture_sheets(), fixture_day())
}

/// チーム別の数え上げを全チーム分足す。
fn team_sum(body: &Value, key: &str) -> i64 {
    body["by_team"]
        .as_object()
        .expect("by_team")
        .values()
        .map(|c| c.get(key).and_then(Value::as_i64).unwrap_or(0))
        .sum()
}

#[test]
fn 当月の母集団がpython版と一致する() {
    // Python 版の実測: 当月に商談予定日があるもの 537件
    assert_eq!(team_sum(&payload(), "pool"), 537);
}

#[test]
fn 取ったアポがpython版と一致する() {
    // Python 版の実測: ①取得アポ 245件
    assert_eq!(team_sum(&payload(), "apo"), 245);
}

#[test]
fn cヨミと置きっぱなしがpython版と一致する() {
    let body = payload();
    // Python 版の実測: ⑨Cヨミ 127件（30日以上 39件）
    //
    // 🔴 この 39 は 2026-09-05 に両者を揃えたあとの値。
    // それまで Python は「今日0時 − 突入日時」をミリ秒で引いていたため
    // 2026-08-05 16:31 に入った1件が 29.31日と出て 38件だった。
    // 日付で数える（時刻を見ない）方に統一している。現場は
    // 「8/5に入って今日9/4だから30日」と日付で数えるため。
    // 🔴 127 は「誰も外さない」ときの値。2026-09-08 に集計対象外
    // （HubSpotチーム＝コンサル営業）を入れたので、画面はそのぶん減る。
    // 数を直書きし直すのではなく、除外を無かったことにした版と突き合わせる。
    let all_in = build_payload(&fixture_sheets_counting_everyone(), fixture_day());
    assert_eq!(team_sum(&all_in, "cyomi"), 127);
    assert_eq!(
        team_sum(&body, "cyomi"),
        127 - count_owned(&fixture_sheets().cyomi, &excluded_owners(&fixture_sheets())),
        "Cヨミの減り方が、集計対象外の担当者が持っている件数と合わない"
    );
    assert_eq!(team_sum(&all_in, "cyomi_stale"), 39);
    assert_eq!(
        body["cyomi_stale"].as_array().unwrap().len() as i64,
        team_sum(&body, "cyomi_stale")
    );
}

#[test]
fn 今週と来週の件数がpython版と一致する() {
    let body = payload();
    // Python 版の実測: 今週 260件 / 来週 213件 / アンケート未回収 267件
    assert_eq!(body["week_deals"].as_array().unwrap().len(), 260);
    assert_eq!(body["next_week_deals"].as_array().unwrap().len(), 213);
    assert_eq!(body["anq_missing"].as_array().unwrap().len(), 267);
}

#[test]
fn 仕分けの合計が母集団と一致する() {
    let body = payload();
    let parts: i64 = ["実施", "未実施", "未処理", "これから", "要判定"]
        .iter()
        .map(|k| team_sum(&body, k))
        .sum();
    assert_eq!(
        parts,
        team_sum(&body, "pool"),
        "仕分けの合計が母集団と合わない＝どれにも入らない取引がある"
    );
}

#[test]
fn 個人別の合計がチーム別の合計と一致する() {
    let body = payload();
    for key in [
        "pool",
        "実施",
        "未実施",
        "未処理",
        "これから",
        "apo",
        "cyomi",
    ] {
        let by_person: i64 = body["by_person"]
            .as_object()
            .unwrap()
            .values()
            .map(|c| c.get(key).and_then(Value::as_i64).unwrap_or(0))
            .sum();
        assert_eq!(
            by_person,
            team_sum(&body, key),
            "{key} がチームと個人で食い違う"
        );
    }
}

#[test]
fn bpoは当月と前月の窓でしか数えない() {
    let body = payload();
    let bpo_pool = body["bpo_total"]["pool"].as_i64().unwrap_or(0);
    let pool = team_sum(&body, "pool");
    // Python 版の実測: BPO 129件。窓を外して「値の有無」で数えると 200件中71件が
    // 2025年まで遡る古い日付を拾い、過大になる（2026-09-04 に実データで確認）。
    assert_eq!(bpo_pool, 129);
    assert!(bpo_pool < pool, "BPO が母集団を超えている");
}

#[test]
fn 架電数が現場の実数と合う() {
    let body = payload();
    // 菊地さんの 2026-09-04 の実数は 206件（本人に確認）。
    // 判定を "Auto Recorded" だけにして 205件（差 -1）。
    let kiku = body["people"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"].as_str().unwrap_or("").contains("菊地"))
        .map(|p| p["id"].as_str().unwrap().to_string())
        .expect("菊地さんが名簿にいません");
    let today = &body["calls"]["periods"]["today"]["by_person"][&kiku];
    let connected = today["connected"].as_i64().unwrap_or(0);
    assert!(
        (200..=210).contains(&connected),
        "架電数が実数(206)から離れすぎ: {connected}"
    );
}

#[test]
fn 架電リストの内訳が母数と一致する() {
    let body = payload();
    let cls = &body["kaden"]["cls"];
    let sum: i64 = ["未架電", "未接触", "接触済み"]
        .iter()
        .map(|k| cls[k].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(sum, body["kaden"]["base"].as_i64().unwrap());
    assert!(body["kaden"]["total"].as_i64().unwrap() >= sum);
}

#[test]
fn 予定日が未来のものを未処理にしない() {
    // 当日を境目に入れると、まだ終わっていない商談が「未処理」に落ちて
    // 商談化率が下がる（実際に 41.0% と出て、正しくは 65.2% だった）。
    let body = payload();
    for row in body["week_deals"].as_array().unwrap() {
        if row["past"] == Value::Bool(false) {
            assert_ne!(
                row["kind"], "未処理",
                "予定日が未来なのに未処理になっている: {row}"
            );
        }
    }
}

// ---------------------------------------------------------------- 週次の記録

/// 週次シートは Python 側（Hubspot リポジトリ `scripts/sales_kpi/sync_daily.py`）が
/// 集計して書く唯一のシート。テストデータもその `weekly_cells()` に
/// **上の TSV をそのまま食わせて**作ってあるので、ここで一致を見るということは
/// Python の `classify()` と Rust の `classify()` が同じ判定かを見ていることになる。
///
/// 🔴 `make_fixture.py` が落とすのは7枚（週次を除く全部）。取り直したら、**必ず**
/// `python scripts/sales_kpi/make_weekly_fixture.py <fixtureのディレクトリ>`
/// を続けて流して週次も作り直すこと。片方だけ新しいとここが落ちる。
#[test]
fn 週次の行が画面側の集計と一致する() {
    // 🔴 **週次シートは「誰も外さない」数え方のまま**である。
    // 集計対象外（2026-09-08 追加）は画面側にしか入っておらず、週次を書く
    // Python の `weekly_cells()` は全員を数えている。だからここで突き合わせるのは
    // 除外を無かったことにした版。
    // このテストの目的は「Python の classify() と Rust の classify() が同じ判定か」で、
    // 除外の有無はその目的に関係しない。目的は保ったまま比べている。
    // ただし **画面の数字と週次表の数字は、除外したぶんだけ食い違う**。
    // 週次側にも除外を通すかどうかは別途の判断（`sync_daily.py` の週次まわりは
    // 別の作業で触っているため、ここでは手を入れていない）。
    let body = build_payload(&fixture_sheets_counting_everyone(), fixture_day());
    let snaps = body["snapshots"].as_array().expect("snapshots が配列でない");
    let cur = snaps
        .iter()
        .find(|s| s["week"] == "2026-W36")
        .expect("fixture の当週（2026-W36）が無い");
    let t = &cur["totals"];

    for (key, got) in [
        ("pool", team_sum(&body, "pool")),
        ("実施", team_sum(&body, "実施")),
        ("未実施", team_sum(&body, "未実施")),
        ("未処理", team_sum(&body, "未処理")),
        ("これから", team_sum(&body, "これから")),
        ("要判定", team_sum(&body, "要判定")),
        ("apo", team_sum(&body, "apo")),
        ("cyomi", team_sum(&body, "cyomi")),
        ("bpo_pool", body["bpo_total"]["pool"].as_i64().unwrap_or(0)),
    ] {
        assert_eq!(
            t[key].as_i64().unwrap_or(-1),
            got,
            "{key} が週次シートと画面の集計で食い違う＝Python と Rust の判定がずれている"
        );
    }
    assert_eq!(
        cur["stale"].as_i64().unwrap_or(-1),
        body["stale"].as_array().unwrap().len() as i64
    );
    assert_eq!(
        cur["anq_missing"].as_i64().unwrap_or(-1),
        body["anq_missing"].as_array().unwrap().len() as i64
    );
    assert_eq!(
        cur["cyomi_stale"].as_i64().unwrap_or(-1),
        body["cyomi_stale"].as_array().unwrap().len() as i64
    );
}

#[test]
fn 週次は古い順に並び画面が要る項目がそろっている() {
    let body = payload();
    let snaps = body["snapshots"].as_array().unwrap();
    assert_eq!(snaps.len(), 2, "fixture は2週ぶん");
    assert_eq!(snaps[0]["week"], "2026-W35");
    assert_eq!(snaps[1]["week"], "2026-W36");
    // 画面（templates/tabs/sales_kpi.html の「先週との比べ方」）が触るキー。
    for s in snaps {
        for key in [
            "week",
            "taken_at",
            "week_start",
            "totals",
            "week_totals",
            "week_partial",
            "stale",
            "zoom_days",
        ] {
            assert!(!s[key].is_null(), "{key} が無い: {s}");
        }
    }
    assert_eq!(snaps[0]["week_start"], "2026-08-24");
    assert_eq!(snaps[0]["taken_at"], "2026-08-28");
    // 週が終わった行は「確定」、まだ途中の行は「集計中」。
    assert_eq!(snaps[0]["zoom_partial"], serde_json::Value::Bool(false));
    assert_eq!(snaps[1]["zoom_partial"], serde_json::Value::Bool(true));

    // 🔴 件数を直書きしない。架電シートをその週で足したものと突き合わせる。
    let sheets = fixture_sheets();
    let monday = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();
    let week: Vec<String> = (0..7)
        .map(|i| {
            (monday + chrono::Duration::days(i))
                .format("%Y-%m-%d")
                .to_string()
        })
        .collect();
    let want = sum_col(&sheets.kaden, "架電数", |r| {
        week.iter().any(|d| d == sheets.kaden.get(r, "日付"))
    });
    assert!(want > 0, "架電の fixture にその週の行が無い");
    assert_eq!(
        snaps[1]["zoom_called"].as_i64().unwrap(),
        want,
        "週次の架電数が架電シートの合計と合わない。\
         6枚を取り直したなら make_weekly_fixture.py も流し直すこと"
    );
}

/// その週に予定された商談の列（`週_`）が、当月ベースの列とは別の窓で数えられていること。
///
/// 当月ベースだけを週次表に並べると、月初の行で 1,064 → 537 と半減して見える
/// （8月と9月を比べているだけで、減ってはいない）。週ベースはそれが起きない。
#[test]
fn 週ベースの列は当月ベースと別の窓で数えている() {
    let body = payload();
    let snaps = body["snapshots"].as_array().unwrap();
    let w35 = &snaps[0];
    let w36 = &snaps[1];

    // 当月ベースは 8月（W35 の記録日）と 9月（W36 の記録日）で丸ごと入れ替わる
    assert_eq!(w35["totals"]["pool"].as_i64(), Some(1064));
    assert_eq!(w36["totals"]["pool"].as_i64(), Some(537));

    // 週ベースは同じ長さの窓なので、そこまで飛ばない
    for s in [w35, w36] {
        assert!(
            !s["week_totals"].is_null(),
            "週ベースの列が読めていない: {s}"
        );
        let wt = &s["week_totals"];
        let parts: i64 = ["実施", "未実施", "未処理", "これから", "要判定"]
            .iter()
            .map(|k| wt[*k].as_i64().unwrap_or(-1))
            .sum();
        assert_eq!(
            parts,
            wt["pool"].as_i64().unwrap(),
            "週ベースの仕分けの合計が母集団と合わない: {s}"
        );
    }

    // 🔴 件数を直書きしない。商談シートをその週で切ったものと突き合わせる。
    let sheets = fixture_sheets();
    for (s, monday) in [(w35, "2026-08-24"), (w36, "2026-08-31")] {
        let lo = format!("{monday} 00:00");
        let hi = {
            let d = NaiveDate::parse_from_str(monday, "%Y-%m-%d").unwrap()
                + chrono::Duration::days(7);
            format!("{} 00:00", d.format("%Y-%m-%d"))
        };
        let want = sheets
            .shodan
            .rows
            .iter()
            .filter(|r| {
                let v = sheets.shodan.get(r, "商談予定日時");
                lo.as_str() <= v && v < hi.as_str()
            })
            .count() as i64;
        assert!(want > 0, "商談の fixture に {monday} の週の行が無い");
        assert_eq!(
            s["week_totals"]["pool"].as_i64().unwrap(),
            want,
            "週ベースの母集団が商談シートのその週の件数と合わない（{monday} の週）。\
             fixture を取り直したなら make_weekly_fixture.py も流し直すこと"
        );
    }

    // 週が終わった行は「確定」、まだ途中の行は「集計中」
    assert_eq!(w35["week_partial"], serde_json::Value::Bool(false));
    assert_eq!(w36["week_partial"], serde_json::Value::Bool(true));
}

/// 「週_」列が無い古い行。2026-09-07 より前に書かれた行がこれになる。
/// 0 で埋めると画面が「その週は0件だった」と嘘をつくので、null にして「—」を出させる。
#[test]
fn 週ベースの列が無い古い行でも落ちない() {
    let text = "週\t記録日\t週はじまり\t母集団\t実施\t未実施\t未処理\tこれから\t要判定\t\
                取ったアポ\tCヨミ\tBPO母集団\t止まっている\tアンケート未回収\tCヨミ置きっぱなし\t\
                架電リスト手をつけた\t架電リスト母数\tZoom架電数\tZoom日数\tZoom集計中\n\
                2026-W37\t2026-09-07\t2026-09-07\t537\t166\t54\t8\t304\t5\t245\t126\t129\t\
                13\t280\t41\t32817\t129867\t\t0\t集計中\n";
    let snaps = super::snapshots_of(&sheet_from_tsv(text));
    assert_eq!(snaps.len(), 1);
    assert!(
        snaps[0]["week_totals"].is_null(),
        "週ベースの列が無い行を0件として出している: {}",
        snaps[0]
    );
    assert_eq!(snaps[0]["week_partial"], serde_json::Value::Bool(false));
    // 当月ベースの列は今まで通り読める
    assert_eq!(snaps[0]["totals"]["pool"].as_i64(), Some(537));
}

/// 架電がまだ1日も入っていない週の行。本番の KPI営業_週次 から取った実物
/// （2026-09-07 月曜、その週の初回。Zoom架電数の欄が空で書かれる）。
/// 空欄を 0 にすると画面が「0件」と嘘をつくので、null にして「—」を出させる。
#[test]
fn 架電がまだ無い週は0でなく空で返す() {
    let text = "週\t記録日\t週はじまり\t母集団\t実施\t未実施\t未処理\tこれから\t要判定\t\
                取ったアポ\tCヨミ\tBPO母集団\t止まっている\tアンケート未回収\tCヨミ置きっぱなし\t\
                架電リスト手をつけた\t架電リスト母数\tZoom架電数\tZoom日数\tZoom集計中\n\
                2026-W37\t2026-09-07\t2026-09-07\t537\t166\t54\t8\t304\t5\t245\t126\t129\t\
                13\t280\t41\t32817\t129867\t\t0\t集計中\n";
    let snaps = super::snapshots_of(&sheet_from_tsv(text));
    assert_eq!(snaps.len(), 1);
    assert!(
        snaps[0]["zoom_called"].is_null(),
        "架電が無い週を0件として出している: {}",
        snaps[0]
    );
    assert_eq!(snaps[0]["zoom_days"].as_i64(), Some(0));
    assert_eq!(snaps[0]["totals"]["pool"].as_i64(), Some(537));
    assert_eq!(snaps[0]["kaden_base"].as_i64(), Some(129867));
}

// ------------------------------------------------ 架電リストの担当者別
//
// 🔴 `KPI営業_架電リスト`（全社）と `KPI営業_架電リスト_担当別` は取った日が違う
// （全社は 2026-09-05、担当別は 2026-09-07 に足したので 09-07）。**両者の合計が
// ぴったり一致することは期待しない。** ここで見るのは、画面が絞り込んだときに
// 数字が破綻しないこと ―― 内訳が合計を超えない・チームと個人で食い違わない ―― の方。

/// 担当者別シートの分類ごとの合計。
fn by_owner_sum(body: &Value, scope: &str, class: &str) -> i64 {
    body["kaden"][scope]
        .as_object()
        .unwrap_or_else(|| panic!("kaden.{scope} が無い"))
        .values()
        .map(|c| c.get(class).and_then(Value::as_i64).unwrap_or(0))
        .sum()
}

#[test]
fn 架電リストの担当者別はチームと個人で食い違わない() {
    let body = payload();
    // どちらも担当なしを含まないので、そのまま一致するはず
    for class in ["未架電", "未接触", "接触済み", "対象外", "base"] {
        assert_eq!(
            by_owner_sum(&body, "by_person", class),
            by_owner_sum(&body, "by_team", class),
            "{class} がチームと個人で食い違う"
        );
    }
}

#[test]
fn 架電リストの担当者別がリスト全体を超えない() {
    let body = payload();
    // 🔴 比べる相手は `all.base`（アポ前リスト全体）。`base` は 2026-09-08 から
    //    「営業チームの合計」になったので、担当者別の合計より小さくて当たり前。
    let base = body["kaden"]["all"]["base"].as_i64().unwrap();
    let counted = body["kaden"]["counted_base"].as_i64().unwrap();
    let no_owner = body["kaden"]["no_owner"]["base"].as_i64().unwrap_or(0);
    assert_eq!(
        counted,
        by_owner_sum(&body, "by_person", "base") + no_owner,
        "counted_base が「個人別の合計＋担当なし」と合わない"
    );
    assert!(counted > 0, "担当者別が空");
    assert!(
        counted <= base,
        "担当者別の合計 {counted} がリスト全体の母数 {base} を超えている"
    );
    // どのチームもリスト全体を超えない
    for (team, counts) in body["kaden"]["by_team"].as_object().unwrap() {
        let n = counts["base"].as_i64().unwrap_or(0);
        assert!(n <= base, "{team} の {n} がリスト全体の {base} を超えている");
    }
}

// ------------------------------------------------ 営業チームと未配布の切り分け
//
// 🔴 2026-09-08 ユーザー判断。アポ前リスト全体で数えると「74.5%が未着手」に
// なるが、それは名簿に載っていない人が持っている在庫（永田さん 70,176件 ほか）に
// 引きずられた数字だった。営業チームだけで数えると 72.8% が着手済み。
// 現場が見たいのは後者で、在庫は別枠で配布の判断に使う。

/// 「すべて」で出す架電リストは、営業チームの合計であってリスト全体ではない。
#[test]
fn 全社の架電リストは営業チームの合計になる() {
    let body = payload();
    let k = &body["kaden"];
    let sales = by_owner_sum(&body, "by_team", "base")
        - k["by_team"]["チーム未設定"]["base"].as_i64().unwrap_or(0);
    assert_eq!(
        k["base"].as_i64().unwrap(),
        sales,
        "「すべて」の母数が営業チームの合計になっていない"
    );
    for class in ["未架電", "未接触", "接触済み"] {
        let want: i64 = k["by_team"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(t, _)| t.as_str() != "チーム未設定")
            .map(|(_, c)| c[class].as_i64().unwrap_or(0))
            .sum();
        assert_eq!(k["cls"][class].as_i64().unwrap_or(0), want, "{class} が合わない");
    }
    // リスト全体は別のキーに残っていること（注記と母数の推移がこちらを使う）
    assert!(k["base"].as_i64().unwrap() < k["all"]["base"].as_i64().unwrap());
}

/// 営業チームぶんと未配布ぶんを足すと、数えられた合計に戻る。
/// どこかで取りこぼすと、画面から静かに件数が消える。
#[test]
fn 営業チームと未配布を足すと元に戻る() {
    let body = payload();
    let k = &body["kaden"];
    let (sales, un) = (
        k["base"].as_i64().unwrap(),
        k["unassigned"]["base"].as_i64().unwrap(),
    );
    let counted = k["counted_base"].as_i64().unwrap();
    assert_eq!(
        sales + un,
        counted,
        "営業チーム {sales} ＋ 未配布 {un} が、数えられた合計 {counted} に戻らない"
    );
    assert!(un > 0, "未配布が空。fixture に名簿外の担当者が居ない");
    // 担当者が入っていない分は未配布側に入れる（どのチームにも属さないため）
    assert_eq!(
        k["unassigned"]["no_owner"].as_i64().unwrap(),
        k["no_owner"]["base"].as_i64().unwrap()
    );
}

/// 未配布は「誰が持っているか」まで出す。配る判断に使うため。
#[test]
fn 未配布は誰が持っているかまで出す() {
    let body = payload();
    let people = body["kaden"]["unassigned"]["people"].as_array().unwrap();
    assert!(!people.is_empty(), "未配布の内訳が空");
    let bases: Vec<i64> = people.iter().map(|p| p["base"].as_i64().unwrap()).collect();
    assert!(bases.windows(2).all(|w| w[0] >= w[1]), "件数の多い順でない");
    for p in people {
        assert_eq!(
            p["team"], "チーム未設定",
            "営業チームの人が未配布に混ざっている: {p}"
        );
        assert!(
            !p["name"].as_str().unwrap().starts_with("owner_"),
            "名前が引けていない: {p}"
        );
    }
    // 上位1名だけで未配布の半分を超える（2026-09-07 実測: 70,176 / 86,168）。
    // この偏りこそが「配る判断に使う」材料なので、消えていないことを見る。
    let total = body["kaden"]["unassigned"]["base"].as_i64().unwrap();
    assert!(
        bases[0] * 2 > total,
        "上位1名の偏りが出ていない: {}/{total}",
        bases[0]
    );
}

/// 担当者別シートが無い環境では、これまでどおりリスト全体を出す（分けようがない）。
#[test]
fn 担当者別シートが無ければリスト全体を出す() {
    let body = build_payload(
        &Sheets {
            kaden_by_owner: super::empty_sheet(),
            ..fixture_sheets()
        },
        fixture_day(),
    );
    let k = &body["kaden"];
    assert_eq!(k["has_by_owner"], Value::Bool(false));
    assert_eq!(k["base"].as_i64(), k["all"]["base"].as_i64());
    assert_eq!(k["unassigned"]["base"].as_i64(), Some(0));
}

/// 担当者が入っていない取引は、どのチームにも個人にも混ぜない。
/// 混ぜると「チームの合計＝全社」に見えてしまい、7千件の持ち主不明が隠れる。
#[test]
fn 担当なしはチームにも個人にも入れない() {
    let body = payload();
    let no_owner = body["kaden"]["no_owner"]["base"].as_i64().unwrap_or(0);
    assert!(no_owner > 0, "fixture に担当なしの行が無い");
    assert!(
        body["kaden"]["by_person"].get("").is_none(),
        "空の ownerId が個人別に入っている"
    );
    for team in body["kaden"]["by_team"].as_object().unwrap().keys() {
        assert_ne!(team, "", "空のチーム名がある");
    }
}

/// 架電リストにしか出てこない担当者（商談が1件も無い BPO など）も個人プルダウンに載せる。
/// 載せないと「チームの合計は出るのに、中の誰も選べない」ことが起きる。
#[test]
fn 架電リストにしかいない担当者も個人で選べる() {
    let body = payload();
    let listed: std::collections::HashSet<&str> = body["people"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["id"].as_str())
        .collect();
    let owners: Vec<&str> = body["kaden"]["by_person"]
        .as_object()
        .unwrap()
        .keys()
        .map(|s| s.as_str())
        .collect();
    assert!(!owners.is_empty(), "fixture に担当者別の行が無い");
    let missing: Vec<&&str> = owners.iter().filter(|o| !listed.contains(**o)).collect();
    assert!(
        missing.is_empty(),
        "架電リストに居るのに個人で選べない担当者: {missing:?}"
    );
    // 商談にしか出てこない人も消えていないこと
    assert!(
        listed.len() >= owners.len(),
        "個人プルダウンが架電リストの担当者だけになっている"
    );
}

/// 画面に数字が出る担当者は、全員が名前を持っていること。
/// 🔴 `owner_96437217` のような ID がそのまま出ていた（2026-09-07 現場指摘）。
/// 直したあとも Zoom の架電表だけ別の一覧を見ていて ID が残っていた。
#[test]
fn 画面に出る担当者はすべて名前が引ける() {
    let body = payload();
    let listed: std::collections::HashMap<&str, &str> = body["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    for (p, name) in &listed {
        assert!(
            !name.starts_with("owner_"),
            "{p} の名前が ID のまま: {name}"
        );
    }
    // Zoom の架電で人別に数字が出る担当者が、全員この一覧に載っていること
    for period in ["today", "this_week", "prev_week", "this_month"] {
        for owner in body["calls"]["periods"][period]["by_person"]
            .as_object()
            .unwrap()
            .keys()
        {
            assert!(
                listed.contains_key(owner.as_str()),
                "{period} の架電表に出る {owner} が people に無い＝画面で owner_ 表示になる"
            );
        }
    }
}

/// 担当者別シートが無くても（足す前の環境）画面は出る。
#[test]
fn 架電リストの担当者別が無くても画面は出る() {
    let body = build_payload(
        &Sheets {
            kaden_by_owner: super::empty_sheet(),
            ..fixture_sheets()
        },
        fixture_day(),
    );
    assert_eq!(body["kaden"]["has_by_owner"], Value::Bool(false));
    assert_eq!(body["kaden"]["by_person"].as_object().unwrap().len(), 0);
    // リスト全体の数字は変わらない（営業チームぶんは分けようがないので出せない）
    assert_eq!(
        body["kaden"]["all"]["base"].as_i64(),
        payload()["kaden"]["all"]["base"].as_i64()
    );
}

// ------------------------------------------------ 母数の動き
//
// 🔴 架電リストの母数は毎月大きく動く。異常ではなくリストマネジメントの正常な運用
// （2026-09-07 ユーザー確認）。実測では 09-01 に BPO→アポ前 7,360件、09-02 に
// アポ前→BPO 6,663件が動き、母数は 136,518 → 129,790 になった。
// 率だけ見て「進んだ／戻った」と読まれないよう、動いた事実を画面に出す。

/// 週次シートを1行ぶん作る。母数以外は画面が触らないので固定でよい。
fn weekly_row(week: &str, taken_at: &str, week_start: &str, kaden_base: i64) -> String {
    format!(
        "{week}\t{taken_at}\t{week_start}\t537\t166\t54\t8\t304\t5\t245\t126\t129\t\
         13\t280\t41\t32817\t{kaden_base}\t1000\t5\t確定\n"
    )
}

const WEEKLY_HEAD: &str = "週\t記録日\t週はじまり\t母集団\t実施\t未実施\t未処理\tこれから\t要判定\t\
     取ったアポ\tCヨミ\tBPO母集団\t止まっている\tアンケート未回収\tCヨミ置きっぱなし\t\
     架電リスト手をつけた\t架電リスト母数\tZoom架電数\tZoom日数\tZoom集計中\n";

/// 週次シートを差し替えて payload を作る。fixture_day() は 2026-09-04（金）、
/// その週のはじまりは 2026-08-31。
fn payload_with_weekly(rows: &str) -> Value {
    build_payload(
        &Sheets {
            weekly: Arc::new(sheet_from_tsv(&format!("{WEEKLY_HEAD}{rows}"))),
            ..fixture_sheets()
        },
        fixture_day(),
    )
}

#[test]
fn 母数が動いたことを前の週の記録と比べて出す() {
    let body = payload_with_weekly(&format!(
        "{}{}",
        weekly_row("2026-W35", "2026-08-28", "2026-08-24", 136_518),
        // 今週（2026-08-31 はじまり）の行。これ自身とは比べない
        weekly_row("2026-W36", "2026-09-04", "2026-08-31", 999_999),
    ));
    let t = &body["kaden"]["base_trend"];
    // 🔴 比べるのは `all.base`（アポ前リスト全体）。週次シートに残っているのが
    //    その数え方なので、営業チームの合計（`base`）と比べると桁が合わない。
    let base = body["kaden"]["all"]["base"].as_i64().unwrap();
    assert_eq!(t["week"], "2026-W35", "今週の行と比べてしまっている");
    assert_eq!(t["base"].as_i64(), Some(136_518));
    assert_eq!(
        t["diff"].as_i64(),
        Some(base - 136_518),
        "差が「リスト全体の今の母数 − 前の記録」になっていない"
    );
    assert!(t["diff"].as_i64().unwrap() < 0, "減っているのに増えて見える");
}

#[test]
fn 前の週の記録が無ければ母数の比較を出さない() {
    // 今週の行しか無い（本番の 2026-09-07 がこの状態だった）
    let body = payload_with_weekly(&weekly_row("2026-W36", "2026-09-04", "2026-08-31", 129_869));
    assert_eq!(
        body["kaden"]["base_trend"],
        Value::Null,
        "比べる相手が無いのに前週比を出している"
    );
    // 週次シートが丸ごと無くても落ちない
    let body = build_payload(&fixture_sheets_without_weekly(), fixture_day());
    assert_eq!(body["kaden"]["base_trend"], Value::Null);
}

#[test]
fn 母数の比較は今週でない一番新しい記録を使う() {
    // 週が飛んでいても、今週でない最新の記録と比べる
    let body = payload_with_weekly(&format!(
        "{}{}{}",
        weekly_row("2026-W30", "2026-07-24", "2026-07-20", 100_000),
        weekly_row("2026-W34", "2026-08-21", "2026-08-17", 136_518),
        weekly_row("2026-W36", "2026-09-04", "2026-08-31", 999_999),
    ));
    assert_eq!(body["kaden"]["base_trend"]["week"], "2026-W34");
    // 母数が空の行（架電がまだ入っていない週）は比較相手にしない
    let body = payload_with_weekly(&format!(
        "{}{}",
        weekly_row("2026-W34", "2026-08-21", "2026-08-17", 136_518),
        weekly_row("2026-W35", "2026-08-28", "2026-08-24", 0),
    ));
    assert_eq!(body["kaden"]["base_trend"]["week"], "2026-W34");
}

// ------------------------------------------------ 商談の集計から外す
//
// 🔴 2026-09-08 ユーザー判断。HubSpotチームが「コンサル営業」の人だけを
// **商談の集計から**外す。BPO と 新規営業（名簿に無い人）は残す。
// 一律に「営業5チームだけ」にはしない。理由は落ちる中身の性質が違うため:
//   コンサル営業  営業KPIの対象ではない          → 落とす
//   BPO_リクロジ  画面に「内BPO」表示があり不整合 → 残す
//   新規営業      今月378架電・商談75件の現役     → 残す
// 🔴 外すのは商談だけ。架電と架電リストは外さない（コンサル営業も架電している）。

/// 誰を外すかは `KPI営業_メンバー` の `集計対象` 列にしか無い。
/// テストでもチーム名を書かず、シートから「対象外」の人を引いて期待値にする。
fn excluded_owners(sheets: &Sheets) -> std::collections::HashSet<String> {
    super::members_of(&sheets.member)
        .into_iter()
        .filter(|(_, p)| !p.counted)
        .map(|(id, _)| id)
        .collect()
}

/// シートの ownerId 列を数える（除外対象の人が何件持っているか）。
fn count_owned(sheet: &SheetData, owners: &std::collections::HashSet<String>) -> i64 {
    sheet
        .rows
        .iter()
        .filter(|r| owners.contains(sheet.get(r, "ownerId")))
        .count() as i64
}

#[test]
fn 集計対象外の担当者ぶんだけ商談が減る() {
    let sheets = fixture_sheets();
    let out = excluded_owners(&sheets);
    assert!(!out.is_empty(), "fixture に集計対象外の担当者が居ない");

    // 除外を無かったことにした版と比べる。差＝除外で落ちた件数のはず。
    let all_in = build_payload(&fixture_sheets_counting_everyone(), fixture_day());
    let body = payload();

    for (key, sheet) in [
        ("apo", &sheets.apo),
        ("cyomi", &sheets.cyomi),
    ] {
        let want = count_owned(sheet, &out);
        assert_eq!(
            team_sum(&all_in, key) - team_sum(&body, key),
            want,
            "{key} の減り方が、集計対象外の担当者が持っている件数と合わない"
        );
    }
    // 当月の母集団も同じ関係
    assert!(team_sum(&all_in, "pool") >= team_sum(&body, "pool"));
    // 落とした件数を画面に出していること（黙って減らさない）
    let dropped = body["excluded"]["件数"].as_i64().unwrap_or(0);
    assert!(dropped > 0, "落とした件数が出ていない");
    // 内訳は HubSpotチーム 別。テスト側でもチーム名は直書きしない
    let by_team: i64 = body["excluded"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.as_str() != "件数")
        .map(|(_, v)| v.as_i64().unwrap_or(0))
        .sum();
    assert_eq!(dropped, by_team, "件数と内訳の合計が合わない");
}

/// 除外しても「内BPO」は壊れない。BPO の人は外さないので数は変わらないはず。
#[test]
fn 除外しても内bpoの表示は変わらない() {
    let all_in = build_payload(&fixture_sheets_counting_everyone(), fixture_day());
    let body = payload();
    for key in ["pool", "実施"] {
        assert_eq!(
            body["bpo_total"][key].as_i64(),
            all_in["bpo_total"][key].as_i64(),
            "内BPO の {key} が除外で変わっている＝BPO を巻き込んでいる"
        );
    }
    assert!(body["bpo_total"]["pool"].as_i64().unwrap_or(0) > 0);
}

/// 商談から外した人でも、架電と架電リストからは外さない。
#[test]
fn 集計対象外でも架電と架電リストには残る() {
    let sheets = fixture_sheets();
    let out = excluded_owners(&sheets);
    let body = payload();
    let in_kaden = body["kaden"]["by_person"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|o| out.contains(o.as_str()))
        .count();
    assert!(
        in_kaden > 0,
        "集計対象外の担当者が架電リストからも消えている（外すのは商談だけ）"
    );
}

/// `集計対象` 列が無い古いシートでは、これまでどおり全員を数える。
#[test]
fn 集計対象の列が無ければ全員数える() {
    let body = build_payload(&fixture_sheets_counting_everyone(), fixture_day());
    assert_eq!(body["excluded"]["件数"].as_i64().unwrap_or(0), 0);
    // Python 版の実測（除外を入れる前の値）に戻ること
    assert_eq!(team_sum(&body, "pool"), 537);
    assert_eq!(team_sum(&body, "apo"), 245);
    assert_eq!(team_sum(&body, "cyomi"), 127);
}

// ------------------------------------------------ メンバー

/// 名簿に載っていない人（BPO など）も名前で出す。
/// 画面に `owner_62991116` と出ていたのを直したぶん（2026-09-07 現場指摘）。
#[test]
fn 名簿にない担当者もhubspotの氏名で出す() {
    let sheets = fixture_sheets();
    let members = super::members_of(&sheets.member);
    // 現場が「名前が分からない」と言った3人。いずれも BPO で名簿に無い。
    for id in ["71368916", "62991116", "96437217"] {
        let p = members.get(id).unwrap_or_else(|| panic!("{id} が名簿に無い"));
        assert!(
            !p.name.starts_with("owner_") && !p.name.is_empty(),
            "{id} の氏名が入っていない: {}",
            p.name
        );
        // チームは名簿が正。名簿に無い人は「チーム未設定」のまま
        assert_eq!(p.team, "チーム未設定");
        // 代わりに HubSpot 側のチームを持たせて、誰なのかが分かるようにする
        assert!(!p.hs_team.is_empty(), "{id} の HubSpotチームが空");
    }
    // 名簿に載っている人は名簿のチームが勝つ（HubSpot は「新規営業」までしか無い）
    let itsubo = members.get("613211320").expect("伊壺さんが名簿に無い");
    assert_eq!(itsubo.team, "伊壺チーム");
    assert_eq!(itsubo.hs_team, "新規営業");
}

#[test]
fn 名簿にもhubspotにも無いownerはidのまま出す() {
    let members = std::collections::HashMap::new();
    assert_eq!(super::person_of(&members, "999").name, "owner_999");
    assert_eq!(super::person_of(&members, "999").team, "チーム未設定");
    assert_eq!(super::person_of(&members, "").name, "担当なし");
}

#[test]
fn 週次シートがまだ無くても画面は出る() {
    // 初回は Python がまだ1度も書いていないのでシート自体が存在しない。
    // ここで落とすと画面ごと出なくなる。
    let body = build_payload(&fixture_sheets_without_weekly(), fixture_day());
    assert_eq!(body["snapshots"].as_array().unwrap().len(), 0);
    // 週次が無いだけで、他の数字は変わらない
    assert_eq!(team_sum(&body, "pool"), 537);
}

// ---------------------------------------------------------------- 架電の週

/// 🔴 2026-09-07（月）の本番で見つかった不具合の再現。
/// 架電の週頭をシートの最終日から求めていたため、月曜の朝は最終日が
/// 金曜（＝先週）になり、「今週」として 8/31〜9/06 が丸ごと出ていた。
/// fixture の架電は 8/25〜9/04 なので、今日を 9/07（月）にすると
/// 今週（9/07〜）には1日も無い。0件と正直に出るのが正しい。
#[test]
fn 月曜に開いても先週を今週として出さない() {
    let monday = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
    let body = build_payload(&fixture_sheets(), monday);
    let periods = &body["calls"]["periods"];

    let this_week: Vec<&str> = periods["this_week"]["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert!(
        this_week.is_empty(),
        "今週に架電の行は無いはずなのに入っている: {this_week:?}"
    );
    assert_eq!(
        periods["this_week"]["total"]["connected"].as_i64().unwrap_or(0),
        0,
        "先週の架電を今週として数えている"
    );

    // 先週は 8/31〜9/06。fixture のある日（8/31〜9/04）が入る。
    let prev: Vec<&str> = periods["prev_week"]["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert_eq!(prev.first(), Some(&"2026-08-31"));
    assert_eq!(prev.last(), Some(&"2026-09-06"));
    assert!(
        periods["prev_week"]["total"]["connected"].as_i64().unwrap_or(0) > 0,
        "先週の架電が0になっている"
    );

    // 今日（9/07）の行はまだ無い。空で返す。
    assert_eq!(periods["today"]["days"].as_array().unwrap().len(), 1);
    assert_eq!(periods["today"]["days"][0], "2026-09-07");
    assert_eq!(periods["today"]["total"]["connected"].as_i64().unwrap_or(0), 0);
    // 今週ぶんが無いので、比較相手の「先週の同じところまで」も空
    assert!(periods["prev_week_same"]["days"].as_array().unwrap().is_empty());
}

/// 「先週の同じところまで」は曜日をそろえる（頭から件数ぶん取らない）。
#[test]
fn 先週の比較は曜日をそろえる() {
    let body = payload(); // 2026-09-04（金）。今週は 8/31〜9/04 の5日
    let periods = &body["calls"]["periods"];
    let this_week: Vec<&str> = periods["this_week"]["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    let prev_same: Vec<&str> = periods["prev_week_same"]["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    assert_eq!(this_week, ["2026-08-31", "2026-09-01", "2026-09-02", "2026-09-03", "2026-09-04"]);
    assert_eq!(prev_same, ["2026-08-24", "2026-08-25", "2026-08-26", "2026-08-27", "2026-08-28"]);
}

/// 架電の最終日がまだ途中かどうかを、取得条件から拾って画面に渡す。
#[test]
fn 架電の最終日が途中かどうかを取得条件から渡す() {
    // 🔴 日付を直書きしない。fixture を取り直すと架電の最終日が動くのに
    //    `fixture_day()` は 9/04 で固定なので、両者が一致する保証は無い。
    //    シートから引いた最終日を期待値にする。
    let sheets = fixture_sheets();
    let last = sheets
        .kaden
        .rows
        .iter()
        .map(|r| sheets.kaden.get(r, "日付").to_string())
        .max()
        .expect("架電の fixture が空");
    let today = fixture_day().format("%Y-%m-%d").to_string();

    let body = payload();
    assert_eq!(body["calls"]["last_day"], last);
    // fixture の取得条件には「架電の最終日」がまだ無い。
    // その場合は「最終日＝今日なら途中」に落とす。
    assert_eq!(
        body["calls"]["last_day_partial"],
        serde_json::Value::Bool(last == today)
    );

    let with_meta = |text: String| {
        build_payload(
            &Sheets {
                meta: Arc::new(sheet_from_tsv(&text)),
                ..fixture_sheets()
            },
            fixture_day(),
        )
    };
    // 取得条件が同じ日について言っていれば、そちらが勝つ
    for (says, want) in [("はい", true), ("いいえ", false)] {
        let body = with_meta(format!(
            "項目\t値\n架電の最終日\t{last}\n架電の最終日は途中\t{says}\n"
        ));
        assert_eq!(
            body["calls"]["last_day_partial"],
            serde_json::Value::Bool(want),
            "取得条件が「{says}」なのに従っていない"
        );
    }
    // 別の日について言っているなら使わない（最終日＝今日かどうかに落とす）
    let body = with_meta("項目\t値\n架電の最終日\t1999-01-01\n架電の最終日は途中\tはい\n".into());
    assert_eq!(
        body["calls"]["last_day_partial"],
        serde_json::Value::Bool(last == today)
    );
}

#[test]
fn 止まっている取引はすべて予定日を過ぎている() {
    let body = payload();
    let cutoff = fixture_day().format("%Y-%m-%d").to_string();
    for row in body["stale"].as_array().unwrap() {
        let date = row["date"].as_str().unwrap_or("");
        assert!(
            date < cutoff.as_str(),
            "予定日が未来なのに止まっている扱い: {row}"
        );
    }
}
