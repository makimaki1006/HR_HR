//! 管理画面の応答 struct と、監査 DB からそれを組み立てる関数 (W8 React 化、計画 §2.8 手順 1-3)。
//!
//! HTML (`handlers.rs` → `render.rs`) と JSON (`json.rs`、`/api/admin/*`) が **同じ struct** を使う。
//! `#[derive(TS)]` の型は `app_api::tests::export_ts_bindings` で `frontend/src/generated/` に書き出す。
//! 値は render 関数が受け取っていたものをそのまま持ち、表示用の加工 (日本語ラベル、30 日 KPI) も
//! ここで済ませる。React 側で計算をやり直さないため (旧新で値がずれる事故を避ける)。

use serde::Serialize;
use ts_rs::TS;

use crate::audit::dao::{self, AccountRow, ActivityLogRow, LoginSessionRow, UsageRow};
use crate::audit::AuditDb;

/// `GET /api/admin/users` (旧 `/admin/users`)。直近ログイン順、最大 500 件。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AdminUsersResponse {
    pub accounts: Vec<AccountRow>,
}

/// 直近 30 日の KPI (`user_detail_page` の 4 枚のカード)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AdminUserKpi30d {
    /// ログイン成功回数
    #[ts(type = "number")]
    pub login_ok: i64,
    /// ログイン失敗回数
    #[ts(type = "number")]
    pub login_fail: i64,
    /// 操作数 (activity_logs の件数)
    #[ts(type = "number")]
    pub activity: i64,
    /// 企業カルテ閲覧数 (`view_company_profile`)
    #[ts(type = "number")]
    pub company_views: i64,
}

/// `GET /api/admin/users/{account_id}` (旧 `/admin/users/{account_id}`)。
/// ログイン履歴は直近 100 件、操作履歴は直近 200 件。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AdminUserDetailResponse {
    pub account: AccountRow,
    pub sessions: Vec<LoginSessionRow>,
    pub activities: Vec<ActivityLogRow>,
    pub kpi_30d: AdminUserKpi30d,
}

/// `GET /api/admin/login-failures` (旧 `/admin/login-failures`)。直近の失敗 200 件。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AdminLoginFailuresResponse {
    pub failures: Vec<LoginSessionRow>,
}

/// 利用状況の集計 1 行。`UsageRow` に画面表示用の日本語ラベルを足したもの。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AdminUsageEntry {
    pub account_id: String,
    /// 機能別の集計では空。ユーザーが accounts に無ければ空 (画面は「(不明)」)。
    pub email: String,
    pub event_type: String,
    /// `view_tab` のときだけタブのパス。他は空。
    pub target_id: String,
    /// `event_type` (+ `target_id`) の日本語名。未知のコードはそのまま。
    pub label: String,
    #[ts(type = "number")]
    pub count: i64,
    pub last_at: String,
}

/// `GET /api/admin/usage?days=30` (旧 `/admin/usage`)。各表は多い順に最大 100 行。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AdminUsageResponse {
    /// 集計期間 (日)。1〜365 に丸めた後の値。
    #[ts(type = "number")]
    pub days: i64,
    pub by_event: Vec<AdminUsageEntry>,
    pub by_account: Vec<AdminUsageEntry>,
    pub cross: Vec<AdminUsageEntry>,
}

// ---------------------------------------------------------------------------
// 純粋関数 (テストしやすいよう DB と時刻を引数で受ける)
// ---------------------------------------------------------------------------

