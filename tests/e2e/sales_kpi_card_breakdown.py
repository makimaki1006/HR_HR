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
    ke = page.evaluate("""()=>{const t=document.querySelector('#ketteibox table');
        return t?{rows:t.querySelectorAll('tbody tr').length}:null}""")
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
    if "0 ÷ 0" in zoom["body"]:
        fails.append("担当なし選択(h-c): ページ内に「0 ÷ 0」が出ている")
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--template", default="templates/tabs/sales_kpi.html")
    ap.add_argument("--cards-only", default=None)
    ap.add_argument("--port", type=int, default=9317)
    ap.add_argument("--only", choices=["main", "blank", "rowteam", "pickblank"], default=None,
                    help="指定したケースだけ走らせる（既定は全部）")
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
        if a.only in ("blank", "rowteam", "pickblank"):
            br.close()
            srv.shutdown()
            print(f"NG {len(fails)} 件" if fails else "OK")
            for f in fails[:40]:
                print(" -", f)
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
        print(f"NG {len(fails)} 件")
        for f in fails[:40]:
            print(" -", f)
        return 1
    print(f"OK: カードの値 == パネル見出し == 一覧の行数の合計（開いたパネル {STATS['drill']} 回、数えた一覧の行 {STATS['rows']} 行）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
