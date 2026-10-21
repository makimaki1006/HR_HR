// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { parseAdminRoute, adminHref } from './route';
import { TalkCallsTable, TalkDetailView, TalkPanel, TalkSummaryTable, type TalkRoute } from './TalkPanel';
import { activePeriod, describeDays, fixtureCalls, fixtureDetail, fixtureSummary, periodRange, sortPeople, judgeItem, judgeAppointment, donePct } from './talkModel';

afterEach(cleanup);

const summary = fixtureSummary('2026-10-19', '2026-10-19', '');
const rowOf = (name: string) => within(screen.getByTestId('talk-table')).getByText(name).closest('tr') as HTMLElement;
const route = (patch: Partial<TalkRoute> = {}): TalkRoute => ({ view: 'talk', from: '2026-10-19', to: '2026-10-19', team: '', owner: '', call: '', ...patch });

describe('トークの振り返り: 人ごとの表', () => {
  it('アポ率は小数2桁、印のセルだけ目立たせる', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const jiro = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'));
    // 名前・チーム・発信数・架電数・アポ・アポ率・判定数・4項目
    expect(jiro[3]?.textContent).toBe('310');
    expect(jiro[5]?.textContent).toBe('0.65%');
    expect(jiro[5]?.getAttribute('data-flagged')).toBe('true');
    expect(jiro[6]?.textContent).toContain('70');
    expect(jiro[6]?.textContent).toContain('待ち 6 ・ 取れない 4');
    // 日程を早く出す: 36/45 = 80% が印、受付は 25% で印なし
    expect(jiro[9]?.textContent).toBe('20%できた 9/45本');
    expect(jiro[9]?.getAttribute('data-flagged')).toBe('true');
    expect(jiro[7]?.getAttribute('data-flagged')).toBeNull();
    const hanako = Array.from(rowOf('架空 花子').querySelectorAll('td'));
    expect(hanako[5]?.textContent).toBe('2.27%');
    expect(hanako[5]?.getAttribute('data-flagged')).toBeNull();
    // 項目の印は別の人 (断られた後 14/16)
    expect(Array.from(rowOf('ダミー 三郎').querySelectorAll('td'))[15]?.getAttribute('data-flagged')).toBe('true');
  });

  it('見出しに資料の全体の割合と決まりを出し、最後の行はチーム全体', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const head = screen.getByRole('columnheader', { name: /早く日付を出す/ });
    expect(head.textContent).toContain('この表の全体 43%');
    expect(head.getAttribute('title')).toContain('資料の全体: できた 19%（話せた電話のうち）');
    expect(head.getAttribute('title')).toContain('具体的な週・曜日・日');
    expect(screen.getByRole('columnheader', { name: /既存感/ }).textContent).toContain('この表の全体 64%');
    const rows = within(screen.getByTestId('talk-table')).getAllByRole('row');
    const last = Array.from(rows.at(-1)?.querySelectorAll('td') ?? []).map(td => td.textContent);
    expect(last.slice(0, 6)).toEqual(['チーム全体', '-', '1771', '770', '12', '1.56%']);
    expect(last[9]).toBe('43%できた 54/125本');
    // 場面ごとのまとまり
    expect(Array.from(document.querySelectorAll('th.talk-group')).map(t => `${t.textContent}:${t.getAttribute('colspan') ?? ''}`)).toEqual(['受付:1', '日付の前:5', '断られた後:3']);
  });

  it('色の凡例を表の上に出し、割合はできた割合だと書く', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const legend = screen.getByTestId('talk-legend').textContent;
    expect(legend).toContain('20 ポイント以上できていない');
    expect(legend).toContain('5 本以上の人だけ');
    expect(legend).toContain('本数が少ないので色を付けていない');
    expect(legend).toContain('架電数 100 本以上');
    expect(legend).toContain('半分未満');
    expect(screen.getByTestId('talk-rules').textContent).toContain('できた電話の割合です');
  });

  it('判定した本数と対象の本数を並べ、割合は判定した電話の中の割合だと書く', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    // 二郎: 判定 70、待ち 6、取れない 4、枠を超えて判定していない 35 → 対象 115
    const cell = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'))[6]?.textContent ?? '';
    expect(cell).toContain('70');
    expect(cell).toContain('対象 115 本');
    const note = screen.getByTestId('talk-sampling').textContent;
    expect(note).toContain('判定した電話の中での割合');
    expect(note).toContain('1 人 1 日あたり新しい電話から 10 本まで');
  });

  it('架電数は営業KPI と同じ数だと書き、読めなければ「—」を出す', () => {
    expect(screen.queryByTestId('talk-dials-rule')).toBeNull();
    const { unmount } = render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    expect(screen.getByRole('columnheader', { name: '架電数' })).toBeTruthy();
    expect(screen.getByRole('columnheader', { name: '発信数' })).toBeTruthy();
    const heads = screen.getAllByRole('columnheader').map(h => h.textContent);
    expect(heads.findIndex(t => t.startsWith('発信数'))).toBe(heads.findIndex(t => t.startsWith('架電数')) - 1);
    expect(screen.getByTestId('talk-dials-rule').textContent).toBe('発信数は Zoom の発信すべて（相手が出る前に切ったもの・つながらなかったものを含む）、架電数はそのうちつながった通話の数で、営業KPI と同じです。アポ率 ＝ アポ数 ÷ 架電数です。');
    // 発信数・架電数の列の値と、チーム全体の行は人の合計
    const first = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'));
    const me = summary.people.find(p => p.name === 'サンプル 二郎');
    expect(first[2]?.textContent).toBe(String(me?.outbound));
    expect(first[3]?.textContent).toBe(String(me?.dials));
    expect(summary.total.outbound).toBe(summary.people.reduce((a, p) => a + (p.outbound ?? 0), 0));
    expect(summary.total.outbound).toBeGreaterThan(summary.total.dials ?? 0);
    unmount();
    const none = { ...summary, people: summary.people.map(p => ({ ...p, dials: null, appointment_rate_pct: null })) };
    render(<TalkSummaryTable data={none} onOpen={() => undefined} />);
    const cells = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'));
    expect(cells[3]?.textContent).toBe('—');
    expect(cells[5]?.textContent).toBe('-');
  });

  it('数えている途中の日を日付でまとめて伝える', () => {
    expect(describeDays(['2026-10-08', '2026-10-06', '2026-10-07', '2026-10-10'])).toBe('10/06〜10/08、10/10');
    expect(describeDays(['2026-10-06'])).toBe('10/06');
    expect(describeDays([])).toBe('');
  });

  it('アポ率の見出しで並べ替える', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const names = () => within(screen.getByTestId('talk-table')).getAllByTestId('talk-row').map(r => r.querySelector('td')?.textContent);
    fireEvent.click(screen.getByRole('button', { name: /アポ率/ }));
    expect(names()).toEqual(['ダミー 三郎', '架空 花子', '架空 四郎', 'サンプル 二郎']);
    fireEvent.click(screen.getByRole('button', { name: /アポ率/ }));
    expect(names()[0]).toBe('サンプル 二郎');
  });

  it('架電数・アポ数・判定した電話数の見出しでも降順→昇順に並べ替える', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const names = () => within(screen.getByTestId('talk-table')).getAllByTestId('talk-row').map(r => r.querySelector('td')?.textContent);
    fireEvent.click(screen.getByRole('button', { name: /^架電数/ }));
    expect(names()[0]).toBe('サンプル 二郎'); // 310 ではなく最多の人
    fireEvent.click(screen.getByRole('button', { name: /^架電数/ }));
    expect(screen.getByRole('columnheader', { name: /架電数/ }).getAttribute('aria-sort')).toBe('ascending');
    fireEvent.click(screen.getByRole('button', { name: /^アポ数/ }));
    expect(screen.getByRole('columnheader', { name: /アポ数/ }).getAttribute('aria-sort')).toBe('descending');
    fireEvent.click(screen.getByRole('button', { name: /^判定した電話数/ }));
    expect(screen.getByRole('columnheader', { name: /判定した電話数/ }).getAttribute('aria-sort')).toBe('descending');
  });

  it('名前を押すと担当者 ID で開く', () => {
    const onOpen = vi.fn();
    render(<TalkSummaryTable data={summary} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole('button', { name: '架空 花子' }));
    expect(onOpen).toHaveBeenCalledWith('9001');
  });

  it('動いていないときは案内を出す', () => {
    render(<TalkSummaryTable data={{ ...summary, enabled: false, people: [] }} onOpen={() => undefined} />);
    expect(screen.getByTestId('talk-disabled').textContent).toContain('まだ動いていません');
    expect(screen.queryByTestId('talk-table')).toBeNull();
  });
});