/// 「直近 30 日」の境界 (ISO-8601 UTC)。文字列比較で使う。
pub fn cutoff_30d() -> String {
    (chrono::Utc::now() - chrono::Duration::days(30))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// `cutoff` 以降の KPI。`user_detail_page` が行っていた計算をそのまま移したもの。
pub fn kpi_30d(
    sessions: &[LoginSessionRow],
    activities: &[ActivityLogRow],
    cutoff: &str,
) -> AdminUserKpi30d {
    let recent_s = sessions.iter().filter(|s| s.started_at.as_str() >= cutoff);
    let recent_a = || activities.iter().filter(|a| a.at.as_str() >= cutoff);
    AdminUserKpi30d {
        login_ok: recent_s.clone().filter(|s| s.success == 1).count() as i64,
        login_fail: recent_s.filter(|s| s.success == 0).count() as i64,
        activity: recent_a().count() as i64,
        company_views: recent_a()
            .filter(|a| a.event_type == "view_company_profile")
            .count() as i64,
    }
}

/// 詳細応答を組み立てる (KPI は `cutoff` 基準)。
pub fn user_detail(
    account: AccountRow,
    sessions: Vec<LoginSessionRow>,
    activities: Vec<ActivityLogRow>,
    cutoff: &str,
) -> AdminUserDetailResponse {
    let kpi_30d = kpi_30d(&sessions, &activities, cutoff);
    AdminUserDetailResponse {
        account,
        sessions,
        activities,
        kpi_30d,
    }
}

/// `?days=` を 1〜365 日に丸める (想定外の値で全期間スキャンにならないように)。既定 30。
pub fn clamp_days(days: Option<i64>) -> i64 {
    days.unwrap_or(30).clamp(1, 365)
}

/// 機能コード → 画面に出す日本語名。
///
/// 未知のコードはそのまま表示する（新しい記録を足したときに黙って消えないように）。
pub fn event_label(event_type: &str, target_id: &str) -> String {
    if event_type == "view_tab" {
        let name = match target_id {
            "/tab/survey" => "媒体分析",
            "/tab/jobmap" => "地図",
            "/tab/regional_analysis" => "地域分析",
            "/tab/company" => "企業検索",
            "/tab/driver" => "職種辞典",
            "/tab/license" => "資格辞書",
            "/tab/keyword_tools" => "キーワード需要",
            "/tab/jobgen_tools" => "求人票作成",
            "/tab/guide" => "使い方ガイド",
            other => other,
        };
        return format!("タブを開く: {name}");
    }
    match event_type {
        // 認証
        "login" => "ログイン".to_string(),
        "logout" => "ログアウト".to_string(),
        // 検索・調査
        "keyword_search" => "キーワード検索".to_string(),
        "keyword_seed_compare" => "見え方チェック(比較)".to_string(),
        "visibility_check" => "求人ページの見え方チェック".to_string(),
        "serp_search" => "検索結果の取得".to_string(),
        "view_company_profile" => "企業カルテを見る".to_string(),
        "view_industry_companies" => "業種別の企業一覧".to_string(),
        // 媒体分析
        "upload_survey_csv" | "upload" => "CSV取込".to_string(),
        "compare_public_jobs" => "公的求人データと比較".to_string(),
        "generate_survey_report" => "媒体分析レポート生成".to_string(),
        "generate_survey_guide" => "解説資料の生成".to_string(),
        "view_survey_report" => "媒体分析レポートを開く".to_string(),
        // レポート
        "generate_integrated_report" => "統合レポート生成".to_string(),
        "view_integrated_report" => "統合レポートを開く".to_string(),
        "generate_insight_report" => "示唆レポート生成".to_string(),
        // コンサル準備（社内用）
        "generate_consult_brief" => "商談準備レポートの作成".to_string(),
        "generate_consult_evidence_pack" => "証拠データJSONの出力".to_string(),
        "generate_consult_hearing_sheet" => "ヒアリングシートの作成".to_string(),
        "generate_consult_action_memo" => "アクションメモの作成".to_string(),
        "view_consult_hearing_form" => "ヒアリング入力を開く".to_string(),
        "save_consult_hearing" => "ヒアリング内容の保存".to_string(),
        "view_consult_hypothesis_review" => "仮説の確認画面を開く".to_string(),
        "save_consult_hypothesis_review" => "仮説の確認内容を保存".to_string(),
        // その他
        "download_csv" => "CSVダウンロード".to_string(),
        "update_profile" => "プロフィール更新".to_string(),
        // 未知のコードはそのまま出す（記録を足したときに黙って消えないように）
        other => other.to_string(),
    }
}

fn usage_entry(r: UsageRow) -> AdminUsageEntry {
    let label = event_label(&r.event_type, &r.target_id);
    AdminUsageEntry {
        account_id: r.account_id,
        email: r.email,
        event_type: r.event_type,
        target_id: r.target_id,
        label,
        count: r.count,
        last_at: r.last_at,
    }
}

/// 集計行 3 種から利用状況の応答を組み立てる。
pub fn usage_response(
    days: i64,
    by_event: Vec<UsageRow>,
    by_account: Vec<UsageRow>,
    cross: Vec<UsageRow>,
) -> AdminUsageResponse {
    AdminUsageResponse {
        days,
        by_event: by_event.into_iter().map(usage_entry).collect(),
        by_account: by_account.into_iter().map(usage_entry).collect(),
        cross: cross.into_iter().map(usage_entry).collect(),
    }
}

// ---------------------------------------------------------------------------
// 監査 DB からの読み出し (reqwest::blocking なので spawn_blocking。失敗時は空 + warn)
// ---------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    what: &'static str,
    f: impl FnOnce() -> T + Send + 'static,
    fallback: impl FnOnce() -> T,
) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("{what} spawn_blocking join failed: {e}");
            fallback()
        }
    }
}

