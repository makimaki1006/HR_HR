"""
トレンドタブ E2Eテスト (Playwright)

対象: ハローワーク求人分析ダッシュボード (localhost:9216)
テスト: E-1 ~ E-7
"""

import os
import sys
import time
from pathlib import Path

try:
    from playwright.sync_api import sync_playwright, TimeoutError as PwTimeout
except ImportError:
    print("[ERROR] playwright がインストールされていません")
    print("  pip install playwright && playwright install chromium")
    sys.exit(1)

# --- 設定 ---
BASE_URL = "http://localhost:9216"
EMAIL = "test@f-a-c.co.jp"
PASSWORD = "test123"
TIMEOUT = 10_000  # 10秒
SCREENSHOT_DIR = Path(__file__).resolve().parent / "screenshots"
SCREENSHOT_DIR.mkdir(exist_ok=True)


def screenshot(page, name: str) -> str:
    """スクリーンショットを保存してパスを返す"""
    path = SCREENSHOT_DIR / f"{name}.png"
    page.screenshot(path=str(path), full_page=False)
    return str(path)


def wait_for_htmx(page, timeout_ms: int = 5000):
    """HTMXリクエスト完了を待つ"""
    page.wait_for_timeout(500)
    try:
        page.wait_for_function(
            """() => {
                // htmx がリクエスト中でなくなるまで待つ
                if (typeof htmx !== 'undefined' && htmx.find) {
                    return document.querySelectorAll('.htmx-request').length === 0;
                }
                return true;
            }""",
            timeout=timeout_ms
        )
    except Exception:
        pass
    page.wait_for_timeout(1500)


def get_trend_content_text(page) -> str:
    """トレンドコンテンツのテキストを安全に取得する。
    #trend-content が見つからない場合は #content 全体を使用する。
    """
    tc = page.locator("#trend-content")
    if tc.count() > 0:
        try:
            return tc.first.inner_text(timeout=3000)
        except Exception:
            pass
    # フォールバック: #content 全体
    return page.locator("#content").inner_text(timeout=5000)


