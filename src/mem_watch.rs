//! メモリの使用量を 15 秒おきに測り、1 分おきにログへ出す (2026-10-25)。
//! 前回から 100MB 以上増えたら、そのとき処理中の要求と、直前に終わった要求を一緒に出す。
//! 昼の短い山 (1.9GB) の原因を特定するための計測で、値は読むだけ。Linux 以外では何もしない。
//!
//! 裏の処理 (起動時の読み込み・定期の作り直し・worker の周回) は `task("名前")` を持って包む (2026-11-05)。
//! 要求と同じく、持っている間が「走っている処理」、手放すと「終わった処理」(名前・かかった時間・始めと終わりの RSS)。
//! 包む粒度は 1 回の作り直し・1 回の周回・起動時の 1 回の読み込みまで (1 件ごとのループは包まない)。
//! 名前は固定の文字列 (`job_copy.index_build` のように 領域.処理)。クエリや個人情報は入れない。
//!
//! Render のログでの読み方:
//! - `q=mem task`: 終わった処理の 1 行。RSS が 50MB 以上動いたか、60 秒以上かかったものだけ
//!   (1 時間に 20 行まで。うち時間だけの行は 10 行まで。出せなかった分は次の行の `skipped=` に数が付く)。
//! - `q=mem jump`: 100MB 以上増えた瞬間。`tasks=` が走っている処理 (名前 経過ms 始めからの RSS の増減)、
//!   `tasks_done=` が前の行から終わった処理 (名前 かかったms RSS の差。差の大きい順)。毎分の `mem` の行にも付く。
//! - RSS はプロセス全体の値なので、同時に走る別の処理や要求の分も含む。重なりは `tasks=` と `inflight=` で見る。
//! - 毎分の `mem` と `mem jump` の行の末尾 (`tasks_done=` の後ろ) に、割り当ての仕組み (glibc の malloc) の数字が付く (2026-11-05)。
//!   Linux の glibc のときだけで、それ以外は項目ごと出ない。`mem task` の行には付けない。
//!   `malloc_inuse_mb` は使用中 (確保したまま解放されていない分)、`malloc_free_mb` は空き (解放済みで、割り当ての仕組みが手元に持っていて、
//!   OS に返していない分)、`malloc_mmap_mb` は大きい確保 (mmap で直接取った分)、`malloc_arena_mb` は通常の領域の合計 (使用中 + 空き。mmap の分は含まない)。
//!   RSS ≈ malloc_inuse_mb + malloc_free_mb + malloc_mmap_mb + (割り当て以外: スレッドのスタック・実行ファイル・ファイルの読み込みなど)。
//!   `malloc_free_mb` が大きく増えていくなら「解放したが OS に返していない分」、`malloc_inuse_mb` が増えていくなら「実際に持ち続けているデータ」。
//!   小さい解放済みの塊をスレッドごとに手元 (tcache) へ置いている分は、使用中に数えられる。
//!   `malloc_stats_us` は数えるのにかかった時間 (マイクロ秒)。空きの塊が多いほど長く、その間は数えている arena を使う処理が待つ。
//! - 重い裏の処理 (RSS が 16MB 以上増えて終わった) が終わったら、次の 15 秒おきの周回で
//!   `malloc_trim(0)` を別スレッドで呼び、glibc が手元に持っている空きを OS に返す (2026-11-11)。本番の 3 日分のログで、
//!   使用中は 200〜300MB で横ばいなのに、空き (返していない分) が起動時の重い処理の一時確保で 550〜650MB まで残っていたため。
//!   毎分の `mem` の行の `trims=` が前の行からの回数、`trimmed_mb=` が返した RSS の合計、`trim_ms=` がかかった時間の合計。
//!   環境変数 `MEM_TRIM_AFTER_TASKS=0` で止まる (比べるとき・不具合のとき)。Linux の glibc のときだけ。
use axum::{extract::Request, middleware::Next, response::Response};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const SAMPLE_EVERY: Duration = Duration::from_secs(15);
const LOG_EVERY_SAMPLES: u32 = 4;
const JUMP_BYTES: u64 = 100 * 1024 * 1024;
const RECENT_MAX: usize = 300;

/// 終わった処理を覚えておく数 (要求と同じ。古いものから捨てる)
const TASK_RECENT_MAX: usize = 300;
/// `mem task` の行を出す条件: RSS がこれ以上動いた、またはこれ以上かかった
const TASK_LOG_BYTES: u64 = 50 * 1024 * 1024;
const TASK_LOG_SLOW: Duration = Duration::from_secs(60);
/// `mem task` の行は 1 時間にこの行数まで。超えた分は出さず、次に出す行に `skipped=` で数を添える
const TASK_LOG_PER_HOUR: u32 = 20;
/// うち、RSS はあまり動かず時間がかかっただけの行は、この行数まで (RSS が動いた行の分を残す)
const TASK_LOG_SLOW_PER_HOUR: u32 = 10;
const TASK_LOG_WINDOW: Duration = Duration::from_secs(3600);
/// 毎分の `mem` と `mem jump` の行に載せる件数 (超えた分は ` (+N more)` と数だけ)
const TASKS_SHOWN: usize = 5;
const TASKS_DONE_SHOWN: usize = 8;
/// `tasks_done=` に載せるのは、これ以上かかったか RSS がこれ以上動いた処理だけ
/// (何も変えない一瞬の周回で行を埋めない)
const TASK_DONE_MIN_ELAPSED: Duration = Duration::from_secs(1);
const TASK_DONE_MIN_BYTES: u64 = 1024 * 1024;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
static INFLIGHT: LazyLock<Mutex<HashMap<u64, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// 終わった要求 (経路, 終わった時刻, かかった時間)。古いものから捨てる。
static RECENT: LazyLock<Mutex<VecDeque<(String, Instant, Duration)>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
/// 裏の処理 (走っているものと、終わったもの)
static TASKS: LazyLock<Mutex<TaskBook>> = LazyLock::new(|| Mutex::new(TaskBook::default()));
/// `mem task` の行の出し過ぎを防ぐ枠
static TASK_BUDGET: LazyLock<Mutex<LogBudget>> = LazyLock::new(|| Mutex::new(LogBudget::default()));

/// RSS がこれ以上増えて終わった裏の処理のあとに、空きを OS に返す (`malloc_trim`)。`MEM_TRIM_AFTER_TASKS=0` で止まる。
/// 時間では判定しない (HubSpot などを待つだけの周回も 1 秒を超え、ほぼ 15 秒ごとに返すことになるため)
const TRIM_AFTER_BYTES: u64 = 16 * 1024 * 1024;
static TRIM_ENABLED: LazyLock<bool> =
    LazyLock::new(|| trim_enabled_from(std::env::var("MEM_TRIM_AFTER_TASKS").ok().as_deref()));
/// 重い処理が終わって、まだ返していない
static TRIM_PENDING: AtomicBool = AtomicBool::new(false);
/// 前の毎分の行からの (回数, 返した RSS のバイト, かかったマイクロ秒)
static TRIM_DONE: LazyLock<Mutex<(u32, u64, u64)>> = LazyLock::new(|| Mutex::new((0, 0, 0)));

