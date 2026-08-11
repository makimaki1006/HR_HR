//! 管理画面 HTML レンダリング
//! Tailwind + minimal HTMX。既存テーマ (navy-900 背景) と整合。

use crate::audit::dao::{AccountRow, ActivityLogRow, LoginSessionRow};
use crate::handlers::helpers::escape_html;

fn layout(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>{title}</title>
<script src="https://cdn.tailwindcss.com"></script>
<script src="/static/js/vendor/htmx-v2.0.3.min.js"></script>
<style>body{{background:#0f172a;color:#e2e8f0;font-family:ui-sans-serif,system-ui;}}</style>
</head>
<body class="min-h-screen p-6">
<nav class="mb-6 flex items-center gap-4 text-sm">
  <a href="/" class="text-slate-400 hover:text-white">← ダッシュボード</a>
  <span class="text-slate-600">|</span>
  <a href="/admin/usage" class="text-blue-400 hover:text-blue-300">利用状況</a>
  <a href="/admin/users" class="text-blue-400 hover:text-blue-300">ユーザー一覧</a>
  <a href="/admin/login-failures" class="text-blue-400 hover:text-blue-300">失敗監視</a>
  <a href="/my/activity" class="text-blue-400 hover:text-blue-300 ml-auto">自分の履歴</a>
</nav>
{body}
</body>
</html>"#,
        title = escape_html(title)
    )
}

pub fn no_audit_db() -> String {
    layout(
        "監査DB未接続",
        r#"<div class="p-8 rounded bg-red-900/30 border border-red-700">
           <h1 class="text-xl font-bold mb-2">監査機能が有効ではありません</h1>
           <p class="text-slate-300">環境変数 AUDIT_TURSO_URL / AUDIT_TURSO_TOKEN を設定してください。</p>
        </div>"#,
    )
}

pub fn not_found(id: &str) -> String {
    layout(
        "アカウント未検出",
        &format!(
            r#"<div class="p-8 rounded bg-amber-900/30 border border-amber-700">
               <h1 class="text-xl font-bold mb-2">アカウントが見つかりません</h1>
               <p class="text-slate-400">ID: {}</p>
            </div>"#,
            escape_html(id)
        ),
    )
}

pub fn users_list_page(accounts: &[AccountRow]) -> String {
    let mut rows = String::new();
    for a in accounts {
        rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700 hover:bg-slate-800/50">
                 <td class="py-2 px-3"><a class="text-blue-400 hover:underline" href="/admin/users/{id}">{email}</a></td>
                 <td class="py-2 px-3">{name}</td>
                 <td class="py-2 px-3">{company}</td>
                 <td class="py-2 px-3"><span class="{role_class}">{role}</span></td>
                 <td class="py-2 px-3 text-right">{count}</td>
                 <td class="py-2 px-3 text-slate-400 text-xs">{last}</td>
                 <td class="py-2 px-3 text-slate-400 text-xs">{first}</td>
                 <td class="py-2 px-3">{disabled}</td>
               </tr>"#,
            id = escape_html(&a.id),
            email = escape_html(&a.email),
            name = escape_html(if a.display_name.is_empty() { "-" } else { &a.display_name }),
            company = escape_html(if a.company.is_empty() { "-" } else { &a.company }),
            role_class = if a.role == "admin" { "px-2 py-0.5 rounded bg-purple-900 text-purple-300 text-xs" } else { "text-slate-400 text-xs" },
            role = escape_html(&a.role),
            count = a.login_count,
            last = escape_html(&a.last_login_at),
            first = escape_html(&a.first_seen_at),
            disabled = if a.disabled_at.is_empty() { "" } else { "<span class=\"px-2 py-0.5 rounded bg-red-900 text-red-300 text-xs\">無効</span>" },
        ));
    }

    let body = format!(
        r#"<h1 class="text-2xl font-bold mb-4">ユーザー一覧 ({} 件)</h1>
<div class="overflow-x-auto rounded bg-slate-800/30">
  <table class="w-full text-sm">
    <thead class="bg-slate-800 text-slate-300 text-xs uppercase">
      <tr>
        <th class="py-2 px-3 text-left">メール</th>
        <th class="py-2 px-3 text-left">氏名</th>
        <th class="py-2 px-3 text-left">会社</th>
        <th class="py-2 px-3 text-left">権限</th>
        <th class="py-2 px-3 text-right">ログイン回数</th>
        <th class="py-2 px-3 text-left">最終ログイン</th>
        <th class="py-2 px-3 text-left">初回ログイン</th>
        <th class="py-2 px-3 text-left">状態</th>
      </tr>
    </thead>
    <tbody>{rows}</tbody>
  </table>
</div>"#,
        accounts.len()
    );
    layout("ユーザー一覧 - 管理", &body)
}

