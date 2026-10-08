// 旧画面 JS の関数 (legacy_sales_kpi.js に逐語コピー) と calc.ts の関数を、
// 同じ入力で並べて比べる。fixture (実データ由来) と、境界値の両方で見る。
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import * as mine from './calc';
import * as legacy from './legacy_sales_kpi.js';
import { WowLine } from './SalesKpiView';

const D = loadFixture();
/** 旧 JS の md/wd/ago はブラウザのローカル TZ に依存する。JST (UTC+9) のときだけ比べられる。 */
const IS_JST = new Date().getTimezoneOffset() === -540;

describe('fmt / pct (旧 JS と同一)', () => {
  it.each([null, undefined, 0, 7, 1234567, -3, 72.44444, 0.5])('fmt(%s)', (n) => {
    expect(mine.fmt(n)).toBe(legacy.fmt(n));
  });
  it.each([null, undefined, 0, 72.44444, 2.6666, 100, 87.05])('pct(%s)', (n) => {
    expect(mine.pct(n)).toBe(legacy.pct(n));
  });
  it('具体値', () => {
    expect(mine.fmt(1234567)).toBe('1,234,567');
    expect(mine.fmt(null)).toBe('—');
    expect(mine.pct(72.44444)).toBe('72.4%');
    expect(mine.pct(2.6666)).toBe('2.7%');
  });
});

describe('md / wd / ago (JST では旧 JS と同一)', () => {
  const dates = ['2026-08-31', '2026-09-06', '2026-09-07', '2026-01-01', '2025-12-31', '2026-02-28', '2026-03-01'];
  it.skipIf(!IS_JST).each(dates)('md/wd(%s)', (s) => {
    expect(mine.md(s)).toBe(legacy.md(s));
    expect(mine.wd(s)).toBe(legacy.wd(s));
  });
  it.skipIf(!IS_JST)('ago: fixture の止まっている取引 (TODAY = generated_at の日付)', () => {
    const today = mine.todayOf(D);
    expect(today).toBe('2026-09-05');
    const ago = legacy.makeAgo(today);
    for (const r of D.stale) expect(mine.ago(today, r.date)).toBe(ago(r.date));
    expect(mine.ago(today, '2026-07-10')).toBe(57);
  });
  it('具体値 (TZ に依存しない)', () => {
    expect(mine.md('2026-08-31')).toBe('8/31');
    expect(mine.wd('2026-08-31')).toBe('月');
    expect(mine.wd('2026-09-06')).toBe('日');
    expect(mine.ago('2026-09-05', '2026-09-05')).toBe(0);
    expect(mine.ago('2026-09-05', '2026-09-06')).toBe(-1);
  });
});

describe('sumIf (旧 JS と同一)', () => {
  const cases: { name: string; hidden: string[]; ok: (id: string) => boolean }[] = [
    { name: '全員', hidden: [], ok: () => true },
    { name: '第1チームだけ', hidden: [], ok: (id) => D.people.find((p) => p.id === id)?.team === '第1チーム' },
    { name: '1人外す', hidden: ['615075002'], ok: () => true },
    { name: '個人', hidden: [], ok: (id) => id === '615075002' },
  ];
  it.each(cases)('$name: by_person', ({ hidden, ok }) => {
    const h = new Set(hidden);
    expect(mine.sumIf(D.by_person, h, ok)).toEqual(legacy.makeSumIf(h)(D.by_person, ok));
  });
  it.each(cases)('$name: kaden.by_person / calls this_week', ({ hidden, ok }) => {
    const h = new Set(hidden);
    expect(mine.sumIf(D.kaden.by_person, h, ok)).toEqual(legacy.makeSumIf(h)(D.kaden.by_person, ok));
    const bp = D.calls.periods.this_week.by_person;
    expect(mine.sumIf(bp, h, ok)).toEqual(legacy.makeSumIf(h)(bp, ok));
  });
  it('undefined を渡すと空 (旧 JS の byPerson||{})', () => {
    expect(mine.sumIf(undefined, new Set(), () => true)).toEqual({});
  });
});

describe('avgLine (旧 JS の HTML と同一の文字列になる)', () => {
  const html = (a: mine.AvgLine | null): string =>
    a ? `${a.label} ${a.value}<span style="opacity:.75">（${String(a.n)}名）</span>` : '';
  it.each([
    [{ label: '1人あたり', team: null, n: 109 }, 245, undefined],
    [{ label: '第1チーム の平均', team: '第1チーム', n: 7 }, 31, undefined],
    [{ label: '1人あたり', team: null, n: 109 }, 29866, '件'],
    [{ label: '1人あたり', team: null, n: 0 }, 5, undefined],
    [null, 5, undefined],
  ] as const)('%o total=%s', (ab, total, unit) => {
    expect(html(mine.avgLine(ab, total, unit))).toBe(legacy.avgLine(ab, total, unit));
  });
  it('具体値', () => {
    expect(mine.avgLine({ label: '1人あたり', team: null, n: 109 }, 245)).toEqual({ label: '1人あたり', value: '2.2件', n: 109 });
  });
});

