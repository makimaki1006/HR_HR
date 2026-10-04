// @vitest-environment happy-dom
// 今月の成績カードの内訳 (#45) を、旧画面の script をそのまま動かした DOM と React 版で突き合わせる。
// 同じ操作 (カードを押す → 区分・BPO・チーム → 担当者 → 取引一覧と全部降りる) を両方に流し、
// そのたびの #panel1 の中身 (文字と id・data 属性) が全部同じになることを見る。
import { afterEach, describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { outline } from './__fixtures__/dual';
import { allCards, cardsOf, nth, panel, same } from './__fixtures__/cardWalk';
import { withBlankOwner, withOffRosterRows } from './__fixtures__/synthetic';

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
