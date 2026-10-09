// ⑨ 持っているCヨミ: 商談予定日時で 今月 / 過去（流れ案件）/ 未来 / 未設定 に分ける。
import { describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { ALL_TEAMS, CLOSED_CARD_PANEL, cardPanelView, cyomiBucket, monthView, teamOfMap } from './calc';
import type { SalesKpiData } from './types';

const D = loadFixture();
const ALL = { team: ALL_TEAMS, person: null, hidden: new Set<string>() };

describe('cyomiBucket: 境界', () => {
  it('10 月が判定月: 10/01 は今月、9/30 は過去、10/31 は今月、11/01 は未来、空は未設定', () => {
    const t = '2026-10-05';
    expect(cyomiBucket('2026-10-01', t)).toBe('今月商談');
    expect(cyomiBucket('2026-09-30', t)).toBe('過去（流れ案件）');
    expect(cyomiBucket('2026-10-31', t)).toBe('今月商談');
    expect(cyomiBucket('2026-11-01', t)).toBe('未来（来月以降）');
    expect(cyomiBucket('', t)).toBe('予定日未設定');
  });
  it('年またぎ: 1 月が判定月のとき前年 12 月は過去', () => {
    expect(cyomiBucket('2025-12-31', '2026-01-02')).toBe('過去（流れ案件）');
    expect(cyomiBucket('2026-01-01', '2026-01-02')).toBe('今月商談');
  });
});

describe('Cヨミ内訳: 合成データ(判定日 2026-10-05)', () => {
  const tpl = D.card_deals.cyomi[0];
  if (!tpl) throw new Error('fixture に Cヨミが無い');
  const dates = ['2026-10-01', '2026-10-20', '2026-09-30', '2026-03-10', '2026-11-01', '', ''];
  const rows = dates.map((date, i) => ({ ...tpl, id: 'x' + String(i), date, bpo: false }));
  const d: SalesKpiData = {
    ...D,
    generated_at: '2026-10-05 09:00',
    card_deals: { ...D.card_deals, cyomi: rows },
  };
  const teamOf = teamOfMap(d.people);
  const view = (seg: string | null) =>
    cardPanelView(d, ALL, teamOf, { ...CLOSED_CARD_PANEL, openCard: 'cyomi', cardSeg: seg });

  it('チップの件数: 今月 2 / 過去 2 / 未来 1 / 未設定 2、合計 7', () => {
    const chips = view(null)?.segChips ?? [];
    expect(chips.map((c) => [c.k, c.n])).toEqual([
      ['', 7],
      ['今月商談', 2],
      ['過去（流れ案件）', 2],
      ['未来（来月以降）', 1],
      ['予定日未設定', 2],
    ]);
    expect(chips.map((c) => c.label)).toEqual([
      'すべて 7件',
      '今月商談 2件',
      '過去（流れ案件） 2件',
      '未来（来月以降） 1件',
      '予定日未設定 2件',
    ]);
  });

  it('区分を選ぶと一覧がその区分の行だけになる(担当者を選んだ状態)', () => {
    const scope = { team: ALL_TEAMS, person: tpl.owner, hidden: new Set<string>() };
    const dr = cardPanelView(d, scope, teamOf, {
      ...CLOSED_CARD_PANEL,
      openCard: 'cyomi',
      cardSeg: '過去（流れ案件）',
    })?.drill;
    if (dr?.level !== 'list') throw new Error('担当者を選んだら一覧のはず');
    expect(dr.rows.map((r) => r.date).sort()).toEqual(['2026-03-10', '2026-09-30']);
  });
});

describe('Cヨミ内訳: 実データ形のフィクスチャで逆証明', () => {
  it('4 区分の合計 == ⑨ のカード値 == 全体の件数', () => {
    const teamOf = teamOfMap(D.people);
    const mv = monthView(D, ALL, teamOf);
    const card = mv.cards.find((c) => c.key === 'cyomi')?.val;
    const v = cardPanelView(D, ALL, teamOf, { ...CLOSED_CARD_PANEL, openCard: 'cyomi' });
    const segs = (v?.segChips ?? []).filter((c) => c.k !== '');
    expect(segs.map((c) => c.k)).toEqual(['今月商談', '過去（流れ案件）', '未来（来月以降）', '予定日未設定']);
    expect(segs.reduce((a, c) => a + c.n, 0)).toBe(card);
    expect(v?.total).toBe(card);
  });
});