/// `MEM_TRIM_AFTER_TASKS` の値の読み方。`0` (前後の空白は無視) だけが止める
fn trim_enabled_from(v: Option<&str>) -> bool {
    v.is_none_or(|v| v.trim() != "0")
}

/// 返す対象の重い処理か (RSS が `TRIM_AFTER_BYTES` 以上増えて終わった = 一時確保の分が手元に残っている)
fn heavy(f: &FinishedTask) -> bool {
    f.delta().is_some_and(|d| d >= TRIM_AFTER_BYTES as i64)
}

/// 重い処理なら、次の見張りの周回で返す印を付ける
fn mark_if_heavy(f: &FinishedTask) {
    if heavy(f) && trim_supported() && *TRIM_ENABLED {
        TRIM_PENDING.store(true, Ordering::Relaxed);
    }
}

struct Guard {
    id: u64,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Some((path, started)) = INFLIGHT.lock().unwrap().remove(&self.id) else {
            return;
        };
        let mut recent = RECENT.lock().unwrap();
        recent.push_back((path, Instant::now(), started.elapsed()));
        while recent.len() > RECENT_MAX {
            recent.pop_front();
        }
    }
}

/// 処理中の要求を数える。クエリは個人情報を含みうるので経路だけを持つ。
pub async fn track(req: Request, next: Next) -> Response {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let path = format!("{} {}", req.method(), req.uri().path());
    INFLIGHT.lock().unwrap().insert(id, (path, Instant::now()));
    let _guard = Guard { id };
    next.run(req).await
}

// ---------------------------------------------------------------------------
// 裏の処理
// ---------------------------------------------------------------------------

struct RunningTask {
    name: &'static str,
    started: Instant,
    /// 始めの RSS (読めなければ None)
    rss: Option<u64>,
}

#[derive(Clone, Debug)]
struct FinishedTask {
    name: &'static str,
    ended: Instant,
    elapsed: Duration,
    rss_before: Option<u64>,
    rss_after: Option<u64>,
}

impl FinishedTask {
    /// 終わりと始めの RSS の差 (バイト。どちらかが読めなければ None)
    fn delta(&self) -> Option<i64> {
        Some(self.rss_after? as i64 - self.rss_before? as i64)
    }
}

/// 行に出す 1 件 (名前、時間、RSS の増減)
#[derive(Debug, PartialEq)]
struct TaskLine {
    name: &'static str,
    elapsed: Duration,
    delta: Option<i64>,
}

#[derive(Default)]
struct TaskBook {
    running: HashMap<u64, RunningTask>,
    /// 終わった順。古いものから捨てる
    done: VecDeque<FinishedTask>,
}

impl TaskBook {
    fn begin(&mut self, id: u64, name: &'static str, rss: Option<u64>, now: Instant) {
        self.running.insert(
            id,
            RunningTask {
                name,
                started: now,
                rss,
            },
        );
    }

    /// 走っている処理を終わった処理に移す。知らない id (二重に終わらせた) なら None
    fn end(&mut self, id: u64, rss: Option<u64>, now: Instant) -> Option<FinishedTask> {
        let task = self.running.remove(&id)?;
        let finished = FinishedTask {
            name: task.name,
            ended: now,
            elapsed: now.saturating_duration_since(task.started),
            rss_before: task.rss,
            rss_after: rss,
        };
        self.done.push_back(finished.clone());
        while self.done.len() > TASK_RECENT_MAX {
            self.done.pop_front();
        }
        Some(finished)
    }

    /// 走っている処理 (長く走っているものから)。増減は、始めの RSS と今の RSS (`rss`) の差
    fn running_lines(&self, now: Instant, rss: Option<u64>) -> Vec<TaskLine> {
        let mut lines: Vec<_> = self
            .running
            .values()
            .map(|t| TaskLine {
                name: t.name,
                elapsed: now.saturating_duration_since(t.started),
                delta: rss.zip(t.rss).map(|(cur, then)| cur as i64 - then as i64),
            })
            .collect();
        lines.sort_by(|a, b| b.elapsed.cmp(&a.elapsed).then(a.name.cmp(b.name)));
        lines
    }

    /// `since` 以降に終わった処理 (RSS が大きく動いたものから)。`since` が None なら覚えている全部。
    /// 一瞬で何も変えなかった周回は除く
    fn done_lines(&self, since: Option<Instant>) -> Vec<TaskLine> {
        let moved = |l: &TaskLine| l.delta.map_or(0, i64::unsigned_abs);
        let mut lines: Vec<_> = self
            .done
            .iter()
            .filter(|t| since.is_none_or(|s| t.ended >= s))
            .map(|t| TaskLine {
                name: t.name,
                elapsed: t.elapsed,
                delta: t.delta(),
            })
            .filter(|l| l.elapsed >= TASK_DONE_MIN_ELAPSED || moved(l) >= TASK_DONE_MIN_BYTES)
            .collect();
        lines.sort_by(|a, b| {
            moved(b)
                .cmp(&moved(a))
                .then(b.elapsed.cmp(&a.elapsed))
                .then(a.name.cmp(b.name))
        });
        lines
    }
}

fn book() -> MutexGuard<'static, TaskBook> {
    // 手放す側 (Drop) で panic しないよう、鍵が壊れていても中身を使う
    TASKS.lock().unwrap_or_else(|e| e.into_inner())
}

/// `mem task` の行を出す理由
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogReason {
    /// RSS が `TASK_LOG_BYTES` 以上動いた
    Moved,
    /// RSS は大きく動かなかったが、`TASK_LOG_SLOW` 以上かかった
    Slow,
}

/// `mem task` の行を 1 時間に `TASK_LOG_PER_HOUR` 行までにする枠
#[derive(Default)]
struct LogBudget {
    window_start: Option<Instant>,
    used: u32,
    used_slow: u32,
    /// 前に出した行から、枠が尽きて出せなかった行数
    skipped: u32,
}

impl LogBudget {
    /// 1 行出してよいか。出せるなら、前に出した行から出せなかった行数を返す
    fn take(&mut self, now: Instant, reason: LogReason) -> Option<u32> {
        let in_window = self
            .window_start
            .is_some_and(|s| now.saturating_duration_since(s) < TASK_LOG_WINDOW);
        if !in_window {
            self.window_start = Some(now);
            self.used = 0;
            self.used_slow = 0;
        }
        let slow = reason == LogReason::Slow;
        if self.used >= TASK_LOG_PER_HOUR || (slow && self.used_slow >= TASK_LOG_SLOW_PER_HOUR) {
            self.skipped += 1;
            return None;
        }
        self.used += 1;
        if slow {
            self.used_slow += 1;
        }
        Some(std::mem::take(&mut self.skipped))
    }
}

/// 裏の処理を数える入れ物。手放すと、その処理が終わったことになる
#[must_use = "変数に持たないと、その場で手放されて何も測れない (`let _task = ...` と書く)"]
pub struct TaskGuard {
    id: u64,
}

