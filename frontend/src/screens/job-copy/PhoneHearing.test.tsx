// @vitest-environment happy-dom
import { fireEvent, render, screen, within, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PhoneHearing, PhoneHearingContent } from './PhoneHearing';
import { demoPhoneHearing } from './demoPhoneHearing';
import { jobs } from './data';

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('電話ヒアリング', () => {
  it('畳んだ件数見出しと、匿名の応募日・媒体・聞き取り・時刻つきメモを表示する', () => {
    const { container } = render(<PhoneHearingContent data={demoPhoneHearing} />);
    const title = screen.getByText('電話ヒアリング（記録あり 3件 / 応募 28件）');
    expect(container.querySelector('details')?.open).toBe(false);
    fireEvent.click(title);
    const cards = container.querySelectorAll('article');
    expect(cards.length).toBe(3);
    expect(cards[0]?.textContent).toContain('応募日：2026/09/24');
    expect(cards[0]?.textContent).toContain('応募媒体：求人検索サイト');
    expect(cards[0]?.textContent).toContain('応募者の属性は記録されていません');
    expect(cards[0]?.textContent).toContain('勤務条件の優先順位通勤時間、休日、給与の順で希望');
    expect(cards[0]?.textContent).toContain('2026/09/24 13:30');
    expect(cards[0]?.textContent).toContain('【架電】土日休みと、見学できる日程を確認したい。');
    expect(cards[1]?.textContent).toContain('ほかの求人にも関連する応募');
    expect(cards[2]?.textContent).toContain('応募媒体：記録なし');
  });
  it('記録なしは失敗と区別し、上限で切った場合に注記する', () => {
    const { rerender } = render(<PhoneHearingContent data={{ total_applicants: 22, items: [], truncated: false }} />);
    expect(screen.getByText('電話ヒアリング（記録あり 0件 / 応募 22件）')).toBeDefined();
    expect(screen.getByText('この求人の応募には、電話で聞いた内容の記録がありません')).toBeDefined();
    rerender(<PhoneHearingContent data={null} />);
    expect(screen.getByRole('status').textContent).toBe('電話ヒアリングを読み込めませんでした');
    expect(screen.queryByText('この求人の応募には、電話で聞いた内容の記録がありません')).toBeNull();
    rerender(<PhoneHearingContent data={{ ...demoPhoneHearing, truncated: true }} />);
    expect(screen.getByText('一部だけ表示しています')).toBeDefined();
  });
  it('取得失敗を記録なしと区別する', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 502 })));
    const job = jobs[0];
    if (!job) throw new Error('demo fixture missing');
    const { container } = render(<PhoneHearing job={{ ...job, phoneHearing: undefined, hubspotId: '30' }} />);
    expect(await within(container).findByText('電話ヒアリングを読み込めませんでした')).toBeDefined();
  });
});
