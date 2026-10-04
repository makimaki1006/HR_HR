// 主要 KPI を fixture (tests/fixtures/sales_kpi/*.tsv → Rust build_payload、判定日 2026-09-04) の
// 具体値で検証する。期待値は Python で fixture JSON から独立に集計したもの
// (Rust 側テストの 母集団 537 / 取ったアポ 245 / Cヨミ 123 とも一致する)。
import { describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import {
  ALL_TEAMS,
  CLOSED_CARD_PANEL,
  actionView,
  avgBase,
  callsView,
  cardPanelView,
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
  toggleCardState,
  visiblePeople,
  weekDays,
  type CardKey,
  type CardPanelState,
  type Scope,
} from './calc';
import type { SalesKpiData } from './types';
import { loadNegtypeFixture } from './__fixtures__/load';
import { withBlankOwner } from './__fixtures__/synthetic';

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
    expect(v).toEqual({ apo: 245, pool: 537, den: 225, done: 163, rate: 163 / 225 * 100, anqrate: 6 / 225 * 100, cyomi: 123 });
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
    expect(v.anqrate).toBeCloseTo(3.333, 2);
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

// ---------------------------------------------------------------- カード内訳 (#45)

describe('カード内訳: カードの値 == 内訳の合計 == 一覧の行数 (全チーム・全個人・チェック外し)', () => {
  const KEYS: CardKey[] = ['apo', 'pool', 'den', 'done', 'rate', 'anqrate', 'cyomi'];
  const blank = withBlankOwner().data;

  /** 開いた内訳を、表 → 担当者 → 一覧と降りて、一覧の行数を全部足す。 */
  function walkTotal(d: typeof D, scope: Scope, key: CardKey, seg: string | null, bpoOnly: boolean): number {
    const teamOf = teamOfMap(d.people);
    const st: CardPanelState = { ...CLOSED_CARD_PANEL, openCard: key, cardSeg: seg, bpoOnly };
    const v0 = cardPanelView(d, scope, teamOf, st);
    if (!v0?.drill) return 0;
    const drill = v0.drill;
    if (drill.level === 'list') return drill.rows.length;
    let total = 0;
    for (const g of drill.groups) {
      if (drill.level === 'team') {
        const v1 = cardPanelView(d, scope, teamOf, { ...st, cardTeam: g.pick });
        const d1 = v1?.drill;
        if (d1?.level !== 'person') throw new Error('チームの次は担当者の表のはず');
        expect(d1.sum).toBe(g.n);
        for (const p of d1.groups) {
          const v2 = cardPanelView(d, scope, teamOf, { ...st, cardTeam: g.pick, cardPerson: p.pick });
          const d2 = v2?.drill;
          if (d2?.level !== 'list') throw new Error('担当者の次は一覧のはず');
          expect(d2.rows.length).toBe(p.n);
          total += d2.rows.length;
        }
      } else {
        const v2 = cardPanelView(d, scope, teamOf, { ...st, cardPerson: g.pick });
        const d2 = v2?.drill;
        if (d2?.level !== 'list') throw new Error('担当者の次は一覧のはず');
        expect(d2.rows.length).toBe(g.n);
        total += d2.rows.length;
      }
    }
    return total;
  }

  const izuboIds = D.people.filter((p) => p.team === '伊壺チーム').map((p) => p.id);
  const scopes: { name: string; d: typeof D; scope: Scope }[] = [
    { name: '全社', d: D, scope: ALL },
    { name: '伊壺チーム', d: D, scope: IZUBO },
    { name: '個人', d: D, scope: P1 },
    { name: '伊壺チーム・3 人外す', d: D, scope: { ...IZUBO, hidden: new Set(izuboIds.slice(0, 3)) } },
    { name: '全社 (担当なし入り)', d: blank, scope: ALL },
    { name: '担当なしを選択', d: blank, scope: { team: ALL_TEAMS, person: '', hidden: new Set() } },
  ];

  it.each(scopes)('$name: 7 枚すべて', ({ d, scope }) => {
    const teamOf = teamOfMap(d.people);
    const mv = monthView(d, scope, teamOf);
    const vals = Object.fromEntries(mv.cards.map((c) => [c.key, c.val]));
    for (const key of KEYS) {
      const v = cardPanelView(d, scope, teamOf, { ...CLOSED_CARD_PANEL, openCard: key });
      if (!v) throw new Error('開けない: ' + key);
      if (key === 'rate' || key === 'anqrate') {
        // 率のカード: 分母 = ④ 日が過ぎた分、分子はカードの hint (n ÷ m 件) と一致
        expect(v.total).toBe(mv.den);
        const a = mv.a;
        expect(v.num).toBe(key === 'rate' ? (a['実施'] ?? 0) : (a.anq_num ?? 0));
        if (key === 'anqrate') expect(v.total).toBe(a.anq_den ?? 0);
      } else {
        expect(v.total, key).toBe(vals[key]);
      }
      // 表の合計 == 件数、降りた一覧の行数の合計 == 件数
      expect(walkTotal(d, scope, key, null, false), `${key} 一覧の合計`).toBe(v.total);
      // 内 BPO チップ == カードの内 BPO、BPO だけの一覧の合計 == 内 BPO
      if (v.bpoN) expect(walkTotal(d, scope, key, null, true), `${key} BPO だけ`).toBe(v.bpoN);
      // 区分の件数の和 == 分母
      if (v.segChips) {
        const parts = v.segChips.filter((c) => c.k).map((c) => c.n);
        expect(parts.reduce((x, y) => x + y, 0)).toBe(v.total);
        for (const c of v.segChips.filter((x) => x.k)) {
          expect(walkTotal(d, scope, key, c.k, false), `${key} 区分 ${c.k}`).toBe(c.n);
        }
      }
    }
  });
  it('見出し: 件数カードは「題（N件）」、率カードは 分子・分母つき', () => {
    const tf = teamOfMap(D.people);
    const t = (key: CardKey) => cardPanelView(D, ALL, tf, { ...CLOSED_CARD_PANEL, openCard: key });
    expect(t('pool')?.title).toBe('③ 商談の予定（537件）');
    expect(t('apo')?.title).toBe('① 取ったアポ（245件）');
    expect(t('rate')?.title).toBe('⑥ 商談化率　72.4%（分子 実施 163件 ÷ 分母 ④ 日が過ぎた分 225件）');
    expect(t('anqrate')?.title).toBe('⑤ アンケート回収率　2.7%（分子 回収済み 6件 ÷ 分母 ④ 日が過ぎた分 225件）');
    expect(t('rate')?.num).toBe(163);
    expect(t('pool')?.num).toBeNull();
  });
  it('区分のチップ: 「分母 … すべて」と「分子 実施」', () => {
    const tf = teamOfMap(D.people);
    const v = cardPanelView(D, ALL, tf, { ...CLOSED_CARD_PANEL, openCard: 'rate' });
    expect(v?.segChips?.map((c) => c.label)).toEqual([
      '分母 ④ 日が過ぎた分 すべて 225件',
      '分子 実施 163件',
      '未実施 ' + String(v?.segChips?.[2]?.n) + '件',
      '未処理 ' + String(v?.segChips?.[3]?.n) + '件',
      '要判定 ' + String(v?.segChips?.[4]?.n) + '件',
    ]);
  });
  it('カードを押すたびに掘り下げ・区分・BPO を捨てる (toggleCardState)', () => {
    const st: CardPanelState = { openCard: 'pool', cardTeam: '伊壺チーム', cardPerson: 'x', cardSeg: '実施', cardNt: null, bpoOnly: true };
    expect(toggleCardState(st, 'pool')).toEqual(CLOSED_CARD_PANEL);
    expect(toggleCardState(st, 'den')).toEqual({ ...CLOSED_CARD_PANEL, openCard: 'den' });
  });
});

// ---------------------------------------------------------------- 商談種別 (#49・#50)

describe('商談種別の表 (種別の列がある fixture)', () => {
  const N = loadNegtypeFixture();
  const open = (key: CardKey, extra: Partial<CardPanelState> = {}, scope: Scope = ALL, d: SalesKpiData = N) =>
    cardPanelView(d, scope, teamOf(d), { ...CLOSED_CARD_PANEL, openCard: key, ...extra });
  const teamOf = (d: SalesKpiData) => teamOfMap(d.people);

  it('並びは payload の negotiation_type_order に従う。決まっている 2 種別は 0 件でも出る', () => {
    const t = open('pool')?.ntTable;
    expect(t?.rows.map((r) => r.name)).toEqual(N.negotiation_type_order);
    // JS 側で並べ直していない証拠: order を入れ替えると、その通りに並ぶ
    const swapped = { ...N, negotiation_type_order: [...N.negotiation_type_order].reverse() };
    expect(open('pool', {}, ALL, swapped)?.ntTable?.rows.map((r) => r.name)).toEqual([...N.negotiation_type_order].reverse());
    // 0 件の決まっている種別: 伊壺チームの個人 1 人でも 決裁者商談 / 非決裁者商談 の行は出る
    const id = N.people.find((p) => p.team === '伊壺チーム')?.id ?? '';
    const one = open('pool', {}, { team: ALL_TEAMS, person: id, hidden: new Set() })?.ntTable;
    expect(one?.level).toBe('person');
    expect(one?.rows.slice(0, 2).map((r) => r.name)).toEqual(['決裁者商談', '非決裁者商談']);
    expect(one?.rows.reduce((a, r) => a + r.n, 0)).toBe(one?.sum);
    const none = { ...N, negotiation_type_fixed: ['決裁者商談', '非決裁者商談', '作ってみた種別'] };
    expect(open('pool', {}, ALL, none)?.ntTable?.rows.map((r) => r.name)).toContain('作ってみた種別');
    expect(open('pool', {}, ALL, none)?.ntTable?.rows.find((r) => r.name === '作ってみた種別')?.n).toBe(0);
  });

  it('全社の合計 == カードの件数、種別ごとの件数は fixture の具体値 (決裁者商談 239 / 非決裁者商談 119 / 未設定 59 / 定義外 60 + 60)', () => {
    const v = open('pool');
    expect(v?.ntTable?.level).toBe('all');
    expect(v?.ntTable?.sum).toBe(v?.total);
    expect(Object.fromEntries((v?.ntTable?.rows ?? []).map((r) => [r.name, r.n]))).toEqual({
      '決裁者商談': 239,
      '非決裁者商談': 119,
      '(未設定)': 59,
      '新種別(定義外)': 60,
      '決裁者商談;非決裁者商談(定義外)': 60,
    });
    expect(v?.ntTable?.numHead).toBeNull();
    expect(v?.ntTable?.countHead).toBe('件数');
  });

  it('⑥⑤: 分母・分子の 2 列。区分を選ぶと件数（区分）、分子でない区分では分子の列は 0 と注釈', () => {
    const v = open('rate');
    expect(v?.ntTable?.countHead).toBe('分母');
    expect(v?.ntTable?.numHead).toBe('分子（実施）');
    expect(v?.ntTable?.sum).toBe(225);
    expect(v?.ntTable?.sumN).toBe(163);
    expect(v?.numNote).toBeNull();
    const un = open('rate', { cardSeg: '未実施' });
    expect(un?.ntTable?.countHead).toBe('件数（未実施）');
    expect(un?.ntTable?.sum).toBe(53);
    expect(un?.ntTable?.sumN).toBe(0);
    expect(un?.numNote).toBe('選んでいる区分（未実施）は分子に当たらないため、分子の列は計算できません（0 と表示しています）');
    const done = open('rate', { cardSeg: '実施' });
    expect(done?.ntTable?.sum).toBe(163);
    expect(done?.ntTable?.sumN).toBe(163);
    expect(done?.numNote).toBeNull();
    // ⑤: 回収済みを選ぶと分子 == 件数
    const q = open('anqrate', { cardSeg: '回収済み' });
    expect(q?.ntTable?.sum).toBe(q?.ntTable?.sumN);
    expect(open('anqrate', { cardSeg: '未回収' })?.numNote).toContain('未回収');
  });

  it('種別で絞る: 一覧の行数 == その種別の件数、下の段 (チーム → 担当者) にも効き、合計は種別の件数に戻る', () => {
    const base = open('pool');
    for (const r of base?.ntTable?.rows ?? []) {
      const v = open('pool', { cardNt: r.name });
      expect(v?.ntChip).toBe(r.name);
      // 絞ったあとも、種別の表は「種別だけ掛けない行」から数える (合計は変わらない)
      expect(v?.ntTable?.sum).toBe(base?.ntTable?.sum);
      if (r.n === 0) {
        expect(v?.noRows).toBe(true);
        continue;
      }
      const d = v?.drill;
      if (d?.level !== 'team') throw new Error('全社ならチーム別');
      expect(d.sum).toBe(r.n);
      let rows = 0;
      for (const g of d.groups) {
        const p = open('pool', { cardNt: r.name, cardTeam: g.pick })?.drill;
        if (p?.level !== 'person') throw new Error('チームの次は担当者');
        expect(p.sum).toBe(g.n);
        for (const q of p.groups) {
          const l = open('pool', { cardNt: r.name, cardTeam: g.pick, cardPerson: q.pick })?.drill;
          if (l?.level !== 'list') throw new Error('担当者の次は一覧');
          expect(l.rows.length).toBe(q.n);
          expect(l.rows.every((x) => x.negotiation_type === r.name)).toBe(true);
          expect(l.head).toContain('（' + r.name + '）');
          rows += l.rows.length;
        }
      }
      expect(rows, r.name).toBe(r.n);
    }
  });

  it('段ごとの種別の表: チーム・担当者を選ぶと、その段の行で数え直す。合計 == その段の件数', () => {
    const izubo = open('pool', { }, IZUBO);
    expect(izubo?.ntTable?.level).toBe('team');
    expect(izubo?.ntTable?.sum).toBe(66);
    const id = N.people.find((p) => p.team === '伊壺チーム')?.id ?? '';
    const p = open('pool', { cardPerson: id }, ALL);
    expect(p?.ntTable?.level).toBe('person');
    const l = p?.drill;
    if (l?.level !== 'list') throw new Error('一覧のはず');
    expect(p?.ntTable?.sum).toBe(l.n);
    // 全社でチームの表から 1 チーム選んだ段
    const t = open('pool', { cardTeam: '平田チーム' });
    expect(t?.ntTable?.level).toBe('team');
    expect(t?.ntTable?.sum).toBe(t?.drill?.level === 'person' ? t.drill.sum : -1);
  });

  it('列が無いシートでは表も絞りも出さない (未取得)。cardNt が残っていても効かない', () => {
    const v = cardPanelView(D, ALL, teamOfMap(D.people), { ...CLOSED_CARD_PANEL, openCard: 'pool', cardNt: '決裁者商談' });
    expect(v?.ntOk).toBe(false);
    expect(v?.ntTable).toBeNull();
    expect(v?.ntChip).toBeNull();
    expect(v?.total).toBe(537);
    expect(v?.drill?.level).toBe('team');
  });

  it('担当なし (合成) でも、種別の表の合計 == 一覧の件数', () => {
    const b = withBlankOwner(N).data;
    const none: Scope = { team: ALL_TEAMS, person: '', hidden: new Set() };
    const v = open('pool', {}, none, b);
    const l = v?.drill;
    if (l?.level !== 'list') throw new Error('担当なしを選ぶとすぐ一覧');
    expect(l.n).toBeGreaterThan(0);
    expect(v?.ntTable?.sum).toBe(l.n);
    expect(l.rows.every((r) => r.owner === '')).toBe(true);
  });
});
