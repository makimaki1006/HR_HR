// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Fresh } from './parts';
import type { FreshMeta } from './types';

afterEach(cleanup);

const old: FreshMeta = {
  today: '2026-11-10',
  source_as_of: '2026-11-08 23:59:59',
  source_age_days: 2,
  last_dispatch_at: '2026-11-10T03:27:18.123+09:00',
  last_result: 'success',
};

describe('鮮度の帯の自動更新の案内', () => {
  it('2日前のデータでは依頼時刻をJSTで表示し、データの時刻はそのまま出す', () => {
    render(<Fresh meta={old} />);
    expect(screen.getByText('自動で更新を頼みました（03:27）')).toBeTruthy();
    expect(screen.getByText('2日前のデータです（2026-11-08 23:59）。')).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/dispatch|API|トークン/);
  });

  it.each(['failure', null, undefined] as const)('結果が %s なら依頼済みとは表示しない', (last_result) => {
    const meta = { ...old };
    if (last_result === undefined) delete meta.last_result;
    else meta.last_result = last_result;
    render(<Fresh meta={meta} />);
    expect(screen.getByText(last_result === 'failure' ? '自動の更新を頼めていません' : '自動の更新はこのあと頼みます')).toBeTruthy();
    expect(screen.queryByText(/自動で更新を頼みました/)).toBeNull();
  });

  it('前日の成功を今日の依頼に見せず、JSTの日付が変わると表示を変える', () => {
    render(<Fresh meta={{ ...old, today: '2026-11-11' }} />);
    expect(screen.getByText('自動の更新はこのあと頼みます')).toBeTruthy();
    expect(screen.queryByText(/自動で更新を頼みました/)).toBeNull();
  });

  it.each([['running', '自動でデータを更新しています'], ['failed', '自動の更新に失敗しました。次の更新を待っています']] as const)('実行の状態 %s を1行で出す', (latest_run_state, text) => {
    render(<Fresh meta={{ ...old, latest_run_state }} />);
    expect(screen.getByText(text)).toBeTruthy();
    expect(screen.queryByText(/自動で更新を頼みました/)).toBeNull();
  });

  it.each([0, 1, null])('古さが %s 日なら自動更新の行を出さない', (source_age_days) => {
    render(<Fresh meta={{ ...old, source_age_days }} />);
    expect(screen.queryByText(/自動で更新を頼みました|自動の更新を頼めていません|自動の更新はこのあと頼みます|自動でデータを更新しています|自動の更新に失敗しました/)).toBeNull();
  });

  it('成功でも時刻が読めなければ成功時刻を作らない', () => {
    render(<Fresh meta={{ ...old, last_dispatch_at: '<script>alert(1)</script>' }} />);
    expect(screen.getByText('自動の更新はこのあと頼みます')).toBeTruthy();
    expect(document.querySelector('script')).toBeNull();
  });
});
