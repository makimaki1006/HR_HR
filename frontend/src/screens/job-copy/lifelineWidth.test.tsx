// @vitest-environment happy-dom
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { JobCopyRecord } from './data';
import { LifelineChart } from './ListingOverview';
import { columnWidth, fitsColumn, MIN_COLUMN_PX } from './lifelineModel';
import type { Lifeline, LifelineWeek } from './lifelineModel';
import { addDays } from './timelineModel';

const job: JobCopyRecord = { id: 'w', title: '架空の求人', company: '架空の顧客', media: 'HRハッカー', mediaJobId: '1234567', location: '大分県', dataSource: 'hubspot', versions: [] };
const week = (index: number, value: number | null): LifelineWeek => ({
  start: addDays('2026-01-05', index * 7), changes: [], billing: value, impressions: value, clicks: value, hubspotApplications: value, mediaApplications: value,
});
const lifeline = (count: number, value: number | null = 12): Lifeline => ({
  weeks: Array.from({ length: count }, (_, index) => week(index, value)),
  startIndex: 0, endIndex: null, todayIndex: count - 1, hasPerformance: true, mediaLabel: 'Indeed',
});

describe('列の幅の決まり', () => {
  it('見えている幅を週の数で割る。最小の幅より狭くしない。測れないときは最小の幅', () => {
    expect(columnWidth(1600, 2)).toBe(800);
    expect(columnWidth(1601, 3)).toBe(533);
    expect(columnWidth(1600, 40)).toBe(MIN_COLUMN_PX);
    expect(columnWidth(null, 2)).toBe(MIN_COLUMN_PX);
    expect(columnWidth(0, 2)).toBe(MIN_COLUMN_PX);
  });
  it('数が列に収まるかは文字の幅で決める', () => {
    expect(fitsColumn('¥987,654,321', 96)).toBe(true);
    expect(fitsColumn('¥98,765,432,100', 96)).toBe(false);
    expect(fitsColumn('¥98,765,432,100', 120)).toBe(true);
    expect(fitsColumn('1億', 30)).toBe(true);
    expect(fitsColumn('1億', 20)).toBe(false);
  });
});

describe('広げた表', () => {
  let visible = 0;
  let resize: (() => void)[] = [];
  beforeEach(() => {
    resize = [];
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const width = this.classList.contains('lf-scroll') ? visible : 0;
      return { width, height: 0, x: 0, y: 0, top: 0, left: 0, right: width, bottom: 0, toJSON: () => ({}) };
    });
    // 表の幅（見えていないときは 0）。最新の週へ寄せるときの位置になる。
    vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.classList.contains('lf-scroll') && visible > 0 ? parseFloat(this.querySelector<HTMLElement>('.lf-track')?.style.width ?? '0') : 0;
    });
    vi.stubGlobal('ResizeObserver', class { constructor(callback: () => void) { resize.push(callback); } observe() { /* テストでは手で呼ぶ */ } disconnect() { /* なし */ } });
  });
  afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
  const fireResize = () => { act(() => { for (const callback of resize) callback(); }); };
  const scroller = () => document.querySelector<HTMLElement>('.lf-scroll');
  const track = () => document.querySelector<HTMLElement>('.lf-track');

  it('2週の求人は、見えている幅を2で割った幅で横幅いっぱいに広げる', () => {
    visible = 1600;
    render(<LifelineChart lifeline={lifeline(2)} job={job} selectedWeek={1} onSelect={() => undefined} />);
    expect(track()?.style.getPropertyValue('--lf-col')).toBe('800px');
    expect(track()?.style.width).toBe('1600px');
    expect(screen.getByTestId('lifeline-selected').style.left).toBe('800px');
    expect(screen.getByTestId('lifeline-selected').style.width).toBe('800px');
    expect(document.querySelector<HTMLElement>('.lf-marker-start')?.style.left).toBe('0px');
    expect(document.querySelector<HTMLElement>('.lf-marker-today')?.style.left).toBe('800px');
  });
  it('40週の求人は最小の幅のまま横にスクロールする', () => {
    visible = 1600;
    render(<LifelineChart lifeline={lifeline(40)} job={job} selectedWeek={10} onSelect={() => undefined} />);
    expect(track()?.style.getPropertyValue('--lf-col')).toBe(`${String(MIN_COLUMN_PX)}px`);
    expect(track()?.style.width).toBe(`${String(40 * MIN_COLUMN_PX)}px`);
    expect(screen.getByTestId('lifeline-selected').style.left).toBe(`${String(10 * MIN_COLUMN_PX)}px`);
    expect(document.querySelector<HTMLElement>('.lf-marker-today')?.style.left).toBe(`${String(39 * MIN_COLUMN_PX)}px`);
  });
  it('列に収まる数は縮めずに出し、収まらないときだけ縮める', () => {
    visible = 1600;
    render(<LifelineChart lifeline={lifeline(2, 98765432100)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    const wide = screen.getByRole('row', { name: '課金（週）' }).querySelector('.lf-cell');
    expect(wide?.textContent).toBe('¥98,765,432,100');
    expect(wide?.getAttribute('title')).toBeNull();
    cleanup();
    render(<LifelineChart lifeline={lifeline(40, 98765432100)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    const narrow = screen.getByRole('row', { name: '課金（週）' }).querySelector('.lf-cell');
    expect(narrow?.textContent).toBe('¥988億');
    expect(narrow?.getAttribute('title')).toBe('¥98,765,432,100');
  });
  it('小数の幅（表示倍率 125% など）は切り捨て、表を見えている幅より広くしない', () => {
    visible = 1159.5;
    render(<LifelineChart lifeline={lifeline(2)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    expect(track()?.style.getPropertyValue('--lf-col')).toBe('579px');
    expect(parseFloat(track()?.style.width ?? '')).toBeLessThanOrEqual(1159.5);
    cleanup();
    visible = 1159.5;
    render(<LifelineChart lifeline={lifeline(7)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    expect(parseFloat(track()?.style.width ?? '')).toBeLessThanOrEqual(1159.5);
  });
  it('幅が変わっても、手で動かした位置を最新の週へ戻さない', () => {
    visible = 1600;
    render(<LifelineChart lifeline={lifeline(15)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    expect(track()?.style.getPropertyValue('--lf-col')).toBe('106px');
    const element = scroller();
    if (!element) throw new Error('表がありません');
    element.scrollLeft = 50;
    visible = 1200;
    fireResize();
    expect(track()?.style.getPropertyValue('--lf-col')).toBe(`${String(MIN_COLUMN_PX)}px`);
    expect(element.scrollLeft).toBe(50);
  });
  it('見えない状態で作られて表示されたら、最新の週が見える位置へ寄せる', () => {
    visible = 0;
    render(<LifelineChart lifeline={lifeline(40)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    const element = scroller();
    if (!element) throw new Error('表がありません');
    expect(element.scrollLeft).toBe(0);
    visible = 1160;
    fireResize();
    expect(track()?.style.getPropertyValue('--lf-col')).toBe(`${String(MIN_COLUMN_PX)}px`);
    expect(element.scrollLeft).toBe(40 * MIN_COLUMN_PX);
  });
  it('測れないときは最小の幅', () => {
    visible = 0;
    render(<LifelineChart lifeline={lifeline(2)} job={job} selectedWeek={null} onSelect={() => undefined} />);
    expect(track()?.style.getPropertyValue('--lf-col')).toBe(`${String(MIN_COLUMN_PX)}px`);
  });
});
