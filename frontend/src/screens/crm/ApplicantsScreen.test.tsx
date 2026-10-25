// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApplicantSheet, ApplicantsScreen } from './ApplicantsScreen';
import { applicantQueueFixture, applicantDetailFixture } from './applicantFixture';
import { ZOOM_EMBED_ORIGIN } from './smartEmbed';
import { applicantMode, modeUrl } from './CrmModeSwitch';
afterEach(() => { cleanup(); localStorage.clear(); vi.unstubAllGlobals(); window.history.replaceState(null, '', '/'); });
describe('応募の読み取りモード', () => {
  it('既存のビューと架空サンプルの条件を保って切り替える', () => {
    const url = modeUrl('?mode=fixture&view=mine&stage=abc', true);
    expect(applicantMode(url)).toBe(true);
    expect(new URLSearchParams(url).get('mode')).toBe('fixture');
    expect(new URLSearchParams(modeUrl(url, false)).get('view')).toBe('mine');
    expect(applicantMode(modeUrl(url, false))).toBe(false);
    expect(applicantMode('?mode=applicants')).toBe(true);
    expect(applicantMode('?applicant=90003')).toBe(true);
    expect(applicantMode(modeUrl('?applicant=90003', false))).toBe(false);
    expect(applicantMode(modeUrl('?mode=applicants', false))).toBe(false);
  });
  it('架空の一覧から詳細・対応ルール・結果・シートを読む。書き込みも実通信もしない', async () => {
    const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
    render(<ApplicantsScreen fixture />);
    expect(await screen.findByText('2件')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /架空 はるか/ }));
    expect(screen.getByRole('heading', { level: 2 }).textContent).toBe('架空 はるか');
    await screen.findByText('応募のお礼を伝え、希望の勤務時間を確認してください。');
    expect(screen.getByRole('tab', { name: '求人票' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.queryByRole('tab', { name: 'Zoom' })).toBeNull();
    expect(document.querySelector('iframe[title^="求人票"]')?.getAttribute('src')).toBe('https://www.example.com/jobs/fictional-inspection');
    expect(within(screen.getByRole('complementary', { name: '求人票・シート・スクリプト' })).getByRole('link', { name: '案件管理で見る' }).getAttribute('target')).toBe('_blank');
    expect(screen.getByRole('tab', { name: '対応の入力' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('button', { name: /応募者.*に発信/ }).hasAttribute('disabled')).toBe(true);
    fireEvent.click(screen.getByRole('tab', { name: 'プロパティ' }));
    const props = screen.getByRole('complementary', { name: '応募のプロパティ' });
    expect(within(props).getByLabelText<HTMLSelectElement>('保有資格').value).toBe('none');
    expect(within(props).getByLabelText<HTMLSelectElement>('保有資格').selectedOptions[0]?.textContent).toBe('資格なし');
    const jobCard = within(props).getByRole('heading', { name: /^応募先$/ }).closest('section');
    expect(jobCard?.textContent).toContain('サンプル物流株式会社');
    expect(jobCard?.textContent).toContain('検品スタッフ（架空求人）');
    expect(within(props).getByRole('region', { name: '関連の求人票' }).querySelector('a')?.href).toBe('https://app.hubspot.com/contacts/0/record/0-420/80001/');
    fireEvent.click(screen.getByRole('tab', { name: 'メモ' }));
    expect(screen.getByRole('region', { name: '応募のメモ' }).textContent).toContain('午後の連絡を希望しています。');
    fireEvent.click(screen.getByRole('tab', { name: '報告シート' }));
    expect(document.querySelector('iframe[title^="報告シート"]')?.getAttribute('src')).toContain('/fictional-applicants-sheet/');
    fireEvent.click(screen.getByRole('tab', { name: '応募一覧' }));
    fireEvent.click(screen.getByRole('button', { name: /架空 あおい/ }));
    expect(await screen.findByText('この応募には求人票の URL がありません。')).toBeTruthy();
    expect(screen.getByRole('tab', { name: '求人票' }).getAttribute('aria-selected')).toBe('true');
    fireEvent.click(screen.getByRole('tab', { name: '報告シート' }));
    expect(screen.getByText('この応募には報告シートの URL がありません')).toBeTruthy();
    expect(fetcher).not.toHaveBeenCalled();
  });
  it.each([
    ['+815000000000', '050-0000-0000'],
    ['番号確認中（架空）', '番号確認中（架空）'],
    ['', '電話番号未登録'],
  ])('一覧の電話 %s を %s と表示する', async (raw, shown) => {
    const readOnly = { ...applicantDetailFixture('90001'), writes_enabled: false };
    vi.stubGlobal('fetch', vi.fn((url: string) => Promise.resolve(new Response(JSON.stringify(
      url.includes('/queue') ? { ...applicantQueueFixture, items: [{ ...applicantQueueFixture.items[0], phone: raw }] } : readOnly,
    ), { status: 200, headers: { 'Content-Type': 'application/json' } }))));
    render(<ApplicantsScreen />);
    const row = await screen.findByRole('button', { name: /架空 はるか/ });
    expect(within(row).getByText(shown)).toBeTruthy();
    if (raw !== shown && raw) expect(within(row).queryByText(raw)).toBeNull();
  });
  it('範囲・検索・媒体を組み合わせ、URLに条件を残す', async () => {
    render(<ApplicantsScreen fixture />);
    await screen.findByText('2件');
    fireEvent.click(screen.getByRole('button', { name: '条件を変更' }));
    fireEvent.change(screen.getByLabelText('範囲'), { target: { value: 'all' } });
    await screen.findByText('3件');
    expect(new URLSearchParams(window.location.search).get('scope')).toBe('all');
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（6件選択）' }));
    expect(screen.getByRole('heading', { name: '採用管理（架空）' })).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: '採用済み' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'キャンセル' }));
    fireEvent.change(screen.getByLabelText('検索対象'), { target: { value: 'deal' } });
    await screen.findByText('3件');
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: 'サンプル商店' } });
    expect(screen.getByText('3件')).toBeTruthy();
    await screen.findByText('1件');
    expect(screen.getByRole('button', { name: /架空 そら/ })).toBeTruthy();
    expect(new URLSearchParams(window.location.search).get('q')).toBe('サンプル商店');
    fireEvent.change(screen.getByLabelText('応募媒体'), { target: { value: '架空採用ページ' } });
    await screen.findByText('条件に一致する応募はありません。');
  });
  it('Enterで検索し、一覧外の応募番号から直接詳細を読む', async () => {
    window.history.replaceState(null, '', '/?mode=fixture&crmMode=applicants&applicant=90003');
    const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
    render(<ApplicantsScreen fixture />);
    await screen.findByText('2件');
    expect(await screen.findByRole('heading', { level: 2 })).toHaveProperty('textContent', '架空 そら');
    expect(screen.queryByRole('button', { name: /架空 そら/ })).toBeNull();
    expect(screen.getByText('この応募のパイプラインは読み取り専用です。')).toBeTruthy();
    expect(screen.queryByLabelText('ステージ')).toBeNull();
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: 'あおい' } });
    fireEvent.submit(screen.getByRole('search'));
    await screen.findByText('1件');
    expect(new URLSearchParams(window.location.search).get('q')).toBe('あおい');
    expect(fetcher).not.toHaveBeenCalled();
  });
  it('実APIへ範囲・検索・媒体・ステージを渡し、入力途中は送らない', async () => {
    window.history.replaceState(null, '', '/?mode=applicants&scope=all&media=web&stages=other-done&applicant=90003');
    const fetcher = vi.fn((url: string) => Promise.resolve({ ok: true, json: () => Promise.resolve(url.includes('/queue') ? applicantQueueFixture : applicantDetailFixture('90003')) }));
    vi.stubGlobal('fetch', fetcher);
    render(<ApplicantsScreen fixture={false} />);
    await screen.findByText('2件');
    await screen.findByRole('heading', { level: 2 });
    const queueCalls = () => fetcher.mock.calls.filter(([url]) => url.includes('/queue'));
    expect(queueCalls()).toHaveLength(1);
    const first = new URL(queueCalls()[0]?.[0] ?? '', 'http://localhost');
    expect(Object.fromEntries(first.searchParams)).toEqual({ scope: 'all', stages: 'other-done', media: 'web' });
    expect(fetcher.mock.calls.some(([url]) => url === '/api/crm/applicants/90003')).toBe(true);
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: '架' } });
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: '架空' } });
    expect(queueCalls()).toHaveLength(1);
    await waitFor(() => { expect(queueCalls()).toHaveLength(2); });
    const next = new URL(queueCalls()[1]?.[0] ?? '', 'http://localhost');
    expect(Object.fromEntries(next.searchParams)).toEqual({ scope: 'all', q: '架空', stages: 'other-done', media: 'web' });
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: '会社' } });
    fireEvent.submit(screen.getByRole('search'));
    await waitFor(() => { expect(queueCalls()).toHaveLength(3); });
    expect(new URL(queueCalls()[2]?.[0] ?? '', 'http://localhost').searchParams.get('q')).toBe('会社');
  });
  it('既存のステージ選択と媒体で絞る', async () => {
    render(<ApplicantsScreen fixture />);
    fireEvent.click(screen.getByRole('button', { name: '条件を変更' }));
    fireEvent.click(await screen.findByRole('button', { name: 'ステージ（5件選択）' }));
    expect(screen.getByRole('heading', { name: '一次対応_BPO' })).toBeTruthy();
    fireEvent.click(screen.getByRole('checkbox', { name: '朝連絡' }));
    fireEvent.click(screen.getByRole('button', { name: '適用' }));
    await waitFor(() => { expect(screen.queryByRole('button', { name: /架空 はるか/ })).toBeNull(); });
    expect(screen.getByRole('button', { name: /架空 あおい/ })).toBeTruthy();
    fireEvent.change(screen.getByLabelText('応募媒体'), { target: { value: '架空求人サイト' } });
    expect(await screen.findByText('条件に一致する応募はありません。')).toBeTruthy();
  });
  it('長い対応ルールは畳んで全表示でき、OFF のプロパティは読み取りになる', async () => {
    const detail = { ...applicantDetailFixture('90001'), writes_enabled: false };
    const rules = detail.sections[1];
    if (!rules) throw new Error('対応ルールがありません');
    rules.fields = [{ label: '初動対応', value: '架空の長い対応ルール。'.repeat(30) }];
    vi.stubGlobal('fetch', vi.fn().mockImplementation((url: string) => Promise.resolve({ ok: true, json: () => Promise.resolve(url.includes('/queue') ? { ...applicantQueueFixtureForTest, items: [detail.row] } : detail) })));
    render(<ApplicantsScreen fixture={false} />);
    fireEvent.click(await screen.findByRole('button', { name: /架空 はるか/ }));
    const expand = await screen.findByRole('button', { name: '全部表示' });
    expect(expand.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(expand);
    expect(screen.getByRole('button', { name: '畳む' }).getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText('架空の長い対応ルール。'.repeat(30))).toBeTruthy();
    fireEvent.click(screen.getByRole('tab', { name: 'プロパティ' }));
    expect(screen.getByText('資格なし', { selector: 'dd' })).toBeTruthy();
    expect(screen.queryByLabelText('保有資格')).toBeNull();
    expect(screen.getByText('いまは HubSpot に保存できない設定です。見るだけになります。')).toBeTruthy();
    expect(screen.queryByRole('button', { name: '記録して次へ' })).toBeNull();
  });
  it('発信は中央から行い、未ログインなら共通の電話の引き出しを開く', async () => {
    const detail = { ...applicantDetailFixture('90001'), phone: '03-0000-0001', writes_enabled: false };
    vi.stubGlobal('fetch', vi.fn().mockImplementation((url: string) => Promise.resolve({ ok: true, json: () => Promise.resolve(url.includes('/queue') ? { ...applicantQueueFixtureForTest, items: [detail.row] } : detail) })));
    render(<ApplicantsScreen fixture={false} />);
    fireEvent.click(await screen.findByRole('button', { name: /架空 はるか/ }));
    const dial = await screen.findByRole('button', { name: '応募者 03-0000-0001 に発信' });
    const frame = document.querySelector<HTMLIFrameElement>('iframe[title="Zoom Phone"]');
    if (!frame?.contentWindow) throw new Error('電話の枠がありません');
    const post = vi.spyOn(frame.contentWindow, 'postMessage').mockImplementation(() => undefined);
    fireEvent.load(frame);
    expect(document.querySelector('.cq-zoom-drawer')?.getAttribute('aria-hidden')).toBe('true');
    fireEvent.click(dial);
    expect(post).not.toHaveBeenCalled();
    expect(document.querySelector('.cq-zoom-drawer')?.getAttribute('aria-hidden')).toBe('false');
    expect(screen.getByText('Zoom にログインしてください。ログインできたら、もう一度発信を押してください')).toBeTruthy();
    fireEvent(window, new MessageEvent('message', { origin: ZOOM_EMBED_ORIGIN, source: frame.contentWindow, data: {} }));
    expect(document.querySelector('.cq-zoom-drawer')?.getAttribute('aria-hidden')).toBe('true');
    expect(screen.getByText('Zoom にログインしました。発信ボタンを押してください')).toBeTruthy();
    fireEvent.click(dial);
    expect(post).toHaveBeenCalledWith({ type: 'zp-make-call', data: { number: '+81300000001', autoDial: true } }, ZOOM_EMBED_ORIGIN);
    expect(screen.queryByRole('tab', { name: 'Zoom' })).toBeNull();
  });
  it('壊れた応募の配置は対応の入力に戻し、取引の保存した配置を変更しない', () => {
    localStorage.setItem('hrhr.crm.applicantsDock.v1', 'invalid');
    localStorage.setItem('hrhr.crm.dockLayout.v2', 'deal-layout-sentinel');
    render(<ApplicantsScreen fixture />);
    expect(screen.getByRole('tab', { name: '対応の入力' }).getAttribute('aria-selected')).toBe('true');
    expect(localStorage.getItem('hrhr.crm.dockLayout.v2')).toBe('deal-layout-sentinel');
    expect(localStorage.getItem('hrhr.crm.applicantsDock.v1')).toContain('"active":"result"');
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
    await screen.findByText('応募のお礼を伝え、希望の勤務時間を確認してください。');
    fireEvent.click(screen.getByRole('tab', { name: 'メモ' }));
    expect(await screen.findByText('架空メモ')).toBeTruthy();
    expect(document.querySelector('.applicants-detail script')).toBeNull();
    expect(screen.getByText('架空メモ').getAttribute('onclick')).toBeNull();
  });
});
const applicantQueueFixtureForTest = { total: 1, next_cursor: null, pipelines: [], media_options: [] };

