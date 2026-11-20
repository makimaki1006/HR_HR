// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { ReportInput } from './reportSlidesModel';

const input: ReportInput = {
  scope: 'listing', id: '501', name: '合成の配送求人', company: '合成運輸', overlap: {}, asOf: '2026-11-15', outcomes: null,
  listings: [{ id: '501', title: '合成の配送求人', media: 'hrh', ended: false, endedOn: null,
    byDate: { '2026-08-20': 1, '2026-10-20': 2 }, events: { listingId: '501', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-08-01', statusEvents: [] } }],
  applicants: { rows: null, reasons: null, missingReason: '応募者を読み込めなかったため' },
};
const dealInput: ReportInput = { ...input, scope: 'deal', id: '901', name: '合成・定期採用', outcomes: { interviews: 4, hires: 1, readOn: '2026-11-16' } };
vi.mock('./reportSlidesLoad', () => ({ loadReportInput: (target: { scope: string }) => Promise.resolve({ ok: true, input: target.scope === 'deal' ? dealInput : input }) }));
const { ReportSlides } = await import('./ReportSlides');

afterEach(cleanup);
describe('報告のスライドを作る画面', () => {
  it('入れるスライドを左で選び、外したものを社内向けに出し、印刷には選んだものだけを並べる', async () => {
    render(<ReportSlides target={{ scope: 'listing', id: '501' }} />);
    const side = await screen.findByRole('complementary', { name: '報告の設定' });
    expect(within(side).getAllByRole('checkbox').map(box => box.getAttribute('aria-label'))).toEqual([
      '表紙を入れる', '目次を入れる', '掲載求人を入れる', '採用の導線を入れる', '採用手法別の進捗を入れる', '数字の表を入れる', '総評と前回との比較を入れる', '現状と課題仮説：合成の配送求人を入れる',
      '応募の推移を入れる', '応募者への対応方針を入れる', '次の施策：合成の配送求人を入れる', '次回までの目標と行動計画を入れる', 'アンケートと次回の日程を入れる',
    ]);
    expect(within(screen.getByRole('region', { name: '外したもの' })).getAllByRole('listitem').map(item => item.textContent)).toContain('応募者の傾向：応募者を読み込めなかったため');
    // 補足のスライド（応募の推移）は最初は入れない。文章が未記入のスライドも出力しない。
    const ids = () => [...document.querySelectorAll<HTMLElement>('.rs-print-deck .rs-slide')].map(item => item.dataset.slide);
    expect(ids()).toEqual(['cover', 'toc', 'listings', 'route', 'numbers', 'review', 'listing:501', 'closing']);
    fireEvent.click(within(side).getByRole('checkbox', { name: '応募の推移を入れる' }));
    expect(ids()).toContain('trend');
    fireEvent.click(within(side).getByRole('button', { name: /次回までの目標と行動計画/ }));
    fireEvent.change(screen.getByRole('textbox', { name: '次回の定期MTGまでの共通目標' }), { target: { value: '内定 1名の獲得' } });
    expect(document.querySelector('.rs-print-deck [data-slide="plan"]')?.textContent).toContain('内定 1名の獲得');
    const cover = document.querySelector('.rs-print-deck [data-slide="cover"]')?.textContent ?? '';
    expect(cover).toContain('合成運輸 御中');
    expect(cover).not.toMatch(/HubSpot|未取得/);
  });
  it('案件の報告は面接・採用を数字の表の別の段に出し、左の切り替えで外せる（求人の報告には切り替えを出さない）', async () => {
    render(<ReportSlides target={{ scope: 'deal', id: '901' }} />);
    const side = await screen.findByRole('complementary', { name: '報告の設定' });
    const box = within(side).getByRole('checkbox', { name: /面接・採用を出す/ });
    expect((box as HTMLInputElement).checked).toBe(true);
    const totals = () => document.querySelector('.rs-print-deck [data-slide="numbers"] .rs-totals');
    expect(totals()?.querySelector('.rs-totals-h')?.textContent).toBe('ご契約からの合計（11/16 時点）');
    expect([...(totals()?.querySelectorAll('.rs-kp > div') ?? [])].map(item => item.textContent)).toEqual(['面接4件', '採用1名']);
    // 期間の数字の表には面接・採用を並べない
    expect(document.querySelector('.rs-print-deck [data-slide="numbers"] .rs-table')?.textContent).not.toMatch(/面接|採用/);
    fireEvent.click(box);
    expect(totals()).toBeNull();
    expect(document.querySelector('.rs-print-deck [data-slide="numbers"] .rs-table')?.textContent).toContain('合成の配送求人');
  });
});
