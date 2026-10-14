// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FIXTURE_ALL } from '../crm/metricsModel';
import { CallersPanel, CallersTable } from './CallersPanel';

afterEach(() => { cleanup(); vi.useRealTimers(); });

const firstColumn = () => within(screen.getByTestId('callers-table')).getAllByRole('row').slice(1).map(r => r.querySelector('td')?.textContent);

describe('今日の架電成績（全員）', () => {
  it('全項目を出す', () => {
    render(<CallersTable data={FIXTURE_ALL} />);
    const row = within(screen.getByTestId('callers-table')).getByText('架空 花子').closest('tr');
    // 架電38・着信5・30秒以上20・90秒以上12・360秒以上3・割合31.6%・合計1:09:00・平均3:27・アポ3・最初9:20・最後13:58
    expect(Array.from(row?.querySelectorAll('td') ?? []).map(td => td.textContent)).toEqual(
      ['架空 花子', '38', '5', '20', '12', '3', '31.6%', '1:09:00', '3:27', '3', '9:20', '13:58', '架空BPOチーム']);
    expect(screen.getByText(/14時05分時点/)).toBeTruthy();
  });

  it('見出しで並べ替える (初期は架電数の多い順、再クリックで逆順)', () => {
    render(<CallersTable data={FIXTURE_ALL} />);
    expect(firstColumn()).toEqual(['サンプル 二郎', '架空 一郎', '架空 花子', 'ダミー 三郎', '架空 四郎']);
    fireEvent.click(screen.getByRole('button', { name: /アポ/ }));
    expect(firstColumn()[0]).toBe('架空 花子');
    expect(screen.getByRole('columnheader', { name: /アポ/ }).getAttribute('aria-sort')).toBe('descending');
    fireEvent.click(screen.getByRole('button', { name: /アポ/ }));
    expect(screen.getByRole('columnheader', { name: /アポ/ }).getAttribute('aria-sort')).toBe('ascending');
    expect(firstColumn().slice(-1)).toEqual(['架空 花子']);
  });

  it('チームで絞る', () => {
    render(<CallersTable data={FIXTURE_ALL} />);
    fireEvent.change(screen.getByLabelText('チームで絞り込む'), { target: { value: '架空BPOチーム' } });
    expect(screen.getByTestId('callers-count').textContent).toBe('3 人');
    expect(firstColumn()).toEqual(['サンプル 二郎', '架空 一郎', '架空 花子']);
  });

  it('権限が無いときは読み込めない理由を出す', async () => {
    const { ApiHttpError } = await import('../../api/client');
    render(<CallersPanel fetcher={() => Promise.resolve({ ok: false, error: new ApiHttpError(403) })} />);
    expect((await screen.findByRole('alert')).textContent).toContain('権限がありません');
  });

  it('集計中は表を出さず、取り直して表が来たら出す', async () => {
    vi.useFakeTimers();
    const loading = { ...FIXTURE_ALL, loading: true, as_of: '', callers: [], teams: [] };
    const f = vi.fn()
      .mockResolvedValueOnce({ ok: true as const, data: loading })
      .mockResolvedValue({ ok: true as const, data: FIXTURE_ALL });
    render(<CallersPanel fetcher={f} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(10); });
    expect(screen.getByTestId('callers-warming')).toBeTruthy();
    expect(screen.queryByTestId('admin-error')).toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(13_000); });
    expect(f).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('callers-table')).toBeTruthy();
    expect(screen.queryByTestId('callers-warming')).toBeNull();
  });
});
