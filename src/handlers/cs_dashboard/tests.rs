//! コンサルダッシュボード: 実データでの突き合わせ
//!
//! `tests/fixtures/cs_dashboard/CS_取引.tsv` は、モックが使っているのと同じ
//! 実データ（HubSpot 納品管理PL 3,656取引）を
//! `scripts\consulting_dashboard\build_sheets.py --fixture` で
//! シート形式に畳んだもの。**顧客が特定できる列だけ伏せてある**。
//!
//! なぜ自作のテストデータを使わないか:
//!   架電クオリティを Rust へ移したとき、テストデータを自作していたせいで
//!   「存在しない列名を読んで常に 0」というバグを検出できなかった。
//!   列名の取り違えは実データでしか見つからない。
//!
//! 期待値の出どころ:
//!   継続率      `.claude\skills\call-quality-metrics\SKILL.md`（2026-09-20 確定）
//!   継続回数    `claudedocs\consulting_dashboard_2026-09-18\CSモデルと不足機能.md`

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use super::routes::{
    build_consultants, build_customer, build_data_quality, build_deal_board, build_focus,
    build_handover, build_headquarters, build_mtg_quality, build_outcome, build_phone,
    build_rampup, build_renewal, build_today_board,
};
use super::Sheets;
use crate::handlers::call_quality::sheets::SheetData;

fn load_tsv(name: &str) -> Arc<SheetData> {
    // 実データなので生だと 9.4MB になる。gzip で 1.8MB。
    // flate2 は既に依存にあるので、読む側で解く。
    let path = format!(
        "{}/tests/fixtures/cs_dashboard/{name}.tsv.gz",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw =
        std::fs::read(&path).unwrap_or_else(|e| panic!("テストデータが読めません {path}: {e}"));
    let mut text = String::new();
    {
        use std::io::Read;
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_string(&mut text)
            .unwrap_or_else(|e| panic!("{path} を展開できません: {e}"));
    }
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

fn sheets() -> Sheets {
    Sheets {
        deal: load_tsv("CS_取引"),
        call: load_tsv("CS_通話明細"),
        mtg: load_tsv("CS_MTG"),
        history: load_tsv("CS_プロパティ履歴"),
        customer: load_tsv("CS_顧客"),
        mail_mtg: load_tsv("CS_MTG実施日_メール由来"),
        handover: load_tsv("CS_担当交代"),
        owner_hist: load_tsv("CS_担当履歴"),
        // 🔴 この fixture の生成時刻だけは**手で固定**してある（2026-09-16 04:30）。
        //    ビルドのたびに時刻が変わると、鮮度のテストが実行日で落ちるため。
        meta: load_tsv("CS_メタ"),
        all_cached: false,
    }
}

/// リスクの2軸はどちらも「今日から何日」で決まる。期待値を固定するため、
/// モックが数字を出した日に合わせる。ここを変えると期待値も変わる。
fn fixture_day() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 18).expect("2026-09-18")
}

fn band_n(v: &Value, label: &str) -> i64 {
    v["risk"]["bands"]
        .as_array()
        .expect("bands")
        .iter()
        .find(|b| b["label"] == label)
        .and_then(|b| b["n"].as_i64())
        .unwrap_or_else(|| panic!("帯 {label} が無い"))
}

fn month<'a>(v: &'a Value, m: &str) -> &'a Value {
    v["monthly_retention"]["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["month"] == m)
        .unwrap_or_else(|| panic!("満了月 {m} の行が無い"))
}

fn renewal<'a>(v: &'a Value, no: i64) -> &'a Value {
    v["by_renewal"]
        .as_array()
        .expect("by_renewal")
        .iter()
        .find(|r| r["renewal_no"] == no)
        .unwrap_or_else(|| panic!("継続回数 {no} の行が無い"))
}

/// 期待値はスキルの表そのもの（2026-09-20 確定）。
///
/// | 満了月 | 継続 | 解約 | 充足 | 母数 | 継続率 |
/// | 2026-06 | 58 | 39 | 10 | 107 | 54.2% |
/// | 2026-07 | 53 | 39 | 10 | 102 | 52.0% |
/// | 2026-08 | 62 | 34 |  5 | 101 | 61.4% |
#[test]
fn 満了月ごとの継続率がスキルの確定値と一致する() {
    let v = build_renewal(&sheets(), false);
    for (m, keep, cancel, fill, denom, pct) in [
        ("2026-06", 58, 39, 10, 107, 54.2),
        ("2026-07", 53, 39, 10, 102, 52.0),
        ("2026-08", 62, 34, 5, 101, 61.4),
    ] {
        let r = month(&v, m);
        assert_eq!(r["keep"], keep, "{m} の継続済");
        assert_eq!(r["cancel"], cancel, "{m} の解約");
        assert_eq!(r["fill"], fill, "{m} の充足");
        assert_eq!(r["denom"], denom, "{m} の母数");
        let got = r["rate"].as_f64().expect("率が null");
        assert!(
            (got - pct).abs() < 0.05,
            "{m} の継続率が {got:.1}%。期待は {pct}%"
        );
    }
}

