// @vitest-environment happy-dom
// 商談属性 (#49・#50): カード内訳の「商談属性」の表・種別で絞る・区分 × 種別・分子の注釈・一覧の行の種別表示を、
// 旧画面の script をそのまま動かした DOM と React 版で突き合わせる。
// 種別の列があるデータ = Rust の examples/dump_sales_kpi.rs --attr の JSON (既知の 3 値・空・空白・定義外・; 区切りが混ざる)。
import { afterEach, describe, expect, it } from 'vitest';
import { loadFixture, loadAttrFixture } from './__fixtures__/load';
import { outline, type Screen } from './__fixtures__/dual';
import { allCards, cardsOf, nth, panel, segBtn, same, walk, type Log } from './__fixtures__/cardWalk';
import { withBlankOwner } from './__fixtures__/synthetic';

const ntNames = (s: Screen): string[] => s.qa('#panel1 table.ntt tbody tr').map((r) => r.getAttribute('data-nt') ?? '');

function pickNt(s: Screen, name: string): void {
  const tr = s.qa('#panel1 table.ntt tbody tr').find((r) => r.getAttribute('data-nt') === name);
  const b = tr?.querySelector('button');
  if (!b) throw new Error('種別の行が無い: ' + name);
  s.click(b);
}

/** 種別を 1 つずつ押して絞り (下の段も降りる)、バッジで外す。 */
function ntSteps(s: Screen, log: Log, tag: string, max: number): void {
  for (const name of ntNames(s).slice(0, max)) {
    pickNt(s, name);
    walk(s, log, `${tag} [種別 ${name}]`, 1);
    s.click(s.el('panel1-nt-chip'));
  }
  log.push([`${tag} [種別を外した]`, panel(s)]);
}

