# -*- coding: utf-8 -*-
"""
E2E カバレッジ拡張スクリプト
docs/E2E_COVERAGE_MATRIX.md で未カバーと特定された P0 項目のうち
以下 5 項目を検証する。

  1. ANA-004  給与パーセンタイル昇順検証
  2. CROSS-001 4タブ総件数一致
  3. CHART-05  canvas/svg 非空白ピクセル検証
  4. ERROR-04  全タブで TypeError / ReferenceError 監視
  5. CHART-12  テンプレート未置換変数検出

対象: https://hr-hw.onrender.com
認証: test@f-a-c.co.jp / cyxen_2025
既存 e2e スクリプトの ss / check / info パターンを踏襲する。
実行時間目安: 5 分以内。
"""
from __future__ import annotations

import os
import re
import sys
import time
import traceback
from typing import Optional

from playwright.sync_api import sync_playwright

# -----------------------------------------------------------------------------
# 設定
# -----------------------------------------------------------------------------
BASE = "https://hr-hw.onrender.com"
EMAIL = "test@f-a-c.co.jp"
PASSWORD = "cyxen_2025"
DIR = os.path.dirname(os.path.abspath(__file__))

# 既定の検索条件（詳細分析の給与分析で必須）
DEFAULT_PREF = "東京都"
DEFAULT_MUNI = "新宿区"

# 7タブ（name, hx-get）
TABS: list[tuple[str, str]] = [
    ("市場概況", "/tab/market"),
    ("地図", "/tab/map"),
    ("詳細分析", "/tab/analysis"),
    ("求人検索", "/tab/search"),
    ("条件診断", "/tab/diagnosis"),
    ("企業検索", "/tab/company"),
    ("媒体分析", "/tab/media"),
]

# 総件数比較対象（KPI に 総求人数 / 求人数 / 件 が出る4タブ）
COUNT_TABS: list[tuple[str, str]] = [
    ("市場概況", "/tab/market"),
    ("詳細分析", "/tab/analysis"),
    ("求人検索", "/tab/search"),
    ("企業検索", "/tab/company"),
]

RESULTS: list[dict] = []
START_TIME = 0.0


# -----------------------------------------------------------------------------
# ヘルパー（既存スクリプトのパターン踏襲）
# -----------------------------------------------------------------------------
def info(msg: str) -> None:
    print(f"  [INFO] {msg}")


def section(title: str) -> None:
    print(f"\n[{title}]")


def check(code: str, label: str, cond: bool, detail: str = "") -> bool:
    status = "PASS" if cond else "FAIL"
    icon = "OK" if cond else "NG"
    suffix = f" ({detail})" if detail else ""
    print(f"  [{icon}] [{status}] P0 {code} {label}{suffix}")
    RESULTS.append({
        "code": code, "label": label, "status": status, "detail": detail,
    })
    return cond


def ss(page, name: str) -> None:
    try:
        path = os.path.join(DIR, f"coverage_{name}.png")
        page.screenshot(path=path, full_page=False, timeout=10000)
    except Exception:
        pass


# -----------------------------------------------------------------------------
# タブ読み込み（NAV-XX と同じく .tab-btn クリック方式）
# -----------------------------------------------------------------------------
def click_tab(page, tab_name: str, wait_sec: int = 4) -> bool:
    try:
        clicked = page.evaluate(f"""
            (function(){{
                var btns = document.querySelectorAll('.tab-btn');
                for (var i = 0; i < btns.length; i++) {{
                    if ((btns[i].textContent || '').trim() === '{tab_name}') {{
                        btns[i].click();
                        return true;
                    }}
                }}
                return false;
            }})()
        """)
        if not clicked:
            return False
        # 市場概況は遅延ロードが多いので延長
        actual_wait = 10 if tab_name == "市場概況" else wait_sec
        time.sleep(actual_wait)
        return True
    except Exception:
        return False


def set_default_filters(page) -> None:
    """給与分析等で東京都/新宿区が必要なので先にセット"""
    try:
        page.evaluate(f"""
            (function(){{
                var pref = document.getElementById('pref-select');
                if (pref) {{
                    pref.value = '{DEFAULT_PREF}';
                    pref.dispatchEvent(new Event('change', {{bubbles: true}}));
                }}
            }})()
        """)
        time.sleep(2)
        page.evaluate(f"""
            (function(){{
                var muni = document.getElementById('muni-select');
                if (muni) {{
                    muni.value = '{DEFAULT_MUNI}';
                    muni.dispatchEvent(new Event('change', {{bubbles: true}}));
                }}
            }})()
        """)
        time.sleep(2)
    except Exception:
        pass