pub fn user_detail_page(
    acc: &AccountRow,
    sessions: &[LoginSessionRow],
    activities: &[ActivityLogRow],
) -> String {
    // プロフィール
    let profile = format!(
        r#"<section class="mb-6 p-4 rounded bg-slate-800/40">
  <h2 class="text-xl font-bold mb-3">{email}</h2>
  <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
    <div><span class="text-slate-400 block">氏名</span>{name}</div>
    <div><span class="text-slate-400 block">会社</span>{company}</div>
    <div><span class="text-slate-400 block">権限</span>{role}</div>
    <div><span class="text-slate-400 block">ログイン回数</span>{count}</div>
    <div><span class="text-slate-400 block">初回</span>{first}</div>
    <div><span class="text-slate-400 block">最終</span>{last}</div>
    <div><span class="text-slate-400 block">ID</span><code class="text-xs">{id}</code></div>
    <div><span class="text-slate-400 block">状態</span>{disabled}</div>
  </div>
</section>"#,
        email = escape_html(&acc.email),
        name = escape_html(if acc.display_name.is_empty() {
            "-"
        } else {
            &acc.display_name
        }),
        company = escape_html(if acc.company.is_empty() {
            "-"
        } else {
            &acc.company
        }),
        role = escape_html(&acc.role),
        count = acc.login_count,
        first = escape_html(&acc.first_seen_at),
        last = escape_html(&acc.last_login_at),
        id = escape_html(&acc.id),
        disabled = if acc.disabled_at.is_empty() {
            "<span class=\"text-green-400\">有効</span>"
        } else {
            "<span class=\"text-red-400\">無効</span>"
        }
    );

    // KPI: 先月のログイン数・操作数
    let kpis = {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(30))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let login_30d = sessions
            .iter()
            .filter(|s| s.started_at.as_str() >= cutoff.as_str() && s.success == 1)
            .count();
        let fail_30d = sessions
            .iter()
            .filter(|s| s.started_at.as_str() >= cutoff.as_str() && s.success == 0)
            .count();
        let activity_30d = activities
            .iter()
            .filter(|a| a.at.as_str() >= cutoff.as_str())
            .count();
        let company_views_30d = activities
            .iter()
            .filter(|a| a.at.as_str() >= cutoff.as_str() && a.event_type == "view_company_profile")
            .count();
        format!(
            r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
  <div class="p-4 rounded bg-slate-800/40"><div class="text-slate-400 text-xs">直近30日 ログイン成功</div><div class="text-3xl font-bold">{login_30d}</div></div>
  <div class="p-4 rounded bg-slate-800/40"><div class="text-slate-400 text-xs">直近30日 ログイン失敗</div><div class="text-3xl font-bold text-red-400">{fail_30d}</div></div>
  <div class="p-4 rounded bg-slate-800/40"><div class="text-slate-400 text-xs">直近30日 操作数</div><div class="text-3xl font-bold">{activity_30d}</div></div>
  <div class="p-4 rounded bg-slate-800/40"><div class="text-slate-400 text-xs">直近30日 企業閲覧数</div><div class="text-3xl font-bold">{company_views_30d}</div></div>
</div>"#
        )
    };

    // ログイン履歴
    let mut session_rows = String::new();
    for s in sessions {
        session_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-1 px-2 text-xs">{started}</td><td class="py-1 px-2">{success}</td><td class="py-1 px-2 text-xs">{method}</td><td class="py-1 px-2 text-xs text-slate-500">{ip_hash}</td><td class="py-1 px-2 text-xs text-slate-500">{ua}</td><td class="py-1 px-2 text-xs text-red-400">{reason}</td></tr>"#,
            started = escape_html(&s.started_at),
            success = if s.success == 1 { "<span class=\"text-green-400\">成功</span>" } else { "<span class=\"text-red-400\">失敗</span>" },
            method = escape_html(&s.login_method),
            ip_hash = escape_html(&s.ip_hash),
            ua = escape_html(&s.user_agent.chars().take(40).collect::<String>()),
            reason = escape_html(&s.failure_reason),
        ));
    }

    // 操作履歴
    let mut activity_rows = String::new();
    for a in activities {
        activity_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-1 px-2 text-xs">{at}</td><td class="py-1 px-2">{event}</td><td class="py-1 px-2 text-xs text-slate-400">{ttype}</td><td class="py-1 px-2 text-xs">{tid}</td></tr>"#,
            at = escape_html(&a.at),
            event = escape_html(&a.event_type),
            ttype = escape_html(&a.target_type),
            tid = escape_html(&a.target_id),
        ));
    }

    let body = format!(
        r#"{profile}
{kpis}
<section class="mb-6">
  <h3 class="text-lg font-bold mb-2">ログイン履歴 ({n_sessions} 件)</h3>
  <div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
    <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
      <th class="py-2 px-2 text-left">日時</th><th class="py-2 px-2 text-left">結果</th>
      <th class="py-2 px-2 text-left">方式</th><th class="py-2 px-2 text-left">IPハッシュ</th>
      <th class="py-2 px-2 text-left">User-Agent</th><th class="py-2 px-2 text-left">失敗理由</th>
    </tr></thead>
    <tbody>{session_rows}</tbody>
  </table></div>
</section>
<section>
  <h3 class="text-lg font-bold mb-2">操作履歴 ({n_activities} 件)</h3>
  <div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
    <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
      <th class="py-2 px-2 text-left">日時</th><th class="py-2 px-2 text-left">イベント</th>
      <th class="py-2 px-2 text-left">対象種別</th><th class="py-2 px-2 text-left">対象ID</th>
    </tr></thead>
    <tbody>{activity_rows}</tbody>
  </table></div>
</section>"#,
        n_sessions = sessions.len(),
        n_activities = activities.len(),
    );
    layout(&format!("{} - 詳細", acc.email), &body)
}

