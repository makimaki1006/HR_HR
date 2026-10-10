import DOMPurify from 'dompurify';

/**
 * 活動 (メール・メモ・ミーティング) の HTML 本文を安全に描くための整形。
 * HubSpot の本文は外部から届いたメールを含むので、描く前に必ず無害化する。
 * 許すのは基本の書式・リンク・画像 (https のみ) だけで、script / style / iframe / form / イベント属性は除く。
 */
const purify = DOMPurify(window);

const ALLOWED_TAGS = [
  'a', 'b', 'strong', 'i', 'em', 'u', 's', 'br', 'p', 'div', 'span', 'ul', 'ol', 'li', 'blockquote', 'pre', 'code',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'table', 'thead', 'tbody', 'tr', 'td', 'th', 'img', 'sub', 'sup',
];
const ALLOWED_ATTR = ['href', 'src', 'alt', 'title', 'width', 'height', 'colspan', 'rowspan', 'class', 'id'];

const SAFE_LINK = /^(https?:|mailto:|tel:)/i;

purify.addHook('afterSanitizeAttributes', node => {
  if (node.tagName === 'A') {
    const href = node.getAttribute('href');
    if (href !== null && !SAFE_LINK.test(href.trim())) node.removeAttribute('href');
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer');
  }
  if (node.tagName === 'IMG') {
    const src = node.getAttribute('src');
    if (src === null || !/^https:\/\//i.test(src.trim())) {
      node.remove();
      return;
    }
    node.setAttribute('referrerpolicy', 'no-referrer');
    node.setAttribute('loading', 'lazy');
  }
});

/** HTML を無害化して文字列で返す */
export function sanitizeHtml(html: string): string {
  return purify.sanitize(html, {
    ALLOWED_TAGS, ALLOWED_ATTR, ALLOW_DATA_ATTR: false, FORBID_TAGS: ['style', 'script', 'iframe', 'form', 'object', 'embed'],
  });
}

const QUOTE_SELECTOR = 'blockquote, .gmail_quote, .gmail_attr, #divRplyFwdMsg, #appendonsend, .moz-cite-prefix, .yahoo_quoted, .hs-quote';
const QUOTE_MARKER = /^\s*(-{2,}\s*(original message|forwarded message|元のメッセージ|転送メッセージ)|on .{3,200} wrote:\s*$|.{3,120}は書きました[:：]?\s*$)/i;

export interface SplitHtml { main: string; quoted: string }

function hasText(html: string): boolean {
  const d = document.createElement('div');
  d.innerHTML = html;
  return d.textContent.trim() !== '';
}

/** 無害化済みの HTML を、本文と「以前のやりとり (引用)」に分ける。引用が無い・本文が空になる場合は quoted を空にする */
export function splitQuotedHtml(safeHtml: string): SplitHtml {
  const root = document.createElement('div');
  root.innerHTML = safeHtml;
  let start: Element | null = root.querySelector(QUOTE_SELECTOR);
  if (start === null) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n !== null; n = walker.nextNode()) {
      if (QUOTE_MARKER.test(n.nodeValue ?? "")) {
        const p = n.parentElement;
        if (p !== null && p !== root) start = p;
        break;
      }
    }
  }
  if (start === null) return { main: safeHtml, quoted: '' };
  const moved = document.createElement('div');
  let cur: Element | null = start;
  while (cur !== null && cur !== root) {
    const parent: Element | null = cur.parentElement;
    let n: ChildNode | null = cur;
    while (n !== null) {
      const next: ChildNode | null = n.nextSibling;
      moved.appendChild(n);
      n = next;
    }
    cur = parent;
  }
  const main = root.innerHTML;
  if (!hasText(main)) return { main: safeHtml, quoted: '' };
  return { main, quoted: moved.innerHTML };
}

/** 平文の本文を、本文と引用 (`>` で始まる行・「-----Original Message-----」以降) に分ける */
export function splitQuotedText(text: string): { main: string; quoted: string } {
  const lines = text.split('\n');
  let idx = lines.findIndex(l => QUOTE_MARKER.test(l) || l.trimStart().startsWith('>'));
  if (idx < 0) return { main: text, quoted: '' };
  while (idx > 0 && lines[idx - 1]?.trim() === '') idx -= 1;
  const main = lines.slice(0, idx).join('\n').trimEnd();
  if (main.trim() === '') return { main: text, quoted: '' };
  return { main, quoted: lines.slice(idx).join('\n') };
}

/** 最初の数行 (折りたたみ時の下見) */
export function previewLines(text: string, n = 3): string {
  return text.split('\n').map(l => l.trim()).filter(l => l !== '').slice(0, n).join('\n');
}

/** HTML から平文を取り出す (下見用) */
export function htmlToText(safeHtml: string): string {
  const d = document.createElement('div');
  d.innerHTML = safeHtml.replace(/<(br|\/p|\/div|\/li|\/tr|\/h[1-6])\s*\/?>/gi, '$&\n');
  return d.textContent;
}
