// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApplicantSheet, ApplicantsScreen } from './ApplicantsScreen';
import { applicantDetailFixture } from './applicantFixture';
import { applicantMode, modeUrl } from './CrmModeSwitch';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
describe('応募の読み取りモード', () => {
  it('既存のビューと架空サンプルの条件を保って切り替える', () => {
    const url = modeUrl('?mode=fixture&view=mine&stage=abc', true);
    expect(applicantMode(url)).toBe(true);
    expect(new URLSearchParams(url).get('mode')).toBe('fixture');
    expect(new URLSearchParams(modeUrl(url, false)).get('view')).toBe('mine');
    expect(applicantMode(modeUrl(url, false))).toBe(false);
    expect(applicantMode('?mode=applicants')).toBe(true);
    expect(applicantMode(modeUrl('?mode=applicants', false))).toBe(false);
  });
  it('架空の一覧から詳細・対応ルール・結果・シートを読む。書き込みも実通信もしない', async () => {
    const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
    render(<ApplicantsScreen fixture />);
    expect(await screen.findByText('2件')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /架空 はるか/ }));
    expect(screen.getByRole('heading', { level: 2 }).textContent).toBe('架空 はるか');
    expect(await screen.findByText('資格なし', { selector: 'dd' })).toBeTruthy();
    const pane = screen.getByRole('main', { name: '応募の詳細' });
    expect(within(pane).getAllByRole('heading').map(h => h.textContent)).toEqual(['架空 はるか', 'この求人の対応ルール', '応募先', 'メモ']);
    const jobCard = within(pane).getByRole('heading', { name: /^応募先$/ }).closest('section');
    expect(jobCard?.textContent).toContain('サンプル物流株式会社');
    expect(jobCard?.textContent).toContain('検品スタッフ（架空求人）');
    expect(jobCard?.querySelector('a')?.href).toBe('https://app.hubspot.com/contacts/0/record/0-420/80001/');
    expect(screen.queryByTestId('link-hint')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /架空 はるか/ }));
    expect(screen.getByText('資格なし', { selector: 'dd' })).toBeTruthy();
    expect(screen.getByText('応募のお礼を伝え、希望の勤務時間を確認してください。')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Zoom で発信' }).hasAttribute('disabled')).toBe(true);
    expect(document.querySelector('iframe[title^="報告シート"]')?.getAttribute('src')).toContain('/fictional-applicants-sheet/');
    fireEvent.click(screen.getByRole('tab', { name: 'Zoom' }));
    expect(screen.getByText('架空サンプルでは発信できません')).toBeTruthy();
    fireEvent.click(screen.getByRole('tab', { name: '報告シート' }));
    const detailPane = screen.getByRole('main', { name: '応募の詳細' });
    detailPane.scrollTop = 500;
    fireEvent.click(screen.getByRole('button', { name: /架空 あおい/ }));
    expect(detailPane.scrollTop).toBe(0);
    expect(await screen.findByText('この応募には報告シートの URL がありません')).toBeTruthy();
    expect(fetcher).not.toHaveBeenCalled();
  });
  it('既存のステージ選択と媒体で絞る', async () => {
    render(<ApplicantsScreen fixture />);
    fireEvent.click(await screen.findByRole('button', { name: 'ステージ（2件選択）' }));
    expect(screen.getByRole('heading', { name: '一次対応_BPO' })).toBeTruthy();
    fireEvent.click(screen.getByRole('checkbox', { name: '朝連絡' }));
    fireEvent.click(screen.getByRole('button', { name: '適用' }));
    await waitFor(() => { expect(screen.queryByRole('button', { name: /架空 はるか/ })).toBeNull(); });
    expect(screen.getByRole('button', { name: /架空 あおい/ })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('応募媒体'), { target: { value: '架空求人サイト' } });
    expect(await screen.findByText('条件に一致する応募はありません。')).toBeTruthy();
  });
  it('Google 以外を枠にせず、危険な URL をリンクにしない', () => {
    const detail = { ...applicantDetailFixture('90001'), sheet_url: 'https://example.invalid/report', sheet_embed: true };
    const view = render(<ApplicantSheet detail={detail} />);
    expect(document.querySelector('iframe')).toBeNull();
    expect(screen.getAllByRole('link', { name: '新しいタブで開く' })[0]?.getAttribute('href')).toBe('https://example.invalid/report');
    view.rerender(<ApplicantSheet detail={{ ...detail, sheet_url: 'javascript:alert(1)' }} />);
    expect(screen.queryByRole('link')).toBeNull();
  });
  it('権限エラーの文言を出し、取引への切り替えは残す', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, json: () => Promise.resolve({ message: 'HubSpot の鍵に応募を読む権限がありません' }) }));
    render(<ApplicantsScreen fixture={false} />);
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'HubSpot の鍵に応募を読む権限がありません再試行');
    expect(screen.getByRole('link', { name: '取引' })).toBeTruthy();
  });
  it('メモのスクリプトとイベント属性を除去する', async () => {
    const detail = applicantDetailFixture('90001');
    detail.notes = [{ date: '2026-10-18', html: '<p onclick="evil()">架空メモ<script>evil()</script></p>' }];
    vi.stubGlobal('fetch', vi.fn().mockImplementation((url: string) => Promise.resolve({ ok: true, json: () => Promise.resolve(url.includes('/queue') ? { ...applicantQueueFixtureForTest, items: [detail.row] } : detail) })));
    render(<ApplicantsScreen fixture={false} />);
    fireEvent.click(await screen.findByRole('button', { name: /架空 はるか/ }));
    expect(await screen.findByText('架空メモ')).toBeTruthy();
    expect(document.querySelector('.applicants-detail script')).toBeNull();
    expect(screen.getByText('架空メモ').getAttribute('onclick')).toBeNull();
  });
});
const applicantQueueFixtureForTest = { total: 1, next_cursor: null, pipelines: [], media_options: [] };