# -----------------------------------------------------------------------------
# 1. ANA-004  給与パーセンタイル昇順検証
# -----------------------------------------------------------------------------
def test_ana_004(page) -> None:
    section("ANA-004  給与パーセンタイル昇順")
    try:
        if not click_tab(page, "詳細分析", wait_sec=5):
            check("ANA-004", "詳細分析タブ読込", False, "タブ未検出")
            return

        # 給与分析サブタブを開く。実装に応じて複数経路を試す。
        # (a) data-subtab 属性の button
        # (b) テキストに「給与」を含む button
        # (c) 直接 htmx で /api/analysis/subtab/2
        opened = page.evaluate("""
            (function(){
                var btns = document.querySelectorAll(
                    '.subtab-btn, [data-subtab], button, a'
                );
                for (var i = 0; i < btns.length; i++) {
                    var t = (btns[i].textContent || '').trim();
                    if (t === '給与分析' || t === '給与') {
                        btns[i].click();
                        return 'clicked:' + t;
                    }
                }
                // HTMX 直接呼び出しフォールバック
                if (window.htmx) {
                    try {
                        window.htmx.ajax(
                            'GET', '/api/analysis/subtab/2',
                            {target: '#analysis-subtab-content, #content', swap: 'innerHTML'}
                        );
                        return 'htmx-ajax';
                    } catch(e) { return 'htmx-error:' + e.message; }
                }
                return 'not-found';
            })()
        """)
        info(f"salary subtab open: {opened}")
        time.sleep(5)

        # DOM から P25 / P50 / P75 / P90 の数値を抽出。
        # 実装で隣接セル or 親要素内 .stat-value / td など構造が揺れうるので広めに拾う。
        percentiles = page.evaluate("""
            (function(){
                var labels = {};
                var keys = ['P25','P50','P75','P90'];
                var all = document.querySelectorAll(
                    'td, th, div, span, .stat-label, .kpi-label, .percentile-label'
                );
                for (var i = 0; i < all.length; i++) {
                    var el = all[i];
                    var t = (el.textContent || '').trim();
                    // 完全一致または「P25 (下位25%)」形式にマッチ
                    var m = t.match(/^(P25|P50|P75|P90)(\\s|:|$|\\().*/);
                    if (!m) continue;
                    var key = m[1];
                    if (labels[key]) continue; // 最初の一致を採用

                    // 隣接セル / 兄弟 / 親の .value を探索
                    var candidates = [];
                    if (el.nextElementSibling) candidates.push(el.nextElementSibling);
                    var parent = el.parentElement;
                    if (parent) {
                        var valEls = parent.querySelectorAll(
                            '.value, .stat-value, .kpi-value, .percentile-value'
                        );
                        for (var j = 0; j < valEls.length; j++) candidates.push(valEls[j]);
                        // 親tr の全td
                        if (parent.tagName === 'TR' || (parent.parentElement && parent.parentElement.tagName === 'TR')) {
                            var tr = parent.tagName === 'TR' ? parent : parent.parentElement;
                            var tds = tr.querySelectorAll('td');
                            for (var k = 0; k < tds.length; k++) {
                                if (tds[k] !== el) candidates.push(tds[k]);
                            }
                        }
                    }
                    for (var c = 0; c < candidates.length; c++) {
                        var txt = (candidates[c].textContent || '').trim();
                        var numMatch = txt.match(/([0-9][0-9,\\.]*)/);
                        if (numMatch) {
                            var v = parseFloat(numMatch[1].replace(/,/g, ''));
                            if (!isNaN(v) && v > 0) {
                                labels[key] = v;
                                break;
                            }
                        }
                    }
                }
                return labels;
            })()
        """) or {}

        info(f"percentiles raw: {percentiles}")
        required = ["P25", "P50", "P75", "P90"]
        missing = [k for k in required if k not in percentiles]
        if missing:
            ss(page, "ana004_missing")
            check("ANA-004", "給与パーセンタイル昇順", False,
                  f"欠損: {missing}")
            return

        p25 = percentiles["P25"]
        p50 = percentiles["P50"]
        p75 = percentiles["P75"]
        p90 = percentiles["P90"]

        all_positive = p25 > 0 and p50 > 0 and p75 > 0 and p90 > 0
        ascending = p25 < p50 < p75 < p90

        ok = all_positive and ascending
        detail = f"P25={int(p25)} P50={int(p50)} P75={int(p75)} P90={int(p90)}"
        if not all_positive:
            detail = "0以下あり: " + detail
        elif not ascending:
            detail = "逆転検出: " + detail
        check("ANA-004", "給与パーセンタイル昇順 (P25<P50<P75<P90)",
              ok, detail)
    except Exception as e:
        traceback.print_exc()
        check("ANA-004", "給与パーセンタイル昇順", False,
              f"例外: {type(e).__name__}: {e}")


