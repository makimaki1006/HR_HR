// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApiResult } from '../../api/client';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { DEFAULT_LAYOUT, DOCK_STORAGE_KEY, dockReducer, serializeLayout } from './dockModel';
import { DEFAULT_SELECTED, PROPS_STORAGE_KEY } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import { makeItem, makeResponse, okCatalogFetch, okMetadataFetch, okPipelinesFetch, okUserFetch, resetDockStorage } from './queueTestUtil';
import type { QueueFilters } from './queueModel';
import { fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS } from './queueModel';
import { fixtureDetail } from './workspaceFixture';
import { liveDetailFetch } from './useDealDetail';
import type { DetailFetch } from './useDealDetail';
import type { CatalogFetch } from './usePropertyCatalog';
import { fixtureOwnersFetch } from './useOwners';
import type { OwnersFetch } from './useOwners';
import type { MetadataFetch } from './useResultDefinitions';

beforeEach(() => { resetDockStorage(); try { window.sessionStorage.clear(); } catch { /* ignore */ } });
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const sampleId = fixtureQueuePage(DEFAULT_FILTERS, null).items[0]?.deal_id ?? '';

/** 通信の回数を数える (配置を変えても増えないことを確かめる) */
function counted() {
  const n = { queue: 0, detail: 0, catalog: 0, metadata: 0, owners: 0 };
  const props: (SelectedProps | undefined)[] = [];
  const queue = (f: QueueFilters) => { n.queue += 1; return Promise.resolve<ApiResult<CallQueueResponse>>({ ok: true, data: makeResponse(f, [makeItem('1'), makeItem('2')]) }); };
  const detail: DetailFetch = (id, _signal, p) => {
    n.detail += 1;
    props.push(p);
    const d = fixtureDetail(sampleId, p);
    if (d === null) throw new Error('fixture');
    return Promise.resolve({ ok: true, data: { ...d, deal: { ...d.deal, id } } });
  };
  const catalog: CatalogFetch = s => { n.catalog += 1; return okCatalogFetch(s); };
  const metadata: MetadataFetch = s => { n.metadata += 1; return okMetadataFetch(s); };
  const owners: OwnersFetch = s => { n.owners += 1; return fixtureOwnersFetch(s); };
  return { n, props, queue, detail, catalog, metadata, owners };
}

async function openDeal(c: ReturnType<typeof counted>) {
  const r = render(<CallQueueScreen catalogFetcher={c.catalog} userFetcher={okUserFetch} fetcher={c.queue} ownersFetcher={c.owners} detailFetcher={c.detail}
    metadataFetcher={c.metadata} pipelinesFetcher={okPipelinesFetch} zoomOptions={{ loadTimeoutMs: 60_000 }} initialSearch="?view=queue&owner=all" />);
  const list = await screen.findByRole('list', { name: '架電キュー' });
  fireEvent.click(within(list).getAllByRole('button')[0] ?? list);
  await screen.findByRole('region', { name: '活動ログ' });
  await within(screen.getByTestId('property-panel')).findByText('次回架電日');
  return r;
}

const tab = (name: string) => screen.getByRole('tab', { name });
const columnOfPanel = (id: string) => document.getElementById(`dock-panel-${id}`)?.closest('[data-testid^="dock-col-"]')?.getAttribute('data-testid');
function moveVia(panel: string, label: string) {
  fireEvent.click(screen.getByRole('button', { name: `「${panel}」の移動` }));
  fireEvent.click(within(screen.getByRole('menu', { name: `「${panel}」の移動` })).getByRole('menuitem', { name: label }));
}

