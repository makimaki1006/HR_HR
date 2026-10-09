// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { CallQueueScreen } from './CallQueueScreen';
import { makeItem, makeResponse, okMetadataFetch, okUserFetch, okCatalogFetch, resetDockStorage, setScreenHeight } from './queueTestUtil';
import type { QueueFilters } from './queueModel';
import { fixtureQueuePage } from './queueFixture';
import { DEFAULT_FILTERS } from './queueModel';
import { fixtureDetail } from './workspaceFixture';
import type { DetailFetch } from './useDealDetail';
import { fixtureOwnersFetch } from './useOwners';
import type { WorkspaceDeal } from '../../generated/WorkspaceDeal';
import { LINK_FRAME_SANDBOX } from './CenterTabs';

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } });
beforeEach(() => { resetDockStorage(); setScreenHeight(900); });
afterEach(() => { cleanup(); });

const STORED = 'https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA&sca_esv=x&ei=y';
const ITEMS = [makeItem('1'), makeItem('2')];
const queue = (f: QueueFilters) => Promise.resolve({ ok: true as const, data: makeResponse(f, ITEMS) });
const sampleId = fixtureQueuePage(DEFAULT_FILTERS, null).items[0]?.deal_id ?? '';

function detailWith(deal: Partial<WorkspaceDeal>): DetailFetch {
  return id => {
    const d = fixtureDetail(sampleId);
    if (d === null) throw new Error('fixture');
    return Promise.resolve({ ok: true, data: { ...d, deal: { ...d.deal, ...deal, id } } });
  };
}

const DEAL_LINKS: Partial<WorkspaceDeal> = {
  job_search_url: STORED,
  homepage_url: 'https://www.example.com/',
  media_job_urls: 'https://app.hubspot.com/contacts/1/record/0-3/1/\njavascript:alert(1)',
  job_posting_url: null,
};

async function openDeal(fetcher: DetailFetch) {
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} fetcher={queue} ownersFetcher={fixtureOwnersFetch} detailFetcher={fetcher} metadataFetcher={okMetadataFetch}
    zoomOptions={{ loadTimeoutMs: 60_000 }} initialSearch="?view=queue&owner=all" />);
  const list = await screen.findByRole('list', { name: '架電キュー' });
  const first = within(list).getAllByRole('button')[0];
  if (!first) throw new Error('row');
  fireEvent.click(first);
  const form = await screen.findByRole('form', { name: '架電結果の入力' });
  await screen.findByRole('region', { name: 'リンク' });
  return form;
}

const tab = (name: string) => screen.getByRole('tab', { name });
const byId = (id: string): HTMLElement => { const e = document.getElementById(id); if (e === null) throw new Error(id); return e; };
const panel = (id: string) => byId(`cq-cpanel-${id}`);
const panelOf = (t: HTMLElement) => byId(t.getAttribute('aria-controls') ?? '');

