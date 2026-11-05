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

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Command, Stdio};

use super::checks::defs;
use super::judge::StoredItem;
use super::routes::{read_contacted, read_contacted_before};
use super::store::ReviewRow;
use crate::audit::fake_turso::{start_sqlite_audit_with_url, SharedConn};
use crate::db::turso_http::TursoDb;

const SERVER_MARK: &str = "BENCH_SERVER_URL=";
/// 子プロセスの偽サーバにだけ付ける環境変数 (`cargo test -- --ignored` で偽サーバが単独で走って待ち続けないように)
const CHILD_ENV: &str = "TALK_REVIEW_BENCH_CHILD";
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
            let items: Vec<StoredItem> = ids
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