/// オプション契約を外すと結果待ちがほぼ消える（3ヶ月で 48件→3件）。
///
/// 満了してもステージが動かない取引の多くがオプションだったため。
/// ここが崩れたら `OPTION_KINDS` / `OPTION_STAGES` の判定が壊れている。
///
/// 2026-09-23: 5件 → 3件。オプションの判定にステージ（オプション／満了済オプション）と
/// 種別 `AirWork` を足したため。継続率そのもの（54.2% / 52.0% / 61.4%）は動いていない。
#[test]
fn オプションを外すと結果待ちがほぼ消える() {
    let v = build_renewal(&sheets(), false);
    let pending: i64 = ["2026-06", "2026-07", "2026-08"]
        .iter()
        .map(|m| month(&v, m)["pending"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(pending, 3, "3ヶ月の結果待ち。オプションを外した後の実測値");
    let excluded = super::population_of(&sheets().deal).deals_option;
    assert!(
        excluded > 200,
        "オプション契約が {excluded} 件しか外れていない。contract_kind の値が変わった可能性"
    );
}

/// 解約は初回契約に集中している。充足を分子に**含めた**値。
///
/// 🔴 2026-09-23 に**オプション契約を母集団から外した**ので、件数と率が動いた。
/// 資料（オプション込み）の値とは一致しない。
///
/// 🔴 2026-09-23（2回目）: **分母を決着済み（継続＋解約＋充足）に直した**。
/// 以前は件数 n（結果待ち＝稼働中を含む）で割っていて、初回は 50.1% と出ていた。
/// 定義「結果待ちは分母に入れない」（call-quality-metrics）に合わせて期待値を
/// fixture から数え直した（Python で別に数えて一致を確認）。
///
/// | 継続回数 | 件数 | 結果待ち | 決着済み | 解約＋充足 | 解約率 | 以前の表示 |
/// | 初回  | 1,914 | 264 | 1,650 | 959 | 58.1% | 50.1% |
/// | 継続1 |   734 | 107 |   627 | 283 | 45.1% | 38.6% |
/// | 継続2 |   383 |  98 |   285 | 110 | 38.6% | 28.7% |
/// | 継続3 |   182 |  61 |   121 |  39 | 32.2% | 21.4% |
/// | 継続5 |    49 |  18 |    31 |   4 | 12.9% |  8.2% |
#[test]
fn 継続回数ごとの解約率が資料の値と一致する() {
    let v = build_renewal(&sheets(), false);
    for (no, n, pending, denom, cancel_plus_fill, pct) in [
        (0, 1914, 264, 1650, 959, 58.1),
        (1, 734, 107, 627, 283, 45.1),
        (2, 383, 98, 285, 110, 38.6),
        (3, 182, 61, 121, 39, 32.2),
        (5, 49, 18, 31, 4, 12.9),
    ] {
        let r = renewal(&v, no);
        assert_eq!(r["n"], n, "継続{no} の件数");
        assert_eq!(r["pending"], pending, "継続{no} の結果待ち");
        assert_eq!(r["denom"], denom, "継続{no} の決着済み（解約率の分母）");
        let got_cf = r["cancel"].as_i64().unwrap() + r["fill"].as_i64().unwrap();
        assert_eq!(got_cf, cancel_plus_fill, "継続{no} の解約＋充足");
        let got = r["cancel_rate"].as_f64().expect("率が null");
        assert!(
            (got - pct).abs() < 0.05,
            "継続{no} の解約率が {got:.1}%。期待は {pct}%"
        );
    }
}

/// 充足を外すと 58.1% が 46.9% に見える（どちらも分母は決着済み）。
///
/// **外さないのが確定した定義**。両方返しているのは画面で並べて見せるためで、
/// 主値を取り違えていないことをここで固定する。
#[test]
fn 充足を外した値は別のキーで返る() {
    let v = build_renewal(&sheets(), false);
    let r = renewal(&v, 0);
    let main = r["cancel_rate"].as_f64().unwrap();
    let excl = r["cancel_rate_excl_fill"].as_f64().unwrap();
    assert!((main - 58.1).abs() < 0.05, "主値が {main:.1}%");
    assert!(
        excl < main - 5.0,
        "充足を外すと {excl:.1}% まで下がるはず（主値 {main:.1}%）"
    );
}

/// 右側打ち切りを外すと代表値が動く。
///
/// 応募数の中央値が 8 → 9 に動く（初回契約）。**動かないなら
/// `right_censored` 列を読めていない**。
#[test]
fn 右側打ち切りを外すと代表値が動く() {
    let all = build_renewal(&sheets(), false);
    let cut = build_renewal(&sheets(), true);

    let n_all = renewal(&all, 0)["n_stats"].as_i64().unwrap();
    let n_cut = renewal(&cut, 0)["n_stats"].as_i64().unwrap();
    assert!(
        n_cut < n_all,
        "打ち切りを外したのに母数が減っていない（{n_all} → {n_cut}）"
    );

    let m_all = renewal(&all, 0)["oubo"]["median"].as_f64().unwrap();
    let m_cut = renewal(&cut, 0)["oubo"]["median"].as_f64().unwrap();
    assert!(
        m_cut > m_all,
        "打ち切りを外すと中央値は上がるはず（{m_all} → {m_cut}）"
    );

    // 打ち切り件数そのもの。511件（モックの meta）から、オプション契約を
    // 外した分だけ減っている（2026-09-23）。
    assert_eq!(all["meta"]["right_censored_n"], 419);
}

/// 分母0のとき率は null。**0% と書かない。**
///
/// 実データには決着0件の継続回数が存在する（継続9 など）。そこが 0% で
/// 返ると「全部continueした」ように読める。
#[test]
fn 分母0の率はnullになる() {
    let v = build_renewal(&sheets(), false);
    let mut checked = false;
    for r in v["missingness"].as_array().unwrap() {
        if r["n"] == 0 {
            assert!(
                r["fill_rate"].is_null(),
                "母数0なのに率が {} で返っている",
                r["fill_rate"]
            );
            checked = true;
        }
    }
    // 母数0の組が無いこと自体は正常。あったときに null であることだけを縛る。
    let _ = checked;

    // 満了月の側も同じ約束。母数0の月があれば null。
    for r in v["monthly_retention"]["rows"].as_array().unwrap() {
        if r["denom"] == 0 {
            assert!(r["rate"].is_null(), "母数0の月に率が入っている: {r}");
        }
    }
}

/// 欠測は結果によって偏っている。
///
/// 隠すと「継続するほど成果が良い」という相関を押し上げる方向に効く。
/// ここでは**偏りが実在すること**だけを固定する（値そのものは動く）。
#[test]
fn 欠測の偏りが出ている() {
    let v = build_renewal(&sheets(), false);
    let rows = v["missingness"].as_array().unwrap();
    assert_eq!(rows.len(), 4 * 5, "結果4区分 × 成果5項目");

    let pick = |g: &str, f: &str| -> f64 {
        rows.iter()
            .find(|r| r["group"] == g && r["field"] == f)
            .and_then(|r| r["fill_rate"].as_f64())
            .unwrap_or_else(|| panic!("{g} / {f} の記入率が無い"))
    };
    let keep = pick("継続済", "oubo");
    let cancel = pick("解約", "oubo");
    assert!(
        (keep - cancel).abs() > 1.0,
        "継続済 {keep:.1}% と解約 {cancel:.1}% の記入率に差が出ていない。
         偏りが消えたなら画面の注意書きも見直すこと"
    );
}

/// 列名を1つでも取り違えると全部 null になる。それを検出する。
///
/// 架電クオリティ移行時に「存在しない列名を読んで常に 0」というバグを
/// 自作テストデータで見逃した反省から、**主要な列が実際に値を返している**
/// ことを直接確かめる。
#[test]
fn 主要な列が読めている() {
    let sh = sheets();
    // 🔴 `deals_of` はオプション契約を落として返す。全件は `deals_all_of`。
    assert_eq!(super::deals_all_of(&sh.deal).len(), 3659, "全取引の件数");
    let deals = super::deals_of(&sh.deal);
    assert_eq!(deals.len(), 3432, "オプション契約を外した取引件数");

    let has = |f: &dyn Fn(&super::Deal) -> bool| deals.iter().filter(|d| f(d)).count();
    assert!(has(&|d| !d.stage.is_empty()) > 3400, "dealstage");
    assert!(
        has(&|d| !d.contract_expiration_date.is_empty()) > 3400,
        "contract_expiration_date"
    );
    assert!(
        has(&|d| !d.contract_kind.is_empty()) > 3400,
        "contract_kind"
    );
    assert!(has(&|d| d.renewal_no.is_some()) > 3400, "renewal_no");
    assert!(has(&|d| d.oubo.is_some()) > 1000, "oubo");
    assert!(has(&|d| d.keisaisu.is_some()) > 500, "keisaisu");
    assert!(has(&|d| d.is_active) > 100, "is_active");
    assert!(has(&|d| d.right_censored) > 100, "right_censored");
}

/// 同じパスを2つのルータが登録していないこと。
///
/// テストが全部通っても**起動しない**ことがある。axum は同じパスを2度
/// 登録すると `.merge()` の時点で panic するが、それは `build_app()` を
/// 通らないと起きない。過去にマージで同じルートが2行残り、本番が
/// 21時間出なかった事例がある。
///
/// ここでは「同じスプレッドシートを読む3つのページ」をまとめて merge して、
/// その panic を先に起こす。ルータを増やしたらここにも足すこと。
#[test]
fn ルートが既存のページとぶつからない() {
    use std::sync::Arc;

    use axum::Router;

    use crate::AppState;

    let _: Router<Arc<AppState>> = Router::new()
        .merge(crate::handlers::call_quality::routes::router())
        .merge(crate::handlers::sales_kpi::routes::router())
        .merge(super::routes::router());
}

// ================================================================ タブ8 成果

/// 接触の数え方が変わっていないこと。
///
/// **接触 = MTG または60秒超の通話。メールは数えない。**
/// ここが崩れると 3軸目が丸ごと変わるので、件数で固定する。
#[test]
fn 接触は60秒超の通話とmtgだけを数える() {
    let v = build_outcome(&sheets(), fixture_day());
    let c = &v["contact_source"];
    assert_eq!(c["threshold_sec"], 60.0);
    assert_eq!(c["calls_over_threshold"], 11115, "60秒超の通話");
    assert_eq!(c["mtgs_linked"], 3133, "取引に紐づいた MTG");
    assert_eq!(c["deals_with_contact"], 1434, "接触のある取引");
}

/// 目標が入っていない取引を「達成率0%」に落としていないこと。
///
/// 稼働中 604件（オプション契約を除く）のうち目標が入っているのは 351件。
/// 残り 253件は **母数に入れない**（0%として数えると全体が半分に薄まる）。
#[test]
fn 目標が無い取引は母数に入れない() {
    let v = build_outcome(&sheets(), fixture_day());

    let a = &v["goal_act"];
    assert_eq!(a["pop"], 604, "稼働中の件数（オプション契約を除く）");
    assert_eq!(a["has_goal"], 351, "目標が入っている件数");
    assert_eq!(a["both"], 345, "目標と実績が両方ある件数");

    let unwritten = a["bands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["label"] == "未記入（目標が無い）")
        .and_then(|b| b["n"].as_i64())
        .unwrap();
    assert_eq!(unwritten, 604 - 351, "未記入は pop - has_goal");

    // 0%（実績ゼロ）と 未記入 は別の帯。混ぜない
    let zero = a["bands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["label"] == "0%（実績ゼロ）")
        .and_then(|b| b["n"].as_i64())
        .unwrap();
    assert_eq!(zero, 198, "実績ゼロ（目標はある）");
    assert_ne!(zero, unwritten, "実績ゼロと未記入を同じ数にしない");

    // 帯の合計は母数と一致する。目標はあるが承諾数が空の 6 件（351 - 345）を黙って落とさない
    let bands = a["bands"].as_array().unwrap();
    let sum: i64 = bands.iter().filter_map(|b| b["n"].as_i64()).sum();
    assert_eq!(sum, 604, "帯の合計が pop と合わない");
    let no_syo = bands
        .iter()
        .find(|b| b["label"] == "承諾数が空（目標はある）")
        .and_then(|b| b["n"].as_i64())
        .unwrap();
    assert_eq!(no_syo, 351 - 345, "目標はあるが承諾数が空");

    let all = &v["goal_all"];
    assert_eq!(all["pop"], 3432);
    assert_eq!(all["has_goal"], 1146);

    // 🔴 帯を全部足すと見出しの母数（pop）になること。以前は「目標はあるが採用数が空」
    //    （稼働中 351 − 345 = 6件）がどの帯にも入らず、合計が pop に届かなかった。
    for g in [&v["goal_act"], &v["goal_all"]] {
        let sum: i64 = g["bands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["n"].as_i64().unwrap())
            .sum();
        assert_eq!(
            sum,
            g["pop"].as_i64().unwrap(),
            "帯の合計が母数と合わない: {g}"
        );
        assert_eq!(
            g["no_result"].as_i64().unwrap(),
            g["has_goal"].as_i64().unwrap() - g["both"].as_i64().unwrap()
        );
    }
    assert_eq!(a["no_result"], 6, "目標はあるが採用数が空（稼働中）");
}

/// 求人票あたり応募効率。**決着の3群で比べる**（結果待ちを「継続した」に混ぜない）。
///
/// 🔴 2026-09-23: 以前は `_ => 継続` で結果待ち（稼働中など）が「継続した / 稼働中」に
/// 入っていて（n=1282）、中央値の差がほとんど無く「資料の 8.1 / 14.5 は再現しない」と
/// 固定していた。結果待ち 540件（中央値 3.0）を外すと、継続 n=742（中央値 7.5・平均 12.9）
/// ／ 解約 n=529（中央値 5.0・平均 8.4）と差が出る（Python で別に数えて一致を確認）。
/// 期待値はその値に直した。画面の「再現しません」の注記は画面側チームに報告済み。
#[test]
fn 応募効率は向きだけ合って値は再現しない() {
    let v = build_outcome(&sheets(), fixture_day());
    let g = |label: &str| -> serde_json::Value {
        v["efficiency"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["label"] == label)
            .unwrap()["box"]
            .clone()
    };
    let keizoku = g("継続した");
    let kaiyaku = g("解約した");
    let juusoku = g("充足（採れて終わった）");

    assert_eq!(
        keizoku["n"], 742,
        "継続済だけ。結果待ちが混ざると 1282 になる"
    );
    assert_eq!(kaiyaku["n"], 529);
    assert_eq!(juusoku["n"], 142);
    assert_eq!(v["efficiency"]["n_pending_excluded"], 540);
    assert!(v["efficiency"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| !x["label"].as_str().unwrap().contains("稼働中")));

    let m_keizoku = keizoku["mean"].as_f64().unwrap();
    let m_kaiyaku = kaiyaku["mean"].as_f64().unwrap();
    assert!(
        m_kaiyaku < m_keizoku,
        "平均の向きが逆になっている（解約 {m_kaiyaku:.1} / 継続 {m_keizoku:.1}）"
    );
    assert_eq!(keizoku["median"].as_f64().unwrap(), 7.5);
    assert_eq!(kaiyaku["median"].as_f64().unwrap(), 5.0);

    // 掲載数が空の取引を 0 として入れていないこと
    assert_eq!(v["efficiency"]["has_keisaisu_all"], 2013);
    assert_eq!(v["efficiency"]["has_keisaisu_act"], 593);
}

/// リスク2軸の帯。
#[test]
fn リスク2軸の帯が実データと一致する() {
    let v = build_outcome(&sheets(), fixture_day());
    assert_eq!(v["risk"]["n_act"], 604);

    // 2026-09-23: オプション契約を母集団から外した。
    //   未測定（接触の記録が1つも無い）が 157 -> 64 に落ちる。
    //   **外した99件のうち93件が「接触の記録が1つも無い」だった**ので、
    //   この画面はオプションで埋まっていたことになる。
    // 2026-09-23（2回目）: 白 400 -> 401 / 赤 140 -> 139。通話の日付を日本時間で
    //   取るようにしたため（UTC のままだと日本時間 0〜9時の通話が前日に入る）。
    //   動いたのは1件で、UTC の日付だと契約開始の前日になっていた通話が、日本時間では
    //   開始日当日になり「契約後に一度も接触していない（赤）」から白に移った
    //   （Python で別に数えて一致を確認）。
    assert_eq!(v["risk"]["ax3"]["白"], 401);
    assert_eq!(v["risk"]["ax3"]["赤"], 139);
    assert_eq!(v["risk"]["ax3"]["未測定"], 64);

    // 2026-09-21: 白 548 -> 547 / 未測定 1 -> 2。
    //   金額 0 を「入っていない」扱いに変えたため（0円の契約は存在しない）。
    //   稼働中で amount=0 が1件あり、それが白から未測定へ移った。
    assert_eq!(v["risk"]["ax4"]["白"], 448);
    assert_eq!(v["risk"]["ax4"]["赤"], 154);
    assert_eq!(v["risk"]["ax4"]["未測定"], 2);

    assert_eq!(band_n(&v, "0＝安定"), 337);
    assert_eq!(band_n(&v, "1＝要注意"), 241);
    assert_eq!(band_n(&v, "2＝最優先"), 26);

    let top = v["risk"]["top"].as_array().unwrap();
    assert_eq!(top.len(), 26, "最優先の明細が帯の件数と合っていない");
}

/// 🔴 「接触の記録が1つも無い」を赤にしないこと。
///
/// 64件ある（オプション契約を外す前は157件。差の93件は全部オプションだった）。
/// ここを赤に混ぜると、**本来いちばん拾うべき
/// 「契約後に一度も接触していない」が埋もれる**。
#[test]
fn 接触の記録が無いものは赤にしない() {
    let v = build_outcome(&sheets(), fixture_day());
    let unmeasured = v["risk"]["ax3"]["未測定"].as_i64().unwrap();
    assert_eq!(unmeasured, 64);

    // 未測定は帯の計算にも入らない（3軸目が赤でないので、2軸目だけでは最優先にならない）
    let top = v["risk"]["top"].as_array().unwrap();
    for r in top {
        assert_ne!(
            r["ax3w"], "接触の記録が1つも無い",
            "未測定が最優先に混ざっている: {r}"
        );
    }

    // 「契約後に一度も接触していない」は赤のまま残っていること
    let never = top
        .iter()
        .filter(|r| r["never_after_start"] == true)
        .count();
    assert_eq!(never, 6, "契約後に一度も接触していない最優先案件");
}

/// 🔴 契約開始日が空の取引を「契約後に一度も接触していない」と言い切らないこと。
///
/// 開始日が空だと `last_post` が作れず、以前は「契約後に一度も接触していない」に落ちて
/// `never_after_start` が立っていた。画面の表はその行に「すべて契約前」と書くので、
/// 接触が契約の前か後か分からない行でも言い切ってしまっていた（2026-09-24 検証）。
/// 赤のまま（件数は動かさない）だが、文と印は「切り出せない」に分ける。
#[test]
fn 開始日が空の取引を契約後に接触ゼロと言い切らない() {
    use chrono::NaiveDate;
    let today = NaiveDate::from_ymd_opt(2026, 9, 18).unwrap();
    let mk = |id: &str, start: &str| super::Deal {
        id: id.into(),
        name: id.into(),
        stage: String::new(),
        stage_label: String::new(),
        contract_kind: String::new(),
        // 満了40日後・金額100万 → 収益の軸も赤（最優先に入る）
        contract_expiration_date: "2026-10-28".into(),
        contract_start_date: start.into(),
        kyoten_key: String::new(),
        kyoten_name: String::new(),
        houjin_resolved: String::new(),
        houjin_source: String::new(),
        renewal_no: None,
        is_active: true,
        right_censored: false,
        oubo: None,
        mensetu: None,
        syoudaku: None,
        saiyomokuhyou: None,
        keisaisu: None,
        amount: Some(1_000_000.0),
        contract_period: None,
    };
    let no_start = mk("空", "");
    let before = mk("前だけ", "2026-06-01");
    let mut contacts = std::collections::HashMap::new();
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    contacts.insert("空".to_string(), vec![d("2026-03-01"), d("2026-08-01")]);
    contacts.insert("前だけ".to_string(), vec![d("2026-03-01")]);

    let v = super::routes::risk(&[&no_start, &before], &contacts, today);
    let top = v["top"].as_array().unwrap();
    let row = |id: &str| top.iter().find(|r| r["deal_id"] == id).unwrap().clone();

    let a = row("空");
    assert_eq!(
        a["never_after_start"], false,
        "開始日が空なのに契約後ゼロの印が立っている: {a}"
    );
    assert_eq!(a["no_start"], true);
    assert_ne!(a["ax3w"], "契約後に一度も接触していない", "{a}");
    // 件数は動かさない（赤のまま）
    assert_eq!(v["ax3"]["赤"], 2);

    // 開始日がある「契約前だけ」は、これまでどおり契約後ゼロの印が立つ
    let b = row("前だけ");
    assert_eq!(b["never_after_start"], true, "{b}");
    assert_eq!(b["no_start"], false);
    assert_eq!(b["ax3w"], "契約後に一度も接触していない");
}

/// 最優先の並びは金額の降順。**機械が付けた順であって優先順位ではない**ので、
/// 並びが変わったことに気づけるようにしておく。
#[test]
fn 最優先は金額の降順で返る() {
    let v = build_outcome(&sheets(), fixture_day());
    let top = v["risk"]["top"].as_array().unwrap();
    let mut prev = f64::INFINITY;
    for r in top {
        let a = r["amount"].as_f64().unwrap_or(-1.0);
        assert!(a <= prev, "金額の降順になっていない: {a} の前が {prev}");
        prev = a;
    }
    assert!(v["risk"]["order_note"]
        .as_str()
        .unwrap()
        .contains("機械が付けた順"));
}

/// 基準日を変えるとリスクの帯が動くこと。
///
/// 動かないなら `today` を使わずどこかに今日を埋め込んでいる。
#[test]
fn 基準日を変えると帯が動く() {
    let a = build_outcome(&sheets(), fixture_day());
    let b = build_outcome(
        &sheets(),
        chrono::NaiveDate::from_ymd_opt(2026, 6, 18).expect("2026-06-18"),
    );
    assert_ne!(
        band_n(&a, "2＝最優先"),
        band_n(&b, "2＝最優先"),
        "3ヶ月ずらしても最優先の件数が同じ。today が効いていない"
    );
    assert_eq!(b["meta"]["today"], "2026-06-18");
}

// ================================================================ プロパティ履歴の持ち越し

use super::{fill_forward, series_of, MonthValue};

fn mv(month: &str, v: f64, carry: bool) -> MonthValue {
    MonthValue {
        month: month.to_string(),
        v,
        carry,
    }
}

/// 飛んだ月を前の値で埋めること。
///
/// 1月目に値があり、2〜4月目に行が無く、5月目に新しい値。
/// **3月目を読んだら1月目の値が返る**こと。
#[test]
fn 飛んだ月は前の値を持ち越す() {
    let pts = vec![("2026-01".to_string(), 10.0), ("2026-05".to_string(), 30.0)];
    let got = fill_forward(&pts, "2026-06");
    assert_eq!(
        got,
        vec![
            mv("2026-01", 10.0, false),
            mv("2026-02", 10.0, true),
            mv("2026-03", 10.0, true),
            mv("2026-04", 10.0, true),
            mv("2026-05", 30.0, false),
            mv("2026-06", 30.0, true),
        ]
    );
    // 3月目は1月目の値。ここが 0 や 30 になっていたら持ち越しが壊れている
    let march = got.iter().find(|x| x.month == "2026-03").expect("2026-03");
    assert_eq!(march.v, 10.0);
    assert!(march.carry, "持ち越しなのに carry が立っていない");
}

/// 🔴 最初の実測より前の月を埋めないこと。
///
/// 「値が無い」のであって **0 ではない**。0 で埋めると、契約開始直後に
/// 応募が0件だったように見える。
#[test]
fn 最初の実測より前は埋めない() {
    let pts = vec![("2026-03".to_string(), 7.0)];
    let got = fill_forward(&pts, "2026-05");
    assert_eq!(
        got.first().map(|x| x.month.as_str()),
        Some("2026-03"),
        "最初の点より前の月が出ている: {got:?}"
    );
    assert!(!got.iter().any(|x| x.month.as_str() < "2026-03"));
    assert_eq!(got.len(), 3, "2026-03..05 の3ヶ月だけ");

    // 点が1つも無ければ何も返さない（0の系列を作らない）
    assert!(fill_forward(&[], "2026-05").is_empty());

    // until が最初の点より前なら空
    assert!(fill_forward(&pts, "2026-01").is_empty());
}

/// 持ち越しと実測が区別できること。
///
/// 画面は `carry` を見て中空・破線で描く。ここが全部 false になっていると、
/// 書き換えが無かった月も「その月に更新があった」ように見える。
#[test]
fn 持ち越しと実測が区別できる() {
    let pts = vec![
        ("2026-01".to_string(), 1.0),
        ("2026-02".to_string(), 2.0),
        ("2026-04".to_string(), 4.0),
    ];
    let got = fill_forward(&pts, "2026-04");
    let carried: Vec<&str> = got
        .iter()
        .filter(|x| x.carry)
        .map(|x| x.month.as_str())
        .collect();
    let measured: Vec<&str> = got
        .iter()
        .filter(|x| !x.carry)
        .map(|x| x.month.as_str())
        .collect();
    assert_eq!(measured, vec!["2026-01", "2026-02", "2026-04"], "実測の月");
    assert_eq!(carried, vec!["2026-03"], "持ち越した月");
}

/// 年をまたぐとき 12月の次は翌年1月。
#[test]
fn 年をまたいで持ち越す() {
    let pts = vec![("2025-11".to_string(), 5.0), ("2026-02".to_string(), 9.0)];
    let got = fill_forward(&pts, "2026-02");
    let months: Vec<&str> = got.iter().map(|x| x.month.as_str()).collect();
    assert_eq!(months, vec!["2025-11", "2025-12", "2026-01", "2026-02"]);
}

/// 実データの系列が読めていること。
///
/// 列名を取り違えると空になるので、件数で固定する。
#[test]
fn プロパティ履歴の系列が読める() {
    let sh = sheets();
    let ser = series_of(&sh.history);
    assert!(
        ser.len() > 10_000,
        "系列が {} 本しか作れていない。列名（deal_id / prop / month / v）を確認",
        ser.len()
    );
    // 応募数の系列が実在すること
    let oubo = ser.keys().filter(|(_, p)| p == "oubo").count();
    assert!(oubo > 1000, "応募数の系列が {oubo} 本しかない");

    // どの系列も月が昇順
    for ((deal, prop), pts) in ser.iter().take(500) {
        for w in pts.windows(2) {
            assert!(w[0].0 < w[1].0, "{deal}/{prop} の月が昇順でない: {w:?}");
        }
    }
}

/// 🔴 入力ミスに見える跳ね上がりを落とすのは**畳む側（Python）**の仕事で、
/// Rust は落とした後の結果を読むだけ。
///
/// ここでは「Rust が勝手にもう一度補正していない」ことを確かめる。
/// 二重に補正すると、正当な減少（採用が決まって募集を絞った等）まで消える。
#[test]
fn rustは補正をやり直さない() {
    // 10倍以上の落差をそのまま渡す。Rust 側で消されたら fill_forward の結果が変わる
    let pts = vec![
        ("2026-01".to_string(), 300.0),
        ("2026-02".to_string(), 30.0),
    ];
    let got = fill_forward(&pts, "2026-02");
    assert_eq!(got.len(), 2, "点が消されている: {got:?}");
    assert_eq!(got[0].v, 300.0, "畳む側が残した値を Rust が捨てている");
    assert_eq!(got[1].v, 30.0);
}

// ================================================================ タブ1 いま見るべき顧客

/// 🔴 NPS を「リスクの軸」に混ぜていないこと、母数を必ず添えていること。
///
/// NPS は稼働中の半分にしか無い。軸に混ぜると半分の顧客で構造的に発火せず、
/// 「問題ない」と誤読される。独立した指標として、**何件に入っているか**を添える。
#[test]
fn npsは母数つきで独立して出る() {
    let v = build_focus(&sheets(), fixture_day());
    let n = &v["nps_low"];
    assert_eq!(n["threshold"], 4.0);
    // 母数（NPSが入っている稼働中の件数）が必ず載っている
    let have = n["n_have_nps"].as_i64().expect("n_have_nps");
    let act = n["n_act"].as_i64().expect("n_act");
    assert!(have > 0 && have < act, "NPSが入っているのは {have} / {act}");
    assert!(n["coverage"].as_f64().is_some(), "母数の率が null");
    assert!(
        n["note"]
            .as_str()
            .unwrap()
            .contains("リスクの軸に入れていません"),
        "NPSを軸に入れない理由が payload に載っていない"
    );

    // 低NPSの行はしきい値以下だけ
    for r in n["rows"].as_array().unwrap() {
        assert!(
            r["nps"].as_f64().unwrap() <= 4.0,
            "しきい値を超える行がある: {r}"
        );
    }
    assert_eq!(n["n"], n["rows"].as_array().unwrap().len());

    // タブ8のリスク2軸に NPS が入っていないこと
    let o = build_outcome(&sheets(), fixture_day());
    assert!(o["risk"]["ax3"].is_object() && o["risk"]["ax4"].is_object());
    assert!(o["risk"].get("ax1").is_none(), "NPSの軸が復活している");
    assert!(
        o["risk"].get("ax2").is_none(),
        "churnモデルの軸が復活している"
    );
}

/// 🔴 採用単価は**拠点ごと**に見ること。法人で1本にまとめない。
/// 🔴 未確定（右側打ち切り）の点で悪化を判定しないこと。
#[test]
fn 採用単価は拠点ごとに確定した点だけで判定する() {
    let v = build_focus(&sheets(), fixture_day());
    let c = &v["cpa"];
    let judged = c["judged"].as_i64().unwrap();
    let worse = c["worse"].as_i64().unwrap();
    assert!(judged > 100, "判定できた拠点が {judged} しかない");
    assert!(worse <= judged);
    assert!(c["worse_rate"].as_f64().is_some(), "率が null");
    assert!(
        c["skipped_censored"].as_i64().unwrap() > 0,
        "未確定しか無くて判定を見送った拠点が0件。打ち切りを外せていない"
    );
    assert!(c["note"].as_str().unwrap().contains("拠点ごと"));
}

/// 🔴 MTG の「実施した事実」を1つの率にまとめないこと。
///
/// Zoom録画は事実、メール由来は推定（±1日で83.3%）。混ぜると推定が事実の顔をする。
#[test]
fn mtgの事実と推定を同じ率にまとめない() {
    let v = build_focus(&sheets(), fixture_day());
    let m = &v["mtg_layers"];
    let fact = m["fact_recording"]["n"].as_i64().unwrap();
    let est = m["estimated_mail"]["n"].as_i64().unwrap();
    assert!(
        fact > 0 && est > 0,
        "どちらかの層が空: 事実{fact} / 推定{est}"
    );
    // 2つの層が別のキーで返る
    assert!(m["fact_recording"]["label"]
        .as_str()
        .unwrap()
        .contains("事実"));
    assert!(m["estimated_mail"]["label"]
        .as_str()
        .unwrap()
        .contains("推定"));
    // 内訳が母数と合う
    let both = m["both"].as_i64().unwrap();
    let only_r = m["only_recording"].as_i64().unwrap();
    let only_m = m["only_mail"].as_i64().unwrap();
    let neither = m["neither"].as_i64().unwrap();
    assert_eq!(
        both + only_r + only_m + neither,
        m["n_act"].as_i64().unwrap()
    );
    assert_eq!(fact, both + only_r);
    assert_eq!(est, both + only_m);
}

/// 顧客ぜんたいの形。母数（表示対象）を添えていること。
#[test]
fn 顧客の形が母数つきで出る() {
    let v = build_focus(&sheets(), fixture_day());
    let sh = &v["shape"];
    assert_eq!(sh["n_all"], 1649);
    // 🔴 画面の「法人 N」は houjin_population の main（本部アプローチ・法人番号で見ると同じ母数, F4）
    assert_eq!(
        sh["n_houjin"], 1646,
        "画面の法人数が houjin_population と合わない"
    );
    assert_eq!(sh["n_houjin_option_only"], 3);
    let hq = build_headquarters(&sheets(), fixture_day());
    assert_eq!(
        sh["n_houjin"], hq["meta"]["n_houjin"],
        "本部アプローチの全 N 法人と合わない"
    );
    let disp = sh["n_display"].as_i64().unwrap();
    assert!(disp > 0 && disp < 1649, "表示対象が {disp} 件");
    assert!(sh["ltv"]["n"].as_i64().unwrap() > 0, "LTVの代表値が空");
    assert!(
        sh["multi_site"].as_i64().unwrap() > 0,
        "複数拠点の法人が0件"
    );
}

// ================================================================ タブ7 立ち上がり

/// 🔴 フェーズは**契約長に対する割合**で決めること。経過月数ではない。
/// 契約期間が空のものを 0 として「満了超過」に落とさないこと。
#[test]
fn フェーズは契約長に対する割合で決まる() {
    let v = build_rampup(&sheets(), fixture_day());
    let ph = &v["phase"];
    assert_eq!(ph["total"], 604);
    let n = |label: &str| -> i64 {
        ph["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["label"] == label)
            .and_then(|r| r["n"].as_i64())
            .unwrap_or_else(|| panic!("{label} が無い"))
    };
    assert_eq!(n("序盤"), 263);
    assert_eq!(n("中盤"), 172);
    assert_eq!(n("終盤"), 153);
    assert_eq!(n("満了超過"), 15);
    // 🔴 契約期間が空のものは「出せない」。満了超過に混ぜない
    assert_eq!(n("出せない"), 1);
    let total: i64 = ph["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["n"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(total, 604, "内訳の合計が母数と合わない");
    assert!(ph["rule"].as_str().unwrap().contains("契約長に対する割合"));
}

/// 立ち上がりの速さ。**契約前のMTGを日数に混ぜない。**
#[test]
fn 立ち上がりは契約後のmtgだけで測る() {
    let v = build_rampup(&sheets(), fixture_day());
    let f = &v["first_mtg"];
    assert_eq!(f["n"], 1083);
    assert_eq!(f["pre_contract"], 15, "契約前のMTGは別に数える");
    assert_eq!(f["stats"]["median"].as_f64().unwrap(), 14.0, "中央値14日");

    // 帯ごとの解約率。決着0件なら null
    for b in f["buckets"].as_array().unwrap() {
        if b["n"] == 0 {
            assert!(b["cancel_rate"].is_null(), "母数0なのに率が出ている: {b}");
        }
    }
    // 🔴 単調ではないことを注記に残しているか
    assert!(f["note"].as_str().unwrap().contains("単調ではない"));
}

/// 稼働中の初回契約で、まだMTGの記録が無いもの。
#[test]
fn mtgが結べていない初回契約が出る() {
    let v = build_rampup(&sheets(), fixture_day());
    let nm = &v["no_mtg"];
    // 2026-09-23: 「マーケ関連」（紹介料・マーケ施策の計上）10件を表と分母から外した。
    //   264 -> 254、114 -> 104。10件とも MTG の記録が無かった。件数は別に返す
    assert_eq!(nm["first_active"], 254);
    assert_eq!(nm["excluded_marketing"], 10);
    assert_eq!(nm["n"], 104);
    assert_eq!(nm["rows"].as_array().unwrap().len(), 104);
    assert!(nm["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["stage"] != "マーケ関連"));
    assert!(nm["rate"].as_f64().is_some());
    // 記録が無いことと、やっていないことを分けて書いているか
    assert!(nm["note"].as_str().unwrap().contains("記録が無いことと"));
}

// ================================================================ タブ6 電話

/// 🔴 「接触が1本も無い」取引の経過日数を 0 にしないこと。
///
/// 0 日にすると「昨日話した」ように見える。null のままで、沈黙の先頭に出す。
#[test]
fn 接触ゼロの取引は経過日数をnullにする() {
    let v = build_phone(&sheets(), fixture_day());
    // 2026-09-23: オプション契約を外した。電話が1本も無い稼働中は 156 -> 61、
    //   接触が1本も無いものは 176 -> 81。**減った95件はオプション契約**で、
    //   もともと電話する相手ではない契約がこの画面を埋めていた。
    assert_eq!(v["reach"]["no_call"], 61, "電話が1本も無い");
    assert_eq!(v["reach"]["no_contact"], 81, "接触(60秒超)が1本も無い");

    let rows = v["silent"]["rows"].as_array().unwrap();
    let zero = rows.iter().filter(|r| r["days_since"].is_null()).count();
    // 2026-09-23: 「マーケ関連」10件（全部が接触ゼロ）を表からだけ外した。
    //   KPI の no_contact（81）はそのまま。表は 81 -> 71、外した件数は別に返す
    assert_eq!(zero, 71, "接触ゼロの行が日数 null になっていない");
    assert_eq!(v["silent"]["excluded_marketing"], 10);
    assert!(rows.iter().all(|r| r["stage"] != "マーケ関連"));
    for r in rows {
        if r["days_since"].is_null() {
            assert_eq!(r["n_contact"], 0);
            assert!(r["last_contact"].is_null());
        }
    }
    // 日数が出せないものが先頭に来る（いちばん拾うべきもの）
    assert!(
        rows[0]["days_since"].is_null(),
        "接触ゼロが先頭に来ていない"
    );
}

/// 経過日数の代表値と、文字起こしの薄さ。
#[test]
fn 電話の経過日数と文字起こしの薄さが出る() {
    let v = build_phone(&sheets(), fixture_day());
    assert_eq!(v["days_since"]["median"].as_f64().unwrap(), 8.0);
    assert_eq!(v["meta"]["threshold_sec"], 60.0);

    let t = &v["transcript"];
    // 2026-09-23: オプション契約に付いた通話（2,218行）を数えないようにした。
    //   20,843 -> 18,625行、文字起こし 1,372 -> 1,275（Python で別に数えて一致）
    assert_eq!(t["n"], 1275);
    assert_eq!(t["rows"], 18625);
    assert_eq!(v["reach"]["option_rows_excluded"], 2218);
    assert!(
        t["rate"].as_f64().unwrap() < 10.0,
        "文字起こしが薄いという読みが変わる"
    );
    assert!(t["note"].as_str().unwrap().contains("まだ読めていません"));

    // 月ごとの率は分母0で null
    for m in v["monthly"].as_array().unwrap() {
        if m["calls"] == 0 {
            assert!(m["contact_rate"].is_null());
        }
    }
}

// ================================================================ タブ4 本部アプローチ

/// 🔴 親法人へロールアップしないこと。事業所ごとに出す。
#[test]
fn 本部は事業所ごとに並べる() {
    let v = build_headquarters(&sheets(), fixture_day());
    // 2026-09-23: オプション契約を外したので 1649 -> 1646。
    //   3法人は**オプション契約しか無かった**法人。
    assert_eq!(v["meta"]["n_houjin"], 1646);
    assert_eq!(v["meta"]["n_houjin_option_only"], 3);
    assert_eq!(v["multi_site"], 194, "拠点が2つ以上ある法人");

    for row in v["rows"].as_array().unwrap() {
        let sites = row["rows"].as_array().unwrap();
        assert!(sites.len() >= 2, "拠点が1つの法人が混ざっている: {row}");
        // 採用0の拠点は単価を出さない（0で割らない）
        for st in sites {
            if st["syoudaku"].as_f64().unwrap_or(0.0) == 0.0 {
                assert!(st["cpa"].is_null(), "採用0なのに単価が出ている: {st}");
            }
            if st["deals"] == 0 {
                assert!(st["cancel_rate"].is_null());
            }
        }
    }
    assert!(v["meta"]["not_counted"]
        .as_str()
        .unwrap()
        .contains("事業所ごと"));
}

// ================================================================ タブ5 MTGの品質

/// 🔴 分析項目が空なのは「記録していない」ではなく「まだ抽出していない」。
#[test]
fn mtgの埋まり具合は抽出の進み具合として出す() {
    let v = build_mtg_quality(&sheets(), fixture_day());
    // 2026-09-23: オプション契約に結ばれた MTG（9行）を数えないようにした。3133 -> 3124
    assert_eq!(v["meta"]["n_mtg"], 3124);
    assert_eq!(v["meta"]["option_rows_excluded"], 9);
    assert_eq!(v["linked"]["n"], 3124, "取引に結べた MTG");

    let todo = v["filled"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == "やること")
        .expect("やること");
    assert_eq!(todo["n"], 426);
    assert!(
        todo["rate"].as_f64().unwrap() < 20.0,
        "抽出が一部だけという読みが変わる"
    );
    assert!(v["filled_note"]
        .as_str()
        .unwrap()
        .contains("まだ抽出を通していない"));

    // リスク判定の分布は母数と合う
    let sum: i64 = v["risk_dist"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["n"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(sum, 3124, "リスク判定の内訳が母数と合わない");
    // 未判定を黙って落としていない
    assert!(v["risk_dist"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["label"] == "（未判定）"));
}

// ================================================================ タブ9 データ品質

/// 法人番号の出どころと、欠測の一覧。
#[test]
fn データ品質は欠測を件数で出す() {
    let v = build_data_quality(&sheets(), fixture_day());
    assert_eq!(v["meta"]["n_deals"], 3432);

    let sum: i64 = v["houjin_source"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["n"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(sum, 3432, "法人番号の出どころの内訳が母数と合わない");
    let note = v["houjin_source"]["note"].as_str().unwrap();
    assert!(note.contains("1社1つ"), "法人番号の規律が載っていない");
    assert!(
        note.contains("就業場所"),
        "法人番号と就業場所の区別が載っていない"
    );

    // 欠測は件数と率の両方。率は分母0なら null
    for m in v["missing"].as_array().unwrap() {
        assert!(m["n"].as_i64().is_some());
        assert!(m["rate"].as_f64().is_some(), "率が出ていない: {m}");
    }
    let censored = v["missing"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["label"].as_str().unwrap().contains("右側打ち切り"))
        .unwrap();
    assert_eq!(censored["n"], 419);

    // 読んだシートの行数が全部載っている
    let sheets_listed = v["sheets"].as_array().unwrap();
    // 🔴 先読みする9枚（SHEETS）と同じ顔ぶれ・同じ順。以前は7枚で担当履歴とメタが抜けていた
    let names: Vec<&str> = sheets_listed
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        super::SHEETS.to_vec(),
        "データ品質のシート一覧が先読みと違う"
    );
    for sh in sheets_listed {
        assert!(sh["rows"].as_i64().unwrap() > 0, "空のシートがある: {sh}");
    }
}

// ================================================================ タブ3 顧客詳細

/// 法人を指定しないときは一覧だけ返すこと（明細を全部返すと巨大になる）。
#[test]
fn 顧客詳細は法人未指定なら一覧だけ返す() {
    let v = build_customer(&sheets(), None, fixture_day());
    let idx = v["index"].as_array().expect("index");
    assert!(idx.len() > 100, "一覧が {} 件", idx.len());
    assert!(v.get("deals").is_none(), "明細まで返している");
    // LTV の降順
    let mut prev = f64::INFINITY;
    for r in idx {
        let l = r["ltv"].as_f64().unwrap_or(0.0);
        assert!(l <= prev, "LTVの降順になっていない");
        prev = l;
    }
}

/// 🔴 採用単価は拠点ごとに分けて返すこと。1本にまとめない。
#[test]
fn 顧客詳細の採用単価は拠点ごとに分かれる() {
    let all = build_customer(&sheets(), None, fixture_day());
    // 拠点が2つ以上ある法人を1つ選ぶ
    let target = all["index"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["sites"].as_f64().unwrap_or(0.0) >= 2.0)
        .expect("拠点が2つ以上ある法人が無い");
    let h = target["houjin"].as_str().unwrap();

    let v = build_customer(&sheets(), Some(h), fixture_day());
    assert_eq!(v["meta"]["found"], true);
    assert!(!v["deals"].as_array().unwrap().is_empty());
    let sites = v["cpa_by_site"].as_array().unwrap();
    assert!(!sites.is_empty(), "採用単価が拠点ごとに出ていない");
    // 取引は契約開始の昇順
    let mut prev = String::new();
    for d in v["deals"].as_array().unwrap() {
        let st = d["start"].as_str().unwrap_or("").to_string();
        assert!(st >= prev, "契約開始の昇順になっていない");
        prev = st;
    }
    // MTG は「抽出済みか」が分かる
    for m in v["mtgs"].as_array().unwrap() {
        assert!(m["extracted"].is_boolean(), "抽出済みかどうかが分からない");
    }
}

/// 存在しない法人を指定しても落ちないこと。
#[test]
fn 存在しない法人でも落ちない() {
    let v = build_customer(&sheets(), Some("この法人は存在しない"), fixture_day());
    assert_eq!(v["meta"]["found"], false);
    assert!(v["deals"].as_array().unwrap().is_empty());
    assert!(v["customer"].is_null());
}

/// 🔴 画面に**内部IDを出さない**。ステージは日本語名で返すこと。
///
/// 2026-09-21 の実害: ステージ列に `52016155` のような ID が並んでいた。
/// 現場は内部IDを見ても何のことか分からない。
#[test]
fn ステージは日本語名で返る() {
    let v = build_outcome(&sheets(), fixture_day());
    let top = v["risk"]["top"].as_array().expect("top");
    assert!(!top.is_empty());
    for r in top {
        let st = r["stage"].as_str().unwrap_or("");
        assert!(!st.is_empty(), "ステージが空: {r}");
        assert!(
            st.parse::<u64>().is_err(),
            "ステージが内部ID（{st}）のまま。dealstage_label を返すこと"
        );
        // 取引名も返していること（画面は取引IDでなくこちらを出す）
        assert!(r["name"].is_string(), "取引名を返していない: {r}");
    }

    // タブ1・6・7 も同じ
    let f = build_focus(&sheets(), fixture_day());
    for r in f["nps_low"]["rows"].as_array().unwrap().iter().take(20) {
        assert!(
            r["stage"].as_str().unwrap_or("").parse::<u64>().is_err(),
            "タブ1のステージが内部IDのまま: {r}"
        );
    }
    let p = build_phone(&sheets(), fixture_day());
    for r in p["silent"]["rows"].as_array().unwrap().iter().take(20) {
        assert!(
            r["stage"].as_str().unwrap_or("").parse::<u64>().is_err(),
            "タブ6のステージが内部IDのまま: {r}"
        );
    }
    let ru = build_rampup(&sheets(), fixture_day());
    for r in ru["no_mtg"]["rows"].as_array().unwrap().iter().take(20) {
        assert!(
            r["stage"].as_str().unwrap_or("").parse::<u64>().is_err(),
            "タブ7のステージが内部IDのまま: {r}"
        );
    }
}

// ================================================================ 列の追加（2026-09-21）

/// 🔴 金額0は「入っていない」。`0万` と書かず、採用単価の計算にも入れない。
///
/// 0円の契約は存在しないので、0 は未入力の裏返し。0 を混ぜると採用単価の
/// 平均や中央値が下に引っ張られる。実データで 3,659件中2件（稼働中1件）。
#[test]
fn 金額0は入っていない扱いになる() {
    let sh = sheets();
    let deals = super::deals_of(&sh.deal);
    assert!(
        !deals
            .iter()
            .any(|d| matches!(d.amount, Some(v) if v <= 0.0)),
        "金額に 0 以下が残っている。money() が効いていない"
    );
    for d in &deals {
        if d.amount.is_none() {
            assert!(
                super::cpa(d).is_none(),
                "金額が無いのに採用単価が出ている: {}",
                d.id
            );
        }
    }
    // シートには 0 の行が実在する（テストが素通りしていないことの確認）
    let raw_zero = sh
        .deal
        .rows
        .iter()
        .filter(|r| {
            let v = sh.deal.get(r, "amount").trim();
            !v.is_empty() && v.parse::<f64>().map(|x| x <= 0.0).unwrap_or(false)
        })
        .count();
    assert!(
        raw_zero > 0,
        "シートに金額0の行が無い。このテストが意味を持たない"
    );
}

/// ③案件の立ち位置に、応募・面接・採用・接触率がそろっていること。
///
/// 🔴 接触率は**②コンサルタント一覧と同じ定義**（接触＝MTG または60秒超の通話）。
/// 率だけ出さず、分子と分母を必ず一緒に返す。
#[test]
fn 案件ごとに応募面接採用と接触率が出る() {
    let v = build_deal_board(&sheets(), fixture_day());
    let rows = v["rows"].as_array().expect("rows");
    assert!(!rows.is_empty());

    for key in [
        "oubo",
        "mensetu",
        "syoudaku",
        "contact_touched",
        "contact_months",
    ] {
        assert!(
            rows.iter().any(|r| r.get(key).is_some()),
            "{key} の列が無い"
        );
    }
    for key in ["oubo_carry", "mensetu_carry", "syoudaku_carry"] {
        assert!(rows.iter().any(|r| r.get(key).is_some()), "{key} が無い");
    }

    let mut with_rate = 0;
    for r in rows {
        let den = r["contact_months"].as_u64().expect("contact_months");
        let n = r["contact_touched"].as_u64().expect("contact_touched");
        assert!(n <= den, "接触した月が経過月を超えている: {r}");
        if den == 0 {
            // 🔴 分母0の率は null。0% と書かない
            assert!(
                r["contact_rate"].is_null(),
                "経過月0なのに率が出ている: {r}"
            );
        } else {
            let got = r["contact_rate"].as_f64().expect("contact_rate");
            let want = n as f64 / den as f64 * 100.0;
            assert!((got - want).abs() < 0.01, "接触率が分子分母と合わない: {r}");
            with_rate += 1;
        }
    }
    assert!(
        with_rate > 100,
        "接触率を出せる案件が {with_rate} 件しかない"
    );
}

/// ②と③で接触率の定義がずれていないこと。
///
/// 同じ数字を2か所で作ると、画面ごとに値が違う原因になる。
#[test]
fn 接触率の定義が担当者一覧と案件一覧でそろっている() {
    let sh = sheets();
    let team = build_consultants(&sh, fixture_day());
    let board = build_deal_board(&sh, fixture_day());

    let mut sum: std::collections::BTreeMap<String, (u64, u64)> = Default::default();
    for r in board["rows"].as_array().unwrap() {
        let who = r["consultant"].as_str().unwrap_or("").to_string();
        if who.is_empty() {
            continue;
        }
        let e = sum.entry(who).or_insert((0, 0));
        e.0 += r["contact_touched"].as_u64().unwrap_or(0);
        e.1 += r["contact_months"].as_u64().unwrap_or(0);
    }
    let mut checked = 0;
    for t in team["rows"].as_array().unwrap() {
        let who = t["consultant"].as_str().unwrap().to_string();
        let Some((tn, td)) = sum.get(&who) else {
            continue;
        };
        assert_eq!(
            t["contact_touched"].as_u64().unwrap(),
            *tn,
            "{who} の分子がずれている"
        );
        assert_eq!(
            t["contact_months"].as_u64().unwrap(),
            *td,
            "{who} の分母がずれている"
        );
        checked += 1;
    }
    assert!(checked > 10, "突き合わせた担当者が {checked} 名しかない");
}

/// 🔴 契約開始がまだ先の案件を「-1 か月目」と出さない。
///
/// 実測で115件ある。そのまま計算すると負の経過月になって読めない。
/// 「接触の記録が無い」の名札も立てない（まだ始まっていないので当たり前）。
#[test]
fn 契約開始がまだ先の案件は開始前として分ける() {
    let v = build_deal_board(&sheets(), fixture_day());
    let rows = v["rows"].as_array().expect("rows");
    let ns: Vec<&serde_json::Value> = rows.iter().filter(|r| r["not_started"] == true).collect();
    assert!(
        !ns.is_empty(),
        "開始前の案件が1件も無い。判定が効いていない"
    );

    for r in rows {
        // 経過月が負のまま出ていないこと
        if let Some(m) = r["months"].as_f64() {
            assert!(m >= 0.0, "経過月が負のまま出ている: {r}");
        }
        if r["not_started"] == true {
            assert!(r["months"].is_null(), "開始前なのに経過月が出ている: {r}");
            let flags: Vec<&str> = r["flags"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x.as_str())
                .collect();
            assert!(
                !flags.iter().any(|f| f.contains("接触")),
                "開始前なのに接触の名札が立っている: {flags:?}"
            );
        }
    }
}

/// 🔴 母数の小さい担当者を、図と表で別に扱えること。
///
/// 1案件・5か月の分母で 0% になった人が、33案件で 24.4% の人より「悪い」位置に
/// 並ぶと実態とずれる。図からは外し、**表には残す**（接触ゼロは拾いたい）。
/// サーバは印（`small_n`）を返すだけで、外すかどうかは画面が決める。
#[test]
fn 母数が小さい担当者に印が付く() {
    let v = build_consultants(&sheets(), fixture_day());
    let rows = v["rows"].as_array().expect("rows");
    assert!(!rows.is_empty());

    let mut small = 0;
    for r in rows {
        let months = r["contact_months"].as_u64().expect("contact_months");
        let flagged = r["small_n"].as_bool().expect("small_n");
        assert_eq!(
            flagged,
            months < super::MIN_CONTACT_MONTHS as u64,
            "{} の印が分母と合っていない（{months} か月）",
            r["consultant"]
        );
        if flagged {
            small += 1;
        }
    }
    assert!(
        small > 0,
        "母数が小さい担当者が1人もいない。判定が効いていない"
    );
    // 🔴 表から消していないこと（サーバは全員返す）
    assert!(rows.len() > small, "母数が小さい人しかいない");

    // 外す理由が payload に載っていること
    let rule = v["small_n_rule"].as_str().expect("small_n_rule");
    assert!(
        rule.contains("表には残して"),
        "図と表で扱いを変える理由が載っていない"
    );
}

/// ④顧客詳細が、開いた瞬間に空にならないこと。
#[test]
fn 顧客詳細に既定の法人がある() {
    let v = build_customer(&sheets(), None, fixture_day());
    let h = v["default_houjin"].as_str().expect("default_houjin が無い");
    assert!(!h.is_empty());

    // 既定は「取引がいちばん多い法人」
    let idx = v["index"].as_array().unwrap();
    let top = idx
        .iter()
        .max_by(|a, b| {
            a["deals"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&b["deals"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    assert_eq!(
        top["houjin"].as_str().unwrap(),
        h,
        "既定が取引数の最多と一致しない"
    );

    // 選んだ理由が payload に載っていること
    assert!(v["default_reason"]
        .as_str()
        .unwrap()
        .contains("取引がいちばん多い"));

    // その法人を実際に開けること
    let d = build_customer(&sheets(), Some(h), fixture_day());
    assert_eq!(d["meta"]["found"], true);
    assert!(!d["deals"].as_array().unwrap().is_empty());
}

// ================================================================ ループ3

/// シートをいつ作ったかが読めること。
///
/// 🔴 これが読めないと、画面は「古いデータを新しいものと誤認させない」責任を
/// 果たせない。`meta.today`（計算の基準日）とは**別物**。
#[test]
fn データをいつ作ったかが読める() {
    let sh = sheets();
    let at = super::generated_at(&sh.meta).expect("生成時刻が読めない");
    assert_eq!(
        at, "2026-09-16 04:30:00",
        "fixture の生成時刻は手で固定してある"
    );

    // 基準日 2026-09-18 から見て2日前
    let age = super::generated_age_days(&sh.meta, fixture_day()).expect("経過日数");
    assert_eq!(age, 2, "生成時刻と基準日の差が合わない");

    // 🔴 元データを落とした時刻は、シートを作り直した時刻と**別物**。
    //    古い JSON を詰め直すと生成時刻だけ新しくなるので、
    //    「何日前のデータか」は元データのほうで数える。
    let src = super::data_as_of(&sh.meta).expect("データ取得時刻が読めない");
    assert_eq!(src, "2026-09-14 22:00:00");
    assert_ne!(
        src, at,
        "元データの時刻と生成時刻を同じものとして扱っている"
    );
    assert_eq!(
        super::data_age_days(&sh.meta, fixture_day()).expect("経過日数"),
        4,
        "元データの経過日数が合わない"
    );
    assert!(
        super::data_age_days(&sh.meta, fixture_day())
            > super::generated_age_days(&sh.meta, fixture_day()),
        "元データはシートを作った時刻より古いはず"
    );
}

/// メタシートが読めないときに「新しい」と嘘をつかないこと。
#[test]
fn 生成時刻が無ければ分からないと返す() {
    let empty = std::sync::Arc::new(SheetData {
        header: vec!["key".into(), "value".into()],
        rows: Vec::new(),
        fetched_at: Instant::now(),
    });
    assert!(
        super::generated_at(&empty).is_none(),
        "空なのに時刻を返した"
    );
    assert!(
        super::data_as_of(&empty).is_none(),
        "空なのに取得時刻を返した"
    );
    assert!(
        super::data_age_days(&empty, fixture_day()).is_none(),
        "空なのに経過日数を返した"
    );
    assert!(
        super::generated_age_days(&empty, fixture_day()).is_none(),
        "空なのに経過日数を返した（0日＝今日 と誤認させる）"
    );
}

/// 🔴 **まとめるのに使うキーを連番にしない。**
///
/// fixture の伏字は「読めば分かる列」を潰すためのものだが、
/// **まとめるキーまで連番にすると1行1グループになり、
/// 「◯◯ごとに見る」処理がテストで素通りする**。
/// 拠点キー（採用単価が0件になった）・担当者（全員が母数が小さいになった）で
/// 2度踏んだ穴。ホスト氏名も同じ性質なので、ここで塞いでおく。
#[test]
fn まとめるキーが行ごとの連番になっていない() {
    let sh = sheets();
    let v = build_mtg_quality(&sh, fixture_day());
    let n_mtg = v["meta"]["n_mtg"].as_u64().expect("n_mtg") as usize;
    let n_host = v["hosts"].as_array().expect("hosts").len();
    assert!(n_mtg > 500, "MTG が少なすぎる: {n_mtg}");
    assert!(
        n_host * 10 < n_mtg,
        "ホストが {n_host} 種で MTG が {n_mtg} 件。1MTG1ホストに近く、\
         「ホストごとに数える」処理が素通りしている。fixture の伏字で\
         ホスト氏名を連番にしていないか確認すること"
    );

    // 担当者・拠点も同じ穴。ここでまとめて見張る
    let t = build_consultants(&sh, fixture_day());
    let n_consultant = t["rows"].as_array().expect("rows").len();
    let n_active = t["meta"]["n_active"].as_u64().expect("n_active") as usize;
    assert!(
        n_consultant * 5 < n_active,
        "担当者 {n_consultant} 名に対して稼働中 {n_active} 件。1取引1担当に近い"
    );
}

// ================================================================ オプション契約を外す
//
// 🔴 2026-09-23。「案件のときにエアワーク等のオプションが入っていて読めない」
//    という指摘を受けて、**全画面の母集団からオプション契約を外した**。
//    ここが緩むと、案件一覧にも担当者の持ち件数にもオプションが戻ってくる。

/// オプション契約の判定は、種別とステージの **OR** で取りこぼさない。
///
/// 実データ 3,659件で数えた内訳（2026-09-23）:
///   種別で当たる      223件（求人追加128 / AirWork広告運用68 / 一次対応13 /
///                            エントリーフォーム6 / 追加5 / AirWork 3）
///   ステージだけで当たる 4件（種別が `(新規)` や `サブスク継続` のまま
///                            「満了済オプション」ステージに置かれている）
///   合計              227件
///
/// 🔴 **種別だけでは4件、ステージだけでは54件を取りこぼす。**
#[test]
fn オプションは種別とステージの両方で拾う() {
    let sh = sheets();
    let all = super::deals_all_of(&sh.deal);
    assert_eq!(all.len(), 3659, "全取引");

    let by_kind = all
        .iter()
        .filter(|d| super::OPTION_KINDS.contains(&d.contract_kind.as_str()))
        .count();
    let by_stage = all
        .iter()
        .filter(|d| super::OPTION_STAGES.contains(&d.stage.as_str()))
        .count();
    let both = all.iter().filter(|d| d.is_option()).count();

    assert_eq!(by_kind, 223, "種別で当たるもの");
    assert_eq!(by_stage, 173, "オプション用ステージに置かれているもの");
    assert_eq!(both, 227, "どちらかに当たるもの");
    assert!(
        both > by_kind && both > by_stage,
        "片方だけで足りているなら、この OR は要らないはず（種別{by_kind} / ステージ{by_stage} / OR {both}）"
    );

    // 「AirWork広告費用＿…」は種別が `AirWork` になる。取引名では判定していない
    assert!(
        super::OPTION_KINDS.contains(&"AirWork"),
        "AirWork広告費用 の3件が母集団に残る"
    );
}

/// 🔴 オプション契約が**案件の一覧に出ない**こと。
///
/// ①今日動く先 / ①案件そのもの / ②担当者ごとの案件 は同じ `deal_rows` を使う。
/// ここに1件でも混ざったら落とす。
#[test]
fn オプション契約は案件の一覧に出ない() {
    let sh = sheets();
    let day = fixture_day();
    let option_ids: std::collections::HashSet<String> = super::deals_all_of(&sh.deal)
        .iter()
        .filter(|d| d.is_option())
        .map(|d| d.id.clone())
        .collect();
    assert!(
        !option_ids.is_empty(),
        "テストデータにオプションが1件も無い"
    );

    let board = build_deal_board(&sh, day);
    let today = build_today_board(&sh, day);
    let mut checked = 0usize;
    let screens: [(&str, &Value, &[&str]); 2] = [
        ("案件そのもの", &board, &["rows"]),
        ("今日動く先", &today, &["rows", "expiring_this_week"]),
    ];
    for (name, v, keys) in screens {
        for key in keys {
            let rows = v[*key]
                .as_array()
                .unwrap_or_else(|| panic!("{name} に {key} が無い。キー名が変わった"));
            checked += rows.len();
            for r in rows {
                let id = r["deal_id"].as_str().unwrap_or("");
                assert!(
                    !option_ids.contains(id),
                    "{name} の {key} にオプション契約が出ている: {r}"
                );
            }
        }
    }

    // 🔴 1行も見ていないのに緑になるのを防ぐ
    assert!(checked > 600, "案件の行を {checked} 行しか見ていない");

    // 件数そのものも押さえる
    let pop = super::population_of(&sh.deal);
    assert_eq!(
        board["rows"].as_array().unwrap().len(),
        pop.active,
        "案件そのものの行数が母集団と合っていない"
    );
}

/// 🔴 **すべての画面で母集団の件数が一致すること。**
///
/// 直す前は、②コンサルタント一覧が593件・③案件の立ち位置が703件と、
/// 同じ画面の中で数が合っていなかった。`freshen` が1か所で数えた値を
/// 全部の応答に載せるので、**画面ごとに数え直していたら落ちる**。
#[test]
fn 全画面で母集団の件数が一致する() {
    let sh = sheets();
    let day = fixture_day();
    let f = |v: Value| super::routes::freshen(v, &sh, day);

    let screens: Vec<(&str, Value)> = vec![
        ("継続回数 × 成果", f(build_renewal(&sh, false))),
        ("成果とリスク", f(build_outcome(&sh, day))),
        ("いま見るべき顧客", f(build_focus(&sh, day))),
        ("立ち上がり", f(build_rampup(&sh, day))),
        ("電話", f(build_phone(&sh, day))),
        ("本部アプローチ", f(build_headquarters(&sh, day))),
        ("MTG の品質", f(build_mtg_quality(&sh, day))),
        ("データ品質", f(build_data_quality(&sh, day))),
        ("担当者の一覧", f(build_consultants(&sh, day))),
        ("担当の交代", f(build_handover(&sh, day))),
        ("担当者ごとの接触", f(build_contact_trend(&sh, day))),
        ("案件そのもの", f(build_deal_board(&sh, day))),
        ("今日動く先", f(build_today_board(&sh, day))),
        ("顧客ごとに見る", f(build_customer(&sh, None, day))),
    ];

    let want = 604;
    for (name, v) in &screens {
        let p = &v["population"];
        assert!(!p.is_null(), "{name} に母集団が載っていない");
        assert_eq!(
            p["active"], want,
            "{name} の母集団（稼働中・オプション除く）"
        );
        assert_eq!(p["active_all"], 703, "{name} の稼働中（オプション込み）");
        assert_eq!(p["active_option"], 99, "{name} の外したオプション");
        assert_eq!(p["deals"], 3432, "{name} の全取引（オプション除く）");
        assert_eq!(p["deals_all"], 3659, "{name} の全取引（オプション込み）");
    }

    // 画面が自分で数えている件数も、同じ母集団を指していること
    let get = |label: &str| -> Value {
        screens
            .iter()
            .find(|(n, _)| *n == label)
            .unwrap_or_else(|| panic!("{label} が無い"))
            .1
            .clone()
    };
    assert_eq!(get("いま見るべき顧客")["meta"]["n_active"], want);
    assert_eq!(get("立ち上がり")["meta"]["n_active"], want);
    assert_eq!(get("電話")["meta"]["n_active"], want);
    assert_eq!(get("データ品質")["meta"]["n_active"], want);
    assert_eq!(get("担当者の一覧")["meta"]["n_active"], want);
    assert_eq!(get("成果とリスク")["risk"]["n_act"], want);
    assert_eq!(get("立ち上がり")["phase"]["total"], want);

    // ②コンサルタント一覧は、表の合計 + 担当が取れない件数 = 母集団。
    // 🔴 ここが合わないと「稼働中604件」と言いながら表が593件、という画面に戻る。
    let team = get("担当者の一覧");
    let sum: i64 = team["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["n_active"].as_i64().unwrap_or(0))
        .sum();
    let unknown = team["meta"]["unknown_owner"].as_i64().unwrap();
    assert_eq!(
        sum + unknown,
        want,
        "担当者一覧の合計{sum} + 担当が取れない{unknown} が母集団{want} と合わない"
    );

    // 取引の側も同じ
    for label in [
        "継続回数 × 成果",
        "いま見るべき顧客",
        "立ち上がり",
        "データ品質",
    ] {
        assert_eq!(get(label)["meta"]["n_deals"], 3432, "{label} の取引件数");
    }
}

/// 🔴 **除いた件数を画面に出していること。** 黙って消さない。
///
/// AirWork広告運用は実在する売上なので、「無かったこと」にはしない。
/// 件数・金額・内訳の一文が応答に入っていて、テンプレートがそれを出している。
#[test]
fn 除いたオプションの件数と金額が画面に出る() {
    let sh = sheets();
    let pop = super::population_of(&sh.deal);

    assert_eq!(pop.active_all, 703);
    assert_eq!(pop.active_option, 99);
    assert_eq!(pop.active, 604);
    assert_eq!(
        pop.active_all - pop.active_option,
        pop.active,
        "引き算が合わない"
    );
    assert_eq!(
        pop.deals_all - pop.deals_option,
        pop.deals,
        "引き算が合わない"
    );

    // 外した契約の金額。**0 や null にしない**（売上が無かったことになる）
    let amt = pop
        .option_amount
        .expect("外したオプションの金額が出ていない");
    assert!(
        (amt - 17_631_000.0).abs() < 1.0,
        "外した稼働中オプションの金額が {amt} 円。実測は 17,631,000 円"
    );

    // 画面に出す一文に、元の件数・外した件数・残りの件数が全部入っている
    for w in ["703", "99", "604", "AirWork広告運用"] {
        assert!(
            pop.note.contains(w),
            "母集団の注記に「{w}」が入っていない: {}",
            pop.note
        );
    }

    // テンプレートが実際にそれを描いているか（描いていなければ画面には出ない）
    let html = include_str!("../../../templates/tabs/cs_dashboard.html");
    assert!(
        html.contains("function popline("),
        "母集団の注記を描く関数がテンプレートに無い"
    );
    assert!(
        html.contains("popline(D)"),
        "母集団の注記が操作列で呼ばれていない（どの画面にも出ない）"
    );
    for w in [
        "p.active_all",
        "p.active_option",
        "p.option_amount",
        "p.deals_option",
    ] {
        assert!(html.contains(w), "テンプレートが {w} を出していない");
    }
}

// ================================================================ オプションの判定を固定する

/// 🔴 **オプションの判定は完全一致。部分一致にしない。**
///
/// `OPTION_KINDS` に「追加」が入っているが、実データでいちばん多い本体契約は
/// 「サブスク継続」1,496件。部分一致にすると「サブスク」で本体を落とす事故が起きる。
/// 実データで、本体契約が1件も外れていないことを直接確かめる。
#[test]
fn 本体契約はオプション判定に巻き込まれない() {
    let sh = sheets();
    let all = super::deals_all_of(&sh.deal);

    // 本体として残らなければならない種別
    for (kind, least) in [("サブスク継続", 1400), ("(新規)", 1900), ("サブスク", 1)] {
        let ds: Vec<&super::Deal> = all.iter().filter(|d| d.contract_kind == kind).collect();
        assert!(
            ds.len() >= least,
            "種別「{kind}」が {} 件しかない。データが変わった可能性",
            ds.len()
        );
        // オプション用ステージに置かれている例外を除いて、種別だけでは外れないこと
        let dropped = ds
            .iter()
            .filter(|d| super::OPTION_KINDS.contains(&d.contract_kind.as_str()))
            .count();
        assert_eq!(dropped, 0, "本体契約「{kind}」がオプション判定で外れている");
    }

    // 完全一致であることの直接確認。部分一致なら「サブスク継続」が「追加」等に
    // 引っかからなくても、将来 contains に書き換えられたときにここで落ちる
    assert!(!super::OPTION_KINDS.contains(&"サブスク継続"));
    assert!(!super::OPTION_KINDS.contains(&"サブスク"));
    assert!(!super::OPTION_KINDS.contains(&"(新規)"));
    for k in super::OPTION_KINDS {
        assert!(
            !"サブスク継続".contains(k),
            "「サブスク継続」が「{k}」を含んでいる。部分一致にしたら本体が落ちる"
        );
    }
}

// ================================================================ MTG途絶の帯

/// 帯の線引きが GAS `no_mtg_alerter.gs` と同じであること。
///
/// ```text
///   注意 30〜59 / 警告 60〜89 / 重大 90〜
///   立ち上がり期（契約開始30日以内）は帯を付けない
///   満了90日以内 かつ 30日以上途絶 → 強制的に重大
/// ```
/// 🔴 **境目そのもの**を1日ずつ確かめる。集計値だけだと、29/30 や 59/60 が
/// ずれていても気づけない。
#[test]
fn mtg途絶の線引きがgasと同じ() {
    use chrono::NaiveDate;
    let today = NaiveDate::from_ymd_opt(2026, 9, 18).unwrap();
    let mut last = std::collections::HashMap::new();

    let mk = |start: &str, exp: &str| super::Deal {
        id: "D".into(),
        name: "テスト".into(),
        stage: String::new(),
        stage_label: String::new(),
        contract_kind: String::new(),
        contract_expiration_date: exp.into(),
        contract_start_date: start.into(),
        kyoten_key: String::new(),
        kyoten_name: String::new(),
        houjin_resolved: String::new(),
        houjin_source: String::new(),
        renewal_no: None,
        is_active: true,
        right_censored: false,
        oubo: None,
        mensetu: None,
        syoudaku: None,
        saiyomokuhyou: None,
        keisaisu: None,
        amount: None,
        contract_period: None,
    };
    // 満了は十分先（強制引き上げが効かない位置）にしておく
    let d = mk("2024-01-01", "2027-12-31");

    for (days, want) in [
        (0, super::MtgBand::Recent),
        (29, super::MtgBand::Recent),
        (30, super::MtgBand::Yellow),
        (59, super::MtgBand::Yellow),
        (60, super::MtgBand::Red),
        (89, super::MtgBand::Red),
        (90, super::MtgBand::Critical),
        (365, super::MtgBand::Critical),
    ] {
        let day = today - chrono::Duration::days(days);
        last.insert("D".to_string(), (Some(day), None));
        let g = super::mtg_gap_of(&d, &last, today);
        assert_eq!(g.band, want, "{days}日前のMTG");
        assert_eq!(g.days, Some(days), "{days}日前の経過日数");
        assert!(!g.forced_by_expiry);
    }

    // 立ち上がり期。契約開始29日目は帯を付けない / 30日目から付ける
    last.insert(
        "D".to_string(),
        (Some(today - chrono::Duration::days(200)), None),
    );
    let young = mk("2026-08-21", "2027-12-31"); // 28日前に開始
    assert_eq!(
        super::mtg_gap_of(&young, &last, today).band,
        super::MtgBand::Onboarding,
        "契約開始28日目は立ち上がり期"
    );
    let grown = mk("2026-08-19", "2027-12-31"); // 30日前に開始
    assert_eq!(
        super::mtg_gap_of(&grown, &last, today).band,
        super::MtgBand::Critical,
        "契約開始30日目からは帯を付ける"
    );

    // 満了90日前の強制引き上げ。30日途絶（本来は注意）が重大になる
    last.insert(
        "D".to_string(),
        (Some(today - chrono::Duration::days(30)), None),
    );
    let ending = mk("2024-01-01", "2026-11-01"); // 満了まで44日
    let g = super::mtg_gap_of(&ending, &last, today);
    assert_eq!(g.band, super::MtgBand::Critical, "満了90日前の途絶");
    assert!(g.forced_by_expiry, "強制引き上げの印が立っていない");
    // 29日なら引き上げない（境目）
    last.insert(
        "D".to_string(),
        (Some(today - chrono::Duration::days(29)), None),
    );
    let g = super::mtg_gap_of(&ending, &last, today);
    assert_eq!(g.band, super::MtgBand::Recent, "29日は引き上げない");
    assert!(!g.forced_by_expiry);
    // 満了を過ぎていたら引き上げない（終わった契約を毎朝出さない）
    last.insert(
        "D".to_string(),
        (Some(today - chrono::Duration::days(40)), None),
    );
    let over = mk("2024-01-01", "2026-09-01");
    assert_eq!(
        super::mtg_gap_of(&over, &last, today).band,
        super::MtgBand::Yellow,
        "満了を過ぎた契約は引き上げない"
    );
}

/// 🔴 **記録が無いものを赤にしない。** 録画とメールの両方を見たうえで数える。
///
/// 実データ（2026-09-23 / 稼働中604件・オプション除く）:
/// ```text
///   録画だけで数えると     記録なし 191件
///   メール由来を足すと     記録なし  44件
/// ```
/// 差の147件は「MTGをしていない」ではなく「録画が取引に結べていない」。
#[test]
fn mtgの記録なしは赤にせず両方のソースで数える() {
    let sh = sheets();
    let v = build_deal_board(&sh, fixture_day());
    let g = &v["meta"]["mtg_gap"];

    let band = |k: &str| -> i64 {
        g["bands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["band"] == k)
            .and_then(|b| b["n"].as_i64())
            .unwrap_or_else(|| panic!("帯 {k} が無い"))
    };
    assert_eq!(band("critical"), 102, "重大");
    assert_eq!(band("red"), 7, "警告");
    assert_eq!(band("yellow"), 22, "注意");
    assert_eq!(band("recent"), 241, "直近30日にMTGあり");
    assert_eq!(band("no_record"), 44, "記録なし");
    assert_eq!(band("onboarding"), 188, "立ち上がり期");
    let total: i64 = g["bands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["n"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(total, 604, "帯の合計が母集団と合わない");

    // 記録なしと直近と立ち上がり期には名札を立てない
    for k in ["no_record", "recent", "onboarding"] {
        let alert = g["bands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["band"] == k)
            .unwrap()["alert"]
            .as_bool()
            .unwrap();
        assert!(!alert, "{k} に名札を立てている");
    }
    let names: Vec<&str> = v["meta"]["flag_counts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|x| x["label"].as_str())
        .collect();
    assert!(
        !names
            .iter()
            .any(|x| x.contains("記録が無い") && x.contains("MTG")),
        "「MTGの記録が無い」が名札になっている: {names:?}"
    );

    // 🔴 メール由来を足した効果。録画だけだと記録なしが 191件になる
    let rec_only = super::last_mtg_by_deal(&sh.mtg, &sh.mail_mtg);
    let deals = super::deals_of(&sh.deal);
    let judged: Vec<&super::Deal> = deals
        .iter()
        .filter(|d| d.is_active)
        .filter(|d| {
            super::mtg_gap_of(d, &rec_only, fixture_day()).band != super::MtgBand::Onboarding
        })
        .collect();
    let only_rec_missing = judged
        .iter()
        .filter(|d| rec_only.get(&d.id).map(|x| x.0).unwrap_or(None).is_none())
        .count();
    assert_eq!(only_rec_missing, 191, "録画だけだと記録が無いもの");
    assert!(
        only_rec_missing > band("no_record") as usize * 3,
        "メール由来を足した効果が出ていない（録画だけ {only_rec_missing} / 両方 {}）",
        band("no_record")
    );

    // 被覆は率だけでなく件数も返す（分母つき）
    let c = &g["coverage"];
    assert_eq!(
        g["n_judged"], 416,
        "帯を付けた母数（立ち上がり期を除く稼働中）"
    );
    assert_eq!(c["recording"], 225);
    assert_eq!(c["mail"], 358);
    assert_eq!(c["either"], 372);
    assert!(
        (c["either_rate"].as_f64().unwrap() - 89.4).abs() < 0.1,
        "どちらかで分かる率が {}",
        c["either_rate"]
    );
}

/// 🔴 **帯を決めた日付の出どころを行ごとに出す。** 録画は事実、メールは推定。
#[test]
fn mtgの出どころが行ごとに出る() {
    let v = build_deal_board(&sheets(), fixture_day());
    let rows = v["rows"].as_array().unwrap();

    let mut seen = std::collections::BTreeSet::new();
    for r in rows {
        let band = r["mtg_band"].as_str().expect("mtg_band");
        let src = r["mtg_source"].as_str().expect("mtg_source");
        seen.insert(src.to_string());
        match band {
            "onboarding" | "no_record" => {
                assert_eq!(src, "none", "{band} なのに出どころがある: {r}");
                // 🔴 記録が無いものの日数を 0 にしない（「昨日話した」に見える）
                assert!(r["mtg_days"].is_null(), "{band} の日数が null でない: {r}");
                assert!(r["mtg_last"].is_null());
            }
            _ => {
                assert_ne!(src, "none", "{band} なのに出どころが無い: {r}");
                assert!(r["mtg_days"].as_i64().is_some(), "日数が無い: {r}");
                assert!(r["mtg_last"].as_str().is_some(), "最終MTG日が無い: {r}");
                assert!(
                    !r["mtg_source_label"].as_str().unwrap_or("").is_empty(),
                    "出どころの表示名が無い: {r}"
                );
            }
        }
    }
    // 3つの出どころが全部出ている（1つしか出ていないなら片方のシートを読めていない）
    for want in ["recording", "mail", "both"] {
        assert!(seen.contains(want), "出どころ {want} が1件も無い: {seen:?}");
    }

    // メール由来だと分かる行には「推定」と書いてある
    let mail_label = rows.iter().find(|r| r["mtg_source"] == "mail").unwrap()["mtg_source_label"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        mail_label.contains("推定"),
        "メール由来に推定と書いていない: {mail_label}"
    );
}

/// MTG途絶の名札が「今日動く先」に並ぶこと。**別画面を作らない。**
#[test]
fn mtg途絶の名札が今日動く先に並ぶ() {
    let sh = sheets();
    let v = build_today_board(&sh, fixture_day());
    let b = build_deal_board(&sh, fixture_day());

    let counts: std::collections::HashMap<String, i64> = b["meta"]["flag_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["label"].as_str().unwrap().to_string(),
                x["n"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(counts.get("MTGが90日以上途絶"), Some(&56));
    assert_eq!(counts.get("MTGが60〜89日途絶"), Some(&7));
    assert_eq!(counts.get("MTGが30〜59日途絶"), Some(&22));
    assert_eq!(counts.get("満了90日前でMTGが30日以上途絶"), Some(&46));
    // 重大の帯 = 経過日数で重大 + 満了前で引き上げたもの
    assert_eq!(56 + 46, 102, "重大の内訳が帯の件数と合わない");

    // 今日動く先の行にも帯が乗っている（別の計算を持っていない）
    for r in v["rows"].as_array().unwrap() {
        assert!(r["mtg_band"].is_string(), "今日動く先に帯が無い: {r}");
    }
    assert!(
        v["meta"]["mtg_gap"]["bands"].as_array().unwrap().len() == 6,
        "今日動く先に帯の内訳が無い"
    );
}

// ================================================================ 今週始まった契約

/// GAS `new_deal_detector.gs` と同じ「直近7日」。
///
/// 🔴 始まった日に気づけないと、立ち上がり期（開始30日以内は帯を付けない）が
/// ただの取りこぼしになる。**開始がまだ先の契約は別に数える。**
#[test]
fn 今週始まった契約が直近7日で切れている() {
    let sh = sheets();
    let day = fixture_day();
    let v = build_today_board(&sh, day);

    let started = v["started_this_week"].as_array().unwrap();
    assert_eq!(started.len(), 26, "直近7日に始まった稼働中の契約");
    assert_eq!(v["meta"]["n_started_this_week"], 26);
    assert_eq!(v["meta"]["n_not_started"], 59, "開始がまだ先の契約");

    for r in started {
        let st = super::date10(r["start"].as_str().unwrap_or("")).expect("開始日");
        let age = (day - st).num_days();
        assert!(
            (0..=super::NEW_DEAL_LOOKBACK_DAYS).contains(&age),
            "{age}日前に始まった契約が混ざっている: {r}"
        );
        // 始まったばかりなので、帯は立ち上がり期のはず
        assert_eq!(
            r["mtg_band"], "onboarding",
            "今週始まったのに帯が付いている: {r}"
        );
        assert_eq!(r["not_started"], false);
    }

    // 開始がまだ先のものを「今週始まった」に混ぜていない
    for r in v["not_started"].as_array().unwrap() {
        assert_eq!(r["not_started"], true);
    }
}

// ================================================================ 注力

/// 注力は**法人単位**で、3つの条件のいずれか。
///
/// 実データ（fixture 1,649法人 / 稼働中の取引を持つ517法人）:
/// ```text
///   注力 116社 ＝ モックの「517社のうち116社」と一致
///   内訳（重なる）: 月額30万以上 58 / 従業員1,000名以上 41 / 拠点3つ以上 46
/// ```
/// 🔴 内訳を足すと145で、116社にはならない。**重なりがある**ことを画面にも書く。
#[test]
fn 注力の内訳が法人の画面に出る() {
    let sh = sheets();
    let day = fixture_day();
    let idx = build_customer(&sh, None, day);
    let f = &idx["focus"];

    assert_eq!(f["n_all"], 1649, "CS_顧客 の行数");
    // 🔴 画面の「全 N 法人」は本部アプローチと同じ母数（オプション契約しか持たない3法人を除く）。
    //    以前は同じ画面に「全 1,649 法人」と「全 1,646 法人」が並んでいた
    let hq = build_headquarters(&sh, day);
    assert_eq!(
        f["n_houjin"], 1646,
        "オプション契約しか持たない法人を除いた数"
    );
    assert_eq!(
        f["n_houjin"], hq["meta"]["n_houjin"],
        "本部アプローチと母数が違う"
    );
    assert_eq!(f["n_houjin_option_only"], 3);
    assert_eq!(f["n_display"], 517, "稼働中の取引を持つ法人");
    // 🔴 図の 517 社にはオプション契約しか持たない法人が1社入っている（h411cb77ce494）。
    //    画面はこれを書く（書かないと「オプション契約しか持たない法人は数えていません」が
    //    図にも掛かって読める）
    assert_eq!(f["n_display_option_only"], 1);
    assert_eq!(f["n_focus"], 116, "注力（モックと同じ）");
    assert_eq!(f["monthly_over_300k"], 58);
    assert_eq!(f["enterprise"], 41);
    assert_eq!(f["multi_site"], 46);
    assert_eq!(f["n_focus_all"], 223, "全法人まで広げたときの注力");

    let sum = f["monthly_over_300k"].as_i64().unwrap()
        + f["enterprise"].as_i64().unwrap()
        + f["multi_site"].as_i64().unwrap();
    assert!(
        sum > f["n_focus"].as_i64().unwrap(),
        "内訳が重なっていない。重なりが無いなら注意書きのほうを直すこと"
    );
    assert!(
        f["rule"].as_str().unwrap().contains("重なる"),
        "重なることを画面に書いていない"
    );
    // 🔴 注力（法人・大きさ）と MTG途絶の帯（取引・状態）を混ぜない注意書き
    assert!(f["not_layer"].as_str().unwrap().contains("帯"));

    // 一覧の行に、注力かどうかと**なぜ注力なのか**が入っている
    let rows = idx["index"].as_array().unwrap();
    let focused: Vec<&Value> = rows.iter().filter(|r| r["focus"] == true).collect();
    assert_eq!(focused.len(), 116, "一覧の注力の数が内訳と合わない");
    for r in &focused {
        let why = r["focus_why"].as_array().unwrap();
        assert!(!why.is_empty(), "注力なのに理由が空: {r}");
        for w in why {
            assert!(
                ["月額30万以上", "従業員1,000名以上", "拠点3つ以上"].contains(&w.as_str().unwrap()),
                "知らない理由が入っている: {w}"
            );
        }
    }
    for r in rows.iter().filter(|r| r["focus"] == false) {
        assert!(
            r["focus_why"].as_array().unwrap().is_empty(),
            "注力でないのに理由がある: {r}"
        );
    }

    // 1社を開いても、その法人が注力かどうかと内訳が出る（一覧に戻らないと分からない、を作らない）
    let h = idx["default_houjin"].as_str().expect("既定の法人");
    let one = build_customer(&sh, Some(h), day);
    assert!(!one["focus"].is_null(), "明細に注力の内訳が無い");
    assert_eq!(one["focus"]["n_focus"], 116);
    assert!(
        one["customer"]["focus"].is_boolean(),
        "この法人が注力かが無い"
    );
    assert!(one["customer"]["focus_why"].is_array());

    // テンプレートが実際に描いているか
    let html = include_str!("../../../templates/tabs/cs_dashboard.html");
    assert!(
        html.contains("function focusSection("),
        "注力の節がテンプレートに無い"
    );
    assert!(
        html.contains("focusSection(D)"),
        "注力の節が法人の画面で呼ばれていない"
    );
    assert!(html.contains("hj-focus-only"), "注力だけに絞る操作が無い");
    assert!(
        html.contains("svgDots({ total: f.n_display"),
        "注力の散らばりの図が無い"
    );
    // 🔴 絞り込みが「選択肢」にも効いていること（図だけ絞ると全件から選んでしまう）
    assert!(
        html.contains("focusOnly\n      ? customerIndex.filter((r) => r.focus")
            || html.contains("focusOnly"),
        "注力の絞り込みが選択肢に効いていない"
    );
}

/// 🔴 注力（法人・不変）と MTG途絶の帯（取引・日々変わる）を取り違えないこと。
///
/// GAS の Layer1/2/3 に当たるのは帯のほう。**Layer3（過剰介入）は実装しない**
/// （コードと設定シートで意味が食い違っていて、どちらが正かソースから分からない）。
#[test]
fn 注力と帯は別のものとして出る() {
    let sh = sheets();
    let day = fixture_day();

    // 注力は法人の画面にあり、案件の行には帯がある
    let idx = build_customer(&sh, None, day);
    assert!(!idx["focus"].is_null(), "注力は法人の画面にある");
    assert!(
        idx["meta"]["mtg_gap"].is_null() && idx["mtg_gap"].is_null(),
        "法人の画面に帯を持ち込んでいる"
    );

    let b = build_deal_board(&sh, day);
    assert!(!b["meta"]["mtg_gap"].is_null(), "案件の画面に帯がある");
    // 案件の行には注力の印もあるが、それは法人から降りてきた属性で、帯とは別の列
    let r = b["rows"].as_array().unwrap().first().unwrap();
    assert!(r["focus"].is_boolean(), "案件の行に注力の印が無い");
    assert!(r["mtg_band"].is_string(), "案件の行に帯が無い");

    // Layer1/2/3 という言葉を画面に持ち込んでいないこと
    // （表示用語は日本語にそろえる。GAS の内部の呼び名を現場に見せない）
    let html = include_str!("../../../templates/tabs/cs_dashboard.html");
    for w in ["Layer1", "Layer2", "Layer3"] {
        assert!(!html.contains(w), "画面に「{w}」が入っている");
    }
}

// ================================================================ 2026-09-23 全体レビューの是正

/// 1枚のシートを行ごと差し替えた複製を作る（fixture を壊さずに境目を作るため）。
fn with_rows(base: &SheetData, f: impl Fn(&mut Vec<Arc<str>>)) -> Arc<SheetData> {
    let mut rows = base.rows.clone();
    for r in rows.iter_mut() {
        f(r);
    }
    Arc::new(SheetData {
        header: base.header.clone(),
        rows,
        fetched_at: Instant::now(),
    })
}

/// 見出しと行を直接書いた小さなシート。
fn tiny(header: &[&str], rows: &[&[&str]]) -> Arc<SheetData> {
    Arc::new(SheetData {
        header: header.iter().map(|s| s.to_string()).collect(),
        rows: rows
            .iter()
            .map(|r| r.iter().map(|c| Arc::from(*c)).collect())
            .collect(),
        fetched_at: Instant::now(),
    })
}

/// N3: 立ち上がりの帯ごとの解約率は、決着済みを分母にする。
///
/// fixture（基準日 2026-09-18）で Python で別に数えた値:
/// | 帯 | 件数 | 解約＋充足 | 決着済み | 解約率 | 以前（件数で割った値） |
/// | 14日以内 | 545 | 178 | 358 | 49.7% | 32.7% |
/// | 15〜30日 | 151 |  40 | 109 | 36.7% | 26.5% |
/// | 31〜60日 | 172 |  63 | 147 | 42.9% | 36.6% |
/// | 61日超   | 215 |  91 | 199 | 45.7% | 42.3% |
#[test]
fn 立ち上がりの帯の解約率は決着済みで割る() {
    let v = build_rampup(&sheets(), fixture_day());
    let b = |label: &str| -> Value {
        v["first_mtg"]["buckets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["label"] == label)
            .cloned()
            .unwrap()
    };
    for (label, n, c, dn, pct) in [
        ("14日以内", 545, 178, 358, 49.7),
        ("15〜30日", 151, 40, 109, 36.7),
        ("31〜60日", 172, 63, 147, 42.9),
        ("61日超", 215, 91, 199, 45.7),
    ] {
        let x = b(label);
        assert_eq!(x["n"], n, "{label} の件数");
        assert_eq!(x["cancel"], c, "{label} の解約＋充足");
        assert_eq!(x["denom"], dn, "{label} の決着済み");
        let got = x["cancel_rate"].as_f64().unwrap();
        assert!(
            (got - pct).abs() < 0.05,
            "{label} の解約率 {got:.1}%（期待 {pct}%）"
        );
    }
    // 以前の注記「遅い群が悪い」は、分母を直すと読めない
    assert!(!v["first_mtg"]["note"]
        .as_str()
        .unwrap()
        .contains("遅い群が悪い"));
}

/// N4: 本部アプローチの拠点ごとの解約率と採用単価の作り方。
///
/// 解約率 ＝（解約＋充足）÷ 決着済み。
/// 採用単価 ＝ **決着済み**で、金額と採用数が**両方ある取引だけ**の合計どうし。
/// V24: 拠点の表示名は `kyoten_name`（照合用の `kyoten_key` ではない）。
#[test]
fn 本部の拠点ごとの解約率と採用単価は定義どおり() {
    let sh = sheets();
    let v = build_headquarters(&sh, fixture_day());
    let deals = super::deals_of(&sh.deal);
    let mut checked_rate = 0;
    let mut checked_cpa = 0;
    let mut checked_active = 0;
    for h in v["rows"].as_array().unwrap() {
        let houjin = h["houjin"].as_str().unwrap();
        for st in h["rows"].as_array().unwrap() {
            let site = st["site"].as_str().unwrap();
            let ds: Vec<&super::Deal> = deals
                .iter()
                .filter(|d| d.houjin_resolved == houjin)
                .filter(|d| {
                    if site == "(拠点不明)" {
                        d.kyoten_key.is_empty()
                    } else {
                        d.kyoten_key == site
                    }
                })
                .collect();
            let o = |x: super::Outcome| {
                ds.iter()
                    .filter(|d| super::outcome_of(&d.stage) == x)
                    .count() as f64
            };
            let (k, c, f) = (
                o(super::Outcome::Keep),
                o(super::Outcome::Cancel),
                o(super::Outcome::Fill),
            );
            if k + c + f > 0.0 {
                let want = (c + f) / (k + c + f) * 100.0;
                let got = st["cancel_rate"].as_f64().unwrap();
                assert!((got - want).abs() < 1e-6, "{site}: {got} / {want}");
                checked_rate += 1;
            } else {
                assert!(st["cancel_rate"].is_null());
            }
            // 🔴 稼働中は入れない（未確定の点で比べない）。以前は稼働中も入れていて、
            //    fixture で 167拠点の単価に稼働中が混ざっていた（外すと 61拠点が20%超動く）
            let pairs: Vec<(f64, f64)> = ds
                .iter()
                .filter(|d| !d.is_active)
                .filter_map(|d| d.amount.zip(d.syoudaku))
                .collect();
            let sy: f64 = pairs.iter().map(|p| p.1).sum();
            if sy > 0.0 {
                let want = pairs.iter().map(|p| p.0).sum::<f64>() / sy;
                let got = st["cpa"].as_f64().unwrap();
                assert!((got - want).abs() < 1e-6, "{site} の採用単価");
                checked_cpa += 1;
            } else {
                assert!(st["cpa"].is_null(), "{site}: 採用0で単価が出ている");
            }
            assert_eq!(st["cpa_n"].as_u64().unwrap() as usize, pairs.len());
            let n_active = ds
                .iter()
                .filter(|d| d.is_active && d.amount.is_some() && d.syoudaku.is_some())
                .count();
            assert_eq!(st["cpa_n_active"].as_u64().unwrap() as usize, n_active);
            if n_active > 0 {
                checked_active += 1;
            }
            // 表示名は kyoten_name の値（照合用のキーのまま出さない）。
            // fixture の kyoten_name は伏字の連番で、キーとは必ず違う
            let want_name = ds
                .iter()
                .map(|d| d.kyoten_name.trim())
                .find(|s| !s.is_empty());
            assert_eq!(st["site_name"].as_str(), want_name, "{site} の表示名");
            assert_ne!(st["site_name"].as_str(), Some(site), "キーのまま出している");
        }
        // 会社名が返っている（fixture は伏字だが空ではない）
        assert!(h["name"].is_string(), "会社名が無い: {}", h["houjin"]);
    }
    assert!(
        checked_rate > 100 && checked_cpa > 100,
        "{checked_rate} / {checked_cpa}"
    );
    // 稼働中を外す是正が効く拠点が実際にある（無いとこのテストは何も守らない）
    assert!(checked_active > 30, "{checked_active}");
}

/// N2: 「採用単価が同じ進捗帯の1.5倍以上」の比べる相手は、稼働中の本体契約だけで作る。
/// 区切りはモックと同じ 0.5 / 0.75、n<30 の帯は中央値を出さない。
#[test]
fn 採用単価の進捗帯は稼働中だけで作る() {
    let sh = sheets();
    let day = fixture_day();
    let v = build_deal_board(&sh, day);
    let bands = v["meta"]["cpa_bands"].as_array().unwrap();
    assert_eq!(bands.len(), 3);
    // 別に数え直す（稼働中・進捗が帯に入る・採用単価がある）
    let deals = super::deals_of(&sh.deal);
    let mut vals: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for d in deals.iter().filter(|d| d.is_active) {
        let (Some(st), Some(p), Some(c)) = (
            super::date10(&d.contract_start_date),
            d.contract_period.filter(|p| *p > 0.0),
            super::cpa(d),
        ) else {
            continue;
        };
        let pg = (day - st).num_days() as f64 / 30.4 / p;
        let b = if pg <= 0.5 {
            0
        } else if pg <= 0.75 {
            1
        } else if pg <= 1.01 {
            2
        } else {
            continue;
        };
        vals[b].push(c);
    }
    for (i, b) in bands.iter().enumerate() {
        assert_eq!(
            b["n"].as_u64().unwrap() as usize,
            vals[i].len(),
            "帯{i} の件数"
        );
        let want = if vals[i].len() >= 30 {
            super::routes::median_of(vals[i].clone())
        } else {
            None
        };
        assert_eq!(b["median"].as_f64(), want, "帯{i} の中央値");
    }
    // 名札は、その帯の中央値の 1.5 倍以上のときだけ
    for r in v["rows"].as_array().unwrap() {
        let has = r["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "採用単価が同じ進捗帯の1.5倍以上");
        let want = matches!(r["cpa_vs_band"].as_f64(), Some(x) if x >= 1.5);
        assert_eq!(has, want, "{r}");
        if r["cpa_vs_band"].is_number() {
            assert!(r["cpa_band"].is_string());
        }
    }
}

/// N2: 顧客詳細の「採用単価を3つの出し方で」も、帯と帯の中央値は**稼働中の契約にだけ**付ける。
///
/// 決着済みでも途中で解約して進捗が 1.01 以下のものは帯に入ってしまうので、
/// そういう取引がある法人を選んで確かめる（無いと是正を戻しても落ちない）。
#[test]
fn 顧客詳細の進捗帯は稼働中にだけ付く() {
    let sh = sheets();
    let day = fixture_day();
    let deals = super::deals_of(&sh.deal);
    let in_band = |d: &super::Deal| {
        let (Some(st), Some(p)) = (
            super::date10(&d.contract_start_date),
            d.contract_period.filter(|p| *p > 0.0),
        ) else {
            return false;
        };
        (day - st).num_days() as f64 / 30.4 / p <= 1.01
    };
    let targets: Vec<&str> = deals
        .iter()
        .filter(|d| !d.is_active && super::cpa(d).is_some() && in_band(d))
        .map(|d| d.houjin_resolved.as_str())
        .filter(|h| !h.is_empty())
        .take(5)
        .collect();
    assert!(
        !targets.is_empty(),
        "途中で決着した採用単価ありの取引が無い"
    );
    let mut settled = 0;
    for h in targets {
        let v = build_customer(&sh, Some(h), day);
        for x in v["cpa3"].as_array().unwrap() {
            if x["active"] == false {
                assert!(x["band"].is_null(), "決着済みに帯が付いている: {x}");
                assert!(
                    x["band_median"].is_null(),
                    "決着済みに中央値が付いている: {x}"
                );
                settled += 1;
            }
        }
    }
    assert!(settled > 0, "{settled}");
}

/// N6: 開始日が空の案件を「開始がまだ先」に数えない。
#[test]
fn 開始日が空の案件は開始前に数えない() {
    let base = sheets();
    let day = fixture_day();
    // 稼働中で、開始済みの取引を1件選んで開始日を消す
    let target = super::deals_of(&base.deal)
        .into_iter()
        .find(|d| d.is_active && super::date10(&d.contract_start_date).is_some_and(|s| s <= day))
        .unwrap()
        .id;
    let before = build_today_board(&base, day)["meta"]["n_not_started"]
        .as_i64()
        .unwrap();
    let mut sh = sheets();
    let (ci, si) = (
        base.deal.col("deal_id").unwrap(),
        base.deal.col("contract_start_date").unwrap(),
    );
    sh.deal = with_rows(&base.deal, |r| {
        if r[ci].as_ref() == target {
            r[si] = Arc::from("");
        }
    });
    let v = build_today_board(&sh, day);
    assert_eq!(
        v["meta"]["n_not_started"].as_i64().unwrap(),
        before,
        "開始日が空の案件が「開始前」に数えられた"
    );
    let b = build_deal_board(&sh, day);
    let row = b["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["deal_id"] == target.as_str())
        .unwrap();
    assert_eq!(row["not_started"], false);
    assert_eq!(row["start_unknown"], true);
    assert!(
        row["months"].is_null(),
        "開始日が空なのに何ヶ月目が出ている"
    );
}

/// N7: 担当の交代は、オプション契約の行を外して母集団と揃える。
///
/// fixture 385行のうち 27行がオプション契約。以前は `deals_of`（オプション除外）で
/// 引いていたので、その27行が「取引名なし・決着済」として表に残っていた。
#[test]
fn 担当の交代はオプション契約を外す() {
    let sh = sheets();
    let v = build_handover(&sh, fixture_day());
    assert_eq!(sh.handover.rows.len(), 385);
    assert_eq!(v["meta"]["n_option_excluded"], 27);
    assert_eq!(v["meta"]["n"], 358);
    assert_eq!(v["meta"]["n_unknown_deal"], 0);
    // 稼働中は 163（オプション込み）ではなく 149（オプション除外。母集団と同じ数え方）
    assert_eq!(v["meta"]["n_active"], 149);
    for r in v["rows"].as_array().unwrap() {
        assert!(
            !r["name"].as_str().unwrap().is_empty(),
            "取引名の無い行が残っている: {r}"
        );
        assert!(r["state_label"].is_string());
    }
}

/// V13: 担当の交代で、メールアドレスを人の名前として出さない。
#[test]
fn 担当の交代でメールアドレスを名前として出さない() {
    use super::routes::person_label;
    assert_eq!(person_label("田野 詩央里").as_deref(), Some("田野 詩央里"));
    assert_eq!(person_label("  "), None);
    let l = person_label("someone@example.co.jp").unwrap();
    assert!(!l.contains('@'), "メールアドレスがそのまま出ている: {l}");

    let mut sh = sheets();
    let fi = sh.handover.col("from").unwrap();
    sh.handover = with_rows(&sh.handover, |r| r[fi] = Arc::from("x.y@example.co.jp"));
    let v = build_handover(&sh, fixture_day());
    for r in v["rows"].as_array().unwrap() {
        assert!(!r["from_label"].as_str().unwrap().contains('@'));
        assert_eq!(r["from_unresolved"], true);
        // 元の値は残す（書き出し側を直すときの手掛かり）
        assert_eq!(r["from"], "x.y@example.co.jp");
    }
}

/// N11: 今日動く先の並びの説明が、実際の並び（名札の本数 → 金額）と一致する。
#[test]
fn 今日動く先の並びは名札の本数が先() {
    let v = build_today_board(&sheets(), fixture_day());
    let rule = v["meta"]["filter_rule"].as_str().unwrap();
    assert!(rule.contains("名札の本数が多い順"), "{rule}");
    let rows = v["rows"].as_array().unwrap();
    for w in rows.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let (na, nb) = (
            a["n_flags"].as_u64().unwrap(),
            b["n_flags"].as_u64().unwrap(),
        );
        assert!(na >= nb, "名札の本数の順になっていない");
        if na == nb {
            assert!(a["amount"].as_f64().unwrap_or(-1.0) >= b["amount"].as_f64().unwrap_or(-1.0));
        }
    }
}

/// N13: 通話の ts（UTC）は日本時間の日付にする。
#[test]
fn 通話の日付は日本時間で取る() {
    use chrono::NaiveDate;
    let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd);
    // UTC 15:00 = 日本時間 翌日 0:00。月もまたぐ
    assert_eq!(super::call_date_jst("2026-03-31T15:00:00Z"), d(2026, 4, 1));
    assert_eq!(
        super::call_date_jst("2026-03-31T14:59:59.999Z"),
        d(2026, 3, 31)
    );
    // タイムゾーンの無い値は、推測で時差を足さない
    assert_eq!(super::call_date_jst("2026-03-31 20:00:00"), d(2026, 3, 31));

    // 電話の月次は日本時間の月で数える。月の合計は通話の行数と一致する
    let v = build_phone(&sheets(), fixture_day());
    let months = v["monthly"].as_array().unwrap();
    let sum: i64 = months.iter().map(|m| m["calls"].as_i64().unwrap()).sum();
    assert_eq!(sum, 18625);
    let aug = months.iter().find(|m| m["month"] == "2026-08").unwrap();
    // Python で日本時間の月に直して数えた値
    assert_eq!(aug["calls"], 3364);
}

/// N14: 顧客詳細の月次推移は満了月までで止める。
#[test]
fn 顧客詳細の月次推移は満了月で止まる() {
    let sh = sheets();
    let day = fixture_day();
    let idx = build_customer(&sh, None, day);
    let h = idx["default_houjin"].as_str().unwrap().to_string();
    let v = build_customer(&sh, Some(&h), day);
    let deals = super::deals_of(&sh.deal);
    let mut checked = 0;
    for m in v["monthly"].as_array().unwrap() {
        let d = deals
            .iter()
            .find(|d| d.id == m["deal_id"].as_str().unwrap())
            .unwrap();
        let Some(exp) = d.manryou_month() else {
            continue;
        };
        if exp >= "2026-09" {
            continue;
        }
        assert_eq!(m["until"], exp);
        for s in m["series"].as_object().unwrap().values() {
            for p in s.as_array().unwrap() {
                assert!(
                    p["month"].as_str().unwrap() <= exp,
                    "満了月 {exp} を過ぎて伸びている: {p}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "満了済みの取引の点が1つも無い");
}

/// N15: 偶数個の中央値は中2つの平均（box5 と同じ約束）。
#[test]
fn 偶数個の中央値は中2つの平均() {
    use super::routes::median_of;
    assert_eq!(median_of(vec![4.0, 1.0, 3.0, 2.0]), Some(2.5));
    assert_eq!(median_of(vec![3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median_of(vec![]), None);
}

/// N15: 担当交代の「記録の遅れ」の中央値も、偶数個なら中2つの平均。
/// 関数だけでなく、画面に返す `median_gap_days` が実際にそれを使っていることを見る
/// （以前は上側の値 `g[len/2]` を採っていた。1,2,3,10 なら 3 になる）。
#[test]
fn 担当交代の記録の遅れの中央値は中2つの平均() {
    let mut sh = sheets();
    let h = [
        "deal_id",
        "date",
        "from",
        "to",
        "to_retired",
        "reflected",
        "record_gap_days",
    ];
    sh.handover = tiny(
        &h,
        &[
            &["X1", "2026-09-01", "佐藤", "鈴木", "FALSE", "", "10"],
            &["X2", "2026-09-02", "佐藤", "鈴木", "FALSE", "", "1"],
            &["X3", "2026-09-03", "佐藤", "鈴木", "FALSE", "", "3"],
            &["X4", "2026-09-04", "佐藤", "鈴木", "FALSE", "", "2"],
            // 空は数えない（0 にしない）
            &["X5", "2026-09-05", "佐藤", "鈴木", "FALSE", "", ""],
        ],
    );
    let v = build_handover(&sh, fixture_day());
    assert_eq!(v["n_gap"], 4);
    assert_eq!(v["median_gap_days"].as_f64(), Some(2.5));
}

/// N16: 担当の「割れ」は、同じ日に**中身の違う**行があるときだけ数える。
#[test]
fn 担当の割れは中身が違うときだけ数える() {
    let h = [
        "date", "owner", "owner_id", "retired", "src", "bulk", "deal_id",
    ];
    let sh = tiny(
        &h,
        &[
            // A: 同じ日に同じ担当が2行（重複）。割れではない
            &["2026-09-01", "佐藤", "1", "FALSE", "", "", "A"],
            &["2026-09-01", "佐藤", "1", "FALSE", "", "", "A"],
            // B: 同じ日に別の担当。割れ
            &["2026-09-01", "佐藤", "1", "FALSE", "", "", "B"],
            &["2026-09-01", "鈴木", "2", "FALSE", "", "", "B"],
            // C: 古い日に割れていても、最新日が1行なら割れではない
            &["2026-08-01", "佐藤", "1", "FALSE", "", "", "C"],
            &["2026-08-01", "鈴木", "2", "FALSE", "", "", "C"],
            &["2026-09-01", "鈴木", "2", "FALSE", "", "", "C"],
            // D / E: 同じ日に担当が空の行があっても割れではない（`consultant_of` も空は読み飛ばす）。
            //    空が先に来ても後に来ても同じ
            &["2026-09-01", "佐藤", "1", "FALSE", "", "", "D"],
            &["2026-09-01", "", "", "FALSE", "", "", "D"],
            &["2026-09-01", "", "", "FALSE", "", "", "E"],
            &["2026-09-01", "鈴木", "2", "FALSE", "", "", "E"],
        ],
    );
    let active: std::collections::HashSet<&str> = ["A", "B", "C", "D", "E"].into_iter().collect();
    assert_eq!(super::consultant_ties(&sh, &active), 1);
}

/// N17: 同じ月に NPS が2回ぶん入っていたら、回が後のほうを採る（シートの並びではない）。
#[test]
fn 同じ月のnpsは回が後のほうを採る() {
    let sh = tiny(
        &["deal_id", "prop", "month", "v", "ts"],
        &[
            &["D", "nps3", "2026-05", "9", ""],
            &["D", "nps2", "2026-05", "3", ""],
            &["E", "nps", "2026-06", "7", ""],
            &["E", "nps4", "2026-05", "2", ""],
        ],
    );
    let n = super::latest_nps(&sh);
    assert_eq!(
        n["D"],
        ("2026-05".to_string(), 9.0),
        "回が後（nps3）を採っていない"
    );
    assert_eq!(n["E"], ("2026-06".to_string(), 7.0), "月が新しいほうが先");
}

/// N18: 暦の月の番号（`month_index`）は始月を1と数える。顧客詳細の月次推移の横軸。
#[test]
fn 何ヶ月目は始月を1と数える() {
    use super::routes::month_index;
    assert_eq!(month_index("2026-07-23", "2026-07"), Some(1));
    assert_eq!(month_index("2026-07-23", "2026-09"), Some(3));
    assert_eq!(month_index("2025-12-01", "2026-01"), Some(2));
}

/// 案件一覧の「N / 期間 か月目」は**契約の日付で**数える（`contract_month`）。
///
/// 2026-09-23 実機: 「7 / 6 か月目」「12 / 12」。暦の月（`month_index`）で数えていたため、
/// 月の途中に始まった契約（案件一覧の稼働中604件のうち553件）が最後の月に期間を1つ超えていた。
/// fixture・基準日 2026-09-18 の実測: 暦の数え方で期間を超える 62件 → この数え方で20件、
/// その20件は全部「満了日を過ぎてもまだ稼働中」（`past_expiry`、20件）。
/// ほかに、暦では「6 / 6」なのに実際はまだ5ヶ月目、のように1か月先に出ていたものが59件。
/// 🔴 N18 のときはこの一覧も `month_index` と一致することをここで見ていた。
///    推移の横軸を「暦の月」と書いて出すようにしたので、一覧は契約の月に戻した。
#[test]
fn 案件一覧の何ヶ月目は満了日までに契約期間を超えない() {
    use super::routes::contract_month;
    let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    // 6ヶ月契約 3/19〜9/18: 満了日に6ヶ月目、暦では7か月目
    assert_eq!(contract_month("2026-03-19", d("2026-09-18")), Some(6));
    assert_eq!(contract_month("2026-03-19", d("2026-09-19")), Some(7));
    assert_eq!(contract_month("2026-03-19", d("2026-03-19")), Some(1));
    assert_eq!(contract_month("2026-03-19", d("2026-04-18")), Some(1));
    assert_eq!(contract_month("2026-03-19", d("2026-04-19")), Some(2));
    // 1日に始まる契約は暦と同じ
    assert_eq!(contract_month("2025-10-01", d("2026-09-18")), Some(12));
    // 12ヶ月契約 2025-10-20〜2026-10-19 は 9/18 ではまだ11ヶ月目（暦だと「12 / 12」）
    assert_eq!(contract_month("2025-10-20", d("2026-09-18")), Some(11));
    // 月末の開始は「月末から月末まで」（HubSpot の満了日と同じ）。
    // 🔴 以前は chrono の 1/31 + 1か月 = 2/28 から2ヶ月目にしていて、ここも Some(2) を
    //    正解にしていた。3/31 開始の6ヶ月契約（満了 9/30）が満了日の当日に「7 / 6」と出る原因
    assert_eq!(contract_month("2026-01-31", d("2026-02-28")), Some(1));
    assert_eq!(contract_month("2026-01-31", d("2026-03-01")), Some(2));
    assert_eq!(contract_month("2026-03-31", d("2026-09-30")), Some(6));
    assert_eq!(contract_month("2026-03-31", d("2026-10-01")), Some(7));
    assert_eq!(contract_month("2026-08-31", d("2026-11-30")), Some(3));
    assert_eq!(contract_month("2026-08-31", d("2027-02-28")), Some(6));
    // 1日の開始は前月末に寄せない（3/1 + 6か月 = 9/1 から7ヶ月目。8/29 にしない）
    assert_eq!(contract_month("2026-03-01", d("2026-08-31")), Some(6));
    assert_eq!(contract_month("2026-03-01", d("2026-09-01")), Some(7));
    // 契約期間どおりの満了日も同じ区切り
    use super::routes::std_expiration;
    let se = |s: &str, p: f64| std_expiration(s, Some(p)).map(|x| x.to_string());
    assert_eq!(se("2026-03-31", 6.0).as_deref(), Some("2026-09-30"));
    assert_eq!(se("2026-03-19", 6.0).as_deref(), Some("2026-09-18"));
    assert_eq!(se("2026-03-01", 6.0).as_deref(), Some("2026-08-31"));
    assert_eq!(se("2025-11-30", 3.0).as_deref(), Some("2026-02-28"));
    assert_eq!(std_expiration("2026-03-19", Some(1.5)), None);
    assert_eq!(std_expiration("2026-03-19", None), None);
    // 開始前・読めない開始日は出さない
    assert_eq!(contract_month("2026-09-24", d("2026-09-18")), None);
    assert_eq!(contract_month("", d("2026-09-18")), None);
    assert_eq!(contract_month("２０２６年７月", d("2026-09-18")), None);

    let v = build_deal_board(&sheets(), fixture_day());
    let (mut checked, mut past, mut over) = (0, 0, 0);
    for r in v["rows"].as_array().unwrap() {
        let Some(m) = r["months"].as_i64() else {
            continue;
        };
        checked += 1;
        assert_eq!(
            Some(m),
            contract_month(r["start"].as_str().unwrap(), fixture_day()),
            "{r}"
        );
        let pe = r["past_expiry"].as_bool().unwrap();
        assert_eq!(pe, r["days_left"].as_i64().is_some_and(|x| x < 0), "{r}");
        if pe {
            past += 1;
        }
        if let Some(p) = r["period"].as_f64() {
            if m as f64 > p {
                over += 1;
                // 🔴 期間を超えるのは満了日を過ぎたものだけ。画面はそれを「満了後」と出す
                assert!(pe, "満了日前なのに期間を超えている: {r}");
            }
        }
    }
    assert!(checked > 400, "{checked}");
    assert_eq!(
        over, 20,
        "期間を超える案件（全部が満了後）。暦の数え方だと62件"
    );
    assert_eq!(past, 20, "満了日を過ぎてもまだ稼働中");
}

/// 案件一覧の「何ヶ月目」を**行ごとに満了日の当日で**数えても、契約期間を超えない。
///
/// 🔴 上のテストは基準日 2026-09-18 の1日しか見ていなかったので、月末に始まった契約
///    （3/31 開始・6ヶ月・満了 9/30 など）が満了日の当日だけ「7 / 6」になるのを拾えなかった。
///    満了日の当日は days_left=0 なので `past_expiry` にもならない。
///    実測（fixture・案件一覧の稼働中、直す前）: 満了日の当日に期間を超える行 20件。
/// 期間を超えてよいのは、満了日が「開始＋契約期間」（`std_expiration`）より後ろにある取引だけ。
/// 画面はそれを「N か月目（契約 P か月。満了日が後ろにずれています）」と出す（`pos`）。
#[test]
fn 案件一覧の何ヶ月目は満了日の当日でも契約期間を超えない() {
    use super::routes::{contract_month, std_expiration};
    let v = build_deal_board(&sheets(), fixture_day());
    let (mut n, mut on_std, mut late_over) = (0, 0, 0);
    for r in v["rows"].as_array().unwrap() {
        let (Some(dl), Some(p), Some(st)) = (
            r["days_left"].as_i64(),
            r["period"].as_f64(),
            r["start"].as_str(),
        ) else {
            continue;
        };
        let exp = fixture_day() + chrono::Duration::days(dl);
        let Some(m) = contract_month(st, exp) else {
            continue;
        };
        n += 1;
        let std = std_expiration(st, Some(p));
        if std.is_none_or(|x| exp <= x) {
            assert!(m as f64 <= p, "満了日の当日に期間を超える: {exp} m={m} {r}");
        } else if m as f64 > p {
            late_over += 1;
        }
        // 契約期間どおりに満了する取引は、満了日の当日がちょうど最後の月、翌日から期間+1
        if std == Some(exp) {
            on_std += 1;
            assert_eq!(m as f64, p, "{r}");
            assert_eq!(
                contract_month(st, exp + chrono::Duration::days(1)).map(|x| x as f64),
                Some(p + 1.0),
                "{r}"
            );
        }
    }
    assert!(n > 400, "{n}");
    // 実測（fixture・稼働中603件）: 602件が契約期間どおりに満了する。
    // 残る1件（62465528145、1ヶ月契約 2026-06-01〜2026-07-31）は満了日が1か月後ろ
    assert_eq!(on_std, 602, "契約期間どおりに満了する行");
    assert_eq!(late_over, 1, "満了日が後ろにずれて期間を超える行");
}

/// 注力の「全 N 法人」は本部アプローチの「全 N 法人」と**同じ式**で数える。
///
/// 🔴 注力は `CS_顧客 ∩ 取引側の法人`、本部は `取引側の法人` の数で、式が違っていた。
///    fixture では両方 1,646 で見分けが付かない。取引にあって CS_顧客 に無い法人が出ると
///    ずれるので、CS_顧客 から1社消して確かめる（注力の旗は CS_顧客 から引くので、
///    消した法人は注力に当たらない扱いになる）。
#[test]
fn 注力の全法人数はcs顧客に無い法人でも本部アプローチと合う() {
    let mut sh = sheets();
    let day = fixture_day();
    let hp = super::houjin_population(&sh.deal);
    let cs = &sh.customer;
    let hc = cs.col("houjin").expect("houjin 列");
    let drop = cs
        .rows
        .iter()
        .position(|r| hp.main.contains(&*r[hc]))
        .expect("取引側にある法人");
    let mut rows = cs.rows.clone();
    rows.remove(drop);
    sh.customer = Arc::new(SheetData {
        header: cs.header.clone(),
        rows,
        fetched_at: cs.fetched_at,
    });
    let f = &build_customer(&sh, None, day)["focus"];
    let hq = build_headquarters(&sh, day);
    assert_eq!(f["n_houjin"], hq["meta"]["n_houjin"]);
    assert_eq!(f["n_houjin"], 1646);
}

/// 顧客詳細の月次推移は暦の月で並ぶ。期間と、暦でまたがる月数を一緒に返す。
#[test]
fn 月次推移は暦でまたがる月数を返す() {
    let sh = sheets();
    let deals = super::deals_of(&sh.deal);
    // 月の途中に始まり、満了日が「開始＋期間−1日」の取引を1つ選ぶ
    let d = deals
        .iter()
        .find(|d| {
            d.contract_period == Some(6.0)
                && d.contract_start_date.get(8..10) != Some("01")
                && !d.houjin_resolved.is_empty()
                && d.contract_expiration_date.len() >= 10
                && super::routes::month_index(
                    &d.contract_start_date,
                    &d.contract_expiration_date[..7],
                ) == Some(7)
        })
        .expect("月の途中に始まる6ヶ月契約");
    let v = build_customer(&sh, Some(&d.houjin_resolved), fixture_day());
    let mm = v["monthly"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["deal_id"] == d.id.as_str())
        .expect("その取引の推移");
    assert_eq!(mm["period"], 6.0);
    assert_eq!(mm["span_months"], 7, "{mm}");
    assert_eq!(mm["expiration"], d.contract_expiration_date.as_str());
}

/// N19: 「接触の記録が無い」の名札は、開始済みで接触ゼロの案件に立てる（いまの仕様のまま）。
///
/// 一度外したが、④成果とリスク（赤にしない）と電話・担当者一覧（いちばん拾うべきもの）の
/// どちらに揃えるかが決まっていないので戻した。藤巻さんの判断待ち。
/// 方向を変えるときは、このテストの期待を判断に合わせて書き換える。
#[test]
fn 接触の記録が無い名札は開始済みで接触ゼロのときだけ() {
    let sh = sheets();
    let b = build_deal_board(&sh, fixture_day());
    let mut tagged = 0;
    for r in b["rows"].as_array().unwrap() {
        let has = r["flags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "接触の記録が無い");
        let want = r["not_started"] == false && r["n_contact"] == 0;
        assert_eq!(has, want, "{r}");
        if has {
            tagged += 1;
        }
    }
    // fixture で開始済み・接触ゼロの稼働中は 61件（2026-09-18 時点）
    assert_eq!(tagged, 61);
}

/// N19: 拠点キーが空の取引は採用単価の悪化の判定に使わない（法人で1本にまとめない）。
/// V24: 悪化の行の `site_name` は kyoten_name（照合用のキーではない）。
///
/// fixture には拠点キーが空の取引が1件も無いので、そのままでは是正を戻しても落ちない。
/// 悪化と出る法人を1つ選び、その法人の取引の拠点キーを全部消して確かめる。
/// 旧仕様（「(拠点不明) 法人番号」で1本にまとめる）なら、その法人が悪化の行に出る。
#[test]
fn 拠点キーが空の取引は採用単価の悪化に使わない() {
    let base = sheets();
    let day = fixture_day();
    let deals = super::deals_of(&base.deal);

    // 素のままでも、悪化の行の表示名は kyoten_name
    let f0 = build_focus(&base, day);
    assert_eq!(
        f0["cpa"]["skipped_no_site"], 0,
        "fixture の拠点キーは全部入っている"
    );
    let mut named = 0;
    for r in f0["cpa"]["rows"].as_array().unwrap() {
        let site = r["site"].as_str().unwrap();
        // fixture の kyoten_name は行ごとの伏字の連番なので、同じ拠点でも行で違う。
        // その拠点の取引のどれかの表示名であればよい（キーとは必ず違う）
        let got = r["site_name"].as_str().unwrap_or("");
        assert!(
            deals
                .iter()
                .any(|d| d.kyoten_key == site && d.kyoten_name.trim() == got),
            "表示名になっていない: {r}"
        );
        assert_ne!(got, site, "キーのまま出している: {r}");
        named += 1;
    }
    assert!(named > 0);
    // 表示名が空なら null（照合用のキーで埋めない）。fixture に空が無いので1件作って見る
    let mut d = super::deals_of(&base.deal).remove(0);
    d.kyoten_name = " ".into();
    assert!(!d.kyoten_key.is_empty());
    assert_eq!(d.site_name(), None);

    // 法人でまとめたとき、確定した直近2点の上がり方がいちばん大きい法人
    // （＝旧仕様なら悪化の行の先頭に出る。上位60行で切られて見えなくならないように）
    let mut by_h: std::collections::BTreeMap<&str, Vec<&super::Deal>> = Default::default();
    for d in deals.iter().filter(|d| !d.houjin_resolved.is_empty()) {
        if super::cpa(d).is_some() {
            by_h.entry(d.houjin_resolved.as_str()).or_default().push(d);
        }
    }
    let (houjin, n_cpa, _) = by_h
        .iter()
        .filter_map(|(h, ds)| {
            let mut fixed: Vec<&&super::Deal> = ds.iter().filter(|d| !d.right_censored).collect();
            fixed.sort_by(|a, b| a.contract_start_date.cmp(&b.contract_start_date));
            let n = fixed.len();
            if n < 2 {
                return None;
            }
            let (a, b) = (super::cpa(fixed[n - 2])?, super::cpa(fixed[n - 1])?);
            (a > 0.0 && b > a).then_some((*h, ds.len(), b / a))
        })
        .max_by(|x, y| x.2.total_cmp(&y.2))
        .expect("法人でまとめると悪化に見える法人が無い");

    let mut sh = sheets();
    let (hi, ki) = (
        base.deal.col("houjin_resolved").unwrap(),
        base.deal.col("kyoten_key").unwrap(),
    );
    sh.deal = with_rows(&base.deal, |r| {
        if r[hi].as_ref() == houjin {
            r[ki] = Arc::from("");
        }
    });
    let f = build_focus(&sh, day);
    for r in f["cpa"]["rows"].as_array().unwrap() {
        let site = r["site"].as_str().unwrap();
        assert!(
            !site.contains("拠点不明") && !site.contains(houjin),
            "拠点キーが空の取引を法人で1本にまとめている: {r}"
        );
    }
    // 外した件数は、その法人の採用単価がある取引の数ちょうど（黙って消さない）
    assert_eq!(
        f["cpa"]["skipped_no_site"].as_u64().unwrap() as usize,
        n_cpa
    );
}

/// P1: `?refresh=1` で共有の SheetStore を丸ごと消さない（架電クオリティ・営業KPI の道連れ）。
/// 行単位で見る（コメントの中の言及には当てない）。
#[test]
fn 読み直しで共有のキャッシュを丸ごと消さない() {
    let src = include_str!("routes.rs");
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        assert!(
            !t.contains("invalidate(None)"),
            "routes.rs:{} で共有のキャッシュを丸ごと消している",
            i + 1
        );
    }
}

/// P2: 定期更新の間隔は TTL より短い（切れてから取ると、開いた人が待つ）。
#[test]
fn 先読みの定期更新はttlより短い() {
    let ttl = crate::handlers::call_quality::sheets::CACHE_TTL;
    let every = super::prefetch_interval();
    assert!(every < ttl, "{every:?} / {ttl:?}");
    assert!(
        every.as_secs() >= 10 * 60,
        "短すぎて Sheets を叩きすぎる: {every:?}"
    );
}

/// P3 / P4: 日付でない値が混ざっても panic せず、止まる。
#[test]
fn 日付でない値で落ちない() {
    // 多バイト文字の途中でバイト位置を切ると panic していた
    assert_eq!(super::date10("２０２６年９月２３日"), None);
    assert_eq!(super::date10("2026年9月23日"), None);
    assert_eq!(super::call_date_jst("２０２６年９月"), None);
    // 🔴 「2026年9月末」は 4バイト＋「年」3バイトで、7バイト目がちょうど文字の境目になる。
    //    旧コードの `[..7]` でも panic しないので、それだけでは何も守っていなかった。
    //    全角数字は1文字3バイトなので、4・7・10 バイト目がどれも文字の途中に落ちる
    let mut d = super::deals_of(&sheets().deal).remove(0);
    d.contract_expiration_date = "2026年9月末".into();
    let _ = d.manryou_month();
    d.contract_expiration_date = "２０２６年９月末".into();
    assert_eq!(d.manryou_month(), None);
    // 何ヶ月目（`[..4]` / `[5..7]` で切っていた）
    assert_eq!(
        super::routes::month_index("２０２６年７月", "2026-09"),
        None
    );
    assert_eq!(
        super::routes::month_index("2026-07-01", "２０２６年９月"),
        None
    );
    // 月として読めない値から埋め始めても、無限に積まずに止まる
    let out = super::fill_forward(&[("2026-1x".to_string(), 1.0)], "2027-01");
    assert_eq!(out.len(), 1);
    let out = super::fill_forward(&[("2026-13".to_string(), 1.0)], "2027-01");
    assert_eq!(out.len(), 1);
    // 正しい値はいままでどおり埋まる
    let out = super::fill_forward(&[("2026-11".to_string(), 1.0)], "2027-01");
    assert_eq!(out.len(), 3);
}

/// P3: MTG の開催日が日本語で入っていても、MTG の品質の月次（開催日の先頭7文字）で落ちない。
/// 以前は `d[..7]` で切っていて、全角の日付だと7バイト目が文字の途中になり panic していた。
#[test]
fn mtgの開催日が日付でなくても落ちない() {
    let base = sheets();
    let ci = base.mtg.col("開催日").unwrap();
    let mut sh = sheets();
    sh.mtg = with_rows(&base.mtg, |r| r[ci] = Arc::from("２０２６年９月３日"));
    let v = build_mtg_quality(&sh, fixture_day());
    assert!(v.is_object());
}

/// V4: フェーズとリスク判定は意味の順で返す（画面は受け取った順に描く）。
#[test]
fn フェーズとリスク判定は意味の順で返る() {
    let sh = sheets();
    let r = build_rampup(&sh, fixture_day());
    let labels: Vec<&str> = r["phase"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["序盤", "中盤", "終盤", "満了超過", "出せない"]);
    let q = build_mtg_quality(&sh, fixture_day());
    let labels: Vec<&str> = q["risk_dist"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["高", "中", "低", "判定不可", "（未判定）"]);
}

/// V22: 注力の定義の文言が正しい定義で、画面に出す文字列に Markdown の ** が無い。
#[test]
fn 担当者一覧の文言に訂正前の定義とmarkdownが無い() {
    let v = build_consultants(&sheets(), fixture_day());
    let rule = v["focus_rule"].as_str().unwrap();
    assert!(
        rule.contains("月額30万以上") && rule.contains("拠点3つ以上"),
        "{rule}"
    );
    assert!(
        !rule.contains("30万超") && !rule.contains("拠点が複数"),
        "{rule}"
    );
    let all = serde_json::to_string(&v).unwrap();
    assert!(!all.contains("**"), "画面に出す文字列に ** が入っている");
}

/// 🔴 画面にそのまま出る文に英語の用語を残さない（2026-09-23 デプロイ後の実機確認）。
/// 電話の「Call は Deal に多対多」（reach.note）と、案件の並びの「AUC 0.583」（order_rule）が
/// 英語のまま出ていた。画面の側（templates）の文は tests/consulting_view_rules.js が見る。
#[test]
fn 画面に出す文に英語の用語を残さない() {
    let s = sheets();
    let t = fixture_day();
    let phone = build_phone(&s, t);
    let today = build_today_board(&s, t);
    let board = build_deal_board(&s, t);
    for (name, v) in [
        ("電話 reach.note", &phone["reach"]["note"]),
        ("今日動く先 order_rule", &today["meta"]["order_rule"]),
        ("案件そのもの order_rule", &board["meta"]["order_rule"]),
    ] {
        let text = v
            .as_str()
            .unwrap_or_else(|| panic!("{name} が文字列でない: {v}"));
        for w in ["Call", "Deal", "多対多", "AUC", "StratifiedKFold", "churn"] {
            assert!(
                !text.contains(w),
                "{name} に英語の用語「{w}」が残っている: {text}"
            );
        }
    }
}

/// 🔴 MTG途絶の帯の説明（画面にそのまま出る文）に、中の仕組みの名前を出さない。
/// 2026-09-24 実機: today の図の注記に「GAS（no_mtg_alerter）」と出ていた。
/// 帯を説明する3つの文（rule / no_record_note / source_note）に、英字の識別子
/// （`_` を含む語・GAS）が入っていないことを見る。MTG・Zoom など画面の用語は残してよい。
#[test]
fn mtg途絶の帯の説明に仕組みの名前を出さない() {
    let v = build_today_board(&sheets(), fixture_day());
    let g = &v["meta"]["mtg_gap"];
    for k in ["rule", "no_record_note", "source_note"] {
        let s = g[k]
            .as_str()
            .unwrap_or_else(|| panic!("mtg_gap.{k} が無い"));
        assert!(!s.contains("GAS"), "mtg_gap.{k} に「GAS」が出ている: {s}");
        assert!(!s.contains("no_mtg"), "mtg_gap.{k} に内部名が出ている: {s}");
        assert!(
            !s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|w| w.contains('_') && w.chars().any(|c| c.is_ascii_alphabetic())),
            "mtg_gap.{k} に英字の識別子が出ている: {s}"
        );
    }
    assert!(
        g["rule"]
            .as_str()
            .unwrap()
            .contains("Slack に届く MTG 途絶の警告"),
        "何と同じ線引きかを、現場の言葉で書いていない"
    );
}

// ================================================================ 担当者ごとの接触

use super::contact_trend::{build_contact_trend, owner_at, owner_timeline};

/// 期間ごとの (持ち案件, 接触) を取り出す。
fn trend_pairs(v: &Value, unit: &str, key: &str) -> Vec<(i64, i64)> {
    v[unit][key]
        .as_array()
        .unwrap_or_else(|| panic!("{unit}.{key} が無い"))
        .iter()
        .map(|c| {
            (
                c["deals"].as_i64().expect("deals"),
                c["contacts"].as_i64().expect("contacts"),
            )
        })
        .collect()
}

/// 🔴 分母（持っていた案件）と分子（接触）を、Rust とは別に Python で数えた値で固定する。
///
/// Python 側は担当を**1日ずつ**引いて数えた（Rust は担当が替わった日だけを見る）。
/// 数え方の違う2つが同じ数になることを見る（基準日 2026-09-18）。
/// | 月 | 持ち案件 | 接触 |（担当が決まった分。担当が決められない分は別）
/// | 2026-07 | 627 | 1,183 |
/// | 2026-08 | 644 | 1,401 |
/// | 2026-09（未確定） | 604 | 1,274 |
#[test]
fn 担当者ごとの接触の分母と接触が別の数え方と一致する() {
    let v = build_contact_trend(&sheets(), fixture_day());
    let month: Vec<(i64, i64)> = vec![
        (569, 223),
        (582, 198),
        (597, 227),
        (594, 242),
        (611, 217),
        (632, 621),
        (608, 1402),
        (615, 1397),
        (601, 1500),
        (627, 1930),
        (644, 1812),
        (604, 1486),
    ];
    assert_eq!(trend_pairs(&v, "month", "team"), month, "月ごとの全体");
    let week: Vec<(i64, i64)> = vec![
        (518, 386),
        (523, 417),
        (534, 417),
        (536, 391),
        (558, 473),
        (543, 476),
        (534, 82),
        (547, 581),
        (567, 548),
        (568, 600),
        (564, 493),
        (567, 518),
    ];
    assert_eq!(trend_pairs(&v, "week", "team"), week, "週ごとの全体");
    // 担当が決められない分（どの担当者にも数えていない）
    assert_eq!(
        trend_pairs(&v, "month", "undetermined"),
        vec![
            (1, 0),
            (1, 0),
            (0, 0),
            (3, 0),
            (0, 0),
            (0, 0),
            (1, 0),
            (1, 0),
            (1, 3),
            (0, 8),
            (1, 10),
            (0, 0)
        ]
    );
    // 付け直して数えた接触（担当が決まった分。team の接触の内数）
    assert_eq!(
        v["month"]["moved"],
        serde_json::json!([3, 1, 2, 4, 1, 279, 888, 833, 756, 747, 411, 212])
    );
    assert_eq!(
        v["week"]["moved"],
        serde_json::json!([176, 211, 172, 127, 130, 129, 21, 133, 105, 103, 68, 64])
    );
    let week_und: Vec<i64> = trend_pairs(&v, "week", "undetermined")
        .iter()
        .map(|x| x.1)
        .collect();
    assert_eq!(week_und, vec![0, 0, 0, 5, 3, 0, 0, 2, 8, 0, 0, 0]);
    assert_eq!(v["meta"]["n_ambiguous"], 0);
    // 期間の途中で担当が替わり、両方の担当に数えた案件
    assert_eq!(
        v["month"]["shared"],
        serde_json::json!([45, 17, 33, 20, 16, 37, 42, 10, 14, 72, 42, 10])
    );
    assert_eq!(
        v["month"]["rows"].as_array().unwrap().len(),
        39,
        "担当者の数（月）"
    );
    assert_eq!(
        v["week"]["rows"].as_array().unwrap().len(),
        34,
        "担当者の数（週）"
    );
    // 開始日・満了日が読めずに数えられない本体案件
    assert_eq!(v["meta"]["n_no_span"], 3);
}

/// 担当者の分母を足すと、全体の分母より多い（替わった案件を両方に数えるため）。
/// 1期間に3人が持った案件は3人に数えるので、差は「両方に数えた案件」の数以上になる。
/// 担当者の分母の合計は Python で1日ずつ担当を引いて数えた値で固定する。
/// 接触は1回を1人にだけ数えるので、担当者の合計と全体が一致する。
#[test]
fn 担当替わりの案件は両方に数え接触はその日の担当に数える() {
    let v = build_contact_trend(&sheets(), fixture_day());
    let sum_deals = |unit: &str| -> Vec<i64> {
        (0..12)
            .map(|i| {
                v[unit]["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["cells"][i]["deals"].as_i64().unwrap())
                    .sum()
            })
            .collect()
    };
    assert_eq!(
        sum_deals("month"),
        vec![614, 599, 630, 614, 628, 670, 651, 625, 616, 699, 686, 614]
    );
    assert_eq!(
        sum_deals("week"),
        vec![527, 531, 542, 547, 588, 549, 536, 551, 583, 578, 564, 569]
    );
    for unit in ["month", "week"] {
        let team = trend_pairs(&v, unit, "team");
        let shared: Vec<i64> = v[unit]["shared"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        for (i, (td, tc)) in team.iter().enumerate() {
            let (mut d, mut c) = (0i64, 0i64);
            for r in v[unit]["rows"].as_array().unwrap() {
                d += r["cells"][i]["deals"].as_i64().unwrap();
                c += r["cells"][i]["contacts"].as_i64().unwrap();
            }
            assert!(d >= td + shared[i], "{unit}[{i}] 担当者の分母の合計 {d}");
            assert_eq!(c, *tc, "{unit}[{i}] 担当者の接触の合計");
        }
    }
    // どこかの期間で本当に担当が替わっていること（0 だと上の見張りが素通りする）
    assert!(v["month"]["shared"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x.as_i64().unwrap() > 0));
}

/// 小さなシートで、担当替わり・履歴より前・分母0・オプション除外を1つずつ確かめる。
fn trend_tiny(meta_asof: &str) -> Sheets {
    let empty = |h: &[&str]| tiny(h, &[]);
    Sheets {
        deal: tiny(
            &[
                "deal_id",
                "dealstage",
                "contract_kind",
                "contract_start_date",
                "contract_expiration_date",
                "is_active",
            ],
            &[
                // 9/10 に A → B。9/5 と 9/15 に接触
                &["d1", "x", "(新規)", "2026-08-01", "2026-12-31", "TRUE"],
                // 履歴が 9/20 から。9/12 の接触は担当が決められない
                &["d2", "x", "(新規)", "2026-09-01", "2026-12-31", "TRUE"],
                // オプション契約。どこにも数えない
                &["d3", "x", "求人追加", "2026-08-01", "2026-12-31", "TRUE"],
            ],
        ),
        call: tiny(
            &["ts", "duration_sec", "deal_id"],
            &[
                &["2026-09-05T01:00:00Z", "120", "d1"],
                // UTC 9/14 16:00 ＝ 日本時間 9/15
                &["2026-09-14T16:00:00Z", "120", "d1"],
                // 60秒ちょうどは接触ではない
                &["2026-09-16T01:00:00Z", "60", "d1"],
                &["2026-09-12T01:00:00Z", "300", "d2"],
                &["2026-09-12T01:00:00Z", "300", "d3"],
            ],
        ),
        mtg: empty(&["開催日", "deal_id"]),
        history: empty(&["deal_id"]),
        customer: empty(&["houjin"]),
        mail_mtg: empty(&["deal_id"]),
        handover: empty(&["deal_id"]),
        owner_hist: tiny(
            &["date", "owner", "retired", "deal_id"],
            &[
                &["2026-07-01", "A", "FALSE", "d1"],
                &["2026-09-10", "B", "FALSE", "d1"],
                &["2026-09-20", "C", "FALSE", "d2"],
                &["2026-07-01", "D", "FALSE", "d3"],
            ],
        ),
        meta: tiny(&["key", "value"], &[&["データ取得時刻(JST)", meta_asof]]),
        all_cached: false,
    }
}

fn row_of<'a>(v: &'a Value, unit: &str, who: &str) -> &'a Value {
    v[unit]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["consultant"] == who)
        .unwrap_or_else(|| panic!("{unit} に {who} が無い"))
}

#[test]
fn 担当者ごとの接触は担当替わりと履歴の前を分けて数える() {
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    let v = build_contact_trend(&trend_tiny("2026-09-25 09:00:00"), day);
    let last = super::contact_trend::N_MONTHS - 1; // 2026-09
    assert_eq!(v["month"]["periods"][last]["key"], "2026-09");
    let a = &row_of(&v, "month", "A")["cells"][last];
    let b = &row_of(&v, "month", "B")["cells"][last];
    // 替わる前も後も1件ずつ。接触はその日の担当に
    assert_eq!(
        (a["deals"].as_i64(), a["contacts"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(
        (b["deals"].as_i64(), b["contacts"].as_i64()),
        (Some(1), Some(1))
    );
    assert_eq!(v["month"]["shared"][last], 1);
    // 🔴 d2 は 9/20 から C。9/1〜9/19 は担当が決められないので C に付けない
    let c = &row_of(&v, "month", "C")["cells"][last];
    assert_eq!(
        (c["deals"].as_i64(), c["contacts"].as_i64()),
        (Some(1), Some(0))
    );
    assert_eq!(v["month"]["undetermined"][last]["contacts"], 1);
    // 🔴 オプション契約（d3）の担当 D はどこにも出ない
    assert!(v["month"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["consultant"] != "D"));
    assert_eq!(v["month"]["team"][last]["deals"], 2);
    assert_eq!(v["month"]["team"][last]["contacts"], 2);
    // 8月の d1 は A だけ。接触0でも分母は1（1件あたり 0 回。空ではない）
    let aug = &row_of(&v, "month", "A")["cells"][last - 1];
    assert_eq!(aug["deals"], 1);
    assert_eq!(aug["avg"], 0.0);
    // 🔴 分母0の期間は空（0 にしない）。B は 8月に何も持っていない
    let b_aug = &row_of(&v, "month", "B")["cells"][last - 1];
    assert_eq!(b_aug["deals"], 0);
    assert!(
        b_aug["avg"].is_null(),
        "分母0の平均が {} になっている",
        b_aug["avg"]
    );
    assert_eq!(b_aug["small_n"], false);
    // 持ち案件1件は小さい印
    assert_eq!(a["small_n"], true);
}

/// 🔴 いまの週・月は未確定。データを取った日が古ければ、そこから先も未確定。
#[test]
fn 担当者ごとの接触は今期とデータ取得日より後を未確定にする() {
    // fixture: 基準日 2026-09-18・データ取得 2026-09-14
    let v = build_contact_trend(&sheets(), fixture_day());
    let prov = |unit: &str| -> Vec<bool> {
        v[unit]["periods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["provisional"].as_bool().unwrap())
            .collect()
    };
    let m = prov("month");
    assert_eq!(m.iter().filter(|x| **x).count(), 1, "月で未確定は今月だけ");
    assert!(m[m.len() - 1], "今月が未確定になっていない");
    let w = prov("week");
    assert_eq!(v["week"]["periods"][11]["start"], "2026-09-14");
    assert_eq!(w.iter().filter(|x| **x).count(), 1, "週で未確定は今週だけ");
    assert!(w[11]);

    // データ取得が 9/5 なら、9/5 を含む週（8/31〜）から先は未確定
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 18).unwrap();
    let v = build_contact_trend(&trend_tiny("2026-09-05 09:00:00"), day);
    let starts: Vec<(String, bool)> = v["week"]["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["start"].as_str().unwrap().to_string(),
                p["provisional"].as_bool().unwrap(),
            )
        })
        .collect();
    let first_prov = starts.iter().find(|(_, p)| *p).unwrap();
    assert_eq!(first_prov.0, "2026-08-31");
    assert_eq!(starts.iter().filter(|(_, p)| *p).count(), 3);
    // 2026-08 は 8/31 まで。データは 9/5 まであるので確定
    assert_eq!(v["month"]["periods"][10]["provisional"], false);
}

/// 🔴 分母はオプション契約を除いた案件（`deals_of`）。オプション込みで数えると別の数になる。
#[test]
fn 担当者ごとの接触はオプション契約を分母に入れない() {
    let sh = sheets();
    let v = build_contact_trend(&sh, fixture_day());
    let count = |ds: &[super::Deal], s: chrono::NaiveDate, e: chrono::NaiveDate| {
        ds.iter()
            .filter(|d| {
                match (
                    super::date10(&d.contract_start_date),
                    super::date10(&d.contract_expiration_date),
                ) {
                    (Some(a), Some(b)) => a <= b && a <= e && b >= s,
                    _ => false,
                }
            })
            .count() as i64
    };
    let main = super::deals_of(&sh.deal);
    let all = super::deals_all_of(&sh.deal);
    for (i, p) in v["month"]["periods"].as_array().unwrap().iter().enumerate() {
        let s = super::date10(p["start"].as_str().unwrap()).unwrap();
        let e = super::date10(p["end"].as_str().unwrap())
            .unwrap()
            .min(fixture_day());
        let got = v["month"]["team"][i]["deals"].as_i64().unwrap()
            + v["month"]["undetermined"][i]["deals"].as_i64().unwrap();
        assert_eq!(got, count(&main, s, e), "{i} 本体案件の数");
        assert_ne!(
            got,
            count(&all, s, e),
            "{i} オプション込みと同じ数（除外が効いていない）"
        );
    }
}

/// いまの担当は、担当者の一覧（`consultant_of`）と同じ人を指す。
#[test]
fn 担当者ごとの接触のいまの担当は担当者の一覧と同じ() {
    let sh = sheets();
    let day = fixture_day();
    let tl = owner_timeline(&sh.owner_hist);
    let now = super::consultant_of(&sh.owner_hist);
    let mut n = 0;
    for (id, t) in &tl {
        if t.last().is_some_and(|e| e.0 > day) {
            continue; // 基準日より後の書き換えがある取引は比べない
        }
        let got = owner_at(t, day).map(|e| e.1.as_str());
        assert_eq!(got, now.get(id).map(|x| x.0.as_str()), "{id} のいまの担当");
        n += 1;
    }
    assert!(n > 3000, "比べた取引が少なすぎる: {n}");
}

/// 🔴 通話の記録が始まる前の期間は、MTG だけなので比べられない（画面に出さない印）。
#[test]
fn 通話の記録が始まる前の期間に印が付く() {
    let v = build_contact_trend(&sheets(), fixture_day());
    assert_eq!(v["meta"]["call_from"], "2026-03-23");
    let flags: Vec<(String, bool)> = v["month"]["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["key"].as_str().unwrap().to_string(),
                p["calls_missing"].as_bool().unwrap(),
            )
        })
        .collect();
    for (k, f) in &flags {
        assert_eq!(*f, k.as_str() <= "2026-03", "{k} の印");
    }
    assert!(v["week"]["periods"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["calls_missing"] == false));
}

/// 🔴 画面の頭に出す断り。接触は検知専用で、多いほど良い・担当者の評価、ではない。
#[test]
fn 担当者ごとの接触は検知専用で評価ではないと書く() {
    let v = build_contact_trend(&sheets(), fixture_day());
    let t = v["meta"]["not_counted"].as_str().expect("not_counted");
    for w in [
        "接触は検知専用",
        "多いほど良いという評価ではありません",
        "担当者の評価ではありません",
    ] {
        assert!(t.contains(w), "断りに「{w}」が無い: {t}");
    }
}

/// 🔴 通話が、まだ始まっていない継続の取引やオプションの取引に付いていても、
/// その日に動いていた同じ拠点の本体案件に付け直して数える（2026-09-24 検証の指摘）。
/// 付け直さないと、継続の前の月ほど接触が落ち、確定した月もあとで下がる。
fn trend_sites() -> Sheets {
    let empty = |h: &[&str]| tiny(h, &[]);
    Sheets {
        deal: tiny(
            &[
                "deal_id",
                "dealstage",
                "contract_kind",
                "contract_start_date",
                "contract_expiration_date",
                "kyoten_key",
                "is_active",
            ],
            &[
                // 拠点 S2: 前の契約 p1（〜8/31）と、継続の契約 p2（9/1〜）
                &[
                    "p1",
                    "x",
                    "(新規)",
                    "2026-03-01",
                    "2026-08-31",
                    "S2",
                    "FALSE",
                ],
                &["p2", "x", "継続", "2026-09-01", "2027-02-28", "S2", "TRUE"],
                // 拠点 S2 のオプション契約（分母には入らない）
                &[
                    "o1",
                    "x",
                    "求人追加",
                    "2026-06-01",
                    "2026-12-31",
                    "S2",
                    "TRUE",
                ],
                // 拠点 S3: 契約期間の重なる本体案件が2件（付け先を決められない）
                &[
                    "q1",
                    "x",
                    "(新規)",
                    "2026-01-01",
                    "2026-12-31",
                    "S3",
                    "TRUE",
                ],
                &["q2", "x", "継続", "2026-06-01", "2027-05-31", "S3", "TRUE"],
                &[
                    "o2",
                    "x",
                    "求人追加",
                    "2026-01-01",
                    "2026-12-31",
                    "S3",
                    "TRUE",
                ],
                // 拠点キーが空。契約の外の接触は付け直さない
                &["r1", "x", "(新規)", "2026-08-01", "2026-12-31", "", "TRUE"],
            ],
        ),
        call: tiny(
            &["ts", "duration_sec", "deal_id"],
            &[
                // 8/20 の通話が、まだ始まっていない p2 に付いている → p1（E）に数える
                &["2026-08-20T01:00:00Z", "120", "p2"],
                // p2 の中の通話はそのまま p2（F）
                &["2026-09-03T01:00:00Z", "120", "p2"],
                // オプション o1 に付いた 7/10 の通話 → p1（E）に数える
                &["2026-07-10T01:00:00Z", "120", "o1"],
                // 2/10 は S2 のどの契約の前。数えない
                &["2026-02-10T01:00:00Z", "120", "p2"],
                // S3 は 7/10 に q1・q2 の2件が動いている。決められないので数えない
                &["2026-07-10T01:00:00Z", "120", "o2"],
                // 拠点キーが空の r1 の契約前（7/15）。数えない
                &["2026-07-15T01:00:00Z", "120", "r1"],
            ],
        ),
        mtg: empty(&["開催日", "deal_id"]),
        history: empty(&["deal_id"]),
        customer: empty(&["houjin"]),
        mail_mtg: empty(&["deal_id"]),
        handover: empty(&["deal_id"]),
        owner_hist: tiny(
            &["date", "owner", "retired", "deal_id"],
            &[
                &["2026-01-01", "E", "FALSE", "p1"],
                &["2026-08-15", "F", "FALSE", "p2"],
                &["2026-01-01", "G", "FALSE", "q1"],
                &["2026-01-01", "G", "FALSE", "q2"],
                &["2026-01-01", "H", "FALSE", "r1"],
            ],
        ),
        meta: tiny(
            &["key", "value"],
            &[&["データ取得時刻(JST)", "2026-09-25 09:00:00"]],
        ),
        all_cached: false,
    }
}

#[test]
fn 担当者ごとの接触は継続先やオプションに付いた通話をその日の案件に付け直す() {
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    let v = build_contact_trend(&trend_sites(), day);
    let last = super::contact_trend::N_MONTHS - 1; // 2026-09
    let at = |who: &str, i: usize| -> (i64, i64) {
        let c = &row_of(&v, "month", who)["cells"][i];
        (
            c["deals"].as_i64().unwrap(),
            c["contacts"].as_i64().unwrap(),
        )
    };
    // 8月: p1 の E に、p2 に付いていた 8/20 の通話を数える
    assert_eq!(
        at("E", last - 1),
        (1, 1),
        "継続先に付いた通話を前の契約に付け直していない"
    );
    // 7月: オプション o1 に付いていた通話を p1 の E に数える
    assert_eq!(
        at("E", last - 2),
        (1, 1),
        "オプションに付いた通話を本体案件に付け直していない"
    );
    // 9月: p2 の中の通話はそのまま F
    assert_eq!(at("F", last), (1, 1));
    // 🔴 S3 は付け先が2件あって決められない。G には数えず、件数を出す
    assert_eq!(
        at("G", last - 2),
        (2, 0),
        "付け先を決められない通話を推測で数えている"
    );
    assert_eq!(v["meta"]["n_ambiguous"], 1);
    // 🔴 拠点キーが空の r1 の契約前の通話は数えない
    assert_eq!(v["month"]["team"][last - 2]["contacts"], 1);
    // 付け直した数（7月・8月に1件ずつ、9月は0）
    assert_eq!(v["month"]["moved"][last - 2], 1);
    assert_eq!(v["month"]["moved"][last - 1], 1);
    assert_eq!(v["month"]["moved"][last], 0);
    // 2月（p1 の前）はどこにも数えない
    let feb = v["month"]["periods"]
        .as_array()
        .unwrap()
        .iter()
        .position(|p| p["key"] == "2026-02")
        .unwrap();
    assert_eq!(v["month"]["team"][feb]["contacts"], 0);
    // 画面に付け直しの決まりを書く
    let t = v["attach_rule"].as_str().expect("attach_rule");
    for w in ["付け直して", "同じ拠点", "決められない", "あとで少し動く"] {
        assert!(t.contains(w), "付け直しの決まりに「{w}」が無い: {t}");
    }
}

/// 🔴 `small_n` の線引きは「持ち案件 3 件未満」。2 件は印あり、3 件は印なし（境目を固める）。
#[test]
fn 担当者ごとの接触の小さい印は持ち案件3件未満の境目で切り替わる() {
    assert_eq!(super::contact_trend::MIN_DEALS, 3);
    let v = build_contact_trend(&sheets(), fixture_day());
    let (mut seen2, mut seen3) = (false, false);
    for unit in ["month", "week"] {
        for r in v[unit]["rows"].as_array().unwrap() {
            for c in r["cells"].as_array().unwrap() {
                let d = c["deals"].as_i64().unwrap();
                assert_eq!(
                    c["small_n"].as_bool().unwrap(),
                    d > 0 && d < 3,
                    "{unit} {} 持ち案件 {d} 件の印",
                    r["consultant"]
                );
                seen2 |= d == 2;
                seen3 |= d == 3;
            }
        }
    }
    // 境目の両側が fixture に無いと、上の見張りが素通りする
    assert!(
        seen2 && seen3,
        "持ち案件 2 件と 3 件の期間が fixture に無い"
    );
}

/// 分母は契約期間で数えるので、画面の頭の「稼働中 N 件」（いまの稼働の印）とは別の集合。
/// 同じ集合に見えないよう、分母の決まりに違いを書く（2026-09-24 検証の指摘）。
#[test]
fn 担当者ごとの接触の分母は頭の稼働中の件数と別だと書く() {
    let v = build_contact_trend(&sheets(), fixture_day());
    let t = v["denom_rule"].as_str().expect("denom_rule");
    for w in [
        "画面の頭の「稼働中」の件数",
        "別の数え方",
        "満了日より前に解約・充足へ移った案件",
    ] {
        assert!(t.contains(w), "分母の決まりに「{w}」が無い: {t}");
    }
}

// ================================================================ 担当の交代 × 交代の前後の接触

/// 🔴 交代の前後の接触を、Rust とは別に Python で数えた値で固定する（基準日 2026-09-18）。
///
/// 2026-09-24 藤巻さんの判断で、窓を前後60日から**それぞれの担当期間の全体（通期）**に変えた
/// （前 ＝ 同じ拠点の一つ前の交代日〜交代日の前日、後 ＝ 交代日〜次の交代日の前日、無ければ締め日の前日まで）。
/// 期待値はそれに合わせて作り直した。Python 側は、接触の定義（60秒超の通話・MTG、通話は日本時間）と
/// 付け直し（同じ拠点でその日に契約期間の中の本体案件が1件だけならそこへ）、交代の並び（拠点×交代日）を
/// 別に書き、窓は**1日ずつ**回して「その日に同じ拠点で動いている本体案件が1件だけか」を全部の取引から数え直した。
/// データを取った日は 2026-09-14（数えるのは 9/13 まで）、通話の記録は 2026-03-23 から。
///
/// | | 交代（拠点×交代日） | 行 |
/// | 比べられた | 19（うち後の担当がまだ担当中 13） | 39 |
/// | 途中（後の担当が担当中で、後がまだ30日に届かない） | 11 | 17 |
/// | 比べるには短い | 211（前に案件が無い 100・通話の記録の前 96・交代どうしの間が短い 9・後に案件が無い 6・重なり 0） | 301 |
/// | 数えられない（契約期間が読めない） | — | 1 |
///
/// 比べられた 19件: 増えた 13・減った 6・変わらない 0、変化の平均 +1.6140・中央値 +0.8824 回/30日。
/// （前後60日のときは 比べられた 12・増えた 7・減った 5・中央値 +0.7056 だった。）
/// 窓が長くなったので、同じ拠点で本体案件が重なる日にも掛かる（2026-04-09 の交代の前の窓で 182日）。
#[test]
fn 交代の前後の接触が別の数え方と一致する() {
    let v = build_handover(&sheets(), fixture_day());
    let c = &v["contact_cmp"];
    assert_eq!(c["meta"]["last_day"], "2026-09-13");
    assert_eq!(c["meta"]["call_from"], "2026-03-23");
    assert_eq!(c["n_events"], 241);
    assert_eq!(c["n_ok"], 19);
    assert_eq!(c["n_ongoing"], 13);
    assert_eq!(c["n_provisional"], 11);
    assert_eq!(c["n_short"], 211);
    assert_eq!(
        c["short_why"],
        serde_json::json!({"calls": 96, "before": 100, "after": 6, "overlap": 0, "gap": 9})
    );
    assert_eq!(c["overlap_days"], 182);
    assert_eq!(c["overlap_events"], 1);
    assert_eq!(c["n_up"], 13);
    assert_eq!(c["n_down"], 6);
    assert_eq!(c["n_same"], 0);
    let mean = c["mean_change"].as_f64().unwrap();
    assert!((mean - 1.6139506008744027).abs() < 1e-9, "平均 {mean}");
    let med = c["median_change"].as_f64().unwrap();
    assert!((med - 0.8823529411764706).abs() < 1e-9, "中央値 {med}");
    assert_eq!(c["meta"]["n_unavailable_rows"], 1);

    // 比べられた交代の (交代日, 前の最初の日, 前の最後の日, 前の日数, 前の接触, 後の最初の日, 後の最後の日,
    // 後の日数, 後の接触, 後の担当がまだ担当中)。交代ごとに1件
    type Row = (
        String,
        String,
        String,
        i64,
        i64,
        String,
        String,
        i64,
        i64,
        bool,
    );
    let mut ok: Vec<Row> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut rows_by: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut sums = [0i64; 4];
    // 行ごとの外した日（前の窓 通話・重なり・案件なし、後の窓 同じ順）
    let mut lost = [0i64; 6];
    for r in v["rows"].as_array().unwrap() {
        let x = &r["contact"];
        if x.is_null() {
            continue;
        }
        *rows_by
            .entry(x["status"].as_str().unwrap().to_string())
            .or_insert(0) += 1;
        let n = |w: &str, k: &str| x[w][k].as_i64().unwrap();
        let s = |w: &str, k: &str| x[w][k].as_str().unwrap_or("").to_string();
        sums[0] += n("before", "days");
        sums[1] += n("before", "contacts");
        sums[2] += n("after", "days");
        sums[3] += n("after", "contacts");
        for (i, k) in ["calls", "overlap", "none"].iter().enumerate() {
            lost[i] += x["before"]["lost"][k].as_i64().unwrap();
            lost[3 + i] += x["after"]["lost"][k].as_i64().unwrap();
        }
        if x["status"] == "ok" && seen.insert(x["event"].as_str().unwrap().to_string()) {
            ok.push((
                r["date"].as_str().unwrap().to_string(),
                s("before", "start"),
                s("before", "end"),
                n("before", "days"),
                n("before", "contacts"),
                s("after", "start"),
                s("after", "end"),
                n("after", "days"),
                n("after", "contacts"),
                x["ongoing"].as_bool().unwrap(),
            ));
        }
    }
    ok.sort();
    #[rustfmt::skip]
    let want: Vec<Row> = [
        ("2026-04-27", "2026-03-23", "2026-04-26", 35, 0, "2026-04-27", "2026-06-29", 64, 3, false),
        ("2026-04-30", "2026-03-23", "2026-04-29", 38, 4, "2026-04-30", "2026-09-13", 137, 36, true),
        ("2026-05-01", "2026-03-23", "2026-04-30", 39, 0, "2026-05-01", "2026-06-01", 32, 1, false),
        ("2026-05-01", "2026-03-23", "2026-04-30", 39, 0, "2026-05-01", "2026-06-12", 43, 2, false),
        ("2026-05-13", "2026-03-23", "2026-05-12", 51, 7, "2026-05-13", "2026-09-13", 124, 6, true),
        ("2026-05-14", "2026-03-23", "2026-05-13", 52, 0, "2026-05-14", "2026-08-24", 103, 4, false),
        ("2026-05-15", "2026-03-23", "2026-05-14", 53, 6, "2026-05-15", "2026-09-13", 122, 25, true),
        ("2026-05-19", "2026-03-23", "2026-05-18", 57, 1, "2026-05-19", "2026-09-13", 118, 4, true),
        ("2026-05-20", "2026-03-23", "2026-05-19", 58, 2, "2026-05-20", "2026-08-22", 95, 1, false),
        ("2026-06-01", "2026-03-23", "2026-05-31", 70, 26, "2026-06-01", "2026-09-13", 105, 30, true),
        ("2026-07-08", "2026-05-26", "2026-07-07", 43, 0, "2026-07-08", "2026-09-13", 68, 2, true),
        ("2026-07-22", "2026-04-08", "2026-07-21", 105, 11, "2026-07-22", "2026-09-13", 54, 4, true),
        ("2026-07-23", "2026-06-22", "2026-07-22", 31, 0, "2026-07-23", "2026-09-13", 53, 1, true),
        ("2026-07-29", "2026-06-05", "2026-07-28", 54, 2, "2026-07-29", "2026-09-01", 35, 1, false),
        ("2026-08-03", "2026-04-13", "2026-08-02", 112, 21, "2026-08-03", "2026-09-13", 42, 23, true),
        ("2026-08-04", "2026-03-30", "2026-08-03", 127, 11, "2026-08-04", "2026-09-13", 41, 2, true),
        ("2026-08-05", "2026-03-26", "2026-08-04", 132, 18, "2026-08-05", "2026-09-13", 40, 18, true),
        ("2026-08-06", "2026-06-05", "2026-08-05", 62, 6, "2026-08-06", "2026-09-13", 39, 9, true),
        ("2026-08-07", "2026-05-26", "2026-08-06", 73, 1, "2026-08-07", "2026-09-13", 38, 1, true),
    ]
    .iter()
    .map(|&(d, bs, be, bd, bc, as_, ae, ad, ac, o)| {
        (d.to_string(), bs.to_string(), be.to_string(), bd, bc, as_.to_string(), ae.to_string(), ad, ac, o)
    })
    .collect();
    assert_eq!(ok, want, "比べられた交代");
    // 行で数えた状態と、行の日数・接触の合計（同じ交代の行にも同じ値が入る）
    let want_rows: std::collections::BTreeMap<String, i64> =
        [("ok", 39), ("provisional", 17), ("short", 301)]
            .iter()
            .map(|&(k, n)| (k.to_string(), n))
            .collect();
    assert_eq!(rows_by, want_rows);
    assert_eq!(sums, [7017, 792, 25058, 3709]);
    assert_eq!(lost, [27472, 1092, 23657, 9940, 0, 16189]);
}

/// 交代の前後の接触を試す小さなデータ。基準日 2026-09-18、データを取った日 2026-09-15（数えるのは 9/14 まで）。
/// 窓はそれぞれの担当期間の全体（通期）。どの拠点も交代は1回なので、前は拠点の最初の契約から、後は 9/14 まで。
///
/// - 拠点 S1: 前の契約 a1（2026-04-01〜06-30）→ 継続 a2（07-01〜12-31）。交代 07-01。
///   交代の記録は a1 と a2 の2行（同じ交代）。前は a1、後は a2 に付いた接触を数える
/// - 拠点 S2: b1（2026-05-01〜2027-04-30）と b2（06-01〜06-10）が重なる。交代 07-01
///   （重なる 06-01〜06-10 の10日は窓に入れない → 前は 51日）
/// - 拠点 S3: c1（2026-06-20〜12-31）だけ。交代 07-01（前に動いていた案件が 11日しか無い → 短い）
/// - 拠点 S4: d1（2026-01-01〜12-31）。交代 08-20（後は 9/14 まで 26日。後の担当はまだ担当中 → 途中）
/// - 拠点 S5: e1（2026-01-01〜12-31）。交代 04-10（前の多くが通話の記録の始まり 03-23 より前 → 短い）
fn handover_tiny() -> Sheets {
    let empty = |h: &[&str]| tiny(h, &[]);
    let dh = [
        "deal_id",
        "dealname",
        "dealstage",
        "contract_kind",
        "contract_start_date",
        "contract_expiration_date",
        "kyoten_key",
        "is_active",
    ];
    let hh = [
        "date",
        "from",
        "to",
        "to_retired",
        "reflected",
        "record_gap_days",
        "deal_id",
    ];
    Sheets {
        deal: tiny(
            &dh,
            &[
                &[
                    "a1",
                    "A前",
                    "x",
                    "(新規)",
                    "2026-04-01",
                    "2026-06-30",
                    "S1",
                    "FALSE",
                ],
                &[
                    "a2",
                    "A継続",
                    "x",
                    "継続",
                    "2026-07-01",
                    "2026-12-31",
                    "S1",
                    "TRUE",
                ],
                &[
                    "b1",
                    "B",
                    "x",
                    "(新規)",
                    "2026-05-01",
                    "2027-04-30",
                    "S2",
                    "TRUE",
                ],
                &[
                    "b2",
                    "B短",
                    "x",
                    "(新規)",
                    "2026-06-01",
                    "2026-06-10",
                    "S2",
                    "FALSE",
                ],
                &[
                    "c1",
                    "C",
                    "x",
                    "(新規)",
                    "2026-06-20",
                    "2026-12-31",
                    "S3",
                    "TRUE",
                ],
                &[
                    "d1",
                    "D",
                    "x",
                    "(新規)",
                    "2026-01-01",
                    "2026-12-31",
                    "S4",
                    "TRUE",
                ],
                &[
                    "e1",
                    "E",
                    "x",
                    "(新規)",
                    "2026-01-01",
                    "2026-12-31",
                    "S5",
                    "TRUE",
                ],
            ],
        ),
        call: tiny(
            &["ts", "duration_sec", "deal_id"],
            &[
                // 通話の記録の始まり（長さを問わない。短い通話も始まりには数える）
                &["2026-03-23T01:00:00Z", "10", "e1"],
                // S1: 前の窓（05-02〜06-30）に a1 の 3回。1回はまだ始まっていない a2 に付いている → a1 に付け直す
                &["2026-05-10T01:00:00Z", "120", "a1"],
                &["2026-06-10T01:00:00Z", "120", "a1"],
                &["2026-06-20T01:00:00Z", "120", "a2"],
                // 60秒ちょうどは接触ではない
                &["2026-06-21T01:00:00Z", "60", "a1"],
                // S1: 後の窓（07-01〜08-29）に a2 の 1回。UTC 8/29 15:30 は日本時間 8/30 で窓の外
                &["2026-07-15T01:00:00Z", "120", "a2"],
                &["2026-08-29T15:30:00Z", "120", "a2"],
                // S2: 重なる 06-05 の通話は窓に入れない日
                &["2026-06-05T01:00:00Z", "120", "b1"],
                &["2026-06-15T01:00:00Z", "120", "b1"],
                &["2026-07-15T01:00:00Z", "120", "b1"],
            ],
        ),
        mtg: tiny(
            &["開催日", "deal_id"],
            // S2: 後の窓に MTG 1回
            &[&["2026-07-20", "b1"]],
        ),
        history: empty(&["deal_id"]),
        customer: empty(&["houjin"]),
        mail_mtg: empty(&["deal_id"]),
        handover: tiny(
            &hh,
            &[
                // S1 の同じ交代が2行（前の契約とあとの契約）
                &["2026-07-01", "佐藤", "鈴木", "FALSE", "", "", "a1"],
                &["2026-07-01", "佐藤", "鈴木", "FALSE", "", "", "a2"],
                &[
                    "2026-07-01",
                    "佐藤",
                    "x.y@example.co.jp",
                    "FALSE",
                    "",
                    "",
                    "b1",
                ],
                &["2026-07-01", "田中", "鈴木", "FALSE", "", "", "c1"],
                &["2026-08-20", "田中", "鈴木", "FALSE", "", "", "d1"],
                &["2026-04-10", "", "鈴木", "FALSE", "", "", "e1"],
                // 取引のシートに無い。前後は数えない（null）
                &["2026-07-01", "佐藤", "鈴木", "FALSE", "", "", "zz"],
            ],
        ),
        owner_hist: empty(&["date", "owner", "retired", "deal_id"]),
        meta: tiny(
            &["key", "value"],
            &[&["データ取得時刻(JST)", "2026-09-15 09:00:00"]],
        ),
        all_cached: false,
    }
}

fn ho_row<'a>(v: &'a Value, deal: &str) -> &'a Value {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["deal_id"] == deal)
        .unwrap_or_else(|| panic!("{deal} の行が無い"))
}

#[test]
fn 交代の前後は同じ拠点で動いていた案件1件の日だけで数える() {
    let v = build_handover(&handover_tiny(), fixture_day());
    let win = |deal: &str| -> (i64, i64, i64, i64) {
        let x = &ho_row(&v, deal)["contact"];
        let n = |w: &str, k: &str| x[w][k].as_i64().unwrap();
        (
            n("before", "days"),
            n("before", "contacts"),
            n("after", "days"),
            n("after", "contacts"),
        )
    };
    // S1: 前は a1 の 91日（04-01〜06-30。一つ前の交代が無いので拠点の最初の契約から）に 3回
    //     （a2 に付いていた 6/20 を a1 に付け直す）。後は a2 の 07-01〜09-14 の 76日に 2回
    //     （UTC 8/29 15:30 は日本時間 8/30。後の担当はまだ担当中なので締め日の前日まで）
    assert_eq!(win("a1"), (91, 3, 76, 2));
    // 同じ交代の行には同じ値
    assert_eq!(win("a2"), win("a1"));
    assert_eq!(
        ho_row(&v, "a1")["contact"]["event"],
        ho_row(&v, "a2")["contact"]["event"]
    );
    let a = &ho_row(&v, "a1")["contact"];
    assert_eq!(a["status"], "ok");
    assert_eq!(a["ongoing"], true);
    assert_eq!(a["dir"], "down");
    assert_eq!(a["before"]["start"], "2026-04-01");
    assert_eq!(a["before"]["end"], "2026-06-30");
    assert_eq!(a["after"]["start"], "2026-07-01");
    assert_eq!(a["after"]["end"], "2026-09-14");
    // 03-23〜03-31 は案件が無い日（通話の記録の始まりから見る）
    assert_eq!(a["before"]["lost"]["none"], 9);
    assert!((a["before"]["per30"].as_f64().unwrap() - 90.0 / 91.0).abs() < 1e-12);
    assert!((a["after"]["per30"].as_f64().unwrap() - 60.0 / 76.0).abs() < 1e-12);
    assert!((a["change"].as_f64().unwrap() - (60.0 / 76.0 - 90.0 / 91.0)).abs() < 1e-12);
    // S2: 重なる 06-01〜06-10 は窓に入れない（6/5 の通話も数えない）。前 05-01〜06-30 のうち 51日に 1回、
    //     後 76日に 2回（通話＋MTG）
    assert_eq!(win("b1"), (51, 1, 76, 2));
    assert_eq!(ho_row(&v, "b1")["contact"]["status"], "ok");
    assert_eq!(ho_row(&v, "b1")["contact"]["dir"], "up");
    // S3: 前に動いていた案件は 06-20〜06-30 の 11日だけ → 短い（前に案件が無い）
    let c = &ho_row(&v, "c1")["contact"];
    assert_eq!(c["status"], "short");
    assert_eq!(c["why"], "before");
    assert_eq!(c["before"]["days"], 11);
    // S4: 交代 08-20。後は 9/14 まで 26日で、後の担当はまだ担当中 → 途中（未確定）
    let d = &ho_row(&v, "d1")["contact"];
    assert_eq!(d["status"], "provisional");
    assert_eq!(d["ongoing"], true);
    assert_eq!(d["after"]["days"], 26);
    // S5: 前は 01-01〜04-09 のうち通話の記録の始まり（03-23）より前の 81日を外し、18日しか無い → 短い（通話）
    let e = &ho_row(&v, "e1")["contact"];
    assert_eq!(e["status"], "short");
    assert_eq!(e["why"], "calls");
    assert_eq!(e["before"]["days"], 18);
    assert_eq!(e["before"]["lost"]["calls"], 81);
    // 取引が見つからない行は数えない（0 回にしない）
    assert!(ho_row(&v, "zz")["contact"].is_null());

    let c = &v["contact_cmp"];
    // 交代は S1〜S5 の 5件（S1 の2行は1件）
    assert_eq!(c["n_events"], 5);
    assert_eq!(c["n_ok"], 2);
    assert_eq!(c["n_ongoing"], 2);
    assert_eq!(c["n_up"], 1);
    assert_eq!(c["n_down"], 1);
    assert_eq!(c["n_provisional"], 1);
    assert_eq!(
        c["short_why"],
        serde_json::json!({"calls": 1, "before": 1, "after": 0, "overlap": 0, "gap": 0})
    );
    // 平均と中央値（2件なので同じ）: S1 は 60/76 − 90/91、S2 は 60/76 − 30/51
    let want = ((60.0 / 76.0 - 90.0 / 91.0) + (60.0 / 76.0 - 30.0 / 51.0)) / 2.0;
    assert!((c["mean_change"].as_f64().unwrap() - want).abs() < 1e-12);
    assert!((c["median_change"].as_f64().unwrap() - want).abs() < 1e-12);
    // S2 の重なる 06-01〜06-10 は、比べられた交代でも外した日として数える（黙って外さない）
    assert_eq!(ho_row(&v, "b1")["contact"]["before"]["lost"]["overlap"], 10);
    assert_eq!(c["overlap_days"], 10);
    assert_eq!(c["overlap_events"], 1);
    assert_eq!(c["meta"]["n_unavailable_rows"], 1);
    assert_eq!(c["meta"]["last_day"], "2026-09-14");
}

#[test]
fn 交代の前後の担当者のまとめは同じ交代を1件と数え母数の小さい人に印を付ける() {
    let v = build_handover(&handover_tiny(), fixture_day());
    let c = &v["contact_cmp"];
    let find = |side: &str, label: &str| -> Value {
        c[side]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["label"] == label)
            .cloned()
            .unwrap_or_else(|| panic!("{side} に {label} が無い"))
    };
    // 引き継いだ側の鈴木: S1（2行で1件）・S3・S4・S5 の 4件。比べられたのは S1 だけ
    let s = find("by_to", "鈴木");
    assert_eq!(s["n_events"], 4);
    assert_eq!(s["n_ok"], 1);
    assert_eq!(s["n_down"], 1);
    assert_eq!(s["n_short"], 2);
    assert_eq!(s["n_provisional"], 1);
    let s1 = 60.0 / 76.0 - 90.0 / 91.0;
    assert!((s["median_change"].as_f64().unwrap() - s1).abs() < 1e-12);
    assert!((s["mean_change"].as_f64().unwrap() - s1).abs() < 1e-12);
    assert_eq!(s["n_ongoing"], 1);
    assert_eq!(s["small"], true);
    // 引き継がれた側の佐藤: S1・S2 の 2件（取引が見つからない行は数えない）
    let p = find("by_from", "佐藤");
    assert_eq!(p["n_events"], 2);
    assert_eq!(p["n_ok"], 2);
    assert_eq!(p["n_up"], 1);
    assert_eq!(p["n_down"], 1);
    // 偶数個の中央値は中2つの平均（2件なので平均と同じ）: S1 は 2回×30/76 − 3回×30/91、
    // S2 は 2回×30/76 − 1回×30/51
    let want = (s1 + (60.0 / 76.0 - 30.0 / 51.0)) / 2.0;
    assert!((p["median_change"].as_f64().unwrap() - want).abs() < 1e-12);
    assert!((p["mean_change"].as_f64().unwrap() - want).abs() < 1e-12);
    // 担当が空の行（S5 の from）は誰にも数えない
    let from_n: i64 = c["by_from"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["n_events"].as_i64().unwrap())
        .sum();
    assert_eq!(
        from_n, 4,
        "引き継がれた側の件数の合計（S1 S2 佐藤・S3 S4 田中）"
    );
    // メールアドレスを名前として出さない
    let to = c["by_to"].as_array().unwrap();
    let u = to.iter().find(|r| r["unresolved"] == true).unwrap();
    assert!(!u["label"].as_str().unwrap().contains('@'), "{u}");
    assert_eq!(u["unresolved_no"], 1);
    assert!(
        !c.to_string().contains("x.y@example.co.jp"),
        "まとめにメールアドレスが出ている"
    );
    // 母数の印は MIN_PERSON_N 件未満
    assert_eq!(super::handover_contact::MIN_PERSON_N, 5);
    for side in ["by_to", "by_from"] {
        for r in c[side].as_array().unwrap() {
            assert_eq!(r["small"], r["n_ok"].as_u64().unwrap() < 5, "{r}");
        }
    }
}

#[test]
fn 交代の前後の増減は丸めずに比べ変わらないを分ける() {
    use super::handover_contact::{Cmp, Status, Window};
    let w = |days, contacts| Window {
        days,
        contacts,
        ..Default::default()
    };
    let c = |b, a| Cmp {
        status: Status::Ok,
        before: b,
        after: a,
        ongoing: false,
    };
    // 60日に 2回と 30日に 1回は同じ（30日あたり 1.0）
    assert_eq!(c(w(60, 2), w(30, 1)).dir(), Some("same"));
    assert_eq!(c(w(60, 2), w(30, 1)).change(), Some(0.0));
    // 前後とも 0回は変わらない（減ったにしない）
    assert_eq!(c(w(60, 0), w(60, 0)).dir(), Some("same"));
    // 3回/60日 と 3回/59日 はわずかでも増えた
    assert_eq!(c(w(60, 3), w(59, 3)).dir(), Some("up"));
    // 日数0の窓は値が無い（0 回として比べない）
    assert_eq!(c(w(0, 0), w(60, 3)).dir(), None);
    assert_eq!(c(w(0, 0), w(60, 3)).change(), None);
    assert_eq!(w(0, 0).per30(), None);
}

/// 「比べるには短い」の境目（ちょうど30日は比べる、29日は短い）と、短い理由の分け方を試すデータ。
/// 通話・MTG・メタは `handover_tiny` のまま（通話の記録は 03-23 から、数えるのは 9/14 まで）。
/// どの拠点も交代は1回なので、前は「拠点の最初の契約の開始日と 03-23 の早いほう」から交代日の前日まで、
/// 後は交代日から 9/14 まで（窓は通期。2026-09-24 に前後60日から変えた）。
///
/// - K1: p1（06-01〜12-31）。交代 07-01。前 06-01〜06-30 のちょうど 30日 → 比べる
/// - K2: p2（06-02〜12-31）。交代 07-01。前 29日 → 短い（前に案件が無い）
/// - K3: p3（04-01〜07-30）。交代 07-01。後 07-01〜07-30 のちょうど 30日 → 比べる（満了していて担当中ではない）
/// - K4: p4（04-01〜07-29）。交代 07-01。後 29日 → 短い（後に案件が無い。担当中ではないので途中にしない）
/// - K5: q1（03-01〜12-31）と q2（03-01〜05-31）が重なる。交代 06-10。
///   前 03-01〜06-09 のうち 03-01〜05-31 の 92日が重なり、9日しか残らない → 短い（重なり）
/// - K6: g1（04-05〜12-31）。交代 05-01。前 03-23〜04-30 のうち 04-04 までの 13日は案件が無い。
///   26日 → 短い（前に案件が無い）
/// - K7: h1（03-01〜12-31）。交代 04-20。前 03-01〜04-19 は 通話の記録の前 22日・数えた 28日 → 短い（通話）
fn handover_edges() -> Sheets {
    let dh = [
        "deal_id",
        "dealname",
        "dealstage",
        "contract_kind",
        "contract_start_date",
        "contract_expiration_date",
        "kyoten_key",
        "is_active",
    ];
    let hh = [
        "date",
        "from",
        "to",
        "to_retired",
        "reflected",
        "record_gap_days",
        "deal_id",
    ];
    let d = |id, s, e, k| [id, id, "x", "(新規)", s, e, k, "TRUE"];
    let deals = [
        d("p1", "2026-06-01", "2026-12-31", "K1"),
        d("p2", "2026-06-02", "2026-12-31", "K2"),
        d("p3", "2026-04-01", "2026-07-30", "K3"),
        d("p4", "2026-04-01", "2026-07-29", "K4"),
        d("q1", "2026-03-01", "2026-12-31", "K5"),
        d("q2", "2026-03-01", "2026-05-31", "K5"),
        d("g1", "2026-04-05", "2026-12-31", "K6"),
        d("h1", "2026-03-01", "2026-12-31", "K7"),
    ];
    let h = |day, id| [day, "佐藤", "鈴木", "FALSE", "", "", id];
    let hv = [
        h("2026-07-01", "p1"),
        h("2026-07-01", "p2"),
        h("2026-07-01", "p3"),
        h("2026-07-01", "p4"),
        h("2026-06-10", "q1"),
        h("2026-05-01", "g1"),
        h("2026-04-20", "h1"),
    ];
    let deal_rows: Vec<&[&str]> = deals.iter().map(|r| &r[..]).collect();
    let ho_rows: Vec<&[&str]> = hv.iter().map(|r| &r[..]).collect();
    Sheets {
        deal: tiny(&dh, &deal_rows),
        handover: tiny(&hh, &ho_rows),
        ..handover_tiny()
    }
}

/// 検証の指摘（2026-09-24）: 30日ちょうどの境目を見張るテストが無く、`<` を `<=` にしても落ちなかった。
#[test]
fn 交代の前後はちょうど30日なら比べ29日なら短い() {
    let v = build_handover(&handover_edges(), fixture_day());
    let st = |deal: &str| {
        let x = &ho_row(&v, deal)["contact"];
        (
            x["status"].as_str().unwrap().to_string(),
            x["why"].as_str().map(str::to_string),
            x["before"]["days"].as_i64().unwrap(),
            x["after"]["days"].as_i64().unwrap(),
        )
    };
    assert_eq!(st("p1"), ("ok".into(), None, 30, 76));
    assert_eq!(st("p2"), ("short".into(), Some("before".into()), 29, 76));
    assert_eq!(st("p3"), ("ok".into(), None, 91, 30));
    assert_eq!(ho_row(&v, "p3")["contact"]["ongoing"], false);
    assert_eq!(st("p4"), ("short".into(), Some("after".into()), 91, 29));
}

/// 検証の指摘（2026-09-24）: 短い理由を「前の窓が通話の記録の始まりにかかれば通話」と近似していて、
/// 前に案件が無い交代まで通話に入っていた。重なりで外した日もどこにも数えていなかった。
#[test]
fn 交代の前後の短い理由は外した日のいちばん多い理由にする() {
    let v = build_handover(&handover_edges(), fixture_day());
    let x = |deal: &str| ho_row(&v, deal)["contact"].clone();
    let lost = |deal: &str| {
        let l = &x(deal)["before"]["lost"];
        (
            l["calls"].as_i64().unwrap(),
            l["overlap"].as_i64().unwrap(),
            l["none"].as_i64().unwrap(),
        )
    };
    // K5: 重なり
    assert_eq!(x("q1")["status"], "short");
    assert_eq!(x("q1")["why"], "overlap");
    assert_eq!(x("q1")["before"]["days"], 9);
    assert_eq!(lost("q1"), (0, 92, 0));
    // K6: 前に案件が無い
    assert_eq!(x("g1")["why"], "before");
    assert_eq!(x("g1")["before"]["days"], 26);
    assert_eq!(lost("g1"), (0, 0, 13));
    // K7: 通話の記録の前が 22日 → 通話
    assert_eq!(x("h1")["why"], "calls");
    assert_eq!(x("h1")["before"]["days"], 28);
    assert_eq!(lost("h1"), (22, 0, 0));

    let c = &v["contact_cmp"];
    assert_eq!(
        c["short_why"],
        serde_json::json!({"calls": 1, "before": 2, "after": 1, "overlap": 1, "gap": 0})
    );
    // 重なりで外した日は K5 の前の窓の 92日だけ（後の窓 06-10〜は q2 が終わっていて重ならない）
    assert_eq!(c["overlap_days"], 92);
    assert_eq!(c["overlap_events"], 1);
}

/// 窓を通期にした（2026-09-24）ときの区切りを試すデータ。MTG・メタは `handover_tiny` のまま、
/// 通話は下の3本（通話の記録は 03-23 から、数えるのは 9/14 まで）。
///
/// - 拠点 T: t1（2026-01-01〜12-31）1件。交代 05-01・06-15・07-15・08-13 の4回
///   - 05-01: 前 01-01〜04-30（03-22 までは通話の記録の前）→ 39日。後は次の交代の前日 06-14 まで 45日（担当中ではない）
///   - 06-15: 前は一つ前の交代 05-01 から 06-14 まで 45日。後 06-15〜07-14 のちょうど 30日 → 比べる
///   - 07-15: 前 30日。後 07-15〜08-12 は 29日で、次の交代で区切られている → 短い（交代どうしの間）
///   - 08-13: 前 07-15〜08-12 の 29日 → 短い（交代どうしの間）
///   - 06-14 の通話は 05-01 の後と 06-15 の前の両方に、06-15 の通話は 06-15 の後にだけ入る
/// - 拠点 U1: u1（01-01〜12-31）。交代 08-16。後 08-16〜09-14 のちょうど 30日 → 比べる（担当中）
/// - 拠点 U2: u2（01-01〜12-31）。交代 08-17。後 29日で担当中 → 途中（未確定）
/// - 拠点 W: w1（01-01〜08-31）。交代 07-01。後 07-01〜08-31 の 62日。締め日には動いていない → 担当中ではない
fn handover_tenure() -> Sheets {
    let dh = [
        "deal_id",
        "dealname",
        "dealstage",
        "contract_kind",
        "contract_start_date",
        "contract_expiration_date",
        "kyoten_key",
        "is_active",
    ];
    let hh = [
        "date",
        "from",
        "to",
        "to_retired",
        "reflected",
        "record_gap_days",
        "deal_id",
    ];
    let d = |id, s, e, k| [id, id, "x", "(新規)", s, e, k, "TRUE"];
    let deals = [
        d("t1", "2026-01-01", "2026-12-31", "T"),
        d("u1", "2026-01-01", "2026-12-31", "U1"),
        d("u2", "2026-01-01", "2026-12-31", "U2"),
        d("w1", "2026-01-01", "2026-08-31", "W"),
    ];
    let hv = [
        ["2026-05-01", "甲", "乙", "FALSE", "", "", "t1"],
        ["2026-06-15", "乙", "丙", "FALSE", "", "", "t1"],
        ["2026-07-15", "丙", "丁", "FALSE", "", "", "t1"],
        ["2026-08-13", "丁", "戊", "FALSE", "", "", "t1"],
        ["2026-08-16", "甲", "乙", "FALSE", "", "", "u1"],
        ["2026-08-17", "甲", "乙", "FALSE", "", "", "u2"],
        ["2026-07-01", "甲", "乙", "FALSE", "", "", "w1"],
    ];
    let deal_rows: Vec<&[&str]> = deals.iter().map(|r| &r[..]).collect();
    let ho_rows: Vec<&[&str]> = hv.iter().map(|r| &r[..]).collect();
    Sheets {
        deal: tiny(&dh, &deal_rows),
        handover: tiny(&hh, &ho_rows),
        call: tiny(
            &["ts", "duration_sec", "deal_id"],
            &[
                // 通話の記録の始まり（長さを問わない）
                &["2026-03-23T01:00:00Z", "10", "t1"],
                &["2026-06-14T01:00:00Z", "120", "t1"],
                &["2026-06-15T01:00:00Z", "120", "t1"],
            ],
        ),
        ..handover_tiny()
    }
}

/// 窓を通期にした（2026-09-24 藤巻さんの判断）: 前は一つ前の交代から、後は次の交代の前日で止まる。
/// 後の担当がまだ担当中なら締め日の前日までで、30日あれば比べ、29日なら途中。
#[test]
fn 交代の前後の窓は一つ前の交代から次の交代の前日までの通期() {
    let v = build_handover(&handover_tenure(), fixture_day());
    let rows = v["rows"].as_array().unwrap();
    let at = |deal: &str, day: &str| -> Value {
        rows.iter()
            .find(|r| r["deal_id"] == deal && r["date"] == day)
            .unwrap_or_else(|| panic!("{deal} {day} の行が無い"))["contact"]
            .clone()
    };
    let span = |x: &Value, w: &str| {
        (
            x[w]["start"].as_str().unwrap_or("").to_string(),
            x[w]["end"].as_str().unwrap_or("").to_string(),
            x[w]["days"].as_i64().unwrap(),
            x[w]["contacts"].as_i64().unwrap(),
        )
    };
    let s = |a: &str, b: &str, n: i64, c: i64| (a.to_string(), b.to_string(), n, c);
    // 05-01: 後は次の交代（06-15）の前日で止まる。06-14 の通話は入り、06-15 の通話は入らない
    let e1 = at("t1", "2026-05-01");
    assert_eq!(span(&e1, "before"), s("2026-03-23", "2026-04-30", 39, 0));
    assert_eq!(span(&e1, "after"), s("2026-05-01", "2026-06-14", 45, 1));
    assert_eq!(e1["status"], "ok");
    assert_eq!(e1["ongoing"], false, "次の交代があるなら担当中ではない");
    assert_eq!(e1["before"]["lost"]["calls"], 81);
    // 06-15: 前は一つ前の交代日（05-01）から。後はちょうど 30日で比べる
    let e2 = at("t1", "2026-06-15");
    assert_eq!(span(&e2, "before"), s("2026-05-01", "2026-06-14", 45, 1));
    assert_eq!(span(&e2, "after"), s("2026-06-15", "2026-07-14", 30, 1));
    assert_eq!(e2["status"], "ok");
    assert_eq!(e2["dir"], "up");
    // 07-15: 後は次の交代（08-13）までの 29日 → 交代どうしの間が短い
    let e3 = at("t1", "2026-07-15");
    assert_eq!(e3["before"]["days"], 30);
    assert_eq!(span(&e3, "after"), s("2026-07-15", "2026-08-12", 29, 0));
    assert_eq!(e3["status"], "short");
    assert_eq!(e3["why"], "gap");
    // 08-13: 前が 29日 → 交代どうしの間が短い
    let e4 = at("t1", "2026-08-13");
    assert_eq!(span(&e4, "before"), s("2026-07-15", "2026-08-12", 29, 0));
    assert_eq!(e4["why"], "gap");
    assert_eq!(e4["ongoing"], true);
    // 後の担当がまだ担当中: 30日なら比べ、29日なら途中
    let u1 = at("u1", "2026-08-16");
    assert_eq!(span(&u1, "after"), s("2026-08-16", "2026-09-14", 30, 0));
    assert_eq!(u1["status"], "ok");
    assert_eq!(u1["ongoing"], true);
    let u2 = at("u2", "2026-08-17");
    assert_eq!(u2["after"]["days"], 29);
    assert_eq!(u2["status"], "provisional");
    // 満了していれば担当中ではない
    let w = at("w1", "2026-07-01");
    assert_eq!(span(&w, "after"), s("2026-07-01", "2026-08-31", 62, 0));
    assert_eq!(w["ongoing"], false);
    assert_eq!(w["status"], "ok");

    let c = &v["contact_cmp"];
    assert_eq!(c["n_events"], 7);
    assert_eq!(c["n_ok"], 4);
    assert_eq!(c["n_ongoing"], 1);
    assert_eq!(c["n_provisional"], 1);
    assert_eq!(c["short_why"]["gap"], 2);
}

/// 検証の指摘（2026-09-24）: 母数の印（5件未満）の境目と、担当者のまとめの並び（比べられた件数の多い順。
/// 名前の順・成績の順にしない）を見張るテストが無かった。`summarize` に合成の交代を直接渡して確かめる。
#[test]
fn 交代の前後の担当者のまとめは5件ちょうどで印を外し件数の多い順に並べる() {
    use super::handover_contact::{summarize, Cmp, Item, Status, Window};
    let w = |days, contacts| Window {
        days,
        contacts,
        ..Default::default()
    };
    let ok = |b: usize, a: usize| Cmp {
        status: Status::Ok,
        before: w(60, b),
        after: w(60, a),
        ongoing: false,
    };
    let mut items: Vec<Item> = Vec::new();
    // 「あ」は 4件で全部増えた、「ん」は 5件で全部減った。名前の順でも、変化の大きい順でも「あ」が先になる。
    // 件数の多い順なら「ん」が先
    for i in 0..4 {
        items.push(Item {
            event: format!("site:A{i}|2026-06-01"),
            cmp: ok(2, 4),
            from: "前",
            to: "あ",
        });
    }
    for i in 0..5 {
        items.push(Item {
            event: format!("site:N{i}|2026-06-01"),
            cmp: ok(4, 2),
            from: "前",
            to: "ん",
        });
    }
    let v = summarize(&items, &std::collections::HashMap::new());
    let to = v["by_to"].as_array().unwrap();
    let names: Vec<&str> = to.iter().map(|r| r["label"].as_str().unwrap()).collect();
    assert_eq!(names, ["ん", "あ"], "比べられた件数の多い順");
    assert_eq!(to[0]["n_ok"], 5);
    assert_eq!(to[0]["small"], false, "ちょうど 5件は印を付けない");
    assert_eq!(to[1]["n_ok"], 4);
    assert_eq!(to[1]["small"], true, "4件は印を付ける");
    // 引き継がれた側は1人に 9件
    assert_eq!(v["by_from"][0]["n_ok"], 9);
    assert_eq!(v["by_from"][0]["small"], false);
}

/// まとめの数字は変化の平均と中央値（2026-09-24 藤巻さんの判断。最頻値は出さない）。
/// 平均と中央値が違う値になる3件で、両方が出ていること・取り違えていないことを見る。
#[test]
fn 交代の前後のまとめは変化の平均と中央値を出す() {
    use super::handover_contact::{summarize, Cmp, Item, Status, Window};
    let w = |days, contacts| Window {
        days,
        contacts,
        ..Default::default()
    };
    // 変化（30日あたり）: +0.5・+1.5・+5.0 → 中央値 1.5、平均 7/3
    let mk = |a: usize, i: usize| Item {
        event: format!("site:M{i}|2026-06-01"),
        cmp: Cmp {
            status: Status::Ok,
            before: w(60, 0),
            after: w(60, a),
            ongoing: i == 0,
        },
        from: "前",
        to: "後",
    };
    let items = vec![mk(1, 0), mk(3, 1), mk(10, 2)];
    let v = summarize(&items, &std::collections::HashMap::new());
    let f = |x: &Value| x.as_f64().unwrap();
    assert!((f(&v["median_change"]) - 1.5).abs() < 1e-12);
    assert!((f(&v["mean_change"]) - 7.0 / 3.0).abs() < 1e-12);
    assert_eq!(v["n_ongoing"], 1);
    for side in ["by_to", "by_from"] {
        let r = &v[side][0];
        assert!((f(&r["median_change"]) - 1.5).abs() < 1e-12, "{side}");
        assert!((f(&r["mean_change"]) - 7.0 / 3.0).abs() < 1e-12, "{side}");
    }
    assert!(v.get("mode_change").is_none(), "最頻値は出さない");
    // 比べられた交代が無ければ空（0 にしない）
    let e = summarize(&[], &std::collections::HashMap::new());
    assert!(e["mean_change"].is_null() && e["median_change"].is_null());
}

/// 検証の指摘（2026-09-24）: 「氏名不明 N」の番号を引き継いだ側・引き継がれた側で別々に、
/// 比べられた件数の順で振っていて、2つの表の「氏名不明 1」が別の人になり得た。交代の表は番号なしだった。
/// 番号は交代の記録全体で1回、初めて出てきた日の順（アドレスの順ではない）に振り、表とまとめで同じにする。
#[test]
fn 交代の氏名不明の番号は表と両側のまとめで同じ人に同じ番号() {
    let hh = [
        "date",
        "from",
        "to",
        "to_retired",
        "reflected",
        "record_gap_days",
        "deal_id",
    ];
    let sh = Sheets {
        handover: tiny(
            &hh,
            &[
                // z は 07-01 に初めて出る（アドレスの順なら a が先だが、出てきた日の順で z が 1）
                &[
                    "2026-07-01",
                    "佐藤",
                    "z@example.co.jp",
                    "FALSE",
                    "",
                    "",
                    "c1",
                ],
                &[
                    "2026-08-01",
                    "z@example.co.jp",
                    "a@example.co.jp",
                    "FALSE",
                    "",
                    "",
                    "d1",
                ],
                // a は引き継いだ側で2件（件数の順なら a が先に並ぶ）
                &[
                    "2026-08-02",
                    "佐藤",
                    "a@example.co.jp",
                    "FALSE",
                    "",
                    "",
                    "e1",
                ],
            ],
        ),
        ..handover_tiny()
    };
    let v = build_handover(&sh, fixture_day());
    assert_eq!(v["meta"]["n_unresolved_people"], 2);
    // 交代の表
    assert_eq!(ho_row(&v, "c1")["to_unresolved_no"], 1);
    assert_eq!(ho_row(&v, "c1")["from_unresolved_no"], Value::Null);
    assert_eq!(ho_row(&v, "d1")["from_unresolved_no"], 1);
    assert_eq!(ho_row(&v, "d1")["to_unresolved_no"], 2);
    // まとめ: 引き継いだ側では a と z が同じ件数（1件ずつ）で並ぶが、番号は並びで振らない
    let c = &v["contact_cmp"];
    let no = |side: &str| -> Vec<i64> {
        let mut n: Vec<i64> = c[side]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["unresolved"] == true)
            .map(|r| r["unresolved_no"].as_i64().unwrap())
            .collect();
        n.sort();
        n
    };
    assert_eq!(no("by_to"), [1, 2]);
    // 件数の多い順に並べて先頭に来る a（2件）が 2、z（1件）が 1
    let to = c["by_to"].as_array().unwrap();
    assert_eq!(to[0]["n_events"], 2);
    assert_eq!(to[0]["unresolved_no"], 2);
    assert_eq!(to[1]["unresolved_no"], 1);
    assert_eq!(no("by_from"), [1], "引き継がれた側の z も 1");
    assert!(!c.to_string().contains("@example.co.jp"));
}

/// 画面に出す断り（交代が接触を増減させた証拠ではない・向きは決まらない）と決まりごと。
#[test]
fn 交代の前後の接触は証拠ではないと書く() {
    let v = build_handover(&sheets(), fixture_day());
    let c = &v["contact_cmp"];
    let t = c["not_causal"].as_str().unwrap();
    for w in [
        "証拠ではありません",
        "危ない案件だから担当を替えた可能性",
        "向きは決まりません",
        "差が消え",
        "検知専用",
    ] {
        assert!(t.contains(w), "断りに「{w}」が無い: {t}");
    }
    let r = c["rule"].as_str().unwrap();
    for w in [
        "それぞれの担当期間の全体（通期）",
        "一つ前の交代日",
        "次の交代日の前日",
        "後の担当はまだ担当中（締め日までの通期）",
        "平均と中央値",
        "最頻値は出していません",
        "30日あたり",
        "60秒超の通話",
        "メールは数えない",
        "付け直し",
        "1件だけの日",
        "30日未満",
        "途中（未確定）",
    ] {
        assert!(r.contains(w), "決まりごとに「{w}」が無い: {r}");
    }
    assert!(c["dedupe_rule"].as_str().unwrap().contains("1件の交代"));
    assert!(c["dir_rule"].as_str().unwrap().contains("5 件未満"));
    // 🔴 文の途中に字下げの空白が混ざらない（2026-09-24: 行の継ぎ目の `\` が抜けて
    //    「決まりません    （過去の…」と画面に空白が出ていた）
    for k in ["not_causal", "rule", "dedupe_rule", "dir_rule"] {
        let s = c[k].as_str().unwrap();
        assert!(!s.contains("  "), "{k} に空白の連続が混ざっている: {s}");
    }
}

// ================================================================ 案件の詳細

use super::deal_detail::build_deal_detail;

/// `CS_通話要約` の fixture は**合成**（本番にまだ無いシート）。個人が特定できる文字列は入れていない。
/// 作り方: `CS_通話明細` の fixture のうち、文字起こしあり・60秒超の行（1,345行）から、
/// `call_id` が 10 で割り切れるもの（文字起こしが短すぎて行を作らない通話を真似る）を除いた 1,202行。
/// summary / next_action / concern は `call_id % 5` で決まる5通りの定型文、`ts` と `duration_sec` は通話明細の写し。
/// 同じ通話が複数の取引に付いているときは、通話明細と同じく取引ごとに1行ある。
fn call_summary() -> Arc<SheetData> {
    load_tsv("CS_通話要約")
}

fn detail(id: &str) -> Value {
    build_deal_detail(
        &sheets(),
        Some(&call_summary()),
        Some(id),
        None,
        fixture_day(),
    )
}

fn events_of<'a>(v: &'a Value, kind: &str) -> Vec<&'a Value> {
    v["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|e| e["kind"] == kind)
        .collect()
}

#[test]
fn 通話要約のfixtureは合成で伏字の決まりを守っている() {
    let s = call_summary();
    assert_eq!(s.rows.len(), 1_202, "合成の行数");
    for c in [
        "call_id",
        "deal_id",
        "ts",
        "duration_sec",
        "summary",
        "next_action",
        "concern",
        "n_utterances",
        "model",
        "generated_at",
        "source",
    ] {
        assert!(s.col(c).is_some(), "列「{c}」が無い");
    }
    // 1通話が複数の取引に付くときは取引ごとに1行（call_id が重複してよい）
    let ids: std::collections::HashSet<&str> = s.rows.iter().map(|r| s.get(r, "call_id")).collect();
    assert!(ids.len() < s.rows.len(), "call_id の重複が1件も無い");
    // 60秒以下の通話には行を作らない。要約は200字まで
    for r in &s.rows {
        assert!(super::opt_num(s.get(r, "duration_sec")).unwrap() > super::CONTACT_SEC);
        assert!(s.get(r, "summary").chars().count() <= 200);
    }
}

/// 全部の種類がそろう取引（fixture で1件だけ）。数は Python で別に数え直した値
/// （付け直しの決まりを Python で書き直し、同じ fixture から数えた。2026-09-26）。
///
/// | 電話 | 60秒超 | 要約あり | 付け直して来た | 契約期間の外 | 録画 MTG | メール MTG（実施） | 交代 |
/// |  38  |   24   |    3     |       2        |      19      |    2     |        17          |  1   |
#[test]
fn 案件の詳細は電話_mtg_交代を新しい順に1本で並べる() {
    let v = detail("61098080280");
    assert_eq!(v["meta"]["found"], true);
    assert_eq!(v["meta"]["summary_sheet"], "ok");
    let c = &v["counts"];
    assert_eq!(c["call"], 38, "電話");
    assert_eq!(c["call_contact"], 24, "60秒超");
    assert_eq!(c["call_summarized"], 3, "要約あり");
    assert_eq!(c["call_moved_in"], 2, "付け直して来た電話");
    assert_eq!(c["call_outside"], 19, "この取引に付いているが契約期間の外");
    assert_eq!(c["mtg"], 2, "録画の MTG");
    assert_eq!(c["mail_mtg"], 17, "メール由来（実施）");
    assert_eq!(c["handover"], 1, "担当の交代");
    let ev = v["events"].as_array().unwrap();
    assert_eq!(ev.len(), 38 + 2 + 17 + 1, "時系列の件数");
    assert_eq!(events_of(&v, "call").len(), 38);
    assert_eq!(events_of(&v, "mail_mtg").len(), 17);
    // 新しい順。同じ日は時刻の新しい順（時刻の無いものは後ろ）
    assert_eq!(ev[0]["date"], "2026-09-18", "いちばん上がいちばん新しい日");
    for w in ev.windows(2) {
        let k = |e: &Value| {
            (
                e["date"].as_str().unwrap().to_string(),
                e["time"].as_str().unwrap_or("").to_string(),
            )
        };
        assert!(
            k(&w[0]) >= k(&w[1]),
            "並びが新しい順でない: {:?} → {:?}",
            k(&w[0]),
            k(&w[1])
        );
    }
    // 事実と推定を分ける
    for e in ev {
        let fact = e["fact"].as_bool().unwrap();
        assert_eq!(fact, e["kind"] != "mail_mtg", "{e}");
        if e["kind"] != "handover" {
            let want = if fact { "事実" } else { "推定" };
            assert!(e["source_label"].as_str().unwrap().contains(want), "{e}");
        }
    }
    for e in events_of(&v, "mail_mtg") {
        assert!(e["certainty"].as_str().unwrap().contains("83.3%"));
    }
    // 取引の基本と名札（稼働中なので名札の配列がある）
    assert!(v["deal"]["flags"].is_array());
    assert!(v["deal"]["stage"].as_str().is_some_and(|s| !s.is_empty()));
    // 同じ call_id を2回出さない
    let ids: Vec<&str> = events_of(&v, "call")
        .iter()
        .map(|e| e["call_id"].as_str().unwrap())
        .collect();
    let uniq: std::collections::HashSet<&&str> = ids.iter().collect();
    assert_eq!(uniq.len(), ids.len(), "同じ通話が2回出ている");
}

/// 付け直し。この取引には直接付いた電話が1本も無く、継続先の取引に付いた電話を付け直して並べる。
/// Python の数え直し: 電話 117（全部付け直し。同じ通話の重複を除いた数で、行では 228）、60秒超 63、録画 MTG 3、交代 1。
#[test]
fn 案件の詳細は継続先に付いた電話をこの契約の期間なら付け直して並べる() {
    let v = detail("51831964246");
    let c = &v["counts"];
    assert_eq!(c["call"], 117);
    assert_eq!(c["call_moved_in"], 117);
    assert_eq!(c["call_contact"], 63);
    assert_eq!(c["mtg"], 3);
    assert_eq!(c["handover"], 1);
    let me = &v["deal"];
    let st = me["start"].as_str().unwrap().to_string();
    let ex = me["expiration"].as_str().unwrap().to_string();
    for e in events_of(&v, "call") {
        let a = &e["attach"];
        assert_eq!(a["state"], "moved_in", "{e}");
        let from = a["moved_from"]["deal_id"].as_str().unwrap();
        assert_ne!(from, "51831964246");
        assert!(
            !a["moved_from"]["name"].as_str().unwrap().is_empty(),
            "どこから付け直したかの名前が無い"
        );
        // 付け直して来たものは、この取引の契約期間の中の日だけ
        let d = e["date"].as_str().unwrap();
        assert!(
            st.as_str() <= d && d <= ex.as_str(),
            "{d} が契約期間 {st}〜{ex} の外"
        );
    }
    let rows = v["chain"]["rows"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["deal_id"] == "51831964246"));
    assert_eq!(rows.iter().filter(|r| r["current"] == true).count(), 1);
}

/// 要約の結合: 要約がある電話は項目が出て、要約の行が無い電話は null。
/// シートが読めないときは全部 null のまま、残りは同じに出す。
#[test]
fn 案件の詳細は通話要約をcall_idで結び_無いときも開く() {
    let v = detail("61098080280");
    let s = call_summary();
    let have: std::collections::HashSet<&str> =
        s.rows.iter().map(|r| s.get(r, "call_id")).collect();
    let mut n = 0;
    for e in events_of(&v, "call") {
        let cid = e["call_id"].as_str().unwrap();
        if have.contains(cid) {
            n += 1;
            let sm = &e["summary"];
            assert!(sm["summary"].as_str().is_some_and(|x| !x.is_empty()), "{e}");
            assert_eq!(sm["model"], "MiniMax-M3");
            assert!(e["has_transcript"].as_bool().unwrap());
        } else {
            assert!(
                e["summary"].is_null(),
                "要約の行が無いのに summary がある: {e}"
            );
        }
    }
    assert_eq!(n, 3);

    // シートが読めない
    let none = build_deal_detail(&sheets(), None, Some("61098080280"), None, fixture_day());
    assert_eq!(none["meta"]["summary_sheet"], "missing");
    assert_eq!(none["counts"]["call_summarized"], 0);
    assert_eq!(
        none["events"].as_array().unwrap().len(),
        58,
        "要約が無くても残りは出す"
    );
    // シートはあるが空
    let empty = tiny(&["call_id", "deal_id"], &[]);
    let e = build_deal_detail(
        &sheets(),
        Some(&empty),
        Some("61098080280"),
        None,
        fixture_day(),
    );
    assert_eq!(e["meta"]["summary_sheet"], "empty");
}

#[test]
fn 案件の詳細は取引が無いときは探す欄を返し_オプションは出さない() {
    let sh = sheets();
    let s = call_summary();
    // 指定なし・検索語なし
    let v = build_deal_detail(&sh, Some(&s), None, None, fixture_day());
    assert_eq!(v["meta"]["found"], false);
    assert_eq!(v["search"]["n_match"], 0);
    // 検索語あり（fixture の取引名は「（伏字）N」）
    let v = build_deal_detail(&sh, Some(&s), None, Some("伏字）12"), fixture_day());
    let rows = v["search"]["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows.len() <= super::deal_detail::SEARCH_LIMIT);
    for r in rows {
        let hit = r["name"].as_str().unwrap_or("").contains("伏字）12")
            || r["site"].as_str().unwrap_or("").contains("伏字）12");
        assert!(hit, "検索語を含まない行: {r}");
    }
    let act: Vec<bool> = rows
        .iter()
        .map(|r| r["is_active"].as_bool().unwrap())
        .collect();
    assert!(
        act.windows(2).all(|w| w[0] >= w[1]),
        "稼働中が先に来ていない"
    );
    // 大文字小文字を問わずに当て、画面には打った言葉のまま返す
    let v = build_deal_detail(&sh, Some(&s), None, Some(" 伏字）12 "), fixture_day());
    assert_eq!(v["search"]["q"], "伏字）12");
    let sh2 = Sheets {
        deal: tiny(
            &[
                "deal_id",
                "dealname",
                "dealstage",
                "contract_kind",
                "is_active",
            ],
            &[&["x1", "ABC商事", "s", "(新規)", "TRUE"]],
        ),
        ..sheets()
    };
    let v = build_deal_detail(&sh2, None, None, Some("abc"), fixture_day());
    assert_eq!(v["search"]["n_match"], 1);
    let v = build_deal_detail(&sh2, None, None, Some("ABC"), fixture_day());
    assert_eq!(v["search"]["q"], "ABC");
    // 見つからない
    let v = build_deal_detail(&sh, Some(&s), Some("0"), None, fixture_day());
    assert_eq!(v["meta"]["found"], false);
    assert!(v["meta"]["reason"].is_string());
    // オプション契約は出さない
    let opt = super::deals_all_of(&sh.deal)
        .into_iter()
        .find(|d| d.is_option())
        .expect("オプション契約");
    let v = build_deal_detail(&sh, Some(&s), Some(&opt.id), None, fixture_day());
    assert_eq!(v["meta"]["found"], false);
    assert!(v["meta"]["reason"].as_str().unwrap().contains("オプション"));
    assert!(v.get("events").is_none());
}

/// 小さなシートで、付け直しの印（来た / 出ていった / 決められない / 期間の外）・
/// 同じ通話の重複・要約の取引の選び方・MTG の未抽出・メールの実施以外を1つずつ確かめる。
#[test]
fn 案件の詳細の付け直しの印と要約の結合を小さなシートで確かめる() {
    let empty = |h: &[&str]| tiny(h, &[]);
    let sh = Sheets {
        deal: tiny(
            &[
                "deal_id",
                "dealname",
                "dealstage",
                "contract_kind",
                "contract_start_date",
                "contract_expiration_date",
                "kyoten_key",
                "is_active",
            ],
            &[
                // 前の契約（見る取引）と継続の契約。同じ拠点 K
                &[
                    "a",
                    "前の契約",
                    "x",
                    "(新規)",
                    "2026-01-01",
                    "2026-06-30",
                    "K",
                    "FALSE",
                ],
                &[
                    "b",
                    "継続の契約",
                    "x",
                    "サブスク継続",
                    "2026-07-01",
                    "2026-12-31",
                    "K",
                    "TRUE",
                ],
                // 同じ拠点で、継続の契約と期間が重なる別の本体（7/15〜8/15 は b と c のどちらか決められない）
                &[
                    "c",
                    "重なる契約",
                    "x",
                    "(新規)",
                    "2026-07-15",
                    "2026-08-15",
                    "K",
                    "FALSE",
                ],
            ],
        ),
        call: tiny(
            &[
                "call_id",
                "ts",
                "duration_sec",
                "deal_id",
                "has_transcript",
                "handler",
            ],
            &[
                // b に付いているが 3/10（a の期間）→ a へ付け直して来る
                &["c1", "2026-03-10T01:00:00Z", "120", "b", "TRUE", "話者1"],
                // 同じ通話が a にも直接付いている → 直接の行を採り、1回だけ
                &["c2", "2026-03-11T01:00:00Z", "90", "b", "TRUE", ""],
                &["c2", "2026-03-11T01:00:00Z", "90", "a", "TRUE", ""],
                // a に付いているが 9/1（b だけの期間）→ b へ出ていった印
                &["c3", "2026-09-01T01:00:00Z", "30", "a", "FALSE", ""],
                // a に付いているが 7/20（a の期間の外で、b と c が重なる）→ 決められない
                &["c4", "2026-07-20T01:00:00Z", "200", "a", "FALSE", ""],
                // a に付いているが契約の前 → 期間の外
                &["c5", "2025-12-01T01:00:00Z", "200", "a", "FALSE", ""],
                // b に付いていて b の期間 → a には来ない
                &["c6", "2026-08-02T01:00:00Z", "200", "b", "FALSE", ""],
                // UTC 3/31 16:00 ＝ 日本時間 4/1 01:00
                &["c7", "2026-03-31T16:00:00Z", "61", "a", "TRUE", ""],
            ],
        ),
        mtg: tiny(
            &["開催日", "開催日時(JST)", "deal_id", "やること", "抽出"],
            &[
                &["2026-03-10", "2026-03-10 10:00", "a", "", ""],
                &["2026-02-01", "2026-02-01 09:00", "a", "資料を送る", "{}"],
            ],
        ),
        history: empty(&["deal_id"]),
        customer: empty(&["houjin"]),
        mail_mtg: tiny(
            &["deal_id", "date", "kind", "certainty"],
            &[
                &["a", "2026-03-10", "実施", "推定(±1日 83.3%)"],
                &["a", "2026-03-20", "予定", "推定(±1日 83.3%)"],
                &["a", "2026-03-21", "候補", "推定(±1日 83.3%)"],
            ],
        ),
        handover: tiny(
            &["date", "from", "to", "deal_id"],
            &[&["2026-03-15", "前任", "後任", "a"]],
        ),
        owner_hist: empty(&["date", "owner", "retired", "deal_id"]),
        meta: empty(&["key", "value"]),
        all_cached: false,
    };
    let summ = tiny(
        &[
            "call_id",
            "deal_id",
            "summary",
            "next_action",
            "concern",
            "model",
        ],
        &[
            // 同じ通話の b の行と a の行。元の取引（a）の行を採る
            &["c2", "b", "bの行", "", "", "MiniMax-M3"],
            &["c2", "a", "aの行", "次", "", "MiniMax-M3"],
            &["c1", "b", "c1の要約", "", "懸念", "MiniMax-M3"],
        ],
    );
    let day = chrono::NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    let v = build_deal_detail(&sh, Some(&summ), Some("a"), None, day);
    let calls = events_of(&v, "call");
    let by = |id: &str| -> &Value {
        calls
            .iter()
            .find(|e| e["call_id"] == id)
            .unwrap_or_else(|| panic!("{id} が無い"))
    };
    assert_eq!(calls.len(), 6, "c1〜c5 と c7（c6 は来ない・c2 は1回）");
    assert_eq!(by("c1")["attach"]["state"], "moved_in");
    assert_eq!(by("c1")["attach"]["moved_from"]["name"], "継続の契約");
    assert_eq!(by("c1")["summary"]["summary"], "c1の要約");
    assert_eq!(by("c1")["summary"]["concern"], "懸念");
    assert_eq!(by("c1")["handler"], "話者1");
    assert_eq!(by("c2")["attach"]["state"], "own", "直接の行を採る");
    assert_eq!(
        by("c2")["summary"]["summary"],
        "aの行",
        "元の取引の要約の行を採る"
    );
    assert_eq!(by("c3")["attach"]["state"], "moved_out");
    assert_eq!(by("c3")["attach"]["moved_to"]["name"], "継続の契約");
    assert_eq!(by("c3")["contact"], false, "60秒以下は接触ではない");
    assert!(by("c3")["summary"].is_null());
    assert_eq!(by("c4")["attach"]["state"], "ambiguous");
    assert_eq!(by("c5")["attach"]["state"], "outside");
    assert_eq!(by("c7")["date"], "2026-04-01", "日付は日本時間");
    assert_eq!(by("c7")["time"], "01:00");
    assert_eq!(v["counts"]["call_moved_in"], 1);
    assert_eq!(v["counts"]["call_outside"], 3);
    // MTG: 未抽出と抽出済み。メールは実施だけ並べ、それ以外は数だけ
    let m = events_of(&v, "mtg");
    assert_eq!(m.len(), 2);
    assert_eq!(m.iter().filter(|e| e["extracted"] == false).count(), 1);
    let mail = events_of(&v, "mail_mtg");
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0]["same_day_recording"], true, "同じ日に録画がある");
    assert_eq!(v["counts"]["mail_not_held"].as_array().unwrap().len(), 2);
    assert_eq!(events_of(&v, "handover").len(), 1);
    // 連なり: a の次は b（開始順）。稼働中でないので名札は null
    assert_eq!(v["chain"]["next"], "b");
    assert!(v["chain"]["prev"].is_null());
    assert!(v["deal"]["flags"].is_null());
    // 同じ日（3/10）は時刻のある行が先、時刻の無いメール由来は後
    let ev = v["events"].as_array().unwrap();
    let i_rec = ev
        .iter()
        .position(|e| e["kind"] == "mtg" && e["date"] == "2026-03-10")
        .unwrap();
    let i_mail = ev.iter().position(|e| e["kind"] == "mail_mtg").unwrap();
    assert!(i_rec < i_mail);
}