describe('wow (旧 wowEl の HTML と同一)', () => {
  it.each([
    [29866, 24884, '件', false],
    [10, 12, '件', false],
    [10, 12, '件', true],
    [5, 5, '件', false],
    [5, null, '件', false],
    [1234, 0, '', false],
  ] as const)('now=%s prev=%s unit=%s invert=%s', (now, prev, unit, invert) => {
    const w = mine.wow(now, prev, unit, invert);
    const html = w ? renderToStaticMarkup(<WowLine w={w} />) : '';
    expect(html).toBe(legacy.wowEl(now, prev, unit, invert));
  });
  it('具体値', () => {
    expect(mine.wow(29866, 24884, '件')).toEqual({ same: false, good: true, arrow: '▲', abs: '4,982', prev: '24,884', unit: '件' });
  });
});

describe('growText / growTone (旧 JS と同一)', () => {
  it.each([null, undefined, 0, 31, -3, 1234])('%s', (n) => {
    expect(mine.growText(n)).toBe(legacy.growText(n));
    expect(`var(--${mine.growTone(n)})`).toBe(legacy.growColor(n));
  });
  it('具体値', () => {
    expect(mine.growText(null)).toBe('—');
    expect(mine.growText(0)).toBe('±0');
    expect(mine.growText(31)).toBe('+31');
    expect(mine.growText(-3)).toBe('-3');
  });
});

describe('リストの在庫 (lsAdd / lsPct / lsNames / lsRows が旧 JS と同一)', () => {
  const lists = D.list_stock.lists;
  const L = legacy.makeLs(lists);
  it('lsNames', () => {
    for (const kind of ['アクティブ', '保管', 'その他']) {
      expect(mine.lsNames(lists, kind)).toEqual(L.lsNames(kind));
    }
    expect(mine.lsNames(lists, 'アクティブ')).toEqual(['FSメンバー', 'パートナー']);
    expect(mine.lsNames(lists, '保管')).toEqual(['保管担当01', '保管担当02', '保管担当03']);
  });
  it('lsRows: 全リストで行の並び・件数・名前ありが同じ', () => {
    for (const l of lists) {
      const a = mine.lsRows(l, lists);
      const b = L.lsRows(l);
      expect(a.length).toBe(b.length);
      a.forEach((row, i) => {
        const lb = b[i];
        if (!lb) throw new Error('legacy row missing');
        // 旧 JS は innerHTML 用に label を escS していた。React は既定でエスケープするので生のまま持つ。
        expect(L.escS(row.label)).toBe(lb.label);
        expect(row.note).toBe(lb.note);
        expect(row.sub).toBe(lb.sub);
        expect(row.key).toBe(lb.key);
        expect(row.n).toEqual(lb.n);
        expect(row.named).toEqual(lb.named);
      });
    }
    const first = lists[0];
    if (!first) throw new Error('fixture に list が無い');
    const r0 = mine.lsRows(first, lists);
    expect(r0.map((r) => r.label)).toEqual(['FSメンバー', 'パートナー', 'アクティブの計', '保管担当01', '保管担当02', '保管担当03', '保管の計', 'その他']);
    expect(r0[2]?.n['すべて']).toBe(48889);
    expect(r0[6]?.n['すべて']).toBe(84486);
    expect(r0[7]?.n['すべて']).toBe(30784);
    // アクティブの計 + 保管の計 + その他 = 全体
    expect(48889 + 84486 + 30784).toBe(164159);
  });
  it('lsAdd / lsPct', () => {
    expect(mine.lsAdd({ a: 1, b: 2 }, { b: 3, c: 4 })).toEqual(L.lsAdd({ a: 1, b: 2 }, { b: 3, c: 4 }));
    expect(mine.lsAdd({ a: 1 }, undefined)).toEqual(L.lsAdd({ a: 1 }, undefined));
    for (const [n, d] of [
      [48889, 164159],
      [0, 10],
      [5, 0],
      [92442, 164159],
    ] as const) {
      expect(mine.lsPct(n, d)).toBe(L.lsPct(n, d));
    }
    expect(mine.lsPct(48889, 164159)).toBe('29.8%');
    expect(mine.lsPct(5, 0)).toBe('—');
  });
});