it('対象ごとに検索し、未完了にない応募は全範囲で探せる', async () => {
  render(<ApplicantsScreen fixture />);
  await screen.findByText('2件');
  fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: 'サンプル商店' } });
  fireEvent.submit(screen.getByRole('search'));
  await screen.findByText('0件');
  fireEvent.change(screen.getByLabelText('検索対象'), { target: { value: 'deal' } });
  await screen.findByText('0件');
  expect(new URLSearchParams(window.location.search).get('qt')).toBe('deal');
  fireEvent.click(screen.getByRole('button', { name: 'すべての応募で探す' }));
  await screen.findByText('1件');
  expect(screen.getByRole('button', { name: /架空 そら/ })).toBeTruthy();
  fireEvent.change(screen.getByLabelText('検索対象'), { target: { value: 'listing' } });
  await screen.findByText('0件');
  fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: '検品' } });
  fireEvent.submit(screen.getByRole('search'));
  await screen.findByText('2件');
  expect(new URLSearchParams(window.location.search).get('qt')).toBe('listing');
});
it('求人票のページが 2 つあるときだけ切り替えが出て、選んだページの URL が枠に入る', async () => {
  vi.stubGlobal('fetch', vi.fn());
  render(<ApplicantsScreen fixture />);
  await screen.findByText('2件');
  fireEvent.click(screen.getByRole('button', { name: /架空 はるか/ }));
  const group = await screen.findByRole('group', { name: '求人票のページ' });
  const frame = () => document.querySelector('iframe[title^="求人票"]')?.getAttribute('src');
  expect(within(group).getAllByRole('button').map(b => b.textContent)).toEqual(['HRハッカー', 'AirWork']);
  expect(within(group).getByRole('button', { name: 'HRハッカー' }).getAttribute('aria-pressed')).toBe('true');
  expect(frame()).toBe('https://www.example.com/jobs/fictional-inspection');
  fireEvent.click(within(group).getByRole('button', { name: 'AirWork' }));
  expect(frame()).toBe('https://www.example.com/airwork/fictional-inspection');
  expect(within(group).getByRole('button', { name: 'AirWork' }).getAttribute('aria-pressed')).toBe('true');
  expect(screen.getAllByRole('link', { name: '新しいタブで開く' }).some(a => a.getAttribute('href') === 'https://www.example.com/airwork/fictional-inspection')).toBe(true);
  // ページが無い応募（偶数番）は切り替えを出さない。
  fireEvent.click(screen.getByRole('button', { name: /架空 あおい/ }));
  await screen.findByText('この応募には求人票の URL がありません。');
  expect(screen.queryByRole('group', { name: '求人票のページ' })).toBeNull();
});
it('応募の概要から求人票・取引カードを開き、今の契約と過去の取引を確認できる', async () => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher);
  render(<ApplicantsScreen fixture />);
  fireEvent.click(await screen.findByRole('button', { name: /架空 はるか/ }));
  await screen.findByText('応募のお礼を伝え、希望の勤務時間を確認してください。');
  const overview = screen.getByRole('main', { name: '応募の詳細' });
  fireEvent.click(within(overview).getByRole('button', { name: '検品スタッフ（架空求人）' }));
  expect(screen.getByRole('tab', { name: 'プロパティ' }).getAttribute('aria-selected')).toBe('true');
  const listing = screen.getByRole('region', { name: '関連の求人票' });
  expect(document.activeElement).toBe(listing);
  expect(listing.textContent).toContain('公開中');
  expect(within(listing).getByText('ほかの項目 3 件').closest('details')?.open).toBe(false);
  fireEvent.click(within(listing).getByText('ほかの項目 3 件'));
  expect(listing.textContent).toContain('2026/11/30');
  expect(within(listing).getByRole('link', { name: '案件管理で見る' }).getAttribute('href')).toBe('https://www.example.com/app/job-copy/fictional-posting');
  fireEvent.click(within(overview).getByRole('button', { name: 'サンプル物流株式会社' }));
  const deal = screen.getByRole('region', { name: '関連の取引' });
  expect(document.activeElement).toBe(deal);
  expect(deal.textContent).toContain('今の契約');
  expect(deal.textContent).toContain('架空 担当');
  expect(within(deal).getAllByRole('link', { name: 'HubSpotで開く' })[0]?.getAttribute('href')).toBe('https://app.hubspot.com/contacts/0/record/0-3/70001/');
  const collapsed = within(deal).getByText('ほかの取引 1 件').closest('details');
  expect(collapsed?.open).toBe(false);
  fireEvent.click(within(deal).getByText('ほかの取引 1 件'));
  expect(within(deal).getByText('解約済み')).toBeTruthy();
  expect(fetcher).not.toHaveBeenCalled();
});

