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
/// | 継続回数 | 件数 | 解約＋充足 | 解約率 |（オプションを外した実測）
/// | 初回  | 1,914 | 959 | 50.1% |
/// | 継続1 |   734 | 283 | 38.6% |
/// | 継続2 |   383 | 110 | 28.7% |
///
/// 初回の解約率が 46.8% → 50.1% と上がるのは、オプション契約が
/// 「初回・決着しない（＝解約にも継続にも数えない）」側へ偏って入っていたため。
#[test]
fn 継続回数ごとの解約率が資料の値と一致する() {
    let v = build_renewal(&sheets(), false);
    for (no, n, cancel_plus_fill, pct) in [
        (0, 1914, 959, 50.1),
        (1, 734, 283, 38.6),
        (2, 383, 110, 28.7),
        (3, 182, 39, 21.4),
        (5, 49, 4, 8.2),
    ] {
        let r = renewal(&v, no);
        assert_eq!(r["n"], n, "継続{no} の件数");
        let got_cf = r["cancel"].as_i64().unwrap() + r["fill"].as_i64().unwrap();
        assert_eq!(got_cf, cancel_plus_fill, "継続{no} の解約＋充足");
        let got = r["cancel_rate"].as_f64().expect("率が null");
        assert!(
            (got - pct).abs() < 0.05,
            "継続{no} の解約率が {got:.1}%。期待は {pct}%"
        );
    }
}

/// 充足を外すと 50.1% が 40.4% に見える。
///
/// **外さないのが確定した定義**。両方返しているのは画面で並べて見せるためで、
/// 主値を取り違えていないことをここで固定する。
#[test]
fn 充足を外した値は別のキーで返る() {
    let v = build_renewal(&sheets(), false);
    let r = renewal(&v, 0);
    let main = r["cancel_rate"].as_f64().unwrap();
    let excl = r["cancel_rate_excl_fill"].as_f64().unwrap();
    assert!((main - 50.1).abs() < 0.05, "主値が {main:.1}%");
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

    let all = &v["goal_all"];
    assert_eq!(all["pop"], 3432);
    assert_eq!(all["has_goal"], 1146);
}

/// 求人票あたり応募効率。**資料の 8.1 / 14.5 は再現しない。**
///
/// 向き（解約 < 継続）は合うが、中央値で見ると差はほとんど無い。
/// ここを「再現する」と書き換えたくなったら、まず実データを数え直すこと。
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
    let keizoku = g("継続した / 稼働中");
    let kaiyaku = g("解約した");
    let juusoku = g("充足（採れて終わった）");

    assert_eq!(keizoku["n"], 1282);
    assert_eq!(kaiyaku["n"], 529);
    assert_eq!(juusoku["n"], 142);

    let m_keizoku = keizoku["mean"].as_f64().unwrap();
    let m_kaiyaku = kaiyaku["mean"].as_f64().unwrap();
    assert!(
        m_kaiyaku < m_keizoku,
        "平均の向きが逆になっている（解約 {m_kaiyaku:.1} / 継続 {m_keizoku:.1}）"
    );
    // 資料の値は 8.1 vs 14.5。実データはここまで離れていない
    assert!(
        (m_keizoku - 14.5).abs() > 1.0,
        "継続の平均が {m_keizoku:.1}。資料の 14.5 に一致したなら、
         『再現しない』と書いた注意書きのほうを直すこと"
    );

    let med_keizoku = keizoku["median"].as_f64().unwrap();
    let med_kaiyaku = kaiyaku["median"].as_f64().unwrap();
    assert!(
        (med_keizoku - med_kaiyaku).abs() < 1.0,
        "中央値の差が {:.2} ある。ほとんど差が無いという読みが変わる",
        med_keizoku - med_kaiyaku
    );

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
    assert_eq!(v["risk"]["ax3"]["白"], 400);
    assert_eq!(v["risk"]["ax3"]["赤"], 140);
    assert_eq!(v["risk"]["ax3"]["未測定"], 64);

    // 2026-09-21: 白 548 -> 547 / 未測定 1 -> 2。
    //   金額 0 を「入っていない」扱いに変えたため（0円の契約は存在しない）。
    //   稼働中で amount=0 が1件あり、それが白から未測定へ移った。
    assert_eq!(v["risk"]["ax4"]["白"], 448);
    assert_eq!(v["risk"]["ax4"]["赤"], 154);
    assert_eq!(v["risk"]["ax4"]["未測定"], 2);

    assert_eq!(band_n(&v, "0＝安定"), 336);
    assert_eq!(band_n(&v, "1＝要注意"), 242);
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
    assert_eq!(nm["first_active"], 264);
    assert_eq!(nm["n"], 114);
    assert_eq!(nm["rows"].as_array().unwrap().len(), 114);
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
    assert_eq!(zero, 81, "接触ゼロの行が日数 null になっていない");
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
    assert_eq!(t["n"], 1372);
    assert_eq!(t["rows"], 20843);
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
    assert_eq!(v["meta"]["n_mtg"], 3133);
    assert_eq!(v["linked"]["n"], 3133, "取引に結べた MTG");

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
    assert_eq!(sum, 3133, "リスク判定の内訳が母数と合わない");
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
    assert_eq!(sheets_listed.len(), 7);
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
