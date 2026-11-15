// @vitest-environment happy-dom
// 「この求人に記録する」: 求人から開いたときだけ出る・押す前に確かめる・保存の結果と失敗の理由。
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiNetworkError } from '../../api/client';

vi.mock('./api', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api')>();
  return { ...actual, fetchOptions: vi.fn(), createReport: vi.fn(), downloadPdf: vi.fn(), fetchProgress: vi.fn(), saveToListing: vi.fn() };
});

import { createReport, fetchOptions, fetchProgress, saveToListing } from './api';
import { CompetitorScreen } from './CompetitorScreen';
import { makeResponse } from './fixtures';
import { competitorUrl } from './listingLink';

const mSave = vi.mocked(saveToListing);
const OPTIONS = { titles: ['施設長'], prefectures: ['北海道', '大阪府'], municipalities: { 北海道: ['札幌市中央区'], 大阪府: ['大阪市北区', '堺市堺区'] }, market_available: true };

async function openResult(url: string): Promise<void> {
  window.history.replaceState(null, '', url);
  render(<CompetitorScreen />);
  await screen.findByRole('option', { name: '施設長' });
  fireEvent.change(screen.getByLabelText('求人一覧CSV'), { target: { files: [new File(['a,b\n1,2\n'], 'jobs.csv', { type: 'text/csv' })] } });
  fireEvent.click(screen.getByRole('button', { name: 'レポートを作成' }));
  await screen.findByTestId('result-summary');
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(fetchOptions).mockResolvedValue({ ok: true, data: OPTIONS });
  vi.mocked(fetchProgress).mockResolvedValue({ ok: false, error: new ApiNetworkError() });
  vi.mocked(createReport).mockResolvedValue({ ok: true, data: makeResponse() });
  sessionStorage.clear();
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-11-15T07:20:00Z'));
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const LISTING_URL = competitorUrl({ id: '30', name: '顧客A 配送スタッフ', prefecture: '大阪府', municipality: '大阪市', jobName: '配送ドライバー' });

describe('この求人に記録する', () => {
  it('求人から開くと、職種名と地域が入力欄に入る（選択肢に無い市区町村は外す）', async () => {
    window.history.replaceState(null, '', LISTING_URL);
    render(<CompetitorScreen />);
    await screen.findByRole('option', { name: '施設長' });
    expect(screen.getByLabelText<HTMLInputElement>('職種名').value).toBe('配送ドライバー');
    expect(screen.getByLabelText<HTMLSelectElement>('対象都道府県').value).toBe('大阪府');
    // 「大阪市」は選択肢（大阪市北区・堺市堺区）に無いので、指定しないに戻る。
    await waitFor(() => { expect(screen.getByLabelText<HTMLSelectElement>('対象市区町村').value).toBe(''); });
    expect(screen.getByTestId('listing-target').textContent).toContain('求人「顧客A 配送スタッフ」の競合調査として開いています');
  });

  it('求人の番号が無いときは、ボタンを出さない', async () => {
    await openResult('/app/competitor');
    expect(screen.queryByRole('button', { name: 'この求人に記録する' })).toBeNull();
    expect(screen.queryByTestId('listing-target')).toBeNull();
  });

  it('押すと保存先と月を確かめ、「保存する」で初めて送る。保存後は月と置き換えを伝える', async () => {
    mSave.mockResolvedValue({ ok: true, saved: { listing_id: '30', listing_name: '顧客A 配送スタッフ', month: '2026-11', replaced: true, months: 3 } });
    await openResult(LISTING_URL);
    fireEvent.click(screen.getByRole('button', { name: 'この求人に記録する' }));
    expect(screen.getByTestId('record-confirm').textContent).toContain('HubSpot の求人「顧客A 配送スタッフ」（番号 30）に、11月の競合調査として保存します。同じ月の記録は置き換わります。');
    expect(mSave).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '保存する' }));
    await waitFor(() => {
      expect(screen.getByTestId('record-status').textContent).toBe('求人「顧客A 配送スタッフ」に、11月の競合調査として保存しました。同じ月の前の記録は置き換えました。案件管理の求人のページの「競合の推移」に出ます。');
    });
    // 送るのは求人の番号・レポートの ID だけ（数字・月・CSV の媒体はサーバーがレポートから決める）。
    expect(mSave.mock.calls).toEqual([['30', 'a'.repeat(64)]]);
    expect(screen.queryByTestId('record-confirm')).toBeNull();
    expect(screen.queryByTestId('record-name-differs')).toBeNull();
  });

  it('保存の文は HubSpot から読んだ求人名を使い、開いたときの名前と違えば知らせる', async () => {
    mSave.mockResolvedValue({ ok: true, saved: { listing_id: '30', listing_name: '顧客A 配送スタッフ（夜勤）', month: '2026-11', replaced: false, months: 1 } });
    await openResult(LISTING_URL);
    fireEvent.click(screen.getByRole('button', { name: 'この求人に記録する' }));
    fireEvent.click(screen.getByRole('button', { name: '保存する' }));
    await waitFor(() => {
      expect(screen.getByTestId('record-status').textContent).toBe('求人「顧客A 配送スタッフ（夜勤）」に、11月の競合調査として保存しました。案件管理の求人のページの「競合の推移」に出ます。');
    });
    expect(screen.getByTestId('record-name-differs').textContent).toBe(
      '開いたときの求人名「顧客A 配送スタッフ」と、HubSpot の求人名「顧客A 配送スタッフ（夜勤）」が違います。保存先の求人（番号 30）が合っているか、案件管理で確かめてください。',
    );
  });

  it('確かめる文の月は、保存を押した月ではなくレポートが届いた月', async () => {
    vi.setSystemTime(new Date('2026-10-31T14:50:00Z')); // 日本時間 10/31 23:50
    await openResult(LISTING_URL);
    vi.setSystemTime(new Date('2026-10-31T15:10:00Z')); // 日本時間 11/1 0:10
    fireEvent.click(screen.getByRole('button', { name: 'この求人に記録する' }));
    expect(screen.getByTestId('record-confirm').textContent).toContain('10月の競合調査として保存します');
  });

  it('「やめる」では送らない', async () => {
    await openResult(LISTING_URL);
    fireEvent.click(screen.getByRole('button', { name: 'この求人に記録する' }));
    fireEvent.click(screen.getByRole('button', { name: 'やめる' }));
    expect(screen.queryByTestId('record-confirm')).toBeNull();
    expect(mSave).not.toHaveBeenCalled();
  });

  it('保存できないときは理由を出す', async () => {
    mSave.mockResolvedValue({ ok: false, kind: 'server', message: 'この求人の競合調査の記録が、HubSpot に入る量の上限に近づいているため保存できません。何も保存されていません。開発の担当に知らせてください。' });
    await openResult(LISTING_URL);
    fireEvent.click(screen.getByRole('button', { name: 'この求人に記録する' }));
    fireEvent.click(screen.getByRole('button', { name: '保存する' }));
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('上限に近づいているため保存できません。何も保存されていません。');
    expect(screen.getByTestId('record-status').textContent).toBe('');
  });
});