pub fn login_failures_page(failures: &[LoginSessionRow]) -> String {
    let mut rows = String::new();
    for f in failures {
        rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-1 px-2 text-xs">{at}</td><td class="py-1 px-2">{email}</td><td class="py-1 px-2 text-red-400 text-xs">{reason}</td><td class="py-1 px-2 text-xs text-slate-500">{ip}</td><td class="py-1 px-2 text-xs text-slate-500">{ua}</td></tr>"#,
            at = escape_html(&f.started_at),
            email = escape_html(&f.attempted_email),
            reason = escape_html(&f.failure_reason),
            ip = escape_html(&f.ip_hash),
            ua = escape_html(&f.user_agent.chars().take(40).collect::<String>()),
        ));
    }
    let body = format!(
        r#"<h1 class="text-2xl font-bold mb-4">ログイン失敗ログ ({} 件)</h1>
<p class="text-slate-400 text-sm mb-4">直近の失敗のみを表示。同一 ip_hash の連続失敗は不正アクセスの可能性があるため確認してください。</p>
<div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
  <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
    <th class="py-2 px-2 text-left">日時</th><th class="py-2 px-2 text-left">試行メール</th>
    <th class="py-2 px-2 text-left">失敗理由</th><th class="py-2 px-2 text-left">IPハッシュ</th>
    <th class="py-2 px-2 text-left">User-Agent</th>
  </tr></thead><tbody>{rows}</tbody>
</table></div>"#,
        failures.len()
    );
    layout("ログイン失敗 - 管理", &body)
}

// ============================================================================
// 利用状況 (2026-08-10 追加)
// ============================================================================