# -----------------------------------------------------------------------------
# 2. CROSS-001  4タブ総件数一致
# -----------------------------------------------------------------------------
def extract_counts(body: str) -> list[int]:
    """本文から 総求人数 / 求人数 / ○○件 近辺の5-7桁整数を抽出。"""
    candidates: list[int] = []
    # 「総求人数」「求人数」「件数」の周辺400字で整数を拾う
    keywords = ["総求人数", "求人数", "総件数", "件数", "Total", "total"]
    for kw in keywords:
        for m in re.finditer(re.escape(kw), body):
            start = max(0, m.start() - 50)
            end = min(len(body), m.end() + 200)
            snippet = body[start:end]
            for nm in re.finditer(r"([0-9]{1,3}(?:,[0-9]{3})+|[0-9]{5,7})", snippet):
                n = int(nm.group(1).replace(",", ""))
                if 100_000 <= n <= 1_000_000:
                    candidates.append(n)
    return candidates


def test_cross_001(page) -> None:
    section("CROSS-001  4タブ総件数一致")
    counts: dict[str, Optional[int]] = {}
    try:
        for tab_name, _ in COUNT_TABS:
            if not click_tab(page, tab_name, wait_sec=6):
                counts[tab_name] = None
                continue
            body = page.text_content("#content") or ""
            nums = extract_counts(body)
            # 最も頻出する候補を採用（なければ最大値）
            if nums:
                freq: dict[int, int] = {}
                for n in nums:
                    freq[n] = freq.get(n, 0) + 1
                # 頻度最大→同率なら最大値
                best = sorted(freq.items(), key=lambda x: (-x[1], -x[0]))[0][0]
                counts[tab_name] = best
            else:
                counts[tab_name] = None
            info(f"{tab_name} total={counts[tab_name]} (candidates={nums[:5]})")

        # 比較（None は除外）
        valid = {k: v for k, v in counts.items() if isinstance(v, int)}
        if len(valid) < 2:
            check("CROSS-001", "4タブ総件数一致", False,
                  f"有効値 {len(valid)} 件: {valid}")
            return

        max_v = max(valid.values())
        min_v = min(valid.values())
        # ±1% 許容
        tolerance = max_v * 0.01
        diff = max_v - min_v
        ok = diff <= tolerance
        detail = (f"counts={valid} max={max_v} min={min_v} diff={diff} "
                  f"tol={int(tolerance)}")
        check("CROSS-001", "4タブ総件数一致 (±1%)", ok, detail)
    except Exception as e:
        traceback.print_exc()
        check("CROSS-001", "4タブ総件数一致", False,
              f"例外: {type(e).__name__}: {e}")


