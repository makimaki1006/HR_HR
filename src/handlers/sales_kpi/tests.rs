//! 営業KPI: 実データでの突き合わせ
//!
//! `tests/fixtures/sales_kpi/` にある TSV は、本番のスプレッドシートから
//! そのまま落としたもの（取引名＝顧客の会社名だけ落としてある）。
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
fn fixture_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()
}

fn load_tsv(name: &str) -> Arc<SheetData> {
    let path = format!(
        "{}/tests/fixtures/sales_kpi/{name}.tsv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("テストデータが読めません {path}: {e}"));
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
    Arc::new(SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    })
}

fn fixture_sheets() -> Sheets {
    Sheets {
        shodan: load_tsv("KPI営業_商談"),
        apo: load_tsv("KPI営業_アポ"),
        cyomi: load_tsv("KPI営業_Cヨミ"),
        kaden: load_tsv("KPI営業_架電日次"),
        kaden_list: load_tsv("KPI営業_架電リスト"),
        member: load_tsv("KPI営業_メンバー"),
        meta: load_tsv("KPI営業_取得条件"),
        weekly: load_tsv("KPI営業_週次"),
        all_cached: true,
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
    assert_eq!(team_sum(&body, "cyomi"), 127);
    assert_eq!(team_sum(&body, "cyomi_stale"), 39);
    assert_eq!(body["cyomi_stale"].as_array().unwrap().len(), 39);
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
#[test]
fn 週次の行が画面側の集計と一致する() {
    let body = payload();
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
        for key in ["week", "taken_at", "week_start", "totals", "stale", "zoom_days"] {
            assert!(!s[key].is_null(), "{key} が無い: {s}");
        }
    }
    assert_eq!(snaps[0]["week_start"], "2026-08-24");
    assert_eq!(snaps[0]["taken_at"], "2026-08-28");
    // 週が終わった行は「確定」、まだ途中の行は「集計中」。
    assert_eq!(snaps[0]["zoom_partial"], serde_json::Value::Bool(false));
    assert_eq!(snaps[1]["zoom_partial"], serde_json::Value::Bool(true));
    assert_eq!(snaps[1]["zoom_called"].as_i64().unwrap(), 46137);
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
    let mut lines = text.lines();
    let header: Vec<String> = lines.next().unwrap().split('\t').map(str::to_string).collect();
    let rows: Vec<Vec<Arc<str>>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut cells: Vec<Arc<str>> = l.split('\t').map(Arc::from).collect();
            cells.resize(header.len(), Arc::from(""));
            cells
        })
        .collect();
    let sheet = SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    };
    let snaps = super::snapshots_of(&sheet);
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

#[test]
fn 週次シートがまだ無くても画面は出る() {
    // 初回は Python がまだ1度も書いていないのでシート自体が存在しない。
    // ここで落とすと画面ごと出なくなる。
    let body = build_payload(&fixture_sheets_without_weekly(), fixture_day());
    assert_eq!(body["snapshots"].as_array().unwrap().len(), 0);
    // 週次が無いだけで、他の数字は変わらない
    assert_eq!(team_sum(&body, "pool"), 537);
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
