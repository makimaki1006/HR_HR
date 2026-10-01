"""営業KPI「今月の成績」カード内訳の画面確認（fixture の JSON を簡易サーバで返す）。

本物の Sheets は読まない。`cargo run --example dump_sales_kpi -- out.json 2026-09-04` で書き出した
JSON を `/api/sales-kpi/data` として返し、テンプレートをそのまま `/sales-kpi` で返す。

使い方:
  python tests/e2e/sales_kpi_card_breakdown.py --json out.json --out <スクショ先> \
      [--template templates/tabs/sales_kpi.html] [--cards-only cards.json]

  --cards-only を付けると、内訳は触らず 7 枚のカードの数字だけを書き出す（変更前テンプレートとの比較用）。
確かめること（--cards-only なし）:
  - 7 枚それぞれを押してパネルが開き、パネル見出しの件数 == カードの値、
  - チーム表の合計 == 見出しの件数、担当者表の合計 == チームの件数、
    全担当者の取引一覧の行数の合計 == 見出しの件数、
  - 内 BPO のボタンの件数 == カードの「内 BPO n件」、押すと BPO の行だけになる、
  - ⑥ ⑤ の分子と分母の内訳、
  - 上を チーム選択・担当者選択・チェック外し の複数通りで繰り返す。
"""
import argparse
import json
import re
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from playwright.sync_api import sync_playwright

CHROME = "C:/Users/fuji1/AppData/Local/ms-playwright/chromium-1155/chrome-win/chrome.exe"
STATS = {"drill": 0, "rows": 0}
CARDS = ["apo", "pool", "den", "done", "rate", "anqrate", "cyomi"]  # 画面の左から順


def serve(tpl_path: Path, json_path: Path, port: int):
    page = tpl_path.read_bytes()
    data = json_path.read_bytes()

    class H(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.startswith("/api/sales-kpi/data"):
                body, ct = data, "application/json; charset=utf-8"
            elif self.path.startswith("/sales-kpi"):
                body, ct = page, "text/html; charset=utf-8"
            else:
                self.send_response(404)
                self.end_headers()
                return
            self.send_response(200)
            self.send_header("Content-Type", ct)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *a):
            pass

    srv = ThreadingHTTPServer(("127.0.0.1", port), H)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def num(text: str):
    m = re.search(r"-?[\d,]+(?:\.\d+)?", text or "")
    return float(m.group(0).replace(",", "")) if m else None


def card_values(page):
    """7 枚のカードの (値の文字列, ヒント, 内BPO) を左から順に。"""
    return page.evaluate(
        """()=>[...document.querySelectorAll('#cards1 .c')].map(c=>({
          lab:c.querySelector('.lab').textContent,
          v:c.querySelector('.v').textContent,
          hint:(c.querySelector('.hint')||{}).textContent||'',
          sub2:(c.querySelector('.sub2')||{}).textContent||''}))"""
    )


def open_page(pw_browser, port, hidden_ids=None):
    ctx = pw_browser.new_context(viewport={"width": 1280, "height": 1000})
    page = ctx.new_page()
    errs = []
    page.on("pageerror", lambda e: errs.append(str(e)))
    if hidden_ids is not None:
        page.add_init_script(
            "localStorage.setItem('salesKpi.hidden.v1',%s)" % json.dumps(json.dumps(hidden_ids))
        )
    page.goto(f"http://127.0.0.1:{port}/sales-kpi")
    page.wait_for_selector("#cards1 .c", timeout=20000)
    return ctx, page, errs


