// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { FIXTURE_ALL, FIXTURE_MONTH_ALL } from '../crm/metricsModel';
import { CallersPanel, MonthCallersTable } from './CallersPanel';

afterEach(() => { cleanup(); });

const ok = <T,>(data: T) => () => Promise.resolve({ ok: true as const, data });
const names = () => within(screen.getByTestId('month-table')).getAllByRole('row').slice(1).map(r => r.querySelector('td')?.textContent);

describe('管理者の表: 今日 / 今月の切り替え', () => {
  it('既定は今日。今月に切り替えると今月の表を読む', async () => {
    const month = vi.fn(ok(FIXTURE_MONTH_ALL));
    render(<CallersPanel fetcher={ok(FIXTURE_ALL)} monthFetcher={month} />);
    expect(await screen.findByTestId('callers-table')).toBeTruthy();
    expect(month).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '今月' }));
    expect(await screen.findByTestId('month-table')).toBeTruthy();
    expect(screen.queryByTestId('callers-table')).toBeNull();
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('今月の架電成績（新規営業）');
    expect(screen.getByRole('button', { name: '今月' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: '今日' }));
    expect(await screen.findByTestId('callers-table')).toBeTruthy();
  });
});

describe('今月の表', () => {
  it('全項目を出す。30/90/360 秒の列は無い', () => {
    render(<MonthCallersTable data={FIXTURE_MONTH_ALL} />);
    const row = within(screen.getByTestId('month-table')).getByText('架空 一郎').closest('tr');
    // 架電812・今日52・合計10:15:00・平均2:03・アポ9
    expect(Array.from(row?.querySelectorAll('td') ?? []).map(td => td.textContent)).toEqual(['架空 一郎', '812', '52', '10:15:00', '2:03', '9']);
    expect(screen.queryByText(/90秒/)).toBeNull();
    expect(screen.getByTestId('month-count').textContent).toBe('5 人');
  });

  it('初期は架電数の多い順。見出しで並べ替える', () => {
    render(<MonthCallersTable data={FIXTURE_MONTH_ALL} />);
    expect(names()).toEqual(['架空 太郎', 'サンプル 花子', '架空 一郎', 'ダミー 次郎', '架空 三郎']);
    fireEvent.click(screen.getByRole('button', { name: /アポ/ }));
    expect(names()[0]).toBe('サンプル 花子');
    expect(screen.getByRole('columnheader', { name: /アポ/ }).getAttribute('aria-sort')).toBe('descending');
  });
});
