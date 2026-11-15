// @vitest-environment happy-dom
import { useEffect, useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { crumbAction, dealBackTarget, originLabel, originMenuView, originOf, originView } from './origin';
import { daysUntil, expiryText } from './DealHeading';
import { OverdueBadge, overdueDays } from './NextTodo';
import { ListingTasks } from './ListingTasks';
import type { ListingTask, ListingTaskOptions } from './ListingTasks';

describe('来た元（?from=）', () => {
  it('知っているメニューだけを来た元として読む', () => {
    expect(originOf('?deal=1&from=risk')).toBe('risk');
    expect(originOf('?deal=1&from=today')).toBe('today');
    expect(originOf('?deal=1&from=houjin')).toBe('houjin');
    expect(originOf('?deal=1&from=unknown')).toBeNull();
    expect(originOf('?deal=1')).toBeNull();
    expect(originLabel('houjin')).toBe('顧客の一覧');
    expect(originLabel('deals')).toBe('案件一覧');
  });
  it('メニューの選択は今日なら null、ほかは同じ view', () => {
    expect(originMenuView('today')).toBeNull();
    expect(originMenuView(null)).toBeNull();
    expect(originMenuView('risk')).toBe('risk');
    expect(originView('today')).toBeNull();
    expect(originView('deals')).toBe('deals');
    expect(originView(null)).toBeUndefined();
    expect(originView('bogus')).toBeUndefined();
  });
  it('戻るリンクは 求人 → 来た元 → 顧客のページ → 案件管理 の順で決める', () => {
    expect(dealBackTarget({ listingReady: true, origin: 'risk', hasCompany: true })).toEqual({ kind: 'listing', label: '求人に戻る' });
    expect(dealBackTarget({ listingReady: false, origin: 'risk', hasCompany: true })).toEqual({ kind: 'origin', label: '危ない案件に戻る' });
    expect(dealBackTarget({ listingReady: false, origin: 'houjin', hasCompany: false })).toEqual({ kind: 'origin', label: '顧客の一覧に戻る' });
    expect(dealBackTarget({ listingReady: false, origin: 'today', hasCompany: false })).toEqual({ kind: 'origin', label: '今日に戻る' });
    expect(dealBackTarget({ listingReady: false, origin: null, hasCompany: true })).toEqual({ kind: 'company', label: '顧客のページに戻る' });
    expect(dealBackTarget({ listingReady: false, origin: null, hasCompany: false })).toEqual({ kind: 'home', label: '案件管理に戻る' });
  });
  it('パンくずは、顧客が URL に無くても分かっていれば顧客へ、先頭は来た元へ', () => {
    const base = { onDealPage: true, trailHasCompany: false, knownCompany: true, origin: 'deals', listingReady: false };
    expect(crumbAction('company', base)).toBe('company');
    expect(crumbAction('company', { ...base, trailHasCompany: true })).toBe('goto');
    expect(crumbAction('company', { ...base, knownCompany: false })).toBe('goto');
    expect(crumbAction('home', base)).toBe('origin');
    expect(crumbAction('home', { ...base, origin: null })).toBe('goto');
    expect(crumbAction('home', { ...base, listingReady: true })).toBe('goto');
    expect(crumbAction('home', { ...base, onDealPage: false })).toBe('goto');
    expect(crumbAction('deal', base)).toBe('goto');
  });
});

describe('満了までの日数', () => {
  it('今日からの日数を数える', () => {
    expect(daysUntil('2026-11-30', '2026-11-15')).toBe(15);
    expect(daysUntil('2026-11-15', '2026-11-15')).toBe(0);
    expect(daysUntil('2026-11-10', '2026-11-15')).toBe(-5);
  });
  it('赤は残り 30 日以内と過ぎて 30 日以内だけ。過ぎて 31 日以上は満了済み（灰）', () => {
    expect(expiryText(31)).toEqual({ text: '満了まで 31日', tone: 'normal' });
    expect(expiryText(30)).toEqual({ text: '満了まで 30日', tone: 'urgent' });
    expect(expiryText(0)).toEqual({ text: '満了まで 0日', tone: 'urgent' });
    expect(expiryText(-1)).toEqual({ text: '満了から 1日', tone: 'urgent' });
    expect(expiryText(-30)).toEqual({ text: '満了から 30日', tone: 'urgent' });
    expect(expiryText(-31)).toEqual({ text: '満了済み', tone: 'ended' });
    expect(expiryText(-400)).toEqual({ text: '満了済み', tone: 'ended' });
  });
});

describe('過ぎた予定', () => {
  it('今日より前の日だけ日数を返す', () => {
    expect(overdueDays('2026-10-29', '2026-11-15')).toBe(17);
    expect(overdueDays('2026-11-14', '2026-11-15')).toBe(1);
    expect(overdueDays('2026-11-15', '2026-11-15')).toBeNull();
    expect(overdueDays('2026-11-20', '2026-11-15')).toBeNull();
    expect(overdueDays(null, '2026-11-15')).toBeNull();
    expect(overdueDays('', '2026-11-15')).toBeNull();
  });
  it('赤い札に「◯日過ぎています」と出し、過ぎていなければ何も出さない', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date('2026-11-15T03:00:00Z'));
    const { container, rerender } = render(<OverdueBadge due="2026-10-29" />);
    expect(container.querySelector('.nt-overdue')?.textContent).toBe('17日過ぎています');
    rerender(<OverdueBadge due="2026-11-20" />);
    expect(container.querySelector('.nt-overdue')).toBeNull();
    vi.useRealTimers();
  });
});

