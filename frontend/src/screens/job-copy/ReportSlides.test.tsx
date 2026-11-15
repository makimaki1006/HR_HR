// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import type { ReportInput } from './reportSlidesModel';

const input: ReportInput = {
  scope: 'listing', id: '501', name: '合成の配送求人', company: '合成運輸', overlap: {}, asOf: '2026-11-15', outcomes: null,
  listings: [{ id: '501', title: '合成の配送求人', media: 'hrh', ended: false, endedOn: null,
    byDate: { '2026-08-20': 1, '2026-10-20': 2 }, events: { listingId: '501', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-08-01', statusEvents: [] } }],
  applicants: { rows: null, reasons: null, missingReason: '応募者を読み込めなかったため' },
};
vi.mock('./reportSlidesLoad', () => ({ loadReportInput: () => Promise.resolve({ ok: true, input }) }));
const { ReportSlides } = await import('./ReportSlides');

describe('報告のスライドを作る画面', () => {
  it('入れるスライドを左で選び、外したものを社内向けに出し、印刷には選んだものだけを並べる', async () => {
    render(<ReportSlides target={{ scope: 'listing', id: '501' }} />);
    const side = await screen.findByRole('complementary', { name: '報告の設定' });
    expect(within(side).getAllByRole('checkbox').map(box => box.getAttribute('aria-label'))).toEqual(['表紙を入れる', 'まとめを入れる', '応募の推移を入れる']);
    expect(within(screen.getByRole('region', { name: '外したもの' })).getAllByRole('listitem').map(item => item.textContent)).toContain('応募者の傾向：応募者を読み込めなかったため');
    expect(document.querySelectorAll('.rs-print-deck .rs-slide')).toHaveLength(3);
    fireEvent.click(within(side).getByRole('checkbox', { name: '応募の推移を入れる' }));
    expect(document.querySelectorAll('.rs-print-deck .rs-slide')).toHaveLength(2);
    fireEvent.change(within(side).getByRole('textbox', { name: '次に行うこと1' }), { target: { value: '求人の写真を差し替えます' } });
    expect(document.querySelectorAll('.rs-print-deck .rs-slide')).toHaveLength(3);
    expect(document.querySelector('.rs-print-deck [data-slide="next"]')?.textContent).toContain('求人の写真を差し替えます');
    const cover = document.querySelector('.rs-print-deck [data-slide="cover"]')?.textContent ?? '';
    expect(cover).toContain('合成運輸 御中');
    expect(cover).not.toMatch(/HubSpot|未取得/);
  });
});
