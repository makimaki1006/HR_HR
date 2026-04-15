//! ユーザー自己サービス画面の HTML レンダリング

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
<style>body{{background:#0f172a;color:#e2e8f0;font-family:ui-sans-serif,system-ui;}}</style>
</head>
<body class="min-h-screen p-6">
<nav class="mb-6 flex items-center gap-4 text-sm">
  <a href="/" class="text-slate-400 hover:text-white">← ダッシュボード</a>
  <span class="text-slate-600">|</span>
  <a href="/my/profile" class="text-blue-400 hover:text-blue-300">プロフィール</a>
  <a href="/my/activity" class="text-blue-400 hover:text-blue-300">自分の履歴</a>
  <a href="/logout" class="text-slate-400 hover:text-white ml-auto">ログアウト</a>
</nav>
{body}
</body>
</html>"#,
        title = escape_html(title)
    )
}

pub fn audit_disabled_page() -> String {
    layout(
        "機能未有効",
        r#"<div class="p-8 rounded bg-amber-900/30 border border-amber-700">
           <h1 class="text-xl font-bold mb-2">この機能は現在ご利用いただけません</h1>
           <p class="text-slate-300">システム管理者が監査機能を有効化すると利用可能になります。</p>
        </div>"#,
    )
}

pub fn not_linked_page() -> String {
    layout(
        "アカウント未連携",
        r#"<div class="p-8 rounded bg-amber-900/30 border border-amber-700">
           <h1 class="text-xl font-bold mb-2">アカウントが見つかりません</h1>
           <p class="text-slate-300">一度 <a href="/logout" class="text-blue-400 underline">ログアウト</a> して再ログインしてください。</p>
        </div>"#,
    )
}

pub fn profile_page(acc: &AccountRow, flash: Option<&str>) -> String {
    let flash_html = flash
        .map(|m| {
            format!(
                r#"<div class="mb-4 p-3 rounded bg-green-900/30 border border-green-700 text-green-300 text-sm">{}</div>"#,
                escape_html(m)
            )
        })
        .unwrap_or_default();

    let body = format!(
        r#"{flash_html}
<h1 class="text-2xl font-bold mb-6">プロフィール</h1>

<div class="grid grid-cols-1 md:grid-cols-2 gap-6">
  <section class="p-4 rounded bg-slate-800/40">
    <h2 class="text-lg font-bold mb-3">基本情報（読み取り専用）</h2>
    <dl class="grid grid-cols-3 gap-2 text-sm">
      <dt class="text-slate-400">メール</dt><dd class="col-span-2">{email}</dd>
      <dt class="text-slate-400">権限</dt><dd class="col-span-2">{role}</dd>
      <dt class="text-slate-400">初回ログイン</dt><dd class="col-span-2 text-slate-400 text-xs">{first}</dd>
      <dt class="text-slate-400">最終ログイン</dt><dd class="col-span-2 text-slate-400 text-xs">{last}</dd>
      <dt class="text-slate-400">ログイン回数</dt><dd class="col-span-2">{count}</dd>
    </dl>
  </section>

  <section class="p-4 rounded bg-slate-800/40">
    <h2 class="text-lg font-bold mb-3">編集可能</h2>
    <form method="post" action="/my/profile" class="space-y-3">
      <label class="block">
        <span class="text-slate-400 text-sm">氏名</span>
        <input type="text" name="display_name" maxlength="80" value="{name}"
               class="mt-1 w-full px-3 py-2 bg-slate-900 border border-slate-600 rounded text-white focus:outline-none focus:ring-2 focus:ring-blue-500">
      </label>
      <label class="block">
        <span class="text-slate-400 text-sm">会社</span>
        <input type="text" name="company" maxlength="120" value="{company}"
               class="mt-1 w-full px-3 py-2 bg-slate-900 border border-slate-600 rounded text-white focus:outline-none focus:ring-2 focus:ring-blue-500">
      </label>
      <button type="submit" class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded">保存</button>
    </form>
  </section>
</div>"#,
        email = escape_html(&acc.email),
        role = escape_html(&acc.role),
        first = escape_html(&acc.first_seen_at),
        last = escape_html(&acc.last_login_at),
        count = acc.login_count,
        name = escape_html(&acc.display_name),
        company = escape_html(&acc.company),
    );
    layout("プロフィール", &body)
}

pub fn activity_page(
    acc: &AccountRow,
    sessions: &[LoginSessionRow],
    activities: &[ActivityLogRow],
) -> String {
    let mut session_rows = String::new();
    for s in sessions {
        session_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-1 px-2 text-xs">{at}</td><td class="py-1 px-2">{ok}</td><td class="py-1 px-2 text-xs text-slate-500">{ua}</td></tr>"#,
            at = escape_html(&s.started_at),
            ok = if s.success == 1 {
                "<span class=\"text-green-400\">成功</span>"
            } else {
                "<span class=\"text-red-400\">失敗</span>"
            },
            ua = escape_html(&s.user_agent.chars().take(50).collect::<String>()),
        ));
    }
    let mut activity_rows = String::new();
    for a in activities {
        activity_rows.push_str(&format!(
            r#"<tr class="border-b border-slate-700"><td class="py-1 px-2 text-xs">{at}</td><td class="py-1 px-2">{evt}</td><td class="py-1 px-2 text-xs text-slate-400">{tt}</td><td class="py-1 px-2 text-xs">{tid}</td></tr>"#,
            at = escape_html(&a.at),
            evt = escape_html(&a.event_type),
            tt = escape_html(&a.target_type),
            tid = escape_html(&a.target_id),
        ));
    }

    let body = format!(
        r#"<h1 class="text-2xl font-bold mb-2">{email}</h1>
<p class="text-slate-400 text-sm mb-6">ご自身の最近の利用履歴 (直近50ログイン / 直近100操作)</p>

<div class="grid grid-cols-1 md:grid-cols-2 gap-6">
  <section class="p-4 rounded bg-slate-800/40">
    <h2 class="text-lg font-bold mb-3">ログイン履歴</h2>
    <div class="overflow-x-auto"><table class="w-full text-sm">
      <thead class="text-xs uppercase text-slate-400"><tr>
        <th class="py-2 px-2 text-left">日時</th>
        <th class="py-2 px-2 text-left">結果</th>
        <th class="py-2 px-2 text-left">端末</th>
      </tr></thead><tbody>{session_rows}</tbody>
    </table></div>
  </section>

  <section class="p-4 rounded bg-slate-800/40">
    <h2 class="text-lg font-bold mb-3">操作履歴</h2>
    <div class="overflow-x-auto"><table class="w-full text-sm">
      <thead class="text-xs uppercase text-slate-400"><tr>
        <th class="py-2 px-2 text-left">日時</th>
        <th class="py-2 px-2 text-left">操作</th>
        <th class="py-2 px-2 text-left">種別</th>
        <th class="py-2 px-2 text-left">対象</th>
      </tr></thead><tbody>{activity_rows}</tbody>
    </table></div>
  </section>
</div>"#,
        email = escape_html(&acc.email)
    );
    layout("自分の履歴", &body)
}