/// アカウント一覧 (直近ログイン順、最大 500 件)
pub async fn load_users(audit: &AuditDb) -> AdminUsersResponse {
    let audit = audit.clone();
    let accounts = blocking(
        "admin_users_list",
        move || dao::list_accounts(audit.turso(), 500),
        Vec::new,
    )
    .await;
    AdminUsersResponse { accounts }
}

/// 顧客詳細。アカウントが無ければ `None`。
pub async fn load_user_detail(
    audit: &AuditDb,
    account_id: &str,
) -> Option<AdminUserDetailResponse> {
    let audit = audit.clone();
    let aid = account_id.to_string();
    // 3 つの blocking DAO 呼出を 1 度の spawn_blocking にまとめる (順次実行、合算 IO は不変)
    let (acc, sessions, activities) = blocking(
        "admin_user_detail",
        move || {
            let acc = dao::find_account_by_id(audit.turso(), &aid);
            let sessions = dao::list_sessions_for_account(audit.turso(), &aid, 100);
            let activities = dao::list_activity_for_account(audit.turso(), &aid, 200);
            (acc, sessions, activities)
        },
        || (None, Vec::new(), Vec::new()),
    )
    .await;
    acc.map(|acc| user_detail(acc, sessions, activities, &cutoff_30d()))
}

/// 最近の失敗ログ (最大 200 件)
pub async fn load_login_failures(audit: &AuditDb) -> AdminLoginFailuresResponse {
    let audit = audit.clone();
    let failures = blocking(
        "admin_login_failures",
        move || dao::list_recent_failures(audit.turso(), 200),
        Vec::new,
    )
    .await;
    AdminLoginFailuresResponse { failures }
}

