"""旧 /tab/guide と新 /app/guide の表示値を比較する (React 移行 W3 の旧新一致確認)。

使い方 (サーバは別途起動しておく):
    PORT=9316 AUTH_PASSWORD=testpass ALLOWED_DOMAINS=f-a-c.co.jp ./rust_dashboard.exe
    python tests/e2e/guide_old_new_compare.py [--base http://localhost:9316] [--shots <dir>]

比べるもの: 見出し (h2-h5)・段落 (p)・リスト項目 (li)・表の見出しセル (th)・表セル (td)・
折りたたみ見出し (summary) のテキストを出現順で、画像は src と alt と枚数。
空白は 1 つに畳んで比較する。不一致があれば列挙して exit 1。
"""

import argparse
import os
import re
import sys

from playwright.sync_api import sync_playwright

CHROME = "C:/Users/fuji1/AppData/Local/ms-playwright/chromium-1155/chrome-win/chrome.exe"

EXTRACT = """(sel) => {
  const root = document.querySelector(sel);
  if (!root) return null;
  const t = (q) => [...root.querySelectorAll(q)].map(e => e.textContent);
  return {
    headings: [...root.querySelectorAll('h1,h2,h3,h4,h5,h6')].map(e => e.tagName + ':' + e.textContent),
    paragraphs: t('p'),
    list_items: t('li'),
    th: t('th'),
    td: t('td'),
    summaries: t('summary'),
    images: [...root.querySelectorAll('img')].map(i => [i.getAttribute('src'), i.getAttribute('alt')]),
    tables: root.querySelectorAll('table').length,
    rows: root.querySelectorAll('tr').length,
    details: root.querySelectorAll('details').length,
  };
}"""


def norm(s):
    return re.sub(r"\s+", " ", s).strip()


def normalize(d):
    out = {}
    for k, v in d.items():
        if isinstance(v, list) and v and isinstance(v[0], str):
            out[k] = [norm(x) for x in v]
        else:
            out[k] = v
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://localhost:9316")
    ap.add_argument("--shots", default=None)
    args = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf8")
    if args.shots:
        os.makedirs(args.shots, exist_ok=True)

    with sync_playwright() as p:
        b = p.chromium.launch(executable_path=CHROME)
        pg = b.new_page(viewport={"width": 1280, "height": 900})
        errors = []
        # ブラウザが自動で取りに行く /favicon.ico の 404 はアプリ全体の既存事象なので除外する
        pg.on(
            "console",
            lambda m: m.type == "error"
            and not m.location.get("url", "").endswith("/favicon.ico")
            and errors.append(f"{m.text} @ {m.location.get('url')}"),
        )
        pg.on("response", lambda r: r.status >= 400 and errors.append(f"{r.status} {r.url}"))
        pg.goto(args.base + "/login")
        pg.fill("#email", "test@f-a-c.co.jp")
        pg.fill("#password", "testpass")
        pg.click("form[action='/login'] button[type=submit]")
        pg.wait_for_load_state("networkidle")

        pg.goto(args.base + "/tab/guide")
        pg.wait_for_load_state("networkidle")
        old = pg.evaluate(EXTRACT, "body > div")
        if args.shots:
            pg.evaluate("document.querySelectorAll('details').forEach(d => d.open = true)")
            pg.screenshot(path=os.path.join(args.shots, "old_tab_guide_expanded.png"), full_page=True)

        pg.goto(args.base + "/app/guide")
        pg.wait_for_selector("[data-testid=guide-root]")
        pg.wait_for_load_state("networkidle")
        new = pg.evaluate(EXTRACT, "[data-testid=guide-root]")
        pg.evaluate("document.querySelectorAll('details').forEach(d => d.open = true)")
        # 画像は loading="lazy" (旧画面と同じ) なので、1 枚ずつ表示位置へスクロールして読み込みを待つ
        widths = pg.evaluate(
            """async () => {
              const out = [];
              for (const i of document.querySelectorAll('[data-testid=guide-root] img')) {
                i.scrollIntoView();
                if (!i.complete || i.naturalWidth === 0) {
                  await new Promise(r => { i.onload = r; i.onerror = r; setTimeout(r, 5000); });
                }
                out.push([i.getAttribute('src'), i.naturalWidth]);
              }
              return out;
            }"""
        )
        if args.shots:
            pg.screenshot(path=os.path.join(args.shots, "new_app_guide_expanded.png"), full_page=True)
        b.close()

    if old is None or new is None:
        print("root element not found: old", old is not None, "new", new is not None)
        return 1
    old, new = normalize(old), normalize(new)
    bad = 0
    for k in old:
        o, n = old[k], new[k]
        size = (len(o), len(n)) if isinstance(o, list) else (o, n)
        print(f"{k:11s} old={size[0]} new={size[1]} equal={o == n}")
        if o != n:
            bad += 1
            if isinstance(o, list):
                for i in range(max(len(o), len(n))):
                    a = o[i] if i < len(o) else "<none>"
                    c = n[i] if i < len(n) else "<none>"
                    if a != c:
                        print(f"  [{i}] old={a!r}\n       new={c!r}")
    broken = [s for s, w in widths if not w]
    print("new images loaded:", len(widths) - len(broken), "/", len(widths), "broken:", broken)
    print("console errors:", errors)
    if broken or errors:
        bad += 1
    print("RESULT:", "MATCH" if bad == 0 else f"MISMATCH ({bad})")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
