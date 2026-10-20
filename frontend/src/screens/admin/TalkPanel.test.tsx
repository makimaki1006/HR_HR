// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { parseAdminRoute, adminHref } from './route';
import { TalkCallsTable, TalkDetailView, TalkPanel, TalkSummaryTable, type TalkRoute } from './TalkPanel';
import { activePeriod, describeDays, fixtureCalls, fixtureDetail, fixtureSummary, periodRange, ruleSentences } from './talkModel';

afterEach(cleanup);

const summary = fixtureSummary('2026-10-19', '2026-10-19', '');
const rowOf = (name: string) => within(screen.getByTestId('talk-table')).getByText(name).closest('tr') as HTMLElement;
const route = (patch: Partial<TalkRoute> = {}): TalkRoute => ({ view: 'talk', from: '2026-10-19', to: '2026-10-19', team: '', owner: '', call: '', ...patch });

describe('トークの振り返り: 人ごとの表', () => {
  it('アポ率は小数2桁、印のセルだけ目立たせる', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const jiro = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'));
    // 名前・チーム・総架電・アポ・アポ率・判定数・4項目
    expect(jiro[2]?.textContent).toBe('310');
    expect(jiro[4]?.textContent).toBe('0.65%');
    expect(jiro[4]?.getAttribute('data-flagged')).toBe('true');
    expect(jiro[5]?.textContent).toContain('70');
    expect(jiro[5]?.textContent).toContain('待ち 6 ・ 取れない 4');
    // 日程を早く出す: 36/45 = 80% が印、受付は 25% で印なし
    expect(jiro[8]?.textContent).toBe('80%36/45本');
    expect(jiro[8]?.getAttribute('data-flagged')).toBe('true');
    expect(jiro[6]?.getAttribute('data-flagged')).toBeNull();
    const hanako = Array.from(rowOf('架空 花子').querySelectorAll('td'));
    expect(hanako[4]?.textContent).toBe('2.27%');
    expect(hanako[4]?.getAttribute('data-flagged')).toBeNull();
    // 項目の印は別の人 (断られた後 14/16)
    expect(Array.from(rowOf('ダミー 三郎').querySelectorAll('td'))[14]?.getAttribute('data-flagged')).toBe('true');
  });

  it('見出しに資料の全体の割合と決まりを出し、最後の行はチーム全体', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const head = screen.getByRole('columnheader', { name: /早く日付を出す/ });
    expect(head.textContent).toContain('全体 81%');
    expect(head.getAttribute('title')).toContain('具体的な週・曜日・日');
    expect(screen.getByRole('columnheader', { name: /既存感/ }).textContent).toContain('全体 -');
    const rows = within(screen.getByTestId('talk-table')).getAllByRole('row');
    const last = Array.from(rows.at(-1)?.querySelectorAll('td') ?? []).map(td => td.textContent);
    expect(last.slice(0, 5)).toEqual(['チーム全体', '-', '770', '12', '1.56%']);
    expect(last[8]).toBe('57%71/125本');
    // 場面ごとのまとまり
    expect(Array.from(document.querySelectorAll('th.talk-group')).map(t => `${t.textContent}:${t.getAttribute('colspan') ?? ''}`)).toEqual(['受付:1', '日付の前:5', '断られた後:3']);
  });

  it('印の決まりを平易な文で出す (pt は使わない)', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    const text = screen.getByTestId('talk-rules').textContent;
    expect(text).toContain('5 本以上');
    expect(text).toContain('20% 以上高い');
    expect(text).toContain('100 本以上');
    expect(text).toContain('半分未満');
    expect(text).not.toContain('pt');
    expect(ruleSentences(summary.rules)[0]).toContain('60% 以上');
  });

  it('判定した本数と対象の本数を並べ、割合は判定した電話の中の割合だと書く', () => {
    render(<TalkSummaryTable data={summary} onOpen={() => undefined} />);
    // 二郎: 判定 70、待ち 6、取れない 4、枠を超えて判定していない 35 → 対象 115
    const cell = Array.from(rowOf('サンプル 二郎').querySelectorAll('td'))[5]?.textContent ?? '';
    expect(cell).toContain('70');
    expect(cell).toContain('対象 115 本');
    const note = screen.getByTestId('talk-sampling').textContent;
    expect(note).toContain('判定した電話の中での割合');
    expect(note).toContain('1 人 1 日あたり新しい電話から 10 本まで');
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
