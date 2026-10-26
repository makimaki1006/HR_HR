// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { adminHref, parseAdminRoute } from './route';
import { TalkPanel, type TalkRoute } from './TalkPanel';
import { diffClass, joinPeople, metricDiff, parsePeople, previousPeriod, sumRows, togglePerson, validCompare } from './talkCompare';
import { FIXTURE_RULES, fixtureSummary } from './talkModel';

afterEach(cleanup);

const route = (patch: Partial<TalkRoute> = {}): TalkRoute => ({
  view: 'talk', from: '2026-10-19', to: '2026-10-19', team: '', owner: '', call: '',
  people: '', only: '', cmp_from: '', cmp_to: '', vs_team: '', tab: '', ...patch,
});
const cells = (tr: HTMLElement) => Array.from(tr.querySelectorAll('td'));
const rowOf = (name: string) => within(screen.getByTestId('talk-table')).getByText(name).closest('tr') as HTMLElement;

const now = fixtureSummary('2026-10-19', '2026-10-19', '');
const prior = fixtureSummary('2026-10-18', '2026-10-18', '', true);
const must = <T,>(v: T | null | undefined): T => { if (v === undefined || v === null) throw new Error('missing'); return v; };
const person = (s: typeof now, id: string) => must(s.people.find(p => p.owner_id === id));

describe('URL', () => {
  it('people・only・cmp_from・cmp_to・vs_team を読み書きできる', () => {
    const q = '?view=talk&from=2026-10-12&to=2026-10-18&people=9001,9002&only=1&cmp_from=2026-10-05&cmp_to=2026-10-11&vs_team=1';
    const r = parseAdminRoute(q);
    expect(r).toEqual({ view: 'talk', from: '2026-10-12', to: '2026-10-18', team: '', owner: '', call: '', people: '9001,9002', only: '1', cmp_from: '2026-10-05', cmp_to: '2026-10-11', vs_team: '1', tab: '' });
    expect(adminHref(r)).toBe('?view=talk&from=2026-10-12&to=2026-10-18&people=9001%2C9002&only=1&cmp_from=2026-10-05&cmp_to=2026-10-11&vs_team=1');
  });
  it('people の値の扱い', () => {
    expect(parsePeople('9001, 9002,,9001')).toEqual(['9001', '9002']);
    expect(togglePerson(['9001'], '9002')).toEqual(['9001', '9002']);
    expect(togglePerson(['9001', '9002'], '9001')).toEqual(['9002']);
    expect(joinPeople(['1', '2'])).toBe('1,2');
  });
});

describe('比べる期間', () => {
  it('直前の同じ長さの期間', () => {
    expect(previousPeriod('2026-10-12', '2026-10-18')).toEqual({ from: '2026-10-05', to: '2026-10-11' });
    expect(previousPeriod('2026-10-19', '2026-10-19')).toEqual({ from: '2026-10-18', to: '2026-10-18' });
    expect(previousPeriod('2026-11-01', '2026-11-03')).toEqual({ from: '2026-10-29', to: '2026-10-31' });
  });
  it('順序の逆・空は使えない', () => {
    expect(validCompare('2026-10-05', '2026-10-11')).toBe(true);
    expect(validCompare('2026-10-11', '2026-10-05')).toBe(false);
    expect(validCompare('', '2026-10-05')).toBe(false);
  });
});

describe('選んだ人の合計', () => {
  it('件数を足して、率は件数の合計から出す', () => {
    const s = sumRows([person(now, '9001'), person(now, '9002')], now.checks, '選んだ人');
    expect(s.dials).toBe(530); // 220 + 310
    expect(s.outbound).toBe(1219); // 506 + 713
    expect(s.appointments).toBe(7);
    expect(s.appointment_rate_pct).toBeCloseTo((7 / 530) * 100, 9);
    expect(s.contacts).toBe(340);
    expect(s.contact_rate_pct).toBeCloseTo((340 / 1219) * 100, 9);
    // 日程を早く出す: できていない 20 + 36 = 56 / 該当 40 + 45 = 85
    const early = must(s.items.find(i => i.id === 'early_date'));
    expect([early.not_done, early.applicable]).toEqual([56, 85]);
  });
});

