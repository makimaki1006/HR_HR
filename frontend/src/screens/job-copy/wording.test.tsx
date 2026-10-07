// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
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
  ctk_basis: 'Indeed上の行動データで労働市場全体ではありません。既存市場レポートのctk_countで、応募者数やHRハッカーのクリック数ではありません。',
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
    // 市場タブでは、サーバーの説明文が言い換えられて「ⓘ 集計の前提」に入っている。
    expect(seen.some(text => text.includes('既存市場レポートのIndeed閲覧者指標で'))).toBe(true);
    expect(seen.some(text => text.includes('ⓘ 集計の前提'))).toBe(true);
    expect(api).toHaveBeenCalled();
  }, 60_000);

  it('formats the selected version timestamp as YYYY/MM/DD HH:mm JST', () => {
    render(<JobCopyScreen />);
    // demo-001 の最新版 v3 は 2026-09-25T12:30:00+09:00 に取得した版（data.ts）。
    const meta = document.querySelector('.jc-record-meta')?.textContent ?? '';
    expect(meta).toContain('取得日時: 2026/09/25 12:30 JST');
    expect(meta).not.toContain('JST JST');
  });
});
