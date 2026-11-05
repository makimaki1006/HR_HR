//! 行・印の読み込みのメモリ計測 (すべて `#[ignore]`。通常のテストでは走らない)。
//!
//! 本番で summary が 1 回で 170〜420MB 増えた (2026-11-04 にメモリの上限 2GB を超えて落ちた)。
//! 読み方を変える前と後で、読む側のプロセスの最大メモリがどれだけ違うかを測る。
//!
//! 偽 Turso (SQLite) を**別のプロセス**で立て (`bench_server`)、読む側 (`bench_read_*`) はそこへ HTTP で
//! つなぐ。同じプロセスで立てると、偽サーバが応答を作るメモリが読む側の数字に混ざるため。
//! データは判定済みの行 20,000 本 (担当者 50 人、62 日にわたる) と、同じ数の接触の印。
//! 行の 1 本は、9 項目それぞれに引用 60 字・助言 80 字・メモ 50 字、まとめ 150 字 (日本語)。
//!
//! 手順 (macOS。`maximum resident set size` はバイト):
//! ```text
//! cargo test --lib crm::talk_review::memory_bench --no-run      # 最後の行にテストの実行ファイルの場所が出る
//! BIN=target/debug/deps/rust_dashboard-xxxx
//! for t in bench_connect_only bench_read_before bench_read_after; do
//!   /usr/bin/time -l $BIN crm::talk_review::memory_bench::$t --ignored --exact --nocapture 2>&1 \
//!     | grep -E "BENCH_|maximum resident"
//! done
//! ```
//! `bench_connect_only` は何も読まない基準 (プロセスと HTTP クライアントだけ)。
//! 前と後の `BENCH_RESULT` の `kept` と `fingerprint` が同じなら、同じ行を読めている。
//!
//! ヒアリングの周回の起動時の読み取り (worker の `find_rejudge`、`bench_hearing_*`) は、読んだあとに持ち続ける量も
//! 測る。手順は下の「ヒアリングの周回の起動時の読み取り」の節。

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};

use chrono::{DateTime, Utc};

use super::apo_link;
use super::checks::{defs, loaded};
use super::judge::{StoredItem, SCHEDULED};
use super::routes::{read_contacted, read_contacted_before};
use super::store::{self, ReviewRow};
use super::worker::{has_hearing, row_has_foreign_text, scan_judged};
use crate::audit::fake_turso::{start_sqlite_audit_with_url, SharedConn};
use crate::db::turso_http::TursoDb;

const SERVER_MARK: &str = "BENCH_SERVER_URL=";
/// 子プロセスの偽サーバにだけ付ける環境変数 (`cargo test -- --ignored` で偽サーバが単独で走って待ち続けないように)
const CHILD_ENV: &str = "TALK_REVIEW_BENCH_CHILD";
/// 設定すると、その本数に 1 本の行の助言に日本語以外を混ぜる (ヒアリングの計測で、判定し直す行の持ち分を見る)
const FOREIGN_ENV: &str = "TALK_REVIEW_BENCH_FOREIGN_EVERY";
const ROWS: usize = 20_000;
const OWNERS: usize = 50;
/// 期間: JST の 2026-09-01 0 時 (= UTC の前日 15 時) から 62 日
const FROM: &str = "2026-08-31T15:00:00Z";
const TO: &str = "2026-11-01T15:00:00Z";
/// 行の間隔 (秒)。20,000 本で 61.8 日
const STEP_SECS: i64 = 267;

/// 日本語の文字列を `chars` 字だけ作る (`seed` で書き出しをずらし、行ごとに違う文にする)
fn jp(seed: usize, chars: usize) -> String {
    const BASE: &str = "採用のご担当者様にお取り次ぎいただき、来週の打ち合わせの日程についてご相談したいのですが、\
        ご都合のよい曜日はございますか。まずは現状のお困りごとを伺えればと存じます。";
    BASE.chars().cycle().skip(seed % 37).take(chars).collect()
}

