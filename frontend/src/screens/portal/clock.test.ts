import { afterEach, expect, it, vi } from 'vitest';
import { portalNow, portalYear, setPortalPreviewClock, TODAY_NUM } from './clock';
import { dayNum, formatDayJa } from './columns';

afterEach(() => { setPortalPreviewClock(null); vi.useRealTimers(); });

it('実画面の既定は実時刻で、日本時間で年と今日を決める', () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2027-12-31T15:30:00Z'));
  setPortalPreviewClock(null);
  expect(portalNow()).toBe(Date.now());
  expect(portalYear()).toBe(2028);
  expect(TODAY_NUM).toBe(dayNum(2028, 1, 1));
  expect(formatDayJa('2028/1/1')?.isToday).toBe(true);
});

it('試作だけが指定の日時を使う', () => {
  setPortalPreviewClock('2026-10-20T14:32:00+09:00');
  expect(formatDayJa('2026/10/20')?.isToday).toBe(true);
  expect(formatDayJa('2025/10/20')?.text).toBe('2025/10/20（月）');
});
