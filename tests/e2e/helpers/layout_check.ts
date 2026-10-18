// 画面の文字の重なり・はみ出し・図の文字の切れを見つける（読むだけ。画面は変えない）。
// コンサルKPI と案件管理の操作テストで共用する。2026-10-18、求人のライフラインで
// 「Indeed の実績データなし」が 1 列の幅で折り返して下の行に重なった件（PR #260）を受けて作った。
//
// 見るもの:
//   overlaps     … 別々の要素の文字どうしが重なっている（「…」で省略した分・枠の外に隠れた分は、見えている部分だけで比べる）
//   spills       … 文字が箱の幅を超えてはみ出している（overflow が visible の要素だけ。スクロール枠・省略は除く）
//   svgClipped   … 図（SVG）の文字が図の外に切れている
//   pageOverflow … 画面全体が横にはみ出している（横スクロールが出る）
import type { Page } from '@playwright/test';

export interface LayoutProblem { text: string; x: number; y: number }
export interface LayoutReport {
  overlaps: LayoutProblem[];
  spills: LayoutProblem[];
  svgClipped: LayoutProblem[];
  pageOverflow: boolean;
}

export async function findLayoutProblems(page: Page, rootSelector = 'body'): Promise<LayoutReport> {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel) ?? document.body;
    const visible = (el: Element) => {
      // 閉じた details の中身や、祖先ごと透明な文字は画面に見えていない。
      if (!el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) return false;
      const s = getComputedStyle(el);
      if (s.visibility === 'hidden' || s.display === 'none' || Number(s.opacity) === 0) return false;
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    };
    // 祖先のうち、はみ出しを隠す箱（overflow が visible 以外）の重なり部分。読み上げ専用の隠し文字は null
    const clipOf = (el: Element) => {
      let c = { l: 0, t: 0, r: innerWidth, b: innerHeight };
      for (let p: Element | null = el; p && p !== document.documentElement; p = p.parentElement) {
        const s = getComputedStyle(p);
        if ((s.clip && s.clip !== 'auto') || (s.clipPath && s.clipPath !== 'none')) return null;
        if (s.overflowX !== 'visible' || s.overflowY !== 'visible') {
          const pr = p.getBoundingClientRect();
          if (pr.width * pr.height < 9) return null;
          c = { l: Math.max(c.l, pr.left), t: Math.max(c.t, pr.top), r: Math.min(c.r, pr.right), b: Math.min(c.b, pr.bottom) };
        }
      }
      return c;
    };
    type Box = { el: Element; l: number; t: number; r: number; b: number; text: string };
    const boxes: Box[] = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = (n.textContent ?? '').trim();
      const el = n.parentElement;
      if (!text || !el || !visible(el) || el.closest('script,style,svg,option,select,textarea')) continue;
      const c = clipOf(el);
      if (!c) continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      for (const q of Array.from(range.getClientRects())) {
        const box = { el, l: Math.max(q.left, c.l), t: Math.max(q.top, c.t), r: Math.min(q.right, c.r), b: Math.min(q.bottom, c.b), text: text.slice(0, 30) };
        if (box.r - box.l >= 2 && box.b - box.t >= 2) {
          // 固定の帯・メニューなどの下に隠れた文字は重なりに数えない。
          const x = (box.l + box.r) / 2, y = (box.t + box.b) / 2;
          const front = document.elementFromPoint(x, y);
          let covered = false;
          for (let p = front; p && !p.contains(el) && !el.contains(p); p = p.parentElement) {
            const style = getComputedStyle(p), background = style.backgroundColor;
            if (['sticky', 'fixed'].includes(style.position) && background !== 'transparent' && !background.endsWith(', 0)')) { covered = true; break; }
          }
          if (covered) continue;
          boxes.push(box);
        }
      }
    }
    const at = (x: number, y: number) => ({ x: Math.round(x + scrollX), y: Math.round(y + scrollY) });
    const overlaps: LayoutProblem[] = [];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i], b = boxes[j];
        if (a.el === b.el || a.el.contains(b.el) || b.el.contains(a.el)) continue;
        const w = Math.min(a.r, b.r) - Math.max(a.l, b.l);
        const h = Math.min(a.b, b.b) - Math.max(a.t, b.t);
        if (w > 3 && h > Math.min(a.b - a.t, b.b - b.t) * 0.4) overlaps.push({ text: `${a.text} ⇔ ${b.text}`, ...at(a.l, a.t) });
      }
    }
    const spills: LayoutProblem[] = [];
    for (const el of Array.from(root.querySelectorAll('*'))) {
      if (!visible(el) || el.closest('svg,select,textarea,input') || el.children.length > 3) continue; // 選択欄・入力欄は中身を自分で切って見せる
      const s = getComputedStyle(el);
      if (s.overflowX !== 'visible' || s.display === 'inline' || !(el.textContent ?? '').trim()) continue;
      if (el.scrollWidth > el.clientWidth + 2 && el.clientWidth > 0) {
        const r = el.getBoundingClientRect();
        spills.push({ text: `${(el.textContent ?? '').trim().slice(0, 30)}（幅 ${el.clientWidth} に ${el.scrollWidth}）`, ...at(r.left, r.top) });
      }
    }
    const svgClipped: LayoutProblem[] = [];
    for (const t of Array.from(root.querySelectorAll('svg text'))) {
      const svg = t.closest('svg');
      if (!svg || !visible(svg) || !(t.textContent ?? '').trim()) continue;
      const sr = svg.getBoundingClientRect(), tr = t.getBoundingClientRect();
      if (tr.width === 0) continue;
      if (tr.left < sr.left - 1 || tr.right > sr.right + 1 || tr.top < sr.top - 1 || tr.bottom > sr.bottom + 1) {
        svgClipped.push({ text: (t.textContent ?? '').trim().slice(0, 30), ...at(tr.left, tr.top) });
      }
    }
    const uniq = (list: LayoutProblem[]) => [...new Map(list.map(p => [p.text, p])).values()];
    return { overlaps: uniq(overlaps), spills: uniq(spills), svgClipped: uniq(svgClipped), pageOverflow: document.documentElement.scrollWidth > innerWidth + 1 };
  }, rootSelector);
}
