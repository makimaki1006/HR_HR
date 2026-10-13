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
  // 詳細を読み終えると、最初に前に出る「求人検索」の枠ができる
  await screen.findByTestId('link-frame');
  return form;
}

const tab = (name: string) => screen.getByRole('tab', { name });
const byId = (id: string): HTMLElement => { const e = document.getElementById(id); if (e === null) throw new Error(id); return e; };
const panel = (id: string) => byId(`cq-cpanel-${id}`);
const tabNames = () => within(screen.getByRole('tablist', { name: '求人検索・リンク先の表示' })).getAllByRole('tab').map(t => t.textContent);
/** 「プロパティ」パネルで、値が URL の項目 (URL_求人検索) のリンクを押す */
async function clickPropertyLink(url: string) {
  fireEvent.click(tab('プロパティ'));
  const link = await within(screen.getByTestId('property-panel')).findByRole('link', { name: url });
  fireEvent.click(link);
}
/** 案件の「URL_求人検索」(bpo_32) の項目の値を差し替えた詳細 (プロパティ パネルのリンク用) */
function detailWithProp(deal: Partial<WorkspaceDeal>, bpo32: string, companyWebsite: string | null = null): DetailFetch {
  return (id, _signal, props) => {
    const d = fixtureDetail(sampleId, props);
    if (d === null) throw new Error('fixture');
    return Promise.resolve({ ok: true, data: {
      ...d, deal: { ...d.deal, ...deal, id },
      companies: d.companies.map(c => ({ ...c, website: companyWebsite })),
      selected: { ...d.selected, deal: { ...d.selected.deal, bpo_32: bpo32 } },
    } });
  };
}