describe('panel layout (dock)', () => {
  it('default: left = 架電一覧 (front) + プロパティ, center = 案件の概要 fixed on top + 活動ログ (front) / 架電結果の入力 / 求人検索・リンク先, right = empty strip', async () => {
    const c = counted();
    await openDeal(c);
    const left = within(screen.getByRole('tablist', { name: '左の列のパネル' })).getAllByRole('tab');
    expect(left.map(t => [t.textContent, t.getAttribute('aria-selected')])).toEqual([['架電一覧', 'true'], ['プロパティ', 'false']]);
    const center = within(screen.getByRole('tablist', { name: '中央の列のパネル' })).getAllByRole('tab');
    expect(center.map(t => [t.textContent, t.getAttribute('aria-selected')])).toEqual([['活動ログ', 'true'], ['架電結果の入力', 'false'], ['求人検索・リンク先', 'false']]);
    expect(screen.getByTestId('dock-pinned').closest('[data-testid="dock-col-1"]')).not.toBeNull();
    expect(within(screen.getByTestId('dock-pinned')).getByRole('article', { name: '架電先の詳細' })).toBeTruthy();
    expect(screen.getByRole('region', { name: '右の列(空き)' })).toBeTruthy();
    // 架電先を選んでも、左の列は「架電一覧」のまま
    expect(tab('架電一覧').getAttribute('aria-selected')).toBe('true');
  });

  it('moving 活動ログ to the left via the menu: no HubSpot call, the panel keeps its state (filter), the typed memo and the Zoom iframe stay the same elements', async () => {
    const c = counted();
    await openDeal(c);
    const zoomFrame = screen.getByTitle('Zoom Phone');
    fireEvent.click(tab('架電結果の入力'));
    const form = await screen.findByRole('form', { name: '架電結果の入力' });
    await within(form).findByRole('group', { name: '今回の結果' });
    const memo = form.querySelector('textarea');
    if (!memo) throw new Error('memo');
    fireEvent.change(memo, { target: { value: '受付で不在' } });
    fireEvent.click(tab('活動ログ'));
    const log = screen.getByRole('region', { name: '活動ログ' });
    fireEvent.click(within(log).getByRole('button', { name: 'メモ' }));
    const before = { ...c.n };

    moveVia('活動ログ', '左へ移動');
    expect(columnOfPanel('activity')).toBe('dock-col-0');
    expect(tab('活動ログ').getAttribute('aria-selected')).toBe('true');
    expect(within(screen.getByRole('tablist', { name: '左の列のパネル' })).getAllByRole('tab').map(t => t.textContent)).toEqual(['架電一覧', 'プロパティ', '活動ログ']);
    // 中央は残りの先頭 (架電結果の入力) が前に出る
    expect(tab('架電結果の入力').getAttribute('aria-selected')).toBe('true');
    // 中身の状態はそのまま (同じ要素・選んだ絞り込み・入力途中のメモ)
    expect(screen.getByRole('region', { name: '活動ログ' })).toBe(log);
    expect(within(log).getByRole('button', { name: 'メモ' }).getAttribute('aria-pressed')).toBe('true');
    expect(form.querySelector('textarea')).toBe(memo);
    expect(memo.value).toBe('受付で不在');
    // Zoom の枠は作り直さない (通話が切れない)
    expect(screen.getByTitle('Zoom Phone')).toBe(zoomFrame);
    expect(zoomFrame.isConnected).toBe(true);

    // 右へ移し、元の配置に戻す
    moveVia('活動ログ', '右へ移動');
    expect(columnOfPanel('activity')).toBe('dock-col-2');
    moveVia('案件の概要', '左へ移動');
    expect(screen.getByTestId('dock-pinned').closest('[data-testid="dock-col-0"]')).not.toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '元の配置に戻す' }));
    expect(columnOfPanel('activity')).toBe('dock-col-1');
    expect(screen.getByTestId('dock-pinned').closest('[data-testid="dock-col-1"]')).not.toBeNull();
    expect(screen.getByTitle('Zoom Phone')).toBe(zoomFrame);
    // 配置の変更では何も読み直さない
    await act(async () => { await Promise.resolve(); });
    expect(c.n).toEqual(before);
  });

  it('the divider between columns resizes with arrow keys (keyboard accessible separator), is saved per browser and calls nothing', async () => {
    const c = counted();
    await openDeal(c);
    const before = { ...c.n };
    const sep = screen.getByRole('separator', { name: '左の列と中央の列の幅' });
    expect(sep.tabIndex).toBe(0);
    expect(sep.getAttribute('aria-orientation')).toBe('vertical');
    expect(sep.getAttribute('aria-valuenow')).toBe('26');
    fireEvent.keyDown(sep, { key: 'ArrowRight' });
    fireEvent.keyDown(sep, { key: 'ArrowRight' });
    expect(sep.getAttribute('aria-valuenow')).toBe('30');
    expect(sep.getAttribute('aria-valuetext')).toBe('左の列 30%');
    fireEvent.keyDown(sep, { key: 'ArrowLeft' });
    expect(sep.getAttribute('aria-valuenow')).toBe('28');
    // 最小幅で止まる
    fireEvent.keyDown(sep, { key: 'Home' });
    expect(Number(sep.getAttribute('aria-valuenow'))).toBeGreaterThanOrEqual(12);
    const grid = screen.getByTestId('dock').style.gridTemplateColumns;
    expect(grid).toMatch(/^minmax\(220px, 0\.12\d*fr\) 6px minmax\(220px, 0\.8\d*fr\) 28px$/);
    const stored = JSON.parse(window.localStorage.getItem(DOCK_STORAGE_KEY) ?? '{}') as { widths: number[] };
    expect(stored.widths[0]).toBeCloseTo(0.12);
    expect(c.n).toEqual(before);
  });

  it('restores the saved layout on open; a corrupted saved layout falls back to the default', async () => {
    const saved = dockReducer(DEFAULT_LAYOUT, { type: 'move', panel: 'activity', to: 0 });
    window.localStorage.setItem(DOCK_STORAGE_KEY, serializeLayout(saved));
    const c = counted();
    await openDeal(c);
    expect(columnOfPanel('activity')).toBe('dock-col-0');
    cleanup();
    window.localStorage.setItem(DOCK_STORAGE_KEY, '{"v":1,"columns":[{"panels":["queue","queue"]}]');
    await openDeal(counted());
    expect(columnOfPanel('activity')).toBe('dock-col-1');
    expect(columnOfPanel('queue')).toBe('dock-col-0');
  });

  it('drag and drop a tab onto another column moves it there (no HubSpot call)', async () => {
    const c = counted();
    await openDeal(c);
    const before = { ...c.n };
    const data = new Map<string, string>();
    const dataTransfer = {
      setData: (k: string, v: string) => { data.set(k, v); }, getData: (k: string) => data.get(k) ?? '',
      get types() { return [...data.keys()]; }, dropEffect: 'none', effectAllowed: 'all',
    };
    const source = tab('プロパティ').closest('[draggable="true"]');
    if (!source) throw new Error('draggable');
    fireEvent.dragStart(source, { dataTransfer });
    const right = screen.getByRole('region', { name: '右の列(空き)' });
    fireEvent.dragOver(right, { dataTransfer });
    fireEvent.drop(right, { dataTransfer });
    expect(columnOfPanel('properties')).toBe('dock-col-2');
    expect(tab('プロパティ').getAttribute('aria-selected')).toBe('true');
    expect(c.n).toEqual(before);
  });
});