/// 裏の処理を数える。持っている間は「走っている処理」、手放すと「終わった処理」(名前・時間・RSS の差) になる。
/// 1 回の作り直し・1 回の周回・起動時の 1 回の読み込みで包む。ループの途中で手放したいときは `drop` する
/// (周回の最後の `sleep` まで持たない)。Linux 以外では RSS が読めず、ログには出ない。
pub fn task(name: &'static str) -> TaskGuard {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    book().begin(id, name, rss_now(), Instant::now());
    TaskGuard { id }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let now = Instant::now();
        let Some(finished) = book().end(self.id, rss_now(), now) else {
            return;
        };
        mark_if_heavy(&finished);
        if let Some(reason) = log_reason(&finished) {
            let budget = TASK_BUDGET
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take(now, reason);
            if let Some(skipped) = budget {
                log_task(&finished, skipped);
            }
        }
    }
}

/// 終わった処理の 1 行を出す条件。始めと終わりの RSS が読めて (Linux)、RSS が大きく動いたか、時間がかかった
fn log_reason(f: &FinishedTask) -> Option<LogReason> {
    let delta = f.delta()?;
    if delta.unsigned_abs() >= TASK_LOG_BYTES {
        Some(LogReason::Moved)
    } else if f.elapsed >= TASK_LOG_SLOW {
        Some(LogReason::Slow)
    } else {
        None
    }
}

fn log_task(f: &FinishedTask, skipped: u32) {
    tracing::info!(
        name = %f.name,
        elapsed_ms = f.elapsed.as_millis() as u64,
        rss_before_mb = opt_mb(f.rss_before),
        rss_after_mb = opt_mb(f.rss_after),
        delta_mb = f.delta().map_or(0, |d| d / MB as i64),
        skipped = (skipped > 0).then_some(skipped),
        "mem task"
    );
}

fn fmt_delta(delta: Option<i64>) -> String {
    match delta {
        Some(d) => format!("{:+}MB", d / MB as i64),
        None => "?MB".to_string(),
    }
}

/// 行に出す文字列: `名前 ms 増減MB, ...` (最大 `shown` 件。超えた分は ` (+N more)`)
fn fmt_tasks(lines: &[TaskLine], shown: usize) -> String {
    let mut text = lines
        .iter()
        .take(shown)
        .map(|l| {
            format!(
                "{} {}ms {}",
                l.name,
                l.elapsed.as_millis(),
                fmt_delta(l.delta)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    if lines.len() > shown {
        text.push_str(&format!(" (+{} more)", lines.len() - shown));
    }
    text
}

/// 走っている処理 (`tasks=`) と、`since` 以降に終わった処理 (`tasks_done=`)。`rss` は今の RSS
fn task_activity(since: Option<Instant>, rss: u64) -> (String, String) {
    let (running, done) = {
        let book = book();
        (
            book.running_lines(Instant::now(), Some(rss)),
            book.done_lines(since),
        )
    };
    (
        fmt_tasks(&running, TASKS_SHOWN),
        fmt_tasks(&done, TASKS_DONE_SHOWN),
    )
}

// ---------------------------------------------------------------------------
// 割り当ての仕組みの数字 (glibc の mallinfo2)
// ---------------------------------------------------------------------------

/// 割り当ての仕組み (glibc の malloc) が数えている、メモリの内訳 (バイト)。
/// RSS の増えた分が、使っているデータなのか、解放したのに OS に返していない空きなのかを見分けるために、
/// 毎分の `mem` と `mem jump` の行に添える。glibc 以外では取れない (`malloc_stats()` が None)。
/// glibc 以外の構成では作る場所が無い (テストだけが作る) ので、そこでだけ未使用の警告を止める。
#[cfg_attr(not(all(target_os = "linux", target_env = "gnu")), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MallocStats {
    /// 使用中: 確保したまま解放されていない分 (mallinfo2 の `uordblks`)
    in_use: u64,
    /// 空き: 解放済みで、割り当ての仕組みが手元に持っている分。OS には返していない (`fordblks`)
    free: u64,
    /// mmap で直接取った大きい確保の合計 (`hblkhd`)
    mmapped: u64,
    /// 通常の領域 (arena) の合計。使用中 + 空き。mmap の分は含まない (`arena`)
    arena: u64,
    /// `mallinfo2()` を呼んで数え終わるまでの時間 (マイクロ秒)。空きの塊が多いほど長く、その間は数えている arena を使う処理が待つ
    took_us: u64,
}

/// glibc の `mallinfo2()` を呼ぶ部分。glibc 2.33 から (本番のイメージは debian bookworm の 2.36。
/// それより古い glibc ではリンクできない)。依存は足さず、C の宣言をここに書く。
#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod glibc {
    use super::MallocStats;

    /// glibc の `struct mallinfo2` (malloc.h) と同じ並びの 10 個の `size_t`。
    /// 使わない欄も、並びと大きさを合わせるために宣言する
    #[repr(C)]
    #[allow(dead_code)]
    struct Mallinfo2 {
        /// 通常の領域の合計 (mmap でない分)
        arena: usize,
        /// 空きの塊の数
        ordblks: usize,
        /// 空きの fastbin の塊の数
        smblks: usize,
        /// mmap の領域の数
        hblks: usize,
        /// mmap の領域の合計
        hblkhd: usize,
        /// (glibc では使われない)
        usmblks: usize,
        /// 空きの fastbin の塊の合計
        fsmblks: usize,
        /// 使用中の合計
        uordblks: usize,
        /// 空きの合計
        fordblks: usize,
        /// 末尾にあって返せる空き
        keepcost: usize,
    }

    // 欄を足したり消したりして大きさがずれたら、ここで止める (並びの取り違えはテストで見る)
    const _: () = assert!(std::mem::size_of::<Mallinfo2>() == 10 * std::mem::size_of::<usize>());

    extern "C" {
        /// 全 arena を順にロックして、空きの塊を数える
        fn mallinfo2() -> Mallinfo2;
        /// 全 arena の空きのページを OS に返す (返したら 1)
        fn malloc_trim(pad: usize) -> std::os::raw::c_int;
    }

    pub(super) fn trim() {
        // SAFETY: 引数 0 は「末尾に余白を残さない」。arena ごとにロックを取って空きのページを madvise で返すだけで、
        // 使用中の塊には触らない。スレッドから呼んでよい (シグナルハンドラの中では呼ばない)。
        unsafe {
            malloc_trim(0);
        }
    }

    pub(super) fn stats() -> MallocStats {
        // SAFETY: 引数も前提もない C の関数で、スレッドから呼んでよい (arena ごとにロックを取って数える)。
        // glibc の安全性の表で値が不確かになるのは `mallopt` と同時に呼んだときだけで、アプリも依存のクレートも `mallopt` を呼ばない。
        // シグナルハンドラの中では呼ばない (ここは普通のタスクの中)。
        let started = std::time::Instant::now();
        let m = unsafe { mallinfo2() };
        let took_us = started.elapsed().as_micros() as u64;
        MallocStats {
            in_use: m.uordblks as u64,
            free: m.fordblks as u64,
            mmapped: m.hblkhd as u64,
            arena: m.arena as u64,
            took_us,
        }
    }
}

/// 今の割り当ての内訳。Linux の glibc のときだけ取れる (それ以外は None で、行には項目ごと出ない)。
/// 全 arena のロックを順に取って空きの塊を数えるので、毎分の行と急増の行を出す回にだけ呼ぶ
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn malloc_stats() -> Option<MallocStats> {
    Some(glibc::stats())
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn malloc_stats() -> Option<MallocStats> {
    None
}

/// 空きを OS に返せるか (Linux の glibc のときだけ)
const fn trim_supported() -> bool {
    cfg!(all(target_os = "linux", target_env = "gnu"))
}

/// 重い処理が終わっていたら、空きを OS に返す。arena のロックを順に取るので、非同期の実行スレッドでは呼ばない
async fn trim_if_pending() {
    if !TRIM_PENDING.swap(false, Ordering::Relaxed) {
        return;
    }
    let done = tokio::task::spawn_blocking(|| {
        let before = rss_now();
        let started = Instant::now();
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        glibc::trim();
        let took_us = started.elapsed().as_micros() as u64;
        let freed = before
            .zip(rss_now())
            .map_or(0, |(b, a)| b.saturating_sub(a));
        (freed, took_us)
    })
    .await;
    if let Ok((freed, took_us)) = done {
        let mut t = TRIM_DONE.lock().unwrap_or_else(|e| e.into_inner());
        t.0 += 1;
        t.1 += freed;
        t.2 += took_us;
    }
}

// ---------------------------------------------------------------------------
// 測る
// ---------------------------------------------------------------------------

#[derive(Default, Clone, Copy)]
struct Sample {
    rss: u64,
    rss_anon: u64,
    rss_file: u64,
    cgroup: Option<u64>,
    cgroup_anon: Option<u64>,
    cgroup_file: Option<u64>,
}

fn status_kb(text: &str, key: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with(key))?;
    line[key.len()..]
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}

fn stat_value(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(' ')?.trim().parse().ok())
}

