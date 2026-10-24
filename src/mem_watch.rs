//! メモリの使用量を 15 秒おきに測り、1 分おきにログへ出す (2026-10-25)。
//! 前回から 100MB 以上増えたら、そのとき処理中の要求と、直前に終わった要求を一緒に出す。
//! 昼の短い山 (1.9GB) の原因を特定するための計測で、値は読むだけ。Linux 以外では何もしない。
use axum::{extract::Request, middleware::Next, response::Response};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const SAMPLE_EVERY: Duration = Duration::from_secs(15);
const LOG_EVERY_SAMPLES: u32 = 4;
const JUMP_BYTES: u64 = 100 * 1024 * 1024;
const RECENT_MAX: usize = 300;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
static INFLIGHT: LazyLock<Mutex<HashMap<u64, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// 終わった要求 (経路, 終わった時刻, かかった時間)。古いものから捨てる。
static RECENT: LazyLock<Mutex<VecDeque<(String, Instant, Duration)>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

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

fn mb(v: u64) -> u64 {
    v / (1024 * 1024)
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

pub fn spawn() {
    if sample().is_none() {
        tracing::info!("mem watch disabled (no /proc/self/status)");
        return;
    }
    tokio::spawn(async {
        let mut prev: Option<(Sample, Instant)> = None;
        let mut count: u32 = 0;
        let mut interval = tokio::time::interval(SAMPLE_EVERY);
        loop {
            interval.tick().await;
            let Some(now) = sample() else { continue };
            let at = Instant::now();
            if let Some((before, before_at)) = prev {
                let rose = |a: Option<u64>, b: Option<u64>| match (a, b) {
                    (Some(a), Some(b)) => a >= b + JUMP_BYTES,
                    _ => false,
                };
                if now.rss >= before.rss + JUMP_BYTES || rose(now.cgroup, before.cgroup) {
                    let (inflight, done) = activity(before_at);
                    tracing::warn!(
                        rss_mb = mb(now.rss),
                        rss_before_mb = mb(before.rss),
                        cgroup_mb = opt_mb(now.cgroup),
                        cgroup_before_mb = opt_mb(before.cgroup),
                        inflight = %inflight,
                        finished = %done,
                        "mem jump"
                    );
                }
            }
            if count.is_multiple_of(LOG_EVERY_SAMPLES) {
                tracing::info!(
                    rss_mb = mb(now.rss),
                    anon_mb = mb(now.rss_anon),
                    file_mb = mb(now.rss_file),
                    cgroup_mb = opt_mb(now.cgroup),
                    cgroup_anon_mb = opt_mb(now.cgroup_anon),
                    cgroup_file_mb = opt_mb(now.cgroup_file),
                    inflight = INFLIGHT.lock().unwrap().len(),
                    "mem"
                );
            }
            count = count.wrapping_add(1);
            prev = Some((now, at));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
