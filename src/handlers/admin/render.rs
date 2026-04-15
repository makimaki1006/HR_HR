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