# -----------------------------------------------------------------------------
# 3. CHART-05  canvas/svg 非空白ピクセル検証
# -----------------------------------------------------------------------------
def test_chart_05(page) -> None:
    section("CHART-05  canvas/svg 非空白検証")
    try:
        if not click_tab(page, "市場概況", wait_sec=12):
            check("CHART-05", "市場概況タブ読込", False, "タブ未検出")
            return

        result = page.evaluate("""
            (function(){
                var canvasResults = [];
                document.querySelectorAll('canvas').forEach(function(canvas){
                    if (canvas.width === 0 || canvas.height === 0) {
                        canvasResults.push({ok: false, reason: 'zero-size'});
                        return;
                    }
                    var ctx;
                    try { ctx = canvas.getContext('2d'); }
                    catch(e) { canvasResults.push({ok: false, reason: 'no-2d-ctx'}); return; }
                    if (!ctx) {
                        canvasResults.push({ok: false, reason: 'no-ctx'});
                        return;
                    }
                    try {
                        var data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
                        var nonEmpty = 0;
                        var totalPx = data.length / 4;
                        for (var i = 3; i < data.length; i += 4) {
                            if (data[i] > 0) nonEmpty++;
                        }
                        canvasResults.push({
                            ok: true,
                            ratio: nonEmpty / totalPx,
                            w: canvas.width,
                            h: canvas.height
                        });
                    } catch(e) {
                        canvasResults.push({ok: false, reason: 'read-error:' + e.message});
                    }
                });

                // SVG レンダラー用代替: ECharts コンテナ内 rect/path/circle 数
                var svgResults = [];
                document.querySelectorAll('svg').forEach(function(svg){
                    // 極小SVG(アイコン)は除外: 幅高さ 100px 以上
                    var bbox = svg.getBoundingClientRect();
                    if (bbox.width < 100 || bbox.height < 100) return;
                    var drawn = svg.querySelectorAll('rect, path, circle, polyline, line, polygon').length;
                    svgResults.push({
                        drawn: drawn,
                        w: Math.round(bbox.width),
                        h: Math.round(bbox.height)
                    });
                });

                return {canvases: canvasResults, svgs: svgResults};
            })()
        """) or {"canvases": [], "svgs": []}

        canvases = result.get("canvases", [])
        svgs = result.get("svgs", [])
        info(f"canvas={len(canvases)} svg(>=100px)={len(svgs)}")

        # canvas 基準: ratio > 1% のものが 1 つでもあれば OK
        canvas_ok_count = sum(
            1 for c in canvases
            if c.get("ok") and c.get("ratio", 0) > 0.01
        )
        # SVG 基準: drawn要素 10 以上のものが 1 つでもあれば OK
        svg_ok_count = sum(
            1 for s in svgs if s.get("drawn", 0) >= 10
        )

        total_charts = len(canvases) + len(svgs)
        ok_charts = canvas_ok_count + svg_ok_count
        ok = ok_charts >= 1

        if canvases:
            for c in canvases[:5]:
                if c.get("ok"):
                    info(f"  canvas {c['w']}x{c['h']} ratio={c['ratio']:.3%}")
                else:
                    info(f"  canvas NG reason={c.get('reason')}")
        if svgs:
            for s in svgs[:5]:
                info(f"  svg {s['w']}x{s['h']} drawn={s['drawn']}")

        detail = (f"canvas_ok={canvas_ok_count}/{len(canvases)} "
                  f"svg_ok={svg_ok_count}/{len(svgs)} "
                  f"total_drawn={ok_charts}/{total_charts}")
        check("CHART-05", "チャート実描画 (canvas 1%超 or svg 10要素超)",
              ok, detail)
        if not ok:
            ss(page, "chart05_blank")
    except Exception as e:
        traceback.print_exc()
        check("CHART-05", "チャート実描画", False,
              f"例外: {type(e).__name__}: {e}")


# -----------------------------------------------------------------------------
# 4. ERROR-04  全タブで TypeError / ReferenceError 監視
# -----------------------------------------------------------------------------
def test_error_04(page, error_sink: list[str]) -> None:
    section("ERROR-04  全タブ TypeError 監視")
    try:
        # フラグリセット用にマーカーを入れる
        marker = f"===ERROR04-START-{int(time.time())}==="
        error_sink.append(marker)

        tab_results = []
        for tab_name, _ in TABS:
            before = len(error_sink)
            ok_click = click_tab(page, tab_name, wait_sec=5)
            after = len(error_sink)
            new_errors = error_sink[before:after]
            tab_results.append((tab_name, ok_click, new_errors))

        # マーカー以降を全部対象に、TypeError / ReferenceError を抽出
        idx = error_sink.index(marker)
        all_new = error_sink[idx + 1:]
        critical = [
            e for e in all_new
            if "TypeError" in e or "ReferenceError" in e
        ]

        for tab_name, ok_click, errs in tab_results:
            tab_crit = [e for e in errs
                        if "TypeError" in e or "ReferenceError" in e]
            if tab_crit:
                info(f"  [{tab_name}] critical={len(tab_crit)}: "
                     f"{tab_crit[0][:120]}")
            elif not ok_click:
                info(f"  [{tab_name}] タブクリック失敗")

        ok = len(critical) == 0
        detail = f"{len(critical)} critical errors across {len(TABS)} tabs"
        if critical:
            detail += f" | first: {critical[0][:150]}"
        check("ERROR-04", "全タブ TypeError/ReferenceError 0件", ok, detail)
    except Exception as e:
        traceback.print_exc()
        check("ERROR-04", "全タブ TypeError 監視", False,
              f"例外: {type(e).__name__}: {e}")


