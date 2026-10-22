// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { ApplyReasonCrosstab } from './ApplyReasonCrosstab';
import { buildCrosstab } from './applyReasonCrosstabModel';
import { demoApplyReasons } from './demoApplyReasons';
import { applyReasonFixture } from './applyReason.testFixtures';
import * as client from '../../api/client';

const demoReport = demoApplyReasons.report ?? (() => { throw new Error('demo'); })();
function head1<T>(xs: T[]): T { const x = xs[0]; if (x === undefined) throw new Error('empty'); return x; }
function open(query: string) { window.history.replaceState({}, '', `/app/job-copy?${query}`); }
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
beforeEach(() => { open('view=reasons&demo=1'); });

describe('ApplyReasonCrosstab (デモデータ)', () => {
  it('1 行の文言と注記 4 つ', () => {
    render(<ApplyReasonCrosstab />);
    expect(demoReport.periods[2]?.with_reason).toBe(72);
    expect(screen.getByText('応募理由が入っている応募は 13,454 件中 72 件（0.5%）')).toBeTruthy();
    for (const t of ['HubSpot に記録された応募だけを数えています。', '地域・職種と理由の重なりは、理由の原因を示すとは限りません。', '0 件は記録が無かったこと、未取得は読めなかったことを表します。', '1 件の応募が複数の区分に入ることがあります。']) expect(screen.getByText(t)).toBeTruthy();
  });
  it('外した件数の文がモデルの値と一致する', () => {
    render(<ApplyReasonCrosstab />);
    const c = buildCrosstab(demoReport, { months: 6, area: 'applicant', occupation: null });
    const text = document.querySelector('.ar-excluded')?.textContent ?? '';
    expect(text).toContain(`地域が読めない ${String(c.excluded.noArea)} 件`);
    expect(text).toContain(`職種が無い ${String(c.excluded.occupationMissing)}・対応表に無い ${String(c.excluded.occupationUnmatched)}・求人票が食い違う ${String(c.excluded.occupationConflict)}`);
    expect(text).toContain(`理由を分類できない ${String(c.excluded.unclassified)} 件`);
    expect(c.excluded.noArea).toBeGreaterThan(0);
  });
  it('都道府県の行を開くと市区町村、文は開くまで出ない', () => {
    render(<ApplyReasonCrosstab />);
    const table = screen.getByRole('table', { name: '地域別の応募理由' });
    const first = head1(within(table).getAllByRole('button', { name: /の市区町村を開く/ }));
    expect(within(table).queryByText('市区町村不明')).toBeNull();
    fireEvent.click(first);
    expect(within(table).getAllByRole('row').length).toBeGreaterThan(within(table).getAllByRole('button', { name: /市区町村を/ }).length + 1);
    expect(document.querySelectorAll('.ar-texts li')).toHaveLength(0);
  });
  it('期間を変えると URL の period に入る', () => {
    render(<ApplyReasonCrosstab />);
    fireEvent.change(screen.getByLabelText('期間'), { target: { value: '1' } });
    expect(window.location.search).toContain('period=1');
  });
});

describe('ApplyReasonCrosstab (API の応答)', () => {
  const mockApi = (data: unknown) => vi.spyOn(client, 'apiGet').mockResolvedValue({ ok: true, status: 200, data } as never);
  it('期間 1 か月・求人の勤務地: 具体値', async () => {
    open('view=reasons&period=1&area=job'); mockApi(applyReasonFixture);
    render(<ApplyReasonCrosstab />);
    expect(await screen.findByText('応募理由が入っている応募は 2,914 件中 4 件（0.1%）')).toBeTruthy();
    const table = screen.getByRole('table', { name: '地域別の応募理由' });
    const firstRow = head1(within(table).getAllByRole('row').slice(1));
    expect(within(firstRow).getByText('神奈川県')).toBeTruthy();
    expect(firstRow.textContent).toContain('3');
    expect(document.querySelector('.ar-excluded')?.textContent).toContain('地域が読めない 0 件');
  });
  it('応募者の住所では地域が読めない 1 件', async () => {
    open('view=reasons&period=1'); mockApi(applyReasonFixture);
    render(<ApplyReasonCrosstab />);
    await screen.findByText('応募理由が入っている応募は 2,914 件中 4 件（0.1%）');
    expect(document.querySelector('.ar-excluded')?.textContent).toContain('地域が読めない 1 件');
  });
  it('準備中・失敗の表示', async () => {
    open('view=reasons'); mockApi({ status: 'preparing' });
    const { unmount } = render(<ApplyReasonCrosstab />);
    expect(await screen.findByText('集計を作っています。')).toBeTruthy();
    unmount(); cleanup(); vi.restoreAllMocks();
    mockApi({ status: 'failed' });
    render(<ApplyReasonCrosstab />);
    expect(await screen.findByText('集計を作れませんでした。')).toBeTruthy();
    expect(screen.getByRole('button', { name: '再読み込み' })).toBeTruthy();
  });
  it('古い集計には日時を添える', async () => {
    open('view=reasons'); mockApi({ ...applyReasonFixture, stale: true, refreshing: true });
    vi.spyOn(Date, 'now').mockReturnValue(new Date(String(applyReasonFixture.built_at)).getTime() + 60 * 60 * 1000);
    render(<ApplyReasonCrosstab />);
    expect(await screen.findByText(/時点の集計です。新しい集計を作っています。/)).toBeTruthy();
  });
});
