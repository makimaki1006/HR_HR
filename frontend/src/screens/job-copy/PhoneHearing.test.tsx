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
    expect(container.querySelectorAll('.ph-card h3')).toHaveLength(0);
    expect(container.textContent).not.toContain('応募者の属性は記録されていません');
    expect(cards[0]?.textContent).toContain('勤務条件の優先順位通勤時間、休日、給与の順で希望');
    expect(cards[0]?.querySelector('.ph-note > p')?.textContent).toBe('2026/09/24 13:30');
    expect(container.textContent).not.toContain('JST');
    expect(cards[0]?.textContent).toContain('【架電】土日休みと、見学できる日程を確認したい。');
    expect(cards[1]?.textContent).toContain('ほかの求人にも関連する応募');
    expect(cards[2]?.textContent).toContain('応募媒体：記録なし');
  });
  it('属性がある応募だけ年代・性別を表示し、空欄や不明の応募は行ごと省く', () => {
    const { container, rerender } = render(<PhoneHearingContent data={demoPhoneHearing} applicationRows={[
      { key: '8'.repeat(64), date: '2026-09-24', age: '30代', gender: '女性', prefecture: null, municipality: null },
      { key: '4'.repeat(64), date: '2026-09-16', age: null, gender: null, prefecture: null, municipality: null },
      { key: '1'.repeat(64), date: '2026-09-02', age: ' ', gender: '不明', prefecture: null, municipality: null },
    ]} />);
    const cards = container.querySelectorAll('article');
    expect(cards[0]?.querySelector('h3')?.textContent).toBe('応募者：30代・女性');
    expect(cards[1]?.querySelector('h3')).toBeNull();
    expect(cards[2]?.querySelector('h3')).toBeNull();
    rerender(<PhoneHearingContent data={demoPhoneHearing} applicationRows={[
      { key: '8'.repeat(64), date: '2026-09-24', age: null, gender: '女性', prefecture: null, municipality: null },
    ]} />);
    expect(container.querySelector('h3')?.textContent).toBe('応募者：女性');
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
  it('応募一覧が無くても、電話の応答にある性別・年齢・地域を表示する', () => {
    const card = demoPhoneHearing.items[0];
    if (!card) throw new Error('fixture');
    const { container } = render(<PhoneHearingContent data={{ ...demoPhoneHearing, items: [{ ...card, gender: '女性', age: '38', prefecture: '大分県', municipality: '大分市' }] }} />);
    expect(container.querySelector('h3')?.textContent).toBe('応募者：女性・38・大分県大分市');
  });
  it('取得失敗を記録なしと区別する', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 502 })));
    const job = jobs[0];
    if (!job) throw new Error('demo fixture missing');
    const { container } = render(<PhoneHearing job={{ ...job, phoneHearing: undefined, hubspotId: '30' }} />);
    expect(await within(container).findByText('電話ヒアリングを読み込めませんでした')).toBeDefined();
  });
});
