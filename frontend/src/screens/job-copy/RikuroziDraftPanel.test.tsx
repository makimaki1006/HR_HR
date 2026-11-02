// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RikuroziDraftPanel } from './RikuroziDraftPanel';
import type { RikuroziDraft } from './RikuroziDraftPanel';
const draft: RikuroziDraft = { images: ['https://example.invalid/one.jpg', '', 'https://example.invalid/three.png'], review_required: true, threshold: 0.2, min_sentence_chars: 8, csv: '\ufeff求人id,店舗id\r\n,4081\r\n', generated_values: { 案件名: '新しい案件名', 仕事内容: '' }, comparisons: [{ title: '倉庫の仕分け', media: 'AirWork', publication: '掲載中か不明', ratio: 0.2, too_similar: true, whole_match: false, matched_sentences: ['配送の手順を研修で学びます'] }, { title: '基準の求人', media: 'HRハッカー', publication: '基準', ratio: null, too_similar: false, whole_match: false, matched_sentences: [] }], copied: [{ label: '給与', value: '時給 1,200円〜1,400円', review: false, reason: null, missing_chunks: [] }, { label: '休日・休暇', value: null, review: true, reason: '原本から取得できていません。', missing_chunks: [] }], generated: [{ label: '仕事内容', value: null, review: true, reason: '原文にない数字の表記があるため空欄にしました。', missing_chunks: ['日給1100円'] }] };
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
/** 画面が呼ぶ口ごとに答える偽の fetch。呼ばれた (メソッド, URL, 本文) を記録する。 */
function routed(handlers: { draft?: () => Response; create?: () => Response; list?: () => Response; put?: () => Response }) {
  const calls: { method: string; url: string; body: unknown }[] = [];
  const fetch = vi.fn().mockImplementation((url: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET';
    calls.push({ method, url, body: typeof init?.body === 'string' ? JSON.parse(init.body) as unknown : null });
    if (url.endsWith('/rikurozi-draft')) return Promise.resolve(handlers.draft?.() ?? json(draft));
    if (url.endsWith('/reclogi-id')) return Promise.resolve(handlers.put?.() ?? json({ listing_id: '900', reclogi_id: '424242' }));
    if (url.endsWith('/rikurozi-listing') && method === 'POST') return Promise.resolve(handlers.create?.() ?? json({ status: 'created', listing_id: '900', linked_deals: 2, unlinked_deals: 0 }));
    if (url.endsWith('/rikurozi-listing')) return Promise.resolve(handlers.list?.() ?? json({ listings: [] }));
    throw new Error(`unexpected ${url}`);
  });
  vi.stubGlobal('fetch', fetch);
  return { fetch, calls };
}
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe('リクロジメディア向けの案', () => {
  it('要確認と未取得と重なりの具体値を示し、識別値・列名は表示しない', async () => {
    const { calls } = routed({});
    const { container } = render(<RikuroziDraftPanel listingId="654321" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    await screen.findByText('要確認の案');
    expect(container.textContent).toContain('掲載中か不明 ／ 文の一致率：20.0％ ／ 似すぎ・不合格（要確認）');
    expect(container.textContent).toContain('文の一致率：未取得（要確認）');
    expect(container.textContent).toContain('時給 1,200円〜1,400円');
    expect(container.textContent).toContain('新しい求人として取り込むCSVを作成します。画像は元の求人と同じ順序で最大3枚を使います。');
    expect(container.textContent).toContain('元の求人から取得できない連絡先・フォーム設定・公開設定などの項目は空欄で出力します。');
    expect(container.textContent).toContain('8文字未満の文は一致率に数えません');
    expect(container.textContent).toContain('文の一致率が20％以上の場合');
    expect(container.textContent).toContain('既知の項目見出しを除いて文を比べます');
    expect(container.textContent).toContain('休日・休暇（要確認）');
    expect(screen.getByRole('heading', { name: '原本の条件' })).toBeTruthy();
    expect(container.querySelector('ol')?.textContent).toBe('画像1：https://example.invalid/one.jpg画像2：空欄画像3：https://example.invalid/three.png');
    expect(container.textContent).toContain('日給1100円');
    expect(container.textContent).toContain('配送の手順を研修で学びます');
    expect(container.textContent).toContain('正しい数字を違う文脈で使った誤りは検知できません');
    expect(container.textContent).not.toMatch(/1234567|4081|654321|求人id|店舗id|基本給与 最小/);
    expect(calls.find(c => c.url.endsWith('/rikurozi-draft'))?.url).toBe('/api/job-copy/listings/654321/rikurozi-draft');
    const url = vi.fn().mockReturnValue('blob:review'); vi.stubGlobal('URL', { createObjectURL: url, revokeObjectURL: vi.fn() });
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => { /* Download is inspected as a Blob in this test. */ });
    fireEvent.click(screen.getByRole('button', { name: 'この案でCSVを出す' }));
    await waitFor(() => { expect(url).toHaveBeenCalledTimes(1); });
    expect(await (url.mock.calls[0]?.[0] as Blob).text()).toBe(draft.csv);
    const create = calls.filter(c => c.method === 'POST' && c.url.endsWith('/rikurozi-listing'));
    expect(create).toHaveLength(1);
    expect(create[0]?.url).toBe('/api/job-copy/listings/654321/rikurozi-listing');
    expect(create[0]?.body).toEqual({ generated: { 案件名: '新しい案件名', 仕事内容: '' } });
    expect((await screen.findByText(/HubSpotに新しい求人（公開前）を作りました。2件の取引に関連付けました。/)).textContent).toBeTruthy();
  });
  it('全文一致の不合格と短文設定を示す', async () => {
    const result = { ...draft, min_sentence_chars: 8, comparisons: [{ ...draft.comparisons[0], whole_match: true, ratio: null, matched_sentences: [] }] };
    routed({ draft: () => json(result) });
    render(<RikuroziDraftPanel listingId="1" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    expect((await screen.findByText('生成した全文が相手の本文に含まれています。')).textContent).toBeTruthy();
    expect(screen.getByText(/8文字未満の文/).textContent).toContain('8文字未満');
    expect(screen.getByText(/掲載中か不明 ／ 文の一致率/).textContent).toContain('似すぎ・不合格');
  });
  it('転記の不一致は要確認として出力を止める', async () => {
    routed({ draft: () => json({ code: 'draft_copy_mismatch' }, 422) });
    render(<RikuroziDraftPanel listingId="1" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    expect((await screen.findByRole('alert')).textContent).toContain('原本から写した条件が一致しないため、出力を止めました（要確認）');
    expect(screen.queryByRole('button', { name: 'この案でCSVを出す' })).toBeNull();
  });
  it('失敗時に原本の確認と再実行を案内する', async () => {
    routed({ draft: () => json({ code: 'customer_relation_unknown' }, 422) });
    render(<RikuroziDraftPanel listingId="1" />); fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    expect((await screen.findByRole('alert')).textContent).toContain('取引先との関連');
    expect(screen.queryByRole('button', { name: 'この案でCSVを出す' })).toBeNull();
  });
  it('別の求人を選んだ後に古い結果を表示しない', async () => {
    let complete: ((value: Response) => void) | undefined;
    vi.stubGlobal('fetch', vi.fn().mockImplementation((url: string) => url.endsWith('/rikurozi-draft') ? new Promise<Response>(resolve => { complete = resolve; }) : Promise.resolve(json({ listings: [] }))));
    const { unmount } = render(<RikuroziDraftPanel listingId="1" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    await waitFor(() => { expect(complete).toBeDefined(); }); unmount();
    render(<RikuroziDraftPanel listingId="2" />); complete?.(new Response(JSON.stringify(draft), { headers: { 'Content-Type': 'application/json' } }));
    expect(screen.queryByText('要確認の案')).toBeNull();
  });
  async function openDraft(listingId = '1') {
    render(<RikuroziDraftPanel listingId={listingId} />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    await screen.findByText('要確認の案');
  }
  it('すでにある求人を使ったときは新しく作っていないと伝え、CSVは出す', async () => {
    routed({ create: () => json({ status: 'existing', listing_id: '900', linked_deals: 2, unlinked_deals: 1 }) });
    const url = vi.fn().mockReturnValue('blob:review'); vi.stubGlobal('URL', { createObjectURL: url, revokeObjectURL: vi.fn() });
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => { /* inspected as a Blob */ });
    await openDraft();
    fireEvent.click(screen.getByRole('button', { name: 'この案でCSVを出す' }));
    const note = await screen.findByText(/新しくは作っていません/);
    expect(note.textContent).toContain('2件の取引に関連付けました。関連付けできなかった取引が1件あります。');
    expect(url).toHaveBeenCalledTimes(1);
  });
  it('求人を作れなかったときはCSVを出さず、理由を平易に示す', async () => {
    routed({ create: () => json({ code: 'deal_relation_missing' }, 422) });
    const url = vi.fn(); vi.stubGlobal('URL', { createObjectURL: url, revokeObjectURL: vi.fn() });
    await openDraft();
    fireEvent.click(screen.getByRole('button', { name: 'この案でCSVを出す' }));
    expect((await screen.findByText(/関連する取引がないため、求人を作っていません/)).textContent).toBeTruthy();
    expect(url).not.toHaveBeenCalled();
  });
  it('リクロジの求人IDを入れて保存でき、重複は理由つきで保存されない', async () => {
    const { calls } = routed({ list: () => json({ listings: [{ listing_id: '900', name: '新しい案件名', reclogi_id: null }, { listing_id: '901', name: '設定済み', reclogi_id: '777' }] }), put: () => json({ code: 'reclogi_id_in_use' }, 409) });
    render(<RikuroziDraftPanel listingId="1" />);
    expect((await screen.findByText('リクロジの求人ID：777')).textContent).toBeTruthy();
    const input = screen.getByLabelText(/リクロジの求人ID（リクロジに取り込んだあとで入力します）/);
    fireEvent.change(input, { target: { value: '424242' } });
    fireEvent.click(screen.getByRole('button', { name: 'IDを保存' }));
    expect((await screen.findByRole('alert')).textContent).toBe('このIDは、ほかの求人にすでに入っています。保存していません。');
    const put = calls.find(c => c.method === 'PUT');
    expect(put?.url).toBe('/api/job-copy/rikurozi-listings/900/reclogi-id');
    expect(put?.body).toEqual({ reclogi_id: '424242' });
  });
  it('IDの保存に成功すると入力欄がIDの表示に変わる', async () => {
    routed({ list: () => json({ listings: [{ listing_id: '900', name: '新しい案件名', reclogi_id: null }] }) });
    render(<RikuroziDraftPanel listingId="1" />);
    fireEvent.change(await screen.findByLabelText(/リクロジの求人ID/), { target: { value: '424242' } });
    fireEvent.click(screen.getByRole('button', { name: 'IDを保存' }));
    expect((await screen.findByText('リクロジの求人ID：424242')).textContent).toBeTruthy();
  });
});