fn at(i: usize, plus_secs: i64) -> String {
    let base = chrono::DateTime::parse_from_rfc3339("2026-08-31T15:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    super::store::ts_text(base + chrono::Duration::seconds(i as i64 * STEP_SECS + plus_secs))
}

/// 判定済みの行 20,000 本と、同じ担当者・5 秒後の接触の印 20,000 本を入れる
fn seed(conn: &SharedConn) {
    let ids: Vec<String> = defs().iter().take(9).map(|d| d.id.clone()).collect();
    let foreign_every: Option<usize> = std::env::var(FOREIGN_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0);
    let c = conn.lock().unwrap();
    c.execute_batch("BEGIN").unwrap();
    {
        let mut row = c
            .prepare(
                "INSERT INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, \
                 zoom_call_log_id, status, attempts, checks_version, model, outcome, reached_person, \
                 reception, first_date_turn, items, summary, note, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 95, ?4, 'judged', 1, 'v1', 'MiniMax-M3', '決まらなかった', 1, 0, 3, \
                 ?5, ?6, 'contacted', 'c', 'u')",
            )
            .unwrap();
        let mut mark = c
            .prepare(
                "INSERT INTO call_contact_marks (zoom_call_id, owner_id, call_at, connected, status, \
                 contacted, date_proposals, attempts, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 1, 'judged', 1, 0, 0, 'c', 'u')",
            )
            .unwrap();
        for i in 0..ROWS {
            let mut items: Vec<StoredItem> = ids
                .iter()
                .enumerate()
                .map(|(k, id)| StoredItem {
                    id: id.clone(),
                    status: ["done", "not_done", "na"][(i + k) % 3].into(),
                    evidence: vec![3, 4, 5],
                    quote: Some(jp(i + k, 60)),
                    advice: Some(jp(i + k + 1, 80)),
                    note: Some(jp(i + k + 2, 50)),
                    answer: None,
                    asked: None,
                    answered: None,
                })
                .collect();
            // ヒアリングの計測だけ: N 本に 1 本、助言に日本語以外が混ざる行 (判定し直す行) にする
            if foreign_every.is_some_and(|n| i % n == 0) {
                items[0].advice = Some("먼저 용건을 전하고 나서 일정을 묻는다".repeat(3));
            }
            let owner = format!("1000{:02}", i % OWNERS);
            row.execute(rusqlite::params![
                format!("{:011}", 90_000_000_000u64 + i as u64),
                owner,
                at(i, 0),
                format!("zoom-{i}"),
                serde_json::to_string(&items).unwrap(),
                jp(i, 150),
            ])
            .unwrap();
            mark.execute(rusqlite::params![format!("zm{i}"), owner, at(i, 5)])
                .unwrap();
        }
    }
    c.execute_batch("COMMIT").unwrap();
}

/// 偽 Turso を立ててデータを入れ、URL を 1 行出して、親が標準入力を閉じるまで動き続ける。
/// 読む側のテストが子プロセスとして起動する (単独で走らせる必要はない)
#[tokio::test(flavor = "multi_thread")]
#[ignore = "メモリ計測用。bench_read_* が子プロセスとして起動する"]
async fn bench_server() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let (_audit, conn, _queries, url) = start_sqlite_audit_with_url().await;
    seed(&conn);
    println!("{SERVER_MARK}{url}");
    tokio::task::spawn_blocking(|| {
        let mut sink = String::new();
        let _ = std::io::stdin().read_to_string(&mut sink);
    })
    .await
    .unwrap();
}

/// 子プロセスの偽サーバ。落とすときは標準入力を閉じて kill する (待たない = 親の最大メモリに数えない)
struct Server {
    child: Child,
    _stdout: BufReader<ChildStdout>,
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.kill();
    }
}

