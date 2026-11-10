// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Fresh, autoRefreshText } from './parts';
import type { FreshMeta } from './types';

afterEach(cleanup);

const old: FreshMeta = {
  today: '2026-11-10',
  source_as_of: '2026-11-08 23:59:59',
  source_age_days: 2,
  auto_refresh: true,
  last_dispatch_at: '2026-11-10T03:27:18.123+09:00',
  last_result: 'success',
  auto_state: 'requested',
};

const ANY_AUTO = /自動で更新を頼みました|自動の更新を頼めていません|自動の更新はこのあと頼みます|自動でデータを更新しています|自動の更新に失敗しました|更新は済んでいます|自動の更新の状態|1時以降に頼みます/;

describe('鮮度の帯の自動更新の案内', () => {
  it('依頼時刻をJSTで「ほかの時刻」の中に出し、帯の1行目は変えない', () => {
    render(<Fresh meta={old} />);
    const text = screen.getByText('自動で更新を頼みました（03:27）。');
    expect(text.closest('details')).not.toBeNull();
    expect(screen.getByText('2日前のデータです（2026-11-08 23:59）。')).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/dispatch|API|トークン/);
  });

  it.each([
    ['running', '自動でデータを更新しています'],
    ['done', '更新は済んでいます。まもなく表示に反映されます'],
    ['failed', '自動の更新に失敗しました。次の更新を待っています'],
    ['unknown', '自動の更新の状態を確かめられません'],
    ['tomorrow', '自動の更新は明日1時以降に頼みます'],
    ['after_one', '自動の更新は1時以降に頼みます'],
    ['request_failed', '自動の更新を頼めていません'],
    ['pending', '自動の更新はこのあと頼みます'],
  ] as const)('状態 %s の文言', (auto_state, text) => {
    expect(autoRefreshText({ ...old, auto_state }, 2)).toBe(text);
    render(<Fresh meta={{ ...old, auto_state }} />);
    expect(screen.getByText(`${text}。`)).toBeTruthy();
  });

  it('昨日のデータ（age 1）でも見張りの条件とそろえて案内する', () => {
    render(<Fresh meta={{ ...old, source_age_days: 1, source_as_of: '2026-11-09 21:30:00', auto_state: 'done' }} />);
    expect(screen.getByText('更新は済んでいます。まもなく表示に反映されます。')).toBeTruthy();
    expect(screen.getByText('昨日 11-09 21:30')).toBeTruthy();
  });

  it('前日の依頼・読めない時刻を今日の依頼に見せない', () => {
    expect(autoRefreshText({ ...old, today: '2026-11-11' }, 2)).toBe('自動の更新はこのあと頼みます');
    render(<Fresh meta={{ ...old, last_dispatch_at: '<script>alert(1)</script>' }} />);
    expect(screen.getByText('自動の更新はこのあと頼みます。')).toBeTruthy();
    expect(document.querySelector('script')).toBeNull();
  });

  it.each([0, null])('古さが %s なら自動更新の案内を出さない', (source_age_days) => {
    render(<Fresh meta={{ ...old, source_age_days }} />);
    expect(document.body.textContent).not.toMatch(ANY_AUTO);
  });

  it.each([{ auto_refresh: false }, { auto_state: null }, {}])('見張りが動いていなければ自動の依頼のことを出さない %o', (patch) => {
    const meta: FreshMeta = { ...old, ...patch };
    if (Object.keys(patch).length === 0) delete meta.auto_refresh;
    render(<Fresh meta={meta} />);
    expect(document.body.textContent).not.toMatch(ANY_AUTO);
  });
});
