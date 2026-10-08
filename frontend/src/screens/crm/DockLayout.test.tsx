// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ApiResult } from '../../api/client';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { CallQueueScreen } from './CallQueueScreen';
import { DEFAULT_LAYOUT, DOCK_STORAGE_KEY, LEGACY_DOCK_STORAGE_KEY, dockReducer, serializeLayout } from './dockModel';
import { DEFAULT_SELECTED, PROPS_STORAGE_KEY } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import { makeItem, makeResponse, okCatalogFetch, okMetadataFetch, okPipelinesFetch, okUserFetch, resetDockStorage, setScreenHeight } from './queueTestUtil';
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

beforeEach(() => { resetDockStorage(); setScreenHeight(900); try { window.sessionStorage.clear(); } catch { /* ignore */ } });
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
  await within(screen.getByTestId('property-panel')).findByText('架電日');
  return r;
}

const tab = (name: string) => screen.getByRole('tab', { name });
const columnOfPanel = (id: string) => document.getElementById(`dock-panel-${id}`)?.closest('[data-testid^="dock-col-"]')?.getAttribute('data-testid');
function moveVia(panel: string, label: string) {
  fireEvent.click(screen.getByRole('button', { name: `「${panel}」の移動` }));
  fireEvent.click(within(screen.getByRole('menu', { name: `「${panel}」の移動` })).getByRole('menuitem', { name: label }));
}

describe('panel layout (dock)', () => {
  it('default (v2): left = 架電一覧 (front) + プロパティ, center = 案件の概要 fixed on top + 活動ログ (front) / 架電結果の入力, right = 求人検索・リンク先', async () => {
    const c = counted();
    await openDeal(c);
    const left = within(screen.getByRole('tablist', { name: '左の列のパネル' })).getAllByRole('tab');
    expect(left.map(t => [t.textContent, t.getAttribute('aria-selected')])).toEqual([['架電一覧', 'true'], ['プロパティ', 'false']]);
    const center = within(screen.getByRole('tablist', { name: '中央の列のパネル' })).getAllByRole('tab');
    expect(center.map(t => [t.textContent, t.getAttribute('aria-selected')])).toEqual([['活動ログ', 'true'], ['架電結果の入力', 'false']]);
    const right = within(screen.getByRole('tablist', { name: '右の列のパネル' })).getAllByRole('tab');
    expect(right.map(t => [t.textContent, t.getAttribute('aria-selected')])).toEqual([['求人検索・リンク先', 'true']]);
    expect(screen.getByTestId('dock-pinned').closest('[data-testid="dock-col-1"]')).not.toBeNull();
    expect(within(screen.getByTestId('dock-pinned')).getByRole('article', { name: '架電先の詳細' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: '右の列(空き)' })).toBeNull();
    // 幅の比: 左 0.21 / 中央 0.34 / 右 0.45 (1440px で 左 ≈ 300px、右 ≈ 640px)
    expect(screen.getByTestId('dock').style.gridTemplateColumns).toBe('minmax(220px, 0.21fr) 6px minmax(220px, 0.34fr) 6px minmax(220px, 0.45fr)');
    // 初めて開いた (以前の配置が無い) ときは案内を出さない
    expect(screen.queryByTestId('layout-note')).toBeNull();
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
    // 左と中央の 2 列のうち左が 0.21 / 0.55 = 38%
    expect(sep.getAttribute('aria-valuenow')).toBe('38');
    fireEvent.keyDown(sep, { key: 'ArrowRight' });
    fireEvent.keyDown(sep, { key: 'ArrowRight' });
    expect(sep.getAttribute('aria-valuenow')).toBe('45');
    expect(sep.getAttribute('aria-valuetext')).toBe('左の列 45%');
    fireEvent.keyDown(sep, { key: 'ArrowLeft' });
    expect(sep.getAttribute('aria-valuenow')).toBe('42');
    // 最小幅で止まる
    fireEvent.keyDown(sep, { key: 'Home' });
    expect(Number(sep.getAttribute('aria-valuenow'))).toBeGreaterThanOrEqual(12);
    const grid = screen.getByTestId('dock').style.gridTemplateColumns;
    expect(grid).toMatch(/^minmax\(220px, 0\.12\d*fr\) 6px minmax\(220px, 0\.43\d*fr\) 6px minmax\(220px, 0\.45fr\)$/);
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
    window.localStorage.setItem(DOCK_STORAGE_KEY, '{"v":2,"columns":[{"panels":["queue","queue"]}]');
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
    const right = screen.getByRole('region', { name: '右の列' });
    fireEvent.dragOver(right, { dataTransfer });
    fireEvent.drop(right, { dataTransfer });
    expect(columnOfPanel('properties')).toBe('dock-col-2');
    expect(tab('プロパティ').getAttribute('aria-selected')).toBe('true');
    expect(c.n).toEqual(before);
  });
});

