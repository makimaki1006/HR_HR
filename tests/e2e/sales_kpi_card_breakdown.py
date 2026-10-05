"""営業KPI「今月の成績」カード内訳の画面確認（fixture の JSON を簡易サーバで返す）。

本物の Sheets は読まない。`cargo run --example dump_sales_kpi -- out.json 2026-09-04` で書き出した
JSON を `/api/sales-kpi/data` として返し、テンプレートをそのまま `/sales-kpi` で返す。

使い方:
  python tests/e2e/sales_kpi_card_breakdown.py --json out.json --out <スクショ先> \
      [--template templates/tabs/sales_kpi.html] [--cards-only cards.json]

  --react <リポのルート> を付けると、旧画面ではなく React 版 (/app/sales-kpi、ビルド済みの static/app) を同じ手順で確かめる
  (id・data 属性が旧画面と同じなので、確認の中身は変えない)。

  --cards-only を付けると、内訳は触らず 7 枚のカードの数字だけを書き出す（変更前テンプレートとの比較用）。
確かめること（--cards-only なし）:
  - 7 枚それぞれを押してパネルが開き、パネル見出しの件数 == カードの値、
  - チーム表の合計 == 見出しの件数、担当者表の合計 == チームの件数、
    全担当者の取引一覧の行数の合計 == 見出しの件数、
  - 内 BPO のボタンの件数 == カードの「内 BPO n件」、押すと BPO の行だけになる、
  - ⑥ ⑤ の分子と分母の内訳、
  - 上を チーム選択・担当者選択・チェック外し の複数通りで繰り返す。

商談属性（2026-10-02）:
  - payload が deal_attr_available:false（既定 fixture）: 全カードのパネルに「商談属性: 未取得」、種別の表は出ない。
  - available:true（`dump_sales_kpi -- out.json 2026-09-04 --attr` で作った JSON。--only attr）:
    どの段（全社 → チーム → 担当者 → 取引一覧）でも 種別ごとの件数の合計 == その段の件数、
    種別を押して絞ると 一覧の行数の合計 == その種別の件数、⑥⑤ の分子・分母、表示は Rust が整えた値だけ（旧「商談種別」の内部値は「(定義外)」つきでしか出ない）。
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
STATS = {"drill": 0, "rows": 0, "nt_levels": 0}
PAGE_PATH = "/sales-kpi"
REACT_STATIC: Path | None = None  # --react 指定時: <repo>/static/app


def react_shell(static_app: Path) -> bytes:
    """Rust の spa_shell.rs と同じ形の HTML シェル (manifest からハッシュ付き JS / CSS を引く)。"""
    man = json.loads((static_app / ".vite" / "manifest.json").read_text(encoding="utf-8"))
    ent = man["src/entries/sales-kpi.tsx"]
    css, seen = [], set()

    def walk(e):
        css.extend(e.get("css", []))
        for k in e.get("imports", []):
            if k not in seen:
                seen.add(k)
                walk(man[k])

    walk(ent)
    links = "".join('<link rel="stylesheet" href="/static/app/%s">\n' % c for c in dict.fromkeys(css))
    head = (
        '<!DOCTYPE html>\n<html lang="ja">\n<head>\n<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1">\n<title>HR_HR</title>\n'
    )
    tail = '<script type="module" src="/static/app/%s"></script>\n</head>\n<body>\n<div id="app-root"></div>\n</body>\n</html>\n' % ent["file"]
    return (head + links + tail).encode("utf-8")


CARDS = ["apo", "pool", "den", "done", "rate", "anqrate", "cyomi"]  # 画面の左から順


def serve(tpl_path: Path, json_path: Path, port: int):
    page = tpl_path.read_bytes()
    data = json_path.read_bytes()

    class H(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.startswith("/api/sales-kpi/data"):
                body, ct = data, "application/json; charset=utf-8"
            elif REACT_STATIC is not None and self.path.startswith("/app/sales-kpi"):
                body, ct = react_shell(REACT_STATIC), "text/html; charset=utf-8"
            elif REACT_STATIC is not None and self.path.startswith("/static/app/"):
                f = REACT_STATIC / self.path[len("/static/app/"):].split("?")[0]
                if not f.is_file():
                    self.send_response(404)
                    self.end_headers()
                    return
                body = f.read_bytes()
                ct = {".js": "text/javascript", ".css": "text/css"}.get(f.suffix, "application/octet-stream")
            elif REACT_STATIC is None and self.path.startswith("/sales-kpi"):
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
    page.goto(f"http://127.0.0.1:{port}{PAGE_PATH}")
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
        if depth > 3:  # チーム → 担当者 → 一覧 の 2 段より深くなることは無い
            fails.append(f"{where}: 表の段が深すぎる（再帰を打ち切り）")
            return
        for name in names:
            click_row(name)
            nxt = level()
            if nxt is not None and nxt["names"] == lv["names"]:
                # 行を押したのに同じ表に留まった = 一覧が開かない
                fails.append(f"{where}: 「{name}」の行を押しても一覧が開かない（同じ表に留まった）")
                continue
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


def serve_json(tpl_path: Path, D: dict, port: int, out: Path, name: str):
    p = out / f"{name}.json"
    p.write_text(json.dumps(D, ensure_ascii=False), encoding="utf-8")
    return serve(tpl_path, p, port)


def blank_owner_input(D: dict) -> dict:
    """「担当なし」: チーム未設定の 1 人の id を空文字に書き換える（ownerId が空の取引）。"""
    D = json.loads(json.dumps(D))
    unset = [p for p in D["people"] if p["team"] == "チーム未設定"]
    old = max(unset, key=lambda p: D["by_person"].get(p["id"], {}).get("pool", 0))["id"]

    def rw(x):
        if isinstance(x, dict):
            return {("" if k == old else k): rw(v) for k, v in x.items()}
        if isinstance(x, list):
            return [rw(v) for v in x]
        return "" if x == old else x

    D = rw(D)
    for p in D["people"]:
        if p["id"] == "":
            p["name"], p["team"] = "担当なし", "チーム未設定"
    for r in D["card_deals"]["pool"] + D["card_deals"]["apo"] + D["card_deals"]["cyomi"]:
        if r["owner"] == "":
            r["ownerName"], r["team"] = "担当なし", "チーム未設定"
    return D


def scenario_blank_owner(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """担当者表の「担当なし」（ownerId が空）を押すと取引一覧が開き、行数が合う。"""
    D2 = blank_owner_input(D)
    n_blank = sum(1 for r in D2["card_deals"]["pool"] if r["owner"] == "")
    assert n_blank > 0, "fixture 書き換えに失敗（owner が空の行が無い）"
    srv = serve_json(tpl, D2, port, out, "blank_owner")
    ctx, page, errs = open_page(br, port)
    # ③ → チーム未設定 → 担当なし
    pool_i = CARDS.index("pool")
    page.locator("#cards1 .c").nth(pool_i).click()
    page.wait_for_selector("#panel1 table.cdrill")
    page.locator("#panel1 table.cdrill tbody tr").filter(
        has=page.get_by_role("button", name="チーム未設定", exact=True)).first.click()
    page.locator("#panel1 table.cdrill tbody tr").filter(
        has=page.get_by_role("button", name="担当なし", exact=True)).first.click()
    got = page.locator("#panel1-listhead")
    if got.count() == 0:
        fails.append("担当なし: 行を押しても取引一覧が開かない")
    else:
        n = int(got.get_attribute("data-n"))
        rows = page.locator("#panel1 .list a.item").count()
        if n != n_blank or rows != n_blank:
            fails.append(f"担当なし: 見出し {n} / 行 {rows} ≠ 期待 {n_blank}")
        page.screenshot(path=str(out / "blank_owner_list.png"))
    # 全カード・全表を降りても無限再帰せず合計が合う
    ctx.close()
    ctx, page, errs = open_page(br, port)
    for i in range(len(CARDS)):
        drill_check(page, i, "担当なし入力", fails)
    if errs:
        fails.append("pageerror(担当なし): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


LOWER = [("stale", "stale", "⑦"), ("anq", "anq_missing", "⑤未回収"), ("cyomi", "cyomi_stale", "⑨止まっている"),
         ("week", "week_deals", "今週"), ("next", "next_week_deals", "来週")]


def lower_vals(page):
    """「いま手を打てること」の 5 枚の値。"""
    return page.evaluate("()=>[...document.querySelectorAll('#cards2 .c .v')].map(v=>v.textContent)")


def kaden_vals(page):
    """架電リスト（アポ前の状況）の 4 枚の値。"""
    return page.evaluate("()=>[...document.querySelectorAll('#cards3b .c .v')].map(v=>v.textContent)")


def pick_blank(page):
    """個人プルダウンで「担当なし」を選ぶ（index で選ぶ。value は修正の前後で変わるため）。"""
    i = page.evaluate(
        "()=>[...document.querySelectorAll('#person option')].findIndex(o=>o.textContent.startsWith('担当なし'))")
    assert i > 0, "プルダウンに「担当なし」が無い"
    page.select_option("#person", index=i)


def ratio(hint: str):
    m = re.search(r"([\d,]+)\s*÷\s*([\d,]+)", hint)
    return (int(m.group(1).replace(",", "")), int(m.group(2).replace(",", ""))) if m else None


def scenario_pick_blank(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """個人プルダウンで「担当なし」を選ぶと、担当者が空の取引だけに絞り込まれる。
    未選択（全員）とは区別され、カード・内訳・下段の一覧・架電リスト・決定者の数字が揃う。"""
    D2 = blank_owner_input(D)
    # サーバの実際の形に合わせる: 架電リスト・架電・決定者の人別には担当なしを入れない（no_owner / 別扱い）
    D2["kaden"]["by_person"].pop("", None)
    for per in D2["calls"]["periods"].values():
        if isinstance(per, dict):
            (per.get("by_person") or {}).pop("", None)
    D2["kettei"]["rows"] = [r for r in D2["kettei"]["rows"] if r["owner"] != ""]
    bp = D2["by_person"][""]

    def g(k):
        return bp.get(k, 0)

    den = g("実施") + g("未実施") + g("未処理") + g("要判定")
    cd = D2["card_deals"]
    ids_pool = {r["id"] for r in cd["pool"] if r["owner"] == ""}
    ids_all = ids_pool | {r["id"] for k in ("apo", "cyomi") for r in cd[k] if r["owner"] == ""}
    assert g("pool") > 0 and ids_pool, "fixture に担当なしの商談が無い"
    srv = serve_json(tpl, D2, port, out, "pick_blank")
    ctx, page, errs = open_page(br, port)
    base_cards = [c["v"] for c in card_values(page)]
    base_lower, base_kaden = lower_vals(page), kaden_vals(page)
    scope_all = page.locator("#scope").text_content()

    pick_blank(page)
    # (a) 表示文
    sc = page.locator("#scope").text_content()
    if "担当なし" not in sc or "数字だけ" not in sc or "全チームの合計" in sc:
        fails.append(f"担当なし選択(a): 表示文が担当なし向けでない: {sc[:60]!r}")
    # (b) 7 枚のカード == by_person[""]
    cards = card_values(page)
    exp = {"apo": g("apo"), "pool": g("pool"), "den": den, "done": g("実施"), "cyomi": g("cyomi")}
    for i, k in enumerate(CARDS):
        c = cards[i]
        if k in exp:
            if num(c["v"]) != exp[k]:
                fails.append(f"担当なし選択(b): {c['lab']} カード {c['v']} ≠ by_person[''] の {exp[k]}")
        elif k == "rate":
            if ratio(c["hint"]) != (g("実施"), den):
                fails.append(f"担当なし選択(b): ⑥ 商談化率 {c['hint']!r} ≠ {g('実施')} ÷ {den}")
        else:  # anqrate
            if ratio(c["hint"]) != (g("anq_num"), g("anq_den")):
                fails.append(f"担当なし選択(b): ⑤ 回収率 {c['hint']!r} ≠ {g('anq_num')} ÷ {g('anq_den')}")
    page.screenshot(path=str(out / "pick_blank_selected.png"), full_page=False)
    # (c) 内訳パネル: 見出し == 一覧 == カード、行の owner はすべて空（id が担当なしの行の集合に収まる）
    for i, k in enumerate(CARDS):
        page.locator("#cards1 .c").nth(i).click()
        page.wait_for_selector("#panel1:not(.hide) #panel1-title")
        hrefs = page.locator("#panel1 .list a.item").evaluate_all("els=>els.map(e=>e.href)")
        got = {h.rstrip("/").split("/")[-1] for h in hrefs}
        if not got <= ids_all:
            fails.append(f"担当なし選択(c): {k} の内訳に担当なし以外の行が {len(got - ids_all)} 件ある")
        if k == "pool":
            if got != ids_pool:
                fails.append(f"担当なし選択(c): ③ の内訳 {len(got)} 行 ≠ 担当なしの商談 {len(ids_pool)} 行")
            page.screenshot(path=str(out / "pick_blank_panel_pool.png"), full_page=False)
        page.locator("#cards1 .c").nth(i).click()
        drill_check(page, i, "担当なし選択", fails)
    # (d) 下段の一覧
    for ci, (key, dk, mark) in enumerate(LOWER):
        want = sorted(r["id"] for r in D2[dk] if r["owner"] == "")
        card = page.locator("#cards2 .c").nth(ci)
        val = num(card.locator(".v").text_content())
        card.click()
        more = page.locator("#panel button.more", has_text="全部の日をまとめて見る")
        if more.count():  # 今週・来週は日ごとの表から入る。まとめて開く
            more.click()
        hrefs = page.locator("#panel .list a.item").evaluate_all("els=>els.map(e=>e.href)")
        ids = sorted(h.rstrip("/").split("/")[-1] for h in hrefs)
        if val != len(want) or ids != want:
            fails.append(f"担当なし選択(d): {mark} カード {val} / 一覧 {len(ids)} ≠ 担当なしの行 {len(want)}")
        card.click()
    # 架電リスト（no_owner）・決定者（no_owner の 1 行）・架電（数字は出さない）
    nk = D2["kaden"]["no_owner"]
    want_k = [nk.get("未架電", 0), nk.get("未接触", 0), nk.get("接触済み", 0)]
    got_k = [num(v) for v in kaden_vals(page)[:3]]
    if got_k != want_k:
        fails.append(f"担当なし選択(架電リスト): {got_k} ≠ kaden.no_owner {want_k}")
    # 決定者タブを開いてから見る（React 版はタブを開いたときだけ表を描く。旧画面は隠れたまま常に描いてある）
    page.locator("#tabs [role=tab]", has_text="決定者・決裁者").click()
    ke = page.evaluate("""()=>{const t=document.querySelector('#ketteibox table');
        return t?{rows:t.querySelectorAll('tbody tr').length}:null}""")
    page.locator("#tabs [role=tab]", has_text="営業KPI").click()
    if not ke or ke["rows"] != 1:
        fails.append(f"担当なし選択(決定者): 表が {ke} （担当なしの 1 行だけのはず）")
    first_call = page.evaluate("()=>{const v=document.querySelector('#cards3 .c .v');return v?v.textContent:null}")
    if first_call is not None and num(first_call) is not None:
        fails.append(f"担当なし選択(架電): 架電数が {first_call!r}（担当なしは人別に持っていないので数字は出さない）")
    # (h) 架電の欄: 見出しに「担当なし」、Zoom 架電の値は「—」、注記、「0 ÷ 0」なし
    zoom = page.evaluate("""()=>({
        h2:[...document.querySelectorAll('h2')].map(e=>e.textContent),
        lead:(document.getElementById('lead3')||{}).textContent||'',
        vals:[...document.querySelectorAll('#cards3 .c')].map(c=>({
            lab:((c.querySelector('.lab')||{}).textContent||''),
            v:((c.querySelector('.v')||{}).textContent||'').trim(),
            hint:((c.querySelector('.hint')||c).textContent||'')})),
        body:document.body.innerText})""")
    h2k = [h for h in zoom["h2"] if h.startswith("架電") and "リスト" not in h]
    if not (h2k and all("担当なし" in h for h in h2k)):
        fails.append(f"担当なし選択(h-a): 架電の見出しに「担当なし」が出ない: {h2k}")
    for w in ("担当なし", "電話をかけた人で数え", "出せません"):
        if w not in zoom["lead"]:
            fails.append(f"担当なし選択(h-b): Zoom 架電の注記に「{w}」が無い: {zoom['lead'][:80]!r}")
    for c in zoom["vals"]:
        if c["lab"].startswith("架電数") or c["lab"].startswith("つながった率"):
            if c["v"] not in ("—", "-", "—") and num(c["v"]) is not None:
                fails.append(f"担当なし選択(h-d): {c['lab']} の値が {c['v']!r}（「—」のはず）")
    # 「0 ÷ 0」は架電（Zoom）の欄だけ見る。成績カード ⑥⑤ の分母が 0 の担当なしでは正しい表示。
    zoom_txt = zoom["lead"] + " ".join(c["lab"] + c["v"] + c["hint"] for c in zoom["vals"])
    if "0 ÷ 0" in zoom_txt:
        fails.append("担当なし選択(h-c): 架電の欄に「0 ÷ 0」が出ている: " + zoom_txt[:200])
    conn_hint = [c["hint"] for c in zoom["vals"] if c["lab"].startswith("つながった率")]
    if not conn_hint or "÷" in conn_hint[0]:
        fails.append(f"担当なし選択(h-c): つながった率の説明が「—」でない: {conn_hint}")
    # (e) 未選択に戻すと全員の値に戻る
    page.select_option("#person", index=0)
    back_z = page.evaluate("""()=>[...document.querySelectorAll('h2')].map(e=>e.textContent).join('|')+'#'+
        (document.getElementById('lead3')||{}).textContent""")
    if "担当なし" in back_z or "出せません" in back_z:
        fails.append("担当なし選択(h-e): 未選択に戻しても架電の「担当なし」表示・注記が消えない")
    back = ([c["v"] for c in card_values(page)], lower_vals(page), kaden_vals(page))
    if back != (base_cards, base_lower, base_kaden):
        fails.append("担当なし選択(e): 未選択に戻しても全員の値に戻らない")
    if page.locator("#scope").text_content() != scope_all:
        fails.append("担当なし選択(e): 未選択に戻しても表示文が元に戻らない")
    # (f) チェックで担当なしを外す: 個人の選択は解除され、数字は担当なしぶんだけ減る
    pick_blank(page)
    page.click("#pickbtn")
    page.locator("#pickpanel .who2 label").filter(has_text=re.compile("^担当なし$")).locator("input").click()
    if page.locator("#person").evaluate("e=>e.selectedIndex") != 0:
        fails.append("担当なし選択(f): チェックで担当なしを外しても個人の選択が残っている")
    cur = [num(c["v"]) for c in card_values(page)][:2]
    want_cur = [num(base_cards[0]) - g("apo"), num(base_cards[1]) - g("pool")]
    if cur != want_cur:
        fails.append(f"担当なし選択(f): チェックで外したあとの ①③ {cur} ≠ {want_cur}")
    page.click("#pickpanel .close")  # 全部戻す
    if [c["v"] for c in card_values(page)] != base_cards:
        fails.append("担当なし選択(f): 全部戻しても元の値に戻らない")
    # (g) チーム切替で個人の選択は外れる
    pick_blank(page)
    page.locator("#teams .chip", has_text=re.compile("^チーム未設定$")).click()
    if page.locator("#person").evaluate("e=>e.selectedIndex") != 0:
        fails.append("担当なし選択(g): チーム切替をしても個人の選択が残っている")
    # チーム未設定の中で担当なしを選ぶ（プルダウンはチームで絞られる）
    pick_blank(page)
    if num(card_values(page)[1]["v"]) != g("pool"):
        fails.append("担当なし選択(g): チーム未設定の中で担当なしを選んだ ③ が by_person[''] と合わない")
    if errs:
        fails.append("pageerror(担当なし選択): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


def scenario_row_team(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """⑦⑤⑨ の行の team が名簿のチームと違っても、絞り込みはカード・内訳と同じ規則（名簿のチーム）。
    名簿に居ない担当者の行は、行の team で絞る（消えない）。"""
    D2 = json.loads(json.dumps(D))
    teams = D2["teams"]
    team_of = {p["id"]: p["team"] for p in D2["people"]}
    keys = {"stale": "⑦", "anq_missing": "⑤", "cyomi_stale": "⑨"}
    ghost_team = teams[1]
    for k in keys:
        rows = D2[k]
        r0 = next(r for r in rows if r["owner"] in team_of and team_of[r["owner"]] != "チーム未設定")
        other = next(t for t in teams if t not in (team_of[r0["owner"]], ghost_team, "チーム未設定"))
        r0["team"] = other  # 名簿のチームと違う値
        g = dict(r0)
        g.update(id="9" * 11 + k[0], owner="ZZ999", ownerName="名簿外", team=ghost_team,
                 url=f"https://app.hubspot.com/contacts/1/record/0-3/{'9' * 11 + k[0]}/")
        rows.append(g)
    srv = serve_json(tpl, D2, port, out, "row_team")
    ctx, page, errs = open_page(br, port)
    card_idx = {"stale": 0, "anq_missing": 1, "cyomi_stale": 2}
    for t in ["すべて"] + teams:
        if t != "すべて":
            page.locator("#teams .chip", has_text=re.compile(f"^{re.escape(t)}$")).click()
        for k, mark in keys.items():
            exp = [r for r in D2[k] if t == "すべて" or team_of.get(r["owner"], r["team"]) == t]
            card = page.locator("#cards2 .c").nth(card_idx[k])
            val = num(card.locator(".v").text_content())
            card.click()
            hrefs = page.locator("#panel .list a.item").evaluate_all("els=>els.map(e=>e.href)")
            ids = sorted(h.rstrip("/").split("/")[-1] for h in hrefs)
            want = sorted(r["id"] for r in exp)
            if val != len(want) or ids != want:
                fails.append(f"[チーム={t}] {mark}: カード {val} / 一覧 {len(ids)} ≠ 名簿のチームで絞った {len(want)}"
                             f"（差: {sorted(set(ids) ^ set(want))[:4]}）")
            if k == "stale" and t == ghost_team:
                page.screenshot(path=str(out / "row_team_stale_ghostteam.png"))
            card.click()  # 閉じる
    if errs:
        fails.append("pageerror(row_team): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


# ---------------------------------------------------------------- 商談属性
NT_ORDER = ["決裁者商談", "決定者商談", "担当者商談", "(未設定)"]
NT_RAW = ["代表者商談"]  # 旧「商談種別」の内部値。商談属性では定義外の値なので「(定義外)」の印なしでは出てはいけない


def nt_rank(label: str):
    return (NT_ORDER.index(label), "") if label in NT_ORDER else (len(NT_ORDER), label)


def nt_table(page):
    """パネルの商談属性の表。無ければ None。"""
    return page.evaluate(
        """()=>{const t=document.querySelector('#panel1 table.ntt'); if(!t) return null;
          const trs=[...t.querySelectorAll('tbody tr')];
          const f=t.querySelector('tfoot [data-sum]'), fn=t.querySelector('tfoot [data-sum-num]');
          return {level:t.dataset.level||'',
            rows:trs.map(r=>({nt:r.dataset.nt,n:+r.dataset.n,bpo:+r.dataset.bpo,
                              num:r.dataset.num===undefined?null:+r.dataset.num,
                              label:r.querySelector('button').textContent})),
            sum:+f.dataset.sum, sumNum:fn?+fn.dataset.sumNum:null};}"""
    )


def nt_level_check(page, where, expect_total, fails, expect_num=None):
    t = nt_table(page)
    if t is None:
        fails.append(f"{where}: 商談属性の表が無い")
        return None
    ns = [r["n"] for r in t["rows"]]
    if sum(ns) != t["sum"] or t["sum"] != expect_total:
        fails.append(f"{where}: 種別の件数 {ns} の和 {sum(ns)} / 合計行 {t['sum']} ≠ その段の件数 {expect_total}")
    labels = [r["nt"] for r in t["rows"]]
    if [r["label"] for r in t["rows"]] != labels:
        fails.append(f"{where}: 表示のラベル {[r['label'] for r in t['rows']]} ≠ 種別 {labels}")
    if labels != sorted(labels, key=nt_rank):
        fails.append(f"{where}: 種別の並びが固定順でない: {labels}")
    if t["sumNum"] is not None:
        nums = [r["num"] or 0 for r in t["rows"]]
        if sum(nums) != t["sumNum"]:
            fails.append(f"{where}: 分子の種別の和 {sum(nums)} ≠ 合計行の分子 {t['sumNum']}")
        if expect_num is not None and t["sumNum"] != expect_num:
            fails.append(f"{where}: 分子の合計 {t['sumNum']} ≠ 見出しの分子 {expect_num}")
        if any((r["num"] or 0) > r["n"] for r in t["rows"]):
            fails.append(f"{where}: 分子が分母より大きい種別がある: {t['rows']}")
    return t


def no_raw_values(page, where, fails):
    body = page.evaluate("()=>document.body.innerText")
    for w in NT_RAW:
        if re.search(re.escape(w) + r"(?!\(定義外\))", body):
            fails.append(f"{where}: 画面に旧内部値「{w}」が定義外の印なしで出ている")


def drill_rows(page):
    """いまの cdrill 表の (名前, 件数) 一覧。無ければ None。"""
    return page.evaluate(
        """()=>{const t=document.querySelector('#panel1 table.cdrill'); if(!t) return null;
          return [...t.querySelectorAll('tbody tr')].map(r=>[r.dataset.name,+r.children[1].textContent.replace(/,/g,'')]);}"""
    )


def level_count(page):
    """いまの段の件数: 表（チーム・担当者）なら行の和、一覧なら見出しの件数。"""
    dr = drill_rows(page)
    if dr is not None:
        return sum(c for _, c in dr)
    h = page.locator("#panel1-listhead")
    return int(h.get_attribute("data-n")) if h.count() else None


def nt_descend(page, where, expect_total, fails, depth=0, nt_filter=None, limit=None):
    """いまの段の種別の表を確かめ、cdrill の行を 1 つずつ降りて同じことを繰り返す。"""
    STATS["nt_levels"] += 1
    if nt_filter is None:
        nt_level_check(page, where, expect_total, fails)
    else:
        t = nt_table(page)
        row = next((r for r in (t or {"rows": []})["rows"] if r["nt"] == nt_filter), None)
        if row is None or row["n"] != expect_total:
            fails.append(f"{where}: 種別「{nt_filter}」で絞った段の表の件数 {row and row['n']} ≠ その段の件数 {expect_total}")
    if depth > 3:
        fails.append(f"{where}: 段が深すぎる")
        return
    rows = drill_rows(page)
    if rows is None:
        # 取引一覧の段: 見出しの件数 == 行数 == その段の件数
        h = page.locator("#panel1-listhead")
        n = int(h.get_attribute("data-n")) if h.count() else None
        items = page.locator("#panel1 .list a.item").count()
        if n != expect_total or items != expect_total:
            fails.append(f"{where}: 一覧の見出し {n} / 行 {items} ≠ その段の件数 {expect_total}")
        return
    if sum(c for _, c in rows) != expect_total:
        fails.append(f"{where}: 表の件数の和 {sum(c for _, c in rows)} ≠ その段の件数 {expect_total}")
    # limit: 各段で降りる行数の上限（先頭から。件数の多い順に並んでいる）。None なら全部
    for name, cnt in (rows if limit is None else rows[:limit]):
        page.locator("#panel1 table.cdrill tbody tr").filter(
            has=page.get_by_role("button", name=name, exact=True)).first.click()
        nt_descend(page, f"{where} > {name}", cnt, fails, depth + 1, nt_filter, limit)
        page.get_by_role("button", name=re.compile("に戻る")).first.click()


def nt_seg_check(page, where, fails, limit=2, ntn=2):
    """区分チップ（④⑥⑤）を 1 つずつ選び、その状態で種別の表を全段で確かめる。
    - 種別の合計 == 区分の件数（チップの件数）、各段（全社 → チーム → 担当者 → 一覧）でも同じ
    - 種別を押して絞ると、一覧の行数の合計 == その種別の件数（区分は選んだまま）
    - 内 BPO を併用: 種別の合計 == その区分の内 BPO
    """
    segs = page.evaluate(
        "()=>[...document.querySelectorAll('#panel1 button.chip[data-seg]')]"
        ".filter(b=>b.dataset.seg!=='').map(b=>[b.dataset.seg,+b.dataset.n])"
    )
    for seg, n in segs:
        if n == 0:
            continue
        STATS["nt_seg"] = STATS.get("nt_seg", 0) + 1
        w = f"{where} 区分「{seg}」"
        page.locator("#panel1 button.chip[data-seg]").filter(has_text=re.compile(f"^(分子 )?{re.escape(seg)} ")).first.click()
        t = nt_level_check(page, w + " 全体", n, fails)
        no_raw_values(page, w, fails)
        nt_descend(page, w, n, fails, limit=limit)
        if t:
            done = 0
            for r in t["rows"]:
                if r["n"] == 0:
                    continue
                done += 1
                if done > ntn:
                    break
                nm = r["nt"]
                page.locator("#panel1 table.ntt tbody tr").filter(
                    has=page.get_by_role("button", name=nm, exact=True)).first.click()
                t2 = nt_table(page)
                if t2 is None or t2["sum"] != n:
                    fails.append(f"{w} 絞り「{nm}」: 種別の表の合計 {t2 and t2['sum']} ≠ 区分の件数 {n}")
                lc = level_count(page)
                if lc != r["n"]:
                    fails.append(f"{w} 絞り「{nm}」: 絞った段の件数 {lc} ≠ 種別の件数 {r['n']}")
                nt_descend(page, f"{w} 絞り「{nm}」", r["n"], fails, nt_filter=nm, limit=limit)
                page.locator("#panel1-nt-chip").click()
        # 内 BPO を併用
        b = page.locator("#panel1-bpo")
        if b.count() and t:
            want_b = sum(r["bpo"] for r in t["rows"])
            b.click()
            tb = nt_table(page)
            if tb is None or tb["sum"] != want_b:
                fails.append(f"{w} BPOだけ: 種別の表の合計 {tb and tb['sum']} ≠ 区分の内BPO {want_b}")
            elif want_b > 0:  # 0 件なら一覧も表も出ない（「当てはまる取引はありません」）。種別の合計 0 は上で確認済み
                nt_descend(page, w + " BPOだけ", want_b, fails, limit=1)
            page.locator("#panel1-bpo").click()
    # 区分を外す（次の検査のために「すべて」へ戻す）
    allc = page.locator("#panel1 button.chip[data-seg='']")
    if allc.count():
        allc.first.click()


def nt_card_check(page, ci: int, label: str, fails: list, shot=None, filters=True, limit=2, full=False):
    """カード ci を開き、種別の表を全段で確かめる。filters なら種別ごとに絞って降りる。"""
    cards = card_values(page)
    where = f"[{label}] {cards[ci]['lab']}"
    page.locator("#cards1 .c").nth(ci).click()
    page.wait_for_selector("#panel1:not(.hide) #panel1-title")
    title = page.locator("#panel1-title")
    total = int(title.get_attribute("data-total"))
    numr = title.get_attribute("data-num")
    if total == 0:
        page.locator("#cards1 .c").nth(ci).click()
        return
    t = nt_level_check(page, where + " 全体", total, fails, expect_num=int(numr) if numr else None)
    no_raw_values(page, where, fails)
    if shot:
        page.screenshot(path=shot, full_page=False)
    nt_descend(page, where, total, fails, limit=None if full else limit)
    if t and filters:
        done = 0
        for r in t["rows"]:
            if r["n"] == 0:
                continue
            done += 1
            if filters is not True and done > filters:
                break
            nm = r["nt"]
            page.locator("#panel1 table.ntt tbody tr").filter(
                has=page.get_by_role("button", name=nm, exact=True)).first.click()
            chip = page.locator("#panel1-nt-chip")
            if chip.count() == 0 or chip.get_attribute("data-nt") != nm:
                fails.append(f"{where} 絞り「{nm}」: 絞りのバッジが出ない")
                continue
            # 絞っても見出し（カードの値）は変わらない。種別の表は全種別のまま
            if int(page.locator("#panel1-title").get_attribute("data-total")) != total:
                fails.append(f"{where} 絞り「{nm}」: 見出しの件数が変わった")
            t2 = nt_table(page)
            if t2 is None or t2["sum"] != total:
                fails.append(f"{where} 絞り「{nm}」: 種別の表の合計 {t2 and t2['sum']} ≠ {total}")
            # 絞った状態の表（チーム or 担当者）の合計 == 種別の件数
            lc = level_count(page)
            if lc != r["n"]:
                fails.append(f"{where} 絞り「{nm}」: 絞った段の件数 {lc} ≠ 種別の件数 {r['n']}")
            # 絞ったまま降りる: 各段で 種別の件数 == 親の表の件数、一覧の行数 == 件数
            nt_descend(page, f"{where} 絞り「{nm}」", r["n"], fails, nt_filter=nm, limit=limit)
            no_raw_values(page, f"{where} 絞り「{nm}」", fails)
            # 途中で外せる: バッジを押すと全体に戻り、一覧の合計は元の件数
            page.locator("#panel1-nt-chip").click()
            if page.locator("#panel1-nt-chip").count() != 0:
                fails.append(f"{where} 絞り「{nm}」: バッジを押しても絞りが外れない")
            lc = level_count(page)
            if lc != total:
                fails.append(f"{where} 絞り「{nm}」: 解除後の段の件数 {lc} が {total} に戻らない")
    # 区分（④⑥⑤）を選んだまま: 種別の合計 == その区分の件数、種別を押した先の一覧の行数 == その種別の件数
    if filters:
        nt_seg_check(page, where, fails, limit=limit, ntn=(2 if filters is True else filters))
    # 内 BPO だけ: 種別の表の合計 == 内 BPO
    b = page.locator("#panel1-bpo")
    if b.count():
        nb = int(b.get_attribute("data-n"))
        b.click()
        tb = nt_table(page)
        if tb is None or tb["sum"] != nb:
            fails.append(f"{where} BPOだけ: 種別の表の合計 {tb and tb['sum']} ≠ 内BPO {nb}")
        page.locator("#panel1-bpo").click()
    page.locator("#cards1 .c").nth(ci).click()  # 閉じる


def scenario_attr(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """種別の列がある payload（available:true）で、全段・全絞り込みの種別の合計を確かめる。"""
    assert D.get("deal_attr_available") is True, "--json は --attr 付きの dump で作ること"
    srv = serve_json(tpl, D, port, out, "attr")
    teams, people, bp = D["teams"], D["people"], D["by_person"]
    labs = {r["deal_attr"] for k in ("pool", "apo", "cyomi") for r in D["card_deals"][k]}
    for raw in NT_RAW:
        assert raw not in labs, f"サーバが旧内部値を定義外の印なしで返している: {raw}"
    ctx, page, errs = open_page(br, port)
    for i, k in enumerate(CARDS):
        shot = str(out / f"attr_all_{i + 1}_{k}.png") if k in ("pool", "rate") else None
        nt_card_check(page, i, "全社", fails, shot=shot, filters=(True if i in (1, 4) else 2), full=True)
        drill_check(page, i, "全社(種別あり)", fails)  # 既存の検査も種別ありの入力で通る
    for t in teams[:2]:
        page.locator("#teams .chip", has_text=re.compile(f"^{re.escape(t)}$")).click()
        for i in range(len(CARDS)):
            nt_card_check(page, i, f"チーム={t}", fails, filters=(3 if i in (1, 4) else 1))
    page.locator("#teams .chip", has_text="すべて").click()
    top = sorted(bp, key=lambda o: -bp[o].get("pool", 0))[:2]
    for o in top:
        page.select_option("#person", o)
        for i in range(len(CARDS)):
            nt_card_check(page, i, f"担当者={o}", fails, filters=(2 if i in (1, 4) else 1))
    # 絞った一覧のスクショ: ③ を 全社 → 種別「決裁者商談」→ 最初のチーム → 担当者
    page.select_option("#person", index=0)
    page.locator("#cards1 .c").nth(CARDS.index("pool")).click()
    page.locator("#panel1 table.ntt tbody tr").filter(
        has=page.get_by_role("button", name="決裁者商談", exact=True)).first.click()
    first = drill_rows(page)[0][0]
    page.locator("#panel1 table.cdrill tbody tr").filter(
        has=page.get_by_role("button", name=first, exact=True)).first.click()
    page.screenshot(path=str(out / "attr_filtered_team.png"), full_page=False)
    nxt = drill_rows(page)[0][0]
    page.locator("#panel1 table.cdrill tbody tr").filter(
        has=page.get_by_role("button", name=nxt, exact=True)).first.click()
    page.screenshot(path=str(out / "attr_filtered_list.png"), full_page=False)
    if errs:
        fails.append("pageerror(attr): " + "; ".join(errs))
    ctx.close()
    hide_team = [p["id"] for p in people if p["team"] == teams[0]]
    for label, ids in (("チェック外し(チーム全員)", hide_team), ("チェック外し(1人)", hide_team[:1])):
        ctx, page, errs = open_page(br, port, hidden_ids=ids)
        for i in range(len(CARDS)):
            nt_card_check(page, i, label, fails, filters=(2 if i in (1, 4) else 1))
        ctx.close()
    srv.shutdown()
    # 担当なし（ownerId が空）
    D2 = blank_owner_input(D)
    srv = serve_json(tpl, D2, port + 1, out, "attr_blank")
    ctx, page, errs = open_page(br, port + 1)
    for i in range(len(CARDS)):
        nt_card_check(page, i, "担当なし入力", fails, filters=(2 if i in (1, 4) else 1))
    pick_blank(page)
    for i in range(len(CARDS)):
        nt_card_check(page, i, "担当なし選択", fails, filters=1)
    if errs:
        fails.append("pageerror(attr 担当なし): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


def scenario_numnote(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """⑥⑤ で分子でない区分を選ぶと、種別表の見出し直下に「分子に当たらない」注釈が出る（列と値は残す）。
    分子の区分を選んだとき・区分未選択のときは出ない。"""
    assert D.get("deal_attr_available") is True, "--json は --attr 付きの dump で作ること"
    srv = serve_json(tpl, D, port, out, "numnote")
    ctx, page, errs = open_page(br, port)
    for key, num_seg in (("rate", "実施"), ("anqrate", "回収済み")):
        ci = CARDS.index(key)
        page.locator("#cards1 .c").nth(ci).click()
        page.wait_for_selector("#panel1:not(.hide) #panel1-title")
        segs = page.evaluate(
            "()=>[...document.querySelectorAll('#panel1 [data-seg]')].map(b=>b.dataset.seg).filter(x=>x)")
        assert num_seg in segs and len(segs) >= 2, f"{key}: 区分が足りない {segs}"

        def note():
            n = page.locator("#panel1-num-note")
            return n.text_content() if n.count() else None

        if note() is not None:
            fails.append(f"分子注釈 {key}: 区分未選択なのに注釈が出ている: {note()!r}")
        for sg in segs:
            page.locator(f"#panel1 [data-seg='{sg}']").click()
            got = note()
            if sg == num_seg:
                if got is not None:
                    fails.append(f"分子注釈 {key}/{sg}: 分子の区分なのに注釈が出ている: {got!r}")
                continue
            want = f"選んでいる区分（{sg}）は分子に当たらないため、分子の列は計算できません（0 と表示しています）"
            if got != want:
                fails.append(f"分子注釈 {key}/{sg}: 注釈 {got!r} ≠ {want!r}")
            # 列は隠さず、値は 0 のまま
            cells = page.evaluate(
                "()=>{const t=document.querySelector('#panel1 table.ntt');"
                "return t?{th:[...t.querySelectorAll('thead th')].map(x=>x.textContent),"
                "vals:[...t.querySelectorAll('tbody tr')].map(r=>r.children[2].textContent)}:null}")
            if not cells or not any(h.startswith("分子") for h in cells["th"]):
                fails.append(f"分子注釈 {key}/{sg}: 分子の列が消えている: {cells}")
            elif any(v.strip() != "0" for v in cells["vals"]):
                fails.append(f"分子注釈 {key}/{sg}: 分子の列が 0 でない: {cells['vals']}")
            if key == "rate" and sg == "未実施":
                page.screenshot(path=str(out / "numerator_note_rate_miJisshi.png"), full_page=False)
            page.locator(f"#panel1 [data-seg='{sg}']").click()  # 外す
            if note() is not None:
                fails.append(f"分子注釈 {key}/{sg}: 区分を外したのに注釈が残る: {note()!r}")
        page.locator("#cards1 .c").nth(ci).click()
    if errs:
        fails.append("pageerror(分子注釈): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


def scenario_nt_missing(br, tpl: Path, D: dict, out: Path, port: int, fails: list):
    """列が無いシート（available:false）では「商談属性: 未取得」を出し、種別の表は出さない。"""
    assert D.get("deal_attr_available") is False
    srv = serve_json(tpl, D, port, out, "ntmissing")
    ctx, page, errs = open_page(br, port)
    for i, k in enumerate(CARDS):
        page.locator("#cards1 .c").nth(i).click()
        page.wait_for_selector("#panel1:not(.hide) #panel1-title")
        m = page.locator("#panel1-nt-missing")
        txt = m.text_content() if m.count() else ""
        if "商談属性: 未取得" not in txt:
            fails.append(f"未取得: {k} のパネルに「商談属性: 未取得」が出ない: {txt!r}")
        if page.locator("#panel1 table.ntt").count() or page.locator("#panel1-nt-chip").count():
            fails.append(f"未取得: {k} のパネルに種別の表・絞りが出ている")
        if k == "pool":
            page.screenshot(path=str(out / "attr_missing_pool.png"), full_page=False)
        no_raw_values(page, f"未取得 {k}", fails)
        page.locator("#cards1 .c").nth(i).click()
    if errs:
        fails.append("pageerror(未取得): " + "; ".join(errs))
    ctx.close()
    srv.shutdown()


def run_guarded(fn, *args):
    """画面の要素が無くて Playwright が待ち切れたとき、落ちた理由を NG として出す（トレースバックで終わらせない）。"""
    fails = args[-1]
    try:
        fn(*args)
    except Exception as e:  # noqa: BLE001
        fails.append(f"{fn.__name__} が途中で止まった: {str(e).splitlines()[0][:160]} / {str(e).splitlines()[-1][:160]}")


def report_fails(fails: list, out: Path):
    """NG の件数・種類ごとの件数・先頭 40 件を出し、全件を <out>/fails.txt に書く。"""
    print(f"NG {len(fails)} 件")
    # 種類ごとの件数（先頭 40 件だけでは区分・内部値・並びのどれが落ちたか分からないため）
    kinds = {"区分を選んだ検査": "区分「", "内部値が画面に出た": "画面に内部値", "並びが固定順でない": "並びが固定順でない"}
    print("   内訳: " + " / ".join(f"{k} {sum(v in f for f in fails)}" for k, v in kinds.items()))
    for k, v in kinds.items():
        ex = next((f for f in fails if v in f), None)
        if ex:
            print(f" * {k}の例: {ex}")
    for f in fails[:40]:
        print(" -", f)
    (out / "fails.txt").write_text(chr(10).join(fails), encoding="utf-8")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--template", default="templates/tabs/sales_kpi.html")
    ap.add_argument("--cards-only", default=None)
    ap.add_argument("--react", default=None, help="リポのルート。指定すると React 版 (/app/sales-kpi) を確かめる")
    ap.add_argument("--port", type=int, default=9317)
    ap.add_argument("--only", choices=["main", "blank", "rowteam", "pickblank", "attr", "ntmissing", "numnote"], default=None,
                    help="指定したケースだけ走らせる（既定は全部）")
    a = ap.parse_args()
    global PAGE_PATH, REACT_STATIC
    if a.react:
        PAGE_PATH = "/app/sales-kpi"
        REACT_STATIC = Path(a.react) / "static" / "app"
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
            res["all:lower"], res["all:kaden"] = lower_vals(page), kaden_vals(page)
            for t in teams:
                page.locator("#teams .chip", has_text=re.compile(f"^{re.escape(t)}$")).click()
                res["team:" + t] = card_values(page)
                res["team:" + t + ":lower"], res["team:" + t + ":kaden"] = lower_vals(page), kaden_vals(page)
            if errs:
                res["errors"] = errs
            Path(a.cards_only).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
            br.close()
            srv.shutdown()
            return 0

        if a.only in (None, "blank"):
            scenario_blank_owner(br, Path(a.template), D, out, a.port + 1, fails)
        if a.only in (None, "rowteam"):
            scenario_row_team(br, Path(a.template), D, out, a.port + 2, fails)
        if a.only in (None, "pickblank"):
            scenario_pick_blank(br, Path(a.template), D, out, a.port + 3, fails)
        if a.only is None:
            if D.get("deal_attr_available") is True:
                run_guarded(scenario_attr, br, Path(a.template), D, out, a.port + 4, fails)
                run_guarded(scenario_numnote, br, Path(a.template), D, out, a.port + 8, fails)
            elif D.get("deal_attr_available") is False:
                run_guarded(scenario_nt_missing, br, Path(a.template), D, out, a.port + 6, fails)
        if a.only == "numnote":
            run_guarded(scenario_numnote, br, Path(a.template), D, out, a.port + 8, fails)
        if a.only == "attr":
            run_guarded(scenario_attr, br, Path(a.template), D, out, a.port + 4, fails)
        if a.only == "ntmissing":
            run_guarded(scenario_nt_missing, br, Path(a.template), D, out, a.port + 6, fails)
        if a.only in ("blank", "rowteam", "pickblank", "attr", "ntmissing", "numnote"):
            br.close()
            srv.shutdown()
            if fails:
                report_fails(fails, out)
            else:
                print("OK" + (f"（種別の表を確かめた段 {STATS['nt_levels']}、区分を選んだ検査 {STATS.get('nt_seg', 0)}）" if STATS["nt_levels"] else ""))
            return 1 if fails else 0
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
        report_fails(fails, out)
        return 1
    print(f"OK: カードの値 == パネル見出し == 一覧の行数の合計（開いたパネル {STATS['drill']} 回、数えた一覧の行 {STATS['rows']} 行、種別の表を確かめた段 {STATS['nt_levels']}、区分を選んだ検査 {STATS.get('nt_seg', 0)}）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