def run_tests():
    passed = 0
    failed = 0
    results = []

    def record(test_id: str, description: str, success: bool, detail: str = ""):
        nonlocal passed, failed
        if success:
            passed += 1
            tag = "[PASS]"
        else:
            failed += 1
            tag = "[FAIL]"
        msg = f"{tag} {test_id}: {description}"
        if detail:
            msg += f" -- {detail}"
        print(msg)
        results.append((test_id, description, success, detail))

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        context = browser.new_context(viewport={"width": 1280, "height": 900})
        page = context.new_page()
        page.set_default_timeout(TIMEOUT)

        # =============================================================
        # E-1: ログインしてダッシュボードに遷移
        # =============================================================
        try:
            page.goto(f"{BASE_URL}/login", wait_until="domcontentloaded")
            page.fill("#email", EMAIL)
            page.fill("#password", PASSWORD)
            page.click('button[type="submit"]')
            # ログイン後のリダイレクトを待つ
            page.wait_for_url("**/", timeout=TIMEOUT)
            # ダッシュボードのコンテンツ読み込みを待つ
            page.wait_for_selector("#content", timeout=TIMEOUT)
            wait_for_htmx(page)
            current_url = page.url.rstrip("/")
            base_stripped = BASE_URL.rstrip("/")
            is_dashboard = (current_url == base_stripped) or current_url.endswith("/")
            record("E-1", "ログイン -> ダッシュボード遷移", is_dashboard,
                   f"URL={page.url}")
            screenshot(page, "e1_dashboard_after_login")
        except Exception as e:
            record("E-1", "ログイン -> ダッシュボード遷移", False, str(e))
            # ログイン失敗時は後続テストをスキップ
            browser.close()
            print(f"\nSummary: {passed}/{passed + failed} passed")
            return 1

        # =============================================================
        # E-2: トレンドタブボタンの存在確認
        # =============================================================
        try:
            trend_btn = page.locator('nav button.tab-btn', has_text="トレンド")
            trend_btn.wait_for(state="visible", timeout=TIMEOUT)
            btn_count = trend_btn.count()
            record("E-2", "トレンドタブボタンが存在する", btn_count >= 1,
                   f"ボタン数={btn_count}")
            screenshot(page, "e2_trend_button_exists")
        except Exception as e:
            record("E-2", "トレンドタブボタンが存在する", False, str(e))

        # =============================================================
        # E-3: トレンドタブをクリックしてコンテンツ確認
        # =============================================================
        try:
            trend_btn = page.locator('nav button.tab-btn', has_text="トレンド")
            trend_btn.click()
            wait_for_htmx(page)
            content = page.locator("#content")
            content_text = content.inner_text()

            has_title = "時系列トレンド分析" in content_text
            # サブタブボタンの存在確認
            subtab_labels = ["量の変化", "質の変化", "構造の変化", "シグナル"]
            found_subtabs = []
            for label in subtab_labels:
                btn = page.locator('#content button.analysis-subtab', has_text=label)
                if btn.count() > 0:
                    found_subtabs.append(label)

            all_subtabs = len(found_subtabs) == 4
            record("E-3", "トレンドタブ表示: タイトル+4サブタブ",
                   has_title and all_subtabs,
                   f"タイトル={'OK' if has_title else 'NG'}, "
                   f"サブタブ={found_subtabs}")
            screenshot(page, "e3_trend_tab_content")
        except Exception as e:
            record("E-3", "トレンドタブ表示: タイトル+4サブタブ", False, str(e))

        # =============================================================
        # E-4: サブタブ切り替え
        # =============================================================
        try:
            checks = []

            # 質の変化
            btn_quality = page.locator('#content button.analysis-subtab',
                                       has_text="質の変化")
            btn_quality.click()
            wait_for_htmx(page, timeout_ms=8000)
            trend_text = get_trend_content_text(page)
            has_salary = "給与推移" in trend_text
            checks.append(("質の変化 -> 給与推移", has_salary))

            # 構造の変化
            btn_structure = page.locator('#content button.analysis-subtab',
                                         has_text="構造の変化")
            btn_structure.click()
            wait_for_htmx(page, timeout_ms=8000)
            trend_text = get_trend_content_text(page)
            has_employment = "雇用形態" in trend_text
            checks.append(("構造の変化 -> 雇用形態", has_employment))

            # シグナル
            btn_signal = page.locator('#content button.analysis-subtab',
                                      has_text="シグナル")
            btn_signal.click()
            wait_for_htmx(page, timeout_ms=8000)
            trend_text = get_trend_content_text(page)
            has_lifecycle = "ライフサイクル" in trend_text
            checks.append(("シグナル -> ライフサイクル", has_lifecycle))

            all_ok = all(c[1] for c in checks)
            detail_parts = [f"{name}={'OK' if ok else 'NG'}" for name, ok in checks]
            record("E-4", "サブタブ切り替え (質/構造/シグナル)",
                   all_ok, ", ".join(detail_parts))
            screenshot(page, "e4_subtab_switching")
        except Exception as e:
            record("E-4", "サブタブ切り替え (質/構造/シグナル)", False, str(e))

        # =============================================================
        # E-5: ECharts レンダリング確認
        # =============================================================
        try:
            # 量の変化サブタブに戻してチャートを確認
            btn_volume = page.locator('#content button.analysis-subtab',
                                      has_text="量の変化")
            btn_volume.click()
            wait_for_htmx(page, timeout_ms=8000)
            # ECharts 初期化を待つ
            page.wait_for_timeout(2000)

            # .echart 要素の存在（#content 全体で検索）
            echart_elements = page.locator("#content .echart")
            echart_count = echart_elements.count()

            # data-chart-config 属性の確認
            has_config = False
            if echart_count > 0:
                first_config = echart_elements.first.get_attribute("data-chart-config")
                has_config = first_config is not None and len(first_config) > 10

            # ECharts canvas レンダリング確認 (JS実行)
            canvas_count = page.evaluate(
                "document.querySelectorAll('#content .echart canvas').length"
            )

            record("E-5", "ECharts レンダリング",
                   echart_count > 0 and has_config,
                   f"echart要素={echart_count}, config={'有' if has_config else '無'}, "
                   f"canvas={canvas_count}")
            screenshot(page, "e5_echarts_rendering")
        except Exception as e:
            record("E-5", "ECharts レンダリング", False, str(e))

        # =============================================================
        # E-6: ガイドタブ確認
        # =============================================================
        try:
            guide_btn = page.locator('nav button.tab-btn', has_text="ガイド")
            guide_btn.click()
            wait_for_htmx(page)

            content_text = page.locator("#content").inner_text()
            has_9tabs = "全9タブ" in content_text

            # 「タブ別ガイド」の details/summary を開く
            tab_guide_summary = page.locator('#content summary',
                                             has_text="タブ別ガイド")
            if tab_guide_summary.count() > 0:
                tab_guide_summary.first.click()
                page.wait_for_timeout(500)

            content_text_after = page.locator("#content").inner_text()
            has_tab9 = "Tab 9" in content_text_after or "トレンド" in content_text_after

            record("E-6", "ガイドタブ: 全9タブ + Tab 9 トレンド",
                   has_9tabs and has_tab9,
                   f"全9タブ={'OK' if has_9tabs else 'NG'}, "
                   f"Tab9トレンド={'OK' if has_tab9 else 'NG'}")
            screenshot(page, "e6_guide_tab")
        except Exception as e:
            record("E-6", "ガイドタブ: 全9タブ + Tab 9 トレンド", False, str(e))

        # =============================================================
        # E-7: 都道府県フィルタ + トレンドタブ
        # =============================================================
        try:
            # 東京都を選択
            pref_select = page.locator("#pref-select")
            pref_select.select_option(label="東京都")
            wait_for_htmx(page)

            # トレンドタブをクリック
            trend_btn = page.locator('nav button.tab-btn', has_text="トレンド")
            trend_btn.click()
            wait_for_htmx(page, timeout_ms=8000)

            content_text = page.locator("#content").inner_text()
            # エラーがなく、トレンド分析タイトルが表示されること
            has_title = "時系列トレンド分析" in content_text
            no_error = "処理エラー" not in content_text
            # 東京都のラベルが含まれているか
            has_tokyo = "東京都" in content_text

            record("E-7", "都道府県フィルタ(東京都) + トレンドタブ",
                   has_title and no_error,
                   f"タイトル={'OK' if has_title else 'NG'}, "
                   f"東京都ラベル={'OK' if has_tokyo else 'NG'}, "
                   f"エラーなし={'OK' if no_error else 'NG'}")
            screenshot(page, "e7_tokyo_trend")
        except Exception as e:
            record("E-7", "都道府県フィルタ(東京都) + トレンドタブ", False, str(e))

        browser.close()

    # --- サマリー ---
    print(f"\n{'=' * 50}")
    print(f"Summary: {passed}/{passed + failed} passed, {failed} failed")
    print(f"Screenshots: {SCREENSHOT_DIR}")
    print(f"{'=' * 50}")

    if failed > 0:
        print("\n失敗したテスト:")
        for tid, desc, ok, detail in results:
            if not ok:
                print(f"  {tid}: {desc} -- {detail}")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(run_tests())