describe('トークの振り返り: 電話の一覧と詳細', () => {
  it('状態を平易な言葉で出し、判定は ○ × −', () => {
    render(<TalkCallsTable data={fixtureCalls('9001', '2026-10-19', '2026-10-19')} onOpen={() => undefined} />);
    const rows = screen.getAllByTestId('talk-call-row');
    expect(rows).toHaveLength(6);
    expect(Array.from(rows[0]?.querySelectorAll('td') ?? []).map(td => td.textContent)).toEqual(
      ['10/19 15:42', '3分32秒', '日程が決まらなかった', '○', '×', '×', '×', '○', '−', '×', '×', '−', '日程の前に「今は募集していないですか」と聞き、「忙しい」と言われて引き下がっています。']);
    expect(rows[3]?.textContent).toContain('文字起こし待ち');
    expect(rows[4]?.textContent).toContain('録音が残っていない');
    expect(rows[5]?.textContent).toContain('判定できなかった');
  });

  it('できていない根拠の発言に色が付き、営業と先方を見分けられる', () => {
    render(<TalkDetailView data={fixtureDetail('fx-9001-1')} />);
    const turns = screen.getAllByTestId('talk-turn');
    expect(turns).toHaveLength(10);
    const marked = turns.filter(t => t.getAttribute('data-evidence') === 'true').map(t => t.textContent);
    expect(marked).toHaveLength(3); // 発言 5・7・9
    expect(marked[0]).toContain('できていない根拠: 日付の前に需要を聞かない');
    expect(turns[4]?.className).toContain('rep');
    expect(turns[5]?.className).toContain('customer');
    expect(turns[4]?.textContent).toContain('営業');
    expect(turns[5]?.textContent).toContain('先方');
    expect(screen.getByText(/別の週・日をもう1回出す/).textContent).toContain('次に言うとよいこと');
    expect(screen.getByRole('link', { name: 'HubSpot で開く' }).getAttribute('href')).toContain('hubspot.com');
  });

  it('文字起こしが取れないときは理由を出す', () => {
    render(<TalkDetailView data={fixtureDetail('fx-9001-2')} />);
    expect(screen.getByTestId('talk-transcript').textContent).toContain('録音が残っていません');
    expect(screen.queryAllByTestId('talk-turn')).toHaveLength(0);
  });
});