describe('求人検索・リンク先 panel (flat tabs)', () => {
  it('shows 求人検索 / ホームページ / 求人票 (+ 求人媒体) in this order, with 求人検索 selected; no リンク一覧 tab', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    expect(tabNames()).toEqual(['求人検索', 'ホームページ', '求人票', '求人媒体']);
    expect(screen.queryByRole('tab', { name: 'リンク一覧' })).toBeNull();
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    // 外側の見出しのタブは出さない (パネルが 1 つだけの列)
    expect(screen.queryByRole('tab', { name: '求人検索・リンク先' })).toBeNull();
    expect(byId('dock-panel-links').closest('[data-testid="dock-col-2"]')).not.toBeNull();
    // 固定のタブは閉じるボタンを持たない
    expect(screen.queryByRole('button', { name: /のタブを閉じる/ })).toBeNull();
  });

  it('the 求人検索 tab shows the sandboxed Google frame with igu=1; the call-result panel keeps the typed draft', async () => {
    const form = await openDeal(detailWith(DEAL_LINKS));
    const memo = form.querySelector('textarea');
    if (!memo) throw new Error('memo');
    fireEvent.change(memo, { target: { value: '受付で不在。来週再架電' } });
    const frame = within(panel('search')).getByTestId('link-frame');
    expect(frame.getAttribute('src')).toBe('https://www.google.com/search?q=03-0000-0001+%E6%B1%82%E4%BA%BA&igu=1');
    expect(frame.getAttribute('sandbox')).toBe(LINK_FRAME_SANDBOX);
    expect(frame.getAttribute('referrerpolicy')).toBe('no-referrer');
    const openNew = within(panel('search')).getByRole('link', { name: '新しいタブで開く' });
    expect(openNew.getAttribute('href')).toBe(STORED);
    expect(openNew.getAttribute('rel')).toBe('noopener noreferrer');
    expect(within(panel('search')).getByText('新しいタブで開く').getAttribute('title')).toBe('表示されない場合は新しいタブで開いてください');
    // 架電結果の入力の入力欄は、外さずに隠すだけ
    fireEvent.click(tab('架電結果の入力'));
    expect(form.querySelector('textarea')).toBe(memo);
    expect(memo.value).toBe('受付で不在。来週再架電');
    expect(within(panel('search')).getByTestId('link-frame')).toBe(frame);
  });

  it('does not create the frame of a tab until it is first opened; one click shows the page', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    expect(document.querySelectorAll('[data-testid="link-frame"]')).toHaveLength(1); // 求人検索 だけ
    expect(within(panel('home')).queryByTestId('link-frame')).toBeNull();
    fireEvent.click(tab('ホームページ'));
    expect(tab('ホームページ').getAttribute('aria-selected')).toBe('true');
    const home = within(panel('home')).getByTestId('link-frame');
    expect(home.getAttribute('src')).toBe('https://www.example.com/');
    expect(within(panel('home')).getByRole('link', { name: '新しいタブで開く' }).getAttribute('href')).toBe('https://www.example.com/');
    // 求人検索に戻って再びホームページへ: 枠は同じ要素 (読み直さない)
    fireEvent.click(tab('求人検索'));
    fireEvent.click(tab('ホームページ'));
    expect(within(panel('home')).getByTestId('link-frame')).toBe(home);
  });

  it('a missing 求人票 still has its tab, with a plain message and no frame', async () => {
    await openDeal(detailWith({ ...DEAL_LINKS, job_posting_url: null }));
    fireEvent.click(tab('求人票'));
    expect(tab('求人票').getAttribute('aria-selected')).toBe('true');
    expect(within(panel('posting')).getByTestId('link-missing-posting').textContent).toBe('この案件には求人票が登録されていません。');
    expect(within(panel('posting')).queryByTestId('link-frame')).toBeNull();
  });

  it('a missing ホームページ shows its message', async () => {
    await openDeal(detailWith({ ...DEAL_LINKS, homepage_url: null }));
    fireEvent.click(tab('ホームページ'));
    expect(within(panel('home')).getByTestId('link-missing-home').textContent).toBe('この案件にはホームページが登録されていません。');
  });

  it('with a 求人票 URL the tab shows it in a frame', async () => {
    await openDeal(detailWith({ ...DEAL_LINKS, job_posting_url: 'https://jobs.example.com/p/1' }));
    fireEvent.click(tab('求人票'));
    expect(within(panel('posting')).getByTestId('link-frame').getAttribute('src')).toBe('https://jobs.example.com/p/1');
  });

  it('the ホームページ falls back to the primary company website, and 会社のサイト appears only when different', async () => {
    await openDeal(detailWithProp({ ...DEAL_LINKS, homepage_url: null }, STORED, 'https://company.example.com/'));
    expect(tabNames()).toEqual(['求人検索', 'ホームページ', '求人票', '求人媒体']);
    fireEvent.click(tab('ホームページ'));
    expect(within(panel('home')).getByTestId('link-frame').getAttribute('src')).toBe('https://company.example.com/');
    cleanup();
    await openDeal(detailWithProp(DEAL_LINKS, STORED, 'https://company.example.com/'));
    expect(tabNames()).toEqual(['求人検索', 'ホームページ', '求人票', '会社のサイト', '求人媒体']);
    fireEvent.click(tab('会社のサイト'));
    expect(within(panel('site')).getByTestId('link-frame').getAttribute('src')).toBe('https://company.example.com/');
  });

  it('a property link equal to the homepage focuses the ホームページ tab instead of adding a tab', async () => {
    await openDeal(detailWithProp(DEAL_LINKS, 'https://www.example.com/'));
    await clickPropertyLink('https://www.example.com/');
    expect(byId('dock-panel-links').classList.contains('is-active')).toBe(true);
    expect(tab('ホームページ').getAttribute('aria-selected')).toBe('true');
    expect(tabNames()).toEqual(['求人検索', 'ホームページ', '求人票', '求人媒体']);
    expect(within(panel('home')).getByTestId('link-frame').getAttribute('src')).toBe('https://www.example.com/');
  });

  it('a property link equal to the search URL focuses the 求人検索 tab (not a new browser tab)', async () => {
    await openDeal(detailWithProp(DEAL_LINKS, STORED));
    fireEvent.click(tab('ホームページ'));
    await clickPropertyLink(STORED);
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    expect(tabNames()).toHaveLength(4);
    expect(tab('プロパティ').getAttribute('aria-selected')).toBe('true');
  });

  it('other URLs open a closable tab after the fixed ones; HubSpot 求人媒体 is a fixed tab with the login note when the extension is installed', async () => {
    document.documentElement.setAttribute('data-hrhr-frames', '1.0');
    try {
      await openDeal(detailWithProp(DEAL_LINKS, 'https://other.example.com/'));
      await clickPropertyLink('https://other.example.com/');
      expect(tabNames()).toEqual(['求人検索', 'ホームページ', '求人票', '求人媒体', 'URL_求人検索 ※編集不可']);
      const other = tab('URL_求人検索 ※編集不可');
      expect(other.getAttribute('aria-selected')).toBe('true');
      expect(within(byId(other.getAttribute('aria-controls') ?? '')).getByTestId('link-frame').getAttribute('src')).toBe('https://other.example.com/');
      fireEvent.click(within(byId(other.getAttribute('aria-controls') ?? '')).getByRole('button', { name: 'このタブを閉じる' }));
      expect(screen.queryByRole('tab', { name: 'URL_求人検索 ※編集不可' })).toBeNull();
      expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');

      fireEvent.click(tab('求人媒体'));
      const hsUrl = 'https://app.hubspot.com/contacts/1/record/0-3/1/';
      const p = panel('media-0');
      expect(within(p).getByTestId('link-frame').getAttribute('src')).toBe(hsUrl);
      expect(within(p).getByRole('link', { name: '新しいタブで開く' }).getAttribute('href')).toBe(hsUrl);
      expect(within(p).getByTestId('hubspot-note').textContent).toBe('HubSpot のログイン画面が繰り返し出る場合は「新しいタブで開く」を使ってください');
    } finally { document.documentElement.removeAttribute('data-hrhr-frames'); }
  });

  it('without the frame extension, a HubSpot 求人媒体 tab explains it cannot be shown inside and offers 新しいタブで開く', async () => {
    await openDeal(detailWith(DEAL_LINKS));
    fireEvent.click(tab('求人媒体'));
    const p = panel('media-0');
    expect(within(p).queryByTestId('link-frame')).toBeNull();
    expect(within(p).getByText('このページは画面の中に表示できません')).toBeTruthy();
    expect(within(p).getAllByRole('link', { name: '新しいタブで開く' })[0]?.getAttribute('href')).toBe('https://app.hubspot.com/contacts/1/record/0-3/1/');
  });

  it('the tab list works with the keyboard (arrows / Home / End / Delete)', async () => {
    await openDeal(detailWithProp(DEAL_LINKS, 'https://other.example.com/'));
    await clickPropertyLink('https://other.example.com/');
    const list = screen.getByRole('tablist', { name: '求人検索・リンク先の表示' });
    expect(within(list).getAllByRole('tab').map(t => t.textContent)).toEqual(['求人検索', 'ホームページ', '求人票', '求人媒体', 'URL_求人検索 ※編集不可']);
    tab('求人検索').focus();
    fireEvent.click(tab('求人検索'));
    fireEvent.keyDown(tab('求人検索'), { key: 'ArrowRight' });
    expect(tab('ホームページ').getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(tab('ホームページ'));
    expect(tab('ホームページ').tabIndex).toBe(0);
    expect(tab('求人検索').tabIndex).toBe(-1);
    fireEvent.keyDown(tab('ホームページ'), { key: 'ArrowLeft' });
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    fireEvent.keyDown(tab('求人検索'), { key: 'End' });
    expect(tab('URL_求人検索 ※編集不可').getAttribute('aria-selected')).toBe('true');
    fireEvent.keyDown(tab('URL_求人検索 ※編集不可'), { key: 'Delete' });
    expect(screen.queryByRole('tab', { name: 'URL_求人検索 ※編集不可' })).toBeNull();
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    // 固定のタブは Delete で閉じない
    fireEvent.keyDown(tab('求人検索'), { key: 'Delete' });
    expect(within(list).getAllByRole('tab')).toHaveLength(4);
    fireEvent.keyDown(tab('求人検索'), { key: 'ArrowLeft' });
    expect(tab('求人媒体').getAttribute('aria-selected')).toBe('true');
  });

  it('switching to another deal closes the opened link tabs and returns to the 求人検索 tab', async () => {
    await openDeal(detailWithProp(DEAL_LINKS, 'https://other.example.com/'));
    await clickPropertyLink('https://other.example.com/');
    expect(tab('URL_求人検索 ※編集不可').getAttribute('aria-selected')).toBe('true');
    const list = screen.getByRole('list', { name: '架電キュー' });
    const second = within(list).getAllByRole('button')[1];
    if (!second) throw new Error('row');
    fireEvent.click(second);
    await waitFor(() => { expect(screen.queryByRole('tab', { name: 'URL_求人検索 ※編集不可' })).toBeNull(); });
    expect(tab('求人検索').getAttribute('aria-selected')).toBe('true');
    // ホームページを開いていた状態からも戻る
    fireEvent.click(tab('ホームページ'));
    const first = within(list).getAllByRole('button')[0];
    if (!first) throw new Error('row');
    fireEvent.click(first);
    await waitFor(() => { expect(tab('求人検索').getAttribute('aria-selected')).toBe('true'); });
  });

  it('without a stored search URL, the search uses the dial number', async () => {
    await openDeal(detailWith({ ...DEAL_LINKS, job_search_url: null }));
    const src = within(panel('search')).getByTestId('link-frame').getAttribute('src') ?? '';
    expect(new URL(src).searchParams.get('q')).toMatch(/^0\d{1,4}-\d{1,4}-\d{4} 求人$/u);
  });
});
