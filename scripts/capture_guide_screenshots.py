"""ガイドタブ用のスクリーンショットを本番から撮影"""
import sys, io, os, time
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')

from playwright.sync_api import sync_playwright

BASE = "https://hr-hw.onrender.com"
OUTDIR = "static/guide"
os.makedirs(OUTDIR, exist_ok=True)

def wait_trend_content(page, text, timeout=20000):
    try:
        page.wait_for_function(
            f"() => {{ const c = document.querySelector('#trend-content') || document.querySelector('#content'); return c && c.innerText.includes('{text}'); }}",
            timeout=timeout
        )
        page.wait_for_timeout(3000)
        return True
    except Exception:
        page.wait_for_timeout(3000)
        return False

def click_trend_tab(page):
    page.evaluate("""() => {
        document.querySelectorAll('nav button.tab-btn').forEach(b => {
            b.classList.remove('active');
            b.setAttribute('aria-selected', 'false');
        });
        const btn = Array.from(document.querySelectorAll('nav button.tab-btn')).find(b => b.textContent.includes('トレンド'));
        if (btn) { btn.classList.add('active'); btn.click(); }
    }""")
    page.wait_for_timeout(8000)

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 1400, "height": 900})

    # Login
    page.goto(f"{BASE}/login", wait_until="networkidle", timeout=60000)
    page.fill('input[name=email]', 'test@cyxen.co.jp')
    page.fill('input[name=password]', 'cyxen_2025')
    page.click('button[type=submit]')
    page.wait_for_timeout(8000)
    if "/login" in page.url:
        print("Login failed")
        browser.close()
        sys.exit(1)
    print("Login OK")

    # === Sub1: 量の変化 ===
    click_trend_tab(page)
    # スクロールして求人数推移チャートが見えるようにする
    page.evaluate("() => document.querySelector('#content').scrollTop = 0")
    page.wait_for_timeout(2000)
    page.screenshot(path=f"{OUTDIR}/trend_sub1.png")
    print("Sub1 captured")

    # === Sub2: 質の変化 ===
    page.locator('#content button.analysis-subtab', has_text="質の変化").click()
    wait_trend_content(page, "給与推移")
    page.screenshot(path=f"{OUTDIR}/trend_sub2.png")
    print("Sub2 captured")

    # === Sub3: 構造の変化 ===
    page.locator('#content button.analysis-subtab', has_text="構造の変化").click()
    wait_trend_content(page, "雇用形態")
    page.screenshot(path=f"{OUTDIR}/trend_sub3.png")
    print("Sub3 captured")

    # === Sub4: シグナル ===
    page.locator('#content button.analysis-subtab', has_text="シグナル").click()
    wait_trend_content(page, "ライフサイクル")
    page.screenshot(path=f"{OUTDIR}/trend_sub4.png")
    print("Sub4 captured")

    # === Sub5: 外部比較 ===
    page.locator('#content button.analysis-subtab', has_text="外部比較").click()
    wait_trend_content(page, "有効求人倍率")
    page.screenshot(path=f"{OUTDIR}/trend_sub5.png")
    print("Sub5 captured")

    # === 地域概況タブ（参考用） ===
    page.click('nav button.tab-btn:has-text("地域概況")')
    page.wait_for_timeout(8000)
    page.screenshot(path=f"{OUTDIR}/tab_overview.png")
    print("Overview captured")

    # === 東京都のトレンド ===
    page.locator("#pref-select").select_option(label="東京都")
    page.wait_for_timeout(3000)
    click_trend_tab(page)
    page.wait_for_timeout(5000)
    page.screenshot(path=f"{OUTDIR}/trend_tokyo.png")
    print("Tokyo trend captured")

    browser.close()
    print(f"\nAll screenshots saved to {OUTDIR}/")
    for f in sorted(os.listdir(OUTDIR)):
        size = os.path.getsize(os.path.join(OUTDIR, f))
        print(f"  {f}: {size/1024:.0f}KB")