describe('トークの振り返り: 画面とルート', () => {
  it('架空サンプルで表が出て、名前を押すと owner 付きで移る', () => {
    const navigate = vi.fn();
    render(<TalkPanel route={route()} navigate={navigate} fixture />);
    expect(screen.getByText('架空サンプルを表示中です。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '架空 花子' }));
    expect(navigate).toHaveBeenCalledWith(route({ owner: '9001' }));
  });

  it('期間とチームの切り替えは from / to / team を変える', () => {
    const navigate = vi.fn();
    render(<TalkPanel route={route()} navigate={navigate} fixture />);
    fireEvent.change(screen.getByLabelText('チームで絞り込む'), { target: { value: '架空営業チーム' } });
    expect(navigate).toHaveBeenLastCalledWith(route({ team: '架空営業チーム' }));
    fireEvent.click(screen.getByRole('button', { name: '昨日' }));
    const last = navigate.mock.calls[navigate.mock.calls.length - 1]?.[0] as TalkRoute;
    expect(last.from).toBe(last.to);
    expect(last.owner).toBe('');
  });

  it('日付を変えただけでは読み直さず、「決定」で from / to を変える', () => {
    const navigate = vi.fn();
    render(<TalkPanel route={route()} navigate={navigate} fixture />);
    const apply = screen.getByRole<HTMLButtonElement>('button', { name: '決定' });
    expect(apply.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('開始日'), { target: { value: '2026-10-01' } });
    fireEvent.change(screen.getByLabelText('終了日'), { target: { value: '2026-10-15' } });
    expect(navigate).not.toHaveBeenCalled();
    fireEvent.click(apply);
    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenLastCalledWith(route({ from: '2026-10-01', to: '2026-10-15' }));
  });

  it('開始日が終了日より後なら「決定」を押せない', () => {
    const navigate = vi.fn();
    render(<TalkPanel route={route()} navigate={navigate} fixture />);
    fireEvent.change(screen.getByLabelText('開始日'), { target: { value: '2026-10-25' } });
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '決定' }).disabled).toBe(true);
    expect(navigate).not.toHaveBeenCalled();
  });

  it('週は月曜始まり', () => {
    // 2026-10-19 は月曜
    expect(periodRange('thisWeek', '2026-10-21')).toEqual({ from: '2026-10-19', to: '2026-10-21' });
    expect(periodRange('lastWeek', '2026-10-21')).toEqual({ from: '2026-10-12', to: '2026-10-18' });
    expect(periodRange('lastWeek', '2026-10-18')).toEqual({ from: '2026-10-05', to: '2026-10-11' });
    expect(periodRange('yesterday', '2026-11-01')).toEqual({ from: '2026-10-31', to: '2026-10-31' });
    expect(activePeriod('2026-10-12', '2026-10-18', '2026-10-21')).toBe('lastWeek');
    expect(activePeriod('2026-10-01', '2026-10-03', '2026-10-21')).toBe('custom');
  });

  it('URL のクエリを往復できる', () => {
    const r = parseAdminRoute('?view=talk&from=2026-10-12&to=2026-10-18&team=A&owner=9001&call=c1');
    expect(r).toEqual({ view: 'talk', from: '2026-10-12', to: '2026-10-18', team: 'A', owner: '9001', call: 'c1' });
    expect(adminHref(r)).toBe('?view=talk&from=2026-10-12&to=2026-10-18&team=A&owner=9001&call=c1');
    expect(parseAdminRoute('?view=talk')).toEqual({ view: 'talk', from: '', to: '', team: '', owner: '', call: '' });
  });
});

