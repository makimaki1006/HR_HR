// @vitest-environment happy-dom
// 配置の組み直し（2026-10-08）と、課金CSV → タイムラインの課金レーンのつながりを、画面全体で確かめる。
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));

beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

function upload(text: string) {
  const file = new File([new TextEncoder().encode(text)], 'billing.csv', { type: 'text/csv' });
  fireEvent.change(screen.getByLabelText('課金CSVファイル'), { target: { files: [file] } });
}
const lane = (name: string) => screen.getByRole('group', { name });
const periodTable = () => within(screen.getByRole('region', { name: '期間比較表の数値' })).getByRole('table');

describe('job copy layout', () => {
  it('opens on the timeline with the list beside it and keeps imports and the applicant search out of the main flow', () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal('fetch', fetchSpy);
    render(<JobCopyScreen />);
    const primary = screen.getByRole('tablist', { name: '求人管理の機能' });
    expect(within(primary).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']);
    expect(within(primary).getByRole('tab', { name: 'タイムライン' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('region', { name: 'タイムライン' })).toBeTruthy();
    // 1 つしかないサブタブの段は出さない。旧来の操作バーも無い。
    expect(screen.queryByRole('tablist', { name: 'タイムラインの表示' })).toBeNull();
    expect(screen.queryByRole('tablist', { name: '求人内容の表示' })).toBeNull();
    expect(screen.queryByRole('button', { name: '求人一覧に戻る' })).toBeNull();
    expect(screen.queryByRole('button', { name: '機能を切り替える' })).toBeNull();
    expect(document.querySelector('.jc-detail-actions')).toBeNull();
    // 一覧は 8 件。絞り込みは 1 行にたたむ。
    expect(document.querySelectorAll('.jc-job')).toHaveLength(8);
    expect(document.querySelector('.jc-filter-more')?.hasAttribute('open')).toBe(false);
    // データ取込は閉じていて、HubSpot・媒体・課金CSV・外部文面の 4 つが中にある
    const importToggle = screen.getByRole('button', { name: 'データ取込' });
    expect(importToggle.getAttribute('aria-expanded')).toBe('false');
    const importArea = document.getElementById('job-copy-data-import');
    expect(importArea?.hasAttribute('hidden')).toBe(true);
    expect(importArea?.querySelector('.jc-live-panel')).not.toBeNull();
    expect(importArea?.querySelector('.jc-media-import')).not.toBeNull();
    expect(importArea?.querySelector('.jc-billing')).not.toBeNull();
    expect(importArea?.querySelector('.jc-receive-entry')).not.toBeNull();
    // 応募者の条件検索はボタンを押すまで出さない
    expect(document.querySelector('.jc-reverse-search')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '応募者の条件で探す' }));
    expect(screen.getByRole('region', { name: '応募者の条件から求人を探す' })).toBeTruthy();
    fireEvent.click(within(screen.getByRole('region', { name: '応募者の条件から求人を探す' })).getByRole('button', { name: '閉じる' }));
    expect(document.querySelector('.jc-reverse-search')).toBeNull();
    // データ出所の表示は 1 行の帯に残す
    expect(document.querySelector('.jc-topline .jc-demo')?.textContent).toContain('すべて架空');
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it('opens the external-text check from データ取込 and returns to the timeline', async () => {
    vi.stubGlobal('fetch', vi.fn());
    render(<JobCopyScreen />);
    fireEvent.click(screen.getByRole('button', { name: 'データ取込' }));
    expect(document.getElementById('job-copy-data-import')?.hasAttribute('hidden')).toBe(false);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '外部文面を照合する' })); await Promise.resolve(); });
    expect(document.getElementById('job-copy-data-import')?.hasAttribute('hidden')).toBe(true);
    const receive = document.querySelector('[id$="-panel-receive"]');
    expect(receive?.hasAttribute('hidden')).toBe(false);
    expect(within(receive as HTMLElement).getByLabelText('受け取った文面')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'タイムラインに戻る' }));
    expect(receive?.hasAttribute('hidden')).toBe(true);
    expect(within(screen.getByRole('tablist', { name: '求人管理の機能' })).getByRole('tab', { name: 'タイムライン' }).getAttribute('aria-selected')).toBe('true');
  });

  it('puts billing CSV rows on the timeline lane and the period table, replacing the HRハッカー row for the same days', async () => {
    vi.stubGlobal('fetch', vi.fn());
    render(<JobCopyScreen />);
    // 取り込む前: demo-001 は HRハッカー実績の 3万円（09-01〜09-14）
    expect(within(periodTable()).getAllByRole('row')[1]?.textContent).toContain('3万円');
    expect(screen.queryByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'データ取込' }));
    upload('媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,DEMO-HRH-001,2026-09-01,2026-09-14,33000\nAirワーク,DEMO-AIR-002,2026-09-05,2026-09-30,40000');
    await screen.findByText('2. 列の対応を確かめる');
    fireEvent.click(screen.getByRole('button', { name: '求人と照合する' }));
    fireEvent.click(screen.getByRole('button', { name: '一致した2行を課金として反映' }));
    await screen.findByText(/課金CSVの 2 期間を反映中/u);
    // demo-001: CSV の 3.3万円 が HRハッカーの 3万円 に置き換わる。後の 2 期間は HRハッカー実績のまま。
    await waitFor(() => { expect(lane('課金').textContent).toContain('3.3万円'); });
    expect([...lane('課金').querySelectorAll('.jt-billing')].map(bar => [bar.className.includes('jt-billing-csv') ? 'csv' : 'hrhacker', bar.textContent])).toEqual([['csv', '3.3万円'], ['hrhacker', '4.5万円'], ['hrhacker', '1.2万円']]);
    expect(screen.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。')).toBeTruthy();
    const firstRow = within(periodTable()).getAllByRole('row')[1];
    expect(firstRow?.querySelectorAll('td')[3]?.textContent).toBe('3.3万円');
    // demo-002（Airワーク）は HRハッカー実績が無く、CSV だけが課金レーンに出る
    const airJob = [...document.querySelectorAll<HTMLButtonElement>('.jc-job')].find(button => button.textContent.includes('倉庫内ピッキングスタッフ'));
    if (!airJob) throw new Error('missing demo-job-002');
    fireEvent.click(airJob);
    await waitFor(() => { expect(lane('課金').textContent).toContain('4万円'); });
    // 横断比較の課金合計にも同じ値が入る
    fireEvent.click(screen.getByRole('button', { name: '横断比較' }));
    const overviewRow = [...within(screen.getByRole('region', { name: '求人の横断比較の表' })).getAllByRole('row')].find(row => row.textContent.includes('倉庫内ピッキングスタッフ'));
    expect(overviewRow?.textContent).toContain('4万円');
    expect(document.body.textContent).not.toMatch(/効果|確実に|必ず|100%/u);
  });
});
