"""営業KPI: 旧画面 /sales-kpi と React 画面 /app/sales-kpi の表示テキストを突き合わせる。

同じ fixture (SALES_KPI_FIXTURE_DIR / SALES_KPI_FIXTURE_TODAY) で起動したサーバに対して、
両方の画面で同じ操作 (チーム・個人・架電の期間・先週比のモード・タブ) をし、
各領域 (id が旧新で同じ) の innerText を行単位で比べる。カードは「ラベル → 値」の組でも比べる。

CSP は有効のまま (bypass_csp は使わない)。コンソールエラー (favicon の 404 を除く) と pageerror が 1 件でもあれば終了コード 1。

使い方 (サーバは別に起動しておく):
    python tests/e2e/sales_kpi_old_new_compare.py --base http://localhost:9311 --shots <dir>
終了コード: 不一致 0 件なら 0、あれば 1。
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import sys

from playwright.sync_api import Page, sync_playwright

CHROME = "C:/Users/fuji1/AppData/Local/ms-playwright/chromium-1155/chrome-win/chrome.exe"

# 絞り込みに連動する領域 (営業KPI タブ)
KPI_REGIONS = [
    "range", "scope", "fixlinks", "bporule", "scope2", "cards1", "cards2",
    "lead3", "cards3", "kadenbar", "lead3b", "cards3b", "listbar",
    "lead4", "snapbox", "foot",
]
KETTEI_REGIONS = ["lead5", "ketteibox"]
STOCK_REGIONS = ["lead6", "stockbox", "lead7", "stockbybands"]

JS_REGION = """(ids) => {
  const out = {};
  for (const id of ids) {
    const el = document.getElementById(id);
    out[id] = el ? el.innerText : null;
  }
  return out;
}"""

# カード 1 枚 = .c 要素。ラベル (.lab) と値 (.v) を組にする。
JS_CARDS = """(ids) => {
  const out = {};
  for (const id of ids) {
    const el = document.getElementById(id);
    if (!el) { out[id] = null; continue; }
    out[id] = [...el.querySelectorAll('.c')].map(c => {
      const lab = c.querySelector('.lab');
      const v = c.querySelector('.v');
      return [lab ? lab.innerText.trim() : '', v ? v.innerText.trim() : ''];
    });
  }
  return out;
}"""

CARD_REGIONS = ["cards1", "cards2", "cards3", "cards3b"]


def login(page: Page, base: str) -> None:
    page.goto(base + "/login")
    page.fill("#email", "tester@f-a-c.co.jp")
    page.fill("#password", "testpass")
    page.click("form[action='/login'] button[type=submit]")
    page.wait_for_load_state("networkidle")


def open_screen(page: Page, url: str) -> None:
    page.goto(url)
    # wait_for_function は CSP (unsafe-eval 無し) の下では評価が止まるので、セレクタで待つ
    page.wait_for_selector("#cards1 .c", timeout=30000)
    page.wait_for_timeout(300)


def lines(text: str | None) -> list[str]:
    if text is None:
        return ["<領域なし>"]
    return [ln.strip() for ln in text.replace("\u00a0", " ").splitlines() if ln.strip()]


def chip_labels(page: Page, sel: str) -> list[str]:
    return page.eval_on_selector_all(sel, "els => els.map(e => e.innerText.trim())")


def click_chip(page: Page, sel: str, label: str) -> None:
    page.locator(sel).filter(has_text=re.compile("^" + re.escape(label) + "$")).first.click()
    page.wait_for_timeout(150)


def snapshot(page: Page, regions: list[str]) -> dict:
    return {
        "text": page.evaluate(JS_REGION, regions),
        "cards": page.evaluate(JS_CARDS, [r for r in CARD_REGIONS if r in regions]),
    }


def collect(page: Page, url: str, shots: str | None, tag: str) -> dict[str, dict]:
    """同じ操作列で各状態のスナップショットを取る。キーは状態名。"""
    open_screen(page, url)
    states: dict[str, dict] = {}
    if shots:
        page.screenshot(path=os.path.join(shots, f"{tag}_kpi_default.png"), full_page=True)

    teams = chip_labels(page, "#teams .chip")
    periods = chip_labels(page, "#callperiod .chip")
    modes = chip_labels(page, "#snapmode .chip")
    persons = page.eval_on_selector_all(
        "#person option", "os => os.map(o => o.value).filter(v => v)"
    )

    for t in teams:
        click_chip(page, "#teams .chip", t)
        states[f"team={t}"] = snapshot(page, KPI_REGIONS)
        for p in periods:
            click_chip(page, "#callperiod .chip", p)
            states[f"team={t}/period={p}"] = snapshot(page, ["lead3", "cards3", "kadenbar"])
        if periods:
            click_chip(page, "#callperiod .chip", periods[0])
        for m in modes:
            click_chip(page, "#snapmode .chip", m)
            states[f"team={t}/snap={m}"] = snapshot(page, ["lead4", "snapbox"])
        if modes:
            click_chip(page, "#snapmode .chip", modes[0])
    if teams:
        click_chip(page, "#teams .chip", teams[0])

    for pid in persons:
        page.select_option("#person", pid)
        page.wait_for_timeout(150)
        states[f"person={pid}"] = snapshot(page, KPI_REGIONS)
    if persons:
        page.select_option("#person", "")
        page.wait_for_timeout(150)

    tabs = chip_labels(page, "#tabs [role=tab]")
    for label, regions, key in (
        ("決定者・決裁者", KETTEI_REGIONS, "kettei"),
        ("リストの在庫", STOCK_REGIONS, "stock"),
    ):
        if label in tabs:
            click_chip(page, "#tabs [role=tab]", label)
            page.wait_for_timeout(200)
            states[f"tab={key}"] = snapshot(page, regions)
            if shots:
                page.screenshot(path=os.path.join(shots, f"{tag}_tab_{key}.png"), full_page=True)
    states["_meta"] = {"text": {"teams": "\n".join(teams), "periods": "\n".join(periods),
                                "modes": "\n".join(modes), "persons": "\n".join(persons),
                                "tabs": "\n".join(tabs)}, "cards": {}}
    return states


# 旧画面にだけある「カードを押すと内訳が開く」表示 (#45)。React 版は未実装 (claudedocs/SALES_KPI_REACT_GAP_2026-10-02.md の A)。
# --ignore-card-open を付けると #cards1 のこの 2 行だけを両画面から除いて比べる。実装したら外す。
CARD_OPEN_LINES = {"一覧を見る ▾", "閉じる ▲"}


def compare(old: dict[str, dict], new: dict[str, dict], ignore_card_open: bool = False) -> tuple[int, int, int, list[str]]:
    n_lines = n_cards = 0
    diffs: list[str] = []
    for state in sorted(set(old) | set(new)):
        if state not in old or state not in new:
            diffs.append(f"[{state}] 片方にしか無い状態 (旧={state in old}, 新={state in new})")
            continue
        o, n = old[state], new[state]
        for rid in sorted(set(o["text"]) | set(n["text"])):
            ol, nl = lines(o["text"].get(rid)), lines(n["text"].get(rid))
            if ignore_card_open and rid == "cards1":
                ol = [x for x in ol if x not in CARD_OPEN_LINES]
                nl = [x for x in nl if x not in CARD_OPEN_LINES]
            n_lines += max(len(ol), len(nl))
            if ol != nl:
                d = list(difflib.unified_diff(ol, nl, "旧", "新", lineterm="", n=0))
                diffs.append(f"[{state}] #{rid}\n  " + "\n  ".join(d[2:40]))
        for rid in sorted(set(o["cards"]) | set(n["cards"])):
            oc, nc = o["cards"].get(rid) or [], n["cards"].get(rid) or []
            n_cards += max(len(oc), len(nc))
            if oc != nc:
                diffs.append(f"[{state}] #{rid} カード\n  旧={oc}\n  新={nc}")
    return n_lines, n_cards, len(diffs), diffs


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default="http://localhost:9311")
    ap.add_argument("--shots", default=None)
    ap.add_argument("--ignore-card-open", action="store_true",
                    help="#cards1 の「一覧を見る ▾」「閉じる ▲」を比べない (React 版に内訳 #45 が入るまで)")
    ap.add_argument("--dump", default=None, help="抜いた値を JSON で書き出す先")
    a = ap.parse_args()
    if a.shots:
        os.makedirs(a.shots, exist_ok=True)
    with sync_playwright() as pw:
        br = pw.chromium.launch(executable_path=CHROME)
        ctx = br.new_context(viewport={"width": 1400, "height": 1000}, locale="ja-JP",
                             timezone_id="Asia/Tokyo")
        page = ctx.new_page()
        errors: list[str] = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.on("console", lambda m: errors.append("console: " + m.text + " @ " + m.location.get("url", ""))
                if m.type == "error" and not m.location.get("url", "").endswith("/favicon.ico") else None)
        login(page, a.base)
        old = collect(page, a.base + "/sales-kpi", a.shots, "old")
        new = collect(page, a.base + "/app/sales-kpi", a.shots, "new")
        br.close()
    if a.dump:
        with open(a.dump, "w", encoding="utf-8") as f:
            json.dump({"old": old, "new": new}, f, ensure_ascii=False, indent=1)
    n_lines, n_cards, n_diff, diffs = compare(old, new, a.ignore_card_open)
    print(f"状態数: 旧 {len(old) - 1} / 新 {len(new) - 1}")
    print(f"比較した行: {n_lines}  カード(ラベル+値): {n_cards}  不一致: {n_diff}")
    for d in diffs:
        print(d)
    if errors:
        print("ページのエラー:", *errors, sep="\n  ")
    return 0 if n_diff == 0 and not errors else 1


if __name__ == "__main__":
    sys.exit(main())
