# -*- coding: utf-8 -*-
"""UI再構築E2Eテスト（本番向け、チャートブランク調査含む）"""
import time, os, sys
from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
DIR = os.path.dirname(os.path.abspath(__file__))
PASS_COUNT = 0
FAIL_COUNT = 0
ISSUES = []

def ss(page, name, delay=1):
    time.sleep(delay)
    path = os.path.join(DIR, f"e2e_{name}.png")
    page.screenshot(path=path, full_page=False)
    print(f"    [screenshot] e2e_{name}.png")

def check(label, cond):
    global PASS_COUNT, FAIL_COUNT
    if cond:
        PASS_COUNT += 1
        print(f"  [OK] {label}")
    else:
        FAIL_COUNT += 1
        ISSUES.append(label)
        print(f"  [NG] {label}")
    return cond

def main():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        ctx = browser.new_context(viewport={"width": 1400, "height": 900})
        page = ctx.new_page()

        # === 1. ログイン ===
        print("\n=== 1. Login ===")
        page.goto(f"{BASE}/login", timeout=60000)
        page.wait_for_selector("#email", timeout=10000)
        page.locator("#email").fill("test@f-a-c.co.jp")
        page.locator("#password").fill("cyxen_2025")
        page.locator('button:has-text("ログイン")').click()
        page.wait_for_timeout(8000)
        check("Login success", "/login" not in page.url)

        # === 2. タブ構成確認 ===
        print("\n=== 2. Tab structure ===")
        tabs = page.query_selector_all(".tab-btn")
        tab_labels = [t.text_content().strip() for t in tabs]
        print(f"    Tabs: {tab_labels}")
        expected = ["市場概況", "地図", "詳細分析", "求人検索", "条件診断", "企業検索", "媒体分析"]
        check("7 tabs", len(tab_labels) == 7)
        check("Tab names match", tab_labels == expected)

        # === 3. 市場概況タブ ===
        print("\n=== 3. Market overview ===")
        page.wait_for_timeout(15000)  # 本番は全国クエリが遅い
        ss(page, "03_market_top")
        body = page.text_content("#content") or ""
        check("KPI displayed", "469" in body or "総求人数" in body)
        check("Section nav exists", page.query_selector('a[href="#sec-overview"]') is not None)

        # チャート確認（ECharts canvas）
        charts = page.query_selector_all('div[data-chart-config]')
        canvases = page.query_selector_all('canvas')
        print(f"    Chart configs: {len(charts)}, Canvases: {len(canvases)}")
        check("Charts rendered (canvas)", len(canvases) >= 1)

        # 各チャートのサイズを確認（ブランクチャート検出）
        blank_charts = []
        for i, canvas in enumerate(canvases):
            box = canvas.bounding_box()
            if box:
                w, h = box.get("width", 0), box.get("height", 0)
                if w < 10 or h < 10:
                    blank_charts.append(f"canvas[{i}]: {w}x{h}")
            else:
                blank_charts.append(f"canvas[{i}]: no bounding box")
        if blank_charts:
            print(f"    [WARN] Blank canvases: {blank_charts}")

        # 遅延ロードセクション確認（本番は全国クエリに最大30秒かかる）
        # hx-trigger="load"なので即座にリクエスト開始、完了を待つ
        page.wait_for_timeout(40000)
        page.evaluate("window.scrollTo(0, 3000)")
        page.wait_for_timeout(3000)
        ss(page, "03_market_mid")
        body2 = page.text_content("#content") or ""
        check("Workstyle section loaded", "福利厚生" in body2 or "社会保険" in body2 or "週休" in body2)

        page.evaluate("window.scrollTo(0, 6000)")
        page.wait_for_timeout(3000)
        ss(page, "03_market_bottom")
        body3 = page.text_content("#content") or ""
        check("Balance section loaded", "従業員" in body3 or "中央値" in body3)

        # === 4. 詳細分析タブ ===
        print("\n=== 4. Analysis tab ===")
        page.locator('.tab-btn:has-text("詳細分析")').click()
        page.wait_for_timeout(10000)
        ss(page, "04_analysis")
        body = page.text_content("#content") or ""
        check("Group nav exists", "構造分析" in body and "トレンド" in body and "総合診断" in body)
        check("Subtab content", "欠員" in body or "求人動向" in body)

        # グループ切替テスト
        trend_btn = page.query_selector('.analysis-group:has-text("トレンド")')
        if trend_btn:
            trend_btn.click()
            page.wait_for_timeout(8000)
            ss(page, "04_trend")
            body = page.text_content("#content") or ""
            check("Trend loaded", "時系列" in body or "推移" in body)

        insight_btn = page.query_selector('.analysis-group:has-text("総合診断")')
        if insight_btn:
            insight_btn.click()
            page.wait_for_timeout(8000)
            ss(page, "04_insight")
            body = page.text_content("#content") or ""
            check("Insight loaded", "採用構造" in body or "診断" in body)

        # === 5. 地図タブ ===
        print("\n=== 5. Map tab ===")
        page.locator('.tab-btn:has-text("地図")').click()
        page.wait_for_timeout(8000)
        ss(page, "05_map")
        check("Map tab loaded", page.query_selector("#content") is not None)

        # === 6. 求人検索タブ ===
        print("\n=== 6. Job search tab ===")
        page.locator('.tab-btn:has-text("求人検索")').click()
        page.wait_for_timeout(8000)
        ss(page, "06_search")
        body = page.text_content("#content") or ""
        check("Search tab loaded", len(body) > 50)

        # === 7. 条件診断タブ ===
        print("\n=== 7. Diagnostic tab ===")
        page.locator('.tab-btn:has-text("条件診断")').click()
        page.wait_for_timeout(6000)
        ss(page, "07_diagnostic")
        body = page.text_content("#content") or ""
        check("Diagnostic form", "月給" in body or "診断" in body)

        # === 8. 企業検索タブ ===
        print("\n=== 8. Company search tab ===")
        page.locator('.tab-btn:has-text("企業検索")').click()
        page.wait_for_timeout(6000)
        ss(page, "08_company")
        body = page.text_content("#content") or ""
        check("Company search tab", "企業" in body)

        # === 9. 媒体分析タブ ===
        print("\n=== 9. Survey tab ===")
        page.locator('.tab-btn:has-text("媒体分析")').click()
        page.wait_for_timeout(6000)
        ss(page, "09_survey")
        body = page.text_content("#content") or ""
        check("Survey title", "媒体分析" in body)
        check("CSV upload form", "CSV" in body or "ファイル" in body)

        # === 10. ヘルプボタン ===
        print("\n=== 10. Help button ===")
        help_btn = page.query_selector('button[title="使い方ガイド"]')
        check("Help button in header", help_btn is not None)

        # === 11. チャートブランク調査 ===
        print("\n=== 11. Chart blank investigation ===")
        # 市場概況タブに戻ってチャートを詳しく調査
        page.locator('.tab-btn:has-text("市場概況")').click()
        page.wait_for_timeout(15000)

        # EChartsのdata-chart-config属性を調査
        chart_divs = page.evaluate("""
            (function() {
                var divs = document.querySelectorAll('div[data-chart-config]');
                var results = [];
                divs.forEach(function(d, i) {
                    var cfg = d.getAttribute('data-chart-config');
                    var parsed = null;
                    try { parsed = JSON.parse(cfg); } catch(e) {}
                    var canvas = d.querySelector('canvas');
                    results.push({
                        index: i,
                        id: d.id || '(no id)',
                        hasCanvas: !!canvas,
                        canvasWidth: canvas ? canvas.width : 0,
                        canvasHeight: canvas ? canvas.height : 0,
                        configLength: cfg ? cfg.length : 0,
                        configValid: parsed !== null,
                        divWidth: d.offsetWidth,
                        divHeight: d.offsetHeight
                    });
                });
                return results;
            })()
        """)
        print(f"    Chart divs found: {len(chart_divs) if chart_divs else 0}")
        if chart_divs:
            for cd in chart_divs:
                status = "OK" if cd.get("hasCanvas") and cd.get("canvasWidth", 0) > 10 else "BLANK"
                print(f"    [{status}] #{cd['id']}: canvas={cd.get('canvasWidth',0)}x{cd.get('canvasHeight',0)}, div={cd.get('divWidth',0)}x{cd.get('divHeight',0)}, config={cd.get('configLength',0)}chars, valid={cd.get('configValid')}")

        # charts.js の初期化状況を確認
        chart_init = page.evaluate("""
            (function() {
                return {
                    echartsLoaded: typeof echarts !== 'undefined',
                    initChartsDefined: typeof initCharts === 'function',
                    chartInstances: typeof echarts !== 'undefined' ? Object.keys(echarts.__instances || {}).length : -1
                };
            })()
        """)
        print(f"    ECharts state: {chart_init}")

        ss(page, "11_chart_debug")

        # === Summary ===
        print("\n" + "=" * 50)
        print(f"E2E Results: {PASS_COUNT} passed, {FAIL_COUNT} failed")
        if ISSUES:
            print(f"Failed: {ISSUES}")
        print("=" * 50)

        browser.close()

    return 0 if FAIL_COUNT == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