describe('絞り込みの規則 inScope / sumScope / pick / avgBase / personName (旧 JS と同一)', () => {
  const roster = D.people;
  const first = (team: string) => roster.find((p) => p.team === team);
  const states: { name: string; team: string; person: string | null; hidden: string[] }[] = [
    { name: '全社・個人なし', team: 'すべて', person: null, hidden: [] },
    { name: '第1チーム', team: '第1チーム', person: null, hidden: [] },
    { name: '第1チーム・1人外す', team: '第1チーム', person: null, hidden: [first('第1チーム')?.id ?? ''] },
    { name: '個人 (名簿に居る)', team: 'すべて', person: first('第2チーム')?.id ?? '', hidden: [] },
    { name: '個人 + 別チームを選択 (個人が優先)', team: '第1チーム', person: first('第2チーム')?.id ?? '', hidden: [] },
    { name: '担当なし (全社)', team: 'すべて', person: '', hidden: [] },
    { name: '担当なし (チーム選択中。個人が優先)', team: '第1チーム', person: '', hidden: [] },
    { name: '個人を選んだがチェックで外した', team: 'すべて', person: first('第2チーム')?.id ?? '', hidden: [first('第2チーム')?.id ?? ''] },
    { name: '名簿に居ない id を個人に', team: 'すべて', person: 'ZZ-OFF', hidden: [] },
  ];
  // id: 名簿に居る人・担当なし ('')・名簿に居ない人。rowTeam: 名簿と同じ・違う・無し
  const ids = ['', 'ZZ-OFF', first('第1チーム')?.id ?? '', first('第2チーム')?.id ?? '', first('チーム未設定')?.id ?? ''];
  const rowTeams = [undefined, '第1チーム', '第2チーム', 'チーム未設定', '存在しないチーム'];
  it.each(states)('inScope: $name (全 id × 全 rowTeam)', (st) => {
    const lg = legacy.makeScoped(D, { team: st.team, person: st.person, hidden: new Set(st.hidden) });
    const scope: mine.Scope = { team: st.team, person: st.person, hidden: new Set(st.hidden) };
    const teamOf = mine.teamOfMap(roster);
    let n = 0;
    for (const id of ids) {
      for (const rt of rowTeams) {
        expect(mine.inScope(scope, teamOf, id, rt), `${id}/${String(rt)}`).toBe(lg.inScope(id, rt));
        n++;
      }
    }
    expect(n).toBe(25);
  });
  it.each(states)('sumScope / pick: $name', (st) => {
    const lg = legacy.makeScoped(D, { team: st.team, person: st.person, hidden: new Set(st.hidden) });
    const scope: mine.Scope = { team: st.team, person: st.person, hidden: new Set(st.hidden) };
    const teamOf = mine.teamOfMap(roster);
    expect(mine.sumScope(D.by_person, scope, teamOf)).toEqual(lg.sumScope(D.by_person));
    expect(mine.pickRows(D.stale, scope, teamOf).map((r) => r.id)).toEqual(lg.pick(D.stale).map((r) => r.id));
    expect(mine.pickRows(D.kettei.rows, scope, teamOf).map((r) => r.owner)).toEqual(lg.pick(D.kettei.rows).map((r) => r.owner));
  });
  it.each(states)('avgBase / personName: $name', (st) => {
    const lg = legacy.makeScoped(D, { team: st.team, person: st.person, hidden: new Set(st.hidden) });
    const scope: mine.Scope = { team: st.team, person: st.person, hidden: new Set(st.hidden) };
    expect(mine.avgBase(roster, scope)).toEqual(lg.avgBase());
    if (st.person !== null) expect(mine.personName(roster, st.person)).toBe(lg.personName());
  });
  it('番兵: 担当なし(null でも "" でもない値)の往復', () => {
    const lg = legacy.makeScoped(D, { team: 'すべて', person: null, hidden: new Set() });
    for (const v of ['', '__none__', '615075002']) {
      expect(mine.personOfValue(v)).toBe(lg.personOfValue(v));
    }
    for (const p of [null, '', '615075002']) {
      expect(mine.valueOfPerson(p)).toBe(lg.valueOfPerson(p));
    }
    expect(mine.personOfValue('')).toBeNull();
    expect(mine.personOfValue('__none__')).toBe('');
    expect(mine.valueOfPerson('')).toBe('__none__');
    expect(mine.valueOfPerson(null)).toBe('');
  });
  it('担当なしを選ぶと、担当なしの行だけになる (空文字を未選択と読まない)', () => {
    const scope: mine.Scope = { team: 'すべて', person: '', hidden: new Set() };
    const teamOf = mine.teamOfMap(roster);
    expect(mine.inScope(scope, teamOf, '')).toBe(true);
    expect(mine.inScope(scope, teamOf, '615075002')).toBe(false);
    expect(mine.inScope({ ...scope, person: null }, teamOf, '615075002')).toBe(true);
  });
});