// 子を wait しない: `time -l` は wait した子孫の最大メモリも親の数字に混ぜる (大きい方になる) ため、
// 偽サーバのメモリが読む側の計測に入ってしまう。子は標準入力を閉じて kill し、親の終了で片付く
#[allow(clippy::zombie_processes)]
fn start_server() -> (Server, String) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "crm::talk_review::memory_bench::bench_server",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("偽サーバを起動できない");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            out.read_line(&mut line).unwrap() > 0,
            "偽サーバが URL を出す前に終わった"
        );
        // libtest は「test 名 ... 」を改行なしで出すので、行の途中にある印を探す
        if let Some((_, url)) = line.trim().split_once(SERVER_MARK) {
            let url = url.to_string();
            return (
                Server {
                    child,
                    _stdout: out,
                },
                url,
            );
        }
    }
}

/// 読んだ行の指紋 (前と後で同じ行を読めたかの確認。軽い行にも残る欄だけで作る)
fn fingerprint(rows: &[ReviewRow]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |s: &str| {
        for b in s.bytes().chain(std::iter::once(0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for r in rows {
        eat(&r.hubspot_call_id);
        eat(&r.owner_id);
        eat(&r.call_at);
        eat(&r.status);
        for i in &r.items {
            eat(&i.id);
            eat(&i.status);
        }
    }
    h
}

/// このプロセスの今の常駐メモリ (KB)。`ps` で読む
fn rss_kb() -> u64 {
    Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

/// 読み終えた行を持ったままの常駐メモリ (= キャッシュに持っている間の姿) と、手放した後の常駐メモリも出す
fn report(variant: &str, rows: Vec<ReviewRow>, started: std::time::Instant) {
    println!(
        "BENCH_RESULT variant={variant} kept={} fingerprint={:016x} read_secs={:.2} rss_holding_rows_kb={}",
        rows.len(),
        fingerprint(&rows),
        started.elapsed().as_secs_f64(),
        rss_kb()
    );
    assert_eq!(rows.len(), ROWS);
    drop(rows);
    println!("BENCH_AFTER_DROP variant={variant} rss_kb={}", rss_kb());
}

/// このプロセスが今持っているヒープの大きさ (KB)。`vmmap` の MALLOC ZONE の表の TOTAL 行の ALLOCATED。
/// 手放した行のページは OS に返らず常駐メモリ (RSS) が下がらないので、「読んだあとに持ち続けている量」は
/// RSS でなくこちらで測る (macOS。`vmmap` の単位は MiB、小数 1 桁)
fn live_heap_kb() -> f64 {
    let out = Command::new("vmmap")
        .args(["-summary", &std::process::id().to_string()])
        .output()
        .expect("vmmap を実行できない");
    let text = String::from_utf8_lossy(&out.stdout);
    let allocated = text
        .lines()
        .skip_while(|l| !l.starts_with("MALLOC ZONE"))
        .find(|l| l.starts_with("TOTAL"))
        .and_then(|l| l.split_whitespace().nth(6))
        .expect("vmmap の MALLOC ZONE の表を読めない");
    let (num, unit) = allocated.split_at(allocated.len() - 1);
    match unit {
        "K" => num.parse::<f64>().unwrap(),
        "M" => num.parse::<f64>().unwrap() * 1024.0,
        "G" => num.parse::<f64>().unwrap() * 1024.0 * 1024.0,
        // 単位が無いときはバイト
        _ => allocated.parse::<f64>().unwrap() / 1024.0,
    }
}

#[test]
#[ignore = "メモリ計測用。手順はファイル先頭"]
fn bench_connect_only() {
    let (_server, url) = start_server();
    let t = TursoDb::new(&url, "bench").unwrap();
    println!("BENCH_RESULT variant=connect_only kept=0 fingerprint=0 read_secs=0");
    drop(t);
}

/// 変更前の読み方: 期間の行を全部の列で 1 回、印を全員分 1 回で読む
#[test]
#[ignore = "メモリ計測用。手順はファイル先頭"]
fn bench_read_before() {
    let (_server, url) = start_server();
    let t = TursoDb::new(&url, "bench").unwrap();
    let started = std::time::Instant::now();
    let rows = read_contacted_before(&t, FROM, TO, None).unwrap();
    report("before", rows, started);
}

/// 変更後の読み方: 軽い列だけを 500 行ずつ、印を 2,000 件ずつ読む (summary の経路。`with_summary` なし)
#[test]
#[ignore = "メモリ計測用。手順はファイル先頭"]
fn bench_read_after() {
    let (_server, url) = start_server();
    let t = TursoDb::new(&url, "bench").unwrap();
    let started = std::time::Instant::now();
    let rows = read_contacted(&t, FROM, TO, None, false).unwrap();
    report("after", rows, started);
}

// ---- ヒアリングの周回の起動時の読み取り (worker の `find_rejudge`) ----------------------------------
//
// 判定済みの行 (直近 `backfill_days + 2` 日) を読み、日本語以外が混ざる行は判定し直す列へ、残りはヒアリングを
// 確かめる一覧 (`hearing_check`) に入れる。この一覧は決まるまで持ち続けるので、次の 2 つを測る。
// - 読み取りの最大メモリ: `/usr/bin/time -l` の `maximum resident set size` (`peak memory footprint` も出る)
// - 読んだあとに持ち続ける量: 読む前と読んだ後の `live_heap_kb` の差 (`BENCH_HEARING` の `retained_kb`)
//
// ```text
// BIN=target/debug/deps/rust_dashboard-xxxx
// for t in bench_connect_only bench_hearing_before bench_hearing_after; do
//   /usr/bin/time -l $BIN crm::talk_review::memory_bench::$t --ignored --exact --nocapture 2>&1 \
//     | grep -E "BENCH_|maximum resident|peak memory footprint"
// done
// ```
// `live_heap_kb` が呼ぶ `vmmap` は、終わった子プロセスとして `time -l` の `maximum resident set size` に
// 混ざる (vmmap の約 38MB。`peak memory footprint` は混ざらない)。最大メモリを `maximum resident set size` で
// 測る回は `TALK_REVIEW_BENCH_NO_HEAP=1` を付けて `vmmap` を呼ばない (`retained_kb` は出ない)。
// `TALK_REVIEW_BENCH_FOREIGN_EVERY=20` を付けると、20 本に 1 本 (5%) の行の助言に日本語以外が混ざり、
// 判定し直す行として全部の欄のまま持つ分が増える。

/// `vmmap` を呼ばない回 (最大メモリを `maximum resident set size` で測る回)
const NO_HEAP_ENV: &str = "TALK_REVIEW_BENCH_NO_HEAP";

/// 読む前の `live_heap_kb` (`vmmap` を呼ばない回は `None`)
fn heap_mark() -> Option<f64> {
    std::env::var_os(NO_HEAP_ENV).is_none().then(live_heap_kb)
}

/// ヒアリングを確かめる 1 行の事実 (変更前の全部の欄の行と変更後の軽い型の両方から同じ形で作り、
/// 同じ行を持てたかを指紋で確かめる)
fn hearing_fact(
    id: &str,
    call_ms: i64,
    scheduled: bool,
    hearing: bool,
    zoom: Option<&str>,
) -> String {
    format!(
        "{id}|{call_ms}|{scheduled}|{hearing}|{}",
        zoom.unwrap_or("-")
    )
}

fn facts_fingerprint(mut facts: Vec<String>) -> u64 {
    facts.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in &facts {
        for b in f.bytes().chain(std::iter::once(0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// 読み取りの結果を持ったまま測った値
struct Held {
    read_secs: f64,
    /// 読んだあとに持ち続けている量 (KB)
    retained_kb: Option<f64>,
    rss_kb: u64,
}

/// 読み終えた直後 (指紋のために文字列を作る前) に、持ち続けている量と今の常駐メモリを測る
fn measure_held(heap_before: Option<f64>, started: std::time::Instant) -> Held {
    let read_secs = started.elapsed().as_secs_f64();
    let retained_kb = heap_before.map(|before| live_heap_kb() - before);
    Held {
        read_secs,
        retained_kb,
        rss_kb: rss_kb(),
    }
}

/// 読んだ行の数・指紋と、測った値を出す。`rejudge` と `hearing` は、判定し直す行と確かめる一覧の行の事実
fn report_hearing(variant: &str, rejudge: &[String], hearing: &[String], held: &Held) {
    let all: Vec<String> = rejudge.iter().chain(hearing).cloned().collect();
    println!(
        "BENCH_HEARING variant={variant} read={} rejudge={} hearing={} fingerprint={:016x} read_secs={:.2} \
         retained_kb={} rss_holding_kb={}",
        rejudge.len() + hearing.len(),
        rejudge.len(),
        hearing.len(),
        facts_fingerprint(all),
        held.read_secs,
        held.retained_kb
            .map_or("none".to_string(), |kb| format!("{kb:.0}")),
        held.rss_kb
    );
}

/// 変更前の `find_rejudge`: 判定済みの行を全部の列で 1 回の問い合わせで読み、日本語以外が混ざる行 (判定し直す) 以外を
/// 全部の欄のまま、確かめる一覧に持つ (一覧の値は行と次に確かめる時刻)
#[allow(clippy::type_complexity)]
fn hearing_scan_before(
    t: &TursoDb,
    since: &str,
) -> (
    VecDeque<ReviewRow>,
    HashMap<String, (ReviewRow, DateTime<Utc>)>,
) {
    let rows = store::judged_since_single_query(t, since).unwrap();
    let (rejudge, rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(row_has_foreign_text);
    let now = Utc::now();
    (
        rejudge.into(),
        rest.into_iter()
            .map(|r| (r.hubspot_call_id.clone(), (r, now)))
            .collect(),
    )
}

#[test]
#[ignore = "メモリ計測用。手順はファイル先頭"]
fn bench_hearing_before() {
    let (_server, url) = start_server();
    let t = TursoDb::new(&url, "bench").unwrap();
    let heap_before = heap_mark();
    let started = std::time::Instant::now();
    let (rejudge, check) = hearing_scan_before(&t, FROM);
    let held = measure_held(heap_before, started);
    let checks = &loaded().checks;
    let fact = |r: &ReviewRow| {
        hearing_fact(
            &r.hubspot_call_id,
            apo_link::call_ms(&r.call_at).unwrap(),
            r.outcome.as_deref() == Some(SCHEDULED),
            has_hearing(r, checks),
            r.zoom_call_log_id.as_deref(),
        )
    };
    let rejudge_facts: Vec<String> = rejudge.iter().map(fact).collect();
    let check_facts: Vec<String> = check.values().map(|(r, _)| fact(r)).collect();
    report_hearing("before", &rejudge_facts, &check_facts, &held);
    // 測り終えるまで手放さない
    std::hint::black_box((&rejudge, &check));
}

/// 変更後の読み方: 判定済みの行を 500 行ずつ読み、日本語以外が混ざる行だけ全部の欄で持ち、残りは軽い型にする
#[test]
#[ignore = "メモリ計測用。手順はファイル先頭"]
fn bench_hearing_after() {
    let (_server, url) = start_server();
    let t = TursoDb::new(&url, "bench").unwrap();
    let heap_before = heap_mark();
    let started = std::time::Instant::now();
    let scan = scan_judged(&t, FROM, &loaded().checks, Utc::now(), store::ROWS_PAGE).unwrap();
    let held = measure_held(heap_before, started);
    let fact = |r: &ReviewRow| {
        hearing_fact(
            &r.hubspot_call_id,
            apo_link::call_ms(&r.call_at).unwrap(),
            r.outcome.as_deref() == Some(SCHEDULED),
            has_hearing(r, &loaded().checks),
            r.zoom_call_log_id.as_deref(),
        )
    };
    let rejudge_facts: Vec<String> = scan.rejudge.iter().map(fact).collect();
    let check_facts: Vec<String> = scan
        .hearing
        .values()
        .map(|c| {
            hearing_fact(
                &c.id,
                c.call_ms,
                c.scheduled,
                c.has_hearing,
                c.zoom_call_log_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(scan.read, rejudge_facts.len() + check_facts.len());
    report_hearing("after", &rejudge_facts, &check_facts, &held);
    std::hint::black_box(&scan);
}
