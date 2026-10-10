// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RikuroziDraftPanel } from './RikuroziDraftPanel';
import type { RikuroziDraft } from './RikuroziDraftPanel';
const draft: RikuroziDraft = { review_required: true, threshold: 0.35, csv: '\ufeff求人id,店舗id\r\n1234567,4081\r\n', comparisons: [{ title: '倉庫の仕分け', media: 'AirWork', publication: '掲載中か不明', ratio: 0.35, too_similar: true }, { title: '基準の求人', media: 'HRハッカー', publication: '基準', ratio: null, too_similar: false }], copied: [{ label: '給与', value: '時給 1,200円〜1,400円', review: false, reason: null }, { label: '休日・休暇', value: null, review: true, reason: '原本から取得できていません。' }], generated: [{ label: '仕事内容', value: null, review: true, reason: '原本の条件と数字が一致しないため空欄にしました。' }] };
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe('リクロジメディア向けの案', () => {
  it('要確認と未取得と重なりの具体値を示し、識別値・列名は表示しない', async () => {
    const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(draft), { headers: { 'Content-Type': 'application/json' } })); vi.stubGlobal('fetch', fetch);
    const { container } = render(<RikuroziDraftPanel listingId="654321" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    await screen.findByText('要確認の案');
    expect(container.textContent).toContain('掲載中か不明 ／ 重なり：35.0％ ／ 似すぎ（要確認）');
    expect(container.textContent).toContain('重なり：未取得（要確認）');
    expect(container.textContent).toContain('時給 1,200円〜1,400円');
    expect(container.textContent).toContain('休日・休暇（要確認）');
    expect(container.textContent).not.toMatch(/1234567|4081|654321|求人id|店舗id|基本給与 最小/);
    expect(fetch.mock.calls[0]?.[0]).toBe('/api/job-copy/listings/654321/rikurozi-draft');
    const url = vi.fn().mockReturnValue('blob:review'); vi.stubGlobal('URL', { createObjectURL: url, revokeObjectURL: vi.fn() });
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => { /* Download is inspected as a Blob in this test. */ });
    fireEvent.click(screen.getByRole('button', { name: '確認用の案をダウンロード' }));
    expect(await (url.mock.calls[0]?.[0] as Blob).text()).toBe(draft.csv);
  });
  it('失敗時に原本の確認と再実行を案内する', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ code: 'customer_relation_unknown' }), { status: 422, headers: { 'Content-Type': 'application/json' } })));
    render(<RikuroziDraftPanel listingId="1" />); fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    expect((await screen.findByRole('alert')).textContent).toContain('取引先との関連');
    expect(screen.queryByRole('button', { name: '確認用の案をダウンロード' })).toBeNull();
  });
  it('別の求人を選んだ後に古い結果を表示しない', async () => {
    let complete: ((value: Response) => void) | undefined;
    vi.stubGlobal('fetch', vi.fn().mockImplementation(() => new Promise<Response>(resolve => { complete = resolve; })));
    const { unmount } = render(<RikuroziDraftPanel listingId="1" />);
    fireEvent.click(screen.getByRole('button', { name: 'リクロジメディア向けの案を作る' }));
    await waitFor(() => { expect(complete).toBeDefined(); }); unmount();
    render(<RikuroziDraftPanel listingId="2" />); complete?.(new Response(JSON.stringify(draft), { headers: { 'Content-Type': 'application/json' } }));
    expect(screen.queryByText('要確認の案')).toBeNull();
  });
});