describe('selected properties are read in the same detail request', () => {
  it('the detail request carries the chosen properties; applying a new choice re-reads once with the new list and saves it', async () => {
    const c = counted();
    await openDeal(c);
    expect(c.n.detail).toBe(1);
    expect(c.props[0]).toEqual(DEFAULT_SELECTED);
    fireEvent.click(tab('プロパティ'));
    const panel = screen.getByTestId('property-panel');
    fireEvent.click(within(panel).getByRole('button', { name: '表示する項目を選ぶ' }));
    const picker = screen.getByRole('region', { name: '表示する項目を選ぶ' });
    // グループを開いて 1 項目足す
    fireEvent.click(within(picker).getByRole('button', { name: /Deal revenue/ }));
    fireEvent.click(within(picker).getByRole('checkbox', { name: '金額' }));
    // 名前で探す
    fireEvent.change(within(picker).getByLabelText('項目名で探す'), { target: { value: '架電' } });
    expect(within(picker).queryByRole('checkbox', { name: '金額' })).toBeNull();
    expect(within(picker).getByRole('checkbox', { name: '次回架電日' })).toBeTruthy();
    expect(c.n.detail).toBe(1);
    fireEvent.click(within(picker).getByRole('button', { name: 'この項目で表示する' }));
    await waitFor(() => { expect(c.n.detail).toBe(2); });
    expect(c.props[1]?.deals).toEqual([...DEFAULT_SELECTED.deals, 'amount']);
    expect((JSON.parse(window.localStorage.getItem(PROPS_STORAGE_KEY) ?? '{}') as { deals: string[] }).deals).toContain('amount');
    await within(screen.getByTestId('property-panel')).findByText('金額');
    expect(within(screen.getByTestId('property-panel')).getByText('120,000')).toBeTruthy();
  });

  it('a whole group can be picked with one checkbox (partly chosen = mixed), and 既定の項目に戻す restores the default', async () => {
    await openDeal(counted());
    fireEvent.click(tab('プロパティ'));
    fireEvent.click(screen.getByRole('button', { name: '表示する項目を選ぶ' }));
    const picker = screen.getByRole('region', { name: '表示する項目を選ぶ' });
    const all = within(picker).getByRole<HTMLInputElement>('checkbox', { name: '「Deal Stage Properties」の項目をまとめて選ぶ' });
    expect(all.checked).toBe(true);
    const info = within(picker).getByRole<HTMLInputElement>('checkbox', { name: '「Deal information」の項目をまとめて選ぶ' });
    expect(info.checked).toBe(false);
    expect(info.indeterminate).toBe(true);
    fireEvent.click(info);
    expect(info.checked).toBe(true);
    fireEvent.click(within(picker).getByRole('button', { name: '既定の項目に戻す' }));
    expect(info.checked).toBe(false);
    expect(within(picker).getByRole('button', { name: 'この項目で表示する' }).hasAttribute('disabled')).toBe(true);
  });

  it('the live request puts the chosen names in the query string of the same workspace URL', async () => {
    const urls: string[] = [];
    vi.stubGlobal('fetch', vi.fn((u: string) => { urls.push(u); return Promise.resolve(new Response('{}', { status: 500, headers: { 'content-type': 'application/json' } })); }));
    await liveDetailFetch('123', new AbortController().signal, { deals: ['bpo_10', 'bpo_32'], contacts: ['phone'], companies: [] });
    expect(urls).toEqual(['/api/crm/workspace/deals/123?deal_props=bpo_10%2Cbpo_32&contact_props=phone']);
  });
});
