// @vitest-environment happy-dom
// 絞り込み (チーム・個人・担当なし) を、旧画面の script をそのまま動かした DOM と React 版で突き合わせる。
// fixture にはチーム未設定の 1 人を担当なし ('') に書き換えた合成データも使う (fixture には担当なしの取引が無い)。
import { afterEach, describe, expect, it } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { mountNew, mountOld, outline, type Screen } from './__fixtures__/dual';
import { withBlankOwner, withOffRosterRows } from './__fixtures__/synthetic';
import type { SalesKpiData } from './types';

const KPI = ['scope', 'cards1', 'cards2', 'h2kaden', 'lead3', 'cards3', 'kadenbar', 'h2kadenlist', 'lead3b', 'cards3b', 'listbar'];

// 🔴 カードを押すと内訳が開く表示 (#45、差の A1) は次の PR で入れる。それまでは #cards1 のこの 2 行だけを両方から除く。
const CARD_OPEN_LINES = new Set(['一覧を見る ▾', '閉じる ▲']);

function snap(s: Screen, ids: string[]): Record<string, string[]> {
  const o: Record<string, string[]> = {};
  for (const id of ids) {
    const e = s.q('#' + id);
    o[id] = e ? outline(e).filter((l) => id !== 'cards1' || !CARD_OPEN_LINES.has(l)) : ['<領域なし>'];
  }
  o['#person options'] = s.qa('#person option').map((x) => (x as HTMLOptionElement).value + '=' + x.textContent);
  return o;
}

function ketteiSnap(s: Screen): Record<string, string[]> {
  const tab = s.qa('#tabs [role=tab]').find((t) => t.textContent === '決定者・決裁者');
  if (!tab) throw new Error('決定者タブが無い');
  s.click(tab);
  return snap(s, ['lead5', 'ketteibox']);
}

interface Step {
  name: string;
  run: (s: Screen) => void;
}
const steps = (blankValue: string | null): Step[] => [
  { name: '初期 (全社)', run: () => undefined },
  { name: 'チーム=伊壺チーム', run: (s) => { s.clickTeam('伊壺チーム'); } },
  { name: 'チーム=チーム未設定', run: (s) => { s.clickTeam('チーム未設定'); } },
  ...(blankValue === null
    ? []
    : [
        { name: '担当なし (チーム未設定の中で)', run: (s: Screen) => { s.selectPerson(blankValue); } },
        { name: '担当なし → 未選択に戻す', run: (s: Screen) => { s.selectPerson(''); } },
        { name: 'チーム=すべて', run: (s: Screen) => { s.clickTeam('すべて'); } },
        { name: '担当なし (全社)', run: (s: Screen) => { s.selectPerson(blankValue); } },
      ]),
];

async function collect(mount: typeof mountOld, data: SalesKpiData, blank: string | null, hidden: string[] = []) {
  const s = await mount(data, hidden);
  const out: Record<string, Record<string, string[]>> = {};
  try {
    for (const st of steps(blank)) {
      st.run(s);
      out[st.name] = snap(s, KPI);
      if (st.name.startsWith('担当なし')) Object.assign(out[st.name] as object, ketteiSnap(s));
      if (st.name.startsWith('担当なし')) {
        // 決定者タブを見たあとは営業KPIタブへ戻す (次の手順が #cards1 を見るため)
        const tab = s.qa('#tabs [role=tab]').find((t) => t.textContent === '営業KPI');
        if (tab) s.click(tab);
      }
    }
  } finally {
    s.teardown();
  }
  return out;
}

afterEach(() => {
  document.body.innerHTML = '';
});

describe('絞り込み: 旧画面と React 版が同じ表示になる (fixture)', () => {
  it('全社・チーム', async () => {
    const D = loadFixture();
    const o = await collect(mountOld, D, null);
    const n = await collect(mountNew, D, null);
    expect(n).toEqual(o);
  });
});

describe('名簿に居ない担当者・名簿とずれた行 (下段の一覧の絞り込み規則 inScope)', () => {
  it('チームを切り替えても旧と同じ件数・同じ表示', async () => {
    const D = withOffRosterRows();
    const o = await collect(mountOld, D, null);
    const n = await collect(mountNew, D, null);
    expect(n).toEqual(o);
    // 旧画面の具体値: 伊壺チームの ⑦ は fixture で 1 件。名簿外の人の行 (off-1) は行のチームで入って 2 件、
    // 名簿が平田チームの人の行 (off-2) は行が「伊壺チーム」と言っていても入らない。
    expect(loadFixture().stale.filter((r) => r.team === '伊壺チーム')).toHaveLength(1);
    expect(o['チーム=伊壺チーム']?.cards2?.slice(0, 2)).toEqual(['⑦ ステージが止まっている', '2件']);
  });
});

describe('担当なしの選択: 旧画面と React 版が同じ表示になる (合成データ)', () => {
  const { data } = withBlankOwner();
  it('番兵 __none__ で選べて、旧と同じ', async () => {
    const o = await collect(mountOld, data, '__none__');
    const n = await collect(mountNew, data, '__none__');
    expect(n).toEqual(o);
  });
  it('旧画面の担当なしは「選択中」の見出し・架電の注記になる (旧画面の値を直接確かめる)', async () => {
    const o = await collect(mountOld, data, '__none__');
    const all = o['担当なし (全社)'];
    expect(all?.h2kaden).toEqual(['架電（担当なしを選択中）']);
    expect(all?.h2kadenlist).toEqual(['架電リストの残り（担当なしを選択中）']);
    expect(all?.scope?.[0]).toContain('担当なし の数字だけを表示しています。');
  });
  it('チェックで外した人が混じっても同じ', async () => {
    const hidden = data.people.filter((p) => p.team === '平田チーム').slice(0, 2).map((p) => p.id);
    const o = await collect(mountOld, data, '__none__', hidden);
    const n = await collect(mountNew, data, '__none__', hidden);
    expect(n).toEqual(o);
  });
});

describe('担当者のチェックを外すと個人指定が解ける (null に戻る)', () => {
  it('選んだ人のチェックを外す → 全社の表示に戻る。旧と同じ', async () => {
    const D = loadFixture();
    const target = D.people.find((p) => p.team === '伊壺チーム');
    if (!target) throw new Error('fixture に伊壺チームの人が居ない');
    const run = async (mount: typeof mountOld) => {
      const s = await mount(D);
      try {
        s.selectPerson(target.id);
        const before = snap(s, KPI);
        s.click(s.el('pickbtn'));
        const cb = s
          .qa('#pickpanel label')
          .find((l) => l.textContent === target.name)
          ?.querySelector('input');
        if (!cb) throw new Error('チェックボックスが無い');
        s.click(cb);
        return { before, after: snap(s, KPI), pickcount: outline(s.el('pickcount')) };
      } finally {
        s.teardown();
      }
    };
    const o = await run(mountOld);
    const n = await run(mountNew);
    expect(n).toEqual(o);
    expect(o.after.scope?.[0]).toContain('全チームの合計を表示しています。');
    expect(o.before.scope?.[0]).toContain(target.name + ' の数字だけを表示しています。');
  });
});
