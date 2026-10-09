// 純粋ロジック (chrome.* に依存しない)。node --test で検証する。

export const DEFAULT_APP_ORIGINS = [
  'https://hr-hw.onrender.com',
  'http://localhost:9216',
  'http://127.0.0.1:9216',
];

export const APP_PATH = '/app/crm';

export const DEFAULT_BLOCKLIST = [
  'accounts.google.com',
  'accounts.youtube.com',
  'login.microsoftonline.com',
  'login.live.com',
  'okta.com',
  'auth0.com',
  'appleid.apple.com',
  'paypal.com',
  'stripe.com',
];

export const RULE_ID = 1;

const LABEL = /^(?!-)[a-z0-9-]{1,63}(?<!-)$/;
const TLD = /^[a-z]{2,63}$/;

/** 1 行 → { ok, domain } | { ok:false, error } */
export function validateDomain(raw) {
  const s = String(raw).trim().toLowerCase();
  if (!s) return { ok: false, error: '空です' };
  if (/\s/.test(s)) return { ok: false, error: '空白を含められません' };
  if (s.includes('://')) return { ok: false, error: 'URL ではなくドメインだけを書いてください' };
  if (s.includes('*')) return { ok: false, error: 'ワイルドカードは使えません (サブドメインは自動で含まれます)' };
  if (/[/?#@]/.test(s)) return { ok: false, error: 'パス・クエリ・認証情報は書けません' };
  if (s.includes(':')) return { ok: false, error: 'ポート番号は書けません' };
  const labels = s.split('.');
  if (labels.length < 2) return { ok: false, error: 'ドメインの形式ではありません' };
  if (!labels.every((l) => LABEL.test(l))) return { ok: false, error: 'ドメインの形式ではありません' };
  if (!TLD.test(labels[labels.length - 1])) return { ok: false, error: 'ドメインの形式ではありません (IP アドレスは不可)' };
  return { ok: true, domain: s };
}

/** textarea の全文 → { domains (重複除去), errors:[{line,text,error}] }。空行と # 行は無視。 */
export function parseBlocklist(text) {
  const domains = [];
  const errors = [];
  String(text ?? '').split(/\r?\n/).forEach((line, i) => {
    const t = line.trim();
    if (!t || t.startsWith('#')) return;
    const r = validateDomain(t);
    if (!r.ok) errors.push({ line: i + 1, text: t, error: r.error });
    else if (!domains.includes(r.domain)) domains.push(r.domain);
  });
  return { domains, errors };
}

/** managed / sync / 既定値から有効な設定を決める。managed が優先。 */
export function resolveConfig({ managed, sync } = {}) {
  const m = managed || {};
  const s = sync || {};
  const managedBlocklist = Array.isArray(m.blocklist);
  const blocklistSource = managedBlocklist ? m.blocklist : Array.isArray(s.blocklist) ? s.blocklist : DEFAULT_BLOCKLIST;
  const blocklist = [];
  for (const d of blocklistSource) {
    const r = validateDomain(d);
    if (r.ok && !blocklist.includes(r.domain)) blocklist.push(r.domain);
  }
  return {
    blocklist,
    appOrigins: [...DEFAULT_APP_ORIGINS], // 固定 (manifest の content_scripts.matches と一致させる)
    managedBlocklist,
  };
}

/** タブのトップレベル URL が CRM 画面か。オリジン完全一致かつ /app/crm または /app/crm/ 配下。 */
export function isAppTab(url, origins) {
  if (!url) return false;
  let u;
  try { u = new URL(url); } catch { return false; }
  if (!origins.includes(u.origin)) return false;
  return u.pathname === APP_PATH || u.pathname.startsWith(APP_PATH + '/');
}

/** 除外ドメイン = ブロックリスト + アプリ自身のホスト名 (requestDomains はポートを見ない)。 */
export function excludedDomains(blocklist, appOrigins) {
  const out = [...blocklist];
  for (const o of appOrigins) {
    const h = new URL(o).hostname;
    if (!out.includes(h)) out.push(h);
  }
  return out;
}

/** DNR セッションルール。CRM タブが無ければ空配列 (= ルール無し)。 */
export function buildRules(tabIds, blocklist, appOrigins = DEFAULT_APP_ORIGINS) {
  const ids = [...new Set(tabIds)].sort((a, b) => a - b);
  if (ids.length === 0) return [];
  return [
    {
      id: RULE_ID,
      priority: 1,
      action: {
        type: 'modifyHeaders',
        responseHeaders: [
          { header: 'x-frame-options', operation: 'remove' },
          { header: 'content-security-policy', operation: 'remove' },
        ],
      },
      condition: {
        tabIds: ids,
        resourceTypes: ['sub_frame'],
        excludedRequestDomains: excludedDomains(blocklist, appOrigins),
      },
    },
  ];
}