/// 機能コード → 画面に出す日本語名。
///
/// 未知のコードはそのまま表示する（新しい記録を足したときに黙って消えないように）。
fn event_label(event_type: &str, target_id: &str) -> String {
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

pub fn usage_page(
    days: i64,
    by_event: &[crate::audit::dao::UsageRow],
    by_account: &[crate::audit::dao::UsageRow],
    cross: &[crate::audit::dao::UsageRow],
) -> String {
    let period_links = [7_i64, 30, 90]
        .iter()
        .map(|d| {
            let cls = if *d == days {
                "px-3 py-1 rounded bg-blue-700 text-white text-xs"
            } else {
                "px-3 py-1 rounded bg-slate-700 text-slate-300 text-xs hover:bg-slate-600"
            };
            format!(r#"<a href="/admin/usage?days={d}" class="{cls}">直近{d}日</a>"#)
        })
        .collect::<Vec<_>>()
        .join(" ");

    let mut event_rows = String::new();
    for r in by_event {
        event_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-2 px-3">{name}</td><td class="py-2 px-3 text-right text-emerald-400">{cnt}</td><td class="py-2 px-3 text-slate-400 text-xs">{last}</td></tr>"#,
            name = escape_html(&event_label(&r.event_type, &r.target_id)),
            cnt = r.count,
            last = escape_html(&r.last_at),
        ));
    }
    if event_rows.is_empty() {
        event_rows.push_str(r#"<tr><td colspan="3" class="py-4 px-3 text-slate-500">この期間の記録はまだありません。</td></tr>"#);
    }

    let mut account_rows = String::new();
    for r in by_account {
        account_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-2 px-3"><a class="text-blue-400 hover:underline" href="/admin/users/{id}">{email}</a></td><td class="py-2 px-3 text-right text-emerald-400">{cnt}</td><td class="py-2 px-3 text-slate-400 text-xs">{last}</td></tr>"#,
            id = escape_html(&r.account_id),
            email = escape_html(if r.email.is_empty() { "(不明)" } else { &r.email }),
            cnt = r.count,
            last = escape_html(&r.last_at),
        ));
    }
    if account_rows.is_empty() {
        account_rows.push_str(r#"<tr><td colspan="3" class="py-4 px-3 text-slate-500">この期間の記録はまだありません。</td></tr>"#);
    }

    let mut cross_rows = String::new();
    for r in cross {
        cross_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-2 px-3">{email}</td><td class="py-2 px-3">{name}</td><td class="py-2 px-3 text-right text-emerald-400">{cnt}</td><td class="py-2 px-3 text-slate-400 text-xs">{last}</td></tr>"#,
            email = escape_html(if r.email.is_empty() { "(不明)" } else { &r.email }),
            name = escape_html(&event_label(&r.event_type, &r.target_id)),
            cnt = r.count,
            last = escape_html(&r.last_at),
        ));
    }
    if cross_rows.is_empty() {
        cross_rows.push_str(r#"<tr><td colspan="4" class="py-4 px-3 text-slate-500">この期間の記録はまだありません。</td></tr>"#);
    }

    let body = format!(
        r#"<h1 class="text-2xl font-bold mb-1">利用状況</h1>
<p class="text-slate-400 text-sm mb-4">誰が・どの機能を・どれだけ使ったかの集計です。記録しているのはタブ切替・検索実行・レポート生成・CSV取込などの操作で、入力途中の絞り込みや画面の再描画は含みません。</p>
<div class="mb-5 flex items-center gap-2">{period_links}</div>

<div class="grid grid-cols-1 lg:grid-cols-2 gap-5 mb-6">
  <div>
    <h2 class="text-sm font-semibold text-slate-200 mb-2">機能別</h2>
    <div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
      <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
        <th class="py-2 px-3 text-left">機能</th><th class="py-2 px-3 text-right">回数</th><th class="py-2 px-3 text-left">最終利用</th>
      </tr></thead><tbody>{event_rows}</tbody>
    </table></div>
  </div>
  <div>
    <h2 class="text-sm font-semibold text-slate-200 mb-2">ユーザー別</h2>
    <div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
      <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
        <th class="py-2 px-3 text-left">ユーザー</th><th class="py-2 px-3 text-right">操作回数</th><th class="py-2 px-3 text-left">最終利用</th>
      </tr></thead><tbody>{account_rows}</tbody>
    </table></div>
  </div>
</div>

<h2 class="text-sm font-semibold text-slate-200 mb-2">ユーザー × 機能（上位100件）</h2>
<div class="overflow-x-auto rounded bg-slate-800/30"><table class="w-full text-sm">
  <thead class="bg-slate-800 text-xs uppercase text-slate-300"><tr>
    <th class="py-2 px-3 text-left">ユーザー</th><th class="py-2 px-3 text-left">機能</th>
    <th class="py-2 px-3 text-right">回数</th><th class="py-2 px-3 text-left">最終利用</th>
  </tr></thead><tbody>{cross_rows}</tbody>
</table></div>
<p class="text-slate-600 text-xs mt-4">日時は協定世界時(UTC)です。ログは1年で自動削除されます。</p>"#
    );
    layout("利用状況 - 管理", &body)
}

