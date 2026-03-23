"""本番E2Eテスト（データ検証付き）"""
import sys, io, json, os, time
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')

from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
SSDIR = "scripts/screenshots/prod_final"
os.makedirs(SSDIR, exist_ok=True)

results = []
def check(name, ok, detail=""):
    tag = "PASS" if ok else "FAIL"
    results.append((name, ok, detail))
    print(f"[{tag}] {name}" + (f" -- {detail}" if detail else ""))

def click_subtab(page, label):
    """サブタブをHTMX経由で切り替え"""
    subtab_map = {"量の変化": 1, "質の変化": 2, "構造の変化": 3, "シグナル": 4, "外部比較": 5}
    sub_id = subtab_map.get(label, 1)
    target = "#trend-content" if page.locator("#trend-content").count() > 0 else "#content"
    page.evaluate(f"""() => {{
        if (typeof htmx !== 'undefined') {{
            htmx.ajax('GET', '/api/trend/subtab/{sub_id}', {{target: '{target}', swap: 'innerHTML'}});
        }}
    }}""")
    page.wait_for_timeout(8000)

def wait_trend_content(page, text, timeout=20000):
    """#trend-content または #content に指定テキストが出現するまで待機"""
    try:
        page.wait_for_function(
            f"() => {{ const c = document.querySelector('#trend-content') || document.querySelector('#content'); return c && c.innerText.includes('{text}'); }}",
            timeout=timeout
        )
        page.wait_for_timeout(2000)
        return True
    except Exception:
        page.wait_for_timeout(3000)
        return False

