// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CallQueueScreen } from './CallQueueScreen';
import { makeItem, makeResponse, okMetadataFetch, okUserFetch } from './queueTestUtil';
import { DEFAULT_FILTERS } from './queueModel';
import type { QueueFilters } from './queueModel';
import { fixtureQueuePage } from './queueFixture';
import { fixtureDetail } from './workspaceFixture';
import type { DetailFetch } from './useDealDetail';
import { fixtureOwnersFetch } from './useOwners';

// 行・詳細欄・電話欄は描くたびに電話番号を整形する。その回数で「描き直したか」を数える
const phoneCalls = vi.hoisted(() => ({ n: 0 }));
vi.mock('./phone', async importOriginal => {
  const orig = await importOriginal<typeof import('./phone')>();
  return { ...orig, formatPhoneForDisplay: (raw: string | null | undefined) => { phoneCalls.n += 1; return orig.formatPhoneForDisplay(raw); } };
});

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } phoneCalls.n = 0; });
afterEach(() => { cleanup(); });

const list = () => screen.getByRole('list', { name: '架電キュー' });
const rows = () => within(list()).getAllByRole('button');
const rowAt = (i: number): HTMLElement => { const b = rows()[i]; if (!b) throw new Error(`row ${String(i)}`); return b; };

// 一覧は 40 行 (「さらに読み込む」を重ねた長い一覧の代わり)。詳細は架空サンプルの 1 件を ID だけ変えて返す
const ITEMS = Array.from({ length: 40 }, (_, i) => makeItem(String(i + 1)));
const queue = (f: QueueFilters) => Promise.resolve({ ok: true as const, data: makeResponse(f, ITEMS) });
const sampleId = fixtureQueuePage(DEFAULT_FILTERS, null).items[0]?.deal_id ?? '';
const detailFetch: DetailFetch = id => {
  const d = fixtureDetail(sampleId);
  if (d === null) throw new Error('fixture');
  return Promise.resolve({ ok: true, data: { ...d, deal: { ...d.deal, id } } });
};

async function openFirst() {
  render(<CallQueueScreen userFetcher={okUserFetch} fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={detailFetch} metadataFetcher={okMetadataFetch}
    zoomOptions={{ loadTimeoutMs: 60_000 }} initialSearch="?view=queue&owner=all" />);
  await waitFor(() => { expect(rows()).toHaveLength(40); });
  fireEvent.click(rowAt(0));
  const form = await screen.findByRole('form', { name: '架電結果の入力' });
  await within(form).findByRole('group', { name: '今回の結果' });
  // 詳細欄 (電話番号) まで出し切る
  await waitFor(() => { expect(screen.getByRole('region', { name: '選んだ架電先の詳細' }).textContent).toContain('発信'); });
  return form;
}

describe('CallQueueScreen re-render cost', () => {
  it('typing in the call-result form re-renders neither the queue rows nor the detail / phone columns', async () => {
    const form = await openFirst();
    const memo = form.querySelector('textarea');
    if (!memo) throw new Error('memo');
    const rowCount = rows().length;
    phoneCalls.n = 0;
    for (let i = 1; i <= 10; i += 1) fireEvent.change(memo, { target: { value: 'あ'.repeat(i) } });
    expect(memo.value).toBe('ああああああああああ');
    // 修正前は 1 文字ごとに全行 (rowCount 行) と詳細欄を描き直していた
    expect(rowCount).toBe(40);
    expect(phoneCalls.n).toBe(0);
  });

  it('an arrow key re-renders only the two rows whose selection changed', async () => {
    await openFirst();
    rowAt(0).focus();
    phoneCalls.n = 0;
    fireEvent.keyDown(rowAt(0), { key: 'ArrowDown' });
    expect(rows().map(b => b.getAttribute('aria-pressed')).slice(0, 3)).toEqual(['false', 'true', 'false']);
    expect(document.activeElement).toBe(rowAt(1));
    expect(phoneCalls.n).toBe(2);
  });
});
