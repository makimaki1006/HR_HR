// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConsultingV2Screen } from './ConsultingV2Screen';
import { BOARD, DETAIL, GOALS, TEAM, TODAY } from './__fixtures__/fixtures';
import { caseManagementHref } from './format';

function reply(path: string): unknown {
  if (path.startsWith('/api/consulting/team')) return TEAM;
  if (path.startsWith('/api/consulting/today')) return TODAY;
  if (path.startsWith('/api/consulting/deals')) return BOARD;
  if (path.startsWith('/api/consulting/deal-detail')) return DETAIL;
  if (path.startsWith('/api/crm/workspace/deals/')) return GOALS;
  return null;
}

let called: string[] = [];

beforeEach(() => {
  called = [];
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof fetch>((input) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.pathname + input.search : input.url;
      called.push(url);
      const body = reply(url);
      return Promise.resolve(
        new Response(JSON.stringify(body), { status: body === null ? 404 : 200, headers: { 'Content-Type': 'application/json' } }),
      );
    }),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  window.history.replaceState(null, '', '/app/consulting-v2');
});

function open(search: string): void {
  window.history.replaceState(null, '', `/app/consulting-v2${search}`);
}

describe('コンサルKPI v2', () => {
  it('チームと担当が既定で開き、今のコンサルKPIへのリンクがある', async () => {
    open('');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('担当者ごとの状態（2 名）')).toBeTruthy();
    expect(screen.getByRole('link', { name: '今のコンサルKPIを開く' }).getAttribute('href')).toBe('/consulting');
    // 今のコンサルKPI (/consulting) と同じ API だけを読む。書き込みや別の API は呼ばない
    expect(called.every((u) => u.startsWith('/api/consulting/'))).toBe(true);
    // 架空データの注意書きは出さない (実データを表示する画面)
    expect(document.body.textContent).not.toContain('架空のデータです');
  });

  it('チームの数字が API の値のまま出る (作った数字を出さない)', async () => {
    open('');
    render(<ConsultingV2Screen />);
    const table = await screen.findByRole('table', { name: '担当者ごとの状態' });
    const row = within(table).getByText('架空担当あお').closest('tr');
    expect(row).not.toBeNull();
    const cells = within(row as HTMLElement).getAllByRole('cell');
    expect(cells[1]?.textContent).toBe('1'); // 持ち件数
    expect(cells[3]?.textContent).toBe('1+1'); // 注意2つ以上 + 前回との差
    expect(cells[9]?.textContent).toContain('35.0%'); // 接触した月の割合
    expect(cells[9]?.textContent).toContain('2/6 か月');
    expect(cells[10]?.textContent).toContain('1.00'); // 1件あたりの接触回数
  });

  it('案件一覧が担当で絞れ、実取引 ID の案件管理リンクが相対 URL になる', async () => {
    open('?view=board&c=架空担当みどり');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText(/3 件中 2 件/)).toBeTruthy();
    const links = screen.getAllByRole('link', { name: /案件管理で確認/ });
    expect(links.map((a) => a.getAttribute('href'))).toEqual(['/app/job-copy?deal=70000000002', '/app/job-copy?deal=70000000003']);
    // 満了を過ぎた案件は「何ヶ月目」に月数を出さず、満了後と書く
    expect(screen.getAllByText('満了後')).toHaveLength(2); // 段階と何ヶ月目 (旧画面と同じ)
    expect(screen.getByText('満了を12日過ぎている')).toBeTruthy();
    // 金額が空の案件は 0 ではなく —
    const rowC = screen.getByText('架空株式会社C 支店').closest('tr') as HTMLElement;
    expect(within(rowC).getAllByRole('cell').at(-2)?.textContent).toBe('—');
  });

  it('今日の画面は API が返した候補をそのまま並べる', async () => {
    open('?view=today');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('今日動く先（2 件）')).toBeTruthy();
    expect(screen.getByText('候補は 2 件です（架空の説明文）。')).toBeTruthy();
  });

  it('案件の詳細に接触履歴・目標・契約の連なり・案件管理へのリンクが出る', async () => {
    open('?view=detail&id=70000000001');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('接触履歴（新しい順）')).toBeTruthy();
    // 既定は電話 60 秒超だけ・応募の報告メールなし (旧画面と同じ)
    expect(screen.getByText(/6 件中 2 件/)).toBeTruthy();
    expect(screen.queryByText('架空の応募報告')).toBeNull();
    expect(screen.getByRole('button', { name: /もっと前を出す.*2 件/ })).toBeTruthy();
    expect(screen.getByText('3分12秒')).toBeTruthy();
    expect(await screen.findByText('応募を月10件にする')).toBeTruthy();
    expect(screen.getByText('契約の連なり（同じ拠点の本体契約）')).toBeTruthy();
    const link = screen.getAllByRole('link', { name: /案件管理で確認/ })[0];
    expect(link?.getAttribute('href')).toBe('/app/job-copy?deal=70000000001');
    expect(link?.getAttribute('href')?.startsWith('http')).toBe(false);
    await waitFor(() => { expect(called.some((u) => u.startsWith('/api/crm/workspace/deals/70000000001?deal_props='))).toBe(true); });
  });

  it('API が失敗したら 0 件に見せず、読み込めなかったと出す', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(new Response('{}', { status: 500 }))));
    open('?view=board');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('読み込めませんでした。')).toBeTruthy();
    expect(screen.queryByText('この条件に合う案件はありません。')).toBeNull();
  });
});

describe('案件管理へのリンク', () => {
  it('取引 ID が数字なら ?deal= を付け、それ以外は一覧へ', () => {
    expect(caseManagementHref('70000000001')).toEqual({ href: '/app/job-copy?deal=70000000001', linked: true });
    expect(caseManagementHref('A')).toEqual({ href: '/app/job-copy', linked: false });
    expect(caseManagementHref('')).toEqual({ href: '/app/job-copy', linked: false });
    expect(caseManagementHref(null)).toEqual({ href: '/app/job-copy', linked: false });
  });
});
