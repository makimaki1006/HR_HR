// @vitest-environment happy-dom
// 今月の成績カードの内訳 (#45) を、旧画面の script をそのまま動かした DOM と React 版で突き合わせる。
// 同じ操作 (カードを押す → 区分・BPO・チーム → 担当者 → 取引一覧と全部降りる) を両方に流し、
// そのたびの #panel1 の中身 (文字と id・data 属性) が全部同じになることを見る。
import { afterEach, describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { mountNew, mountOld, outline, type Screen } from './__fixtures__/dual';
import { withBlankOwner, withOffRosterRows } from './__fixtures__/synthetic';
import type { SalesKpiData } from './types';

type Log = [string, string[]][];

const panel = (s: Screen): string[] => outline(s.el('panel1'));
const cardsOf = (s: Screen): string[] => outline(s.el('cards1'));

/** パネルの表を 1 行ずつ降りて、各段の #panel1 を記録する。戻る → 次の行。 */
function walk(s: Screen, log: Log, tag: string, lim: number, depth = 0): void {
  log.push([tag, panel(s)]);
  if (depth > 3) throw new Error('表の段が深すぎる');
  // 旧画面は 1 回押すたびに画面全体を描き直すので、全員を降りると時間がかかる。段ごとに先頭 lim 行と最後の 1 行だけ降りる
  const total = s.qa('#panel1 table.cdrill tbody tr').length;
  const idx = [...new Set([...Array.from({ length: Math.min(lim, total) }, (_, i) => i), total - 1])].filter((i) => i >= 0);
  const n = idx.length;
  for (let k = 0; k < n; k++) {
    const tr = s.qa('#panel1 table.cdrill tbody tr')[idx[k] ?? 0];
    const name = tr?.getAttribute('data-name') ?? '';
    const btn = tr?.querySelector('button');
    if (!btn) throw new Error('行のボタンが無い');
    s.click(btn);
    walk(s, log, `${tag} > ${name}`, lim, depth + 1);
    s.clickButton('#panel1', /に戻る/);
  }
}

const segBtn = (s: Screen, k: string): HTMLElement => {
  const b = s.qa('#panel1 [data-seg]').find((x) => x.getAttribute('data-seg') === k);
  if (!b) throw new Error('区分チップが無い: ' + k);
  return b;
};
const nth = (s: Screen, sel: string, i: number): HTMLElement => {
  const b = s.qa(sel)[i];
  if (!b) throw new Error(sel + ' の ' + String(i) + ' 番目が無い');
  return b;
};

/** 区分チップ・BPO を 1 つずつ入れて walk する。 */
function walkFilters(s: Screen, log: Log, tag: string): void {
  walk(s, log, tag, 6);
  const segs = s.qa('#panel1 [data-seg]').map((b) => b.getAttribute('data-seg') ?? '');
  for (const k of segs.filter((x) => x)) {
    s.click(segBtn(s, k));
    walk(s, log, `${tag} [区分 ${k}]`, 1);
    s.click(segBtn(s, k)); // 外す
  }
  const bpo = s.q('#panel1-bpo');
  if (bpo) {
    s.click(bpo);
    walk(s, log, `${tag} [BPO だけ]`, 1);
    s.click(s.el('panel1-bpo'));
  }
}

/** 7 枚のカードを順に開き、閉じる。 */
function allCards(s: Screen, log: Log, tag: string): void {
  for (let i = 0; i < 7; i++) {
    s.clickCard(i);
    log.push([`${tag} card${String(i)} 開いた直後 cards1`, cardsOf(s)]);
    walkFilters(s, log, `${tag} card${String(i)}`);
    s.clickCard(i); // 閉じる
    log.push([`${tag} card${String(i)} 閉じた直後`, [...panel(s), ...cardsOf(s)]]);
  }
}

async function run(mount: typeof mountOld, data: SalesKpiData, setup: (s: Screen, log: Log) => void, hidden: string[] = []): Promise<Log> {
  const s = await mount(data, hidden);
  const log: Log = [];
  try {
    setup(s, log);
  } finally {
    s.teardown();
  }
  return log;
}

async function same(data: SalesKpiData, setup: (s: Screen, log: Log) => void, hidden: string[] = []): Promise<Log> {
  const o = await run(mountOld, data, setup, hidden);
  const n = await run(mountNew, data, setup, hidden);
  expect(n.length).toBe(o.length);
  for (let i = 0; i < o.length; i++) expect(n[i], o[i]?.[0]).toEqual(o[i]);
  return o;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('カード内訳: 旧画面と React 版が同じ (fixture)', () => {
  const D = loadFixture();
  it('全社: 7 枚すべて、区分・BPO・チーム → 担当者 → 一覧を全部降りる', async () => {
    const log = await same(D, (s, l) => { allCards(s, l, '全社'); });
    // 旧画面で実際に降りている (全社の ③ の 1 段目はチームの表、最後は取引の一覧)
    expect(log.some(([t]) => t.includes('card1') && t.split('>').length === 3)).toBe(true);
    expect(log.length).toBeGreaterThan(100);
  }, 120_000);
  it('伊壺チームを選んでいる: 担当者の表から始まる', async () => {
    await same(D, (s, l) => {
      s.clickTeam('伊壺チーム');
      allCards(s, l, '伊壺');
    });
  }, 60_000);
  it('個人を選んでいる: いきなり取引一覧', async () => {
    const id = D.people.find((p) => p.team === '伊壺チーム')?.id ?? '';
    await same(D, (s, l) => {
      s.selectPerson(id);
      allCards(s, l, '個人');
    });
  }, 60_000);
  it('チェックで外した人が居る', async () => {
    const hidden = D.people.filter((p) => p.team === '伊壺チーム').slice(0, 3).map((p) => p.id);
    await same(D, (s, l) => { allCards(s, l, '外す'); }, hidden);
  }, 120_000);
  it('開いたまま絞り込みを変えると閉じる / 別のカードを押すと掘り下げが消える', async () => {
    await same(D, (s, l) => {
      s.clickCard(1);
      s.click(nth(s, '#panel1 table.cdrill tbody tr button', 0));
      l.push(['team 選択後', panel(s)]);
      s.clickCard(2);
      l.push(['別カード', panel(s)]);
      s.clickTeam('平田チーム');
      l.push(['チップ切替で閉じる', [...panel(s), ...cardsOf(s)]]);
      s.clickCard(0);
      s.selectPerson(D.people.find((p) => p.team === '平田チーム')?.id ?? '');
      l.push(['個人選択で閉じる', [...panel(s), ...cardsOf(s)]]);
    });
  });
  it('下の一覧 (⑦ など) を開くと内訳は閉じ、内訳を開くと下の一覧は閉じる', async () => {
    await same(D, (s, l) => {
      s.clickCard(1);
      s.click(nth(s, '#cards2 .c', 0));
      l.push(['⑦ を開いた', [...panel(s), ...outline(s.el('panel'))]]);
      s.clickCard(3);
      l.push(['② を開いた', [...panel(s), ...outline(s.el('panel'))]]);
    });
  });
});

describe('カード内訳: 担当なし・名簿外の行 (合成データ)', () => {
  it('担当者の表に「担当なし」の行が出て、押すと一覧が開く', async () => {
    const { data } = withBlankOwner();
    const log = await same(data, (s, l) => { allCards(s, l, '全社(担当なし入り)'); });
    const hasBlankList = log.some(([t, lines]) => t.endsWith('> 担当なし') && lines.some((x) => x.startsWith('担当なし ')));
    expect(hasBlankList).toBe(true);
  }, 120_000);
  it('個人プルダウンで担当なしを選ぶ → いきなり取引一覧', async () => {
    const { data } = withBlankOwner();
    await same(data, (s, l) => {
      s.selectPerson('__none__');
      allCards(s, l, '担当なし選択');
    });
  }, 60_000);
  it('名簿外の担当者・名簿とずれた行があるとき', async () => {
    const d = withOffRosterRows();
    // card_deals にも同じ名簿外の行を足す
    const row = d.card_deals.pool[0];
    if (!row) throw new Error('fixture に pool の行が無い');
    d.card_deals.pool.push({ ...row, id: 'off-9', owner: 'ZZ-OFF', ownerName: '名簿外の人', team: '伊壺チーム' });
    await same(d, (s, l) => {
      allCards(s, l, '名簿外(全社)');
      s.clickTeam('伊壺チーム');
      allCards(s, l, '名簿外(伊壺)');
    });
  }, 120_000);
});