# -----------------------------------------------------------------------------
# 5. CHART-12  テンプレート未置換変数検出
# -----------------------------------------------------------------------------
# {{ var }} / ${ var } / {{var}} を検出。
# false positive を避けるため、JSON の "{ }" は除外し、
# "{{ 識別子 }}" / "${ 識別子 }" の形だけを対象とする。
_UNRENDERED_RE = re.compile(
    r"\{\{\s*[A-Za-z_][A-Za-z0-9_\.]*\s*\}\}"
    r"|\$\{\s*[A-Za-z_][A-Za-z0-9_\.]*\s*\}"
)


def test_chart_12(page) -> None:
    section("CHART-12  テンプレート未置換変数検出")
    try:
        all_hits: dict[str, list[str]] = {}
        for tab_name, _ in TABS:
            if not click_tab(page, tab_name, wait_sec=4):
                continue
            # 表示されているテキストのみ対象（<script> などは除外）
            body = page.evaluate("""
                (function(){
                    var c = document.getElementById('content');
                    return c ? (c.innerText || c.textContent || '') : '';
                })()
            """) or ""
            hits = _UNRENDERED_RE.findall(body)
            # ユニーク化
            uniq = sorted(set(hits))
            if uniq:
                all_hits[tab_name] = uniq
                info(f"  [{tab_name}] 未置換 {len(uniq)}種: {uniq[:5]}")
            else:
                info(f"  [{tab_name}] OK")

        ok = len(all_hits) == 0
        if ok:
            detail = f"0 patterns across {len(TABS)} tabs"
        else:
            total = sum(len(v) for v in all_hits.values())
            detail = f"{total} patterns in {len(all_hits)} tabs: {all_hits}"
        check("CHART-12", "テンプレート未置換変数 0件", ok, detail)
    except Exception as e:
        traceback.print_exc()
        check("CHART-12", "テンプレート未置換検出", False,
              f"例外: {type(e).__name__}: {e}")


# -----------------------------------------------------------------------------
# main
# -----------------------------------------------------------------------------
def main() -> int:
    global START_TIME
    START_TIME = time.time()

    print("=" * 60)
    print("Coverage Extension: 5 P0 items")
    print(f"Target: {BASE}")
    print("=" * 60)

    error_sink: list[str] = []

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True, slow_mo=80)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # エラー監視フック（ERROR-04 で利用するが、全体通して記録する）
        page.on("console", lambda m: error_sink.append(
            f"[console.{m.type}] {m.text}"
        ) if m.type in ("error", "warning") else None)
        page.on("pageerror", lambda exc: error_sink.append(
            f"[pageerror] {exc}"
        ))

        # === ログイン ===
        print("\n[LOGIN]")
        try:
            page.goto(BASE, timeout=60000)
            time.sleep(3)
            page.fill('input[name="email"]', EMAIL, timeout=15000)
            page.fill('input[name="password"]', PASSWORD, timeout=15000)
            page.click('button[type="submit"]', timeout=15000)
            time.sleep(8)
            body = page.text_content("body") or ""
            login_ok = "ログアウト" in body or "都道府県" in body
            if not login_ok:
                print("[FATAL] ログイン失敗")
                ss(page, "login_fail")
                browser.close()
                return 2
            print("  [OK] ログイン成功")
        except Exception as e:
            print(f"[FATAL] ログイン例外: {e}")
            browser.close()
            return 2

        # === 既定フィルタ（給与分析で必要） ===
        set_default_filters(page)

        # === 5項目の検証 ===
        # ERROR-04 は他タブ巡回と二重計測しないよう先頭で実施（独立にタブ巡回する）
        for fn in [
            lambda: test_error_04(page, error_sink),
            lambda: test_ana_004(page),
            lambda: test_cross_001(page),
            lambda: test_chart_05(page),
            lambda: test_chart_12(page),
        ]:
            try:
                fn()
            except Exception as e:
                print(f"[ERROR] テスト関数例外: {e}")
                traceback.print_exc()

        browser.close()

    # === サマリー ===
    elapsed = time.time() - START_TIME
    total = len(RESULTS)
    passed = sum(1 for r in RESULTS if r["status"] == "PASS")
    failed = total - passed

    print("\n" + "=" * 60)
    print(f"Summary: {passed}/{total} PASS")
    minutes = int(elapsed // 60)
    seconds = int(elapsed % 60)
    print(f"所要時間: {minutes}分{seconds}秒")
    print("=" * 60)

    if failed > 0:
        print("\n[FAIL 詳細]")
        for r in RESULTS:
            if r["status"] == "FAIL":
                print(f"  - {r['code']} {r['label']} :: {r['detail']}")

    # P0 のみなので 1 件でも FAIL なら exit 2
    return 2 if failed > 0 else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        traceback.print_exc()
        sys.exit(3)