def drill_check(page, ci: int, label: str, fails: list, shot=None):
    """カード ci を開いて、件数の一致を全部確かめる。"""
    STATS["drill"] += 1
    cards = card_values(page)
    card = cards[ci]
    page.locator("#cards1 .c").nth(ci).click()
    page.wait_for_selector("#panel1:not(.hide) #panel1-title")
    title = page.locator("#panel1-title")
    total = int(title.get_attribute("data-total"))
    numr = title.get_attribute("data-num")
    key = CARDS[ci]
    where = f"[{label}] {card['lab']}"
    # カードの値 == 見出しの件数
    if key in ("rate", "anqrate"):
        m = re.search(r"([\d,]+)\s*÷\s*([\d,]+)", card["hint"])
        cn, cd = int(m.group(1).replace(",", "")), int(m.group(2).replace(",", ""))
        if (cn, cd) != (int(numr or -1), total):
            fails.append(f"{where}: カード 分子{cn}/分母{cd} ≠ パネル 分子{numr}/分母{total}")
        # 分母の内訳の和 == 分母
        segs = page.evaluate(
            "()=>[...document.querySelectorAll('#panel1 [data-seg]')].filter(b=>b.dataset.seg).map(b=>+b.dataset.n)"
        )
        if sum(segs) != total:
            fails.append(f"{where}: 分母の内訳 {segs} の和 ≠ {total}")
    else:
        if num(card["v"]) != total:
            fails.append(f"{where}: カード {card['v']} ≠ パネル見出し {total}")
        m = re.search(r"内 BPO\s*([\d,]+)件", card["sub2"] or "")
        card_bpo = int(m.group(1).replace(",", "")) if m else 0
        b = page.locator("#panel1-bpo")
        pb = int(b.get_attribute("data-n")) if b.count() else 0
        # ④ のカードは内 BPO を表示しない（パネルにだけ出る）。サーバ側のテストで件数を担保している
        if key != "den" and card_bpo != pb:
            fails.append(f"{where}: カード内BPO {card_bpo} ≠ パネル内BPO {pb}")
    if shot:
        page.screenshot(path=shot, full_page=False)
    # 一覧の行数の合計 == 件数（表を降りて数える）
    walked = walk(page, fails, where)
    STATS["rows"] += walked
    if walked != total:
        fails.append(f"{where}: 一覧の行数の合計 {walked} ≠ パネル見出し {total}")
    # 内 BPO を押した一覧の合計 == 内 BPO
    b = page.locator("#panel1-bpo")
    if b.count():
        n_b = int(b.get_attribute("data-n"))
        b.click()
        w2 = walk(page, fails, where + " BPOだけ")
        if w2 != n_b:
            fails.append(f"{where}: BPO だけの一覧の合計 {w2} ≠ 内BPO {n_b}")
        page.locator("#panel1-bpo").click()  # 戻す
    page.locator("#cards1 .c").nth(ci).click()  # 閉じる
    return total