/** 7 枚のカードで、(区分なし + 各区分) × 種別の絞りを順に確かめる。 */
function ntCards(s: Screen, log: Log, tag: string): void {
  for (let i = 0; i < 7; i++) {
    s.clickCard(i);
    ntSteps(s, log, `${tag} card${String(i)}`, 8);
    const segs = s.qa('#panel1 [data-seg]').map((b) => b.getAttribute('data-seg') ?? '').filter((x) => x);
    for (const k of segs) {
      s.click(segBtn(s, k));
      log.push([`${tag} card${String(i)} 区分 ${k}`, panel(s)]);
      ntSteps(s, log, `${tag} card${String(i)} 区分 ${k}`, 2);
      s.click(segBtn(s, k)); // 区分を外す
    }
    const bpo = s.q('#panel1-bpo');
    if (bpo) {
      s.click(bpo);
      ntSteps(s, log, `${tag} card${String(i)} BPO`, 2);
      s.click(s.el('panel1-bpo'));
    }
    s.clickCard(i);
  }
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('商談属性: 旧画面と React 版が同じ (種別の列がある fixture)', () => {
  const D = loadAttrFixture();
  it('前提: 属性の列があり、Rust が整えた表示名が来る (React は変換しない。旧「商談種別」の内部値は定義外)', () => {
    expect(D.deal_attr_available).toBe(true);
    expect(D.deal_attr_sheets).toEqual({ apo: true, cyomi: true, pool: true });
    expect(D.deal_attr_fixed).toEqual(['決裁者商談', '決定者商談', '担当者商談']);
    const labels = new Set(D.card_deals.pool.map((r) => r.deal_attr));
    expect(labels.has('代表者商談')).toBe(false);
    expect([...labels]).toContain('代表者商談(定義外)');
    expect([...labels]).toContain('決裁者商談;担当者商談(定義外)');
    expect([...labels]).toContain('(未設定)');
  });
  it('全社: 7 枚すべて、区分・BPO・チーム → 担当者 → 一覧と種別の表を全部降りる', async () => {
    await same(D, (s, l) => { allCards(s, l, '全社'); });
  }, 280_000);
  it('全社: 種別を押して絞る (各カード・区分 × 種別・BPO × 種別) と下の段にも効く', async () => {
    const log = await same(D, (s, l) => { ntCards(s, l, '全社'); });
    expect(log.length).toBeGreaterThan(100);
  }, 280_000);
  it('第1チーム・個人・チェック外し', async () => {
    const id = D.people.find((p) => p.team === '第1チーム')?.id ?? '';
    await same(D, (s, l) => {
      s.clickTeam('第1チーム');
      ntCards(s, l, '伊壺');
      s.clickTeam('すべて');
      s.selectPerson(id);
      ntCards(s, l, '個人');
    });
    const hidden = D.people.filter((p) => p.team === '第1チーム').slice(0, 3).map((p) => p.id);
    await same(D, (s, l) => { ntCards(s, l, '外す'); }, hidden);
  }, 280_000);
  it('分子でない区分を選ぶと「分子の列は計算できません」の注釈 (⑥⑤)。分子の区分では出ない', async () => {
    const log = await same(D, (s, l) => {
      s.clickCard(4); // ⑥ 商談化率
      for (const k of ['実施', '未実施', '未処理', '要判定']) {
        s.click(segBtn(s, k));
        l.push([`⑥ 区分 ${k}`, panel(s)]);
        s.click(segBtn(s, k));
      }
      s.clickCard(4);
      s.clickCard(5); // ⑤ アンケート回収率
      for (const k of ['回収済み', '未回収']) {
        s.click(segBtn(s, k));
        l.push([`⑤ 区分 ${k}`, panel(s)]);
        s.click(segBtn(s, k));
      }
    });
    const note = '選んでいる区分（未実施）は分子に当たらないため、分子の列は計算できません（0 と表示しています）';
    expect(log.find(([t]) => t === '⑥ 区分 未実施')?.[1]).toContain(note);
    expect(log.find(([t]) => t === '⑥ 区分 実施')?.[1].join('\n')).not.toContain('は分子に当たらない');
    expect(log.find(([t]) => t === '⑤ 区分 未回収')?.[1].join('\n')).toContain('選んでいる区分（未回収）は分子に当たらない');
  }, 120_000);
  it('下の一覧 (⑦ など) の行に「 ・種別」が付く', async () => {
    const log = await same(D, (s, l) => {
      for (let i = 0; i < 5; i++) {
        s.click(nth(s, '#cards2 .c', i));
        l.push([`下の一覧 ${String(i)}`, [...outline(s.el('panel'))]]);
      }
      s.click(nth(s, '#cards2 .c', 3));
      s.click(s.qa('#panel .wkday:not(.zero)')[0] ?? s.el('panel'));
      l.push(['今週の 1 日', outline(s.el('panel'))]);
    });
    const joined = log.map(([, x]) => x.join('\n')).join('\n');
    expect(joined).toMatch(/ ・決裁者商談/);
    // 旧「商談種別」の内部値は、定義外の印なしでは画面に出ない
    expect(joined).not.toMatch(/代表者商談(?!\(定義外\))/);
  }, 120_000);
});

describe('商談属性: 担当なし・列が無いシート', () => {
  it('担当なし (合成) でも種別の表が降りられる', async () => {
    const { data } = withBlankOwner(loadAttrFixture());
    await same(data, (s, l) => {
      s.selectPerson('__none__');
      ntCards(s, l, '担当なし');
    });
  }, 200_000);
  it('列が無い fixture: 「商談属性: 未取得」の注記だけで、表も絞りも出ない', async () => {
    const D = loadFixture();
    const log = await same(D, (s, l) => {
      for (let i = 0; i < 7; i++) {
        s.clickCard(i);
        l.push([`card${String(i)}`, panel(s)]);
        s.clickCard(i);
      }
      l.push(['cards', cardsOf(s)]);
    });
    const opened = log.filter(([t]) => t.startsWith('card') && t !== 'cards');
    expect(opened).toHaveLength(7);
    for (const [t, lines] of opened) {
      expect(lines, t).toContain('商談属性: 未取得（シートに「商談属性」の列がまだありません）');
      expect(lines.filter((x) => x.includes('商談属性')), t).toEqual(['商談属性: 未取得（シートに「商談属性」の列がまだありません）']);
      expect(lines.join(' '), t).not.toMatch(/data-level|panel1-nt-chip/);
    }
  }, 120_000);
});
