// 主要 KPI を fixture (tests/fixtures/sales_kpi/*.tsv → Rust build_payload、判定日 2026-09-04) の
// 具体値で検証する。期待値は Python で fixture JSON から独立に集計したもの
// (Rust 側テストの 母集団 537 / 取ったアポ 245 / Cヨミ 123 とも一致する)。
import { describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import {
  ALL_TEAMS,
  actionView,
  avgBase,
  callsView,
  excludedParts,
  kadenListView,
  ketteiView,
  monthView,
  rangeText,
  scopeText,
  snapView,
  stockOverview,
  tabsOf,
  teamOfMap,
  visiblePeople,
  weekDays,
  type Scope,
} from './calc';

const D = loadFixture();
const TEAM_OF = teamOfMap(D.people);
const ALL: Scope = { team: ALL_TEAMS, person: null, hidden: new Set() };
const IZUBO: Scope = { team: '伊壺チーム', person: null, hidden: new Set() };
const P1: Scope = { team: ALL_TEAMS, person: '613211320', hidden: new Set() };

describe('fixture の前提', () => {
  it('人数・チーム・週', () => {
    expect(D.people).toHaveLength(109);
    expect(D.teams).toEqual(['チーム未設定', '伊壺チーム', '平田チーム', '櫻井チーム', '野中チーム', '野口チーム']);
    expect(D.week).toEqual({ start: '2026-08-31', end: '2026-09-06' });
    expect(rangeText(D)).toBe('2026年9月　／　今週 8/31（月）〜9/6（日）　※2026-09-05 19:08 時点');
  });
});

describe('今月の成績 (cards1)', () => {
  it('すべて: ①245 ③537 ④225 ②163 ⑥72.4% ⑤2.7% ⑨123', () => {
    const mv = monthView(D, ALL, TEAM_OF);
    const v = Object.fromEntries(mv.cards.map((c) => [c.key, c.val]));
    expect(v).toEqual({ apo: 245, pool: 537, den: 225, done: 163, rate: 163 / 225 * 100, anq: 6 / 225 * 100, cyomi: 123 });
    expect(mv.cards.map((c) => c.lab)).toEqual([
      '① 取ったアポ', '③ 商談の予定', '④ 日が過ぎた分', '② やった商談', '⑥ 商談化率', '⑤ アンケート回収率', '⑨ 持っているCヨミ',
    ]);
    expect(mv.cards[4]?.hint).toBe('163 ÷ 225 件');
    expect(mv.cards[5]?.hint).toBe('6 ÷ 225 件（④ 日が過ぎた分と同じ母数）');
  });
  it('すべて: 内 BPO と 1人あたり (109 名)', () => {
    const mv = monthView(D, ALL, TEAM_OF);
    expect(mv.cards[0]?.bpo).toEqual({ v: 51, ratio: '21' });
    expect(mv.cards[1]?.bpo).toEqual({ v: 129, ratio: '24' });
    expect(mv.cards[3]?.bpo).toEqual({ v: 28, ratio: '17' });
    expect(mv.cards[6]?.bpo).toEqual({ v: 14, ratio: '11' });
    expect(mv.cards[0]?.avg).toEqual({ label: '1人あたり', value: '2.2件', n: 109 });
    expect(mv.cards[1]?.avg).toEqual({ label: '1人あたり', value: '4.9件', n: 109 });
    expect(mv.cards[2]?.avg).toEqual({ label: '1人あたり', value: '2.1件', n: 109 });
    expect(mv.cards[3]?.avg).toEqual({ label: '1人あたり', value: '1.5件', n: 109 });
    expect(mv.cards[4]?.avg).toBeUndefined();
    expect(mv.cards[5]?.avg).toBeUndefined();
    expect(mv.cards[6]?.avg).toEqual({ label: '1人あたり', value: '1.1件', n: 109 });
  });
  it('伊壺チーム: ①31 ③66 ④30 ②19 ⑥63.3% ⑨11、1人あたりは 7 名で割る', () => {
    const mv = monthView(D, IZUBO, TEAM_OF);
    const v = Object.fromEntries(mv.cards.map((c) => [c.key, c.val]));
    expect(v.apo).toBe(31);
    expect(v.pool).toBe(66);
    expect(v.den).toBe(30);
    expect(v.done).toBe(19);
    expect(v.rate).toBeCloseTo(63.333, 2);
    expect(v.anq).toBeCloseTo(3.333, 2);
    expect(v.cyomi).toBe(11);
    expect(mv.cards[0]?.avg).toEqual({ label: '1人あたり', value: '4.4件', n: 7 });
    expect(mv.cards[6]?.bpo).toBeNull();
  });
  it('個人 (担当001): 自分の数字と、伊壺チームの平均', () => {
    const mv = monthView(D, P1, TEAM_OF);
    const v = Object.fromEntries(mv.cards.map((c) => [c.key, c.val]));
    expect(v.apo).toBe(10);
    expect(v.pool).toBe(24);
    expect(v.den).toBe(10);
    expect(v.done).toBe(6);
    expect(v.rate).toBe(60);
    expect(v.cyomi).toBe(4);
    expect(mv.ab).toEqual({ label: '伊壺チーム の平均', team: '伊壺チーム', n: 7 });
    expect(mv.cards[0]?.avg).toEqual({ label: '伊壺チーム の平均', value: '4.4件', n: 7 });
  });
  it('チェックで 1 人外すと、その人のぶんが減り分母も減る', () => {
    const scope: Scope = { ...ALL, hidden: new Set(['613211320']) };
    const mv = monthView(D, scope, TEAM_OF);
    expect(mv.cards[0]?.val).toBe(245 - 10);
    expect(mv.cards[1]?.val).toBe(537 - 24);
    expect(mv.cards[0]?.avg?.n).toBe(108);
    expect(avgBase(D.people, scope)?.n).toBe(108);
  });
  it('scope 文と集計除外', () => {
    expect(scopeText(D, ALL, monthView(D, ALL, TEAM_OF).a)).toBe('全チームの合計を表示しています。チーム名か、右のプルダウンで絞り込めます。');
    expect(scopeText(D, IZUBO, monthView(D, IZUBO, TEAM_OF).a)).toBe('伊壺チーム の数字だけを表示しています。');
    expect(scopeText(D, P1, monthView(D, P1, TEAM_OF).a)).toBe('担当001 の数字だけを表示しています。');
    expect(scopeText(D, { ...ALL, hidden: new Set(['613211320', '96032023']) }, {})).toBe(
      '全チームの合計を表示しています。チーム名か、右のプルダウンで絞り込めます。　2名をチェックで外しています。',
    );
    expect(excludedParts(D.excluded)).toEqual({ n: 7, parts: ['コンサル営業 7件'] });
  });
  it('個人プルダウンに出る人', () => {
    expect(visiblePeople(D.people, ALL)).toHaveLength(109);
    expect(visiblePeople(D.people, IZUBO)).toHaveLength(7);
    expect(visiblePeople(D.people, { ...IZUBO, hidden: new Set(['613211320']) })).toHaveLength(6);
  });
});

describe('いま手を打てること (cards2)', () => {
  it('すべて: ⑦9 ⑤267 ⑨36 今週260 (うち206 過去) 来週213', () => {
    const av = actionView(D, ALL);
    expect(av.cards.map((c) => [c.key, c.val, c.tone ?? null, c.hint])).toEqual([
      ['stale', 9, 'alert', '商談日が過ぎたのに動いていない'],
      ['anq', 267, 'warn', '今週・来週これからの商談のうち'],
      ['cyomi', 36, 'warn', '30日以上ステージが動いていない'],
      ['week', 260, null, 'うち 206件 は日が過ぎました'],
      ['next', 213, null, '9/7〜9/13 の予定'],
    ]);
  });
  it('伊壺チーム: ⑦1 ⑤29 ⑨3 今週33 来週23', () => {
    const av = actionView(D, IZUBO);
    expect(av.cards.map((c) => c.val)).toEqual([1, 29, 3, 33, 23]);
  });
  it('個人 (担当001): ⑦0 は ok 色で「ありません」', () => {
    const av = actionView(D, P1);
    expect(av.cards[0]?.val).toBe(0);
    expect(av.cards[0]?.tone).toBe('ok');
    expect(av.cards[0]?.hint).toBe('ありません');
    expect(av.cards[3]?.val).toBe(11);
  });
  it('今週の日別: 0 件の日も出す。過去/今日の判定', () => {
    const av = actionView(D, ALL);
    const days = weekDays(av.wk, D.week, '2026-09-05');
    expect(days.map((d) => [d.date, d.rows.length, d.past, d.today])).toEqual([
      ['2026-08-31', 30, true, false],
      ['2026-09-01', 42, true, false],
      ['2026-09-02', 71, true, false],
      ['2026-09-03', 63, true, false],
      // 9/4 は Rust の判定日 (9/4) には過ぎていないので past=false のまま
      ['2026-09-04', 52, false, false],
      ['2026-09-05', 0, false, true],
      ['2026-09-06', 2, false, false],
    ]);
  });
});

describe('架電 (Zoom)', () => {
  it('今週・すべて: 架電数 29,866、発信 34,295、率 87.1%、5分超 791、1日 5,973、前週比 +4,982', () => {
    const cv = callsView(D, ALL, TEAM_OF, avgBase(D.people, ALL), 'this_week');
    if (!cv) throw new Error('calls');
    expect(cv.dl).toEqual(['2026-08-31', '2026-09-01', '2026-09-02', '2026-09-03', '2026-09-04']);
    const v = Object.fromEntries(cv.cards.map((c) => [c.key, c.val]));
    expect(v.conn).toBe(29866);
    expect(v.calls).toBe(34295);
    expect(v.ratio).toBeCloseTo(87.085, 2);
    expect(v.long).toBe(791);
    expect(v.perday).toBe(5973);
    expect(cv.cards[0]?.hint).toBe('8/31〜9/4');
    expect(cv.cards[0]?.avg).toEqual({ label: '1人あたり', value: '274.0件', n: 109 });
    expect(cv.cards[0]?.wow).toEqual({ same: false, good: true, arrow: '▲', abs: '4,982', prev: '24,884', unit: '件' });
    expect(cv.cards[4]?.hint).toBe('5日で割った平均');
    expect(cv.cards[4]?.avg).toBeUndefined();
    // 最終日 9/4 が途中 (last_day_partial=true) なので注記が付く。fetched_at が空なので「集計中」
    expect(cv.fresh).toBe('partial');
    expect(cv.asof).toBe('');
    expect(cv.cards[0]?.sub2).toBe('Zoomでつながった通話の数（9/4 は集計中）');
    expect(cv.totalCalls).toBe(54098);
    expect(cv.unmatchedTop).toEqual(['(不明) 1,358件', 'Capital 20件', 'FAC Capital 8,947件']);
  });
  it('人別の表: つながった数の降順、上位 40 名、前週比', () => {
    const cv = callsView(D, ALL, TEAM_OF, null, 'this_week');
    if (!cv) throw new Error('calls');
    expect(cv.rows).toHaveLength(40);
    expect(cv.rows[0]).toEqual({ id: '96032022', name: '担当407', team: '櫻井チーム', calls: 1126, conn: 949, lng: 30, prev: 698 });
    expect(cv.rows[1]?.conn).toBe(887);
    expect(cv.rows[2]?.conn).toBe(875);
    expect(cv.hasPrev).toBe(true);
  });
  it('先週: 前週比なし、7 日', () => {
    const cv = callsView(D, ALL, TEAM_OF, null, 'prev_week');
    if (!cv) throw new Error('calls');
    expect(cv.hasPrev).toBe(false);
    expect(cv.conn).toBe(24885);
    expect(cv.dl).toHaveLength(7);
    expect(cv.cards[0]?.wow).toBeNull();
    expect(cv.fresh).toBeNull();
  });
  it('今日: 5,909 件 (担当者に紐づいた分)', () => {
    const cv = callsView(D, ALL, TEAM_OF, null, 'today');
    if (!cv) throw new Error('calls');
    expect(cv.conn).toBe(5909);
    expect(cv.cards[0]?.hint).toBe('9/4');
  });
  it('日別グラフ: calls>100 の 9 日、最大 12,291', () => {
    const cv = callsView(D, ALL, TEAM_OF, null, 'this_week');
    if (!cv) throw new Error('calls');
    expect(cv.bars).toHaveLength(9);
    expect(cv.mx).toBe(12291);
    expect(cv.bars[0]).toEqual({ date: '2026-08-25', calls: 12291, connected: 10528, long: 246 });
  });
  it('periods が無ければ null (旧: 架電データがありません)', () => {
    const d2 = loadFixture();
    (d2 as { calls: unknown }).calls = undefined;
    expect(callsView(d2, ALL, TEAM_OF, null, 'this_week')).toBeNull();
  });
});

describe('架電リストの残り', () => {
  it('すべて = 営業 5 チームの合計: 未架電 11,942 / 未接触 15,327 / 接触済み 16,344 / 母数 43,613 / 72.6%', () => {
    const kv = kadenListView(D, ALL, TEAM_OF);
    expect(kv.scope).toEqual({ c: kv.salesCls, base: 43613, who: '営業5チームの合計', whole: true });
    const v = Object.fromEntries(kv.cards.map((c) => [c.key, c.val]));
    expect(v.mikaden).toBe(11942);
    expect(v.misesshoku).toBe(15327);
    expect(v.sesshoku).toBe(16344);
    expect(v.touched).toBeCloseTo(72.62, 1);
    expect(kv.cards[0]?.hint).toBe('27.4% を占めます');
    expect(kv.cards[3]?.hint).toBe('31,671 ÷ 43,613 件');
    expect(kv.cards[3]?.sub2).toBe('母数 43,613件');
    expect(kv.fillParts).toEqual(['決定者の役職 0.2%', '決定者名 0.2%', '決裁者の役職 0.1%', '決裁者名 0.1%']);
  });
  it('まだ配られていないリスト: 86,168 件、担当なし 7,337、上位 8 名', () => {
    const kv = kadenListView(D, ALL, TEAM_OF);
    expect(kv.showUnassigned).toBe(true);
    expect(kv.unCls.base).toBe(86168);
    expect(kv.unCls['未架電']).toBe(84592);
    expect(kv.noOwn).toBe(7337);
    expect(kv.unassignedRows).toHaveLength(8);
    expect(kv.unassignedRows[0]?.base).toBe(70176);
  });
  it('伊壺チーム: 母数 12,039、手をつけた 50.6%、未配布は出さない', () => {
    const kv = kadenListView(D, IZUBO, TEAM_OF);
    expect(kv.scope?.who).toBe('伊壺チーム が持っている分');
    expect(kv.scope?.base).toBe(12039);
    expect(kv.cards[3]?.val).toBeCloseTo(50.57, 1);
    expect(kv.showUnassigned).toBe(false);
  });
  it('担当者別シートが無いときは会社全体 (リスト全体) を出す', () => {
    const d2 = loadFixture();
    d2.kaden.has_by_owner = false;
    d2.kaden.by_person = {};
    const kv = kadenListView(d2, ALL, TEAM_OF);
    expect(kv.scope?.who).toBe('会社全体');
    expect(kv.scope?.base).toBe(d2.kaden.base);
    expect(kadenListView(d2, IZUBO, TEAM_OF).scope).toBeNull();
  });
});

describe('先週との比べ方 (週次)', () => {
  it('その週の商談: 2 行、W36 は集計中', () => {
    const sv = snapView(D.snapshots, 'week');
    expect(sv.rows).toHaveLength(2);
    expect(sv.missing).toBe(0);
    expect(sv.few).toBe(false);
    expect(sv.rows[0]).toEqual({
      week: '2026-W35', weekStart: '2026-08-24',
      cells: { kind: 'data', pool: '319', poolPartial: false, done: '224', rate: '70.2%' },
      zoom: { text: '40,293', partialNote: null }, apo: '245', stale: '5', base: '129,869', baseDiff: null,
    });
    expect(sv.rows[1]).toEqual({
      week: '2026-W36', weekStart: '2026-08-31',
      cells: { kind: 'data', pool: '260', poolPartial: true, done: '188', rate: '74.9%' },
      // 母数の差は 0 (旧 JS と同じく null ではなく 0。画面では 0 のとき差を出さない)
      zoom: { text: '46,137', partialNote: '5日目まで（集計中）' }, apo: '245', stale: '9', base: '129,869', baseDiff: 0,
    });
  });
  it('当月の累積: W35 1,063/755/72.9% (755÷1,036)、W36 537/163/72.4%', () => {
    const sv = snapView(D.snapshots, 'month');
    expect(sv.rows.map((r) => r.cells)).toEqual([
      { kind: 'data', pool: '1,063', poolPartial: false, done: '755', rate: '72.9%' },
      { kind: 'data', pool: '537', poolPartial: false, done: '163', rate: '72.4%' },
    ]);
  });
  it('週ベースの列が無い古い行は「記録する前の週」、母数の差も出す', () => {
    const s0 = D.snapshots[0];
    if (!s0) throw new Error('snapshot');
    const old = { ...s0, week: '2026-W34', week_start: '2026-08-17', week_totals: null, kaden_base: 120000, zoom_called: null };
    const sv = snapView([old, s0], 'week');
    expect(sv.rows[0]?.cells).toEqual({ kind: 'missing' });
    expect(sv.missing).toBe(1);
    expect(sv.rows[0]?.zoom.text).toBeNull();
    expect(sv.rows[1]?.baseDiff).toBe(129869 - 120000);
    expect(snapView([old, s0], 'month').missing).toBe(0);
  });
  it('末尾 8 週だけ', () => {
    const s0 = D.snapshots[0];
    if (!s0) throw new Error('snapshot');
    const many = Array.from({ length: 10 }, (_, i) => ({ ...s0, week: `2026-W${String(30 + i)}` }));
    expect(snapView(many, 'week').rows.map((r) => r.week)[0]).toBe('2026-W32');
    expect(snapView(many, 'week').rows).toHaveLength(8);
  });
});

describe('決定者・決裁者', () => {
  it('すべて: 6 名 + 担当なし、合計 793、増加 +125 (1 行は前の記録なし)', () => {
    const kv = ketteiView(D, ALL);
    expect(kv.rows.map((r) => r.ownerName)).toEqual(['担当001', '担当008', '担当017', '担当024', '担当029', '担当238']);
    expect(kv.no?.['合計']).toBe(29);
    expect(kv.sum).toEqual({ 決定者名: 202, 決定者役職: 202, 決裁者名: 177, 決裁者役職: 212, 合計: 793 });
    expect(kv.grew).toBe(125);
    expect(kv.grewKnown).toBe(6);
    expect(kv.shown).toBe(7);
    expect(kv.miss).toBe(1);
    expect(kv.who).toBe('全社');
    expect(kv.asof).toBe('9/7（月）');
  });
  it('伊壺チーム: 1 名、担当なし行は出さない', () => {
    const kv = ketteiView(D, IZUBO);
    expect(kv.rows).toHaveLength(1);
    expect(kv.no).toBeNull();
    expect(kv.sum['合計']).toBe(289);
    expect(kv.grew).toBe(31);
    expect(kv.who).toBe('伊壺チーム');
  });
  it('タブ: 決定者とリストの在庫があるので 3 つ', () => {
    expect(tabsOf(D).map((t) => t.key)).toEqual(['kpi', 'kettei', 'stock']);
    const d2 = loadFixture();
    d2.kettei.rows = [];
    d2.kettei.no_owner = null;
    d2.list_stock.lists = [];
    expect(tabsOf(d2).map((t) => t.key)).toEqual(['kpi']);
  });
});

describe('リストの在庫', () => {
  it('概要表: リクロジ 164,159 / 大分 96,105 / 計 260,264、名前あり', () => {
    const so = stockOverview(D.list_stock);
    expect(so.cols).toEqual(['リクロジ', '大分', '計']);
    expect(so.whole).toEqual([164159, 96105, 260264]);
    expect(so.wholeNamed).toEqual([92442, 34883, 127325]);
    expect(so.named).toBe(true);
    expect(so.hasGroups).toBe(true);
    expect(so.rows.map((r) => r.row.label)).toEqual(['FSメンバー', 'パートナー', 'アクティブの計', '保管担当01', '保管担当02', '保管担当03', '保管の計', 'その他']);
    expect(so.rows[2]?.ns).toEqual([48889, 44958, 93847]);
    expect(so.rows[6]?.ns).toEqual([84486, 44599, 129085]);
    expect(so.rows[7]?.ns).toEqual([30784, 6548, 37332]);
    // 区分の計 + 保管の計 + その他 = 全体 (リストごと)
    expect(48889 + 84486 + 30784).toBe(164159);
    expect(44958 + 44599 + 6548).toBe(96105);
    expect(so.trendParts).toBeNull();
    expect(so.bands).toEqual(['未入力', '〜49人', '50〜99人', '100〜299人', '300〜600人', '601人〜']);
  });
  it('前の週の記録があれば増減を出す', () => {
    const d2 = loadFixture();
    d2.list_stock.trend = {
      week: '2026-W35', week_start: '2026-08-24',
      lists: { リクロジ: { 全体: 164000, アクティブ: 48000, 保管: 85000 } },
    };
    expect(stockOverview(d2.list_stock).trendParts).toEqual(['リクロジ 全体 +159 ／ アクティブ +889 ／ 保管 -514']);
  });
});

describe('担当なしの選択 (person = "")', () => {
  const NONE: Scope = { team: ALL_TEAMS, person: '', hidden: new Set() };
  it('平均は付けない・表示名は「担当なし」・scope 文', () => {
    expect(avgBase(D.people, NONE)).toBeNull();
    const mv = monthView(D, NONE, TEAM_OF);
    expect(mv.cards.every((c) => !c.avg)).toBe(true);
    // fixture には担当なしの取引が無い: 件数は 0、「商談がありません」の注記
    expect(mv.cards.map((c) => c.val)).toEqual([0, 0, 0, 0, null, null, 0]);
    expect(scopeText(D, NONE, mv.a)).toBe(
      '担当なし の数字だけを表示しています。この担当者には今月の商談がありません（架電リストの数字だけ出ます）。',
    );
  });
  it('架電: 数字は出さない (0 と書くと「かけていない」に読める)', () => {
    const cv = callsView(D, NONE, TEAM_OF, null, 'this_week');
    expect(cv?.noCall).toBe(true);
    const v = Object.fromEntries((cv?.cards ?? []).map((c) => [c.key, c.val]));
    expect(v).toEqual({ conn: null, calls: null, ratio: null, long: null, perday: null });
    expect(cv?.cards.find((c) => c.key === 'ratio')?.hint).toBe('—');
    expect(cv?.cards.find((c) => c.key === 'conn')?.wow).toBeNull();
    // 人別の表: 個人指定なので、担当なしは id '' の行だけ (架電の人別には居ない → 0 行)
    expect(cv?.rows).toEqual([]);
    // 未選択 (null) なら全員
    expect(callsView(D, { ...NONE, person: null }, TEAM_OF, null, 'this_week')?.noCall).toBe(false);
  });
  it('架電リストの残り: no_owner を出す', () => {
    const kv = kadenListView(D, NONE, TEAM_OF);
    expect(kv.scope?.who).toBe('担当なし（担当者が入っていない取引）が持っている分');
    expect(kv.scope?.whole).toBe(false);
    expect(kv.scope?.c).toBe(D.kaden.no_owner);
    expect(kv.scope?.base).toBe(D.kaden.no_owner.base);
    expect(kv.scope?.base).toBeGreaterThan(0);
    expect(kv.showUnassigned).toBe(false);
  });
  it('決定者・決裁者: 担当なしの行だけが出る (チームを選んでいても)', () => {
    const kv = ketteiView(D, NONE);
    expect(kv.rows).toEqual([]);
    expect(kv.no).toBe(D.kettei.no_owner);
    expect(kv.who).toBe('担当なし');
    expect(ketteiView(D, { team: '伊壺チーム', person: '', hidden: new Set() }).no).toBe(D.kettei.no_owner);
    expect(ketteiView(D, { team: '伊壺チーム', person: null, hidden: new Set() }).no).toBeNull();
  });
});