describe('トークの振り返り: 集計中と未設定', () => {
  const mockFetch = (responses: { status: number; body: unknown }[]) => {
    const f = vi.fn().mockImplementation(() => {
      const r = responses.shift() ?? { status: 200, body: {} };
      return Promise.resolve(new Response(JSON.stringify(r.body), { status: r.status, headers: { 'Content-Type': 'application/json' } }));
    });
    vi.stubGlobal('fetch', f);
    return f;
  };
  afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });

  it('503 counting のあいだは「集計しています…」を出し、数秒おきに取り直して表を出す', async () => {
    vi.useFakeTimers();
    const f = mockFetch([{ status: 503, body: { error_kind: 'counting' } }, { status: 200, body: summary }]);
    render(<TalkPanel route={route()} navigate={() => undefined} fixture={false} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(50); });
    expect(screen.getByTestId('talk-counting').textContent).toContain('集計しています');
    expect(f).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(4100); });
    expect(f).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('talk-table')).toBeTruthy();
    expect(screen.queryByTestId('talk-counting')).toBeNull();
  });

  it('集計が終わらないときは取り直しを打ち切って案内を出す', async () => {
    vi.useFakeTimers();
    const f = mockFetch(Array.from({ length: 30 }, () => ({ status: 503, body: { error_kind: 'counting' } })));
    render(<TalkPanel route={route()} navigate={() => undefined} fixture={false} />);
    await act(async () => { await vi.advanceTimersByTimeAsync(4100 * 20); });
    expect(f).toHaveBeenCalledTimes(15);
    expect(screen.getByTestId('admin-error').textContent).toContain('集計がまだ終わっていません');
  });

  it('電話の一覧が 503 not_configured のときは「まだ動いていません」', async () => {
    mockFetch([{ status: 503, body: { error_kind: 'not_configured' } }]);
    render(<TalkPanel route={route({ owner: '9001' })} navigate={() => undefined} fixture={false} />);
    expect((await screen.findByTestId('talk-disabled')).textContent).toContain('まだ動いていません');
  });
});

