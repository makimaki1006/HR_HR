// @vitest-environment happy-dom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useApplicantPresence } from './ApplicantPresence';

let hidden = false;
const setHidden = (value: boolean) => { act(() => { hidden = value; document.dispatchEvent(new Event('visibilitychange')); }); };
const fetchMock = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ viewers: [] }), { status: 200 })));
/** 知らせた回数 (取り下げ = applicant: null は数えない)。 */
const beats = () => fetchMock.mock.calls.filter(call => (JSON.parse((call as unknown as [string, { body: string }])[1].body) as { applicant: string | null }).applicant !== null).length;
const leaves = () => fetchMock.mock.calls.length - beats();
const advance = (ms: number) => { act(() => { vi.advanceTimersByTime(ms); }); };

beforeEach(() => { vi.useFakeTimers(); vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden); vi.stubGlobal('fetch', fetchMock); });
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); fetchMock.mockClear(); hidden = false; });

it('表示中は 15 秒おきに知らせる', () => {
  renderHook(() => useApplicantPresence(false, '1', '09012345678'));
  expect(beats()).toBe(1);
  advance(60_000);
  expect(beats()).toBe(5);
});

it('隠れている 1 時間は 60 秒おき (60 回)、戻ったらすぐ 1 回知らせて 15 秒おきに戻る', () => {
  renderHook(() => useApplicantPresence(false, '1', '09012345678'));
  expect(beats()).toBe(1);
  setHidden(true);
  advance(59_999);
  expect(beats()).toBe(1);
  advance(1);
  expect(beats()).toBe(2);
  advance(3_600_000 - 60_000);
  expect(beats()).toBe(61);
  setHidden(false);
  expect(beats()).toBe(62);
  advance(15_000);
  expect(beats()).toBe(63);
  advance(15_000);
  expect(beats()).toBe(64);
});

it('隠れた時点で前回から 10 秒たっていれば、次は 50 秒後', () => {
  renderHook(() => useApplicantPresence(false, '1', '09012345678'));
  advance(10_000);
  setHidden(true);
  advance(49_999);
  expect(beats()).toBe(1);
  advance(1);
  expect(beats()).toBe(2);
});

it('閉じるときは取り下げ、そのあとは知らせない', () => {
  const { unmount } = renderHook(() => useApplicantPresence(false, '1', '09012345678'));
  unmount();
  expect(leaves()).toBe(1);
  advance(600_000);
  expect(beats()).toBe(1);
});