describe('差', () => {
  it('期間どうし: 本数は増減、率はポイント', () => {
    const cur = person(now, '9001');
    const prev = person(prior, '9001');
    expect(metricDiff(cur, prev, 'dials', '', FIXTURE_RULES)?.text).toBe('+22'); // 220 - 198
    expect(metricDiff(cur, prev, 'appointments', '', FIXTURE_RULES)?.text).toBe('+1'); // 5 - 4
    expect(metricDiff(cur, prev, 'appointmentRate', '', FIXTURE_RULES)?.text).toBe('+0.3pt'); // 2.2727 - 2.0202
    // 日程を早く出す: できた率 50% と、前は 22/40 できていない = 45%
    const d = must(metricDiff(cur, prev, 'item', 'early_date', FIXTURE_RULES));
    expect(d.text).toBe('+5.0pt');
    expect(d.dir).toBe('up');
    expect(diffClass(d)).toBe('talk-diff talk-diff-good');
  });
  it('悪くなったら bad、同じなら色なし', () => {
    const cur = person(prior, '9001');
    const prev = person(now, '9001');
    const d = must(metricDiff(cur, prev, 'item', 'early_date', FIXTURE_RULES));
    expect(d.text).toBe('−5.0pt');
    expect(diffClass(d)).toBe('talk-diff talk-diff-bad');
    const same = must(metricDiff(cur, cur, 'item', 'early_date', FIXTURE_RULES));
    expect(same.text).toBe('±0pt');
    expect(diffClass(same)).toBe('talk-diff');
  });
  it('チームとの差: そのチームの人の件数を合わせた全体との差', () => {
    const bpo = must(now.team_totals.find(t => t.name === '架空BPOチーム'));
    const early = must(bpo.items.find(i => i.id === 'early_date'));
    expect([early.not_done, early.applicable]).toEqual([56, 85]);
    // 花子の「できた」50% - チーム (85-56)/85 = 34.1% = +15.9pt
    expect(metricDiff(person(now, '9001'), bpo, 'item', 'early_date', FIXTURE_RULES)?.text).toBe('+15.9pt');
    expect(bpo.dials).toBe(530);
  });
  it('件数が少ない割合の差は薄く出す (色を付けない)', () => {
    // 四郎の該当は 10 本で十分。当てはまる電話を 3 本に減らした行と比べる
    const few = { ...person(now, '9004'), items: person(now, '9004').items.map(i => ({ ...i, applicable: 3, not_done: 1 })) };
    const d = must(metricDiff(few, person(now, '9001'), 'item', 'early_date', FIXTURE_RULES));
    expect(d.faint).toBe(true);
    expect(diffClass(d)).toBe('talk-diff talk-diff-faint');
    // アポ率: 架電数が下限 (100 本) 未満
    expect(metricDiff(person(now, '9004'), person(now, '9001'), 'appointmentRate', '', FIXTURE_RULES)?.faint).toBe(true);
  });
});

describe('表の上での動き', () => {
  const panel = (r: TalkRoute, navigate = vi.fn()) => render(<TalkPanel route={r} navigate={navigate} fixture />);

  it('チェックを入れると people に残る', () => {
    const navigate = vi.fn();
    panel(route({ people: '9001' }), navigate);
    fireEvent.click(screen.getByLabelText('サンプル 二郎を選ぶ'));
    expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ people: '9001,9002' }));
    fireEvent.click(screen.getByLabelText('架空 花子を選ぶ'));
    expect(navigate).toHaveBeenLastCalledWith(expect.objectContaining({ people: '' }));
  });

  it('選んだ人の合計の行が出て、「選んだ人だけ」で他の人が消える', () => {
    panel(route({ people: '9001,9002' }));
    const sel = screen.getByTestId('talk-selected-row');
    expect(cells(sel)[0]?.textContent).toBe('選んだ人の合計（2人）');
    expect(cells(sel)[3]?.textContent).toBe('530');
    expect(screen.getAllByTestId('talk-row')).toHaveLength(4);
    cleanup();
    panel(route({ people: '9001,9002', only: '1' }));
    expect(screen.getAllByTestId('talk-row')).toHaveLength(2);
    expect(screen.queryByText('ダミー 三郎')).toBeNull();
  });

  it('選んでいなければ合計の行は出ない', () => {
    panel(route());
    expect(screen.queryByTestId('talk-selected-row')).toBeNull();
  });

  it('比べる期間: 各セルに前との差が出る。前の期間に行が無い人は注記', () => {
    panel(route({ cmp_from: '2026-10-18', cmp_to: '2026-10-18' }));
    expect(cells(rowOf('架空 花子'))[3]?.textContent).toBe('220前 +22');
    expect(within(rowOf('架空 四郎')).getByTestId('talk-no-prev')).toBeTruthy();
    expect(screen.getByTestId('talk-cmp-note').textContent).toContain('2026-10-18 と比べています');
    // トークのタブでも、項目ごとに前との差が出る
    cleanup();
    panel(route({ cmp_from: '2026-10-18', cmp_to: '2026-10-18', tab: 'talk' }));
    expect(cells(rowOf('架空 花子'))[5]?.textContent).toBe('50%できた 20/40本前 +5.0pt');
  });

  it('チームとの差: 率の横にチーム比と、チームの全体の行が出る', () => {
    panel(route({ vs_team: '1', tab: 'talk' }));
    expect(cells(rowOf('架空 花子'))[5]?.textContent).toBe('50%できた 20/40本チーム比 +15.9pt');
    expect(screen.getAllByTestId('talk-team-row')).toHaveLength(2);
    expect(screen.getByText('架空BPOチームの全体')).toBeTruthy();
  });

  it('何も選ばない・比べない・チーム比を出さないときは従来の表のまま', () => {
    panel(route());
    expect(document.querySelectorAll('[data-diff]')).toHaveLength(0);
    expect(screen.queryByTestId('talk-team-row')).toBeNull();
  });
});
