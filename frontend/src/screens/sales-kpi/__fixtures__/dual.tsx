// テスト専用: 旧画面 templates/tabs/sales_kpi.html の script を happy-dom 上で**そのまま**動かし、
// React 版と同じ状態にして DOM を突き合わせるための道具。旧 JS を書き写さないので、写し間違いが入らない。
// 使う側のテストファイルの先頭に `// @vitest-environment happy-dom` が要る。
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { act, fireEvent, render } from '@testing-library/react';
import { vi } from 'vitest';
import { SalesKpiScreen } from '../SalesKpiScreen';
import type { SalesKpiData } from '../types';

const TPL_PATH = path.resolve(__dirname, '../../../../../templates/tabs/sales_kpi.html');

function template(): { html: string; script: string } {
  const src = readFileSync(TPL_PATH, 'utf8');
  const scripts = [...src.matchAll(/<script>([\s\S]*?)<\/script>/g)];
  const last = scripts[scripts.length - 1];
  if (!last?.[1]) throw new Error('旧画面の script が見つからない');
  return { html: src.replace(/<script>[\s\S]*?<\/script>/g, ''), script: last[1] };
}

function stubFetch(data: SalesKpiData): void {
  const body = JSON.stringify(data);
  vi.stubGlobal('fetch', () =>
    Promise.resolve(new Response(body, { status: 200, headers: { 'Content-Type': 'application/json' } })),
  );
}

/** 旧画面・React 版の両方を同じ操作で動かすための手。どちらも id・data 属性・文言が同じ DOM を出す。 */
export interface Screen {
  el: (id: string) => HTMLElement;
  q: (sel: string) => HTMLElement | null;
  qa: (sel: string) => HTMLElement[];
  click: (el: Element) => void;
  /** チームチップ (#teams .chip) を文言で押す */
  clickTeam: (label: string) => void;
  /** 個人プルダウン (#person) を value で選ぶ */
  selectPerson: (value: string) => void;
  /** 今月の成績カード (#cards1 .c) を左から n 番目 (0 始まり) 押す */
  clickCard: (n: number) => void;
  /** テキストが一致するボタン (チップ・戻る) を探して押す */
  clickButton: (within: string, text: string | RegExp) => void;
  teardown: () => void;
}

function driver(teardown: () => void): Screen {
  const el = (id: string): HTMLElement => {
    const e = document.getElementById(id);
    if (!e) throw new Error('#' + id + ' が無い');
    return e;
  };
  const click = (e: Element): void => {
    act(() => {
      fireEvent.click(e);
    });
  };
  return {
    el,
    q: (sel) => document.querySelector<HTMLElement>(sel),
    qa: (sel) => Array.from(document.querySelectorAll<HTMLElement>(sel)),
    click,
    clickTeam: (label) => {
      const b = [...document.querySelectorAll('#teams .chip')].find((c) => c.textContent === label);
      if (!b) throw new Error('チームチップが無い: ' + label);
      click(b);
    },
    selectPerson: (value) => {
      act(() => {
        fireEvent.change(el('person'), { target: { value } });
      });
    },
    clickCard: (n) => {
      const c = document.querySelectorAll('#cards1 .c')[n];
      if (!c) throw new Error('カードが無い: ' + String(n));
      click(c);
    },
    clickButton: (within, text) => {
      const b = [...document.querySelectorAll(within + ' button')].find((x) =>
        typeof text === 'string' ? x.textContent === text : text.test(x.textContent),
      );
      if (!b) throw new Error(within + ' にボタンが無い: ' + String(text));
      click(b);
    },
    teardown,
  };
}

async function waitCards(): Promise<void> {
  await vi.waitFor(
    () => {
      if (document.querySelectorAll('#cards1 .c').length !== 7) throw new Error('まだ描けていない');
    },
    { timeout: 5000 },
  );
}

/** 旧画面を data で起動して、最初の描画 (#cards1 に 7 枚) が出るまで待つ。 */
export async function mountOld(data: SalesKpiData, hidden: string[] = []): Promise<Screen> {
  const { html, script } = template();
  document.body.innerHTML = html;
  localStorage.setItem('salesKpi.hidden.v1', JSON.stringify(hidden));
  Element.prototype.scrollIntoView = () => undefined;
  stubFetch(data);
  // 旧画面の script は async IIFE。トップレベルの eval で走らせる。
  (0, eval)(script);
  await waitCards();
  return driver(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    vi.unstubAllGlobals();
  });
}

/** React 版 (SalesKpiScreen) を同じ data で起動する。 */
export async function mountNew(data: SalesKpiData, hidden: string[] = []): Promise<Screen> {
  localStorage.setItem('salesKpi.hidden.v1', JSON.stringify(hidden));
  Element.prototype.scrollIntoView = () => undefined;
  stubFetch(data);
  const r = render(<SalesKpiScreen />);
  await waitCards();
  return driver(() => {
    r.unmount();
    document.body.innerHTML = '';
    localStorage.clear();
    vi.unstubAllGlobals();
  });
}

// ---------------------------------------------------------------- 突き合わせ用の抜き出し

/** 突き合わせに使う属性。旧画面が E2E で使っている id / data 属性だけ。style・tag・class の細部は見ない。 */
const ATTRS = [
  'id',
  'data-seg',
  'data-n',
  'data-total',
  'data-num',
  'data-name',
  'data-nt',
  'data-sum',
  'data-sumnum',
  'data-sum-num',
  'data-level',
  'data-bpo',
];

/**
 * 要素の中身を「見える文字 + 印の属性」の行にする。隣り合う文字は 1 行にまとめ、要素の境目で改行する
 * (旧画面は innerHTML の文字列、React は要素ごとに文字を分けて出すので、text node の切れ目には依存しない)。
 */
export function outline(root: Element): string[] {
  const out: string[] = [];
  const walk = (node: Node): void => {
    let buf = '';
    const flush = (): void => {
      const t = buf.replace(/\u00a0/g, ' ').replace(/\s+/g, ' ').trim();
      if (t) out.push(t);
      buf = '';
    };
    for (const ch of Array.from(node.childNodes)) {
      if (ch.nodeType === 3) {
        buf += ch.textContent ?? '';
      } else if (ch.nodeType === 1) {
        const e = ch as Element;
        const tag = e.tagName.toLowerCase();
        if (tag === 'style' || tag === 'script') continue;
        // span・b・small はインライン。文字を切らない (旧: 1 つの文字列、新: 複数の要素)
        const inline = tag === 'span' || tag === 'b' || tag === 'small' || tag === 'i';
        const marks = ATTRS.filter((a) => e.hasAttribute(a)).map((a) => a + '=' + (e.getAttribute(a) ?? ''));
        if (!inline || marks.length) {
          flush();
          if (marks.length) out.push('@' + marks.join(' '));
        }
        if (inline && !marks.length) {
          buf += e.textContent;
        } else {
          walk(e);
          flush();
        }
      }
    }
    flush();
  };
  walk(root);
  return out;
}