#[cfg(test)]
mod usage_render_tests {
    use super::*;
    use crate::audit::dao::UsageRow;

    fn row(email: &str, ev: &str, cnt: i64) -> UsageRow {
        row_t(email, ev, "", cnt)
    }

    fn row_t(email: &str, ev: &str, target: &str, cnt: i64) -> UsageRow {
        UsageRow {
            account_id: "acc-1".to_string(),
            email: email.to_string(),
            event_type: ev.to_string(),
            target_id: target.to_string(),
            count: cnt,
            last_at: "2026-08-10T09:00:00Z".to_string(),
        }
    }

    /// 集計値がそのまま画面に出ること（要素の存在だけでなく実数を検証）
    #[test]
    fn usage_page_shows_counts_and_japanese_labels() {
        let by_event = vec![row("", "view_tab", 42), row("", "upload_survey_csv", 7)];
        let by_account = vec![row("a@f-a-c.co.jp", "", 49)];
        let cross = vec![row("a@f-a-c.co.jp", "view_tab", 42)];
        let html = usage_page(30, &by_event, &by_account, &cross);

        assert!(html.contains(">42<"), "機能別の回数 42 が表示されること");
        assert!(
            html.contains(">49<"),
            "ユーザー別の回数 49 が表示されること"
        );
        assert!(
            html.contains("a@f-a-c.co.jp"),
            "ユーザーのメールが表示されること"
        );
        assert!(
            html.contains("CSV取込"),
            "内部コードでなく日本語名で表示されること"
        );
        assert!(
            !html.contains("upload_survey_csv"),
            "内部イベントコードを画面に出さない"
        );
        // 期間切替リンク
        for d in ["days=7", "days=30", "days=90"] {
            assert!(html.contains(d), "{d} の切替リンクが必要");
        }
    }

    /// 逆証明: タブ閲覧は「どのタブか」まで出さないと集計の意味がない。
    /// 2026-08-10 の本番確認で「タブを開く:」と行き先が空のまま出ていた回帰。
    #[test]
    fn tab_views_are_broken_down_by_tab() {
        let by_event = vec![
            row_t("", "view_tab", "/tab/survey", 12),
            row_t("", "view_tab", "/tab/company", 5),
        ];
        let html = usage_page(30, &by_event, &[], &[]);
        assert!(html.contains("タブを開く: 媒体分析"), "どのタブか出ること");
        assert!(
            html.contains("タブを開く: 企業検索"),
            "タブごとに行が分かれること"
        );
        assert!(
            !html.contains("タブを開く:<"),
            "行き先が空のまま出てはいけない"
        );
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

    /// 記録が 0 件のときに空表ではなく説明を出す
    #[test]
    fn empty_usage_shows_explanation_not_blank_table() {
        let html = usage_page(7, &[], &[], &[]);
        assert!(
            html.contains("この期間の記録はまだありません"),
            "0 件のとき空表にしない"
        );
    }
}