/// 利用状況 (`days` は `clamp_days` 済みの値を渡す)
pub async fn load_usage(audit: &AuditDb, days: i64) -> AdminUsageResponse {
    let since = (chrono::Utc::now() - chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let audit = audit.clone();
    let (by_event, by_account, cross) = blocking(
        "admin_usage",
        move || {
            let turso = audit.turso();
            (
                dao::usage_by_event(turso, &since, 100),
                dao::usage_by_account(turso, &since, 100),
                dao::usage_by_account_and_event(turso, &since, 100),
            )
        },
        || (Vec::new(), Vec::new(), Vec::new()),
    )
    .await;
    usage_response(days, by_event, by_account, cross)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::test_fixtures as fx;

    /// KPI は境界の文字列比較 (cutoff 以上を数える)
    #[test]
    fn kpi_30d_counts_only_rows_at_or_after_cutoff() {
        let k = kpi_30d(
            &fx::hanako_sessions(),
            &fx::hanako_activities(),
            "2050-01-01T00:00:00Z",
        );
        assert_eq!(
            k,
            AdminUserKpi30d {
                login_ok: 1,
                login_fail: 1,
                activity: 3,
                company_views: 2
            }
        );
        // 境界ちょうどは含む / 全部が古い扱いなら 0
        let k = kpi_30d(
            &fx::hanako_sessions(),
            &fx::hanako_activities(),
            "2099-01-02T09:00:00Z",
        );
        assert_eq!(k.login_ok, 1);
        assert_eq!(k.login_fail, 0, "08:00 の失敗は 09:00 の境界より前");
        let k = kpi_30d(
            &fx::hanako_sessions(),
            &fx::hanako_activities(),
            "2100-01-01T00:00:00Z",
        );
        assert_eq!(
            (k.login_ok, k.login_fail, k.activity, k.company_views),
            (0, 0, 0, 0)
        );
        assert!(cutoff_30d().ends_with('Z'));
    }

    #[test]
    fn clamp_days_bounds() {
        assert_eq!(clamp_days(None), 30);
        assert_eq!(clamp_days(Some(7)), 7);
        assert_eq!(clamp_days(Some(0)), 1);
        assert_eq!(clamp_days(Some(-5)), 1);
        assert_eq!(clamp_days(Some(9999)), 365);
    }

    /// タブ名は URL でなく日本語で出す
    #[test]
    fn tab_paths_are_shown_in_japanese() {
        assert_eq!(
            event_label("view_tab", "/tab/survey"),
            "タブを開く: 媒体分析"
        );
        assert_eq!(
            event_label("view_tab", "/tab/company"),
            "タブを開く: 企業検索"
        );
        // 未知のパスは握り潰さずそのまま出す（記録が黙って消えないように）
        assert_eq!(
            event_label("view_tab", "/tab/unknown"),
            "タブを開く: /tab/unknown"
        );
        assert_eq!(event_label("mystery_event", ""), "mystery_event");
    }

    /// 記録している全イベントが日本語名を持つ（内部コードの露出を防ぐ）
    #[test]
    fn every_recorded_event_has_a_japanese_label() {
        let recorded = [
            "login",
            "logout",
            "upload_survey_csv",
            "generate_survey_report",
            "generate_survey_guide",
            "generate_integrated_report",
            "generate_insight_report",
            "generate_consult_brief",
            "generate_consult_evidence_pack",
            "generate_consult_hearing_sheet",
            "generate_consult_action_memo",
            "view_consult_hearing_form",
            "save_consult_hearing",
            "view_consult_hypothesis_review",
            "save_consult_hypothesis_review",
            "view_company_profile",
            "view_industry_companies",
            "download_csv",
            "update_profile",
            "view_tab",
            "keyword_search",
            "keyword_seed_compare",
            "visibility_check",
            "serp_search",
            "view_survey_report",
            "view_integrated_report",
            "compare_public_jobs",
        ];
        for ev in recorded {
            let label = event_label(ev, "/tab/survey");
            assert_ne!(
                label, ev,
                "{ev} に日本語名が必要（内部コードが画面に出ている）"
            );
        }
    }

    /// 集計行 → エントリ: ラベルが付き、他の値はそのまま
    #[test]
    fn usage_response_attaches_labels_and_keeps_values() {
        let (by_event, by_account, cross) = fx::usage_rows();
        let r = usage_response(30, by_event, by_account, cross);
        assert_eq!(r.days, 30);
        assert_eq!(r.by_event[0].label, "タブを開く: 媒体分析");
        assert_eq!(r.by_event[0].count, 42);
        assert_eq!(r.by_event[1].label, "CSV取込");
        assert_eq!(r.by_event[2].label, "mystery_event");
        assert_eq!(r.by_account[0].email, "hanako@f-a-c.co.jp");
        assert_eq!(r.by_account[0].account_id, fx::HANAKO_ID);
        assert_eq!(r.by_account[0].count, 49);
        assert_eq!(r.cross.len(), 3);
        assert_eq!(r.cross[2].email, "");
    }
}
