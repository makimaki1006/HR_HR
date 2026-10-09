import test from 'node:test';
import assert from 'node:assert/strict';
import {
  validateDomain, parseBlocklist, isAppTab, buildRules, resolveConfig, normalizeOrigins,
  excludedDomains, extraMatchPatterns, DEFAULT_APP_ORIGINS, DEFAULT_BLOCKLIST,
} from '../lib/logic.js';

test('validateDomain: 正常系は小文字化・trim', () => {
  assert.deepEqual(validateDomain('  Example.COM '), { ok: true, domain: 'example.com' });
  assert.deepEqual(validateDomain('a-b.co.jp'), { ok: true, domain: 'a-b.co.jp' });
  assert.equal(validateDomain('blocked.test').ok, true);
});

test('validateDomain: 不正を理由付きで拒否', () => {
  for (const bad of ['', 'https://example.com', 'example.com/path', '*.example.com', 'example.com:8080',
    'localhost', '127.0.0.1', 'exa mple.com', '-a.com', 'a-.com', 'a..com', 'user@example.com', 'example.com?x=1']) {
    const r = validateDomain(bad);
    assert.equal(r.ok, false, bad);
    assert.ok(r.error, bad);
  }
});

test('parseBlocklist: 空行・コメント無視、重複除去、行番号付きエラー', () => {
  const r = parseBlocklist('# memo\nA.com\n\na.com\nhttps://b.com\nc.com/x\nd.org\n');
  assert.deepEqual(r.domains, ['a.com', 'd.org']);
  assert.deepEqual(r.errors.map((e) => e.line), [5, 6]);
});

test('parseBlocklist: CRLF', () => {
  assert.deepEqual(parseBlocklist('a.com\r\nb.com').domains, ['a.com', 'b.com']);
});

test('isAppTab: オリジンとパスの境界', () => {
  const o = DEFAULT_APP_ORIGINS;
  assert.equal(isAppTab('https://hr-hw.onrender.com/app/crm', o), true);
  assert.equal(isAppTab('https://hr-hw.onrender.com/app/crm/queue?x=1#a', o), true);
  assert.equal(isAppTab('http://127.0.0.1:9216/app/crm', o), true);
  assert.equal(isAppTab('http://localhost:9216/app/crm/', o), true);
  assert.equal(isAppTab('https://hr-hw.onrender.com/app/crmx', o), false);
  assert.equal(isAppTab('https://hr-hw.onrender.com/app/other', o), false);
  assert.equal(isAppTab('http://hr-hw.onrender.com/app/crm', o), false); // スキーム違い
  assert.equal(isAppTab('http://localhost:3000/app/crm', o), false); // ポート違い
  assert.equal(isAppTab('https://hr-hw.onrender.com.evil.com/app/crm', o), false);
  assert.equal(isAppTab('https://evil.com/?https://hr-hw.onrender.com/app/crm', o), false);
  assert.equal(isAppTab(undefined, o), false);
  assert.equal(isAppTab('not a url', o), false);
});

test('buildRules: タブが無ければルール無し', () => {
  assert.deepEqual(buildRules([], DEFAULT_BLOCKLIST), []);
});

test('buildRules: tabIds 昇順・重複除去、sub_frame のみ、ヘッダ 2 本を remove', () => {
  const [rule] = buildRules([7, 3, 7], ['a.com']);
  assert.equal(rule.action.type, 'modifyHeaders');
  assert.deepEqual(rule.condition.tabIds, [3, 7]);
  assert.deepEqual(rule.condition.resourceTypes, ['sub_frame']);
  assert.deepEqual(rule.action.responseHeaders,
    [{ header: 'x-frame-options', operation: 'remove' }, { header: 'content-security-policy', operation: 'remove' }]);
  assert.equal(rule.condition.initiatorDomains, undefined); // initiator には依存しない
});

test('buildRules: 除外にブロックリストとアプリ自身のホストが入る', () => {
  const [rule] = buildRules([1], ['a.com']);
  assert.deepEqual(rule.condition.excludedRequestDomains,
    ['a.com', 'hr-hw.onrender.com', 'localhost', '127.0.0.1']);
  assert.deepEqual(excludedDomains(['a.com'], ['https://x.example:8443']), ['a.com', 'x.example']);
});

test('既定ブロックリストに必須ドメインが入っている', () => {
  for (const d of ['zoom.us', 'hubspot.com', 'accounts.google.com', 'stripe.com']) {
    assert.ok(DEFAULT_BLOCKLIST.includes(d), d);
  }
  assert.equal(DEFAULT_BLOCKLIST.length, 13);
});

test('resolveConfig: managed が sync を上書き、不正値は捨てる', () => {
  const c = resolveConfig({ managed: { blocklist: ['Managed.com', 'bad/url'] }, sync: { blocklist: ['sync.com'] } });
  assert.deepEqual(c.blocklist, ['managed.com']);
  assert.equal(c.managedBlocklist, true);
});

test('resolveConfig: managed 無しなら sync、sync 無しなら既定', () => {
  assert.deepEqual(resolveConfig({ managed: {}, sync: { blocklist: ['sync.com'] } }).blocklist, ['sync.com']);
  const d = resolveConfig({});
  assert.deepEqual(d.blocklist, DEFAULT_BLOCKLIST);
  assert.deepEqual(d.appOrigins, DEFAULT_APP_ORIGINS);
  assert.equal(d.managedBlocklist, false);
});

test('resolveConfig: sync の空配列は「ブロック無し」として尊重する', () => {
  assert.deepEqual(resolveConfig({ sync: { blocklist: [] } }).blocklist, []);
});

test('normalizeOrigins / extraMatchPatterns', () => {
  assert.deepEqual(normalizeOrigins(['https://a.example/path', 'ftp://x', 'junk', 'https://a.example']), ['https://a.example']);
  assert.deepEqual(resolveConfig({ managed: { appOrigins: ['junk'] } }).appOrigins, DEFAULT_APP_ORIGINS);
  assert.deepEqual(extraMatchPatterns(['https://hr-hw.onrender.com', 'https://stg.example']), ['https://stg.example/*']);
});
