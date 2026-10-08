// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';
import { jobs } from './data';
import { CAUSAL_PATTERN, JARGON_PATTERN } from './format';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));

// サーバーが実際に返す説明文（src/handlers/job_copy_market.rs）と同じ開発用の言葉を含める。
const market = {
  source: 'Indeed 採用市場レポート（求人企業向け）', titles: ['配送ドライバー'], prefectures: ['大分県'],
  ctk_basis: 'Indeed上の行動データで労働市場全体ではありません。既存市場レポートのctk_countで、応募者数やHRハッカーのクリック数ではありません。上流の計測定義は別途確認が必要です。',
  series: { prefecture: '大分県', months: ['2026-07', '2026-08'], job_count: [120, 130], ctk_count: [900, 950], employer_count: [40, 41], seekers_per_posting: [7.5, 7.3] },
};

/** 画面の文言（求人本文の原文・応募理由の原文・画像は除く）と、ツールチップ・読み上げラベルを集める。 */
function visibleWording(): string {
  const copy = document.body.cloneNode(true) as HTMLElement;
  copy.querySelectorAll('pre, blockquote, img, textarea').forEach(node => { node.remove(); });
  const attributes = [...copy.querySelectorAll('[title], [aria-label], [placeholder]')].flatMap(node => ['title', 'aria-label', 'placeholder'].map(name => node.getAttribute(name) ?? ''));
  return `${copy.textContent}\n${attributes.join('\n')}`;
}