it.each([false, true])('詳細の応答待ちは読み込み中と表示し、禁止理由は届いたときだけ表示する（一覧選択=%s）', async fromQueue => {
  if (!fromQueue) window.history.replaceState(null, '', '/?applicant=90003');
  let finish: ((value: unknown) => void) | undefined;
  vi.stubGlobal('fetch', vi.fn((url: string) => url.includes('/queue')
    ? Promise.resolve({ ok: true, json: () => Promise.resolve(applicantQueueFixture) })
    : url.includes('/presence') || url.includes('/reminders')
      ? Promise.resolve({ ok: true, json: () => Promise.resolve({ viewers: [], items: [], total: 0 }) })
      : new Promise(resolve => { finish = resolve; })));
  render(<ApplicantsScreen fixture={false} />);
  await screen.findByText('2件');
  if (fromQueue) fireEvent.click(screen.getByRole('button', { name: /架空 はるか/ }));
  const overview = screen.getByRole('main', { name: '応募の詳細' });
  expect(within(overview).getByRole('heading', { level: 2 }).textContent).toBe(fromQueue ? '架空 はるか' : '読み込み中…');
  expect(within(overview).queryByText('氏名未登録')).toBeNull();
  expect(within(overview).queryByText('架電禁止またはブロックのため発信できません')).toBeNull();
  expect(within(overview).getByText('読み込み中…', { selector: '.wd-dial-reason' })).toBeTruthy();
  expect(within(overview).getByRole('button', { name: /に発信/ })).toHaveProperty('disabled', true);
  const detail = { ...applicantDetailFixture(fromQueue ? '90001' : '90003'), stop_reason: '架空の連絡停止理由' };
  finish?.({ ok: true, json: () => Promise.resolve(detail) });
  await screen.findByText('架電禁止・ブロック: 架空の連絡停止理由');
  expect(within(overview).getByRole('button', { name: /に発信/ })).toHaveProperty('disabled', true);
});

