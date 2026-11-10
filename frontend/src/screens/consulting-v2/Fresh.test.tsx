// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Fresh } from './parts';
import type { FreshMeta } from './types';

afterEach(cleanup);

const old: FreshMeta = {
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

  it.each(['failure', null, undefined] as const)('結果が %s なら頼めていないと表示する', (last_result) => {
    const meta = { ...old };
    if (last_result === undefined) delete meta.last_result;
    else meta.last_result = last_result;
    render(<Fresh meta={meta} />);
    expect(screen.getByText('自動の更新を頼めていません')).toBeTruthy();
    expect(screen.queryByText(/自動で更新を頼みました/)).toBeNull();
  });

  it.each([0, 1, null])('古さが %s 日なら自動更新の行を出さない', (source_age_days) => {
    render(<Fresh meta={{ ...old, source_age_days }} />);
    expect(screen.queryByText(/自動で更新を頼みました|自動の更新を頼めていません/)).toBeNull();
  });

  it('成功でも時刻が読めなければ成功時刻を作らない', () => {
    render(<Fresh meta={{ ...old, last_dispatch_at: '<script>alert(1)</script>' }} />);
    expect(screen.getByText('自動の更新を頼めていません')).toBeTruthy();
    expect(document.querySelector('script')).toBeNull();
  });
});
