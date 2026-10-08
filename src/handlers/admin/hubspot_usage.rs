//! `GET /api/admin/hubspot-usage` : HubSpot 呼び出しの関所 (`hubspot::gateway`) の観測値 (管理者専用)。
//!
//! 返すのは数字だけ: 最後に見た `X-HubSpot-RateLimit-*` (10 秒・1 秒・1 日の残り)、起動してからの呼び出し回数
//! (種類別・Search・429・相乗り・混雑で断った回数)、キャッシュの当たり外れ、今の待ち行列と直近 5 分の待ち時間。
//! **鍵・レコードの中身・利用者は返さない**。HubSpot は呼ばない。
//! `require_admin_mw` の内側に置くので、非管理者は 403・未ログインは 401。

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use ts_rs::TS;

use crate::hubspot::gateway::{cache_stats_snapshot, Gateway, GatewaySnapshot, WAIT_WINDOW};
use crate::AppState;

/// 流量の設定 (環境変数で変えられる。`docs/env_variables_reference.md`)
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageLimits {
    /// このアプリが Search 以外に使う 1 秒あたりの上限
    pub per_second: u32,
    /// このアプリが Search 以外に使う 10 秒あたりの上限
    pub per_10s: u32,
    /// Search の開始間隔 (ms)
    #[ts(type = "number")]
    pub search_interval_ms: u64,
    /// 画面の操作が待てる上限 (ms)。超えると「混み合っています」
    #[ts(type = "number")]
    pub interactive_max_wait_ms: u64,
    /// 背景の取得が待てる上限 (ms)
    #[ts(type = "number")]
    pub background_max_wait_ms: u64,
}

/// 最後に見た HubSpot の応答ヘッダの値 (鍵を共有する他のバッチの分も含めたアカウント全体の残り)
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageRateLimit {
    /// 観測した時刻 (RFC 3339)。まだ 1 回も呼んでいなければ null
    pub observed_at: Option<String>,
    #[ts(type = "number | null")]
    pub per_10s_max: Option<u64>,
    #[ts(type = "number | null")]
    pub per_10s_remaining: Option<u64>,
    #[ts(type = "number | null")]
    pub per_second_max: Option<u64>,
    #[ts(type = "number | null")]
    pub per_second_remaining: Option<u64>,
    #[ts(type = "number | null")]
    pub daily_max: Option<u64>,
    #[ts(type = "number | null")]
    pub daily_remaining: Option<u64>,
}

/// 起動してからの合計
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageCounters {
    /// HubSpot へ実際に送った回数 (retry も 1 回)
    #[ts(type = "number")]
    pub calls: u64,
    #[ts(type = "number")]
    pub search_calls: u64,
    /// HubSpot が 429 (呼び出し回数の上限) を返した回数
    #[ts(type = "number")]
    pub rate_limited: u64,
    /// 同時の同じ読み取りに相乗りして、HubSpot を呼ばずに済んだ回数
    #[ts(type = "number")]
    pub coalesced: u64,
    /// 待ちが長すぎるので断った回数 (画面には「混み合っています」)
    #[ts(type = "number")]
    pub busy_rejected: u64,
}

/// 呼び出しの種類ごとの回数
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageGroup {
    pub key: String,
    pub label: String,
    #[ts(type = "number")]
    pub count: u64,
}

/// キャッシュごとの当たり外れ
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageCache {
    pub key: String,
    pub label: String,
    #[ts(type = "number")]
    pub hits: u64,
    #[ts(type = "number")]
    pub misses: u64,
}

/// 今の待ち行列と直近の待ち時間
#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageQueue {
    /// 許可を待っている呼び出しの数 (Search 以外)
    pub waiting: u32,
    /// 許可を待っている Search の数
    pub waiting_search: u32,
    /// 429 で全員を止めている残り時間 (ms)。止めていなければ null
    #[ts(type = "number | null")]
    pub paused_ms: Option<u64>,
    /// 直近の待ち時間の中央値 (ms)。記録が無ければ null
    #[ts(type = "number | null")]
    pub wait_p50_ms: Option<u64>,
    /// 直近の待ち時間の 95 パーセンタイル (ms)
    #[ts(type = "number | null")]
    pub wait_p95_ms: Option<u64>,
    /// 待ち時間を集計した件数
    pub wait_samples: u32,
    /// 待ち時間を集計する期間 (秒)
    pub wait_window_secs: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct HubSpotUsageResponse {
    /// HubSpot の鍵が設定されているか
    pub configured: bool,
    pub generated_at: String,
    pub limits: HubSpotUsageLimits,
    pub rate_limit: HubSpotUsageRateLimit,
    pub counters: HubSpotUsageCounters,
    /// 種類別の呼び出し回数 (回数の多い順)
    pub calls_by_group: Vec<HubSpotUsageGroup>,
    /// キャッシュごとの当たり外れ (名前順)
    pub caches: Vec<HubSpotUsageCache>,
    pub queue: HubSpotUsageQueue,
}