it.each(['検索', '検索対象', '範囲', '媒体', 'ステージ'])('プロパティ表示中の%s変更で応募一覧へ戻す', async change => {
  render(<ApplicantsScreen fixture />);
  fireEvent.click(await screen.findByRole('button', { name: /架空 はるか/ }));
  await screen.findByText('応募のお礼を伝え、希望の勤務時間を確認してください。');
  fireEvent.click(screen.getByRole('tab', { name: 'プロパティ' }));
  fireEvent.click(screen.getByRole('button', { name: '条件を変更' }));
  if (change === '検索') {
    fireEvent.change(screen.getByLabelText('応募を検索'), { target: { value: 'あおい' } });
    fireEvent.submit(screen.getByRole('search'));
  } else if (change === '検索対象') fireEvent.change(screen.getByLabelText('検索対象'), { target: { value: 'deal' } });
  else if (change === '範囲') fireEvent.change(screen.getByLabelText('範囲'), { target: { value: 'all' } });
  else if (change === '媒体') fireEvent.change(screen.getByLabelText('応募媒体'), { target: { value: '架空求人サイト' } });
  else {
    fireEvent.click(screen.getByRole('button', { name: 'ステージ（5件選択）' }));
    fireEvent.click(screen.getByRole('checkbox', { name: '朝連絡' }));
    fireEvent.click(screen.getByRole('button', { name: '適用' }));
  }
  expect(screen.getByRole('tab', { name: '応募一覧' }).getAttribute('aria-selected')).toBe('true');
  const queue = await screen.findByRole('list', { name: '応募キュー' });
  await waitFor(() => { expect(within(queue).getAllByRole('button').length).toBeGreaterThan(0); });
});

it('未選択で再表示すると保存されたプロパティ表示を応募一覧へ戻す', async () => {
  const view = render(<ApplicantsScreen fixture />);
  await screen.findByText('2件');
  fireEvent.click(screen.getByRole('tab', { name: 'プロパティ' }));
  view.unmount();
  render(<ApplicantsScreen fixture />);
  expect(screen.getByRole('tab', { name: '応募一覧' }).getAttribute('aria-selected')).toBe('true');
  expect(await screen.findByRole('button', { name: /架空 はるか/ })).toBeTruthy();
});