describe('layout v2: migration, maximize, compact summary', () => {
  it('a v1 layout is replaced once by the new default (links on the right) with a notice; the v1 key goes away and the notice can be closed', async () => {
    const v1 = JSON.stringify({ v: 1, columns: [{ panels: ['queue', 'properties'], active: 'queue' }, { panels: ['overview', 'activity', 'result', 'links'], active: 'result' }, { panels: [], active: null }], widths: [0.26, 0.74, 0.3] });
    window.localStorage.setItem(LEGACY_DOCK_STORAGE_KEY, v1);
    await openDeal(counted());
    expect(columnOfPanel('links')).toBe('dock-col-2');
    const note = screen.getByTestId('layout-note');
    expect(note.getAttribute('role')).toBe('status');
    expect(note.textContent).toContain('画面の配置を更新しました。「求人検索・リンク先」を右の列に移し');
    expect(window.localStorage.getItem(LEGACY_DOCK_STORAGE_KEY)).toBeNull();
    expect((JSON.parse(window.localStorage.getItem(DOCK_STORAGE_KEY) ?? '{}') as { v: number }).v).toBe(2);
    fireEvent.click(within(note).getByRole('button', { name: '閉じる' }));
    expect(screen.queryByTestId('layout-note')).toBeNull();
    // 次に開いたときは案内しない
    cleanup();
    await openDeal(counted());
    expect(screen.queryByTestId('layout-note')).toBeNull();
  });

  it('広げる lays the link panel over the whole dock without moving it (same iframe element, other columns inert); Esc and 戻す restore it', async () => {
    const c = counted();
    await openDeal(c);
    fireEvent.click(within(screen.getByRole('region', { name: 'リンク' })).getByRole('link', { name: /求人を検索する/ }));
    const frame = within(document.getElementById('cq-cpanel-search') ?? document.body).getByTestId('link-frame');
    const zoomFrame = screen.getByTitle('Zoom Phone');
    const before = { ...c.n };
    const toggle = screen.getByTestId('links-maximize');
    expect(toggle.textContent).toBe('広げる');
    fireEvent.click(toggle);
    const rightCol = screen.getByTestId('dock-col-2');
    expect(rightCol.classList.contains('is-max')).toBe(true);
    expect(screen.getByTestId('dock').classList.contains('has-max')).toBe(true);
    expect(screen.getByTestId('dock-col-0').hasAttribute('inert')).toBe(true);
    expect(screen.getByTestId('dock-col-1').hasAttribute('inert')).toBe(true);
    expect(rightCol.hasAttribute('inert')).toBe(false);
    expect(toggle.textContent).toBe('戻す');
    // 枠は同じ要素のまま (読み直さない)
    expect(within(rightCol).getByTestId('link-frame')).toBe(frame);
    expect(frame.isConnected).toBe(true);
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(rightCol.classList.contains('is-max')).toBe(false);
    expect(screen.getByTestId('dock-col-0').hasAttribute('inert')).toBe(false);
    expect(within(rightCol).getByTestId('link-frame')).toBe(frame);
    // もう一度広げて「戻す」でも戻る
    fireEvent.click(toggle);
    expect(rightCol.classList.contains('is-max')).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '戻す' }));
    expect(rightCol.classList.contains('is-max')).toBe(false);
    expect(within(rightCol).getByTestId('link-frame')).toBe(frame);
    expect(screen.getByTitle('Zoom Phone')).toBe(zoomFrame);
    // 広げても通信は起きない
    await act(async () => { await Promise.resolve(); });
    expect(c.n).toEqual(before);
  });

  it('on a short screen the summary is one line (company · contact · hyphenated number · 発信); 詳しく表示 expands it and 1 行にする folds it again', async () => {
    setScreenHeight(720);
    await openDeal(counted());
    const compact = screen.getByTestId('overview-compact');
    expect(compact.getAttribute('aria-label')).toBe('架電先の詳細');
    const fixture = fixtureDetail(sampleId);
    expect(within(compact).getByRole('heading', { level: 2 }).textContent).toBe(fixture?.companies[0]?.name);
    expect(compact.textContent).toContain(fixture?.contacts[0]?.name ?? '?');
    expect(compact.querySelector('.cq-phone')?.textContent).toBe('03-0000-0005');
    const dial = within(compact).getByRole('button', { name: /に発信$/ });
    expect(dial.textContent).toBe('発信');
    // 1 行ではコピー・端末の電話は出さない
    expect(within(compact).queryByRole('button', { name: /番号をコピー/ })).toBeNull();
    fireEvent.click(within(compact).getByRole('button', { name: '詳しく表示' }));
    const full = screen.getByRole('article', { name: '架電先の詳細' });
    expect(full.getAttribute('data-testid')).toBeNull();
    expect(within(full).getByRole('region', { name: '架ける番号' })).toBeTruthy();
    expect(within(full).getAllByRole('button', { name: /番号をコピー/ }).length).toBeGreaterThan(0);
    expect(within(full).getByRole('button', { name: '1 行にする' }).getAttribute('aria-expanded')).toBe('true');
    fireEvent.click(within(full).getByRole('button', { name: '1 行にする' }));
    expect(screen.getByTestId('overview-compact')).toBeTruthy();
  });

  it('on a tall screen (or without matchMedia) the summary is the full block', async () => {
    setScreenHeight(900);
    await openDeal(counted());
    expect(screen.queryByTestId('overview-compact')).toBeNull();
    expect(within(screen.getByRole('article', { name: '架電先の詳細' })).getByRole('region', { name: '架ける番号' })).toBeTruthy();
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

  it('a whole group can be picked with one checkbox (partly chosen = mixed), and 既定に戻す restores the default', async () => {
    await openDeal(counted());
    fireEvent.click(tab('プロパティ'));
    fireEvent.click(screen.getByRole('button', { name: '表示する項目を選ぶ' }));
    const picker = screen.getByRole('region', { name: '表示する項目を選ぶ' });
    const all = within(picker).getByRole<HTMLInputElement>('checkbox', { name: '「Deal Stage Properties」の項目をまとめて選ぶ' });
    expect(all.checked).toBe(false);
    const info = within(picker).getByRole<HTMLInputElement>('checkbox', { name: '「Deal information」の項目をまとめて選ぶ' });
    expect(info.checked).toBe(false);
    expect(info.indeterminate).toBe(true);
    fireEvent.click(info);
    expect(info.checked).toBe(true);
    fireEvent.click(within(picker).getByRole('button', { name: '既定に戻す' }));
    expect(info.checked).toBe(false);
    expect(within(picker).getByRole('button', { name: 'この項目で表示する' }).hasAttribute('disabled')).toBe(true);
  });

  it('「HubSpotのカードから選ぶ」 picks a whole HubSpot card; unpicking リスト情報 leaves BPOアポ情報 partly chosen; 既定に戻す brings both back', async () => {
    const c = counted();
    await openDeal(c);
    fireEvent.click(tab('プロパティ'));
    fireEvent.click(screen.getByRole('button', { name: '表示する項目を選ぶ' }));
    const picker = screen.getByRole('region', { name: '表示する項目を選ぶ' });
    const presets = within(picker).getByRole('group', { name: 'HubSpotのカードから選ぶ' });
    // カードの選択は種類の切り替えより上にある
    expect(picker.firstElementChild).toBe(presets);
    const list = within(presets).getByRole<HTMLInputElement>('checkbox', { name: 'HubSpot のカード「リスト情報」の項目をまとめて選ぶ' });
    const bpo = within(presets).getByRole<HTMLInputElement>('checkbox', { name: 'HubSpot のカード「BPOアポ情報」の項目をまとめて選ぶ' });
    expect([list.checked, bpo.checked]).toEqual([true, true]);
    expect(presets.textContent).toContain('リスト情報(43 項目)');
    expect(presets.textContent).toContain('BPOアポ情報(25 項目)');
    fireEvent.click(list);
    // 担当者名などは両方のカードにあるので、BPOアポ情報は一部だけ選んだ状態になる
    expect([list.checked, bpo.checked, bpo.indeterminate]).toEqual([false, false, true]);
    fireEvent.click(within(picker).getByRole('button', { name: 'この項目で表示する' }));
    await waitFor(() => { expect(c.n.detail).toBe(2); });
    expect(c.props[1]?.deals).toEqual(['bpo_appo_date', 'scheduled_business_meeting_date', 'negotiation_type', 'bpo_20251', 'bpo_24', 'bpo_20252', 'bpo_2', 'bpo_1',
      'recruitment_issues', 'bpo_20253', 'remarks_after_calling', 'charge_impression', 'bpo_29', 'bpo_apo_rikulogi', 'bpo_transactio_id', 'bpo_hsurl',
      'nkadainofukasa', 'tjikanjiku', 'ckyougoujoukyou', 'ahkessaifuro']);
    // リスト情報は出さない (選んだ項目が 0)
    const panel = screen.getByTestId('property-panel');
    await waitFor(() => { expect(within(panel).queryByTestId('pp-card-list_info')).toBeNull(); });
    fireEvent.click(within(panel).getByRole('button', { name: '表示する項目を選ぶ' }));
    const again = screen.getByRole('region', { name: '表示する項目を選ぶ' });
    fireEvent.click(within(again).getByRole('button', { name: '既定に戻す' }));
    fireEvent.click(within(again).getByRole('button', { name: 'この項目で表示する' }));
    await waitFor(() => { expect(c.n.detail).toBe(3); });
    expect(c.props[2]).toEqual(DEFAULT_SELECTED);
  });

  it('the default panel shows the HubSpot cards リスト情報 (open) and BPOアポ情報 (closed) in HubSpot order, with HubSpot labels only', async () => {
    await openDeal(counted());
    fireEvent.click(tab('プロパティ'));
    const panel = screen.getByTestId('property-panel');
    const list = within(panel).getByRole('region', { name: 'リスト情報' });
    const bpo = within(panel).getByRole('region', { name: 'BPOアポ情報' });
    // HubSpot と同じ順 (リスト情報 → BPOアポ情報)
    expect([...panel.querySelectorAll('.pp-card')].map(el => el.getAttribute('aria-label'))).toEqual(['リスト情報', 'BPOアポ情報']);
    const listToggle = within(list).getByRole('button', { name: /リスト情報/ });
    const bpoToggle = within(bpo).getByRole('button', { name: /BPOアポ情報/ });
    expect([listToggle.getAttribute('aria-expanded'), bpoToggle.getAttribute('aria-expanded')]).toEqual(['true', 'false']);
    const labels = [...list.querySelectorAll('dt')].map(d => d.textContent);
    expect(labels).toHaveLength(43);
    expect(labels.slice(0, 5)).toEqual(['URL_求人検索 ※編集不可', '架電日', '再架電日', '再架電時間', '激アツメモ（決済・利用サービス・決算・職種など）']);
    // 値: 日付は年月日、空は未入力、URL はリンク (パネルに開く)
    const value = (label: string) => [...list.querySelectorAll('dt')].find(d => d.textContent === label)?.nextElementSibling?.textContent;
    expect(value('架電日')).toBe('2026/09/25');
    expect(value('利用中サービス')).toBe('未入力');
    expect(within(list).getAllByRole('link').map(a => a.getAttribute('href'))).toEqual(expect.arrayContaining(['https://www.example.com/job/1', 'https://www.example.com/job/2']));
    // BPOアポ情報は閉じている。開くと HubSpot の並びで出る
    expect(bpo.querySelectorAll('dt')).toHaveLength(0);
    fireEvent.click(bpoToggle);
    expect(bpoToggle.getAttribute('aria-expanded')).toBe('true');
    expect([...bpo.querySelectorAll('dt')].map(d => d.textContent).slice(0, 3)).toEqual(['BPOアポ取得日', '商談予定日時', '商談種別']);
    expect(within(bpo).getByRole('link', { name: 'https://app.hubspot.com/contacts/0/record/0-3/1' })).toBeTruthy();
    // 内部名は画面に出さない
    expect(panel.textContent).not.toMatch(/risuto_|bpo_|recruit_media|tanntousya/u);
    // カードに無い項目は無いので「そのほかの項目」は出さない
    expect(within(panel).queryByRole('region', { name: '案件(そのほかの項目)' })).toBeNull();
  });

  it('the live request puts the chosen names in the query string of the same workspace URL', async () => {
    const urls: string[] = [];
    vi.stubGlobal('fetch', vi.fn((u: string) => { urls.push(u); return Promise.resolve(new Response('{}', { status: 500, headers: { 'content-type': 'application/json' } })); }));
    await liveDetailFetch('123', new AbortController().signal, { deals: ['bpo_10', 'bpo_32'], contacts: ['phone'], companies: [] });
    expect(urls).toEqual(['/api/crm/workspace/deals/123?deal_props=bpo_10%2Cbpo_32&contact_props=phone']);
  });
});
