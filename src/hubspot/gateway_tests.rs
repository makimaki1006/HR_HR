//! 関所 (`gateway.rs`) のテスト。仮想時計 (`start_paused`) で待ちを再現する (HTTP は使わない)。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use super::*;

fn cfg(per_second: u32, per_10s: u32) -> GatewayConfig {
    GatewayConfig {
        per_second,
        per_10s,
        search_interval: Duration::from_secs(1),
        interactive_max_wait: Duration::from_secs(600),
        background_max_wait: Duration::from_secs(600),
        rate_limited_min_pause: Duration::from_secs(1),
        max_pause: MAX_PAUSE,
    }
}

/// どの長さ `w` の区間 (t-w, t] を切り取っても `max` 件以下か
fn max_in_window(at: &[Instant], w: Duration) -> usize {
    let mut sorted = at.to_vec();
    sorted.sort();
    let mut best = 0;
    for (i, t) in sorted.iter().enumerate() {
        // t を右端とする区間 (t-w, t] に入る件数
        let n = sorted[..=i].iter().filter(|g| **g + w > *t).count();
        best = best.max(n);
    }
    best
}

#[tokio::test(start_paused = true)]
async fn 二百本同時でも_1秒あたり_10秒あたりの上限を超えない() {
    let gw = Arc::new(Gateway::new(cfg(8, 80)));
    let grants = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..200 {
        let (gw, grants) = (gw.clone(), grants.clone());
        let p = if i % 3 == 0 {
            Priority::Background
        } else {
            Priority::Interactive
        };
        tasks.push(tokio::spawn(async move {
            gw.acquire(Lane::General, p).await.unwrap();
            grants.lock().unwrap().push(Instant::now());
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let at = grants.lock().unwrap().clone();
    assert_eq!(at.len(), 200, "全員が許可された");
    assert_eq!(max_in_window(&at, Duration::from_secs(1)), 8, "1 秒の窓");
    assert_eq!(max_in_window(&at, Duration::from_secs(10)), 80, "10 秒の窓");
    // 200 件は 10 秒に 80 件までなので、最後の許可は 20 秒以上後 (80 件 × 2 窓 + 40 件)
    let last = at.iter().max().unwrap().duration_since(start);
    assert!(last >= Duration::from_secs(20), "last {last:?}");
    let snap = gw.snapshot();
    assert_eq!(snap.granted, 200);
    assert_eq!(snap.busy, 0);
    assert_eq!(snap.queue_general, 0, "列は空に戻る");
}

#[tokio::test(start_paused = true)]
async fn search_の窓は通常の窓と別で間隔を空ける() {
    let gw = Arc::new(Gateway::new(cfg(8, 80)));
    let start = Instant::now();
    for _ in 0..3 {
        gw.acquire(Lane::Search, Priority::Interactive)
            .await
            .unwrap();
    }
    // 1 秒に 1 回: 0s, 1s, 2s
    assert_eq!(start.elapsed(), Duration::from_secs(2));
    // 通常の窓は Search に食われていない
    let t = Instant::now();
    for _ in 0..8 {
        gw.acquire(Lane::General, Priority::Interactive)
            .await
            .unwrap();
    }
    assert_eq!(t.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn 画面の操作は先に並んだ背景の取得より先に通る() {
    let gw = Arc::new(Gateway::new(cfg(1, 0)));
    // 枠を 1 つ使っておく (以後は 1 秒に 1 件)
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for name in ["bg1", "bg2", "bg3"] {
        let (gw, order) = (gw.clone(), order.clone());
        tasks.push(tokio::spawn(async move {
            gw.acquire(Lane::General, Priority::Background)
                .await
                .unwrap();
            order.lock().unwrap().push(name);
        }));
    }
    // 背景が先に並び終わるまで進める (時計は止まったまま)
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert_eq!(gw.snapshot().queue_general, 3);
    for name in ["ui1", "ui2"] {
        let (gw, order) = (gw.clone(), order.clone());
        tasks.push(tokio::spawn(async move {
            gw.acquire(Lane::General, Priority::Interactive)
                .await
                .unwrap();
            order.lock().unwrap().push(name);
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(
        *order.lock().unwrap(),
        vec!["ui1", "ui2", "bg1", "bg2", "bg3"],
        "画面の操作が先、同じ優先度の中は到着順"
    );
}

#[tokio::test(start_paused = true)]
async fn 待ちの見積もりが上限を超えるなら並ばずに断る() {
    let mut c = cfg(1, 0);
    c.interactive_max_wait = Duration::from_millis(2_500);
    let gw = Arc::new(Gateway::new(c));
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let start = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let gw = gw.clone();
        tasks.push(tokio::spawn(async move {
            let r = gw.acquire(Lane::General, Priority::Interactive).await;
            (r, start.elapsed())
        }));
        // 到着順を固定する
        tokio::task::yield_now().await;
    }
    let mut ok = Vec::new();
    let mut busy = Vec::new();
    for t in tasks {
        let (r, at) = t.await.unwrap();
        match r {
            Ok(_) => ok.push(at),
            Err(Busy) => busy.push(at),
        }
    }
    // 1 件目: 1 秒待ち、2 件目: 2 秒待ち (2.5 秒以内)。3 件目以降は 3 秒以上の見込みなので即座に断る
    assert_eq!(
        ok,
        vec![Duration::from_secs(1), Duration::from_secs(2)],
        "ok {ok:?}"
    );
    assert_eq!(busy, vec![Duration::ZERO; 3], "並ばずに断る: {busy:?}");
    assert_eq!(gw.snapshot().busy, 3);
}

#[tokio::test(start_paused = true)]
async fn 並んだ後でも待ちの上限に達したら列から抜けて断る() {
    let mut c = cfg(5, 0);
    c.interactive_max_wait = Duration::from_secs(2);
    let gw = Arc::new(Gateway::new(c));
    let waiter = {
        let gw = gw.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            // 並んだ時点では待ちは無い見込み
            tokio::task::yield_now().await;
            let r = gw.acquire(Lane::General, Priority::Interactive).await;
            (r, start.elapsed())
        })
    };
    // 先に 429 で 10 秒止める (並んだ人の見積もりは 10 秒 > 2 秒なので断られる)
    gw.on_rate_limited(Some(Duration::from_secs(10)));
    let (r, at) = waiter.await.unwrap();
    assert_eq!(r, Err(Busy));
    assert_eq!(at, Duration::ZERO, "見込みの時点で断る");

    // 並んだ後に止まった場合: 上限 (2 秒) で抜ける
    let gw = Arc::new(Gateway::new({
        let mut c = cfg(1, 0);
        c.interactive_max_wait = Duration::from_secs(2);
        c
    }));
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let start = Instant::now();
    let w = {
        let gw = gw.clone();
        tokio::spawn(async move { gw.acquire(Lane::General, Priority::Interactive).await })
    };
    tokio::task::yield_now().await;
    assert_eq!(gw.snapshot().queue_general, 1, "1 秒待ちの見込みで並んだ");
    gw.on_rate_limited(Some(Duration::from_secs(10)));
    assert_eq!(w.await.unwrap(), Err(Busy));
    assert_eq!(
        start.elapsed(),
        Duration::from_secs(2),
        "上限ちょうどで抜ける"
    );
    assert_eq!(gw.snapshot().queue_general, 0, "抜けた券は列に残らない");
}

#[tokio::test(start_paused = true)]
async fn 一度の_429_で全員が_retry_after_の間止まる() {
    let gw = Arc::new(Gateway::new(cfg(8, 80)));
    gw.on_rate_limited(Some(Duration::from_secs(3)));
    let start = Instant::now();
    let mut tasks = Vec::new();
    for lane in [Lane::General, Lane::Search, Lane::General] {
        let gw = gw.clone();
        tasks.push(tokio::spawn(async move {
            gw.acquire(lane, Priority::Background).await.unwrap();
            start.elapsed()
        }));
    }
    for t in tasks {
        assert!(t.await.unwrap() >= Duration::from_secs(3));
    }
    let snap = gw.snapshot();
    assert_eq!(snap.rate_limited, 1);
    assert_eq!(snap.paused_for, None, "停止は明けている");
}

#[tokio::test(start_paused = true)]
async fn retry_after_が無い_0_の_429_も最低時間は止め_大きすぎる値は上限で切る() {
    let gw = Gateway::new(cfg(8, 80));
    let start = Instant::now();
    gw.on_rate_limited(Some(Duration::ZERO));
    gw.acquire(Lane::General, Priority::Background)
        .await
        .unwrap();
    assert_eq!(start.elapsed(), Duration::from_secs(1));
    let start = Instant::now();
    gw.on_rate_limited(Some(Duration::from_secs(3600)));
    gw.acquire(Lane::General, Priority::Background)
        .await
        .unwrap();
    assert_eq!(start.elapsed(), MAX_PAUSE);
}

#[tokio::test(start_paused = true)]
async fn 取り消された待ちは列に残らず後ろの人が進む() {
    let gw = Arc::new(Gateway::new(cfg(1, 0)));
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let first = {
        let gw = gw.clone();
        tokio::spawn(async move { gw.acquire(Lane::General, Priority::Interactive).await })
    };
    tokio::task::yield_now().await;
    let second = {
        let gw = gw.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            gw.acquire(Lane::General, Priority::Interactive)
                .await
                .unwrap();
            start.elapsed()
        })
    };
    tokio::task::yield_now().await;
    assert_eq!(gw.snapshot().queue_general, 2);
    first.abort();
    // 先頭が抜けたので 2 番目が 1 秒後 (窓が空いた時点) に通る
    assert_eq!(second.await.unwrap(), Duration::from_secs(1));
    assert_eq!(gw.snapshot().queue_general, 0);
}

#[tokio::test(start_paused = true)]
async fn 待ち時間の_p50_p95_と列の長さを数える() {
    let gw = Arc::new(Gateway::new(cfg(1, 0)));
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let gw = gw.clone();
        tasks.push(tokio::spawn(async move {
            gw.acquire(Lane::General, Priority::Interactive)
                .await
                .unwrap()
        }));
        tokio::task::yield_now().await;
    }
    for t in tasks {
        t.await.unwrap();
    }
    let snap = gw.snapshot();
    // 0,1,..,19 秒待ち。nearest-rank: p50 = 10 番目 = 9 秒、p95 = 19 番目 = 18 秒
    assert_eq!(snap.wait_samples, 20);
    assert_eq!(snap.wait_p50, Some(Duration::from_secs(9)));
    assert_eq!(snap.wait_p95, Some(Duration::from_secs(18)));
    // 5 分経つと対象から外れる
    tokio::time::advance(WAIT_WINDOW + Duration::from_secs(1)).await;
    let snap = gw.snapshot();
    assert_eq!(snap.wait_samples, 0);
    assert_eq!(snap.wait_p50, None);
}

#[test]
fn 応答ヘッダの_ratelimit_を記録し_無いヘッダは前回値を残す() {
    let gw = Gateway::new(cfg(8, 80));
    let mut h = HeaderMap::new();
    h.insert("x-hubspot-ratelimit-max", "190".parse().unwrap());
    h.insert("x-hubspot-ratelimit-remaining", "150".parse().unwrap());
    h.insert("x-hubspot-ratelimit-secondly", "19".parse().unwrap());
    h.insert(
        "x-hubspot-ratelimit-secondly-remaining",
        "17".parse().unwrap(),
    );
    h.insert("x-hubspot-ratelimit-daily", "625000".parse().unwrap());
    h.insert(
        "x-hubspot-ratelimit-daily-remaining",
        "600123".parse().unwrap(),
    );
    h.insert(
        "x-hubspot-ratelimit-interval-milliseconds",
        "10000".parse().unwrap(),
    );
    gw.record_headers(&h);
    let r = gw.snapshot().rate_limit;
    assert_eq!(r.max, Some(190));
    assert_eq!(r.remaining, Some(150));
    assert_eq!(r.secondly, Some(19));
    assert_eq!(r.secondly_remaining, Some(17));
    assert_eq!(r.daily, Some(625_000));
    assert_eq!(r.daily_remaining, Some(600_123));
    assert_eq!(r.interval_ms, Some(10_000));
    assert!(r.observed_at.is_some());
    // 一部だけのヘッダは、その値だけ更新する
    let mut h2 = HeaderMap::new();
    h2.insert("x-hubspot-ratelimit-remaining", "149".parse().unwrap());
    gw.record_headers(&h2);
    let r = gw.snapshot().rate_limit;
    assert_eq!(r.remaining, Some(149));
    assert_eq!(r.daily_remaining, Some(600_123));
    // 何も無い応答 (Search 等) では変えない
    gw.record_headers(&HeaderMap::new());
    assert_eq!(gw.snapshot().rate_limit.remaining, Some(149));
}

#[test]
fn 呼び出しの種類をパスから決める() {
    assert_eq!(endpoint_group("/crm/v3/objects/deals/search"), "search");
    assert_eq!(
        endpoint_group("/crm/v3/objects/deals/batch/read"),
        "batch_read"
    );
    assert_eq!(
        endpoint_group("/crm/v4/associations/deals/contacts/batch/read"),
        "associations_batch"
    );
    assert_eq!(
        endpoint_group("/crm/v4/objects/deals/1/associations/contacts"),
        "associations"
    );
    assert_eq!(endpoint_group("/crm/v3/objects/deals/1"), "object_read");
    assert_eq!(endpoint_group("/crm/v3/owners"), "owners");
    assert_eq!(endpoint_group("/crm/v3/properties/deals"), "properties");
    assert_eq!(endpoint_group("/crm/v3/pipelines/deals"), "pipelines");
    assert_eq!(
        endpoint_group("/oauth/v2/private-apps/get/access-token-info"),
        "token_info"
    );
}

#[test]
fn 環境変数が無ければ既定値で_範囲外は既定値に戻す() {
    // 既定値 (テストでは環境変数を書き換えない。値の解釈は env_u64 で確かめる)
    let d = GatewayConfig::default();
    assert_eq!(d.per_second, 8);
    assert_eq!(d.per_10s, 80);
    assert_eq!(d.search_interval, Duration::from_millis(333));
    assert_eq!(d.interactive_max_wait, Duration::from_secs(5));
    assert_eq!(d.background_max_wait, Duration::from_secs(60));
    let v = |raw: Option<&str>| env_u64("X", raw.map(str::to_string), 7, 1, 10);
    assert_eq!(v(None), 7, "未設定");
    assert_eq!(v(Some("  ")), 7, "空白だけ");
    assert_eq!(v(Some(" 4 ")), 4, "前後の空白は落とす");
    assert_eq!(v(Some("10")), 10, "上限ちょうど");
    assert_eq!(v(Some("11")), 7, "範囲外");
    assert_eq!(v(Some("0")), 7, "範囲外");
    assert_eq!(v(Some("abc")), 7, "数字でない");
}

#[tokio::test(start_paused = true)]
async fn 書き込みの続きは先に並んだ新しい読み取りより先に通る() {
    let gw = Arc::new(Gateway::new(cfg(1, 0)));
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for name in ["read1", "read2"] {
        let (gw, order) = (gw.clone(), order.clone());
        tasks.push(tokio::spawn(async move {
            gw.acquire(Lane::General, Priority::Interactive)
                .await
                .unwrap();
            order.lock().unwrap().push(name);
        }));
    }
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert_eq!(gw.snapshot().queue_general, 2);
    let (gw2, order2) = (gw.clone(), order.clone());
    tasks.push(tokio::spawn(async move {
        gw2.acquire(Lane::General, Priority::Continuation)
            .await
            .unwrap();
        order2.lock().unwrap().push("patch");
    }));
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(
        *order.lock().unwrap(),
        vec!["patch", "read1", "read2"],
        "読み取りが済んだ保存の PATCH を先に通す (読み取りだけ無駄にしない)"
    );
}

#[tokio::test(start_paused = true)]
async fn 続きの待ちの上限は読み取りより長く_429_の最長停止を待てる() {
    let mut c = cfg(8, 0);
    c.interactive_max_wait = Duration::from_secs(5);
    let gw = Arc::new(Gateway::new(c));
    gw.on_rate_limited(Some(Duration::from_secs(8)));
    // 8 秒の停止: 新しい読み取りは 5 秒の上限を超えるので断る
    assert_eq!(
        gw.acquire(Lane::General, Priority::Interactive).await,
        Err(Busy)
    );
    // 続きは (5 + 10 秒) まで待てるので、停止が明けたら通る
    let waited = gw
        .acquire(Lane::General, Priority::Continuation)
        .await
        .unwrap();
    assert!(waited >= Duration::from_secs(7), "{waited:?}");
}

#[tokio::test(start_paused = true)]
async fn 受け入れの見積もりは枠も列も取らずに_読み取りと_patch_の分を数える() {
    let mut c = cfg(1, 0);
    c.interactive_max_wait = Duration::from_millis(2_500);
    let gw = Arc::new(Gateway::new(c));
    // 空いている: 2 回 (読み取り + PATCH) 分を見積もっても 2.5 秒に収まる
    assert_eq!(gw.admit(Lane::General, Priority::Interactive, 2), Ok(()));
    assert_eq!(gw.snapshot().granted, 0, "枠は取らない");
    assert_eq!(gw.snapshot().queue_general, 0, "列にも並ばない");
    // 枠を使い切り、先に 2 件並んでいる: 1 秒 + 1 秒 × (2 + 2) > 2.5 秒
    gw.acquire(Lane::General, Priority::Interactive)
        .await
        .unwrap();
    let mut waiting = Vec::new();
    for _ in 0..2 {
        let gw = gw.clone();
        waiting.push(tokio::spawn(async move {
            gw.acquire(Lane::General, Priority::Interactive).await
        }));
    }
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    let before = gw.snapshot().busy;
    assert_eq!(
        gw.admit(Lane::General, Priority::Interactive, 2),
        Err(Busy),
        "読み取りの後ろで PATCH が断られそうなら、読み取る前に断る"
    );
    assert_eq!(gw.snapshot().busy, before + 1, "断った回数に数える");
    for w in waiting {
        let _ = w.await;
    }
}
