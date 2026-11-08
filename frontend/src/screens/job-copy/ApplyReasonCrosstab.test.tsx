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
  it('1 行の文言と注記', () => {
    render(<ApplyReasonCrosstab />);
    expect(demoReport.rows).toHaveLength(72);
    expect(screen.getByText('応募動機を聞けた応募 72 件／全応募 13,454 件（0.5%）')).toBeTruthy();
    for (const t of ['一次対応のヒアリングで選んだ値を、そのまま数えています。', '応募動機は複数選べるので、1 件の応募が複数の列に入ることがあります。', '地域・職種と理由の重なりは、理由の原因を示すとは限りません。']) expect(screen.getByText(t)).toBeTruthy();
  });
  it('列は HubSpot の選択肢 13 個', () => {
    render(<ApplyReasonCrosstab />);
    const table = screen.getByRole('table', { name: '地域別の応募動機' });
    const heads = within(table).getAllByRole('columnheader').map(h => h.textContent);
    expect(heads).toHaveLength(15);
    expect(heads[1]).toBe('給与／手当');
    expect(heads[13]).toBe('回答なし');
    expect(heads[14]).toBe('応募動機を聞けた応募');
  });
  it('外した件数の文がモデルの値と一致する', () => {
    render(<ApplyReasonCrosstab />);
    const c = buildCrosstab(demoReport, { months: 6, kind: 'motive', area: 'applicant', occupation: null });
    const text = document.querySelector('.ar-excluded')?.textContent ?? '';
    expect(text).toContain(`地域が読めない ${String(c.excluded.noArea)} 件`);
    expect(text).toContain(`職種が無い ${String(c.excluded.occupationMissing)}・対応表に無い ${String(c.excluded.occupationUnmatched)}・求人票が食い違う ${String(c.excluded.occupationConflict)}`);
    expect(c.excluded.noArea).toBeGreaterThan(0);
  });
  it('都道府県の行を開くと市区町村、内容は開くまで出ない', () => {
    render(<ApplyReasonCrosstab />);
    const table = screen.getByRole('table', { name: '地域別の応募動機' });
    const first = head1(within(table).getAllByRole('button', { name: /の市区町村を開く/ }));
    fireEvent.click(first);
    expect(within(table).getAllByRole('row').length).toBeGreaterThan(within(table).getAllByRole('button', { name: /市区町村を/ }).length + 1);
    expect(document.querySelectorAll('.ar-texts li')).toHaveLength(0);
  });
  it('期間と表の切り替えは URL に入る', () => {
    render(<ApplyReasonCrosstab />);
    fireEvent.change(screen.getByLabelText('期間'), { target: { value: '1' } });
    expect(window.location.search).toContain('period=1');
    fireEvent.click(screen.getByRole('button', { name: '勤務条件の優先順位' }));
    expect(window.location.search).toContain('reason=priority');
  });
});

describe('ApplyReasonCrosstab (API の応答)', () => {
  const mockApi = (data: unknown) => vi.spyOn(client, 'apiGet').mockResolvedValue({ ok: true, status: 200, data } as never);
  it('期間 1 か月・求人の勤務地: 具体値', async () => {
    open('view=reasons&period=1&area=job'); mockApi(applyReasonFixture);
    render(<ApplyReasonCrosstab />);
    expect(await screen.findByText('応募動機を聞けた応募 4 件／全応募 2,914 件（0.1%）')).toBeTruthy();
    const table = screen.getByRole('table', { name: '地域別の応募動機' });
    const firstRow = head1(within(table).getAllByRole('row').slice(1));
    expect(within(firstRow).getByText('神奈川県')).toBeTruthy();
    expect(firstRow.querySelectorAll('td')[1]?.textContent).toBe('2');
    expect(document.querySelector('.ar-excluded')?.textContent).toContain('地域が読めない 0 件');
  });
  it('優先順位の表・6 か月: 記録の始まりの注記', async () => {
    open('view=reasons&reason=priority'); mockApi(applyReasonFixture);
    render(<ApplyReasonCrosstab />);
    expect(await screen.findByText('勤務条件の優先順位を聞けた応募 3 件／全応募 13,454 件（0.0%）')).toBeTruthy();
    expect(screen.getByText('選択式の記録は 2026/09/22 の応募からです。それより前の応募には値がありません。')).toBeTruthy();
    expect(screen.getByRole('table', { name: '地域別の勤務条件の優先順位' })).toBeTruthy();
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
