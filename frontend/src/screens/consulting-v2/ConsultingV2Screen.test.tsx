// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConsultingV2Screen } from './ConsultingV2Screen';
import { DATA_QUALITY, STAGE_STAY, FOCUS, BOARD, CUSTOMER, CUSTOMER_INDEX, DETAIL, GOALS, MTGQ, PHONE, RENEWAL_PIPE, RESULTS, TEAM, TODAY, TODAY_ALL_CANDIDATES } from './__fixtures__/fixtures';
import { caseManagementHref } from './format';

function reply(path: string): unknown {
  if (path.startsWith('/api/consulting/data-quality')) return DATA_QUALITY;
  if (path.startsWith('/api/consulting/stage-stay')) return STAGE_STAY;
  if (path.startsWith('/api/consulting/focus')) return FOCUS;
  if (path.startsWith('/api/consulting/team')) return TEAM;
  if (path.startsWith('/api/consulting/today')) return TODAY;
  if (path.startsWith('/api/consulting/deals')) return BOARD;
  if (path.startsWith('/api/consulting/deal-detail')) return DETAIL;
  if (path.startsWith('/api/consulting/renewal-pipe')) return RENEWAL_PIPE;
  if (path.startsWith('/api/consulting/customer?houjin=')) return CUSTOMER;
  if (path.startsWith('/api/consulting/customer')) return CUSTOMER_INDEX;
  if (path.startsWith('/api/consulting/results')) return RESULTS;
  if (path.startsWith('/api/consulting/mtg-quality')) return MTGQ;
  if (path.startsWith('/api/consulting/phone')) return PHONE;
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

describe('今のコンサルKPIとそろえた表示', () => {
  it('データの古さと取得日時を常に出す（7日前なら警告の色）', async () => {
    open('');
    render(<ConsultingV2Screen />);
    const box = await screen.findByText('7日前のデータです（2026-10-26 14:01）。');
    expect(box.closest('.cv2-fresh')?.className).toContain('cv2-fresh-bad');
  });

  it('担当者ごとの表の既定の並びは「手が回っていない案件」の多い順', async () => {
    open('');
    render(<ConsultingV2Screen />);
    const table = await screen.findByRole('table', { name: '担当者ごとの状態' });
    const names = within(table).getAllByRole('row').slice(1, 3).map((r) => r.textContent);
    expect(names[0]).toContain('架空担当みどり'); // 手が回っていない 2
    expect(names[1]).toContain('架空担当あお'); // 手が回っていない 1
  });

  it('上の札にも前回との差を出す', async () => {
    open('');
    render(<ConsultingV2Screen />);
    await screen.findByText('担当者ごとの状態（2 名）');
    expect(screen.getByText('注意が 2 つ以上の案件').closest('.cv2-kpi')?.textContent).toContain('+1');
    expect(screen.getAllByText('今週満了')[0]?.closest('.cv2-kpi')?.textContent).toContain('−1');
  });
});

describe('コンサルKPI v2 第2段', () => {
  it('案件一覧に応募・面接・採用・目標・接触率・採用単価の列と HubSpot へのリンクが出る', async () => {
    open('?view=board&c=架空担当あお');
    render(<ConsultingV2Screen />);
    const row = (await screen.findByText('架空株式会社A 本社')).closest('tr') as HTMLElement;
    const text = row.textContent;
    expect(text).toContain('12'); // 応募
    expect(text).toContain('持越'); // 面接は前の値を引き継いだ
    expect(text).toContain('25%'); // 採用目標に対して
    expect(text).toContain('1/4（途中）');
    expect(text).toContain('40%');
    expect(text).toContain('2/5 か月');
    expect(text).toContain('120万');
    expect(text).toContain('80万'); // 同じ進み具合の区分の中央
    const hs = within(row).getByRole('link', { name: /HubSpot で開く/ });
    expect(hs.getAttribute('href')).toBe('https://app.hubspot.com/contacts/12345/record/0-3/70000000001/');
  });

  it('満了と継続は API の件数・金額をそのまま出し、金額が空なら金額なしと書く', async () => {
    open('?view=renewal');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('今月 2026-11 に満了する契約（満了の近い順、1 件）')).toBeTruthy();
    expect(screen.getAllByText(/金額が空 1 件/).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/金額なし（金額が空 0 件）/).length).toBe(2);
  });

  it('顧客と拠点: 法人の一覧から法人を開ける', async () => {
    open('?view=customer');
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('法人を選ぶ（2 社）')).toBeTruthy();
    expect(screen.getByText('1,200人')).toBeTruthy();
    open('?view=customer&houjin=9000000000001');
    cleanup();
    render(<ConsultingV2Screen />);
    expect(await screen.findByText('拠点: 本社（契約 2 件）')).toBeTruthy();
    expect(screen.getByText('累計契約金額')).toBeTruthy();
    expect(screen.getByText('210万')).toBeTruthy();
  });

  it('成果と継続: 月次の継続率・金額継続率・結果待ちの月を API の値で出す', async () => {
    open('?view=results');
    render(<ConsultingV2Screen />);
    // 結果待ちが残る 10 月は札にせず、結果がそろった 9 月を使う
    expect(await screen.findByText('月次の継続率（2026-09 に満了）')).toBeTruthy();
    expect(screen.getAllByText('66.7%').length).toBeGreaterThan(0);
    expect(screen.getAllByText('72.7%').length).toBeGreaterThan(0);
    expect(await screen.findByText('MTG の品質')).toBeTruthy();
    expect(await screen.findByText('電話の到達')).toBeTruthy();
  });

  it('接触の推移の図が出る', async () => {
    open('');
    render(<ConsultingV2Screen />);
    expect(await screen.findByRole('img', { name: '担当ごとの1件あたりの接触回数の推移' })).toBeTruthy();
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


describe('コンサルKPI v2 第3段', () => {
  it('今日の札は一覧を切り替え、もう一度押すと解除する。開始前も表示できる', async () => {
    open('?view=today');
    render(<ConsultingV2Screen />);
    await screen.findByText('今日動く先（2 件）');
    const card = screen.getByRole('button', { name: /今週始まった契約/ });
    fireEvent.click(card);
    expect(card.getAttribute('aria-pressed')).toBe('true');
    let table = screen.getByRole('table', { name: '今日動く案件' });
    expect(within(table).getAllByRole('row')).toHaveLength(2);
    expect(table.textContent).toContain('架空株式会社B 第二拠点');
    expect(table.textContent).not.toContain('架空株式会社A 本社');
    fireEvent.click(card);
    expect(card.getAttribute('aria-pressed')).toBe('false');
    expect(within(table).getAllByRole('row')).toHaveLength(3);
    fireEvent.click(screen.getByRole('button', { name: /開始前の契約/ }));
    table = screen.getByRole('table', { name: '今日動く案件' });
    expect(table.textContent).toContain('架空株式会社D 開始前');
    expect(within(table).getAllByRole('row')).toHaveLength(2);
  });

  it('今日の担当絞り込みは上位24件の外の候補も表示し、札の解除後も担当を保つ', async () => {
    const fetcher = fetch;
    vi.stubGlobal('fetch', vi.fn<typeof fetch>((input, init) => input === '/api/consulting/today'
      ? Promise.resolve(new Response(JSON.stringify(TODAY_ALL_CANDIDATES), { status: 200, headers: { 'Content-Type': 'application/json' } })) : fetcher(input, init)));
    open('?view=today');
    render(<ConsultingV2Screen />);
    await screen.findByText('今日動く先（24 件）');
    const table = screen.getByRole('table', { name: '今日動く案件' });
    expect(within(table).getAllByRole('row')).toHaveLength(25);
    expect(table.textContent).not.toContain('架空株式会社E 候補のみ');
    const card = screen.getByRole('button', { name: /今日動く候補/ });
    fireEvent.click(card);
    fireEvent.change(screen.getByRole('combobox', { name: 'コンサル担当者' }), { target: { value: '架空担当きいろ' } });
    expect(screen.getByText('今日動く先（1 件）')).toBeTruthy();
    expect(within(table).getAllByRole('row')).toHaveLength(2);
    expect(table.textContent).toContain('架空株式会社E 候補のみ');
    expect(table.textContent).not.toContain('架空上位案件');
    fireEvent.click(card);
    expect(card.getAttribute('aria-pressed')).toBe('false');
    expect(table.textContent).toContain('架空株式会社E 候補のみ');
    fireEvent.change(screen.getByRole('combobox', { name: 'コンサル担当者' }), { target: { value: '' } });
    expect(within(table).getAllByRole('row')).toHaveLength(25);
    expect(table.textContent).not.toContain('架空株式会社E 候補のみ');
  });

  it.each([false, true])('今日の全候補が無い応答は一覧へ戻し、空配列なら0件のままにする（空配列: %s）', async (empty) => {
    const fetcher = fetch;
    vi.stubGlobal('fetch', vi.fn<typeof fetch>((input, init) => input === '/api/consulting/today'
      ? Promise.resolve(new Response(JSON.stringify({ ...TODAY, candidates: empty ? [] : undefined }), {
        status: 200, headers: { 'Content-Type': 'application/json' },
      })) : fetcher(input, init)));
    open('?view=today&c=架空担当あお');
    render(<ConsultingV2Screen />);
    await screen.findByText(`今日動く先（${empty ? '0' : '1'} 件）`);
    fireEvent.click(screen.getByRole('button', { name: /今日動く候補/ }));
    if (empty) {
      const section = screen.getByText('今日動く先（0 件）').closest('section');
      if (!section) throw new Error('今日動く先のセクションがありません');
      expect(section.textContent).toContain('この条件に合う案件はありません。');
      expect(section.textContent).not.toContain('架空株式会社A 本社');
    } else {
      const table = screen.getByRole('table', { name: '今日動く案件' });
      expect(within(table).getAllByRole('row')).toHaveLength(2);
      expect(table.textContent).toContain('架空株式会社A 本社');
      expect(table.textContent).not.toContain('架空株式会社B 第二拠点');
    }
  });

  it('滞留の両方の表で内部IDの代わりにステージの表示名を出す', async () => {
    open('?view=today');
    render(<ConsultingV2Screen />);
    fireEvent.click(await screen.findByText('滞留した案件'));
    for (const name of ['同じステージに長く止まっている案件', '満了後も稼働中の案件']) {
      const table = await screen.findByRole('table', { name });
      const row = within(table).getAllByRole('row')[1];
      if (!row) throw new Error('滞留案件の行がありません');
      expect(within(row).getAllByRole('cell')[2]?.textContent).toBe('稼働中');
      expect(table.textContent).not.toContain('fiction-active-stage');
    }
  });

  it('箱ひげの五数・平均・件数と立ち上がり、2軸の未測定、データ品質をAPIの具体値で出す', async () => {
    open('?view=results');
    render(<ConsultingV2Screen />);
    await screen.findByText('継続回数ごとの成果のばらつき');
    screen.getAllByText('数字を表で見る').forEach((summary) => { fireEvent.click(summary); });
    const box = await screen.findByRole('table', { name: '応募数の散らばり（継続回数ごと）' });
    expect(within(box).getAllByRole('row').slice(1, 2).flatMap((row) => within(row).getAllByRole('cell').map((c) => c.textContent))).toEqual(['初回', '37', '0.00', '9.00', '12.00', '19.00', '27.00', '22.31']);
    const chart = screen.getByRole('img', { name: '応募数の散らばり（継続回数ごと）' });
    expect(Number(chart.querySelector('rect')?.getAttribute('x'))).toBe(300); // API の q1=9 を座標に変換
    expect(Number(chart.querySelector('rect')?.getAttribute('width'))).toBeCloseTo(166.6667); // q3=19
    expect(await screen.findByText('契約前のMTG 7 件は含めていません。')).toBeTruthy();
    const launch = screen.getByRole('img', { name: '初回契約の開始月ごとの応募の推移' });
    expect(launch.querySelectorAll('circle')).toHaveLength(1); // 31件の4.7だけ、2件の9.9は描かない
    expect(launch.querySelector('circle')?.textContent).toContain('4.7（31件）');
    expect(screen.getByRole('img', { name: '最優先の広がり' }).querySelector('circle')?.getAttribute('fill')).toBe('white');
    const axes = screen.getByRole('table', { name: 'リスクの2軸' });
    expect(within(axes).getAllByRole('row')[1]?.textContent).toContain('放置5119');
    fireEvent.click(await screen.findByText('データの欠けと記入率の詳細'));
    expect(await screen.findByRole('table', { name: '結果ごとの記入率' })).toBeTruthy();
    const dq = screen.getByRole('table', { name: '結果ごとの記入率' });
    expect(dq.textContent).toContain('応募数解約13753.8%');
    expect(screen.getByRole('table', { name: '読んでいるシート' }).textContent).toContain('架空取引シート123');
    expect(screen.getByRole('table', { name: '読んでいるシート' }).textContent).toContain('架空録画シート—');
  });

  it('滞留はAPIの印で分け、担当で絞る。履歴が無い件数も残す', async () => {
    open('?view=today&c=架空担当あお');
    render(<ConsultingV2Screen />);
    fireEvent.click(await screen.findByText('滞留した案件'));
    const table = await screen.findByRole('table', { name: '同じステージに長く止まっている案件' });
    expect(table.textContent).toContain('架空株式会社A 本社');
    expect(table.textContent).toContain('57');
    expect(table.textContent).not.toContain('架空株式会社C 支店');
    expect(await screen.findByText(/入った日が分からない案件 4 件/)).toBeTruthy();
  });

  it('満了日超過は今月分も含め、APIの印で選んで過ぎた日数の順に出す', async () => {
    open('?view=renewal');
    render(<ConsultingV2Screen />);
    await screen.findByText('満了日超過の稼働中（今月の満了も含む）');
    fireEvent.click(screen.getByText('満了日超過の稼働中（今月の満了も含む）'));
    const table = await screen.findByRole('table', { name: '満了日超過の稼働中' });
    expect(table.textContent).toContain('架空株式会社C 支店');
    expect(table.textContent).toContain('12');
    expect(table.textContent).not.toContain('架空株式会社A 本社');
  });

  it('注目顧客の満足度と採用単価は別々の指標として具体値を出す', async () => {
    open('?view=customer');
    render(<ConsultingV2Screen />);
    await screen.findByText('注目顧客（満足度・採用単価・MTG の記録）');
    fireEvent.click(screen.getByText('注目顧客（満足度・採用単価・MTG の記録）'));
    const table = await screen.findByRole('table', { name: '満足度が低い顧客' });
    expect(table.textContent).toContain('架空株式会社A 本社');
    expect(table.textContent).toContain('3');
    expect(screen.getByRole('table', { name: '採用単価が悪化した拠点' }).textContent).toContain('架空本社40万60万1.50');
  });

  it.each(['data-quality', 'stage-stay', 'focus', 'deals'])('%s の取得失敗を空一覧にしない', async (part) => {
    const fetcher = fetch;
    vi.stubGlobal('fetch', vi.fn<typeof fetch>((input, init) => (typeof input === 'string' ? input : input instanceof URL ? input.href : input.url).startsWith(`/api/consulting/${part}`)
      ? Promise.resolve(new Response('{}', { status: 500 })) : fetcher(input, init)));
    open(`?view=${part === 'data-quality' ? 'results' : part === 'stage-stay' ? 'today' : part === 'focus' ? 'customer' : 'renewal'}`);
    render(<ConsultingV2Screen />);
    if (part === 'stage-stay') fireEvent.click(await screen.findByText('滞留した案件'));
    if (part === 'focus') fireEvent.click(await screen.findByText('注目顧客（満足度・採用単価・MTG の記録）'));
    if (part === 'deals') fireEvent.click(await screen.findByText('満了日超過の稼働中（今月の満了も含む）'));
    expect(await screen.findByText('読み込めませんでした。')).toBeTruthy();
  });
});


it('少数の成果は図から外し、nullは表でも0にしない', async () => {
  const fetcher = fetch;
  vi.stubGlobal('fetch', vi.fn<typeof fetch>((input, init) => input === '/api/consulting/results'
    ? Promise.resolve(new Response(JSON.stringify({ ...RESULTS, renewal: { ...RESULTS.renewal, by_renewal: [
      { ...RESULTS.renewal?.by_renewal?.[0], denom: 29, syoudaku: null },
    ] } }), { status: 200, headers: { 'Content-Type': 'application/json' } })) : fetcher(input, init)));
  open('?view=results');
  render(<ConsultingV2Screen />);
  await screen.findByText('継続回数ごとの成果のばらつき');
  expect(screen.queryByRole('img', { name: '応募数の散らばり（継続回数ごと）' })).toBeNull();
  screen.getAllByText('数字を表で見る').forEach((summary) => { fireEvent.click(summary); });
  const table = screen.getByRole('table', { name: '承諾数の散らばり（継続回数ごと）' });
  expect(within(table).getAllByRole('row').slice(1, 2).flatMap((row) => within(row).getAllByRole('cell').map((c) => c.textContent))).toEqual(['初回', '—', '—', '—', '—', '—', '—', '—']);
});