describe('求人検索・リンク先 panel (links open inside the panel)', () => {
  it('the 求人検索 link opens the search tab with igu=1 in a sandboxed frame; the call-result panel keeps the typed draft', async () => {
    const form = await openDeal(detailWith(DEAL_LINKS));
    const memo = form.querySelector('textarea');
    if (!memo) throw new Error('memo');
    fireEvent.change(memo, { target: { value: '受付で不在。来週再架電' } });

    // 既定の配置 (v2) では「求人検索・リンク先」は右の列にあり、最初から前に出ている
    expect(tab('求人検索・リンク先').getAttribute('aria-selected')).toBe('true');
    expect(byId('dock-panel-links').closest('[data-testid="dock-col-2"]')).not.toBeNull();
    expect(tab('リンク一覧').getAttribute('aria-selected')).toBe('true');
    // 開くまでは Google を読みに行かない
    expect(screen.queryByTestId('link-frame')).toBeNull();
    const links = screen.getByRole('region', { name: 'リンク' });
    const searchLink = within(links).getByRole('link', { name: /求人を検索する/ });
    expect(searchLink.getAttribute('href')).toBe(STORED);
    expect(searchLink.getAttribute('target')).toBe('_blank');
    searchLink.focus();
    fireEvent.click(searchLink);

    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    // 押したリンクは隠れるので、フォーカスは前に出たタブへ移る
    expect(document.activeElement).toBe(tab('求人検索'));
    expect(tab('リンク一覧').getAttribute('aria-selected')).toBe('false');
    // リンクを開いても「求人検索・リンク先」パネルは前に出たまま (中央の列は「活動ログ」が前のままで、「架電結果の入力」は隠れている)
    expect(tab('求人検索・リンク先').getAttribute('aria-selected')).toBe('true');
    const frame = within(panel('search')).getByTestId('link-frame');
    expect(frame.getAttribute('src')).toBe('https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA&igu=1');
    expect(frame.getAttribute('sandbox')).toBe(LINK_FRAME_SANDBOX);
    expect(frame.getAttribute('referrerpolicy')).toBe('no-referrer');
    // 新しいタブは元の URL のまま
    const openNew = within(panel('search')).getByRole('link', { name: '新しいタブで開く' });
    expect(openNew.getAttribute('href')).toBe(STORED);
    expect(openNew.getAttribute('rel')).toBe('noopener noreferrer');
    expect(within(panel('search')).getByText('表示されない場合は新しいタブで開いてください')).toBeTruthy();
    // リンク一覧のタブ・架電結果の入力のパネルは外さずに隠すだけ (入力欄は同じ要素で、操作できない)
    expect(panel('deal').hasAttribute('inert')).toBe(true);
    expect(byId('dock-panel-result').hasAttribute('inert')).toBe(true);
    expect(memo.isConnected).toBe(true);

    fireEvent.click(tab('リンク一覧'));
    expect(panel('deal').hasAttribute('inert')).toBe(false);
    fireEvent.click(tab('架電結果の入力'));
    expect(byId('dock-panel-result').hasAttribute('inert')).toBe(false);
    expect(form.querySelector('textarea')).toBe(memo);
    expect(memo.value).toBe('受付で不在。来週再架電');
    // 戻っても検索の枠は残っている (読み直さない)
    expect(within(panel('search')).getByTestId('link-frame')).toBe(frame);
  });

  it('other links open their own closable tab; HubSpot opens directly in a new browser tab; javascript: is not a link', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    const links = screen.getByRole('region', { name: 'リンク' });
    expect(within(links).queryByText(/javascript:/)).toBeNull();

    fireEvent.click(within(links).getByRole('link', { name: 'https://www.example.com/' }));
    const home = tab('ホームページ');
    expect(home.getAttribute('aria-selected')).toBe('true');
    const homePanel = panelOf(home);
    expect(within(homePanel).getByTestId('link-frame').getAttribute('src')).toBe('https://www.example.com/');

    // HubSpot はパネルのタブを作らず、直接新しいタブで開く (元の URL のまま)
    fireEvent.click(tab('リンク一覧'));
    const hs = within(links).getByRole('link', { name: 'https://app.hubspot.com/contacts/1/record/0-3/1/' });
    expect(hs.getAttribute('target')).toBe('_blank');
    expect(hs.getAttribute('rel')).toBe('noopener noreferrer');
    expect(hs.getAttribute('href')).toBe('https://app.hubspot.com/contacts/1/record/0-3/1/');
    expect(fireEvent.click(hs)).toBe(true); // preventDefault されない = ブラウザが新しいタブで開く
    expect(screen.queryByRole('tab', { name: '求人媒体' })).toBeNull();
    expect(tab('リンク一覧').getAttribute('aria-selected')).toBe('true');

    // ホームページのタブを閉じると、左隣の求人検索に戻る
    fireEvent.click(tab('ホームページ'));
    fireEvent.click(within(panelOf(tab('ホームページ'))).getByRole('button', { name: 'このタブを閉じる' }));
    expect(screen.queryByRole('tab', { name: 'ホームページ' })).toBeNull();
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
  });

  it('with the frame extension, a HubSpot link opens as an embedded tab with the open-in-new-tab button and the login note', async () => {
    document.documentElement.setAttribute('data-hrhr-frames', '1.0');
    try {
      await openDeal(detailWith(DEAL_LINKS));
      const links = screen.getByRole('region', { name: 'リンク' });
      const hsUrl = 'https://app.hubspot.com/contacts/1/record/0-3/1/';
      expect(fireEvent.click(within(links).getByRole('link', { name: hsUrl }))).toBe(false); // preventDefault = パネルで開く
      const hs = tab('求人媒体');
      expect(hs.getAttribute('aria-selected')).toBe('true');
      const p = panelOf(hs);
      expect(within(p).getByTestId('link-frame').getAttribute('src')).toBe(hsUrl);
      expect(within(p).getByRole('link', { name: '新しいタブで開く' }).getAttribute('href')).toBe(hsUrl);
      expect(within(p).getByTestId('hubspot-note').textContent).toBe('HubSpot のログイン画面が繰り返し出る場合は「新しいタブで開く」を使ってください');
      // HubSpot 以外のタブには出ない
      fireEvent.click(tab('リンク一覧'));
      fireEvent.click(within(links).getByRole('link', { name: 'https://www.example.com/' }));
      expect(within(panelOf(tab('ホームページ'))).queryByTestId('hubspot-note')).toBeNull();
    } finally { document.documentElement.removeAttribute('data-hrhr-frames'); }
  });

  it('the tab list works with the keyboard (arrows / Home / End / Delete)', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    fireEvent.click(within(screen.getByRole('region', { name: 'リンク' })).getByRole('link', { name: 'https://www.example.com/' }));
    const list = screen.getByRole('tablist', { name: '求人検索・リンク先の表示' });
    expect(within(list).getAllByRole('tab').map(t => t.textContent)).toEqual(['リンク一覧', '求人検索', 'ホームページ']);
    tab('リンク一覧').focus();
    fireEvent.click(tab('リンク一覧'));
    fireEvent.keyDown(tab('リンク一覧'), { key: 'ArrowRight' });
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(tab('求人検索'));
    expect(tab('求人検索').tabIndex).toBe(0);
    expect(tab('リンク一覧').tabIndex).toBe(-1);
    fireEvent.keyDown(tab('求人検索'), { key: 'End' });
    expect(tab('ホームページ').getAttribute('aria-selected')).toBe('true');
    fireEvent.keyDown(tab('ホームページ'), { key: 'Delete' });
    expect(screen.queryByRole('tab', { name: 'ホームページ' })).toBeNull();
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    fireEvent.keyDown(tab('求人検索'), { key: 'Home' });
    expect(tab('リンク一覧').getAttribute('aria-selected')).toBe('true');
    // リンク一覧・求人検索は Delete で閉じない
    fireEvent.keyDown(tab('リンク一覧'), { key: 'Delete' });
    expect(within(list).getAllByRole('tab')).toHaveLength(2);
  });

  it('switching to another deal closes the opened link tabs and returns to the リンク一覧 tab', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    fireEvent.click(within(screen.getByRole('region', { name: 'リンク' })).getByRole('link', { name: 'https://www.example.com/' }));
    expect(tab('ホームページ').getAttribute('aria-selected')).toBe('true');
    const list = screen.getByRole('list', { name: '架電キュー' });
    const second = within(list).getAllByRole('button')[1];
    if (!second) throw new Error('row');
    fireEvent.click(second);
    await waitFor(() => { expect(screen.queryByRole('tab', { name: 'ホームページ' })).toBeNull(); });
    expect(tab('リンク一覧').getAttribute('aria-selected')).toBe('true');
  });

  it('without a stored search URL, the search uses the dial number', async () => {
    await openDeal(detailWith({ ...DEAL_LINKS, job_search_url: null }));
    const link = within(screen.getByRole('region', { name: 'リンク' })).getByRole('link', { name: /求人を検索する/ });
    expect(link.textContent).toContain('架ける番号で検索');
    expect(new URL(link.getAttribute('href') ?? '').searchParams.get('q')).toMatch(/^0\d{1,4}-\d{1,4}-\d{4} 求人$/u);
  });

  it('a URL in the プロパティ panel opens in the 求人検索・リンク先 panel (same URL as the search → the 求人検索 tab), not a new browser tab', async () => {
    const fetcher: DetailFetch = (id, _signal, props) => {
      const d = fixtureDetail(sampleId, props);
      if (d === null) throw new Error('fixture');
      return Promise.resolve({ ok: true, data: { ...d, deal: { ...d.deal, ...DEAL_LINKS, id }, selected: { ...d.selected, deal: { ...d.selected.deal, bpo_32: STORED } } } });
    };
    await openDeal(fetcher);
    fireEvent.click(tab('プロパティ'));
    const props = screen.getByTestId('property-panel');
    const link = await within(props).findByRole('link', { name: STORED });
    expect(link.getAttribute('href')).toBe(STORED);
    fireEvent.click(link);
    expect(tab('求人検索・リンク先').getAttribute('aria-selected')).toBe('true');
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    expect(within(panel('search')).getByTestId('link-frame').getAttribute('src')).toBe('https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA&igu=1');
    // 左の列の「プロパティ」はそのまま前に出ている (同じ列ではないので隠れない)
    expect(tab('プロパティ').getAttribute('aria-selected')).toBe('true');
  });
});