def get_chart_data(page):
    """全EChartsチャートのデータポイント情報を取得"""
    return page.evaluate("""() => {
        const els = document.querySelectorAll('.echart[data-chart-config]');
        return Array.from(els).map(el => {
            try {
                const cfg = JSON.parse(el.getAttribute('data-chart-config'));
                const xAxis = cfg.xAxis;
                let xData = [];
                if (Array.isArray(xAxis)) { xData = xAxis[0]?.data || []; }
                else if (xAxis) { xData = xAxis.data || []; }
                const series = cfg.series || [];
                return {
                    title: cfg.title?.text || '',
                    xCount: xData.length,
                    seriesCount: series.length,
                    seriesInfo: series.map(s => ({
                        name: s.name,
                        dataPoints: (s.data || []).filter(v => v !== null).length,
                        total: (s.data || []).length
                    }))
                };
            } catch(e) { return {error: e.message}; }
        });
    }""")

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1400, "height": 900})

    # === Login ===
    page.goto(f"{BASE}/login", wait_until="networkidle", timeout=60000)
    page.fill('input[name=email]', 'test@cyxen.co.jp')
    page.fill('input[name=password]', 'cyxen_2025')
    page.click('button[type=submit]')
    page.wait_for_timeout(8000)
    logged_in = "/login" not in page.url
    check("Login", logged_in, page.url)
    if not logged_in:
        browser.close()
        sys.exit(1)

    # === E-1: トレンドタブ表示 ===
    # トレンドタブをHTMX経由で開く（htmx.ajax使用）
    page.evaluate("""() => {
        if (typeof htmx !== 'undefined') {
            htmx.ajax('GET', '/tab/trend', {target: '#content', swap: 'innerHTML'});
        }
        document.querySelectorAll('nav button.tab-btn').forEach(b => {
            b.classList.remove('active');
            b.setAttribute('aria-selected', 'false');
        });
        const trendBtn = Array.from(document.querySelectorAll('nav button.tab-btn')).find(b => b.textContent.includes('トレンド'));
        if (trendBtn) {
            trendBtn.classList.add('active');
            trendBtn.setAttribute('aria-selected', 'true');
        }
    }""")
    page.wait_for_timeout(10000)
    content_text = page.locator("#content").inner_text(timeout=10000)
    trend_loaded = "時系列トレンド分析" in content_text
    check("E-1 トレンドタブ表示", trend_loaded, "loaded" if trend_loaded else content_text[:80])
    page.screenshot(path=f"{SSDIR}/01_trend_sub1.png")

    # === E-2: 5サブタブ存在 ===
    labels = ["量の変化", "質の変化", "構造の変化", "シグナル", "外部比較"]
    found = [l for l in labels if page.locator("button.analysis-subtab", has_text=l).count() > 0]
    check("E-2 5サブタブ", len(found) == 5, str(found))

    # === E-3: Sub1 データ点数検証 ===
    charts = get_chart_data(page)
    for i, cd in enumerate(charts):
        if "error" in cd:
            check(f"E-3-{i+1} JSONパース", False, cd["error"])
            continue
        x = cd["xCount"]
        title = cd["title"]
        dp_info = []
        all_ok = x >= 2
        for si in cd.get("seriesInfo", []):
            dp = si["dataPoints"]
            dp_info.append(f"{si['name']}={dp}")
            if dp < 2:
                all_ok = False
        check(f"E-3-{i+1} {title}", all_ok, f"X軸={x}点, {', '.join(dp_info)}")
    page.screenshot(path=f"{SSDIR}/02_sub1_detail.png")

    # === E-4: Sub2 質の変化 ===
    click_subtab(page, "質の変化")
    t2 = page.locator("#content").inner_text()
    check("E-4a パート時給表記", "パート" in t2 and "時給" in t2)
    charts2 = get_chart_data(page)
    for i, cd in enumerate(charts2):
        if "error" in cd: continue
        x = cd["xCount"]
        title = cd["title"]
        dp = cd["seriesInfo"][0]["dataPoints"] if cd["seriesInfo"] else 0
        check(f"E-4b-{i+1} {title}", x >= 2 and dp >= 2, f"X={x}, DP={dp}")
    page.screenshot(path=f"{SSDIR}/03_sub2.png")

    # === E-5: Sub3 構造の変化 ===
    click_subtab(page, "構造の変化")
    charts3 = get_chart_data(page)
    for i, cd in enumerate(charts3):
        if "error" in cd: continue
        x = cd["xCount"]
        title = cd["title"]
        dp = cd["seriesInfo"][0]["dataPoints"] if cd["seriesInfo"] else 0
        check(f"E-5-{i+1} {title}", x >= 2 and dp >= 2, f"X={x}, DP={dp}")
    page.screenshot(path=f"{SSDIR}/04_sub3.png")

    # === E-6: Sub4 シグナル ===
    click_subtab(page, "シグナル")
    charts4 = get_chart_data(page)
    for i, cd in enumerate(charts4):
        if "error" in cd: continue
        x = cd["xCount"]
        title = cd["title"]
        dp = cd["seriesInfo"][0]["dataPoints"] if cd["seriesInfo"] else 0
        check(f"E-6-{i+1} {title}", x >= 2 and dp >= 2, f"X={x}, DP={dp}")
    page.screenshot(path=f"{SSDIR}/05_sub4.png")

    # === E-7: Sub5 外部比較 ===
    click_subtab(page, "外部比較")
    t5 = page.locator("#content").inner_text()
    check("E-7a 有効求人倍率", "有効求人倍率" in t5)
    check("E-7b 賃金比較", "賃金比較" in t5)
    check("E-7c 離職率比較", "離職率" in t5)
    charts5 = get_chart_data(page)
    for i, cd in enumerate(charts5):
        if "error" in cd: continue
        x = cd["xCount"]
        title = cd["title"]
        dp = cd["seriesInfo"][0]["dataPoints"] if cd["seriesInfo"] else 0
        check(f"E-7d-{i+1} {title}", x >= 2 and dp >= 2, f"X={x}, DP={dp}")
    page.screenshot(path=f"{SSDIR}/06_sub5.png")

    # === E-8: 都道府県フィルタ ===
    page.locator("#pref-select").select_option(label="東京都")
    page.wait_for_timeout(3000)
    page.click('nav button.tab-btn:has-text("トレンド")')
    wait_trend_content(page, "時系列トレンド分析", timeout=30000)
    tt = page.locator("#content").inner_text()
    check("E-8a 東京都ラベル", "東京" in tt)
    tokyo_charts = get_chart_data(page)
    if tokyo_charts:
        cd = tokyo_charts[0]
        x = cd.get("xCount", 0)
        dp = cd["seriesInfo"][0]["dataPoints"] if cd.get("seriesInfo") else 0
        check("E-8b 東京データ点数", x >= 2 and dp >= 2, f"X={x}, DP={dp}")
    page.screenshot(path=f"{SSDIR}/07_tokyo.png")

    # === E-9: ガイドタブ ===
    page.locator("#pref-select").select_option(value="")
    page.wait_for_timeout(2000)
    page.click('nav button.tab-btn:has-text("ガイド")')
    page.wait_for_timeout(8000)
    page.screenshot(path=f"{SSDIR}/08_guide_top.png")

    # detailsを全て展開してからテキスト取得
    page.evaluate("() => document.querySelectorAll('#content details').forEach(d => d.open = true)")
    page.wait_for_timeout(1000)
    g = page.locator("#content").inner_text()
    check("E-9a 全9タブ", "全9タブ" in g)
    check("E-9b Tab9トレンド", "Tab 9" in g and "トレンド" in g)
    check("E-9c 外部比較説明", "外部比較" in g)
    check("E-9d パート時給記述", "パート時給" in g)
    check("E-9e FAQ市区町村", "市区町村" in g and "トレンド" in g)
    check("E-9f FAQ外部データ粒度", "外部データ" in g and "粒度" in g)
    check("E-9g ユースケース外部比較", "外部比較" in g and "求人倍率" in g)
    page.screenshot(path=f"{SSDIR}/09_guide_full.png", full_page=True)

    browser.close()

# === Summary ===
passed = sum(1 for _, ok, _ in results if ok)
failed = sum(1 for _, ok, _ in results if not ok)
print(f"\n{'='*60}")
print(f"Summary: {passed}/{passed+failed} passed, {failed} failed")
print(f"Screenshots: {SSDIR}/")
print(f"{'='*60}")
if failed > 0:
    print("\n失敗:")
    for name, ok, detail in results:
        if not ok:
            print(f"  {name}: {detail}")
    sys.exit(1)
