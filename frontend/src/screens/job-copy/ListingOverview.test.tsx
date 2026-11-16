// @vitest-environment happy-dom
import { beforeEach as pinSidebarBeforeEach, afterEach as clearSidebarAfterEach } from 'vitest';
pinSidebarBeforeEach(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
clearSidebarAfterEach(() => { localStorage.removeItem('hrhr-job-copy-sidebar-pinned'); });
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); });
afterEach(() => { cleanup(); });
function query(selector: string): HTMLElement {
  const element = document.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`${selector} が見つかりません`);
  return element;
}

describe('求人の画面（案3-2）', () => {
  it('上の帯に最新の変更と全文を比べる操作があり、手直しの入口は「この求人でやること」の1つにまとめる', () => {
    render(<JobCopyScreen />);
    const band = screen.getByRole('region', { name: 'この求人の要点' });
    expect(within(band).getByTestId('band-latest-event').textContent).toBe('最新の変更：9/25 給与と本文と画像を変更見る ›');
    expect(within(band).getByRole('button', { name: '全文を比べる' })).toBeTruthy();
    expect(within(band).queryByRole('button', { name: 'この求人から新しいパターンを作る' })).toBeNull();
    // データの最後の日は帯の中の1行。右の「数字の前提」欄はない。
    expect(within(band).getByTestId('band-data').textContent).toContain('応募データ：');
    expect(document.querySelector('[aria-label="数字の前提"]')).toBeNull();
    // 最新の変更を押すと、その出来事が下で開く。
    fireEvent.click(within(band).getByTestId('band-latest-event'));
    expect(query('.lo-event.lo-open .lo-num').textContent).toBe('5');
  });

  it('出来事は既定で新しい順に5件だけ出し、すべて表示で全9件になる。番号は応募以外に古い順で振り、判定語を出さない', () => {
    render(<JobCopyScreen />);
    const nums = () => [...query('.lo-events').querySelectorAll('.lo-event-head')].map(head => head.querySelector('.lo-num')?.textContent);
    expect(nums()).toEqual(['5', '応', '応', '4', '3']);
    expect(screen.getByText('新しい順に5件を表示しています（全9件）。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'すべて表示（9件）' }));
    expect(nums()).toEqual(['5', '応', '応', '4', '3', '応', '応', '2', '1']);
    const list = query('.lo-events');
    const heads = list.querySelectorAll('.lo-event-head');
    expect(heads[0]?.querySelector('.lo-result strong')?.textContent).toBe('15 → 3 件');
    // 前と後の日数が違う（前24日・後11日）ので、件数の差は出さない
    expect(heads[0]?.querySelector('.lo-result small')?.textContent).toBe('後の期間がまだ短いため差は出していません（前 24 日・後 11 日）');
    expect(list.textContent).not.toMatch(/増えた|減った|効果|効いた/u);
    fireEvent.click(screen.getByRole('button', { name: '最新5件だけにする' }));
    expect(nums()).toHaveLength(5);
  });

  it('「この求人でやること」の4枚の札を出来事の上に並べ、札ごとに1行の説明を付ける', () => {
    render(<JobCopyScreen />);
    expect(document.querySelector('details.lo-work')).toBeNull();
    expect(document.body.textContent).not.toContain('求人を選ぶ → 手直し');
    const tasks = screen.getByRole('region', { name: 'この求人でやること' });
    const cards = within(tasks).getAllByRole('button');
    expect(cards.map(card => card.querySelector('strong')?.textContent)).toEqual(['文面を手直しする', 'リクロジ向けの案を作る', '電話ヒアリングを見る', 'くわしい分析を開く']);
    expect(cards[0]?.textContent).toContain('今の本文を基に直し、確認待ちの案として保存します');
    expect(cards[1]?.textContent).toContain('AirWork と重ならない文章を作ります（数分）');
    expect(cards[3]?.textContent).toContain('本文と画像・変更の前後・応募者など');
    // 操作デモの求人は HubSpot の求人と結び付いていないので、手直し・リクロジ向けの案は押せず、理由を出す。
    expect((cards[0] as HTMLButtonElement).disabled).toBe(true);
    expect(cards[0]?.textContent).toContain('HubSpot の求人と結び付いた求人だけで使えます');
    expect((cards[1] as HTMLButtonElement).disabled).toBe(true);
    expect(cards[1]?.textContent).toContain('HRハッカーの求人だけで作れます');
    // 出来事の上、ライフラインの下に置く。
    const events = screen.getByRole('heading', { name: '出来事（新しい順）' });
    const lifeline = screen.getByRole('heading', { name: '求人のライフライン' });
    expect(tasks.compareDocumentPosition(events) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(tasks.compareDocumentPosition(lifeline) & Node.DOCUMENT_POSITION_PRECEDING).toBeTruthy();
    // 電話ヒアリングの札を押すと、札の下に開いた状態で出て、「× 閉じる」で閉じる。
    const hearingCard = cards[2]; if (!hearingCard) throw new Error('電話ヒアリングの札が必要です');
    fireEvent.click(hearingCard);
    expect(cards[2]?.getAttribute('aria-pressed')).toBe('true');
    const panel = screen.getByRole('region', { name: '電話ヒアリングを見る' });
    expect(panel.hidden).toBe(false);
    // 札の中では見出しが二重にならないよう、畳む見出し（details）を付けない
    expect(panel.querySelector('details')).toBeNull();
    expect(panel.querySelector('.ph-section')).not.toBeNull();
    fireEvent.click(within(panel).getByRole('button', { name: '電話ヒアリングを見るを閉じる' }));
    // 閉じても中身は消さず hidden にする（作りかけを残すため）
    expect(document.querySelector<HTMLElement>('.lt-panel[aria-label="電話ヒアリングを見る"]')?.hidden).toBe(true);
  });

  it('出来事を押すと、ほかに応募を動かしそうなことが出る。もう一度押すと閉じる', () => {
    render(<JobCopyScreen />);
    const head = query('.lo-event-head');
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(head.querySelector('.lo-caret')?.textContent).toBe('詳細 ▼');
    expect(screen.getByText('行を押すと、その出来事の詳細が開きます。上のライフラインのその週にも印が付きます。')).toBeTruthy();
    fireEvent.click(head);
    expect(head.getAttribute('aria-expanded')).toBe('true');
    expect(head.querySelector('.lo-caret')?.textContent).toBe('閉じる ▲');
    const checks = screen.getByRole('list', { name: 'ほかに応募を動かしそうなこと' });
    expect(within(checks).getAllByRole('listitem').map(item => item.querySelector('strong')?.textContent)).toEqual(['課金', '市場', 'ほかの変更']);
    expect(within(checks).getByText('「課金を開始」が24日前（9/01）に、「課金を増額」が10日前（9/15）にあります。')).toBeTruthy();
    fireEvent.click(head);
    expect(head.getAttribute('aria-expanded')).toBe('false');
  });

  it('出来事を押すと、横の時間軸のその週に印が付く', () => {
    render(<JobCopyScreen />);
    expect(screen.queryByTestId('lifeline-selected')).toBeNull();
    fireEvent.click(query('.lo-event-head'));
    const mark = screen.getByTestId('lifeline-selected');
    expect(mark.style.width).toBe('56px');
    // 「—」（取れていない）と「0」（0件）を見分けられるよう、表の下に1行で書く。
    expect(screen.getByTestId('lifeline-legend').textContent).toBe('「—」はデータを取れていない週、「0」は0件の週です。');
    expect(query('.lo-event.lo-open .lo-num').textContent).toBe('5');
  });

  it('応募者・応募理由の前後は横から出て、閉じられる。「くわしい分析」は入口の1つから開き、タブは7つ', () => {
    render(<JobCopyScreen />);
    fireEvent.click(query('.lo-event-head'));
    fireEvent.click(screen.getByRole('button', { name: '応募者・応募理由の前後を見る' }));
    const drawer = screen.getByRole('dialog', { name: '応募者・応募理由の前後' });
    expect(within(drawer).getByText(/前の4週 15 件 → 後の4週 3 件/u)).toBeTruthy();
    fireEvent.click(within(drawer).getByRole('button', { name: '閉じる' }));
    expect(screen.queryByRole('dialog', { name: '応募者・応募理由の前後' })).toBeNull();
    const tasks = within(screen.getByRole('region', { name: 'この求人でやること' }));
    fireEvent.click(tasks.getByRole('button', { name: /^くわしい分析を開く/ }));
    expect(screen.getByRole('dialog', { name: 'くわしい分析' })).toBeTruthy();
    expect(within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['本文と画像', '変更の前後', 'ほかの求人と比べる', '応募者', '応募理由', 'メモ']);
  });
});
