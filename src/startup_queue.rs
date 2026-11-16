//! 起動直後の重い裏の処理を 1 本ずつ走らせる順番待ち (2026-11-16)。
//!
//! 起動直後は、求人一覧の索引の復元・作り直し、直近 7 日の変更の集計、今月の架電成績、コンサルKPI のシートの先読み、
//! トークの振り返りの担当者の付け直しが重なって走り、一時確保の山が重なっていた (#403 の `mem task` の行)。
//! それぞれの最初の 1 回の重い部分を [`turn`] で包むと、同時に走るのは 1 本だけになる。
//!
//! - 効くのはプロセスが起動してから [`WINDOW`] の間に始めた分だけ。それより後 (6 時間ごとの作り直しなど普段の周期) は待たずに走る。
//! - 待つのは最長 [`MAX_WAIT`]。超えたら順番を持たずに走る (止まったままにしない)。
//! - 順番は来た順 (tokio の Semaphore は先着順)。
//! - 今日の電話の取り込み (トークの振り返り・接触の印の見つけ) と、画面が使う今日の架電成績の温めは包まない
//!   (遅らせない。#404 で起動から 2 分 36 秒にした今日の取り込みを守る)。
//! - 単一インスタンス前提 (プロセス内だけの順番)。
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, SemaphorePermit};

/// 起動からこの時間の間に始めた処理だけを順番待ちにする
pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// 順番を待つ最長の時間
pub const MAX_WAIT: Duration = Duration::from_secs(10 * 60);

static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
static QUEUE: Semaphore = Semaphore::const_new(1);

/// 起動の時刻を決める (main の最初に呼ぶ。呼ばなくても最初の [`turn`] の時刻になる)
pub fn mark_process_start() {
    LazyLock::force(&STARTED);
}

/// 起動直後なら順番を待って受け取る。受け取った値を持っている間は、ほかの起動直後の重い処理が待つ。
/// 起動直後でない・待ちすぎたときは None (待たずにそのまま走ってよい)。
/// テストでは順番待ちをしない (テストどうしが同じ順番を取り合って待たないように)。
pub async fn turn(name: &'static str) -> Option<SemaphorePermit<'static>> {
    if cfg!(test) {
        return None;
    }
    acquire(&QUEUE, *STARTED, name, WINDOW, MAX_WAIT).await
}

async fn acquire<'a>(
    queue: &'a Semaphore,
    started: Instant,
    name: &'static str,
    window: Duration,
    max_wait: Duration,
) -> Option<SemaphorePermit<'a>> {
    if started.elapsed() >= window {
        return None;
    }
    let asked = Instant::now();
    match tokio::time::timeout(max_wait, queue.acquire()).await {
        Ok(Ok(permit)) => {
            let waited = asked.elapsed();
            if waited >= Duration::from_secs(1) {
                tracing::info!(
                    task = name,
                    waited_ms = waited.as_millis() as u64,
                    "startup queue: got the turn"
                );
            }
            Some(permit)
        }
        Ok(Err(_)) => None,
        Err(_) => {
            tracing::warn!(
                task = name,
                "startup queue: waited too long, running without the turn"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: Duration = Duration::from_secs(3600);

    #[tokio::test(start_paused = true)]
    async fn 起動直後は一本ずつ_手放すと次が走る() {
        let q = Semaphore::new(1);
        let started = Instant::now();
        let first = acquire(&q, started, "a", LONG, LONG).await;
        assert!(first.is_some());
        // 2 本目は 1 本目が持っている間は受け取れない
        let second = tokio::time::timeout(
            Duration::from_secs(5),
            acquire(&q, started, "b", LONG, LONG),
        )
        .await;
        assert!(second.is_err(), "持っている間は待つ");
        drop(first);
        let second = acquire(&q, started, "b", LONG, LONG).await;
        assert!(second.is_some());
    }

    #[tokio::test]
    async fn 起動から時間が経てば待たない() {
        let q = Semaphore::new(1);
        let _held = q.acquire().await.unwrap();
        // 起動の時刻を窓より前にすると、持たれていてもすぐ None で返る
        let started = Instant::now() - Duration::from_secs(16 * 60);
        let got = tokio::time::timeout(
            Duration::from_secs(1),
            acquire(&q, started, "late", WINDOW, MAX_WAIT),
        )
        .await
        .expect("待たずに返る");
        assert!(got.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn 待ちすぎたら順番なしで走る() {
        let q = Semaphore::new(1);
        let _held = q.acquire().await.unwrap();
        let started = Instant::now();
        let got = acquire(&q, started, "slow", LONG, Duration::from_secs(600)).await;
        assert!(got.is_none(), "10 分待っても空かなければ順番を持たずに走る");
    }
}
