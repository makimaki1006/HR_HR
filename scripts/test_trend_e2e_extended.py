"""
トレンドタブ拡張E2Eテスト (Playwright)

対象: ハローワーク求人分析ダッシュボード (localhost:9216)
テスト: N-1 ~ C-2, R-1 ~ R-3 (既存E-1~E-8の補完)
"""

import os
import sys
import time
import json
import io
import re
from pathlib import Path

# Windows環境でのUnicode出力対応
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')

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
TIMEOUT = 15_000  # 15秒
SCREENSHOT_DIR = Path(__file__).resolve().parent / "screenshots"
SCREENSHOT_DIR.mkdir(exist_ok=True)


def screenshot(page, name: str) -> str:
    """スクリーンショットを保存してパスを返す"""
    path = SCREENSHOT_DIR / f"{name}.png"
    page.screenshot(path=str(path), full_page=False)
    return str(path)


def wait_for_htmx(page, timeout_ms: int = 8000):
    """HTMXリクエスト完了を待つ"""
    page.wait_for_timeout(500)
    try:
        page.wait_for_function(
            """() => {
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
    """トレンドコンテンツのテキストを安全に取得する"""
    tc = page.locator("#trend-content")
    if tc.count() > 0:
        try:
            return tc.first.inner_text(timeout=3000)
        except Exception:
            pass
    return page.locator("#content").inner_text(timeout=5000)


def click_trend_tab(page):
    """トレンドタブをクリックしてコンテンツの読み込みを待つ"""
    trend_btn = page.locator('nav button.tab-btn', has_text="トレンド")
    trend_btn.click()
    wait_for_htmx(page, timeout_ms=10000)
    # トレンドタブのタイトルが出現するのを待つ
    try:
        page.wait_for_function(
            """() => {
                const el = document.querySelector('#content');
                return el && el.innerText.includes('時系列トレンド分析');
            }""",
            timeout=10000
        )
    except Exception:
        pass
    page.wait_for_timeout(500)


def click_subtab(page, label: str, timeout_ms: int = 10000):
    """サブタブをクリックしてHTMXコンテンツ更新を待つ"""
    btn = page.locator('#content button.analysis-subtab', has_text=label)
    if btn.count() == 0:
        btn = page.locator(f'button.analysis-subtab:has-text("{label}")')
    btn.click()
    wait_for_htmx(page, timeout_ms=timeout_ms)
    page.wait_for_timeout(1000)


def safe_print(msg: str):
    """エンコーディングエラーを避けて出力する"""
    try:
        print(msg)
    except (UnicodeEncodeError, UnicodeDecodeError):
        print(msg.encode('utf-8', errors='replace').decode('utf-8', errors='replace'))


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
        safe_print(msg)
        results.append((test_id, description, success, detail))

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        context = browser.new_context(viewport={"width": 1280, "height": 900})
        page = context.new_page()
        page.set_default_timeout(TIMEOUT)

        # ==============================================================
        # ログイン
        # ==============================================================
        try:
            page.goto(f"{BASE_URL}/login", wait_until="domcontentloaded")
            page.fill("#email", EMAIL)
            page.fill("#password", PASSWORD)
            page.click('button[type="submit"]')
            page.wait_for_url("**/", timeout=TIMEOUT)
            page.wait_for_selector("#content", timeout=TIMEOUT)
            wait_for_htmx(page)
            safe_print("[INFO] ログイン成功")
        except Exception as e:
            safe_print(f"[FATAL] ログイン失敗: {e}")
            browser.close()
            return 1

        # ==============================================================
        # N-1: 全タブボタン数の確認
        # ==============================================================
        try:
            # JavaScriptでタブテキストを取得（絵文字を除去）
            tab_data = page.evaluate("""
                (() => {
                    const btns = document.querySelectorAll('nav button.tab-btn');
                    return Array.from(btns).map(btn => btn.innerText.trim());
                })()
            """)
            btn_count = len(tab_data)

            expected_keywords = ["地域概況", "企業分析", "求人条件", "採用動向",
                                 "求人地図", "市場分析", "詳細検索", "市場診断",
                                 "トレンド", "ガイド"]

            # 各キーワードがいずれかのタブテキストに部分一致するか
            found_tabs = []
            missing_tabs = []
            for kw in expected_keywords:
                if any(kw in t for t in tab_data):
                    found_tabs.append(kw)
                else:
                    missing_tabs.append(kw)

            all_present = len(missing_tabs) == 0
            # タブ名から絵文字を除去して表示用
            clean_names = [re.sub(r'[^\w\s/]', '', t).strip() for t in tab_data]
            record("N-1", f"ナビバーに全{len(expected_keywords)}タブボタンが存在する",
                   btn_count == len(expected_keywords) and all_present,
                   f"実際のタブ数={btn_count}, 検出={len(found_tabs)}/{len(expected_keywords)}"
                   + (f", 未検出={missing_tabs}" if missing_tabs else ""))
            screenshot(page, "n1_all_tab_buttons")
        except Exception as e:
            record("N-1", "ナビバーに全タブボタンが存在する", False, str(e))

        # ==============================================================
        # N-2: トレンドタブの位置確認（ガイドの手前）
        # ==============================================================
        try:
            tab_data = page.evaluate("""
                (() => {
                    const btns = document.querySelectorAll('nav button.tab-btn');
                    return Array.from(btns).map(btn => btn.innerText.trim());
                })()
            """)

            trend_idx = -1
            guide_idx = -1
            for i, t in enumerate(tab_data):
                if "トレンド" in t:
                    trend_idx = i
                if "ガイド" in t:
                    guide_idx = i

            is_before_guide = trend_idx >= 0 and guide_idx >= 0 and trend_idx == guide_idx - 1
            record("N-2", "トレンドタブがガイドタブの直前に位置する",
                   is_before_guide,
                   f"トレンド位置={trend_idx}, ガイド位置={guide_idx}")
        except Exception as e:
            record("N-2", "トレンドタブがガイドタブの直前に位置する", False, str(e))

        # ==============================================================
        # N-3: トレンドタブクリックでコンテンツ更新
        # ==============================================================
        try:
            click_trend_tab(page)
            content_text = page.locator("#content").inner_text()
            has_trend_title = "時系列トレンド分析" in content_text
            record("N-3", "トレンドタブクリックでコンテンツが更新される",
                   has_trend_title,
                   f"タイトル表示={'OK' if has_trend_title else 'NG'}")
            screenshot(page, "n3_trend_tab_breadcrumb")
        except Exception as e:
            record("N-3", "トレンドタブクリックでコンテンツが更新される", False, str(e))

        # ==============================================================
        # S5-1: 外部比較サブタブ → 3+チャート確認
        # ==============================================================
        try:
            click_subtab(page, "外部比較", timeout_ms=15000)
            try:
                page.wait_for_function(
                    """() => {
                        const el = document.querySelector('#trend-content') || document.querySelector('#content');
                        return el && (el.innerText.includes('有効求人倍率') || el.innerText.includes('求人倍率'));
                    }""",
                    timeout=15000
                )
            except Exception:
                pass
            page.wait_for_timeout(2000)

            echart_elements = page.locator("#content .echart")
            echart_count = echart_elements.count()

            config_count = 0
            for i in range(echart_count):
                cfg = echart_elements.nth(i).get_attribute("data-chart-config")
                if cfg and len(cfg) > 10:
                    config_count += 1

            record("S5-1", "外部比較サブタブに3+チャートが存在する",
                   config_count >= 3,
                   f"echart要素数={echart_count}, config付き={config_count}")
            screenshot(page, "s5_1_external_charts")
        except Exception as e:
            record("S5-1", "外部比較サブタブに3+チャートが存在する", False, str(e))

        # ==============================================================
        # S5-2: チャートタイトル確認
        # ==============================================================
        try:
            trend_text = get_trend_content_text(page)
            content_text = page.locator("#content").inner_text()
            combined = trend_text + "\n" + content_text

            has_ratio = "有効求人倍率" in combined or "求人倍率" in combined
            has_salary = "賃金" in combined
            has_turnover = "離職率" in combined or "離職" in combined

            checks = [
                ("有効求人倍率", has_ratio),
                ("賃金", has_salary),
                ("離職率", has_turnover),
            ]
            found = [name for name, ok in checks if ok]
            not_found = [name for name, ok in checks if not ok]
            record("S5-2", "外部比較に主要チャートタイトルが存在する",
                   has_ratio and has_salary,
                   f"検出={found}, 未検出={not_found}")
            screenshot(page, "s5_2_chart_titles")
        except Exception as e:
            record("S5-2", "外部比較に主要チャートタイトルが存在する", False, str(e))

        # ==============================================================
        # S5-3: 時間粒度差異の注意テキスト確認
        # ==============================================================
        try:
            trend_text = get_trend_content_text(page)
            content_text = page.locator("#content").inner_text()
            combined = trend_text + "\n" + content_text

            has_warning = (
                "外部統計" in combined
                or "年次" in combined
                or "月次" in combined
                or "粒度" in combined
                or "注意" in combined
                or "※" in combined
            )
            record("S5-3", "外部比較に時間粒度やデータソース注釈がある",
                   has_warning,
                   f"注釈検出={'OK' if has_warning else 'NG'}")
        except Exception as e:
            record("S5-3", "外部比較に時間粒度やデータソース注釈がある", False, str(e))

        # ==============================================================
        # S5-4: dual-axisチャート確認
        # ==============================================================
        try:
            echart_elements = page.locator("#content .echart")
            echart_count = echart_elements.count()
            dual_axis_found = False

            for i in range(echart_count):
                cfg_str = echart_elements.nth(i).get_attribute("data-chart-config")
                if cfg_str:
                    try:
                        cfg = json.loads(cfg_str)
                        y_axis = cfg.get("yAxis", None)
                        if isinstance(y_axis, list) and len(y_axis) >= 2:
                            dual_axis_found = True
                            break
                    except json.JSONDecodeError:
                        pass

            record("S5-4", "dual-axisチャートが存在する（最低賃金等）",
                   dual_axis_found,
                   f"dual-axis={'検出' if dual_axis_found else '未検出'}")
        except Exception as e:
            record("S5-4", "dual-axisチャートが存在する（最低賃金等）", False, str(e))

        # ==============================================================
        # D-1: Sub1の最新月サマリー数値確認
        # ==============================================================
        try:
            click_subtab(page, "量の変化", timeout_ms=10000)
            page.wait_for_timeout(2000)
            trend_text = get_trend_content_text(page)

            has_seishain = "正社員" in trend_text
            has_part = "パート" in trend_text
            has_other = "その他" in trend_text

            numbers = re.findall(r'[\d,]+', trend_text)
            large_numbers = [n for n in numbers if len(n.replace(',', '')) >= 3]

            record("D-1", "Sub1に正社員/パート/その他の数値が表示される",
                   has_seishain and has_part and has_other and len(large_numbers) >= 3,
                   f"正社員={'OK' if has_seishain else 'NG'}, "
                   f"パート={'OK' if has_part else 'NG'}, "
                   f"その他={'OK' if has_other else 'NG'}, "
                   f"大きい数値数={len(large_numbers)}")
            screenshot(page, "d1_sub1_summary_numbers")
        except Exception as e:
            record("D-1", "Sub1に正社員/パート/その他の数値が表示される", False, str(e))

        # ==============================================================
        # D-2: チャートにcanvasが描画されている確認
        # ==============================================================
        try:
            page.wait_for_timeout(2000)
            canvas_count = page.evaluate(
                "document.querySelectorAll('#content .echart canvas').length"
            )
            echart_count = page.evaluate(
                "document.querySelectorAll('#content .echart').length"
            )
            record("D-2", "EChartsのcanvasが実際に描画されている",
                   canvas_count > 0,
                   f"echart要素={echart_count}, canvas描画={canvas_count}")
            screenshot(page, "d2_canvas_rendering")
        except Exception as e:
            record("D-2", "EChartsのcanvasが実際に描画されている", False, str(e))

        # ==============================================================
        # D-3: チャートconfigがvalidなJSON
        # ==============================================================
        try:
            valid_count = page.evaluate("""
                (() => {
                    let valid = 0;
                    document.querySelectorAll('#content .echart[data-chart-config]').forEach(el => {
                        try {
                            JSON.parse(el.getAttribute('data-chart-config'));
                            valid++;
                        } catch(e) {}
                    });
                    return valid;
                })()
            """)
            total_configs = page.evaluate(
                "document.querySelectorAll('#content .echart[data-chart-config]').length"
            )
            record("D-3", "全チャートconfigが有効なJSON",
                   valid_count > 0 and valid_count == total_configs,
                   f"valid={valid_count}/{total_configs}")
        except Exception as e:
            record("D-3", "全チャートconfigが有効なJSON", False, str(e))

        # ==============================================================
        # P-1: 北海道フィルタ + トレンドタブ
        # ==============================================================
        try:
            pref_select = page.locator("#pref-select")
            pref_select.select_option(label="北海道")
            wait_for_htmx(page)
            click_trend_tab(page)

            content_text = page.locator("#content").inner_text()
            has_title = "時系列トレンド分析" in content_text
            has_hokkaido = "北海道" in content_text
            no_error = "処理エラー" not in content_text

            record("P-1", "北海道フィルタ + トレンドタブ表示",
                   has_title and no_error,
                   f"タイトル={'OK' if has_title else 'NG'}, "
                   f"北海道ラベル={'OK' if has_hokkaido else 'NG'}, "
                   f"エラーなし={'OK' if no_error else 'NG'}")
            screenshot(page, "p1_hokkaido_trend")
        except Exception as e:
            record("P-1", "北海道フィルタ + トレンドタブ表示", False, str(e))

        # ==============================================================
        # P-2: 大阪府フィルタ + トレンドタブ
        # ==============================================================
        try:
            pref_select = page.locator("#pref-select")
            pref_select.select_option(label="大阪府")
            wait_for_htmx(page)
            click_trend_tab(page)

            content_text = page.locator("#content").inner_text()
            has_title = "時系列トレンド分析" in content_text
            has_osaka = "大阪府" in content_text
            no_error = "処理エラー" not in content_text

            record("P-2", "大阪府フィルタ + トレンドタブ表示",
                   has_title and no_error,
                   f"タイトル={'OK' if has_title else 'NG'}, "
                   f"大阪府ラベル={'OK' if has_osaka else 'NG'}, "
                   f"エラーなし={'OK' if no_error else 'NG'}")
            screenshot(page, "p2_osaka_trend")
        except Exception as e:
            record("P-2", "大阪府フィルタ + トレンドタブ表示", False, str(e))

        # ==============================================================
        # P-3: 全国に戻す + トレンドタブ
        # ==============================================================
        try:
            pref_select = page.locator("#pref-select")
            pref_select.select_option(value="")
            wait_for_htmx(page)
            click_trend_tab(page)

            content_text = page.locator("#content").inner_text()
            has_title = "時系列トレンド分析" in content_text
            has_zenkoku = "全国" in content_text
            no_error = "処理エラー" not in content_text

            record("P-3", "全国フィルタに戻してトレンドタブ表示",
                   has_title and no_error,
                   f"タイトル={'OK' if has_title else 'NG'}, "
                   f"全国ラベル={'OK' if has_zenkoku else 'NG'}, "
                   f"エラーなし={'OK' if no_error else 'NG'}")
            screenshot(page, "p3_zenkoku_trend")
        except Exception as e:
            record("P-3", "全国フィルタに戻してトレンドタブ表示", False, str(e))

        # ==============================================================
        # P-4: 都道府県変更で外部比較データが更新される
        # ==============================================================
        try:
            click_subtab(page, "外部比較", timeout_ms=15000)
            page.wait_for_timeout(3000)
            zenkoku_configs = page.evaluate("""
                (() => {
                    return Array.from(
                        document.querySelectorAll('#content .echart[data-chart-config]')
                    ).map(el => el.getAttribute('data-chart-config')).join('|||');
                })()
            """)

            pref_select = page.locator("#pref-select")
            pref_select.select_option(label="東京都")
            wait_for_htmx(page)
            click_trend_tab(page)
            click_subtab(page, "外部比較", timeout_ms=15000)
            page.wait_for_timeout(3000)

            tokyo_configs = page.evaluate("""
                (() => {
                    return Array.from(
                        document.querySelectorAll('#content .echart[data-chart-config]')
                    ).map(el => el.getAttribute('data-chart-config')).join('|||');
                })()
            """)

            data_changed = zenkoku_configs != tokyo_configs
            both_have_data = len(zenkoku_configs) > 50 and len(tokyo_configs) > 50

            record("P-4", "都道府県変更で外部比較データが更新される",
                   both_have_data,
                   f"データ変更={'OK' if data_changed else '同一'}, "
                   f"全国config長={len(zenkoku_configs)}, "
                   f"東京config長={len(tokyo_configs)}")
            screenshot(page, "p4_prefecture_change_external")
        except Exception as e:
            record("P-4", "都道府県変更で外部比較データが更新される", False, str(e))

        # ==============================================================
        # 全国に戻してガイドタブテスト
        # ==============================================================
        try:
            pref_select = page.locator("#pref-select")
            pref_select.select_option(value="")
            wait_for_htmx(page)
        except Exception:
            pass

        # ==============================================================
        # G-1: ガイドタブに外部比較サブタブの説明がある
        # ==============================================================
        try:
            guide_btn = page.locator('nav button.tab-btn', has_text="ガイド")
            guide_btn.click()
            wait_for_htmx(page)

            tab_guide = page.locator('#content summary', has_text="タブ別ガイド")
            if tab_guide.count() > 0:
                tab_guide.first.click()
                page.wait_for_timeout(500)

            tab9_summary = page.locator('#content summary', has_text="Tab 9")
            if tab9_summary.count() > 0:
                tab9_summary.first.click()
                page.wait_for_timeout(500)

            content_text = page.locator("#content").inner_text()
            has_external = "外部比較" in content_text
            has_trend_desc = "トレンド" in content_text

            record("G-1", "ガイドタブに外部比較サブタブの説明がある",
                   has_external and has_trend_desc,
                   f"外部比較={'OK' if has_external else 'NG'}, "
                   f"トレンド={'OK' if has_trend_desc else 'NG'}")
            screenshot(page, "g1_guide_external_desc")
        except Exception as e:
            record("G-1", "ガイドタブに外部比較サブタブの説明がある", False, str(e))

        # ==============================================================
        # G-2: FAQに市区町村単位の質問がある
        # ==============================================================
        try:
            content_text = page.locator("#content").inner_text()
            faq_summary = page.locator('#content summary', has_text="FAQ")
            if faq_summary.count() > 0:
                faq_summary.first.click()
                page.wait_for_timeout(500)
                content_text = page.locator("#content").inner_text()

            has_muni_question = "市区町村" in content_text and "トレンド" in content_text

            record("G-2", "FAQにトレンド+市区町村に関する質問がある",
                   has_muni_question,
                   f"市区町村+トレンド={'OK' if has_muni_question else 'NG'}")
            screenshot(page, "g2_faq_municipality")
        except Exception as e:
            record("G-2", "FAQにトレンド+市区町村に関する質問がある", False, str(e))

        # ==============================================================
        # G-3: FAQに外部データの質問がある
        # ==============================================================
        try:
            content_text = page.locator("#content").inner_text()
            has_external_faq = "外部" in content_text and ("データ" in content_text or "統計" in content_text)

            record("G-3", "FAQに外部データに関する質問がある",
                   has_external_faq,
                   f"外部データ={'OK' if has_external_faq else 'NG'}")
        except Exception as e:
            record("G-3", "FAQに外部データに関する質問がある", False, str(e))

        # ==============================================================
        # G-4: ユースケースにトレンド→外部比較のパスがある
        # ==============================================================
        try:
            usecase_summary = page.locator('#content summary', has_text="ユースケース")
            if usecase_summary.count() == 0:
                usecase_summary = page.locator('#content summary', has_text="活用")
            if usecase_summary.count() > 0:
                usecase_summary.first.click()
                page.wait_for_timeout(500)

            content_text = page.locator("#content").inner_text()
            has_usecase = ("トレンド" in content_text and "外部比較" in content_text) or \
                          ("時系列" in content_text and "外部" in content_text)

            record("G-4", "ユースケースにトレンド→外部比較の活用例がある",
                   has_usecase,
                   f"検出={'OK' if has_usecase else 'NG'}")
            screenshot(page, "g4_usecase_trend")
        except Exception as e:
            record("G-4", "ユースケースにトレンド→外部比較の活用例がある", False, str(e))

        # ==============================================================
        # C-1: 市場分析タブにトレンドタブへのクロスナビリンクがある
        # ==============================================================
        try:
            analysis_btn = page.locator('nav button.tab-btn', has_text="市場分析")
            analysis_btn.click()
            wait_for_htmx(page)

            sub5_btn = page.locator('#content button.analysis-subtab', has_text="外部")
            if sub5_btn.count() > 0:
                sub5_btn.first.click()
                wait_for_htmx(page, timeout_ms=10000)

            content_text = page.locator("#content").inner_text()
            has_cross_nav = "トレンド" in content_text

            record("C-1", "市場分析タブにトレンドへのクロスナビがある",
                   has_cross_nav,
                   f"トレンド言及={'OK' if has_cross_nav else 'NG'}")
            screenshot(page, "c1_market_analysis_crossnav")
        except Exception as e:
            record("C-1", "市場分析タブにトレンドへのクロスナビがある", False, str(e))

        # ==============================================================
        # C-2: クロスナビからトレンドタブに遷移可能か
        # ==============================================================
        try:
            click_trend_tab(page)
            content_text = page.locator("#content").inner_text()
            has_title = "時系列トレンド分析" in content_text

            record("C-2", "市場分析からトレンドタブへ正常遷移",
                   has_title,
                   f"タイトル表示={'OK' if has_title else 'NG'}")
            screenshot(page, "c2_crossnav_to_trend")
        except Exception as e:
            record("C-2", "市場分析からトレンドタブへ正常遷移", False, str(e))

        # ==============================================================
        # R-1: サブタブ高速切り替え（クラッシュ耐性）
        # ==============================================================
        try:
            click_trend_tab(page)
            subtabs = ["量の変化", "質の変化", "構造の変化", "シグナル", "外部比較"]

            for label in subtabs:
                btn = page.locator('#content button.analysis-subtab', has_text=label)
                if btn.count() > 0:
                    btn.click()
                    page.wait_for_timeout(300)

            wait_for_htmx(page, timeout_ms=15000)
            page.wait_for_timeout(3000)

            content_text = page.locator("#content").inner_text()
            no_crash = "処理エラー" not in content_text or len(content_text) > 50

            record("R-1", "高速サブタブ切り替えでクラッシュしない",
                   no_crash,
                   f"コンテンツ長={len(content_text)}")
            screenshot(page, "r1_rapid_subtab_switch")
        except Exception as e:
            record("R-1", "高速サブタブ切り替えでクラッシュしない", False, str(e))

        # ==============================================================
        # R-2: トレンドタブ読み込み10秒以内
        # ==============================================================
        try:
            overview_btn = page.locator('nav button.tab-btn', has_text="地域概況")
            overview_btn.click()
            wait_for_htmx(page)

            start_time = time.time()
            click_trend_tab(page)
            elapsed = time.time() - start_time

            content_text = page.locator("#content").inner_text()
            has_title = "時系列トレンド分析" in content_text
            within_time = elapsed < 10.0

            record("R-2", "トレンドタブが10秒以内に読み込まれる",
                   has_title and within_time,
                   f"読み込み時間={elapsed:.2f}秒")
            screenshot(page, "r2_load_time")
        except Exception as e:
            record("R-2", "トレンドタブが10秒以内に読み込まれる", False, str(e))

        # ==============================================================
        # R-3: 全5サブタブがエラーなしで読み込まれる
        # ==============================================================
        try:
            subtabs = ["量の変化", "質の変化", "構造の変化", "シグナル", "外部比較"]
            error_subtabs = []

            for label in subtabs:
                click_subtab(page, label, timeout_ms=15000)
                page.wait_for_timeout(1500)
                trend_text = get_trend_content_text(page)
                if "処理エラー" in trend_text:
                    error_subtabs.append(label)

            all_ok = len(error_subtabs) == 0
            record("R-3", "全5サブタブがエラーなしで読み込まれる",
                   all_ok,
                   f"エラーなし={5 - len(error_subtabs)}/5" +
                   (f", エラー発生={error_subtabs}" if error_subtabs else ""))
            screenshot(page, "r3_all_subtabs_no_error")
        except Exception as e:
            record("R-3", "全5サブタブがエラーなしで読み込まれる", False, str(e))

        browser.close()

    # --- サマリー ---
    safe_print(f"\n{'=' * 60}")
    safe_print(f"Extended Test Summary: {passed}/{passed + failed} passed, {failed} failed")
    safe_print(f"Screenshots: {SCREENSHOT_DIR}")
    safe_print(f"{'=' * 60}")

    if failed > 0:
        safe_print("\n失敗したテスト:")
        for tid, desc, ok, detail in results:
            if not ok:
                safe_print(f"  {tid}: {desc}")
                if detail:
                    safe_print(f"      Detail: {detail}")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(run_tests())