describe('トークの振り返り: 並べ替えと色の決まり (具体値)', () => {
  const rules = { item_min_applicable: 5, item_excess_pct: 20, appointment_min_dials: 100, appointment_rate_ratio: 0.5 };
  const ri = (nd: number, ap: number) => ({ id: 'x', not_done: nd, applicable: ap, rate_pct: ap === 0 ? null : (nd / ap) * 100, flagged: false });
  const tot = ri(39, 100); // できた 61%

  it('できた割合は (該当 - できていない) ÷ 該当', () => {
    expect(donePct(ri(7, 9))).toBeCloseTo(22.22, 1);
    expect(donePct(ri(1, 5))).toBe(80);
    expect(donePct(ri(0, 0))).toBeNull();
  });

  it('1/1本は灰色、9本で22%は赤、5本で80%は色なし', () => {
    expect(judgeItem(ri(0, 1), tot, rules)).toMatchObject({ state: 'grey' });
    expect(judgeItem(ri(0, 1), tot, rules).reason).toBe('当てはまる電話が 1 本で、5 本未満なので色を付けていません。');
    const red = judgeItem(ri(7, 9), tot, rules);
    expect(red.state).toBe('red');
    expect(red.reason).toBe('できた 2/9本（22%）。この表の全体 61% より 39 ポイント低いので赤。');
    const none = judgeItem(ri(1, 5), tot, rules);
    expect(none.state).toBe('none');
    expect(none.reason).toBe('できた 4/5本（80%）。この表の全体との差が 20 ポイント未満。');
  });

  it('境界: 差がちょうど20は赤、19は色なし、該当ちょうど5本は判定する、4本は灰色', () => {
    const t = ri(0, 100); // 全体 100%
    expect(judgeItem(ri(1, 5), t, rules).state).toBe('red'); // 80% → 差 20
    expect(judgeItem(ri(19, 100), ri(0, 100), rules).state).toBe('none'); // 81% → 差 19
    expect(judgeItem(ri(20, 100), ri(0, 100), rules).state).toBe('red'); // 80% → 差 20
    expect(judgeItem(ri(1, 4), t, rules).state).toBe('grey');
    expect(judgeItem(ri(0, 0), t, rules).state).toBe('empty');
  });

  const person = (dials: number | null, rate: number | null) => ({ ...summary.total, dials, appointment_rate_pct: rate });
  it('アポ率: 架電数不足は灰色、全体の半分未満は赤、以上は色なし', () => {
    const total = person(1000, 2);
    expect(judgeAppointment(person(99, 0), total, rules).state).toBe('grey');
    expect(judgeAppointment(person(100, 0.99), total, rules).state).toBe('red');
    expect(judgeAppointment(person(100, 1), total, rules).state).toBe('none'); // ちょうど半分は赤にしない
    expect(judgeAppointment(person(null, null), total, rules).state).toBe('empty');
    expect(judgeAppointment(person(310, 0.65), total, rules).reason).toBe('アポ率 0.65% が、この表の全体 2.00% の半分（1.00%）未満なので赤。');
  });

  it('並べ替え: 架電数・アポ数・判定数・アポ率は降順/昇順で、値の無い人は常に最後', () => {
    const mk = (name: string, dials: number | null, appts: number, reviewed: number, r: number | null) => ({ ...summary.total, name, dials, appointments: appts, reviewed, appointment_rate_pct: r });
    const ps = [mk('A', 50, 2, 10, 4), mk('B', null, 0, 30, null), mk('C', 200, 1, 20, 0.5)];
    const names = (k: Parameters<typeof sortPeople>[1]) => sortPeople(ps, k).map(p => p.name).join('');
    const ob = ps.map((p, i) => ({ ...p, outbound: [70, null, 300][i] ?? null }));
    const obNames = (dir: 'asc' | 'desc') => sortPeople(ob, { key: { kind: 'outbound' }, dir }).map(p => p.name).join('');
    expect(obNames('desc')).toBe('CAB');
    expect(obNames('asc')).toBe('ACB');
    expect(names({ key: { kind: 'dials' }, dir: 'desc' })).toBe('CAB');
    expect(names({ key: { kind: 'dials' }, dir: 'asc' })).toBe('ACB');
    expect(names({ key: { kind: 'appointments' }, dir: 'desc' })).toBe('ACB');
    expect(names({ key: { kind: 'appointments' }, dir: 'asc' })).toBe('BCA');
    expect(names({ key: { kind: 'reviewed' }, dir: 'desc' })).toBe('BCA');
    expect(names({ key: { kind: 'reviewed' }, dir: 'asc' })).toBe('ACB');
    expect(names({ key: { kind: 'rate' }, dir: 'asc' })).toBe('CAB');
    expect(names({ key: { kind: 'rate' }, dir: 'desc' })).toBe('ACB');
  });

  it('項目の並べ替えはできた割合。昇順はできていない人が上、該当0本は最後', () => {
    const mk = (name: string, nd: number, ap: number) => ({ ...summary.total, name, items: [{ ...ri(nd, ap), id: 'k' }] });
    const ps = [mk('A', 1, 10), mk('B', 9, 10), mk('C', 0, 0)];
    const key = { kind: 'item', id: 'k' } as const;
    expect(sortPeople(ps, { key, dir: 'asc' }).map(p => p.name).join('')).toBe('BAC');
    expect(sortPeople(ps, { key, dir: 'desc' }).map(p => p.name).join('')).toBe('ABC');
  });
});