/// 呼び出しの種類 (`gateway::endpoint_group`) の表示名
fn group_label(key: &str) -> &'static str {
    match key {
        "search" => "検索 (Search)",
        "batch_read" => "まとめ読み",
        "associations_batch" => "関連のまとめ読み",
        "associations" => "関連・関連ラベル",
        "object_read" => "レコード 1 件の読み取り",
        "owners" => "担当者",
        "properties" => "プロパティ定義",
        "pipelines" => "パイプライン定義",
        "token_info" => "鍵の確認",
        _ => "その他",
    }
}

/// キャッシュ名の表示名
fn cache_label(key: &str) -> &'static str {
    match key {
        "call_queue_page" => "架電キューの一覧 (30 秒)",
        "call_queue_count" => "架電キューの件数 (60 秒)",
        "workspace" => "案件の詳細 (60 秒)",
        "owner_by_email" => "本人の担当者 ID (10 分)",
        "owners" => "担当者一覧 (10 分)",
        "pipelines" => "パイプライン・ステージ名 (5 分)",
        "property_catalog" => "プロパティ一覧 (6 時間)",
        "assoc_labels" => "関連ラベル (6 時間)",
        "crm_metadata" => "CRM の定義 (60 秒)",
        _ => "その他",
    }
}

fn ms(d: std::time::Duration) -> u64 {
    d.as_millis().min(u128::from(u64::MAX)) as u64
}

/// 観測値から応答を作る (テストで値を固定できるように分けている)
pub fn build_response(configured: bool, snap: &GatewaySnapshot) -> HubSpotUsageResponse {
    let mut calls_by_group: Vec<HubSpotUsageGroup> = snap
        .calls_by_group
        .iter()
        .map(|(k, n)| HubSpotUsageGroup {
            key: (*k).to_string(),
            label: group_label(k).to_string(),
            count: *n,
        })
        .collect();
    calls_by_group.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    let caches = cache_stats_snapshot()
        .into_iter()
        .map(|(k, (hits, misses))| HubSpotUsageCache {
            key: k.to_string(),
            label: cache_label(k).to_string(),
            hits,
            misses,
        })
        .collect();
    let r = &snap.rate_limit;
    HubSpotUsageResponse {
        configured,
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        limits: HubSpotUsageLimits {
            per_second: snap.config.per_second,
            per_10s: snap.config.per_10s,
            search_interval_ms: ms(snap.config.search_interval),
            interactive_max_wait_ms: ms(snap.config.interactive_max_wait),
            background_max_wait_ms: ms(snap.config.background_max_wait),
        },
        rate_limit: HubSpotUsageRateLimit {
            observed_at: r.observed_at.clone(),
            per_10s_max: r.max,
            per_10s_remaining: r.remaining,
            per_second_max: r.secondly,
            per_second_remaining: r.secondly_remaining,
            daily_max: r.daily,
            daily_remaining: r.daily_remaining,
        },
        counters: HubSpotUsageCounters {
            calls: snap.total_calls,
            search_calls: snap.search_calls,
            rate_limited: snap.rate_limited,
            coalesced: snap.coalesced,
            busy_rejected: snap.busy,
        },
        calls_by_group,
        caches,
        queue: HubSpotUsageQueue {
            waiting: snap.queue_general as u32,
            waiting_search: snap.queue_search as u32,
            paused_ms: snap.paused_for.map(ms),
            wait_p50_ms: snap.wait_p50.map(ms),
            wait_p95_ms: snap.wait_p95.map(ms),
            wait_samples: snap.wait_samples as u32,
            wait_window_secs: WAIT_WINDOW.as_secs() as u32,
        },
    }
}

/// GET /api/admin/hubspot-usage
pub async fn api_hubspot_usage(State(state): State<Arc<AppState>>) -> Response {
    // 本番の HubSpot クライアントは全部プロセス共有の関所を使う。CRM のクライアントがあればその関所
    // (= 共有のもの)、無ければ共有の関所をそのまま読む
    let gateway: Arc<Gateway> = match state.hubspot.as_deref() {
        Some(c) => c.gateway().clone(),
        None => Gateway::shared(),
    };
    let body = build_response(state.hubspot.is_some(), &gateway.snapshot());
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(body),
    )
        .into_response()
}

#[cfg(test)]
#[path = "hubspot_usage_tests.rs"]
mod hubspot_usage_tests;