fn sample() -> Option<Sample> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let cgroup = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
        .ok()
        .and_then(|s| s.trim().parse().ok());
    let stat = std::fs::read_to_string("/sys/fs/cgroup/memory.stat").unwrap_or_default();
    Some(Sample {
        rss: status_kb(&status, "VmRSS:")?,
        rss_anon: status_kb(&status, "RssAnon:").unwrap_or(0),
        rss_file: status_kb(&status, "RssFile:").unwrap_or(0),
        cgroup,
        cgroup_anon: stat_value(&stat, "anon"),
        cgroup_file: stat_value(&stat, "file"),
    })
}

/// 今の RSS だけ (裏の処理の始めと終わりに読む。`/proc/self/status` の 1 ファイル。Linux 以外では None)
fn rss_now() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status_kb(&status, "VmRSS:")
}

const MB: u64 = 1024 * 1024;

fn mb(v: u64) -> u64 {
    v / MB
}

fn opt_mb(v: Option<u64>) -> i64 {
    v.map(|v| mb(v) as i64).unwrap_or(-1)
}

/// 処理中の要求 (長い順、最大 10 件) と、`since` 以降に終わった要求 (時間の長い順、最大 10 件)。
fn activity(since: Instant) -> (String, String) {
    let mut inflight: Vec<_> = INFLIGHT
        .lock()
        .unwrap()
        .values()
        .map(|(p, s)| (p.clone(), s.elapsed()))
        .collect();
    inflight.sort_by_key(|e| std::cmp::Reverse(e.1));
    let mut done: Vec<_> = RECENT
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, ended, _)| *ended >= since)
        .map(|(p, _, d)| (p.clone(), *d))
        .collect();
    done.sort_by_key(|e| std::cmp::Reverse(e.1));
    let fmt = |v: &[(String, Duration)]| {
        v.iter()
            .take(10)
            .map(|(p, d)| format!("{p} {}ms", d.as_millis()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    (fmt(&inflight), fmt(&done))
}

/// 空の文字列は項目ごと出さない (毎分の行を、裏の処理が無いときに長くしない)
fn shown(text: &str) -> Option<tracing::field::DisplayValue<&str>> {
    (!text.is_empty()).then(|| tracing::field::display(text))
}

/// `malloc` が None (glibc 以外) のときは、割り当ての項目を 4 つとも出さない
fn log_jump(
    now: &Sample,
    before: &Sample,
    (inflight, finished): &(String, String),
    (tasks, tasks_done): &(String, String),
    malloc: Option<&MallocStats>,
) {
    tracing::warn!(
        rss_mb = mb(now.rss),
        rss_before_mb = mb(before.rss),
        cgroup_mb = opt_mb(now.cgroup),
        cgroup_before_mb = opt_mb(before.cgroup),
        inflight = %inflight,
        finished = %finished,
        tasks = %tasks,
        tasks_done = %tasks_done,
        malloc_inuse_mb = malloc.map(|m| mb(m.in_use)),
        malloc_free_mb = malloc.map(|m| mb(m.free)),
        malloc_mmap_mb = malloc.map(|m| mb(m.mmapped)),
        malloc_arena_mb = malloc.map(|m| mb(m.arena)),
        malloc_stats_us = malloc.map(|m| m.took_us),
        "mem jump"
    );
}

/// `malloc` が None (glibc 以外) のときは、割り当ての項目を 4 つとも出さない
fn log_minute(
    now: &Sample,
    inflight: usize,
    (tasks, tasks_done): &(String, String),
    malloc: Option<&MallocStats>,
    trims: Option<(u32, u64, u64)>,
) {
    tracing::info!(
        rss_mb = mb(now.rss),
        anon_mb = mb(now.rss_anon),
        file_mb = mb(now.rss_file),
        cgroup_mb = opt_mb(now.cgroup),
        cgroup_anon_mb = opt_mb(now.cgroup_anon),
        cgroup_file_mb = opt_mb(now.cgroup_file),
        inflight,
        tasks = shown(tasks),
        tasks_done = shown(tasks_done),
        malloc_inuse_mb = malloc.map(|m| mb(m.in_use)),
        malloc_free_mb = malloc.map(|m| mb(m.free)),
        malloc_mmap_mb = malloc.map(|m| mb(m.mmapped)),
        malloc_arena_mb = malloc.map(|m| mb(m.arena)),
        malloc_stats_us = malloc.map(|m| m.took_us),
        trims = trims.map(|t| t.0),
        trimmed_mb = trims.map(|t| mb(t.1)),
        trim_ms = trims.map(|t| t.2 / 1000),
        "mem"
    );
}

pub fn spawn() {
    if sample().is_none() {
        tracing::info!("mem watch disabled (no /proc/self/status)");
        return;
    }
    tokio::spawn(async {
        let mut prev: Option<(Sample, Instant)> = None;
        let mut count: u32 = 0;
        // 毎分の行に載せる「終わった処理」の起点。最初の行は、ここまでに終わった全部
        let mut last_line: Option<Instant> = None;
        let mut interval = tokio::time::interval(SAMPLE_EVERY);
        loop {
            interval.tick().await;
            trim_if_pending().await;
            let Some(now) = sample() else { continue };
            let at = Instant::now();
            // 割り当ての内訳は、行を出す回にだけ、最初に要った行のところで 1 回取る (急増の行と毎分の行が
            // 重なっても 1 回)。15 秒おきの計測では取らない (全 arena のロックを順に取って数えるため)
            let mut malloc: Option<Option<MallocStats>> = None;
            if let Some((before, before_at)) = prev {
                let rose = |a: Option<u64>, b: Option<u64>| match (a, b) {
                    (Some(a), Some(b)) => a >= b + JUMP_BYTES,
                    _ => false,
                };
                if now.rss >= before.rss + JUMP_BYTES || rose(now.cgroup, before.cgroup) {
                    log_jump(
                        &now,
                        &before,
                        &activity(before_at),
                        &task_activity(Some(before_at), now.rss),
                        malloc.get_or_insert_with(malloc_stats).as_ref(),
                    );
                }
            }
            if count.is_multiple_of(LOG_EVERY_SAMPLES) {
                let inflight = INFLIGHT.lock().unwrap().len();
                // 返した回数などは、返せる環境 (glibc) で有効なときだけ行に載せる
                let trims = (trim_supported() && *TRIM_ENABLED).then(|| {
                    std::mem::take(&mut *TRIM_DONE.lock().unwrap_or_else(|e| e.into_inner()))
                });
                log_minute(
                    &now,
                    inflight,
                    &task_activity(last_line, now.rss),
                    malloc.get_or_insert_with(malloc_stats).as_ref(),
                    trims,
                );
                last_line = Some(at);
            }
            count = count.wrapping_add(1);
            prev = Some((now, at));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const SEC: Duration = Duration::from_secs(1);

    fn bytes(n: u64) -> Option<u64> {
        Some(n * MB)
    }

    #[test]
    fn 重い処理だけを_空きを返す対象にする() {
        let f = |ms: u64, before: u64, after: u64| FinishedTask {
            name: "t",
            ended: Instant::now(),
            elapsed: Duration::from_millis(ms),
            rss_before: bytes(before),
            rss_after: bytes(after),
        };
        assert!(heavy(&f(10, 100, 116)), "16MB 増えた");
        assert!(!heavy(&f(10, 100, 115)), "15MB は対象外");
        assert!(
            !heavy(&f(10, 116, 100)),
            "減って終わった処理は返すものが無い"
        );
        assert!(
            !heavy(&f(60_000, 100, 100)),
            "待っていただけの長い周回は対象外"
        );
        let unknown = FinishedTask {
            rss_after: None,
            ..f(10, 100, 200)
        };
        assert!(!heavy(&unknown), "RSS が読めない (Linux 以外) ときは対象外");
    }

    #[test]
    fn 止める設定は_0_だけ() {
        assert!(trim_enabled_from(None));
        assert!(trim_enabled_from(Some("1")));
        assert!(trim_enabled_from(Some("")));
        assert!(!trim_enabled_from(Some("0")));
        assert!(!trim_enabled_from(Some(" 0\n")));
    }

    /// 印を付ける → 見張りの周回で返す → 回数を足して印を下ろす (返せる環境だけ。印を下ろすのはこのテストだけ)
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[tokio::test]
    async fn 重い処理の後に印が付き_次の周回で返して数える() {
        let heavy_task = FinishedTask {
            name: "t",
            ended: Instant::now(),
            elapsed: SEC,
            rss_before: bytes(100),
            rss_after: bytes(200),
        };
        let before = TRIM_DONE.lock().unwrap().0;
        mark_if_heavy(&heavy_task);
        assert!(TRIM_PENDING.load(Ordering::Relaxed));
        trim_if_pending().await;
        assert!(!TRIM_PENDING.load(Ordering::Relaxed), "返したら印を下ろす");
        assert!(TRIM_DONE.lock().unwrap().0 > before, "回数を数える");
        // 印が無ければ何もしない
        let count = TRIM_DONE.lock().unwrap().0;
        trim_if_pending().await;
        assert_eq!(TRIM_DONE.lock().unwrap().0, count);
    }

    #[test]
    fn 毎分の行に_空きを返した回数と量と時間が載る() {
        let now = Sample {
            rss: 300 * MB,
            ..Sample::default()
        };
        let none = (String::new(), String::new());
        let text = captured(|| log_minute(&now, 0, &none, None, Some((3, 120 * MB, 45_000))));
        assert!(text.contains("trims=3"), "{text}");
        assert!(text.contains("trimmed_mb=120"), "{text}");
        assert!(text.contains("trim_ms=45"), "{text}");
        let text = captured(|| log_minute(&now, 0, &none, None, None));
        assert!(
            !text.contains("trims="),
            "返せない環境・止めたときは項目ごと出さない: {text}"
        );
    }

    #[test]
    fn parses_proc_status_and_cgroup_stat() {
        let status =
            "Name:\tx\nVmRSS:\t  204800 kB\nRssAnon:\t  153600 kB\nRssFile:\t   51200 kB\n";
        assert_eq!(status_kb(status, "VmRSS:"), Some(200 * 1024 * 1024));
        assert_eq!(status_kb(status, "RssAnon:"), Some(150 * 1024 * 1024));
        assert_eq!(status_kb(status, "RssFile:"), Some(50 * 1024 * 1024));
        let stat = "anon 1048576\nfile 2097152\nanon_thp 0\nfile_mapped 5\n";
        assert_eq!(stat_value(stat, "anon"), Some(1_048_576));
        assert_eq!(stat_value(stat, "file"), Some(2_097_152));
        assert_eq!(stat_value(stat, "shmem"), None);
    }

    #[test]
    fn activity_lists_recent_finished_requests_after_since() {
        let since = Instant::now();
        {
            let _g = {
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                INFLIGHT
                    .lock()
                    .unwrap()
                    .insert(id, ("GET /mem-watch-test".into(), Instant::now()));
                Guard { id }
            };
        }
        let (_, done) = activity(since);
        assert!(done.contains("GET /mem-watch-test"), "{done}");
        assert!(INFLIGHT
            .lock()
            .unwrap()
            .values()
            .all(|(p, _)| p != "GET /mem-watch-test"));
    }

    #[test]
    fn a_task_moves_from_running_to_done_with_its_time_and_rss() {
        let mut book = TaskBook::default();
        let t0 = Instant::now();
        book.begin(1, "a.build", bytes(300), t0);
        // 走っている間: 経過と、始めからの増減 (今の RSS 340MB - 始め 300MB)
        assert_eq!(
            book.running_lines(t0 + 2 * SEC, bytes(340)),
            vec![TaskLine {
                name: "a.build",
                elapsed: 2 * SEC,
                delta: Some(40 * MB as i64),
            }]
        );
        assert!(book.done_lines(None).is_empty());
        // 終わる: 時間と始め・終わりの RSS が入り、走っている一覧から消える
        let f = book.end(1, bytes(860), t0 + 8 * SEC).unwrap();
        assert_eq!(f.name, "a.build");
        assert_eq!(f.elapsed, 8 * SEC);
        assert_eq!((f.rss_before, f.rss_after), (bytes(300), bytes(860)));
        assert_eq!(f.delta(), Some(560 * MB as i64));
        assert!(book.running_lines(t0 + 9 * SEC, bytes(860)).is_empty());
        assert_eq!(
            book.done_lines(None),
            vec![TaskLine {
                name: "a.build",
                elapsed: 8 * SEC,
                delta: Some(560 * MB as i64),
            }]
        );
        // 二重に終わらせても増えない
        assert!(book.end(1, bytes(860), t0 + 9 * SEC).is_none());
        assert_eq!(book.done.len(), 1);
    }

    #[test]
    fn running_tasks_are_listed_longest_first_and_unreadable_rss_is_unknown() {
        let mut book = TaskBook::default();
        let t0 = Instant::now();
        book.begin(1, "b.short", bytes(100), t0 + 9 * SEC);
        book.begin(2, "a.long", None, t0);
        let lines = book.running_lines(t0 + 10 * SEC, bytes(150));
        assert_eq!(
            lines,
            vec![
                TaskLine {
                    name: "a.long",
                    elapsed: 10 * SEC,
                    delta: None,
                },
                TaskLine {
                    name: "b.short",
                    elapsed: SEC,
                    delta: Some(50 * MB as i64),
                },
            ]
        );
        assert_eq!(
            fmt_tasks(&lines, TASKS_SHOWN),
            "a.long 10000ms ?MB, b.short 1000ms +50MB"
        );
    }

    #[test]
    fn done_tasks_keep_only_the_latest_300() {
        let mut book = TaskBook::default();
        let t0 = Instant::now();
        // i 番目は (i+1) 秒かかった。先頭の 5 件が押し出される
        for i in 0..305u64 {
            book.begin(i, "ring.task", None, t0);
            book.end(i, None, t0 + Duration::from_secs(i + 1));
        }
        assert_eq!(book.done.len(), TASK_RECENT_MAX);
        assert_eq!(book.done.front().unwrap().elapsed, 6 * SEC);
        assert_eq!(book.done.back().unwrap().elapsed, 305 * SEC);
        assert!(book.running.is_empty());
    }

    #[test]
    fn done_lines_after_since_are_sorted_by_how_far_rss_moved() {
        let mut book = TaskBook::default();
        let t0 = Instant::now();
        // 古い: since より前に終わる
        book.begin(1, "old.task", bytes(100), t0);
        book.end(1, bytes(900), t0 + SEC);
        let since = t0 + 10 * SEC;
        book.begin(2, "small.up", bytes(100), t0 + 11 * SEC);
        book.end(2, bytes(102), t0 + 16 * SEC); // 5 秒 +2MB
        book.begin(3, "big.down", bytes(900), t0 + 11 * SEC);
        book.end(3, bytes(600), t0 + 14 * SEC); // 3 秒 -300MB
        book.begin(4, "big.up", bytes(100), t0 + 11 * SEC);
        book.end(4, bytes(400), t0 + 13 * SEC); // 2 秒 +300MB
        let lines = book.done_lines(Some(since));
        let names: Vec<_> = lines.iter().map(|l| l.name).collect();
        // 動きが同じ 300MB なら長くかかった方が先。古い処理は since で外れる
        assert_eq!(names, ["big.down", "big.up", "small.up"]);
        assert_eq!(lines[0].delta, Some(-300 * MB as i64));
        assert_eq!(
            fmt_tasks(&lines, TASKS_DONE_SHOWN),
            "big.down 3000ms -300MB, big.up 2000ms +300MB, small.up 5000ms +2MB"
        );
        // since が無ければ覚えている全部
        assert_eq!(book.done_lines(None).len(), 4);
    }

    #[test]
    fn instant_quiet_tasks_are_left_out_of_the_done_list() {
        let mut book = TaskBook::default();
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        book.begin(1, "quiet.fast", bytes(100), t0);
        book.end(1, bytes(100), t0 + ms(100)); // 0.1 秒・動きなし: 出さない
        book.begin(2, "quiet.edge", bytes(100), t0);
        book.end(2, bytes(100), t0 + ms(999)); // 0.999 秒・動きなし: 出さない
        book.begin(3, "alloc.fast", bytes(100), t0);
        book.end(3, bytes(105), t0 + ms(100)); // 0.1 秒でも +5MB: 出す
        book.begin(4, "slow.quiet", bytes(100), t0);
        book.end(4, bytes(100), t0 + ms(1000)); // ちょうど 1 秒: 出す
        book.begin(5, "unknown.fast", None, t0);
        book.end(5, None, t0 + ms(100)); // RSS が読めず短い: 出さない
        let names: Vec<_> = book.done_lines(None).iter().map(|l| l.name).collect();
        assert_eq!(names, ["alloc.fast", "slow.quiet"]);
    }

    #[test]
    fn the_line_text_is_limited_to_the_shown_count_and_says_how_many_were_left_out() {
        let line = |name, ms, n: i64| TaskLine {
            name,
            elapsed: Duration::from_millis(ms),
            delta: Some(n * MB as i64),
        };
        let lines = vec![
            line("a.one", 8012, 560),
            line("b.two", 1500, -12),
            line("c.three", 90, 0),
            line("d.four", 70, 1),
        ];
        assert_eq!(
            fmt_tasks(&lines, 4),
            "a.one 8012ms +560MB, b.two 1500ms -12MB, c.three 90ms +0MB, d.four 70ms +1MB"
        );
        assert_eq!(
            fmt_tasks(&lines, 2),
            "a.one 8012ms +560MB, b.two 1500ms -12MB (+2 more)"
        );
        assert_eq!(fmt_tasks(&[], 5), "");
    }

    #[test]
    fn a_mem_task_line_is_for_big_moves_or_slow_tasks_only() {
        let t0 = Instant::now();
        let finished = |secs: u64, before: Option<u64>, after: Option<u64>| FinishedTask {
            name: "x.task",
            ended: t0,
            elapsed: Duration::from_secs(secs),
            rss_before: before,
            rss_after: after,
        };
        // 59 秒・動き 49MB: 出さない
        assert_eq!(log_reason(&finished(59, bytes(500), bytes(549))), None);
        // ちょうど 60 秒: 動きがなくても出す (時間がかかった行)
        assert_eq!(
            log_reason(&finished(60, bytes(500), bytes(500))),
            Some(LogReason::Slow)
        );
        // 短くても +50MB: 出す。減った 50MB も出す (動いた行)
        assert_eq!(
            log_reason(&finished(1, bytes(500), bytes(550))),
            Some(LogReason::Moved)
        );
        assert_eq!(
            log_reason(&finished(1, bytes(550), bytes(500))),
            Some(LogReason::Moved)
        );
        // 長くて大きく動いたら「動いた行」
        assert_eq!(
            log_reason(&finished(600, bytes(500), bytes(900))),
            Some(LogReason::Moved)
        );
        // RSS が読めなければ (Linux 以外) 長くても出さない
        assert_eq!(log_reason(&finished(600, None, None)), None);
        assert_eq!(log_reason(&finished(600, bytes(500), None)), None);
    }

    #[test]
    fn at_most_20_mem_task_lines_an_hour_and_the_next_line_says_how_many_were_skipped() {
        let mut budget = LogBudget::default();
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        // 最初の 20 行は出せて、出せなかった数は 0
        for i in 0..TASK_LOG_PER_HOUR {
            assert_eq!(budget.take(at(i as u64), LogReason::Moved), Some(0));
        }
        // 同じ 1 時間の 21・22 行目は出さない
        assert_eq!(budget.take(at(60), LogReason::Moved), None);
        assert_eq!(budget.take(at(3599), LogReason::Moved), None);
        // 1 時間たつと枠が戻り、最初の行に出せなかった 2 行を添える。その次は 0
        assert_eq!(budget.take(at(3600), LogReason::Moved), Some(2));
        assert_eq!(budget.take(at(3601), LogReason::Moved), Some(0));
    }

    #[test]
    fn slow_only_lines_stop_at_10_an_hour_and_leave_room_for_lines_where_rss_moved() {
        let mut budget = LogBudget::default();
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        for i in 0..TASK_LOG_SLOW_PER_HOUR {
            assert_eq!(budget.take(at(i as u64), LogReason::Slow), Some(0));
        }
        // 時間がかかっただけの 11 行目は出さない
        assert_eq!(budget.take(at(20), LogReason::Slow), None);
        // RSS が動いた行は、残りの 10 行 (合計 20 行) まで出せる。最初の行に出せなかった 1 行を添える
        assert_eq!(budget.take(at(21), LogReason::Moved), Some(1));
        for i in 1..(TASK_LOG_PER_HOUR - TASK_LOG_SLOW_PER_HOUR) {
            assert_eq!(budget.take(at(30 + i as u64), LogReason::Moved), Some(0));
        }
        assert_eq!(budget.take(at(100), LogReason::Moved), None);
        assert_eq!(budget.take(at(101), LogReason::Slow), None);
        // 次の 1 時間では、時間がかかっただけの行もまた出せる。出せなかった 2 行 (100 秒と 101 秒) を添える
        assert_eq!(budget.take(at(3600), LogReason::Slow), Some(2));
    }

    #[test]
    fn the_public_guard_moves_a_task_from_running_to_done_in_the_shared_book() {
        // 共有の入れ物は他のテストも使うので、この名前の行だけを見る
        let before = Instant::now();
        let guard = task("mem_watch_test.guard");
        let rss = rss_now();
        {
            let book = book();
            let running = book.running_lines(Instant::now(), rss);
            assert_eq!(
                running
                    .iter()
                    .filter(|l| l.name == "mem_watch_test.guard")
                    .count(),
                1
            );
        }
        std::thread::sleep(Duration::from_millis(30));
        drop(guard);
        let book = book();
        assert!(book
            .running_lines(Instant::now(), rss)
            .iter()
            .all(|l| l.name != "mem_watch_test.guard"));
        let done = book
            .done
            .iter()
            .rev()
            .find(|t| t.name == "mem_watch_test.guard" && t.ended >= before)
            .expect("終わった処理に入っている");
        assert!(done.elapsed >= Duration::from_millis(30), "{done:?}");
        // Linux では始めと終わりの RSS が読める (それ以外は None のまま)
        assert_eq!(done.rss_before.is_some(), cfg!(target_os = "linux"));
        assert_eq!(done.rss_after.is_some(), cfg!(target_os = "linux"));
    }

    /// 出力を取り込んで、1 行の文字列の形を確かめる
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn captured(run: impl FnOnce()) -> String {
        let out = Captured::default();
        let writer = out.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .finish();
        tracing::subscriber::with_default(subscriber, run);
        let bytes = out.0.lock().unwrap().clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn the_mem_task_line_has_name_time_and_rss_before_and_after() {
        let f = FinishedTask {
            name: "talk_review.find_rejudge",
            ended: Instant::now(),
            elapsed: Duration::from_millis(8012),
            rss_before: bytes(300),
            rss_after: bytes(860),
        };
        let text = captured(|| log_task(&f, 0));
        assert!(
            text.contains("mem task name=talk_review.find_rejudge elapsed_ms=8012 rss_before_mb=300 rss_after_mb=860 delta_mb=560"),
            "{text}"
        );
        assert!(!text.contains("skipped"), "{text}");
        // 出せなかった行があれば、その数を添える
        let text = captured(|| log_task(&f, 3));
        assert!(text.contains("delta_mb=560 skipped=3"), "{text}");
    }

    #[test]
    fn the_jump_line_always_has_tasks_and_tasks_done_and_the_minute_line_only_when_there_are_any() {
        let now = Sample {
            rss: 1100 * MB,
            ..Sample::default()
        };
        let before = Sample {
            rss: 950 * MB,
            ..Sample::default()
        };
        let requests = ("GET /a 12ms".to_string(), "GET /b 3ms".to_string());
        let tasks = (
            "job_copy.index_build 45123ms +612MB".to_string(),
            "talk_review.find_rejudge 8012ms +560MB, x.y 1000ms +0MB (+2 more)".to_string(),
        );
        let text = captured(|| log_jump(&now, &before, &requests, &tasks, None));
        assert!(
            text.contains("mem jump rss_mb=1100 rss_before_mb=950"),
            "{text}"
        );
        assert!(
            text.contains("inflight=GET /a 12ms finished=GET /b 3ms"),
            "{text}"
        );
        assert!(
            text.contains("tasks=job_copy.index_build 45123ms +612MB tasks_done=talk_review.find_rejudge 8012ms +560MB, x.y 1000ms +0MB (+2 more)"),
            "{text}"
        );
        // 裏の処理が無くても、急増の行には項目を残す
        let text = captured(|| {
            log_jump(
                &now,
                &before,
                &requests,
                &(String::new(), String::new()),
                None,
            )
        });
        assert!(text.contains("tasks= tasks_done="), "{text}");

        // 毎分の行: ある項目だけ出す
        let text = captured(|| log_minute(&now, 2, &(String::new(), String::new()), None, None));
        assert!(text.contains("mem rss_mb=1100"), "{text}");
        assert!(text.contains("inflight=2"), "{text}");
        assert!(!text.contains("tasks"), "{text}");
        let text = captured(|| log_minute(&now, 2, &tasks, None, None));
        assert!(
            text.contains("inflight=2 tasks=job_copy.index_build 45123ms +612MB tasks_done="),
            "{text}"
        );
    }

    /// 計測に頼らず、行の形だけを確かめるための割り当ての内訳。
    /// MB に満たない端数を持たせて、切り捨てて出る (`mb()` と同じ) ことも見る
    fn malloc_sample() -> MallocStats {
        MallocStats {
            in_use: 212 * MB + 5,
            free: 1103 * MB + 700_000,
            mmapped: 48 * MB + 1,
            arena: 1315 * MB + 700_005,
            took_us: 1234,
        }
    }

    const MALLOC_ITEMS: &str =
        "malloc_inuse_mb=212 malloc_free_mb=1103 malloc_mmap_mb=48 malloc_arena_mb=1315 malloc_stats_us=1234";

    #[test]
    fn the_minute_line_gets_the_malloc_numbers_at_its_end_and_nothing_else_changes() {
        let now = Sample {
            rss: 1100 * MB,
            ..Sample::default()
        };
        let none = (String::new(), String::new());
        let tasks = (
            "job_copy.index_build 45123ms +612MB".to_string(),
            "talk_review.find_rejudge 8012ms +560MB".to_string(),
        );
        let head = "mem rss_mb=1100 anon_mb=0 file_mb=0 cgroup_mb=-1 cgroup_anon_mb=-1 cgroup_file_mb=-1 inflight=2";
        let task_items = "tasks=job_copy.index_build 45123ms +612MB tasks_done=talk_review.find_rejudge 8012ms +560MB";
        let malloc = malloc_sample();

        // 裏の処理があるとき: 今までの項目の後ろ (tasks_done= の後ろ) に 4 つ付く
        let text = captured(|| log_minute(&now, 2, &tasks, Some(&malloc), None));
        assert!(
            text.contains(&format!("{head} {task_items} {MALLOC_ITEMS}")),
            "{text}"
        );
        assert!(text.trim_end().ends_with(MALLOC_ITEMS), "{text}");
        // 裏の処理が無いとき: inflight= の後ろに付く
        let text = captured(|| log_minute(&now, 2, &none, Some(&malloc), None));
        assert!(text.contains(&format!("{head} {MALLOC_ITEMS}")), "{text}");
        // 内訳が無いとき (glibc 以外): 今までの行のまま、割り当ての項目は 1 つも出ない
        let text = captured(|| log_minute(&now, 2, &tasks, None, None));
        assert!(text.contains(&format!("{head} {task_items}")), "{text}");
        assert!(text.trim_end().ends_with(task_items), "{text}");
        assert!(!text.contains("malloc"), "{text}");
        let text = captured(|| log_minute(&now, 2, &none, None, None));
        assert!(text.trim_end().ends_with("inflight=2"), "{text}");
        assert!(!text.contains("malloc"), "{text}");
    }

    #[test]
    fn the_jump_line_gets_the_malloc_numbers_at_its_end_and_nothing_else_changes() {
        let now = Sample {
            rss: 1100 * MB,
            ..Sample::default()
        };
        let before = Sample {
            rss: 950 * MB,
            ..Sample::default()
        };
        let requests = ("GET /a 12ms".to_string(), "GET /b 3ms".to_string());
        let tasks = (
            "job_copy.index_build 45123ms +612MB".to_string(),
            "talk_review.find_rejudge 8012ms +560MB".to_string(),
        );
        let head = "mem jump rss_mb=1100 rss_before_mb=950 cgroup_mb=-1 cgroup_before_mb=-1 inflight=GET /a 12ms finished=GET /b 3ms";
        let task_items = "tasks=job_copy.index_build 45123ms +612MB tasks_done=talk_review.find_rejudge 8012ms +560MB";
        let malloc = malloc_sample();

        let text = captured(|| log_jump(&now, &before, &requests, &tasks, Some(&malloc)));
        assert!(
            text.contains(&format!("{head} {task_items} {MALLOC_ITEMS}")),
            "{text}"
        );
        assert!(text.trim_end().ends_with(MALLOC_ITEMS), "{text}");
        // 裏の処理が無くても、急増の行には tasks= tasks_done= を残し、その後ろに付く
        let text = captured(|| {
            log_jump(
                &now,
                &before,
                &requests,
                &(String::new(), String::new()),
                Some(&malloc),
            )
        });
        assert!(
            text.contains(&format!("{head} tasks= tasks_done= {MALLOC_ITEMS}")),
            "{text}"
        );
        // 内訳が無いとき (glibc 以外): 今までの行のまま、割り当ての項目は 1 つも出ない
        let text = captured(|| log_jump(&now, &before, &requests, &tasks, None));
        assert!(text.contains(&format!("{head} {task_items}")), "{text}");
        assert!(text.trim_end().ends_with(task_items), "{text}");
        assert!(!text.contains("malloc"), "{text}");
    }

    #[test]
    fn malloc_numbers_are_cut_down_to_whole_mb_like_the_other_fields() {
        let m = MallocStats {
            in_use: MB - 1,
            free: 2 * MB - 1,
            mmapped: 3 * MB,
            arena: 3 * MB - 2,
            took_us: 7,
        };
        let none = (String::new(), String::new());
        let text = captured(|| log_minute(&Sample::default(), 0, &none, Some(&m), None));
        assert!(
            text.contains("malloc_inuse_mb=0 malloc_free_mb=1 malloc_mmap_mb=3 malloc_arena_mb=2 malloc_stats_us=7"),
            "{text}"
        );
    }

    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    #[test]
    fn off_glibc_there_are_no_malloc_numbers_and_no_malloc_items_in_the_lines() {
        assert_eq!(malloc_stats(), None);
        let now = Sample {
            rss: 1100 * MB,
            ..Sample::default()
        };
        let none = (String::new(), String::new());
        let text = captured(|| log_minute(&now, 0, &none, malloc_stats().as_ref(), None));
        assert!(text.contains("mem rss_mb=1100"), "{text}");
        assert!(!text.contains("malloc"), "{text}");
        let text = captured(|| log_jump(&now, &now, &none, &none, malloc_stats().as_ref()));
        assert!(text.contains("mem jump rss_mb=1100"), "{text}");
        assert!(!text.contains("malloc"), "{text}");
    }

    /// Linux の glibc でだけ動く (macOS・musl では動かない)。実際に `mallinfo2` を呼ぶ
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn on_glibc_mallinfo2_returns_real_numbers() {
        // 使用中: 64KB の塊を 8,192 個 (約 512MB)。mmap で取る下限 (128KB 以上) より小さいので通常の領域に載る。
        // 塊の先頭の管理情報しか書かないので、RSS はほとんど増えない
        let chunks: Vec<Vec<u8>> = (0..8192).map(|_| Vec::with_capacity(64 * 1024)).collect();
        // mmap: 64MB は mmap で取る下限 (動いても最大 32MB) より大きいので、必ず mmap で取られる
        let big: Vec<u8> = Vec::with_capacity(64 * MB as usize);
        // 最適化で確保ごと消されないよう、取る前と後で触れたことにする
        std::hint::black_box((&chunks, &big));
        let m = malloc_stats().expect("glibc では取れる");
        std::hint::black_box((&chunks, &big));
        drop((chunks, big));
        // 他のテストの確保・解放が混ざっても、持っている分は必ず数に入る (下限だけを見る)
        assert!(m.in_use >= 512 * MB, "{m:?}");
        assert!(m.mmapped >= 64 * MB, "{m:?}");
        assert!(m.free > 0, "{m:?}");
        assert!(m.arena > 0, "{m:?}");
        // 通常の領域は使用中と空きに過不足なく分かれる (glibc の数え方。欄の並びを取り違えると崩れる)
        assert_eq!(m.arena, m.in_use + m.free, "{m:?}");
        // 行にも出る
        let none = (String::new(), String::new());
        let text = captured(|| log_minute(&Sample::default(), 0, &none, Some(&m), None));
        assert!(
            text.contains(&format!(
                "malloc_inuse_mb={} malloc_free_mb={} malloc_mmap_mb={} malloc_arena_mb={}",
                mb(m.in_use),
                mb(m.free),
                mb(m.mmapped),
                mb(m.arena)
            )),
            "{text}"
        );
    }
}