beforeEach(() => {
  window.history.replaceState(null, '', '/app/job-copy?demo=1');
  api.mockResolvedValue({ ok: true, data: market });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

describe('job copy screen wording', () => {
  it('shows no developer terms or causal wording on any tab of any demo job', async () => {
    const { container } = render(<JobCopyScreen />);
    expect(container.querySelectorAll('.jc-job')).toHaveLength(jobs.length);
    const seen: string[] = [];
    for (const index of jobs.map((_, position) => position)) {
      const job = container.querySelectorAll<HTMLButtonElement>('.jc-job')[index];
      if (!job) throw new Error('missing job button');
      fireEvent.click(job);
      // 上の段 5 つ（タイムライン・求人内容はサブタブなし）と、サブタブ 10 個
      const tabs = [...container.querySelectorAll<HTMLButtonElement>('[role="tab"][id*="-group-"], [role="tab"][id*="-feature-"]')];
      expect(tabs.length).toBe(15);
      for (const tab of tabs) {
        await act(async () => { fireEvent.click(tab); await Promise.resolve(); });
        const wording = visibleWording();
        seen.push(wording);
        expect(wording, `${job.textContent} / ${tab.textContent}`).not.toMatch(JARGON_PATTERN);
        expect(wording, `${job.textContent} / ${tab.textContent}`).not.toMatch(CAUSAL_PATTERN);
      }
    }
    // データ取込（HubSpot・媒体・課金CSV・外部文面）と、そこから開く外部文面の照合
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'データ取込' })); await Promise.resolve(); });
    expect(visibleWording()).not.toMatch(JARGON_PATTERN);
    expect(visibleWording()).not.toMatch(CAUSAL_PATTERN);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '外部文面を照合する' })); await Promise.resolve(); });
    expect(container.querySelector('[role="tabpanel"][id$="-panel-receive"]')?.hasAttribute('hidden')).toBe(false);
    expect(visibleWording()).not.toMatch(JARGON_PATTERN);
    // 応募者の条件で探すパネル
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '応募者の条件で探す' })); await Promise.resolve(); });
    expect(visibleWording()).not.toMatch(JARGON_PATTERN);
    expect(visibleWording()).not.toMatch(CAUSAL_PATTERN);
    // 操作デモの市場タブは、タイムラインの「市場」の段と同じ架空の市場データを使い、サーバーへ問い合わせない。
    expect(seen.some(text => text.includes('架空の市場データ（デモ）'))).toBe(true);
    expect(seen.some(text => text.includes('ⓘ 集計の前提'))).toBe(true);
    expect(api.mock.calls.filter(call => String(call[0]).includes('/api/job-copy/market'))).toHaveLength(0);
  }, 60_000);

  it('shows no developer terms or causal wording on the cross-job overview', async () => {
    render(<JobCopyScreen />);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '横断比較' })); await Promise.resolve(); });
    expect(screen.getByRole('heading', { level: 1, name: '求人の横断比較' })).toBeTruthy();
    expect(visibleWording()).not.toMatch(JARGON_PATTERN);
    expect(visibleWording()).not.toMatch(CAUSAL_PATTERN);
    expect(visibleWording()).toContain('タイムラインの「市場」の段');
  });

  it('shows plain wording and the HubSpot application dates for a job opened from the HubSpot check', async () => {
    const record = { id: '901', properties: { hs_name: '合成ドライバー', shigotonaiyou: '仕事内容：配送\n給与：月給25万円', id_hrhakkaa: 'SYN-1', qinwude: '大分県大分市' } };
    api.mockImplementation((path: string) => {
      if (path.includes('/api/job-copy/market')) return Promise.resolve({ ok: true, data: market });
      if (path.includes('listing=')) return Promise.resolve({ ok: true, data: { metric: 'HubSpot応募レコード数', total_ms: 812, fetched_at: '2026-10-05T00:00:00Z', version_attribution: '現在の関連による集計。', attribute_basis: '現在取得できる属性',
        summary: { total: 5, duplicate_ids: 0, missing_date: 1, by_date: { '2026-09-20': 1, '2026-10-01': 3 }, dimensions: { gender: { 男性: 3, 女性: 1, 不明: 1 } } }, capture_bundle: null, dated_comparison: null } });
      if (path.includes('company=')) return Promise.resolve({ ok: true, data: { company_id: '77', portal_id: null, contracts: [{ id: '5', properties: { dealname: '合成契約' } }], jobs: [{ record, deal_ids: ['5'] }], total: 1, next_offset: null, total_ms: 345, fetched_at: '2026-10-05T00:00:00Z' } });
      return Promise.resolve({ ok: true, data: { customers: [{ id: '77', properties: { name: '合成取引先' } }], next_after: null, total_ms: 123 } });
    });
    const { container } = render(<JobCopyScreen />);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'データ取込' })); await Promise.resolve(); });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '取引先を取得' })); await Promise.resolve(); });
    await screen.findByRole('option', { name: '合成取引先' });
    expect(visibleWording()).not.toMatch(/\d+\s?ms/);
    fireEvent.change(screen.getByLabelText('HubSpotの取引先'), { target: { value: '77' } });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '関連する求人を取得' })); await Promise.resolve(); });
    await act(async () => { fireEvent.click(await screen.findByRole('button', { name: '合成ドライバー（関連契約1件）' })); await Promise.resolve(); });
    await waitFor(() => { expect(screen.getByText('応募を取得しました。')).toBeTruthy(); });
    expect(visibleWording()).not.toMatch(JARGON_PATTERN);
    expect(visibleWording()).not.toContain('shigotonaiyou');
    // HubSpot の応募日（4件、日付不明1件）がタイムラインと横断比較に届いている。
    const lane = screen.getByRole('group', { name: '応募' });
    expect(lane.textContent).not.toContain('応募日別の件数は未取得です');
    expect(screen.getByText('応募日が分からない応募 1件 はグラフに含めていません')).toBeTruthy();
    const tabs = [...container.querySelectorAll<HTMLButtonElement>('[role="tab"][id*="-group-"], [role="tab"][id*="-feature-"]')];
    for (const tab of tabs) {
      await act(async () => { fireEvent.click(tab); await Promise.resolve(); });
      expect(visibleWording(), tab.textContent).not.toMatch(JARGON_PATTERN);
      expect(visibleWording(), tab.textContent).not.toMatch(CAUSAL_PATTERN);
      expect(visibleWording(), tab.textContent).not.toContain('shigotonaiyou');
    }
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '横断比較' })); await Promise.resolve(); });
    const row = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getAllByRole('row')[1];
    expect(row?.textContent).toContain('合成ドライバー');
    expect(row?.textContent).not.toContain('応募未取得');
  }, 30_000);

  it('formats the selected version timestamp as YYYY/MM/DD HH:mm JST', () => {
    render(<JobCopyScreen />);
    // demo-001 の最新版 v3 は 2026-09-25T12:30:00+09:00 に取得した版（data.ts）。
    const meta = document.querySelector('.jc-record-meta')?.textContent ?? '';
    expect(meta).toContain('取得日時: 2026/09/25 12:30 JST');
    expect(meta).not.toContain('JST JST');
  });
});