describe('この求人でやること', () => {
  /** 生成中の処理を表す部品。片付けられると abort を呼ぶ。 */
  function Generation({ onAbort }: { onAbort: () => void }) {
    useEffect(() => () => { onAbort(); }, [onAbort]);
    return <p>案を作っています…</p>;
  }
  function Harness({ onAbort }: { onAbort: () => void }) {
    const [active, setActive] = useState<ListingTask | null>(null);
    const options: ListingTaskOptions = {
      active, onChange: setActive,
      edit: { disabled: null, onOpen: () => undefined, panel: <label>見出し<input aria-label="見出し" /></label> },
      rikurozi: { disabled: null, panel: <Generation onAbort={onAbort} /> },
      onAnalysis: () => undefined,
    };
    return <ListingTasks options={options} hearingSummary="記録あり 0件 ／ 応募 3件" hearingPanel={<p>聞き取りの記録</p>} />;
  }
  const panel = (name: string) => { const element = document.querySelector<HTMLElement>(`.lt-panel[aria-label="${name}"]`); if (!element) throw new Error(name); return element; };
  it('札を閉じる・切り替えても、手直しの入力とリクロジ向けの案の生成は消えない', () => {
    const onAbort = vi.fn();
    render(<Harness onAbort={onAbort} />);
    // 開く前は中身を描かない（開く前の読み込みを増やさない）
    expect(document.querySelector('.lt-panel')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /文面を手直しする/ }));
    const input = screen.getByLabelText('見出し');
    fireEvent.change(input, { target: { value: '書きかけの見出し' } });
    fireEvent.click(screen.getByRole('button', { name: /リクロジ向けの案を作る/ }));
    expect(panel('リクロジ向けの案を作る').hidden).toBe(false);
    expect(panel('文面を手直しする').hidden).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジ向けの案を作るを閉じる' }));
    fireEvent.click(screen.getByRole('button', { name: /電話ヒアリングを見る/ }));
    fireEvent.click(screen.getByRole('button', { name: /文面を手直しする/ }));
    expect(screen.getByLabelText<HTMLInputElement>('見出し').value).toBe('書きかけの見出し');
    expect(onAbort).not.toHaveBeenCalled();
    expect(panel('文面を手直しする').hidden).toBe(false);
    expect(panel('電話ヒアリングを見る').hidden).toBe(true);
  });
});
