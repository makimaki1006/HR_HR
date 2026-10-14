// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { DealOverview, dialDisabledReason, lastActivityLine } from './DealDetail';
import type { DetailState } from './useDealDetail';
import type { ZoomPhone } from './useZoomPhone';
import { fixtureDetail } from './workspaceFixture';
import { fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS } from './queueModel';
import { EMPTY_CALL } from './smartEmbed';

afterEach(cleanup);

const id = fixtureQueuePage(DEFAULT_FILTERS, null).items[0]?.deal_id ?? '';
const zoom = (embed: ZoomPhone['embed'] = 'loaded'): ZoomPhone => ({
  embed, onLoad: () => undefined, call: EMPTY_CALL, pending: null, stalled: false, heard: true, now: () => 0, dial: () => 'sent',
});
function base(): WorkspaceResponse {
  const d = fixtureDetail(id);
  if (d === null) throw new Error('fixture');
  return d;
}
function show(data: WorkspaceResponse, compact: boolean, z = zoom()) {
  const state = { phase: 'ready', data, refreshing: false, refreshError: null } as unknown as DetailState;
  return render(<DealOverview state={state} reload={() => undefined} zoom={z} density={{ compact, onToggle: () => undefined }} />);
}

describe('deal header (2 rows)', () => {
  it('row 1 = company · contact + HubSpot link + toggle; row 2 = number + dial; separators only between items', () => {
    show(base(), true);
    const r1 = screen.getByTestId('head-line-1');
    const r2 = screen.getByTestId('head-line-2');
    expect(within(r1).getByRole('heading', { level: 2 }).textContent).toBe(base().companies[0]?.name);
    expect(within(r1).getByRole('link', { name: 'HubSpotで開く' })).toBeTruthy();
    expect(within(r1).getByRole('button', { name: '詳しく表示' })).toBeTruthy();
    expect(r1.querySelectorAll('.wd-line-sep').length).toBe(1);
    expect(r2.querySelector('.cq-phone')?.textContent).toBe('03-0000-0005');
    expect(within(r2).getByRole('button', { name: /に発信$/ }).hasAttribute('disabled')).toBe(false);
    expect(r2.querySelectorAll('.wd-line-sep').length).toBe(0);
    expect(r1.querySelector('.cq-phone')).toBeNull();
  });

  it('long company name keeps the full name in title (CSS ellipsis)', () => {
    const d = base();
    const long = 'とても長い会社名株式会社'.repeat(8);
    d.companies = d.companies.map(c => ({ ...c, name: long }));
    show(d, true);
    expect(within(screen.getByTestId('head-line-1')).getByRole('heading', { level: 2 }).getAttribute('title')).toBe(long);
  });
});

describe('last activity line', () => {
  it('shows the first activity: date, kind and body head, with the full text in title', () => {
    show(base(), true);
    const line = screen.getByTestId('last-activity');
    expect(line.textContent).toBe('前回 10/03 通話・受付で不在。来週再架電。');
    expect(line.getAttribute('title')).toBe('前回 10/03 通話・受付で不在。来週再架電。');
  });
  it('is also shown in the expanded header, and not shown when there is no activity', () => {
    show(base(), false);
    expect(screen.getByTestId('last-activity').textContent).toContain('前回 10/03 通話');
    cleanup();
    const d = base();
    d.activities = [];
    show(d, true);
    expect(screen.queryByTestId('last-activity')).toBeNull();
    expect(lastActivityLine([])).toBeNull();
  });
  it('uses the note kind label and collapses line breaks', () => {
    const a = base().activities[1];
    if (!a) throw new Error('fixture');
    expect(lastActivityLine([{ ...a, body: '一行目\n\n二行目' }])?.text).toBe('前回 10/02 メモ・一行目 二行目');
  });
});

describe('why 発信 is disabled', () => {
  it('reason texts', () => {
    expect(dialDisabledReason('+81312345678', 'loaded', false)).toBeNull();
    expect(dialDisabledReason('+81312345678', 'disabled', false)).toBe('Zoom を読み込めないため発信できません');
    expect(dialDisabledReason(null, 'loaded', false)).toBe('ダイヤルできる形式ではないため発信できません');
    expect(dialDisabledReason('+81312345678', 'loaded', true, 'ブロックのため発信できません')).toBe('ブロックのため発信できません');
  });
  it('shows the reason as text and title on the disabled button (prohibited, blocked)', () => {
    const d = base();
    d.deal = { ...d.deal, stop: { ...d.deal.stop, prohibited_reason: '架空の理由', block_reason: null } };
    show(d, true);
    const btn = within(screen.getByTestId('head-line-2')).getByRole('button', { name: /に発信$/ });
    expect(btn.hasAttribute('disabled')).toBe(true);
    expect(btn.getAttribute('title')).toBe('架電禁止のため発信できません');
    expect(screen.getByTestId('dial-disabled-reason').textContent).toBe('架電禁止のため発信できません');
    cleanup();
    const e = base();
    e.deal = { ...e.deal, stop: { ...e.deal.stop, prohibited_reason: null, block_reason: 'X' } };
    show(e, true);
    expect(screen.getByTestId('dial-disabled-reason').textContent).toBe('ブロックのため発信できません');
  });
  it('Zoom unavailable and no phone number', () => {
    show(base(), true, zoom('disabled'));
    expect(screen.getByTestId('dial-disabled-reason').textContent).toBe('Zoom を読み込めないため発信できません');
    cleanup();
    const d = base();
    d.dial = null;
    show(d, true);
    expect(screen.getByTestId('dial-disabled-reason').textContent).toBe('電話番号がありません');
  });

  it('「前回 …」の行は押せるボタンで、押すと活動ログを開く合図が出る (つながっていないときは文字だけ)', () => {
    const calls: string[] = [];
    const state = { phase: 'ready', data: base(), refreshing: false, refreshError: null } as unknown as DetailState;
    render(<DealOverview state={state} reload={() => undefined} zoom={zoom()} density={{ compact: true, onToggle: () => undefined }} onOpenActivity={() => { calls.push('open'); }} />);
    const btn = within(screen.getByTestId('last-activity')).getByRole('button');
    expect(btn.textContent).toMatch(/^前回 /u);
    fireEvent.click(btn);
    expect(calls).toEqual(['open']);
    cleanup();
    show(base(), true);
    expect(within(screen.getByTestId('last-activity')).queryByRole('button')).toBeNull();
  });
});