def walk(page, fails, where) -> int:
    """パネルの表を チーム → 担当者 → 一覧 と降りて、一覧の行数を全部足す。"""
    total = 0

    def level():
        """いまの表の (行の名前, 合計セル) 。表が無ければ None。"""
        return page.evaluate(
            """()=>{const t=document.querySelector('#panel1 table.cdrill'); if(!t) return null;
              return {names:[...t.querySelectorAll('tbody tr')].map(r=>r.dataset.name),
                      sum:+t.querySelector('tfoot [data-sum]').dataset.sum};}"""
        )

    def list_n():
        h = page.locator("#panel1-listhead")
        return int(h.get_attribute("data-n")) if h.count() else None

    def click_row(name):
        page.locator("#panel1 table.cdrill tbody tr").filter(has=page.get_by_role("button", name=name, exact=True)).first.click()

    def items():
        return page.locator("#panel1 .list a.item").count()

    def back():
        page.get_by_role("button", name=re.compile("に戻る")).first.click()

    lv = level()
    if lv is None:  # 担当者を選んでいる: いきなり一覧
        n = list_n() or 0
        if items() != n:
            fails.append(f"{where}: 一覧の行 {items()} ≠ 見出し {n}")
        return n
    # 表（チーム or 担当者）
    def descend(lv, depth):
        nonlocal total
        names = lv["names"]
        s = 0
        for name in names:
            click_row(name)
            nxt = level()
            if nxt is None:
                n = list_n() or 0
                if items() != n:
                    fails.append(f"{where}: {name} の一覧の行 {items()} ≠ 見出し {n}")
                total += n
                s += n
                back()
            else:
                before = total
                descend(nxt, depth + 1)
                got = total - before
                s += got
                back()
        if s != lv["sum"]:
            fails.append(f"{where}: 表の合計 {lv['sum']} ≠ 下の一覧の合計 {s}")

    descend(lv, 0)
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--template", default="templates/tabs/sales_kpi.html")
    ap.add_argument("--cards-only", default=None)
    ap.add_argument("--port", type=int, default=9317)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    D = json.loads(Path(a.json).read_text(encoding="utf-8"))
    srv = serve(Path(a.template), Path(a.json), a.port)
    fails: list = []
    with sync_playwright() as pw:
        br = pw.chromium.launch(executable_path=CHROME, headless=True)
        teams = D["teams"]
        people = D["people"]
        if a.cards_only:
            res = {}
            ctx, page, errs = open_page(br, a.port)
            res["all"] = card_values(page)
            for t in teams:
                page.locator("#teams .chip", has_text=re.compile(f"^{re.escape(t)}$")).click()
                res["team:" + t] = card_values(page)
            if errs:
                res["errors"] = errs
            Path(a.cards_only).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
            br.close()
            srv.shutdown()
            return 0

        # --- 1. 全社 ---
        ctx, page, errs = open_page(br, a.port)
        for i, k in enumerate(CARDS):
            drill_check(page, i, "全社", fails, shot=str(out / f"all_{i + 1}_{k}.png"))
        # --- 2. チーム選択 ---
        for t in teams[:3]:
            page.locator("#teams .chip", has_text=re.compile(f"^{re.escape(t)}$")).click()
            for i in range(len(CARDS)):
                shot = str(out / f"team_{t}_{CARDS[i]}.png") if i in (4, 5) else None
                drill_check(page, i, f"チーム={t}", fails, shot=shot)
        # --- 3. 担当者選択（人数の多い順に 2 人） ---
        page.locator("#teams .chip", has_text="すべて").click()
        bp = D["by_person"]
        top = sorted(bp, key=lambda o: -bp[o].get("pool", 0))[:2]
        for o in top:
            page.select_option("#person", o)
            for i in range(len(CARDS)):
                shot = str(out / f"person_{o}_{CARDS[i]}.png") if i == 4 else None
                drill_check(page, i, f"担当者={o}", fails, shot=shot)
        ctx.close()
        # --- 4. チェック外し（先頭チームを丸ごと外す / 1 人外す） ---
        hide_team = [p["id"] for p in people if p["team"] == teams[0]]
        for label, ids in (("チェック外し(チーム全員)=" + teams[0], hide_team), ("チェック外し(1人)", hide_team[:1])):
            ctx, page, errs = open_page(br, a.port, hidden_ids=ids)
            for i in range(len(CARDS)):
                shot = str(out / f"hidden_{len(ids)}_{CARDS[i]}.png") if i == 4 and len(ids) > 1 else None
                drill_check(page, i, label, fails, shot=shot)
            ctx.close()
        # --- 5. UI でチェックを外す（localStorage 注入でなく画面操作で）---
        ctx, page, errs = open_page(br, a.port)
        page.click("#pickbtn")
        page.locator("#pickpanel .grp2 .gh input[type=checkbox]").first.click()
        for i in range(len(CARDS)):
            drill_check(page, i, "UIでチェック外し", fails)
        if errs:
            fails.append("pageerror: " + "; ".join(errs))
        br.close()
    srv.shutdown()
    if fails:
        print(f"NG {len(fails)} 件")
        for f in fails[:40]:
            print(" -", f)
        return 1
    print(f"OK: カードの値 == パネル見出し == 一覧の行数の合計（開いたパネル {STATS['drill']} 回、数えた一覧の行 {STATS['rows']} 行）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
