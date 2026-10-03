// 営業KPI (React): 値の端 (0 件・分母 0・名簿外・月またぎ・日付の境界) を洗う逆証明。
// 「描けた」ではなく「NaN / undefined / Infinity が画面に出ない」「0 件でも注記が出る」を具体値で見る。
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { ALL_TEAMS, DEFAULT_CALL_PERIOD, rangeText, type Scope } from './calc';
import { SalesKpiView, type UiActions, type UiState } from './SalesKpiView';
import type { CallPeriodKey, SalesKpiData } from './types';

const noop = new Proxy({} as UiActions, { get: () => vi.fn() });

const base: UiState = {
  scope: { team: ALL_TEAMS, person: '', hidden: new Set() },
  openKey: null,
  dayKey: null,
  weekOpen: false,
  callPeriod: DEFAULT_CALL_PERIOD,
  snapMode: 'week',
  tab: 'kpi',
  pickOpen: false,
};

function render(data: SalesKpiData, ui: Partial<UiState> = {}, scope: Partial<Scope> = {}): string {
  return renderToStaticMarkup(
    <SalesKpiView data={data} ui={{ ...base, ...ui, scope: { ...base.scope, ...ui.scope, ...scope } }} actions={noop} />,
  );
}

/** 画面に出てはいけない文字列。タグの中 (属性) ではなく本文だけを見る。 */
function badTokens(html: string): string[] {
  const text = html.replace(/<[^>]+>/g, '\n');
  return text.split('\n').filter((t) => /NaN|undefined|Infinity|\[object/.test(t));
}

/** fixture から集計と一覧を空にしたもの (シートがまだ空の朝の状態に近い)。 */
function wiped(): SalesKpiData {
  const d = loadFixture();
  d.by_team = {};
  d.by_person = {};
  d.people = [];
  d.teams = [];
  d.bpo_total = {};
  d.stale = [];
  d.week_deals = [];
  d.next_week_deals = [];
  d.anq_missing = [];
  d.cyomi_stale = [];
  d.card_deals = { pool: [], apo: [], cyomi: [] };
  d.kaden.by_person = {};
  d.kaden.by_team = {};
  d.kaden.cls = {};
  d.kaden.no_owner = {};
  d.kaden.fill = { 氏名: 0 };
  d.kaden.total = 0;
  d.kaden.base = 0;
  d.kaden.has_by_owner = true;
  d.kettei.rows = [];
  d.list_stock.lists = [];
  d.snapshots = [];
  for (const p of Object.values(d.calls.periods)) {
    p.by_person = {};
    p.days = [];
    p.total = { ...p.total, calls: 0, connected: 0, long: 0 };
  }
  d.calls.daily = [];
  d.calls.people = [];
  return d;
}

describe('0 件・分母 0 でも NaN / undefined を出さない', () => {
  const d = wiped();
  it('すべて空のデータで、全タブ・全期間・全 open を描いても本文に NaN / undefined / Infinity が無い', () => {
    const keys = ['stale', 'anq', 'cyomi', 'week', 'next'] as const;
    const periods = Object.keys(d.calls.periods) as CallPeriodKey[];
    const bad: string[] = [];
    for (const tab of ['kpi', 'kettei', 'stock'] as const) {
      for (const callPeriod of periods) {
        for (const snapMode of ['week', 'month'] as const) {
          bad.push(...badTokens(render(d, { tab, callPeriod, snapMode })));
        }
      }
    }
    for (const openKey of keys) bad.push(...badTokens(render(d, { openKey })));
    expect(bad).toEqual([]);
  }, 60_000);
  it('分母 0 の率は 0% ではなく「—」', () => {
    const html = render(d);
    expect(html).toMatch(/data-card="rate"><div class="lab">⑥ 商談化率<\/div><div class="v">—/);
    expect(html).toMatch(/data-card="anq"><div class="lab">⑤ アンケート回収率<\/div><div class="v">—/);
  });
  it('架電リストの充足が 0 件 (kaden.total = 0) でも NaN% を出さない', () => {
    const html = render(d);
    expect(badTokens(html)).toEqual([]);
  });
});

describe('担当者・チームの全組み合わせ (fixture) で NaN / undefined を出さない', () => {
  const D = loadFixture();
  it('全チーム × 全個人 × 全期間', () => {
    const bad: string[] = [];
    const periods = Object.keys(D.calls.periods) as CallPeriodKey[];
    for (const team of [ALL_TEAMS, ...D.teams]) {
      bad.push(...badTokens(render(D, {}, { team })));
      for (const callPeriod of periods) bad.push(...badTokens(render(D, { callPeriod }, { team })));
    }
    for (const p of D.people) {
      for (const tab of ['kpi', 'kettei'] as const) bad.push(...badTokens(render(D, { tab }, { person: p.id })));
    }
    expect(bad).toEqual([]);
  }, 60_000);
  it('名簿に無い id を個人に指定しても「undefined」を出さない', () => {
    const html = render(D, {}, { person: 'no-such-owner' });
    expect(badTokens(html)).toEqual([]);
    expect(html).toContain('この担当者には今月の商談がありません');
  });
  it('全員のチェックを外しても (hidden = 全員) 例外にならず NaN も出ない', () => {
    const html = render(D, {}, { hidden: new Set(D.people.map((p) => p.id)) });
    expect(badTokens(html)).toEqual([]);
  });
});

describe('月またぎ・日付の境界', () => {
  const crossMonth = (): SalesKpiData => {
    const d = loadFixture();
    d.generated_at = '2026-10-01 07:00';
    d.week = { start: '2026-09-28', end: '2026-10-04' };
    d.next_week = { start: '2026-10-05', end: '2026-10-11' };
    return d;
  };
  it('見出しの「今月」は generated_at の月 (2026-10-01 なら 2026年10月)。9 月の固定文字を出さない', () => {
    expect(rangeText(crossMonth())).toContain('2026年10月　／　今週 9/28（月）〜10/4（日）');
    expect(rangeText(crossMonth())).not.toContain('9月　／');
  });
  it('fixture の日付 (2026-09-05) では従来どおり 2026年9月', () => {
    expect(rangeText(loadFixture())).toContain('2026年9月　／　今週 8/31（月）〜9/6（日）　※2026-09-05 19:08 時点');
  });
  it('年またぎ (2026-12-31 → 2027-01-01) でも月と曜日が正しい', () => {
    const d = loadFixture();
    d.generated_at = '2027-01-01 07:00';
    d.week = { start: '2026-12-28', end: '2027-01-03' };
    expect(rangeText(d)).toContain('2027年1月　／　今週 12/28（月）〜1/3（日）');
  });
  it('止まっている取引の「N日前」が月またぎでも正しい (10/1 の 2026-09-30 は 1 日前)', () => {
    const d = crossMonth();
    const r0 = d.stale[0];
    if (!r0) throw new Error('fixture に stale が無い');
    r0.date = '2026-09-30';
    expect(render(d, { openKey: 'stale' })).toContain('<span>1日前</span>');
  });
  it('generated_at が空でも今月の見出しで例外にならない (年月を出さず「今週」だけ)', () => {
    const d = loadFixture();
    d.generated_at = '';
    expect(() => rangeText(d)).not.toThrow();
    expect(rangeText(d)).not.toMatch(/NaN|undefined/);
  });
});
